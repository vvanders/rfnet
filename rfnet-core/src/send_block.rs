use packet::*;
use framed::FramedWrite;

use std::io;

#[derive(Debug)]
pub struct SendBlock {
    packet_idx: u16,
    pending_response: bool,
    eof: bool,
    last_send: usize,
    retry_attempts: usize,
    stats: SendStats,
    config: SendConfig,
    retry_config: RetryConfig,
    last_packet: Vec<u8>
}

#[derive(Debug,Clone)]
pub struct SendStats {
    pub bytes_sent: usize,
    pub packets_sent: usize,
    pub missed_acks: usize,
    pub recv_bit_err: usize
}

#[derive(Debug)]
struct SendConfig {
    fec: Option<u8>,
    retry_enabled: bool,
    link_width: u16,
    data_size: usize
}

#[derive(Debug,Clone)]
pub struct RetryConfig {
    pub delay_ms: usize,
    pub bps: usize,
    pub bps_scale: f32,
    pub retry_attempts: usize
}

#[derive(Debug)]
pub enum SendResult {
    Active,
    PendingResponse,
    CompleteNoResponse,
    CompleteResponse
}

#[derive(Debug)]
pub enum SendError {
    Io(io::Error),
    TimeOut
}

impl From<io::Error> for SendError {
    fn from(err: io::Error) -> SendError {
        SendError::Io(err)
    }
}

impl RetryConfig {
    pub fn calc_delay(&self, send_bytes: usize, recv_bytes: usize) -> usize {
        ((self.bps as f32 / (8.0 * (send_bytes + recv_bytes) as f32) * 1000.0) * self.bps_scale) as usize + self.delay_ms
    }

    pub fn default(bps: usize) -> RetryConfig {
        RetryConfig {
            bps,
            delay_ms: 0,
            bps_scale: 1.5,
            retry_attempts: 5
        }
    }
}

impl SendBlock {
    pub fn new(data_size: usize, link_width: u16, fec: Option<u8>, retry_enabled: bool, retry_config: RetryConfig) -> SendBlock {
        SendBlock {
            packet_idx: 0,
            pending_response: false,
            eof: false,
            last_send: 0,
            retry_attempts: 0,
            stats: SendStats {
                bytes_sent: 0,
                packets_sent: 0,
                missed_acks: 0,
                recv_bit_err: 0
            },
            config: SendConfig {
                fec,
                retry_enabled,
                link_width,
                data_size
            },
            retry_config,
            last_packet: vec!()
        }
    }

    #[cfg(test)]
    pub fn get_stats(&self) -> &SendStats {
        &self.stats
    }

    fn send_data<W,R>(&mut self, packet_idx: u16, packet_writer: &mut W, data_reader: &mut R) -> io::Result<()>
            where W: FramedWrite, R: io::Read {
        self.retry_attempts = 0;
        self.last_send = 0;

        let bytes_per_packet = data_bytes_per_packet(self.config.fec, self.config.link_width);
        let eof = self.config.data_size - self.stats.bytes_sent <= bytes_per_packet;

        let header = DataPacket {
            packet_idx: packet_idx,
            fec_bytes: self.config.fec.unwrap_or(0),
            start_flag: self.packet_idx == 0,
            end_flag: eof
        };

        self.last_packet.clear();
        let (_bytes, data_written, _eof) = encode_data(header, self.config.fec.is_some(), self.config.link_width, data_reader, &mut self.last_packet)?;

        packet_writer.write_frame(&self.last_packet[..])?;

        self.stats.packets_sent += 1;
        self.stats.bytes_sent += data_written;

        if eof { 
            self.eof = eof;
        }

        debug!("Sending data packet {}", packet_idx);

        Ok(())
    }

    fn resend<W>(&mut self, packet_writer: &mut W) -> Result<(), SendError> where W: FramedWrite {
        self.retry_attempts += 1;
        self.stats.missed_acks += 1;
        self.last_send = 0;

        if self.retry_attempts > self.retry_config.retry_attempts {
            info!("Exceeded {} retry attempts, connection lost", self.retry_attempts);
            return Err(SendError::TimeOut)
        }

        if self.config.retry_enabled {
            packet_writer.write_frame(&self.last_packet[..])?;
            self.stats.packets_sent += 1;
        }

        Ok(())
    }

    fn send_ack<W>(&mut self, packet_writer: &mut W) -> Result<(), SendError> where W: FramedWrite {
        let ack = AckPacket {
            packet_idx: self.packet_idx,
            nack: false,
            pending_response: false,
            response: false,
            corrected_errors: 0
        };

        packet_writer.start_frame()?;
        encode(&Packet::Ack(ack), self.config.fec.is_some(), packet_writer)?;
        packet_writer.end_frame()?;

        Ok(())
    }

    pub fn send<W,R>(&mut self, packet_writer: &mut W, data_reader: &mut R) -> io::Result<SendResult> where W: FramedWrite, R: io::Read {
        self.send_data(0, packet_writer, data_reader)?;

        Ok(SendResult::Active)
    }

    pub fn on_packet<W,R>(&mut self, packet: &Packet, packet_writer: &mut W, data_reader: &mut R) -> Result<SendResult, SendError>
            where W: FramedWrite, R: io::Read {
        match packet {
            &Packet::Ack(ref ack) => {
                if ack.packet_idx == self.packet_idx {
                    self.stats.recv_bit_err += ack.corrected_errors as usize;
                    if ack.nack {
                        debug!("Heard NACK for {}, resending", self.packet_idx);
                        self.resend(packet_writer)?;
                    } else {
                        if ack.pending_response {
                            debug!("Endpoint is pending response");
                            self.pending_response = true;
                            return Ok(SendResult::PendingResponse)
                        } else if self.pending_response || self.eof {
                            self.send_ack(packet_writer)?;
                            if !ack.response {
                                info!("Transaction complete, no response");
                                return Ok(SendResult::CompleteNoResponse)
                            } else {
                                info!("Transaction complete, response pending");
                                return Ok(SendResult::CompleteResponse)
                            }
                        } else {
                            debug!("Heard ACK for {}", ack.packet_idx);
                            self.packet_idx = self.packet_idx + 1;

                            let packet_idx = self.packet_idx;
                            self.send_data(packet_idx, packet_writer, data_reader)?;
                        }
                    }
                } else {
                    debug!("Heard ack for bad packet index {} != {}", ack.packet_idx, self.packet_idx);
                }
            },
            _ => {}
        }

        Ok(SendResult::Active)
    }

    pub fn tick<W>(&mut self, elapsed_ms: usize, packet_writer: &mut W) -> Result<SendResult, SendError> where W: FramedWrite {
        let resend = {
            self.last_send = self.last_send + elapsed_ms;
            let next_retry = self.retry_config.calc_delay(self.last_packet.len(), calc_ack_bytes(self.config.fec.is_some()));

            self.last_send > next_retry && !self.pending_response
        };

        if resend {
            debug!("Missed ack, resending data packet");
            self.resend(packet_writer)?;
        }

        Ok(SendResult::Active)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_send() {
        let data = (0..16).collect::<Vec<u8>>();
        let retry = RetryConfig::default(1200);

        let mut output = vec!();

        let mut data_reader = io::Cursor::new(&data);
        let mut send = SendBlock::new(data.len(), 32, Some(0), true, retry);

        match send.send(&mut output, &mut data_reader).unwrap() {
            SendResult::Active => {
                let decoded = decode(&mut output[..], true).unwrap();
                if let &(Packet::Data(ref header, payload),_) = &decoded {
                    assert_eq!(header.packet_idx, 0);
                    assert_eq!(header.start_flag, true);
                    assert_eq!(header.end_flag, true);
                    assert_eq!(header.fec_bytes, 0);

                    let mut dpayload = vec!();
                    decode_data_blocks(header, payload, true, &mut dpayload).unwrap();
                    assert_eq!(dpayload, data);
                } else {
                    panic!("{:?}", decoded);
                }
            },
            o => panic!("{:?}", o)
        }
        output.clear();

        let mut ack = AckPacket {
            packet_idx: 0,
            nack: false,
            pending_response: true,
            response: false,
            corrected_errors: 5
        };

        match send.on_packet(&Packet::Ack(ack.clone()), &mut output, &mut data_reader).unwrap() {
            SendResult::PendingResponse => {},
            o => panic!("{:?}", o)
        }

        assert_eq!(send.get_stats().recv_bit_err, 5);

        ack.pending_response = false;
        ack.response = false;

        match send.on_packet(&Packet::Ack(ack), &mut output, &mut data_reader).unwrap() {
            SendResult::CompleteNoResponse => {},
            o => panic!("{:?}", o)
        }
    }

    #[test]
    fn test_resend() {
        let data = (0..16).collect::<Vec<u8>>();
        let retry = RetryConfig::default(1200);

        let mut output = vec!();

        let mut data_reader = io::Cursor::new(&data);
        let mut send = SendBlock::new(data.len(), 32, Some(0), true, retry.clone());
        send.send(&mut output, &mut data_reader).unwrap();

        let expected_resend = retry.calc_delay(output.len(), 9);

        for i in 0..5 {
            output.clear();

            send.tick(expected_resend * 2, &mut output).unwrap();

            assert_eq!(send.get_stats().missed_acks, i+1);
            match decode(&mut output[..], true).unwrap() {
                (Packet::Data(_header, _payload),_) => {},
                o => panic!("{:?}", o)
            }
        }

        match send.tick(expected_resend * 2, &mut output) {
            Err(SendError::TimeOut) => {},
            o => panic!("{:?}", o)
        }

        assert_eq!(send.get_stats().missed_acks, 6);
    }

    #[test]
    fn send_large() {
        let data = (0..4096).map(|v| v as u8).collect::<Vec<u8>>();
        let retry = RetryConfig::default(1200);

        let mut output = vec!();
        let mut final_data = vec!();

        let link_width = 32;
        let fec = Some(0);
        let bytes_per_packet = data_bytes_per_packet(fec, link_width);

        let mut data_reader = io::Cursor::new(&data);
        let mut send = SendBlock::new(data.len(), link_width, fec, true, retry);

        match send.send(&mut output, &mut data_reader).unwrap() {
            SendResult::Active => {},
            o => panic!("{:?}", o)
        }

        fn ack_packet<'a>(idx: u16, err: u16, pending: bool) -> Packet<'a> {
            Packet::Ack( AckPacket {
                packet_idx: idx,
                nack: false,
                pending_response: pending,
                response: false,
                corrected_errors: err
            })
        }

        let remaining_full = data.len() / bytes_per_packet+1;

        for i in 0..remaining_full {
            assert!(output.len() <= link_width as usize, "{} <= {}", output.len(), link_width);

            assert_eq!(send.get_stats().packets_sent, i+1);

            let is_end = {
                let decoded = decode(&mut output[..], true).unwrap();
                if let &(Packet::Data(ref header, payload),_) = &decoded {
                    if i == 0 {
                        assert_eq!(header.packet_idx, 0);
                        assert_eq!(header.start_flag, true);
                    } else if i == remaining_full-1 {
                        assert_eq!(header.packet_idx, i as u16);
                        assert_eq!(header.end_flag, true);
                    } else {
                        assert_eq!(header.packet_idx, i as u16);
                    }

                    assert_eq!(header.fec_bytes, 0);

                    let mut dpayload = vec!();
                    decode_data_blocks(header, payload, true, &mut dpayload).unwrap();

                    final_data.extend_from_slice(&dpayload[..]);

                    header.end_flag
                } else {
                    panic!("{:?}", decoded);
                }
            };

            if !is_end {
                assert_eq!(send.get_stats().bytes_sent, bytes_per_packet * (i+1));
            } else {
                assert_eq!(send.get_stats().bytes_sent, data.len());
            }

            output.clear();

            {
                let result = send.on_packet(&ack_packet(i as u16, 5, is_end), &mut output, &mut data_reader).unwrap();
                if is_end {
                    match result {
                        SendResult::PendingResponse => {},
                        o => panic!("{:?}", o)
                    }
                } else {
                    match result {
                        SendResult::Active => {},
                        o => panic!("{:?}", o)
                    }
                }
            }
            assert_eq!(send.get_stats().recv_bit_err, 5 * (i+1));
        }

        assert_eq!(data, final_data);
        let final_ack = Packet::Ack(AckPacket {
                packet_idx: remaining_full as u16 - 1,
                nack: false,
                pending_response: false,
                response: false,
                corrected_errors: 0
            });

        match send.on_packet(&final_ack, &mut output, &mut data_reader).unwrap() {
            SendResult::CompleteNoResponse => {},
            o => panic!("{:?}", o)
        }
    }

    #[test]
    fn test_no_resend() {
        let data = (0..16).collect::<Vec<u8>>();
        let mut output = vec!();

        let mut data_reader = io::Cursor::new(&data);
        let mut send = SendBlock::new(data.len(), 32, Some(0), false, RetryConfig::default(1200));
        send.send(&mut output, &mut data_reader).unwrap();
        output.clear();
        send.tick(10_000, &mut output).unwrap();

        assert_eq!(output.len(), 0);
    }
}