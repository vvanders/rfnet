use std::io::Read;

enum State {
    PendingAck(usize),
    NodeListening
}

pub struct Transfer<R> where R: Read {
    session_id: u16,
    packet_idx: u16,
    reader: R,
    state: State
}