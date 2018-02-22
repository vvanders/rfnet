use packet::*;
use framed::FramedWrite;
use send_block::{SendBlock, SendResult, SendError, RetryConfig};
use recv_block::{RecvBlock, RecvResult, RecvError};
use message;

use std::io;
use hyper;
use rand;

//@todo suspend
//@todo http req

pub struct Link {
    callsign: String,
    inner_state: InnerState,
    suspended_state: InnerState,
    config: LinkConfig
}

pub struct LinkConfig {
    pub link_width: usize,
    pub fec: bool,
    pub enable_broadcast: bool
}

pub trait HttpProvider {
    fn request(&mut self, request: hyper::Request) -> Result<hyper::Response, hyper::Error>;
}

enum InnerState {
    Idle,
    Connected { remote: String, session_id: u16, idle: usize },
    Request { remote: String, request: Vec<u8>, recv: RecvBlock },
    Response { remote: String, response: io::Cursor<Vec<u8>>, send: SendBlock }
}

const NEGOTIATION_TIMEOUT: usize = 2000;

impl Link {
    pub fn new(callsign: &str, config: LinkConfig) -> Link {
        Link {
            callsign: callsign.to_string(),
            inner_state: InnerState::Idle,
            suspended_state: InnerState::Idle,
            config
        }
    }

    fn send_link_established<W>(source: &String, dest: &String, session_id: u16, fec: bool, packet_writer: &mut W) -> io::Result<()> where W: FramedWrite {
        let packet = Packet::Control(ControlPacket {
            session_id,
            ctrl_type: ControlType::LinkOpened,
            source_callsign: source.as_bytes(),
            dest_callsign: dest.as_bytes()
        });

        packet_writer.start_frame()?;
        encode(&packet, fec, packet_writer)?;
        packet_writer.end_frame()?;

        Ok(())
    }

    fn send_disconnect<W>(source: &String, dest: &String, fec: bool, packet_writer: &mut W) -> io::Result<()> where W: FramedWrite {
        let packet = Packet::Control(ControlPacket {
            session_id: 0,
            ctrl_type: ControlType::LinkClose,
            source_callsign: source.as_bytes(),
            dest_callsign: dest.as_bytes()
        });

        packet_writer.start_frame()?;
        encode(&packet, fec, packet_writer)?;
        packet_writer.end_frame()?;

        Ok(())
    }

    fn connect<W>(source: &String, callsign: &[u8], session_id: u16, fec: bool, packet_writer: &mut W) -> io::Result<Option<InnerState>> where W: FramedWrite {
        let res = match ::std::str::from_utf8(callsign) {
            Ok(s) => {
                let callsign = s.to_string();

                //Send response
                Self::send_link_established(&source, &callsign, session_id, fec, packet_writer)?;

                Some(InnerState::Connected { remote: callsign, session_id, idle: 0})
            },
            Err(e) => {
                info!("Failed to read callsign, invalid UTF8 {:?} {:?}", callsign, e);
                None
            }
        };

        Ok(res)
    }

    fn build_request(msg: message::RequestMessage) -> Result<hyper::Request, String> {
        match msg.req_type {
            message::RequestType::REST { method, url, headers, body } => {
                let method = match method {
                    message::RESTMethod::GET => hyper::Method::Get,
                    message::RESTMethod::PUT => hyper::Method::Put,
                    message::RESTMethod::POST => hyper::Method::Post,
                    message::RESTMethod::PATCH => hyper::Method::Patch,
                    message::RESTMethod::DELETE => hyper::Method::Delete
                };

                use std::str::FromStr;
                let url = hyper::Uri::from_str(url)
                    .map_err(|e| format!("Unable to parse url {:?}", e))?;

                let mut req = hyper::Request::new(method, url);

                req.headers_mut().append_raw("rfnet-signature", msg.signature);

                //@todo parse + set headers

                req.set_body(body.to_string());

                Ok(req)
            },
            _ => Err("Unsupported request".to_string())
        }
    }

    fn encode_response(session_id: u16, code: u16, body: &str) -> Option<Vec<u8>> {
        let response = message::ResponseMessage {
            sequence_id: session_id,
            resp_type: message::ResponseType::REST {
                code,
                body
            }
        };

        let mut encoded = vec!();
        match message::encode_response_message(&response, &mut encoded) {
            Ok(()) => Some(encoded),
            Err(e) => {
                info!("Failed to encode response {:?}", e);
                None
            }
        }
    }

    fn handle_response<H>(request: &Vec<u8>, session_id: u16, http: &mut H) -> Option<Vec<u8>> where H: HttpProvider {
        match message::decode_request_message(request) {
            Ok(msg) => {
                match Self::build_request(msg) {
                    Ok(request) => {
                        match http.request(request) {
                            Ok(resp) => {
                                let code = resp.status().as_u16();

                                use futures::stream::Stream;
                                use futures::Future;
                                let body = resp.body().concat2().wait();

                                match body {
                                    Ok(body) => match ::std::str::from_utf8(body.as_ref()) {
                                        Ok(body) => Self::encode_response(session_id, code, body),
                                        Err(e) => Self::encode_response(session_id, code, format!("Unable to decode UTF response {:?}", e).as_str())
                                    }
                                    Err(e) => Self::encode_response(session_id, 500, format!("Error during http request {:?}", e).as_str())
                                }
                            },
                            Err(e) => Self::encode_response(session_id, 500, format!("Unable to issue http request {:?}", e).as_str())
                        }
                    },
                    Err(e) => Self::encode_response(session_id, 500, e.as_str())
                }
            },
            Err(e) => Self::encode_response(session_id, 500, "Error when decoding message")
        }
    }

    pub fn recv_data<W,H>(&mut self, data: &mut [u8], packet_writer: &mut W, http: &mut H) -> io::Result<()> where W: FramedWrite, H: HttpProvider {
        let (packet, err) = match decode(data, self.config.fec) {
            Ok(p) => p,
            Err(e) => {
                info!("Unable to decode packet {:?}", e);
                return Ok(())
            }
        };

        let new_state = match &mut self.inner_state {
            &mut InnerState::Idle => {
                match &packet {
                    &Packet::Control(ref ctrl) => {
                        if ctrl.dest_callsign == self.callsign.as_bytes() {
                            match ctrl.ctrl_type {
                                ControlType::LinkRequest => {
                                    let session_id = rand::random::<u16>();
                                    Self::connect(&self.callsign, ctrl.source_callsign, session_id, self.config.fec, packet_writer)?
                                },
                                _ => {
                                    trace!("Unexpected control packet during Idle {:?}", ctrl);
                                    None
                                }
                            }
                        } else {
                            None
                        }
                    },
                    p => {
                        trace!("Unexpected packet during Idle {:?}", p);
                        None
                    }
                }
            },
            &mut InnerState::Connected { ref remote, session_id, idle: _idle } => {
                let start_data = if let &Packet::Data(ref header,_) = &packet {
                    header.packet_idx == session_id
                } else {
                    false
                };

                if start_data {
                    let recv = RecvBlock::new(session_id, self.config.fec);
                    Some(InnerState::Request {
                        remote: remote.clone(),
                        request: vec!(),
                        recv
                    })
                } else {
                    if let &Packet::Control(ref header) = &packet {
                        if header.source_callsign == remote.as_bytes() && header.dest_callsign == self.callsign.as_bytes() {
                            match header.ctrl_type {
                                ControlType::LinkClose => {
                                    Self::send_disconnect(&self.callsign, remote, self.config.fec, packet_writer)?;
                                    Some(InnerState::Idle)
                                },
                                ControlType::LinkRequest => {
                                    //If we missed ack, resend it
                                    Self::send_link_established(&self.callsign, remote, session_id, self.config.fec, packet_writer)?;

                                    None
                                }
                                _ => None
                            }
                        } else {
                            trace!("Control packet with wrong callsign {:?} -> {:?}", header.source_callsign, header.dest_callsign);
                            None
                        }
                    } else {
                        None
                    }
                }
            },
            &mut InnerState::Request { ref remote, ref mut request, ref mut recv } => {
                //If we didn't get the link established on first pass then resend it
                let handled = if let &Packet::Control(ref header) = &packet {
                    Self::send_link_established(&self.callsign, remote, recv.get_session_id(), self.config.fec, packet_writer)?;
                    true
                } else {
                    false
                };
                
                if !handled {
                    match recv.on_packet(&(packet, err), packet_writer, request) {
                        Err(e) => match e {
                            RecvError::Io(e) => return Err(e),
                            o => {
                                info!("Disconnecting due to {:?}", o);
                                Self::send_disconnect(&self.callsign, remote, self.config.fec, packet_writer)?;
                                Some(InnerState::Idle)
                            }
                        },
                        Ok(RecvResult::CompleteSendResponse) => {
                            if let Some(response) = Self::handle_response(request, recv.get_session_id(), http) {
                                let retry_config = RetryConfig {
                                    delay_ms: 0,
                                    bps: 1200,
                                    bps_scale: 1.0,
                                    retry_attempts: 5
                                };
                                let fec = if self.config.fec {
                                    Some(1)
                                } else {
                                    None
                                };
                                let send = SendBlock::new(response.len(), recv.get_session_id(), self.config.link_width, fec, retry_config);

                                Some(InnerState::Response { remote: remote.clone(), response: io::Cursor::new(response), send})
                            } else {
                                let session_id = rand::random::<u16>();
                                Some(InnerState::Connected { remote: remote.clone(), session_id, idle: 0})
                            }
                        },
                        _ => None
                    }
                } else {
                    None
                }
            },
            &mut InnerState::Response { ref remote, ref mut response, ref mut send } => {
                match send.on_packet(&packet, packet_writer, response) {
                    Err(e) => match e {
                        SendError::Io(e) => return Err(e),
                        o => {
                            info!("Disconnecting due to {:?}", o);
                            Self::send_disconnect(&self.callsign, remote, self.config.fec, packet_writer)?;
                            Some(InnerState::Idle)
                        }
                    },
                    Ok(SendResult::CompleteResponse) | Ok(SendResult::CompleteNoResponse) => {
                        let session_id = rand::random::<u16>();
                        Some(InnerState::Connected { remote: remote.clone(), session_id, idle: 0})
                    },
                    _ => None
                }
            }
        };

        if let Some(new_state) = new_state {
            self.inner_state = new_state
        }

        Ok(())
    }

    pub fn elapsed<W>(&mut self, ms: usize, packet_writer: &mut W) -> io::Result<()> where W: FramedWrite {
        let new_state = match &mut self.inner_state {
            &mut InnerState::Idle => None,
            &mut InnerState::Connected { ref remote, session_id, ref mut idle } => {
                *idle += ms;

                if *idle >= NEGOTIATION_TIMEOUT {
                    info!("Timeout of idle state for {}, disconnecting", remote);

                    Self::send_disconnect(&self.callsign, remote, self.config.fec, packet_writer)?;
                    Some(InnerState::Idle)
                } else {
                    None
                }
            },
            &mut InnerState::Request { ref remote, request: ref _request, ref mut recv } => {
                match recv.tick(ms, packet_writer) {
                    Ok(_) => None,
                    Err(e) => match e {
                        RecvError::Io(ie) => return Err(ie),
                        o => {
                            info!("Error on connection {:?}, disconnecting", o);

                            Self::send_disconnect(&self.callsign, remote, self.config.fec, packet_writer)?;
                            Some(InnerState::Idle)
                        }
                    }
                }
            },
            &mut InnerState::Response { ref remote, response: ref _response, ref mut send } => {
                match send.tick(ms, packet_writer) {
                    Ok(_) => None,
                    Err(e) => match e {
                        SendError::Io(ie) => return Err(ie),
                        o => {
                            info!("Error on connection {:?}, disconnecting", o);

                            Self::send_disconnect(&self.callsign, remote, self.config.fec, packet_writer)?;
                            Some(InnerState::Idle)
                        }
                    }
                }
            }
        };

        if let Some(new_state) = new_state {
            self.inner_state = new_state;
        }

        Ok(())
    }
}