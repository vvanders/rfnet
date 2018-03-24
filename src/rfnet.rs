use rfnet_core::*;
use rfnet_core::framed::{KISSFramed, FramedRead, FramedWrite};
use rfnet_web::proto;

use hyper;
use tokio_core;

use std::net::TcpStream;
use std::io;

pub enum Mode {
    Node { node: Node },
    Link { link: Link },
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
    request_reader: Vec<u8>,
    response_writer: io::Cursor<Vec<u8>>,
    event_loop: tokio_core::reactor::Core
}

impl RFNet {
    pub fn new() -> RFNet {
        RFNet {
            mode: Mode::Unconfigured,
            tnc: None,
            recv_buffer: vec!(),
            request_reader: vec!(),
            response_writer: io::Cursor::new(vec!()),
            event_loop: tokio_core::reactor::Core::new().unwrap()
        }
    }

    pub fn update_tnc(&mut self, elapsed: usize) -> io::Result<bool> {
        let frame = if let Some(ref mut tnc) = self.tnc {
            match tnc {
                &mut TNC::TCP(ref mut tnc) => tnc.read_frame(&mut self.recv_buffer)
            }
        } else {
            ::std::thread::sleep(::std::time::Duration::from_millis(100));
            Ok(None)
        };

        if let Some(ref mut tnc) = self.tnc {
            let result = if let Ok(Some(frame)) = frame {
                match self.mode {
                    Mode::Node { ref mut node } => {
                        node.on_data(frame, tnc, &mut self.request_reader, &mut self.response_writer, |_| println!("unimplemented"))?;
                        true
                    },
                    Mode::Link { ref mut link } => {
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
                            client: hyper::Client::new(&self.event_loop.handle())
                        };

                        link.recv_data(frame, tnc, &mut http)?;
                        true
                    },
                    Mode::Unconfigured => false
                }
            } else {
                false
            };

            match self.mode {
                Mode::Node { ref mut node } => node.tick(elapsed, tnc, |_| println!("unimplemented"))?,
                Mode::Link { ref mut link } => link.elapsed(elapsed, tnc)?,
                Mode::Unconfigured => {}
            }

            Ok(result)
        } else {
            Ok(false)
        }
    }

    pub fn snapshot(&self) -> proto::Interface {
        let mode = match self.mode {
            Mode::Node { ref node } => proto::Mode::Node(proto::NodeState::from(node.get_state())),
            Mode::Link { .. } => proto::Mode::Link,
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
                Mode::Node {
                    node: Node::new(config.callsign, None, retry_config)
                }
            },
            proto::ConfigureMode::Link(link_config) => {
                let link_config = LinkConfig {
                    link_width: link_config.link_width,
                    fec: link_config.fec,
                    retry_enabled: link_config.retry,
                    retry: retry_config,
                    broadcast_rate: link_config.broadcast_rate
                };

                Mode::Link { 
                    link: Link::new(config.callsign.as_str(), link_config)
                }
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

