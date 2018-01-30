use packet::*;

use std::io;

pub enum State {
    Idle,
    Negotiating,
    Established
}

pub struct Link<W> where W: io::Write {
    writer: W,
    state: State
}

impl<W> Link<W> where W: io::Write {
    pub fn new(writer: W) -> Link<W> {
        Link {
            writer,
            state: State::Idle
        }
    }

    pub fn recv_data(data: &[u8]) -> Result<Option<usize>, PacketDecodeError> {
        Ok(None)
    }

    pub fn elapsed(ms: usize) -> Result<Option<usize>, io::Error> {
        Ok(None)
    }
}