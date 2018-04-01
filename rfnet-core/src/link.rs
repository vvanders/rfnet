use packet::*;
use framed::FramedWrite;
use send_block::{SendBlock, SendResult, SendError, RetryConfig};
use recv_block::{RecvBlock, RecvResult, RecvError};
use message;

use std::io;
use hyper;
use base64;

//@todo suspend

pub struct Link {
    callsign: String,
    inner_state: InnerState,
    config: LinkConfig
}

pub struct LinkConfig {
    pub link_width: u16,
    pub fec: bool,
    pub retry_enabled: bool,
    pub retry: RetryConfig,
    pub broadcast_rate: Option<usize>
}

pub trait HttpProvider {
    fn request(&mut self, request: hyper::Request) -> Result<hyper::Response, hyper::Error>;
}

#[derive(Debug)]
enum InnerState {
    Idle { last_broadcast: usize },
    Connected { remote: String, idle: usize },
    Request { remote: String, request: Vec<u8>, recv: RecvBlock, response: Option<Vec<u8>> },
    Response { remote: String, response: io::Cursor<Vec<u8>>, send: SendBlock }
}

const NEGOTIATION_TIMEOUT: usize = 2000;

impl Link {
    pub fn new(callsign: &str, config: LinkConfig) -> Link {
        Link {
            callsign: callsign.to_string(),
            inner_state: InnerState::Idle { last_broadcast: 0 },
            config
        }
    }

    fn send_link_established<W>(source: &String, dest: &String, fec: bool, packet_writer: &mut W) -> io::Result<()> where W: FramedWrite {
        let packet = Packet::Control(ControlPacket {
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
            ctrl_type: ControlType::LinkClear,
            source_callsign: source.as_bytes(),
            dest_callsign: dest.as_bytes()
        });

        packet_writer.start_frame()?;
        encode(&packet, fec, packet_writer)?;
        packet_writer.end_frame()?;

        Ok(())
    }

    fn connect<W>(source: &String, callsign: &[u8], fec: bool, packet_writer: &mut W) -> io::Result<Option<InnerState>> where W: FramedWrite {
        let res = match ::std::str::from_utf8(callsign) {
            Ok(s) => {
                let callsign = s.to_string();

                //Send response
                Self::send_link_established(&source, &callsign, fec, packet_writer)?;

                Some(InnerState::Connected { remote: callsign, idle: 0})
            },
            Err(e) => {
                info!("Failed to read callsign, invalid UTF8 {:?} {:?}", callsign, e);
                None
            }
        };

        Ok(res)
    }

    fn build_request(env: message::RequestEnvelope) -> Result<hyper::Request, String> {
        match env.msg.req_type {
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

                req.headers_mut().append_raw("X-rfnet-signature", base64::encode(env.signature));
                req.headers_mut().append_raw("X-rfnet-sequence_id", format!("{}", env.msg.sequence_id));

                for header in headers.lines() {
                    let mut parsed = header.splitn(2, ":");
                    use ::std::iter::Iterator;
                    if let Some(key) = parsed.next() {
                        if let Some(value) = parsed.next() {
                            req.headers_mut().append_raw(key.trim().to_string(), value.trim().to_string());
                        } else {
                            return Err(format!("Malformed header {}", header))
                        }
                    } else {
                        return Err(format!("Malformed header {}", header))
                    }
                }

                if body.len() > 0 {
                    req.set_body(body.to_string());
                }

                Ok(req)
            },
            _ => Err("Unsupported request".to_string())
        }
    }

    fn encode_response(code: u16, body: &str) -> Option<Vec<u8>> {
        let response = message::ResponseMessage {
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

    fn handle_response<H>(request: &Vec<u8>, http: &mut H) -> Option<Vec<u8>> where H: HttpProvider {
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
                                        Ok(body) => Self::encode_response(code, body),
                                        Err(e) => Self::encode_response(code, format!("Unable to decode UTF response {:?}", e).as_str())
                                    }
                                    Err(e) => Self::encode_response(500, format!("Error during http request {:?}", e).as_str())
                                }
                            },
                            Err(e) => Self::encode_response(500, format!("Unable to issue http request {:?}", e).as_str())
                        }
                    },
                    Err(e) => Self::encode_response(500, e.as_str())
                }
            },
            Err(e) => Self::encode_response(500, format!("Error when decoding message {:?}", e).as_str())
        }
    }

    fn handle_data<W,H>(
                config: &LinkConfig,
                callsign: &String,
                packet: &(Packet, usize),
                remote: &String,
                request: &mut Vec<u8>,
                response: &mut Option<Vec<u8>>,
                recv: &mut RecvBlock,
                http: &mut H,
                packet_writer: &mut W) -> io::Result<Option<InnerState>>
            where W: FramedWrite, H: HttpProvider {
        let result = match recv.on_packet(packet, packet_writer, request) {
            Err(e) => match e {
                RecvError::Io(e) => return Err(e),
                o => {
                    info!("Disconnecting due to {:?}", o);
                    Self::send_disconnect(callsign, remote, config.fec, packet_writer)?;
                    Some(InnerState::Idle { last_broadcast: 0 })
                }
            },
            Ok(RecvResult::CompleteSendResponse) => {
                info!("Request received, preparing response");
                if let Some(http_response) = Self::handle_response(request, http) {
                    //HTTP always has a response
                    match recv.send_response(true, packet_writer) {
                        Err(RecvError::Io(e)) => return Err(e),
                        Err(e) => {
                            info!("Error sending response {:?}, resetting", e);
                            Some(InnerState::Connected { remote: remote.clone(), idle: 0})
                        },
                        Ok(_) => {
                            *response = Some(http_response);
                            None
                        }
                    }
                } else {
                    Some(InnerState::Connected { remote: remote.clone(), idle: 0})
                }
            },
            Ok(RecvResult::Complete) => {
                if let &mut Some(ref response) = response {
                    info!("Ack'd response, switching to sending");
                    let fec = if config.fec {
                        Some(1)
                    } else {
                        None
                    };

                    let mut send = SendBlock::new(response.len(), config.link_width, fec, config.retry_enabled, config.retry.clone());
                    let mut payload = io::Cursor::new(response.clone());

                    send.send(packet_writer, &mut payload)?;

                    Some(InnerState::Response { remote: remote.clone(), response: payload, send})
                } else {
                    info!("Completed without response");
                    None
                }
            }
            _ => None
        };

        Ok(result)
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
            &mut InnerState::Idle { last_broadcast: _ } => {
                match &packet {
                    &Packet::Control(ref ctrl) => {
                        if ctrl.dest_callsign == self.callsign.as_bytes() {
                            match ctrl.ctrl_type {
                                ControlType::LinkRequest => {
                                    Self::connect(&self.callsign, ctrl.source_callsign, self.config.fec, packet_writer)?
                                },
                                _ => {
                                    debug!("Unexpected control packet during Idle {:?}", ctrl);
                                    None
                                }
                            }
                        } else {
                            None
                        }
                    },
                    p => {
                        debug!("Unexpected packet during Idle {:?}", p);
                        None
                    }
                }
            },
            &mut InnerState::Connected { ref remote, idle: _idle } => {
                let start_data = if let &Packet::Data(ref header,_) = &packet {
                    header.packet_idx == 0
                } else {
                    false
                };

                if start_data {
                    let mut recv = RecvBlock::new(self.config.fec);
                    let mut request = vec!();

                    if let Some(new_state) = Self::handle_data(&self.config, &self.callsign, &(packet, err), remote, &mut request, &mut None, &mut recv, http, packet_writer)? {
                        Some(new_state)
                    } else {
                        Some(InnerState::Request {
                            remote: remote.clone(),
                            request,
                            recv,
                            response: None
                        })
                    }
                } else {
                    if let &Packet::Control(ref header) = &packet {
                        if header.source_callsign == remote.as_bytes() && header.dest_callsign == self.callsign.as_bytes() {
                            match header.ctrl_type {
                                ControlType::LinkClose => {
                                    Self::send_disconnect(&self.callsign, remote, self.config.fec, packet_writer)?;
                                    Some(InnerState::Idle { last_broadcast: 0 })
                                },
                                ControlType::LinkRequest => {
                                    //If we missed ack, resend it
                                    Self::send_link_established(&self.callsign, remote, self.config.fec, packet_writer)?;

                                    None
                                }
                                _ => None
                            }
                        } else {
                            debug!("Control packet with wrong callsign {:?} -> {:?}", header.source_callsign, header.dest_callsign);
                            None
                        }
                    } else {
                        None
                    }
                }
            },
            &mut InnerState::Request { ref remote, ref mut request, ref mut recv, ref mut response } => {
                //If we didn't get the link established on first pass then resend it
                let handled = if let &Packet::Control(ref header) = &packet {
                    if header.source_callsign == remote.as_bytes() && header.dest_callsign == self.callsign.as_bytes() {
                        match header.ctrl_type {
                            ControlType::LinkRequest => {
                                Self::send_link_established(&self.callsign, remote, self.config.fec, packet_writer)?;
                                true
                            },
                            _ => false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };
                
                if !handled {
                    Self::handle_data(&self.config, &self.callsign, &(packet, err), remote, request, response, recv, http, packet_writer)?
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
                            Some(InnerState::Idle { last_broadcast: 0 })
                        }
                    },
                    Ok(SendResult::CompleteResponse) | Ok(SendResult::CompleteNoResponse) => {
                        Some(InnerState::Connected { remote: remote.clone(), idle: 0})
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
            &mut InnerState::Idle { ref mut last_broadcast } => {
                *last_broadcast += ms;

                if let Some(timeout) = self.config.broadcast_rate {
                    if *last_broadcast / 1_000 >= timeout {
                        *last_broadcast = 0;

                        let packet = Packet::Broadcast(BroadcastPacket {
                            fec_enabled: self.config.fec,
                            retry_enabled: self.config.retry_enabled,
                            major_ver: ::MAJOR_VER,
                            minor_ver: ::MINOR_VER,
                            link_width: self.config.link_width,
                            callsign: self.callsign.as_bytes()
                        });

                        packet_writer.start_frame()?;
                        encode(&packet, self.config.fec, packet_writer)?;
                        packet_writer.end_frame()?;

                        debug!("Sending broadcast packet");
                    }
                }

                None
            },
            &mut InnerState::Connected { ref remote, ref mut idle } => {
                *idle += ms;

                if *idle >= NEGOTIATION_TIMEOUT {
                    info!("Timeout of idle state for {}, disconnecting", remote);

                    Self::send_disconnect(&self.callsign, remote, self.config.fec, packet_writer)?;
                    Some(InnerState::Idle { last_broadcast: 0 })
                } else {
                    None
                }
            },
            &mut InnerState::Request { ref remote, request: ref _req, ref mut recv, response: ref _resp } => {
                match recv.tick(ms, packet_writer) {
                    Ok(_) => None,
                    Err(e) => match e {
                        RecvError::Io(ie) => return Err(ie),
                        o => {
                            info!("Error on connection {:?}, disconnecting", o);

                            Self::send_disconnect(&self.callsign, remote, self.config.fec, packet_writer)?;
                            Some(InnerState::Idle { last_broadcast: 0 })
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
                            Some(InnerState::Idle { last_broadcast: 0 })
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

#[cfg(test)]
mod test {
    use super::*;
    use framed::{KISSFramed, LoopbackIo, FramedRead};

    fn write_packet<H,P>(link: &mut Link, packet: &Packet, http: &mut H, on_packet: P)
            where H: HttpProvider, P: Fn(Option<Packet>) {
        let mut output = vec!();
        let mut input = vec!();

        encode(packet, true, &mut input).unwrap();
        link.recv_data(&mut input[..], &mut output, http).unwrap();

        if output.len() > 0 {
            let decoded = decode(&mut output[..], true).unwrap();
            on_packet(Some(decoded.0));
        } else {
            on_packet(None);
        }
    }

    fn tick<P>(link: &mut Link, ms: usize, on_packet: P) where P: Fn(Option<Packet>) {
        let mut output = vec!();
        link.elapsed(ms, &mut output).unwrap();

        if output.len() > 0 {
            let decoded = decode(&mut output[..], true).unwrap();
            on_packet(Some(decoded.0));
        } else {
            on_packet(None)
        }
    }

    struct MockHttp {
    }

    impl HttpProvider for MockHttp {
        fn request(&mut self, _request: hyper::Request) -> Result<hyper::Response, hyper::Error> {
            Err(hyper::Error::Status)
        }
    }

    fn mock_http() -> MockHttp {
        MockHttp {}
    }

    #[test]
    fn test_connect() {
        let config = LinkConfig {
            link_width: 32,
            fec: true,
            retry_enabled: true,
            retry: RetryConfig::default(1200),
            broadcast_rate: None
        };
        let mut link = Link::new("KI7EST", config);

        let connect = Packet::Control(ControlPacket {
            ctrl_type: ControlType::LinkRequest,
            source_callsign: "KI7EST-1".as_bytes(),
            dest_callsign: "KI7EST".as_bytes()
        });
        fn verify_connect(p: Option<Packet>) {
            match p {
                Some(Packet::Control(ctrl)) => {
                    assert_eq!(ctrl.ctrl_type, ControlType::LinkOpened);
                    assert_eq!(ctrl.source_callsign, "KI7EST".as_bytes());
                    assert_eq!(ctrl.dest_callsign, "KI7EST-1".as_bytes());
                },
                o => panic!("{:?}", o)
            }
        }

        write_packet(&mut link, &connect, &mut mock_http(), verify_connect);

        //Verify if we missed the first one reset connects
        write_packet(&mut link, &connect, &mut mock_http(), verify_connect);
    }

    #[test]
    fn test_broadcast() {
        let config = LinkConfig {
            link_width: 32,
            fec: true,
            retry_enabled: true,
            retry: RetryConfig::default(1200),
            broadcast_rate: Some(10)
        };
        let mut link = Link::new("KI7EST", config);

        let mut output = vec!();
        link.elapsed(10_000, &mut output).unwrap();

        match decode(&mut output[..], true) {
            Ok((Packet::Broadcast(broadcast),_)) => {
                assert_eq!(broadcast.fec_enabled, true);
                assert_eq!(broadcast.retry_enabled, true);
                assert_eq!(broadcast.major_ver, ::MAJOR_VER);
                assert_eq!(broadcast.minor_ver, ::MINOR_VER);
                assert_eq!(broadcast.link_width, 32);
                assert_eq!(broadcast.callsign, "KI7EST".as_bytes());
            },
            o => panic!("{:?}", o)
        }
    }

    #[test]
    fn test_disconnect() {
        let config = LinkConfig {
            link_width: 32,
            fec: true,
            retry_enabled: true,
            retry: RetryConfig::default(1200),
            broadcast_rate: None
        };
        let mut link = Link::new("KI7EST", config);

        let connect = Packet::Control(ControlPacket {
            ctrl_type: ControlType::LinkRequest,
            source_callsign: "KI7EST-1".as_bytes(),
            dest_callsign: "KI7EST".as_bytes()
        });

        write_packet(&mut link, &connect, &mut mock_http(), |_| {});
        tick(&mut link, NEGOTIATION_TIMEOUT, |p| {
            match p {
                Some(Packet::Control(ctrl)) => {
                    match ctrl.ctrl_type {
                        ControlType::LinkClear => {
                            assert_eq!(ctrl.source_callsign, "KI7EST".as_bytes());
                            assert_eq!(ctrl.dest_callsign, "KI7EST-1".as_bytes());
                        },
                        o => panic!("{:?}", o)
                    }
                },
                o => panic!("{:?}", o)
            }
        });
    }

    #[test]
    fn test_http() {
        let config = LinkConfig {
            link_width: 32,
            fec: true,
            retry_enabled: true,
            retry: RetryConfig::default(1200),
            broadcast_rate: None
        };
        let mut link = Link::new("KI7EST", config);

        let connect = Packet::Control(ControlPacket {
            ctrl_type: ControlType::LinkRequest,
            source_callsign: "KI7EST-1".as_bytes(),
            dest_callsign: "KI7EST".as_bytes()
        });

        write_packet(&mut link, &connect, &mut mock_http(), |_| {});

        let mut payload = vec!();
        message::encode_request_message(&message::RequestMessage {
                sequence_id: 1000,
                addr: "KI7EST@rfnet.net",
                req_type: message::RequestType::REST {
                    method: message:: RESTMethod::GET,
                    url: "http://rfnet.net/test",
                    headers: "header1: foo\r\nheader2: bar",
                    body: "Body"
                }
            }, &[0; 64], &mut vec!(), &mut payload).unwrap();

        let mut sender = SendBlock::new(payload.len(), 32, Some(1), true, RetryConfig::default(1200));

        let mut send = KISSFramed::new(LoopbackIo::new(), 0);
        let mut recv = KISSFramed::new(LoopbackIo::new(), 0);
        let mut payload_reader = io::Cursor::new(&payload[..]);

        sender.send(&mut send, &mut payload_reader).unwrap();

        struct HttpResponse {
        }

        impl HttpProvider for HttpResponse {
            fn request(&mut self, request: hyper::Request) -> Result<hyper::Response, hyper::Error> {
                use hyper::header;

                assert_eq!(request.headers().get_raw("header1"), Some(&header::Raw::from("foo")));
                assert_eq!(request.headers().get_raw("header2"), Some(&header::Raw::from("bar")));
                assert_eq!(request.headers().get_raw("X-rfnet-sequence_id"), Some(&header::Raw::from("1000")));
                assert!(request.headers().get_raw("X-rfnet-signature").is_some());

                Ok(hyper::Response::new()
                    .with_status(hyper::StatusCode::Ok)
                    .with_body("Test"))
            }
        }

        let mut http = HttpResponse {};

        let mut iters = 0;
        let mut response = false;
        let mut recv_frame = vec!();
        while iters < 200 && !response {
            iters += 1;

            while let Ok(Some(framed)) = send.read_frame(&mut recv_frame) {
                link.recv_data(framed, &mut recv, &mut http).unwrap();
            }

            while let Ok(Some(framed)) = recv.read_frame(&mut recv_frame) {
                let (packet,_err) = decode(framed, true).unwrap();
                match sender.on_packet(&packet, &mut send, &mut payload_reader).unwrap() {
                    SendResult::CompleteResponse => response = true,
                    SendResult::CompleteNoResponse => panic!(),
                    _ => {}
                }
            }
        }

        assert!(response);

        let mut receiver = RecvBlock::new(true);
        let mut response = vec!();

        iters = 0;
        let mut received = false;
        while iters < 100 && !received {
            iters += 1;

            while let Ok(Some(framed)) = send.read_frame(&mut recv_frame) {
                link.recv_data(framed, &mut recv, &mut http).unwrap();
            }

            while let Ok(Some(framed)) = recv.read_frame(&mut recv_frame) {
                let packet = decode(framed, true).unwrap();
                match receiver.on_packet(&packet, &mut send, &mut response).unwrap() {
                    RecvResult::CompleteSendResponse => {
                        receiver.send_response(false, &mut send).unwrap();
                        received = true;
                    }
                    _ => {}
                }
            }
        }

        assert!(received);

        let http_response = message::decode_response_message(&response[..]).unwrap();

        match http_response.resp_type {
            message::ResponseType::REST { code, body } => {
                assert_eq!(code, 200);
                assert_eq!(body, "Test");
            },
            o => panic!("{:?}", o)
        }
    }
}