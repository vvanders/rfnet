use packet::*;

use std::io;

pub struct SendBlock<R> where R: io::Read {
    data_reader: R,
    session_id: u16,
    packet_idx: u16,
    eof: bool,
    suspended: bool,
    last_send: usize,
    retry_attempts: usize,
    stats: SendStats,
    config: SendConfig,
    retry_config: RetryConfig,
    payload_scratch: Vec<u8>
}

pub struct SendStats {
    pub bytes_sent: usize,
    pub packets_sent: usize,
    pub missed_acks: usize,
    pub recv_bit_err: usize
}

struct SendConfig {
    fec: Option<u8>,
    link_width: usize,
}

pub struct RetryConfig {
    delay_ms: usize,
    bps: usize,
    bps_scale: f32,
    retry_attempts: usize
}

pub enum SendResult<'a> {
    Status(&'a SendStats),
    Suspended,
    CompleteNoResponse,
    CompleteResponse
}

pub enum SendError {
    Io(io::Error),
    Encode(PacketEncodeError),
    TimeOut
}

impl From<io::Error> for SendError {
    fn from(err: io::Error) -> SendError {
        SendError::Io(err)
    }
}

impl From<PacketEncodeError> for SendError {
    fn from(err: PacketEncodeError) -> SendError {
        SendError::Encode(err)
    }
}

impl<R> SendBlock<R> where R: io::Read {
    pub fn new(data_reader: R, session_id: u16, link_width: usize, fec: bool, start_ms: usize, retry_config: RetryConfig) -> SendBlock<R> {
        let fec = if fec {
            Some(0)
        } else {
            None
        };

        SendBlock {
            data_reader,
            session_id, 
            packet_idx: 0,
            eof: false,
            suspended: false,
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
                link_width
            },
            retry_config,
            payload_scratch: Vec::with_capacity(link_width)
        }
    }

    fn fill_payload(packet_read: &mut R, out: &mut Vec<u8>, link_width: usize) -> Result<bool, io::Error> {
        out.clear();
        let mut scratch: [u8; 256] = unsafe { ::std::mem::uninitialized() };

        loop {
            let remaining = ::std::cmp::min(link_width - out.len(), scratch.len());
            let read = packet_read.read(&mut scratch[..remaining])?;

            if read == 0 {
                return Ok(true)
            }

            if out.len() == link_width {
                return Ok(false)
            }
        }
    }

    fn send_data<W>(&mut self, packet_idx: u16, last_packet: &mut Vec<u8>, packet_writer: &mut W) -> Result<(), SendError> where W: io::Write {
        self.retry_attempts = 0;
        self.last_send = 0;

        self.eof = Self::fill_payload(&mut self.data_reader, &mut self.payload_scratch, self.config.link_width)?;
        let header = DataPacket {
            packet_idx: packet_idx,
            fec_bytes: self.config.fec.unwrap_or(0),
            start_flag: self.packet_idx == 0,
            end_flag: self.eof
        };

        let packet = Packet::Data(header, &self.payload_scratch[..]);

        last_packet.clear();
        encode(&packet, self.config.fec.is_some(), last_packet)?;

        packet_writer.write(&last_packet[..])?;

        self.stats.packets_sent += 1;

        Ok(())
    }

    fn resend<W>(&mut self, last_packet: &Vec<u8>, packet_writer: &mut W) -> Result<(), SendError> where W: io::Write {
        self.retry_attempts += 1;
        self.stats.missed_acks += 1;

        if self.retry_attempts > self.retry_config.retry_attempts {
            return Err(SendError::TimeOut)
        }

        packet_writer.write(&last_packet[..])?;

        self.stats.packets_sent += 1;

        Ok(())
    }

    pub fn send<W>(&mut self, packet: &Packet, last_packet: &mut Vec<u8>, packet_writer: &mut W) -> Result<SendResult, SendError> where W: io::Write {
        let packet_idx = self.session_id;
        self.send_data(packet_idx, last_packet, packet_writer)?;

        Ok(SendResult::Status(&self.stats))
    }

    pub fn on_packet<W>(&mut self, packet: &Packet, last_packet: &mut Vec<u8>, packet_writer: &mut W) -> Result<SendResult, SendError> where W: io::Write {
        match packet {
            &Packet::Control(ref ctrl) => {
                if let ControlType::NodeWaiting = ctrl.ctrl_type {
                    return Ok(SendResult::Suspended)
                }
            },
            &Packet::Ack(ref ack) => {
                if ack.packet_idx == self.packet_idx {
                    self.stats.recv_bit_err += ack.corrected_errors as usize;

                    if ack.nack {
                        self.resend(last_packet, packet_writer)?;
                    } else {
                        if self.eof {
                            if !ack.response {
                                return Ok(SendResult::CompleteNoResponse)
                            } else {
                                return Ok(SendResult::CompleteResponse)
                            }
                        } else {
                            self.packet_idx = self.packet_idx + 1;
                            self.stats.bytes_sent += last_packet.len();

                            let packet_idx = self.packet_idx;
                            self.send_data(packet_idx, last_packet, packet_writer)?;
                        }
                    }
                }
            },
            _ => {}
        }

        Ok(SendResult::Status(&self.stats))
    }

    pub fn tick<W>(&mut self, elapsed_ms: usize, last_packet: &Vec<u8>, packet_writer: &mut W) -> Result<SendResult, SendError> where W: io::Write {
        self.last_send = self.last_send + elapsed_ms;
        let next_retry = ((self.retry_config.bps * 8 * last_packet.len()) as f32 * self.retry_config.bps_scale) as usize + self.retry_config.delay_ms;

        if self.last_send > next_retry {
            self.resend(last_packet, packet_writer)?;
        }

        Ok(SendResult::Status(&self.stats))
    }
}