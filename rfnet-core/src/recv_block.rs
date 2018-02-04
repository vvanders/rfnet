use packet::*;

use std::io;

pub struct RecvBlock<T> where T: io::Write {
    session_id: u16,
    fec: bool,
    packet_idx: u16,
    last_heard: usize,
    waiting_for_response: bool,
    data_output: T,
    stats: RecvStats,
    decode_block: Vec<u8>
}

pub struct RecvStats {
    pub recv_bytes: usize,
    pub recv_bit_err: usize,
    pub packets_received: usize,
    pub acks_sent: usize
}

pub enum RecvResult<'a> {
    Status(&'a RecvStats),
    CompleteSendResponse,
    Complete
}

pub enum RecvError {
    Io(io::Error),
    Decode(PacketDecodeError),
    Encode(PacketEncodeError),
    NotResponding,
    TimedOut
}

impl From<PacketDecodeError> for RecvError {
    fn from(err: PacketDecodeError) -> RecvError {
        RecvError::Decode(err)
    }
}

impl From<PacketEncodeError> for RecvError {
    fn from(err: PacketEncodeError) -> RecvError {
        RecvError::Encode(err)
    }
}

impl From<DataDecodeError> for RecvError {
    fn from(err: DataDecodeError) -> RecvError {
        match err {
            DataDecodeError::TooManyFECErrors => RecvError::Decode(PacketDecodeError::TooManyFECErrors),
            DataDecodeError::Io(io) => RecvError::Io(io)
        }
    }
}

impl From<io::Error> for RecvError {
    fn from(err: io::Error) -> RecvError {
        RecvError::Io(err)
    }
}

const TIMEOUT_MS: usize = 10_000;
const PENDING_REPEAT_MS: usize = 500;

impl<T> RecvBlock<T> where T: io::Write {
    pub fn new(session_id: u16, fec: bool, data_output: T) -> RecvBlock<T> {
        RecvBlock {
            session_id,
            fec,
            packet_idx: 0,
            last_heard: 0,
            waiting_for_response: false,
            data_output,
            stats: RecvStats {
                recv_bytes: 0,
                recv_bit_err: 0,
                packets_received: 0,
                acks_sent: 0
            },
            decode_block: vec!()
        }
    }

    fn send_nack<W>(packet_idx: u16, fec: bool, packet_writer: &mut W) -> Result<(), RecvError> where W: io::Write {
        let packet = Packet::Ack(AckPacket {
            packet_idx,
            nack: true,
            response: false,
            pending_response: false,
            corrected_errors: 0
        });

        encode(&packet, fec, packet_writer).map(|_| ())?;

        Ok(())
    }

    fn send_ack<W>(packet_idx: u16, err: u16, fec: bool, packet_writer: &mut W) -> Result<(), RecvError> where W: io::Write {
        let packet = Packet::Ack(AckPacket {
            packet_idx,
            nack: false,
            response: false,
            pending_response: false,
            corrected_errors: err
        });

        encode(&packet, fec, packet_writer).map(|_| ())?;

        Ok(())
    }

    fn send_pending_response<W>(packet_idx: u16, err: u16, fec: bool, packet_writer: &mut W) -> Result<(), RecvError> where W: io::Write {
        let packet = Packet::Ack(AckPacket {
            packet_idx,
            nack: false,
            response: false,
            pending_response: true,
            corrected_errors: err
        });

        encode(&packet, fec, packet_writer).map(|_| ())?;

        Ok(())
    }

    pub fn on_packet<W>(&mut self, (packet, err): (&Packet, usize), packet_writer: &mut W) -> Result<RecvResult, RecvError> where W: io::Write {
        match packet {
            &Packet::Data(ref header, payload) => {
                let packet_idx = if header.start_flag {
                    0
                } else {
                    header.packet_idx
                };

                if self.packet_idx == 0 && header.start_flag && header.packet_idx != self.session_id {
                    Self::send_nack(0, self.fec, packet_writer)?;
                } else if header.packet_idx == self.packet_idx {
                    //try to decode, technically we could let the client handle this but then they'd have to be
                    //responsible for "rewinding" on FEC error.
                    self.decode_block.clear();
                    let block_errs = match decode_data_blocks(header, payload, self.fec, &mut self.decode_block) {
                        Ok(s) => s,
                        Err(DataDecodeError::TooManyFECErrors) => {
                            //Send NACK since we know id
                            Self::send_nack(packet_idx, self.fec, packet_writer)?;

                            return Err(RecvError::Decode(PacketDecodeError::TooManyFECErrors));
                        },
                        Err(e) => Err(e)?
                    };

                    self.data_output.write(&self.decode_block[..])?;

                    self.last_heard = 0;

                    let total_err: u16 = (err + block_errs) as u16;
                    if header.end_flag {
                        Self::send_pending_response(packet_idx, total_err, self.fec, packet_writer)?;
                        self.waiting_for_response = true;

                        return Ok(RecvResult::CompleteSendResponse)
                    } else {
                        Self::send_ack(packet_idx, total_err, self.fec, packet_writer)?;
                        self.packet_idx += 1;
                    }

                } else if header.packet_idx < self.packet_idx {
                    //We alread heard this so re-ack it
                    Self::send_ack(packet_idx, 0, self.fec, packet_writer)?;
                }
            }
            _ => {}
        }

        Ok(RecvResult::Status(&self.stats))
    }

    pub fn tick<W>(&mut self, elapsed_ms: usize, packet_writer: &mut W) -> Result<RecvResult, RecvError> where W: io::Write {
        self.last_heard += elapsed_ms;

        if self.waiting_for_response {
            if self.last_heard > PENDING_REPEAT_MS {
                Self::send_pending_response(self.packet_idx, 0, self.fec, packet_writer)?;
                self.last_heard = 0;
            }
        } else {
            if self.last_heard > TIMEOUT_MS {
                return Err(RecvError::TimedOut)
            }
        }

        Ok(RecvResult::Status(&self.stats))
    }

    pub fn send_response<W>(&mut self, is_response: bool, packet_writer: &mut W) -> Result<RecvResult, RecvError> where W: io::Write {
        if !self.waiting_for_response {
            return Err(RecvError::NotResponding)
        }

        let packet = Packet::Ack(AckPacket {
            packet_idx: self.packet_idx,
            nack: false,
            response: is_response,
            pending_response: false,
            corrected_errors: 0
        });

        encode(&packet, self.fec, packet_writer).map(|_| ())?;

        Ok(RecvResult::Complete)
    }
}