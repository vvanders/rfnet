use packet::*;
use acked_packet::{AckedPacket, AckResult};

use std::io;

#[derive(Clone)]
pub enum State {
    Listening(usize),
    Idle,
    Negotiating(AckedPacket),
    Established
}

pub enum RecvResult {
    BadPacket,
    Noop,
    Ack(usize,usize),
    Complete(u16, bool),
    Response(u16)
}

pub struct Node<'a,W,R,T> where W: io::Write, W:'a, R: io::Write, R: 'a, T: io::Read {
    callsign: String,
    packet_writer: &'a mut W,
    response_writer: &'a mut R,
    last_packet: Vec<u8>,
    state: State,
    link_info: Option<LinkInformation>,
    pending_requests: Vec<(u16,T)>,
    next_request_id: u16
}

#[derive(Debug, Clone, PartialEq)]
struct LinkInformation {
    fec_enabled: bool,
    retry_enabled: bool,
    callsign: String,
    link_width: u16,
    major_ver: u8,
    minor_ver: u8
}


const LISTEN_TIMEOUT: usize = 10 * 1000;
const NEGOTIATE_RETRY: usize = 500;

impl<'a,W,R,T> Node<'a,W,R,T> where W: io::Write, W: 'a, R: io::Write, R: 'a, T: io::Read {
    pub fn new(callsign: String, packet_writer: &'a mut W, response_writer: &'a mut R) -> Node<'a,W,R,T> {
        Node {
            callsign,
            packet_writer,
            response_writer,
            last_packet: vec!(),
            state: State::Listening(0),
            link_info: None,
            pending_requests: vec!(),
            next_request_id: 0
        }
    }

    pub fn link_info(&self) -> &Option<LinkInformation> {
        &self.link_info
    }

    pub fn send_request(&mut self, request_reader: T) -> Result<u16, PacketEncodeError> {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id + 1;

        self.pending_requests.push((request_id, request_reader));

        //If we're idle kick over to connecting
        self.connect().map(|_| request_id)
    }

    fn connect(&mut self) -> Result<(), PacketEncodeError> {
        if let State::Idle = self.state {
            self.state = State::Negotiating(AckedPacket::new(NEGOTIATE_RETRY));
            self.last_packet.clear();

            let fec = self.is_fec_enabled();
            //Destructure this here so we can borrow different fields as mut/non-mut
            let &mut Node {ref link_info, ref mut packet_writer, .. } = self;

            let dest = if let &Some(ref link_info) = link_info {
                link_info.callsign.as_bytes()
            } else {
                &"".as_bytes()
            };

            let packet = Packet::Control(ControlPacket {
                ctrl_type: ControlType::LinkRequest,
                session_id: 0,
                source_callsign: self.callsign.as_bytes(),
                dest_callsign: dest
            });

            encode(&packet, fec, packet_writer).map(|_| ())
        } else {
            Ok(())
        }
    }

    fn is_fec_enabled(&self) -> bool {
        self.link_info.as_ref().map(|v| v.fec_enabled).unwrap_or(true)
    }

    fn get_link_width(&self) -> u16 {
        self.link_info.as_ref().map(|v| v.link_width).unwrap_or(256)
    }

    fn get_dest_callsign(&self) -> &str {
        if let Some(ref link_info) = self.link_info {
            &link_info.callsign.as_str()
        } else {
            &""
        }
    }

    fn decode_broadcast(&mut self, data: &mut [u8]) -> Result<Option<LinkInformation>, PacketDecodeError> {
        fn parse_link_info(header: &BroadcastPacket) -> LinkInformation {
            LinkInformation {
                fec_enabled: header.fec_enabled,
                retry_enabled: header.retry_enabled,
                link_width: header.link_width,
                major_ver: header.major_ver,
                minor_ver: header.minor_ver,
                callsign: String::from_utf8_lossy(header.callsign).to_string()
            }
        }

        let link_info = decode(data, self.is_fec_enabled()).map(|packet| {
            match packet {
                (Packet::Broadcast(ref header),_) => Some(parse_link_info(header)),
                _ => None
            }
        });

        if let Ok(Some(link_info)) = link_info {
            Ok(Some(link_info))
        } else if self.link_info.is_none() {
            decode(data, false).map(|packet| {
                match packet {
                    (Packet::Broadcast(ref header),_) => Some(parse_link_info(header)),
                    _ => None
                }
            })
        } else {
            Ok(None)
        }
    }

    pub fn recv_data(&mut self, data: &mut [u8]) -> Result<RecvResult, PacketDecodeError> {
        let fec = self.is_fec_enabled();

        let new_state = match self.state {
            State::Listening(_) | State::Idle => {
                let broadcast = self.decode_broadcast(data);

                if let Ok(Some(link_info)) = broadcast {
                    //Broadcast packets don't count towards listening/idle
                    self.link_info = Some(link_info);

                    None
                } else {
                    //Any packet while listening or idle resets out listen timer
                    Some((State::Listening(0), RecvResult::Noop))
                }
            },
            State::Negotiating(ref mut ack_state) => {
                let callsign = self.callsign.as_bytes();
                let ack = match decode(data, fec)? {
                    (Packet::Control(ref header),_) if header.dest_callsign == callsign => {
                        match header.ctrl_type {
                            ControlType::LinkOpened => Some(header.session_id),
                            _ => None
                        }
                    },
                    _ => None
                };

                if let Some(session_id) = ack {
                    Some((State::Established, RecvResult::Ack(1,1)))
                } else {
                    None
                }
            },
            State::Established => None
        };

        if let Some((new_state, result)) = new_state {
            self.state = new_state;
            Ok(result)
        } else {
            Ok(RecvResult::Noop)
        }
    }

    fn is_suspend_packet(packet: &Packet) -> bool {
        if let &Packet::Control(ref ctrl) = packet {
            if let ControlType::NodeWaiting = ctrl.ctrl_type {
                return true
            }
        }

        false
    }

    pub fn elapsed(&mut self, ms: usize) -> Result<bool, io::Error> {
        let result = match self.state {
            State::Listening(w) => {
                let elapsed = ms+w;
                if elapsed >= LISTEN_TIMEOUT {
                    Some(State::Idle)
                } else {
                    Some(State::Listening(elapsed))
                }
            },
            State::Negotiating(ref mut ack_state) => {
                match ack_state.tick(&self.last_packet[..], ms, &mut self.packet_writer) {
                    Ok(AckResult::Failed) => Some(State::Listening(0)),
                    Ok(AckResult::Waiting(_next_tick)) => None,
                    Err(e) => None,
                    _ => None
                }
            },
            State::Established => None,
            State::Idle => None
        };

        if let Some(new_state) = result {
            self.state = new_state;
        }

        if let State::Idle = self.state {
            Ok(false)
        } else {
            Ok(true)
        }
    }
}

// #[cfg(test)]
// mod test {
//     use super::*;

//     fn recv<W,R,T>(node: &mut Node<W,R,T>, packet: Packet, fec: bool) where W: io::Write, R: io::Write, T: io::Read {
//         let mut scratch = vec!();

//         encode(&packet, fec, &mut scratch).unwrap();
//         node.recv_data(&mut scratch[..]).unwrap();
//     }

//     #[test]
//     fn test_broadcast() {
//         let callsign = "ki7est";
//         let broadcast = BroadcastPacket {
//             fec_enabled: true,
//             retry_enabled: true,
//             link_width: 32,
//             major_ver: 1,
//             minor_ver: 0,
//             callsign: callsign.as_bytes()
//         };

//         let link_info = LinkInformation {
//             fec_enabled: true,
//             retry_enabled: true,
//             link_width: 32,
//             major_ver: 1,
//             minor_ver: 0,
//             callsign: callsign.to_string()
//         };

//         {
//             let mut node = Node::new(callsign.to_string(),&mut vec!(), &mut vec!());
//             recv(&mut node, Packet::Broadcast(broadcast.clone()), true);
//             assert_eq!(node.link_info(), &Some(link_info.clone()));
//         }

//         {
//             let mut node = Node::new(callsign.to_string(),&mut vec!(), &mut vec!());
//             recv(&mut node, Packet::Broadcast(broadcast.clone()), false);
//             assert_eq!(node.link_info(), &Some(link_info.clone()));
//         }

//         {
//             let mut node = Node::new(callsign.to_string(),&mut vec!(), &mut vec!());
//             recv(&mut node, Packet::Broadcast(broadcast.clone()), true);
//             assert_eq!(node.link_info(), &Some(link_info.clone()));

//             let mut new_state = broadcast.clone();
//             new_state.link_width = 64;

//             let mut new_info = link_info.clone();
//             new_info.link_width = 64;

//             recv(&mut node, Packet::Broadcast(new_state), true);
//             assert_eq!(node.link_info(), &Some(new_info));
//         }
//     }
// }