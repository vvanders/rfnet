use rfnet_core::*;
use rfnet_core::framed::{KISSFramed, FramedRead, FramedWrite};
use rfnet_core::node::{ClientEvent, ClientState};
use rfnet_core::message;
use rfnet_web::proto;

use hyper;
use tokio_core;

use std::net::TcpStream;
use std::io;

struct NodeState {
    node: Node,
    request_reader: io::Cursor<Vec<u8>>,
    response_writer: Vec<u8>,
    requests: Vec<proto::Request>,
    active_request: Option<proto::Request>,

    private_key: [u8; 64],
    message_encode_scratch: Vec<u8>
}

struct LinkState {
    link: Link,
    event_loop: tokio_core::reactor::Core
}

enum Mode {
    Node(NodeState),
    Link(LinkState),
    Unconfigured
}

enum TNC {
    TCP(KISSFramed<TcpStream>)
}

impl io::Write for TNC {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            &mut TNC::TCP(ref mut tnc) => tnc.write(buf)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            &mut TNC::TCP(ref mut tnc) => tnc.flush()
        }
    }
}

impl FramedWrite for TNC {
    fn start_frame(&mut self) -> io::Result<()> {
        match self {
            &mut TNC::TCP(ref mut tnc) => tnc.start_frame()
        }
    }

    fn end_frame(&mut self) -> io::Result<()> {
        match self {
            &mut TNC::TCP(ref mut tnc) => tnc.end_frame()
        }
    }
}

pub struct RFNet {
    mode: Mode,
    tnc: Option<TNC>,
    recv_buffer: Vec<u8>,
}

impl RFNet {
    pub fn new() -> RFNet {
        RFNet {
            mode: Mode::Unconfigured,
            tnc: None,
            recv_buffer: vec!(),
        }
    }

    pub fn request(&mut self, request: proto::Request) {
        match self.mode {
            Mode::Node(ref mut state) => state.requests.push(request),
            _ => error!("Attempting to queue request on non-node")
        }
    }

    fn send_request(state: &mut NodeState) -> io::Result<()> {
        state.active_request = state.requests.pop();
        state.response_writer = vec!();

        if let Some(ref request) = state.active_request {
            let mut buffer = vec!();
            let msg = message::RequestMessage {
                sequence_id: 0, //@todo
                addr: request.addr.as_str(),
                req_type: message::RequestType::REST {
                    url: request.url.as_str(),
                    headers: "",
                    body: request.content.as_str(),
                    method: request.method.clone().into()
                }
            };

            message::encode_request_message(&msg, &state.private_key[..], &mut state.message_encode_scratch, &mut buffer)?;

            state.request_reader = io::Cursor::new(buffer);
        } else {
            state.request_reader = io::Cursor::new(vec!());
        }

        Ok(())
    }

    pub fn update_tnc<'a>(&'a mut self, elapsed: usize) -> io::Result<Option<proto::Response>> {
        if let Some(ref mut tnc) = self.tnc {
            let frame = match tnc {
                &mut TNC::TCP(ref mut tnc) => tnc.read_frame(&mut self.recv_buffer)?
            };

            match self.mode {
                Mode::Node(ref mut state) => {
                    let mut events = vec!();
                    {
                        let mut event_handler = |e| events.push(e);

                        if let Some(frame) = frame {
                            state.node.on_data(frame, tnc, &mut state.response_writer, &mut state.request_reader, &mut event_handler)?;
                        }

                        let send_request = match state.node.get_state() {
                            ClientState::Sending | ClientState::Receiving => false,
                            ClientState::Established => true,
                            ClientState::Listening | ClientState::Idle | ClientState::Negotiating => false,
                        };

                        if let Some(_) = state.active_request {
                            if send_request {
                                let size = state.request_reader.get_ref().len();
                                state.node.start_request(&mut state.request_reader, size, tnc, &mut event_handler)?;
                            } else {
                                if let ClientState::Idle = state.node.get_state() {
                                    state.node.connect(tnc, &mut event_handler)?;
                                }
                            }
                        }

                        state.node.tick(elapsed, tnc, &mut event_handler)?;
                    }

                    let mut result = None;

                    for event in events {
                        match event {
                            ClientEvent::StateChange(_,_) => {},
                            ClientEvent::Connected => Self::send_request(state)?,
                            ClientEvent::ConnectionFailed | ClientEvent::Disconnected => {
                                error!("Failed to connect/disconnected");

                                state.requests.clear();
                                state.active_request = None;
                            },
                            ClientEvent::ResponseComplete => {
                                if let Some(ref active_request) = state.active_request {
                                    let (code, content) = match message::decode_response_message(&state.response_writer[..]) {
                                        Ok(m) => match m.resp_type {
                                            message::ResponseType::REST { code, body } => {
                                                (code, body.to_string())
                                            },
                                            _ => {
                                                (400, "Invalid raw/reserved message response".to_string())
                                            }
                                        },
                                        Err(e) => (400, format!("Failed to decode message {:?}", e))
                                    };

                                    result = Some(proto::Response {
                                        id: active_request.id,
                                        code,
                                        content
                                    });
                                }

                                Self::send_request(state)?;
                            },
                            ClientEvent::RecvProgress(_) => {},
                            ClientEvent::SendProgress(_,_) => {}
                        }
                    }

                    Ok(result)
                },
                Mode::Link(ref mut state) => {
                    if let Some(frame) = frame {
                        struct Http {
                            client: hyper::Client<hyper::client::HttpConnector, hyper::Body>
                        }

                        impl HttpProvider for Http {
                            fn request(&mut self, request: hyper::Request) -> Result<hyper::Response, hyper::Error> {
                                use futures::Future;
                                self.client.request(request).wait()
                            }
                        }

                        let mut http = Http {
                            client: hyper::Client::new(&state.event_loop.handle())
                        };

                        state.link.recv_data(frame, tnc, &mut http)?;
                    }

                    state.link.elapsed(elapsed, tnc)?;

                    Ok(None)
                },
                Mode::Unconfigured => Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    pub fn snapshot(&self) -> proto::Interface {
        let mode = match self.mode {
            Mode::Node(ref state) => proto::Mode::Node(proto::NodeState::from(state.node.get_state())),
            Mode::Link(_) => proto::Mode::Link,
            Mode::Unconfigured => proto::Mode::Unconfigured
        };

        let tnc = match self.tnc {
            None => "Disconnected".to_string(),
            Some(TNC::TCP(ref tnc)) => format!("TCP {:?}", tnc.get_tnc().peer_addr())
        };

        proto::Interface {
            mode,
            tnc
        }
    }

    pub fn configure(&mut self, config: proto::Configuration) {
        let retry_config = RetryConfig {
            bps: config.retry_config.bps,
            bps_scale: config.retry_config.bps_scale,
            delay_ms: config.retry_config.delay_ms,
            retry_attempts: config.retry_config.retry_attempts
        };

        self.mode = match config.mode {
            proto::ConfigureMode::Node => {
                let state = NodeState {
                    node: Node::new(config.callsign, None, retry_config),
                    request_reader: io::Cursor::new(vec!()),
                    response_writer: vec!(),
                    requests: vec!(),
                    active_request: None,
                    private_key: [0; 64],
                    message_encode_scratch: vec!()
                };

                Mode::Node(state)
            },
            proto::ConfigureMode::Link(link_config) => {
                let link_config = LinkConfig {
                    link_width: link_config.link_width,
                    fec: link_config.fec,
                    retry_enabled: link_config.retry,
                    retry: retry_config,
                    broadcast_rate: link_config.broadcast_rate
                };

                let state = LinkState {
                    link: Link::new(config.callsign.as_str(), link_config),
                    event_loop: tokio_core::reactor::Core::new().unwrap()
                };

                Mode::Link(state)
            }
        }
    }

    pub fn connect_tcp_tnc(&mut self, address: &str) {
        let stream = match TcpStream::connect(address) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to connect TNC: {:?}", e);
                return
            }
        };

        use std::time::Duration;
        stream.set_read_timeout(Some(Duration::from_millis(100))).unwrap();

        self.tnc = Some(TNC::TCP(KISSFramed::new(stream, 0)));
    }
}

