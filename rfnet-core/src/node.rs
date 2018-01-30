use packet::*;

use std::io;

#[derive(Clone)]
pub enum State {
    Listening(usize),
    Idle,
    Negotiating(usize),
    Established
}

pub struct Node<W> where W: io::Write {
    writer: W,
    state: State,
    link_info: Option<LinkInformation>
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

impl<W> Node<W> where W: io::Write {
    pub fn new(writer: W) -> Node<W> {
        Node {
            writer,
            state: State::Listening(0),
            link_info: None
        }
    }

    pub fn link_info(&self) -> &Option<LinkInformation> {
        &self.link_info
    }

    fn is_fec_enabled(&self) -> bool {
        self.link_info.as_ref().map(|v| v.fec_enabled).unwrap_or(true)
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

    pub fn recv_data(&mut self, data: &mut [u8]) -> Result<Option<usize>, PacketDecodeError> {
        let (new_state, next_tick) = match self.state {
            State::Listening(_) | State::Idle => {
                let broadcast = self.decode_broadcast(data);

                if let Ok(Some(link_info)) = broadcast {
                    //Broadcast packets don't count towards listening/idle
                    self.link_info = Some(link_info);

                    (self.state.clone(), None)
                } else {
                    //Any packet while listening or idle resets out listen timer
                    (State::Listening(0), Some(LISTEN_TIMEOUT))
                }
            },
            State::Negotiating(_) => {
                (self.state.clone(), None)
            },
            State::Established => (self.state.clone(), None)
        };

        self.state = new_state;

        Ok(next_tick)
    }

    pub fn elapsed(&mut self, ms: usize) -> Result<Option<usize>, io::Error> {
        let (new_state, next_tick) = match self.state {
            State::Listening(w) => {
                let elapsed = ms+w;
                if elapsed >= LISTEN_TIMEOUT {
                    (State::Idle, None)
                } else {
                    (State::Listening(elapsed), Some(LISTEN_TIMEOUT - elapsed))
                }
            },
            _ => (self.state.clone(), None)
        };

        self.state = new_state;

        Ok(next_tick)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn recv<W>(node: &mut Node<W>, packet: Packet, fec: bool) -> Option<usize> where W: io::Write {
        let mut scratch = vec!();

        encode(&packet, fec, &mut scratch).unwrap();
        node.recv_data(&mut scratch[..]).unwrap()
    }

    #[test]
    fn test_broadcast() {

        let callsign = "ki7est";
        let broadcast = BroadcastPacket {
            fec_enabled: true,
            retry_enabled: true,
            link_width: 32,
            major_ver: 1,
            minor_ver: 0,
            callsign: callsign.as_bytes()
        };

        let link_info = LinkInformation {
            fec_enabled: true,
            retry_enabled: true,
            link_width: 32,
            major_ver: 1,
            minor_ver: 0,
            callsign: callsign.to_string()
        };

        {
            let mut node = Node::new(vec!());
            recv(&mut node, Packet::Broadcast(broadcast.clone()), true);
            assert_eq!(node.link_info(), &Some(link_info.clone()));
        }

        {
            let mut node = Node::new(vec!());
            recv(&mut node, Packet::Broadcast(broadcast.clone()), false);
            assert_eq!(node.link_info(), &Some(link_info.clone()));
        }

        {
            let mut node = Node::new(vec!());
            recv(&mut node, Packet::Broadcast(broadcast.clone()), true);
            assert_eq!(node.link_info(), &Some(link_info.clone()));

            let mut new_state = broadcast.clone();
            new_state.link_width = 64;

            let mut new_info = link_info.clone();
            new_info.link_width = 64;

            recv(&mut node, Packet::Broadcast(new_state), true);
            assert_eq!(node.link_info(), &Some(new_info));
        }
    }
}