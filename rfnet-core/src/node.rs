use packet::*;
use framed::FramedWrite;
use send_block::{SendBlock, RetryConfig, SendError, SendResult};
use recv_block::{RecvBlock, RecvResult, RecvError};

use std::io;

pub struct Node {
    state: State,
    inner: Inner
}

struct Inner {
    callsign: String,
    config: Option<LinkConfig>,
    retry_config: RetryConfig,
}

enum State {
    Listening { idle: usize },
    Idle,
    Negotiating(NegotiatingState),
    Established { idle: usize },
    SendingRequest { send: SendBlock },
    ReceivingResponse { recv: RecvBlock }
}

struct NegotiatingState {
    retry_count: usize,
    last_attempt: usize
}

enum Event<'a,R,W> where R: 'a + io::Read, W: 'a + io::Write {
    Connect,
    StartRequest { data_size: usize, request_reader: &'a mut R },
    Data { packet: &'a (Packet<'a>, usize), request_reader: &'a mut R, response_writer: &'a mut W },
    OtherData,
    Tick { ms: usize }
}

impl<'a,R,W> ::std::fmt::Debug for Event<'a,R,W> where R: 'a + io::Read, W: 'a + io::Write {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> Result<(), ::std::fmt::Error> {
        match self {
            &Event::Connect => write!(f, "Connect"),
            &Event::StartRequest { .. } => write!(f, "StartRequest"),
            &Event::OtherData => write!(f, "OtherData"),
            &Event::Data { .. } => write!(f, "Data"),
            &Event::Tick { .. } => write!(f, "Tick")
        }
    }
}

pub struct LinkConfig {
    pub fec_enabled: bool,
    pub retry_enabled: bool,
    pub major_ver: u8,
    pub minor_ver: u8,
    pub link_width: u16,
    pub callsign: String
}

#[derive(Debug, PartialEq)]
pub enum ClientState {
    Listening,
    Idle,
    Negotiating,
    Established,
    Sending,
    Receiving
}

impl ClientState {
    fn translate(state: &State) -> ClientState {
        match state {
            &State::Listening { .. } => ClientState::Listening,
            &State::Idle => ClientState::Idle,
            &State::Negotiating { .. } => ClientState::Negotiating,
            &State::Established { .. } => ClientState::Established,
            &State::SendingRequest { .. } => ClientState::Sending,
            &State::ReceivingResponse { .. } => ClientState::Receiving
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ClientEvent {
    Connected,
    ConnectionFailed,
    Disconnected,
    StateChange(ClientState,ClientState),
    SendProgress(usize, usize),
    RecvProgress(usize),
    ResponseComplete
}

const LISTEN_TIMEOUT: usize = 10_000;
const IDLE_TIMEOUT: usize = 2_000;

fn link_config_from_broadcast(broadcast: &BroadcastPacket) -> LinkConfig {
    LinkConfig {
        fec_enabled: broadcast.fec_enabled,
        retry_enabled: broadcast.retry_enabled,
        major_ver: broadcast.major_ver,
        minor_ver: broadcast.minor_ver,
        link_width: broadcast.link_width,
        callsign: String::from_utf8_lossy(broadcast.callsign).to_string()
    }
}

impl Node {
    pub fn new(callsign: String, config: Option<LinkConfig>, retry_config: RetryConfig) -> Node {
        Node {
            state: State::Listening { idle: 0 },
            inner: Inner {
                callsign,
                config,
                retry_config
            }
        }
    }

    pub fn get_state(&self) -> ClientState {
        ClientState::translate(&self.state)
    }


    fn handle_event<P,R,W,E>(&mut self, event: Event<R,W>, packet_writer: &mut P, mut event_handler: E) -> io::Result<()>
            where P: FramedWrite, R: io::Read, W: io::Write, E: FnMut(ClientEvent) {
        let new_state = match &mut self.state {
            &mut State::Listening { ref mut idle } => self.inner.handle_listening(idle, event, packet_writer, &mut event_handler)?,
            &mut State::Idle => self.inner.handle_idle(event, packet_writer, &mut event_handler)?,
            &mut State::Negotiating(ref mut state) => self.inner.handle_negotiating(state, event, packet_writer, &mut event_handler)?,
            &mut State::Established { ref mut idle } => self.inner.handle_established(idle, event, packet_writer, &mut event_handler)?,
            &mut State::SendingRequest { ref mut send } => self.inner.handle_send(send, event, packet_writer, &mut event_handler)?,
            &mut State::ReceivingResponse { ref mut recv } => self.inner.handle_recv(recv, event, packet_writer, &mut event_handler)?
        };

        if let Some(new_state) = new_state {
            info!("{:?} -> {:?}", ClientState::translate(&self.state), ClientState::translate(&new_state));
            event_handler(ClientEvent::StateChange(ClientState::translate(&self.state), ClientState::translate(&new_state)));

            self.state = new_state;
        }

        Ok(())
    }

    pub fn start_request<R,W,E>(&mut self, request_reader: &mut R, data_size: usize, packet_writer: &mut W, event_handler: E) -> io::Result<()>
            where R: io::Read, W: FramedWrite, E: FnMut(ClientEvent) {
        self.handle_event::<_,_,Vec<u8>,_>(Event::StartRequest { request_reader, data_size }, packet_writer, event_handler)
    }

    pub fn connect<W,E>(&mut self, packet_writer: &mut W, event_handler: E) -> io::Result<()> where W: FramedWrite, E: FnMut(ClientEvent) {
        self.handle_event::<_,io::Cursor<&[u8]>,Vec<u8>,_>(Event::Connect, packet_writer, event_handler)
    }

    pub fn on_data<P,W,R,E>(&mut self, packet: &(Packet, usize), packet_writer: &mut P, response_writer: &mut W, request_reader: &mut R, event_handler: E) -> io::Result<()> 
            where P: FramedWrite, W: io::Write, R: io::Read, E: FnMut(ClientEvent) {
        self.handle_event(Event::Data { packet, request_reader, response_writer }, packet_writer, event_handler)
    }

    pub fn on_other_data<P,E>(&mut self, packet_writer: &mut P, event_handler: E) -> io::Result<()> where P: FramedWrite, E: FnMut(ClientEvent) {
        self.handle_event::<_,io::Cursor<&[u8]>,Vec<u8>,_>(Event::OtherData, packet_writer, event_handler)
    }

    pub fn tick<W,E>(&mut self, ms: usize, packet_writer: &mut W, handle_event: E) -> io::Result<()> where W: FramedWrite, E: FnMut(ClientEvent) {
        self.handle_event::<_,io::Cursor<&[u8]>,Vec<u8>,_>(Event::Tick { ms }, packet_writer, handle_event)
    }
}

impl Inner {
    fn send_negotiation_request<W>(&self, packet_writer: &mut W) -> io::Result<Option<State>> where W: FramedWrite {
        if let Some(ref config) = self.config {
            let packet = Packet::Control(ControlPacket {
                source_callsign: self.callsign.as_bytes(),
                dest_callsign: config.callsign.as_bytes(),
                ctrl_type: ControlType::LinkRequest
            });

            packet_writer.start_frame()?;
            encode(&packet, config.fec_enabled, packet_writer)?;
            packet_writer.end_frame()?;

            Ok(None)
        } else {
            error!("Tried to connect without endpoint specified, returning to Listening");
            Ok(Some(State::Listening { idle: 0 }))
        }
    }

    fn handle_listening<P,R,W,E>(&mut self, idle: &mut usize, event: Event<R,W>, packet_writer: &mut P, event_handler: &mut E) -> io::Result<Option<State>>
            where P: FramedWrite, R: io::Read, W: io::Write, E: FnMut(ClientEvent) {
        let res = match event {
            Event::Tick { ms } => {
                *idle += ms;

                if *idle >= LISTEN_TIMEOUT && self.config.is_some() {
                    info!("Nothing heard on channel, channel is idle");
                    Some(State::Idle)
                } else {
                    None
                }
            },
            Event::Data { packet, .. } => {
                let new_state = match &packet.0 {
                    &Packet::Broadcast(ref broadcast) => {
                        self.config = Some(link_config_from_broadcast(broadcast));
                        info!("Heard broadcast packet from {}, channel is idle", String::from_utf8_lossy(broadcast.callsign));
                        Some(State::Idle)
                    },
                    &Packet::Control(ref ctrl) => match ctrl.ctrl_type {
                        ControlType::LinkClear if self.config.is_some() => Some(State::Idle),
                        _ => None
                    },
                    _ => None
                };

                if let None = new_state {
                    *idle = 0;
                }

                new_state
            },
            e => return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Unsupported event {:?} for Listening state", e)))
        };

        Ok(res)
    }

    fn handle_idle<P,R,W,E>(&mut self, event: Event<R,W>, packet_writer: &mut P, event_handler: &mut E) -> io::Result<Option<State>>
            where P: FramedWrite, R: io::Read, W: io::Write, E: FnMut(ClientEvent) {
        let res = match event {
            Event::Data { packet, .. } => {
                match &packet.0 {
                    &Packet::Broadcast(ref broadcast) => {
                        self.config = Some(link_config_from_broadcast(broadcast));
                        None
                    },
                    _ => {
                        info!("Heard non-broadcast packet, returning to listening");
                        Some(State::Listening { idle: 0 })
                    }
                }
            },
            Event::OtherData => {
                info!("Heard non-rfnet packet, returning to listening");
                Some(State::Listening { idle: 0 })
            },
            Event::Connect => {
                match self.send_negotiation_request(packet_writer)? {
                    None => Some(State::Negotiating(NegotiatingState { last_attempt: 0, retry_count: 0 })),
                    o => o
                }
            }
            e => return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Unsupported event {:?} for Idle state", e)))
        };

        Ok(res)
    }

    fn handle_negotiating<P,R,W,E>(&mut self, state: &mut NegotiatingState, event: Event<R,W>, packet_writer: &mut P, event_handler: &mut E) -> io::Result<Option<State>>
            where P: FramedWrite, R: io::Read, W: io::Write, E: FnMut(ClientEvent) {
        let res = match event {
            Event::Tick { ms } => {
                if let Some(ref config) = self.config {
                    state.last_attempt += ms;

                    let ctrl_bytes = calc_ctrl_bytes(self.callsign.as_str(), config.callsign.as_str(), config.fec_enabled);
                    let next_resend = self.retry_config.calc_delay(ctrl_bytes, ctrl_bytes);
                    if state.last_attempt >= next_resend {
                        if state.retry_count >= self.retry_config.retry_attempts {
                            info!("Failed to connect, resetting to listening");
                            event_handler(ClientEvent::ConnectionFailed);

                            Some(State::Listening { idle: 0 })
                        } else {
                            info!("Failed to hear negotiation response in {}ms, resending", next_resend);
                            state.last_attempt = 0;
                            state.retry_count += 1;

                            self.send_negotiation_request(packet_writer)?
                        }
                    } else {
                        None
                    }
                } else {
                    error!("Attempting to negotiate with empty config, resetting to listening");
                    Some(State::Listening { idle: 0 })
                }
            },
            Event::Data { packet, .. } => {
                if let &(Packet::Control(ref ctrl),_) = packet {
                    if let Some(ref config) = self.config {
                        let source = String::from_utf8_lossy(ctrl.source_callsign);
                        let dest = String::from_utf8_lossy(ctrl.dest_callsign);

                        if dest != self.callsign.as_str() || source != config.callsign {
                            info!("Discarded link request from {}", source);
                        }

                        match ctrl.ctrl_type {
                            ControlType::LinkOpened => {
                                info!("Link established with {}", source);
                                event_handler(ClientEvent::Connected);
                                Some(State::Established { idle: 0 })
                            },
                            _ => None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            e => return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Unsupported event {:?} for Negotiating state", e)))
        };

        Ok(res)
    }

    fn handle_send_data<E>(&self, send_res: Result<SendResult, SendError>, event_handler: &mut E) -> io::Result<Option<State>>
            where E: FnMut(ClientEvent) {
        match send_res {
            Ok(SendResult::CompleteResponse) => {
                let fec = self.config.as_ref().map(|c| c.fec_enabled).unwrap_or(false);
                Ok(Some(State::ReceivingResponse { recv: RecvBlock::new(fec) }))
            },
            Ok(SendResult::CompleteNoResponse) => {
                event_handler(ClientEvent::ResponseComplete);
                Ok(Some(State::Established { idle: 0 }))
            },
            Ok(SendResult::Active) | Ok(SendResult::PendingResponse) => Ok(None),
            Err(SendError::TimeOut) => {
                    info!("Failed to send packet, returning to listening");
                    Ok(Some(State::Listening { idle: 0 }))
            },
            Err(SendError::Io(e)) => Err(e)
        }
    }

    fn handle_established<P,R,W,E>(&mut self, idle: &mut usize, event: Event<R,W>, packet_writer: &mut P, event_handler: &mut E) -> io::Result<Option<State>>
            where P: FramedWrite, R: io::Read, W: io::Write, E: FnMut(ClientEvent) {
        if let Some(ref config) = self.config {
            match event {
                Event::Tick { ms } => {
                    *idle += ms;

                    if *idle >= IDLE_TIMEOUT {
                        info!("No activity in {}ms, returning to idle", *idle);
                        event_handler(ClientEvent::Disconnected);

                        Ok(Some(State::Idle))
                    } else {
                        Ok(None)
                    }
                },
                Event::StartRequest { data_size, request_reader } => {
                    let fec = if config.fec_enabled {
                        Some(0)
                    } else {
                        None
                    };

                    let mut send = SendBlock::new(data_size, config.link_width, fec, config.retry_enabled, self.retry_config.clone());
                    let send_res = send.send(packet_writer, request_reader);
                    match self.handle_send_data(send_res.map_err(|e| SendError::Io(e)), event_handler) {
                        Ok(None) => Ok(Some(State::SendingRequest { send })),
                        o => o
                    }
                },
                Event::Data { packet, .. } => {
                    Ok(match &packet.0 {
                        &Packet::Control(ref ctrl) => {
                            if self.callsign.as_bytes() == ctrl.dest_callsign && config.callsign.as_bytes() == ctrl.source_callsign {
                                match ctrl.ctrl_type {
                                    ControlType::LinkClear => {
                                        info!("Force disconnect from link, moving to idle");
                                        event_handler(ClientEvent::Disconnected);

                                        Some(State::Idle)
                                    },
                                    ref o => {
                                        trace!("Ignored invalid control type {:?}", o);
                                        None
                                    }
                                }
                            } else {
                                trace!("Ignored control packet not targeted for us S: {:?}, D: {:?}", ctrl.source_callsign, ctrl.dest_callsign);
                                None
                            }
                        },
                        o => {
                            trace!("Ignored non control packet {:?}", o);
                            None
                        }
                    })
                }
                e => return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Unsupported event {:?} for Established state", e)))
            }
        } else {
            error!("In established state with no config, returning to listening");
            Ok(Some(State::Listening { idle: 0 }))
        }
    }

    fn handle_send<P,R,W,E>(&mut self, send: &mut SendBlock, event: Event<R,W>, packet_writer: &mut P, event_handler: &mut E) -> io::Result<Option<State>>
            where P: FramedWrite, R: io::Read, W: io::Write, E: FnMut(ClientEvent) {
        let res = match event {
            Event::Data { packet, request_reader, .. } => send.on_packet(&packet.0, packet_writer, request_reader),
            Event::Tick { ms } => send.tick(ms, packet_writer),
            e => Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Unsupported event {:?} for Send state", e)).into())
        };

        self.handle_send_data(res, event_handler)
    }

    fn handle_recv<P,R,W,E>(&mut self, recv: &mut RecvBlock, event: Event<R,W>, packet_writer: &mut P, event_handler: &mut E) -> io::Result<Option<State>>
            where P: FramedWrite, R: io::Read, W: io::Write, E: FnMut(ClientEvent) {
        let res = match event {
            Event::Data { packet, response_writer, .. } => {
                match recv.on_packet(packet, packet_writer, response_writer) {
                    Ok(RecvResult::Complete) => {
                        event_handler(ClientEvent::ResponseComplete);
                        Ok(Some(State::Established { idle: 0 }))
                    },
                    Ok(RecvResult::CompleteSendResponse) => recv.send_response(false, packet_writer).map(|_| None),
                    Ok(RecvResult::Active) => Ok(None),
                    Err(e) => Err(e)
                }
            },
            Event::Tick { ms } => {
                Ok(None)
            },
            e => Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Unsupported event {:?} for Recv state", e)).into())
        };

        match res {
            Ok(new_state) => Ok(new_state),
            Err(e) => match e {
                RecvError::TimedOut | RecvError::NotResponding => {
                    info!("Failed to recv response, returning to listening");
                    Ok(Some(State::Listening { idle: 0 }))
                },
                RecvError::Decode(_) => Ok(None),
                RecvError::Io(e) => Err(e)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_broadcast() {
        let mut node = Node::new("KI7EST".to_string(), None, RetryConfig::default(1200));
        let mut packet_writer = vec!();
        assert_eq!(node.get_state(), ClientState::Listening);
        node.tick(LISTEN_TIMEOUT, &mut packet_writer, |_| {
            assert!(false);
        }).unwrap();
        assert_eq!(node.get_state(), ClientState::Listening);

        let broadcast = Packet::Broadcast(BroadcastPacket {
            fec_enabled: true,
            retry_enabled: true,
            major_ver: 1,
            minor_ver: 1,
            link_width: 32,
            callsign: "KI7EST".as_bytes(),
        });

        let mut response_writer = vec!();
        let mut request_reader = io::Cursor::new(&[]);
        node.on_data(&(broadcast, 0), &mut packet_writer, &mut response_writer, &mut request_reader, |e| {
            assert_eq!(e, ClientEvent::StateChange(ClientState::Listening, ClientState::Idle));
        }).unwrap();
        assert_eq!(node.get_state(), ClientState::Idle);
    }

    fn get_default_config() -> LinkConfig {
        LinkConfig {
            fec_enabled: true,
            retry_enabled: true,
            major_ver: 1,
            minor_ver: 1,
            link_width: 32,
            callsign: "KI7EST".to_string()
        }
    }

    #[test]
    fn test_idle() {
        let mut node = Node::new("KI7EST".to_string(), Some(get_default_config()), RetryConfig::default(1200));
        let mut packet_writer = vec!();

        assert_eq!(node.get_state(), ClientState::Listening);
        node.tick(LISTEN_TIMEOUT, &mut packet_writer, |e| {
            assert_eq!(e, ClientEvent::StateChange(ClientState::Listening, ClientState::Idle));
        }).unwrap();
        assert_eq!(node.get_state(), ClientState::Idle)
    }

    #[test]
    fn test_link_clear() {
        let mut node = Node::new("KI7EST".to_string(), Some(get_default_config()), RetryConfig::default(1200));
        let mut packet_writer = vec!();

        let clear = Packet::Control(ControlPacket {
            source_callsign: "KI7EST".as_bytes(),
            dest_callsign: "ANY".as_bytes(),
            ctrl_type: ControlType::LinkClear
        });

        let mut response_writer = vec!();
        let mut request_reader = io::Cursor::new(&[]);

        assert_eq!(node.get_state(), ClientState::Listening);
        node.on_data(&(clear, 0), &mut packet_writer, &mut response_writer, &mut request_reader, |e| {
            assert_eq!(e, ClientEvent::StateChange(ClientState::Listening, ClientState::Idle));
        }).unwrap();
        assert_eq!(node.get_state(), ClientState::Idle)
    }

    #[test]
    fn test_other_data() {
        let mut node = Node::new("KI7EST".to_string(), Some(get_default_config()), RetryConfig::default(1200));
        let mut packet_writer = vec!();
        node.tick(LISTEN_TIMEOUT, &mut packet_writer, |_| {}).unwrap();

        assert_eq!(node.get_state(), ClientState::Idle);
        node.on_other_data(&mut packet_writer, |e| {
            assert_eq!(e, ClientEvent::StateChange(ClientState::Idle, ClientState::Listening));
        }).unwrap();

        assert_eq!(node.get_state(), ClientState::Listening);
    }

    fn connect() -> Node {
        let mut node = Node::new("KI7EST".to_string(), Some(get_default_config()), RetryConfig::default(1200));
        let mut packet_writer = vec!();
        node.tick(LISTEN_TIMEOUT, &mut packet_writer, |_| {}).unwrap();

        node.connect(&mut packet_writer, |e| {
            assert_eq!(e, ClientEvent::StateChange(ClientState::Idle, ClientState::Negotiating));
        }).unwrap();
        assert_eq!(node.get_state(), ClientState::Negotiating);
        assert!(packet_writer.len() != 0);

        {
            let decoded = decode(&mut packet_writer[..], true).unwrap();

            match decoded.0 {
                Packet::Control(ctrl) => {
                    assert_eq!(ctrl.source_callsign, "KI7EST".as_bytes());
                    assert_eq!(ctrl.dest_callsign, "KI7EST".as_bytes());
                    assert_eq!(ctrl.ctrl_type, ControlType::LinkRequest);
                },
                o => assert!(false, "{:?}", o)
            }
        }
        packet_writer.clear();

        let response = Packet::Control(ControlPacket {
            source_callsign: "KI7EST".as_bytes(),
            dest_callsign: "KI7EST".as_bytes(),
            ctrl_type: ControlType::LinkOpened
        });

        let mut response_writer = vec!();
        let mut request_reader = io::Cursor::new(&[]);
        let mut event_idx = 0;
        node.on_data(&(response, 0), &mut packet_writer, &mut response_writer, &mut request_reader, |e| {
            match event_idx {
                0 => assert_eq!(e, ClientEvent::Connected),
                1 => assert_eq!(e, ClientEvent::StateChange(ClientState::Negotiating, ClientState::Established)),
                _ => assert!(false)
            }

            event_idx += 1;
        }).unwrap();
        assert_eq!(node.get_state(), ClientState::Established);

        node
    }

    #[test]
    fn test_connect() {
        connect();
    }

    #[test]
    fn test_timeout() {
        let mut node = connect();
        let mut packet_writer = vec!();

        let mut event_idx = 0;
        node.tick(IDLE_TIMEOUT, &mut packet_writer, |e| {
            match event_idx {
                0 => assert_eq!(e, ClientEvent::Disconnected),
                1 => assert_eq!(e, ClientEvent::StateChange(ClientState::Established, ClientState::Idle)),
                _ => assert!(false)
            }

            event_idx += 1;
        }).unwrap();
        assert_eq!(event_idx, 2);
        assert_eq!(node.get_state(), ClientState::Idle);
    }

    #[test]
    fn test_disconnect() {
        let mut node = connect();
        let mut packet_writer = vec!();

        let disconnect = Packet::Control(ControlPacket {
            source_callsign: "KI7EST".as_bytes(),
            dest_callsign: "KI7EST".as_bytes(),
            ctrl_type: ControlType::LinkClear
        });

        let mut response_writer = vec!();
        let mut request_reader = io::Cursor::new(&[]);
        let mut event_idx = 0;
        node.on_data(&(disconnect, 0), &mut packet_writer, &mut response_writer, &mut request_reader, |e| {
            match event_idx {
                0 => assert_eq!(e, ClientEvent::Disconnected),
                1 => assert_eq!(e, ClientEvent::StateChange(ClientState::Established, ClientState::Idle)),
                _ => assert!(false)
            }

            event_idx += 1;
        }).unwrap();
        assert_eq!(event_idx, 2);
        assert_eq!(node.get_state(), ClientState::Idle);
    }

    #[test]
    fn test_connect_fail() {
        use simple_logger;
        simple_logger::init();

        let mut node = Node::new("KI7EST".to_string(), Some(get_default_config()), RetryConfig::default(1200));
        let mut packet_writer = vec!();
        node.tick(LISTEN_TIMEOUT, &mut packet_writer, |_| {}).unwrap();

        node.connect(&mut packet_writer, |_| {}).unwrap();

        let ctrl_bytes = calc_ctrl_bytes("KI7EST", "KI7EST", true);
        let retry_ms = RetryConfig::default(1200).calc_delay(ctrl_bytes, ctrl_bytes);
        let retry_attempts = RetryConfig::default(1200).retry_attempts * 2;

        for i in 0..retry_attempts+1 {
            if i % 2 == 0 {
                node.tick(1, &mut packet_writer, |e| {
                    assert!(false, "{} {:?}", i, e);
                }).unwrap();
            } else {
                let mut event_idx = 0;
                node.tick(retry_ms - 1, &mut packet_writer, |e| {
                    if i == retry_attempts {
                        match event_idx {
                            0 => assert_eq!(e, ClientEvent::ConnectionFailed),
                            1 => assert_eq!(e, ClientEvent::StateChange(ClientState::Negotiating, ClientState::Idle)),
                            _ => assert!(false)
                        }

                        event_idx += 1;
                    } else {
                        assert!(false);
                    }
                }).unwrap();

                if i == retry_attempts {
                    assert_eq!(event_idx, 2);
                }
            }
        }
    }
}