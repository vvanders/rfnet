use packet::*;
use framed::FramedWrite;
use send_block::{SendBlock, RetryConfig};

use std::io;

pub struct Node {
    callsign: String,
    state: State,
    config: Option<LinkConfig>,
    retry_config: RetryConfig
}

enum State {
    Listening { idle: usize },
    Idle,
    Negotiating { retry_count: usize, last_attempt: usize },
    Established,
    SendingRequest,
    ReceivingResponse
}

pub struct LinkConfig {
    pub fec_enabled: bool,
    pub retry_enabled: bool,
    pub major_ver: u8,
    pub minor_ver: u8,
    pub link_width: u16,
    pub callsign: String
}

#[derive(Debug)]
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
            &State::Listening { idle: _idle } => ClientState::Listening,
            &State::Idle => ClientState::Idle,
            &State::Negotiating { retry_count: _r, last_attempt: _l } => ClientState::Negotiating,
            &State::Established => ClientState::Established,
            &State::SendingRequest => ClientState::Sending,
            &State::ReceivingResponse => ClientState::Receiving
        }
    }
}

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

impl Node {
    pub fn new(callsign: String, config: Option<LinkConfig>, retry_config: RetryConfig) -> Node {
        Node {
            callsign,
            state: State::Listening { idle: 0 },
            config,
            retry_config
        }
    }

    pub fn start_request(&mut self) -> bool {
        match &self.state {
            &State::Idle => {
                true
            },
            &State::Established => {
                true
            },
            o => {
                info!("Unable to start request, in {:?} state", ClientState::translate(o));
                false
            }
        }
    }

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

    fn send_negotiation_request<W>(source: &String, dest: &String, packet_writer: &mut W) -> io::Result<()> where W: FramedWrite {
        Ok(())
    }

    pub fn on_data<P,W,R,E>(&mut self, packet: &(Packet, usize), packet_writer: &mut P, response_writer: &mut W, request_reader: &mut R, mut event_handler: E) -> io::Result<()> 
            where P: FramedWrite, W: io::Write, R: io::Read, E: FnMut(ClientEvent) {
        let new_state = match &mut self.state {
            &mut State::Listening { ref mut idle } => {
                match &packet.0 {
                    &Packet::Broadcast(ref broadcast) => {
                        self.config = Some(Self::link_config_from_broadcast(broadcast));
                        info!("Heard broadcast packet from {}, channel is idle", String::from_utf8_lossy(broadcast.callsign));
                        Some(State::Idle)
                    },
                    &Packet::Control(ref ctrl) => match ctrl.ctrl_type {
                        ControlType::LinkClear if self.config.is_some() => Some(State::Idle),
                        _ => {
                            *idle = 0;
                            None
                        }
                    },
                    _ => {
                        *idle = 0;
                        None
                    }
                }
            },
            &mut State::Idle => {
                if let &Packet::Broadcast(ref broadcast) = &packet.0 {
                    self.config = Some(Self::link_config_from_broadcast(broadcast));
                    None
                } else {
                    info!("Non-broadcast packet heard on channel, channel is busy");
                    Some(State::Listening { idle: 0 })
                }
            },
            &mut State::Negotiating { ref mut retry_count, ref mut last_attempt } => {
                if let Packet::Control(ref ctrl) = packet.0 {
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
                                Some(State::Established)
                            },
                            _ => None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            &mut State::Established => {
                None
            }
            &mut State::SendingRequest => {
                None
            },
            &mut State::ReceivingResponse => {
                None
            }
        };

        if let Some(new_state) = new_state {
            event_handler(ClientEvent::StateChange(ClientState::translate(&self.state), ClientState::translate(&new_state)));
            self.state = new_state;
        }

        Ok(())
    }

    pub fn tick<W,E>(&mut self, ms: usize, packet_writer: &mut W, mut handle_event: E) -> io::Result<()> where W: FramedWrite, E: FnMut(ClientEvent) {
        let new_state = match &mut self.state {
            &mut State::Listening { ref mut idle }=> {
                *idle += ms;

                if *idle >= LISTEN_TIMEOUT {
                    info!("Nothing heard on channel, channel is idle");
                    Some(State::Idle)
                } else {
                    None
                }
            },
            &mut State::Idle => None,
            &mut State::Negotiating { ref mut retry_count, ref mut last_attempt } => {
                if let Some(ref config) = self.config {
                    *last_attempt += ms;

                    if *retry_count >= self.retry_config.retry_attempts {
                        info!("Failed to connect, resetting to listening");
                        handle_event(ClientEvent::ConnectionFailed);

                        Some(State::Listening { idle: 0 })
                    } else {
                        let ctrl_bytes = calc_ctrl_bytes(self.callsign.as_str(), config.callsign.as_str(), config.fec_enabled);
                        if *last_attempt >= self.retry_config.calc_delay(ctrl_bytes, ctrl_bytes) {
                            info!("Failed to hear negotiation response, resending");
                            Self::send_negotiation_request(&self.callsign, &config.callsign, packet_writer)?;

                            *last_attempt += 1;
                            *retry_count += 1;
                        }

                        None
                    }
                } else {
                    error!("Attempting to negotiate with empty config, resetting to listening");
                    Some(State::Listening { idle: 0 })
                }
            },
            &mut State::Established => {
                None
            },
            &mut State::SendingRequest => {
                None
            },
            &mut State::ReceivingResponse => {
                None
            }
        };

        if let Some(new_state) = new_state {
            self.state = new_state;
        }

        Ok(())
    }
}