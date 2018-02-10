use packet::*;
use framed::FramedWrite;

use std::io;

pub struct RecvBlock<T> where T: io::Write {
    session_id: u16,
    fec: bool,
    packet_idx: u16,
    last_heard: usize,
    last_sent: usize,
    waiting_for_response: bool,
    response: Option<bool>,
    data_output: T,
    stats: RecvStats,
    decode_block: Vec<u8>
}

#[derive(Debug)]
pub struct RecvStats {
    pub recv_bytes: usize,
    pub recv_bit_err: usize,
    pub packets_received: usize,
    pub acks_sent: usize
}

#[derive(Debug)]
pub enum RecvResult<'a> {
    Status(&'a RecvStats),
    CompleteSendResponse,
    Complete
}

#[derive(Debug)]
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
            last_sent: 0,
            waiting_for_response: false,
            response: None,
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

    fn send_nack<W>(packet_idx: u16, err: u16, fec: bool, packet_writer: &mut W) -> Result<(), RecvError> where W: FramedWrite {
        let packet = Packet::Ack(AckPacket {
            packet_idx,
            nack: true,
            response: false,
            pending_response: false,
            corrected_errors: err
        });

        packet_writer.start_frame()?;
        encode(&packet, fec, packet_writer).map(|_| ())?;
        packet_writer.end_frame()?;

        Ok(())
    }

    fn send_ack<W>(packet_idx: u16, err: u16, fec: bool, packet_writer: &mut W) -> Result<(), RecvError> where W: FramedWrite {
        let packet = Packet::Ack(AckPacket {
            packet_idx,
            nack: false,
            response: false,
            pending_response: false,
            corrected_errors: err
        });

        packet_writer.start_frame()?;
        encode(&packet, fec, packet_writer).map(|_| ())?;
        packet_writer.end_frame()?;

        Ok(())
    }

    fn send_pending_response<W>(packet_idx: u16, err: u16, fec: bool, packet_writer: &mut W) -> Result<(), RecvError> where W: FramedWrite {
        let packet = Packet::Ack(AckPacket {
            packet_idx,
            nack: false,
            response: false,
            pending_response: true,
            corrected_errors: err
        });

        packet_writer.start_frame()?;
        encode(&packet, fec, packet_writer).map(|_| ())?;
        packet_writer.end_frame()?;

        Ok(())
    }

    fn send_response_result<W>(response: Option<bool>, packet_idx: u16, fec: bool, packet_writer: &mut W) -> Result<(), RecvError> where W: FramedWrite {
        let packet = Packet::Ack(AckPacket {
            packet_idx: packet_idx,
            nack: false,
            response: response.unwrap(),
            pending_response: false,
            corrected_errors: 0
        });

        packet_writer.start_frame()?;
        encode(&packet, fec, packet_writer).map(|_| ())?;
        packet_writer.end_frame()?;

        Ok(())
    }

    pub fn on_packet<W>(&mut self, &(ref packet, err): &(Packet, usize), packet_writer: &mut W) -> Result<RecvResult, RecvError> where W: FramedWrite {
        match packet {
            &Packet::Data(ref header, payload) => {
                let packet_idx = if header.start_flag {
                    0
                } else {
                    header.packet_idx
                };

                if self.packet_idx == 0 && header.start_flag && header.packet_idx != self.session_id {
                    Self::send_nack(0, err as u16, self.fec, packet_writer)?;
                } else if packet_idx == self.packet_idx {
                    self.last_heard = 0;
                    self.last_sent = 0;

                    //try to decode, technically we could let the client handle this but then they'd have to be
                    //responsible for "rewinding" on FEC error.
                    self.decode_block.clear();
                    let block_errs = match decode_data_blocks(header, payload, self.fec, &mut self.decode_block) {
                        Ok(s) => s,
                        Err(DataDecodeError::TooManyFECErrors) => {
                            //Send NACK since we know id
                            Self::send_nack(packet_idx, err as u16, self.fec, packet_writer)?;

                            return Err(RecvError::Decode(PacketDecodeError::TooManyFECErrors));
                        },
                        Err(e) => Err(e)?
                    };

                    self.data_output.write(&self.decode_block[..])?;

                    let total_err: u16 = (err + block_errs) as u16;
                    if header.end_flag {
                        Self::send_pending_response(packet_idx, total_err, self.fec, packet_writer)?;
                        self.waiting_for_response = true;

                        return Ok(RecvResult::CompleteSendResponse)
                    } else {
                        Self::send_ack(packet_idx, total_err, self.fec, packet_writer)?;
                        self.packet_idx += 1;
                    }

                } else if packet_idx < self.packet_idx {
                    self.last_heard = 0;
                    self.last_sent = 0;

                    //We alread heard this so re-ack it
                    Self::send_ack(packet_idx, 0, self.fec, packet_writer)?;
                }
            }
            &Packet::Ack(ref ack) => {
                if !self.waiting_for_response {
                    return Err(RecvError::NotResponding)
                }

                return Ok(RecvResult::Complete)
            }
            _ => {}
        }

        Ok(RecvResult::Status(&self.stats))
    }

    pub fn tick<W>(&mut self, elapsed_ms: usize, packet_writer: &mut W) -> Result<RecvResult, RecvError> where W: FramedWrite {
        self.last_heard += elapsed_ms;
        self.last_sent += elapsed_ms;

        if self.waiting_for_response {
            if self.last_sent >= PENDING_REPEAT_MS {
                if self.response.is_none() {
                    Self::send_pending_response(self.packet_idx, 0, self.fec, packet_writer)?;
                } else {
                    if self.last_heard >= TIMEOUT_MS {
                        return Err(RecvError::TimedOut)
                    }

                    Self::send_response_result(self.response, self.packet_idx, self.fec, packet_writer)?;
                }
                self.last_sent = 0;
            }
        } else {
            if self.last_heard >= TIMEOUT_MS {
                return Err(RecvError::TimedOut)
            }
        }

        Ok(RecvResult::Status(&self.stats))
    }

    pub fn send_response<W>(&mut self, is_response: bool, packet_writer: &mut W) -> Result<RecvResult, RecvError> where W: FramedWrite {
        if !self.waiting_for_response {
            return Err(RecvError::NotResponding)
        }

        self.response = Some(is_response);
        self.last_heard = 0;

        Self::send_response_result(self.response, self.packet_idx, self.fec, packet_writer)?;

        Ok(RecvResult::CompleteSendResponse)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn gen_data<'a>(packet_idx: u16, start_flag: bool, end_flag: bool, data: &[u8], err: usize, packet_holder: &'a mut Vec<u8>) -> (Packet<'a>,usize) {
        packet_holder.clear();

        let header = DataPacket {
            packet_idx,
            fec_bytes: 0,
            start_flag,
            end_flag
        };

        encode_data(header, true, 2048, &mut io::Cursor::new(data), packet_holder).unwrap();
        (decode(packet_holder, true).unwrap().0, err)
    }

    fn send_to_response<'a>(recv_data: &'a mut Vec<u8>) -> RecvBlock<&'a mut Vec<u8>> {
        let payload = get_payload();

        let mut recv = RecvBlock::new(1000, true, recv_data);

        let mut output = vec!();
        let mut data_packet = vec!();

        match recv.on_packet(&gen_data(1000, true, false, &payload[..], 5, &mut data_packet), &mut output) {
            Ok(RecvResult::Status(_)) => {},
            o => panic!("{:?}", o)
        }

        match decode(&mut output[..], true) {
            Ok((Packet::Ack(ack),_)) => {
                assert_eq!(ack.packet_idx, 0);
                assert_eq!(ack.corrected_errors, 5);
                assert_eq!(ack.pending_response, false);
                assert_eq!(ack.nack, false);
            },
            o => panic!("{:?}", o)
        }

        output.clear();

        match recv.on_packet(&gen_data(1, false, true, &payload[..], 5, &mut data_packet), &mut output) {
            Ok(RecvResult::CompleteSendResponse) => {},
            o => panic!("{:?}", o)
        }

        match decode(&mut output[..], true) {
            Ok((Packet::Ack(ack),_)) => {
                assert_eq!(ack.packet_idx, 1);
                assert_eq!(ack.corrected_errors, 5);
                assert_eq!(ack.pending_response, true);
                assert_eq!(ack.nack, false);
            },
            o => panic!("{:?}", o)
        }

        recv
    }

    fn get_payload() -> Vec<u8> {
        (0..16).collect::<Vec<u8>>()
    }

    #[test]
    fn test_recv() {
        let mut recv_data = vec!();
        let payload = get_payload();

        {
            let mut output = vec!();
            let mut recv = send_to_response(&mut recv_data);

            recv.send_response(true, &mut output).unwrap();

            match decode(&mut output[..], true) {
                Ok((Packet::Ack(ack), _)) => {
                    assert_eq!(ack.packet_idx, 1);
                    assert_eq!(ack.pending_response, false);
                    assert_eq!(ack.response, true);
                },
                o => panic!("{:?}", o)
            }

            let response_ack = AckPacket {
                packet_idx: 1,
                pending_response: false,
                response: false,
                corrected_errors: 0,
                nack: false
            };

            match recv.on_packet(&(Packet::Ack(response_ack),0), &mut output) {
                Ok(RecvResult::Complete) => {},
                o => panic!("{:?}", o)
            }
        }

        let mut final_data = vec!();
        final_data.extend_from_slice(&payload[..]);
        final_data.extend_from_slice(&payload[..]);

        assert_eq!(final_data, recv_data);
    }

    #[test]
    fn test_timeout() {
        let mut data = vec!();
        let mut recv = RecvBlock::new(1000, true, &mut data);
        let mut output = vec!();

        match recv.tick(10, &mut output) {
            Ok(RecvResult::Status(_)) => {},
            o => panic!("{:?}", o)
        }

        match recv.tick(TIMEOUT_MS - 10, &mut output) {
            Err(RecvError::TimedOut) => {},
            o => panic!("{:?}", o)
        }

        assert_eq!(output.len(), 0);
    }

    #[test]
    fn test_resend_response() {
        let mut recv_data = vec!();
        let payload = get_payload();

        {
            let mut output = vec!();
            let mut recv = send_to_response(&mut recv_data);

            recv.send_response(true, &mut output).unwrap();

            match decode(&mut output[..], true) {
                Ok((Packet::Ack(ack), _)) => {
                    assert_eq!(ack.packet_idx, 1);
                    assert_eq!(ack.pending_response, false);
                    assert_eq!(ack.response, true);
                },
                o => panic!("{:?}", o)
            }

            output.clear();

            match recv.tick(PENDING_REPEAT_MS, &mut output) {
                Ok(RecvResult::Status(_)) => {},
                o => panic!("{:?}", o)
            }

            match decode(&mut output[..], true) {
                Ok((Packet::Ack(ref ack), _)) => {
                    assert_eq!(ack.packet_idx, 1);
                    assert_eq!(ack.corrected_errors, 0);
                    assert_eq!(ack.pending_response, false);
                    assert_eq!(ack.response, true); 
                },
                o => panic!("{:?}", o)
            }
        }
    }

    #[test]
    fn test_resend_timeout() {
        let mut recv_data = vec!();
        let payload = get_payload();

        {
            let mut output = vec!();
            let mut recv = send_to_response(&mut recv_data);

            recv.send_response(true, &mut output).unwrap();

            match decode(&mut output[..], true) {
                Ok((Packet::Ack(ack), _)) => {
                    assert_eq!(ack.packet_idx, 1);
                    assert_eq!(ack.pending_response, false);
                    assert_eq!(ack.response, true);
                },
                o => panic!("{:?}", o)
            }

            output.clear();

            match recv.tick(TIMEOUT_MS, &mut output) {
                Err(RecvError::TimedOut) => {},
                o => panic!("{:?}", o)
            }
        }
    }

    #[test]
    fn test_repeat_pending() {
        let mut recv_data = vec!();
        let payload = get_payload();

        {
            let mut output = vec!();
            let mut recv = send_to_response(&mut recv_data);

            match recv.tick(PENDING_REPEAT_MS, &mut output) {
                Ok(RecvResult::Status(_)) => {},
                o => panic!("{:?}", o)
            }

            match decode(&mut output[..], true) {
                Ok((Packet::Ack(ref ack), _)) => {
                    assert_eq!(ack.packet_idx, 1);
                    assert_eq!(ack.corrected_errors, 0);
                    assert_eq!(ack.pending_response, true);
                    assert_eq!(ack.nack, false); 
                },
                o => panic!("{:?}", o)
            }
        }
    }

    #[test]
    fn test_nack() {
        let mut recv_data = vec!();
        let payload = get_payload();

        {
            let mut recv = RecvBlock::new(1000, true, &mut recv_data);

            let mut output = vec!();
            let mut data_packet = vec!();

            let header = DataPacket {
                packet_idx: 1000,
                fec_bytes: 0,
                start_flag: true,
                end_flag: false
            };

            encode_data(header, true, 2048, &mut io::Cursor::new(&payload[..]), &mut data_packet).unwrap();

            //Zero out header + payload
            for i in 0..3+payload.len() {
                data_packet[i] = 0;
            }

            let decoded = decode(&mut data_packet, true).unwrap();

            match recv.on_packet(&decoded, &mut output) {
                Err(RecvError::Decode(PacketDecodeError::TooManyFECErrors)) => {},
                o => panic!("{:?}", o)
            }

            match decode(&mut output[..], true) {
                Ok((Packet::Ack(ack),_)) => {
                    assert_eq!(ack.packet_idx, 0);
                    assert_eq!(ack.corrected_errors, 3);
                    assert_eq!(ack.pending_response, false);
                    assert_eq!(ack.nack, true);
                },
                o => panic!("{:?}", o)
            }
        }

        assert_eq!(recv_data.len(), 0);
    }

    #[test]
    fn test_reack() {
        let mut recv_data = vec!();
        let payload = get_payload();

        {
            let mut recv = RecvBlock::new(1000, true, &mut recv_data);

            let mut output = vec!();
            let mut data_packet = vec!();
            let mut send_packet = &gen_data(1000, true, false, &payload[..], 5, &mut data_packet);

            match recv.on_packet(send_packet, &mut output) {
                Ok(RecvResult::Status(_)) => {},
                o => panic!("{:?}", o)
            }

            match decode(&mut output[..], true) {
                Ok((Packet::Ack(ack),_)) => {
                    assert_eq!(ack.packet_idx, 0);
                    assert_eq!(ack.corrected_errors, 5);
                    assert_eq!(ack.pending_response, false);
                    assert_eq!(ack.nack, false);
                },
                o => panic!("{:?}", o)
            }

            output.clear();

            match recv.on_packet(send_packet, &mut output) {
                Ok(RecvResult::Status(_)) => {},
                o => panic!("{:?}", o)
            }

            match decode(&mut output[..], true) {
                Ok((Packet::Ack(ack),_)) => {
                    assert_eq!(ack.packet_idx, 0);
                    assert_eq!(ack.corrected_errors, 0);
                    assert_eq!(ack.pending_response, false);
                    assert_eq!(ack.nack, false);
                },
                o => panic!("{:?}", o)
            }
        }

        assert_eq!(recv_data.len(), payload.len());
    }
}