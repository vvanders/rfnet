use std::io::{Cursor, Write, Read};

use reed_solomon;
use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian};

#[derive(Debug, PartialEq, Clone)]
pub enum Packet<'a> {
    Broadcast(BroadcastPacket<'a>),
    Control(ControlPacket<'a>),
    Data(DataPacket, &'a [u8]),
    Ack(AckPacket)
}

#[derive(Debug, PartialEq, Clone)]
pub struct BroadcastPacket<'a> {
    pub fec_enabled: bool,
    pub retry_enabled: bool,
    pub major_ver: u8,
    pub minor_ver: u8,
    pub link_width: u16,
    pub callsign: &'a [u8]
}

#[derive(Debug, PartialEq, Clone)]
pub enum ControlType {
    Reserved,
    LinkRequest,
    LinkOpened,
    LinkClose,
    LinkClear,
    NodeWaiting,
    Notification
}

#[derive(Debug, PartialEq, Clone)]
pub struct ControlPacket<'a> {
    pub ctrl_type: ControlType,
    pub session_id: u16,
    pub source_callsign: &'a [u8],
    pub dest_callsign: &'a [u8]
}

#[derive(Debug, PartialEq, Clone)]
pub struct DataPacket {
    pub packet_idx: u16,
    pub fec_bytes: u8,
    pub start_flag: bool,
    pub end_flag: bool
}

#[derive(Debug, PartialEq, Clone)]
pub struct AckPacket {
    pub packet_idx: u16,
    pub nack: bool,
    pub response: bool,
    pub pending_response: bool,
    pub corrected_errors: u16
}

#[derive(Debug)]
pub enum PacketDecodeError {
    TooManyFECErrors,
    BadFormat
}

#[derive(Debug)]
pub enum DataDecodeError {
    Io(::std::io::Error),
    TooManyFECErrors
}

#[derive(Debug)]
pub enum PacketEncodeError {
    Io(::std::io::Error),
    BadFormat
}

impl From<::std::io::Error> for PacketEncodeError {
    fn from(err: ::std::io::Error) -> Self {
        PacketEncodeError::Io(err)
    }
}

pub fn decode<'a>(data: &'a mut [u8], fec_enabled: bool) -> Result<(Packet<'a>, usize), PacketDecodeError> {
    if fec_enabled {
        decode_fec(data)
    } else {
        decode_corrected(data).map(|p| (p,0))
    }
}

pub fn decode_data_blocks<T>(header: &DataPacket, data: &[u8], fec: bool, out: &mut T) -> Result<usize, DataDecodeError> where T: Write {
    if !fec {
        out.write_all(data).map_err(|e| DataDecodeError::Io(e))?;
        return Ok(0)
    }

    let mut acc_err = 0;

    let decoder = reed_solomon::Decoder::new(get_fec_bytes(header.fec_bytes));
    for block in data.chunks(BLOCK_SIZE) {
        let (decoded, err) = decoder.correct_err_count(block, None).map_err(|_| DataDecodeError::TooManyFECErrors)?;
        out.write_all(decoded.data()).map_err(|e| DataDecodeError::Io(e))?;

        acc_err += err;
    }

    Ok(acc_err)
}

pub fn encode<'a,T>(packet: &Packet<'a>, fec_enabled: bool, writer: &mut T) -> Result<usize, PacketEncodeError> where T: Write {
    if fec_enabled {
        encode_fec(packet, writer)
    } else {
        encode_non_fec(packet, writer)
    }
}

pub fn encode_data<W,R>(header: DataPacket, fec: bool, link_width: usize, data: &mut R, writer: &mut W) -> Result<(usize, usize, bool), PacketEncodeError> where W: Write, R: Read {
    let (header_size, fec_bytes) = if fec {
        (3 + FEC_CRC_BYTES, get_fec_bytes(header.fec_bytes))
    } else {
        (3, 0)
    };

    let data_size = link_width - header_size;

    let block_count = if data_size % 256 != 0 {
        data_size / 256 + 1
    } else {
        data_size / 256
    };

    let mut scratch_header: [u8; 3] = unsafe { ::std::mem::uninitialized() };
    {
        let mut cursor = Cursor::new(&mut scratch_header[..]);
        encode_inner(&Packet::Data(header, &[]), &mut cursor)?;
    }

    writer.write_all(&scratch_header)?;

    let encoder = reed_solomon::Encoder::new(fec_bytes);
    let mut data_written = 0;
    let mut payload_written = 0;
    let mut eof = false;

    for _ in 0..block_count {
        let mut block: [u8; BLOCK_SIZE] = unsafe { ::std::mem::uninitialized() };
        let data_block_size = ::std::cmp::min(block.len(), data_size) - fec_bytes;

        let mut block_read = 0;
        loop {
            let read = data.read(&mut block[block_read..data_block_size])?;

            block_read += read;

            if read == 0 {
                eof = true;
                break;
            } else if block_read == data_block_size {
                break;
            }
        }

        if fec {
            if block_read != 0 {
                let encoded = encoder.encode(&block[..block_read]);
                writer.write(&**encoded)?;
                payload_written += encoded.len();
            }
        } else {
            writer.write(&block[..block_read])?;
            payload_written += block_read;
        }

        data_written += block_read;

        if eof {
            break;
        }
    }

    //Append our header FEC to the end of the packet so we can match what
    //we do for non-data packets.
    if fec {
        let encoder = reed_solomon::Encoder::new(FEC_CRC_BYTES);
        let encoded = encoder.encode(&scratch_header[..]);

        writer.write_all(encoded.ecc())?;
    }

    Ok((header_size + payload_written, data_written, eof))
}

pub fn data_bytes_per_packet(fec: Option<u8>, link_width: usize) -> usize {
    match fec {
        Some(fec_bytes) => link_width - 3 - FEC_CRC_BYTES - get_fec_bytes(fec_bytes),
        None => link_width - 3
    }
}

const BROADCAST_MASK: u8 = 0b0000_0000;
const DATA_MASK: u8 = 0b0100_0000;
const ACK_MASK: u8 = 0b1000_0000;
const CTRL_MASK: u8 = 0b1100_0000;
const PACKET_TYPE_MASK: u8 = 0b1100_0000;

//Broadcast
const FEC_ENABLED_MASK: u8 = 0b0010_0000;
const RETRY_ENABLED_MASK: u8 = 0b0001_0000;

//Data
const START_FLAG_MASK: u8 = 0b1000_0000;
const END_FLAG_MASK: u8 = 0b0100_0000;
const FEC_BYTES_MASK: u8 = 0b0011_1111;
const BLOCK_SIZE: usize = 255;

//Ack
const RESPONSE_MASK: u8 = 0b1000_0000;
const NEGATIVE_ACK_MASK: u8 = 0b0100_0000;
const PENDING_RESPONSE_MASK: u8 = 0b0010_0000;
const CORRECTED_ERR_MASK: u8 = 0b0000_1111;

//Ctrl
const CONTROL_TYPE_MASK: u8 = 0b0000_0111;

//Shared
const FEC_CRC_BYTES: usize = 6;

fn get_fec_bytes(fec_count: u8) -> usize {
    (fec_count+1) as usize * 2
}

fn encode_fec<'a,T>(packet: &Packet<'a>, writer: &mut T) -> Result<usize, PacketEncodeError> where T: Write {
    match packet {
        &Packet::Data(ref header, ref content) => {
            panic!("Data should be encoded with encode_data");
        },
        _ => {
            //Max size of an inner frame at 2x FEC with 255b frame
            let mut scratch: [u8; 85] = unsafe { ::std::mem::uninitialized() };
            let len = {
                let mut cursor = Cursor::new(&mut scratch[..]);
                encode_inner(packet, &mut cursor)?;

                cursor.position() as usize
            };

            let encoder = reed_solomon::Encoder::new(len * 2);
            let encoded = encoder.encode(&scratch[..len]);

            writer.write_all(&**encoded)?;

            //Since we have disparate sizes for our FEC types(data vs other)
            //we need to emulate the same way we treat the first 3 bytes in data packets so
            //we can cleanly differentiate between our types. Otherwise stray
            //bytes in callsign/etc can trigger false positives and "correct"
            //a packet incorrectly.
            let crc_encoder = reed_solomon::Encoder::new(FEC_CRC_BYTES);
            let crc_encoded = crc_encoder.encode(&scratch[..3]);

            writer.write_all(crc_encoded.ecc())?;

            Ok(encoded.len() + crc_encoded.ecc().len())
        }
    }
}

fn encode_non_fec<'a,T>(packet: &Packet<'a>, writer: &mut T) -> Result<usize, PacketEncodeError> where T: Write {
    match packet {
        &Packet::Data(ref _header, ref payload) => {
            let len = encode_inner(packet, writer)?;
            writer.write_all(payload)?;

            Ok(len+payload.len())
        },
        _ => encode_inner(packet, writer)
    }
}

fn encode_inner<'a,T>(packet: &Packet<'a>, writer: &mut T) -> Result<usize, PacketEncodeError> where T: Write {
    match packet {
        &Packet::Ack(ref header) => {
            let mut seq_id = header.packet_idx & ((!DATA_MASK as u16) << 8 | 0xFF);
            seq_id = seq_id | ((ACK_MASK as u16) << 8);
            
            let mut errs_flags: u8 = 0;
            if header.nack {
                errs_flags = errs_flags | NEGATIVE_ACK_MASK;
            }

            if header.response {
                errs_flags = errs_flags | RESPONSE_MASK;
            }

            if header.pending_response {
                errs_flags = errs_flags | PENDING_RESPONSE_MASK;
            }

            let mut err = header.corrected_errors & ((CORRECTED_ERR_MASK as u16) << 8 | 0xFF);
            err = err | ((errs_flags as u16) << 8);

            writer.write_u16::<BigEndian>(seq_id)?;
            writer.write_u16::<BigEndian>(err)?;

            Ok(4)
        },
        &Packet::Broadcast(ref header) => {
            let mut packet_type = BROADCAST_MASK;
            if header.fec_enabled {
                packet_type = packet_type | FEC_ENABLED_MASK;
            }

            if header.retry_enabled {
                packet_type = packet_type | RETRY_ENABLED_MASK;
            }

            writer.write_u8(packet_type)?;
            writer.write_u8(header.major_ver)?;
            writer.write_u8(header.minor_ver)?;
            writer.write_u16::<BigEndian>(header.link_width)?;
            writer.write_all(header.callsign)?;

            Ok(5+header.callsign.len())
        },
        &Packet::Control(ref header) => {
            let ctrl_type = match header.ctrl_type {
                ControlType::Reserved => 0,
                ControlType::LinkRequest => 1,
                ControlType::LinkOpened => 2,
                ControlType::LinkClose => 3,
                ControlType::LinkClear => 4,
                ControlType::NodeWaiting => 5,
                ControlType::Notification => 6
            };
            let mut packet_type = CTRL_MASK | (CONTROL_TYPE_MASK & ctrl_type);

            writer.write_u8(packet_type)?;
            writer.write_u16::<BigEndian>(header.session_id)?;
            writer.write_all(header.source_callsign)?;
            writer.write_u8(0)?;
            writer.write_all(header.dest_callsign)?;

            Ok(4 + header.source_callsign.len() + header.dest_callsign.len())
        },
        &Packet::Data(ref header, ref _content) => {
            let mut packet_idx = header.packet_idx & ((!PACKET_TYPE_MASK as u16) << 8 | 0xFF);
            packet_idx = packet_idx | ((DATA_MASK as u16) << 8);

            let mut flag_bytes = header.fec_bytes & FEC_BYTES_MASK;
            if header.start_flag {
                flag_bytes = flag_bytes | START_FLAG_MASK;
            }

            if header.end_flag {
                flag_bytes = flag_bytes | END_FLAG_MASK;
            }

            writer.write_u16::<BigEndian>(packet_idx)?;
            writer.write_u8(flag_bytes)?;

            Ok(3)
        }
    }
}

fn decode_fec<'a>(data: &'a mut [u8]) -> Result<(Packet<'a>, usize), PacketDecodeError> {
    if data.len() < 9 {
        trace!("Failed to decode packet, missing data header");
        return Err(PacketDecodeError::BadFormat)
    }

    //See what type of packet we have
    //Header is 1 + 2 bytes split between start and end of the packet
    let fec_start = data.len() - FEC_CRC_BYTES;
    let mut header: [u8; 9] = unsafe { ::std::mem::uninitialized() };
    header[..3].copy_from_slice(&data[..3]);
    header[3..].copy_from_slice(&data[fec_start..]);

    let header_decoder = reed_solomon::Decoder::new(6);
    let header_decoded = header_decoder.correct_err_count(&header, None).map_err(|_| PacketDecodeError::TooManyFECErrors);

    if let Ok((header, errs)) = header_decoded {
        if header[0] & PACKET_TYPE_MASK == DATA_MASK {
            let data_end = data.len() - FEC_CRC_BYTES;
            return decode_data(&header[..3], &data[3..data_end]).map(|p| (p,errs))
        }
    } else {
        trace!("Failed to decoded header, try as non-data packet");
    }

    //Possibly non-data packet
    let potential_data_bytes = data.len() - FEC_CRC_BYTES;
    if potential_data_bytes % 3 == 0 {
        let data_len = potential_data_bytes / 3;

        let decoder = reed_solomon::Decoder::new(potential_data_bytes - data_len);
        let decoded = decoder.correct_err_count(&data[..potential_data_bytes], None).map_err(|_| PacketDecodeError::TooManyFECErrors);

        //We have non-data packet
        if let Ok((fixed, errs)) = decoded {
            if errs > 0 {
                data[..potential_data_bytes].copy_from_slice(&**fixed);
            }

            return decode_corrected(&data[..data_len]).map(|p| (p,errs))
        }
    }

    Err(PacketDecodeError::TooManyFECErrors)
}

fn decode_corrected<'a>(data: &'a [u8]) -> Result<Packet<'a>, PacketDecodeError> {
    match data[0] & PACKET_TYPE_MASK {
        BROADCAST_MASK => decode_broadcast(data),
        DATA_MASK => decode_data(&data[..3], &data[3..]),
        ACK_MASK => decode_ack(data),
        CTRL_MASK => decode_ctrl(data),
        _ => Err(PacketDecodeError::BadFormat)
    }
}

fn decode_broadcast<'a>(data: &'a [u8]) -> Result<Packet<'a>, PacketDecodeError> {
    if data.len() < 6 {
        return Err(PacketDecodeError::BadFormat)
    }

    let fec_enabled = data[0] & FEC_ENABLED_MASK == FEC_ENABLED_MASK;
    let retry_enabled = data[0] & RETRY_ENABLED_MASK == RETRY_ENABLED_MASK;

    let major_ver = data[1];
    let minor_ver = data[2];

    let link_width = Cursor::new(&data[3..5]).read_u16::<BigEndian>().map_err(|_| PacketDecodeError::BadFormat)?;

    let callsign = &data[5..];

    Ok(Packet::Broadcast(BroadcastPacket {
        fec_enabled,
        retry_enabled,
        major_ver,
        minor_ver,
        link_width,
        callsign
    }))
}

fn decode_sequence_id(id: &[u8]) -> Result<u16, PacketDecodeError> {
    if id.len() != 2 {
        return Err(PacketDecodeError::BadFormat)
    }

    //The header also contains packet type so strip that before we decode our id
    let mut seq_id: [u8; 2] = [0; 2];
    seq_id.copy_from_slice(&id);
    seq_id[0] = seq_id[0] & !PACKET_TYPE_MASK;

    Cursor::new(&seq_id).read_u16::<BigEndian>().map_err(|_| PacketDecodeError::BadFormat)
}

fn decode_data<'a>(header: &[u8], payload: &'a [u8]) -> Result<Packet<'a>, PacketDecodeError> {
    if header.len() != 3 {
        return Err(PacketDecodeError::BadFormat)
    }

    let packet_idx = decode_sequence_id(&header[0..2])?;
    let start_flag = header[2] & START_FLAG_MASK == START_FLAG_MASK;
    let end_flag = header[2] & END_FLAG_MASK == END_FLAG_MASK;

    let fec_bytes = header[2] & FEC_BYTES_MASK;

    Ok(Packet::Data(DataPacket {
        packet_idx,
        fec_bytes,
        start_flag,
        end_flag
    }, payload))
}

fn decode_ack<'a>(data: &'a [u8]) -> Result<Packet<'a>, PacketDecodeError> {
    if data.len() != 4 {
        return Err(PacketDecodeError::BadFormat)
    }

    let packet_idx = decode_sequence_id(&data[0..2])?;
    let nack = data[2] & NEGATIVE_ACK_MASK == NEGATIVE_ACK_MASK;
    let response = data[2] & RESPONSE_MASK == RESPONSE_MASK;
    let pending_response = data[2] & PENDING_RESPONSE_MASK == PENDING_RESPONSE_MASK;

    let mut err: [u8; 2] = [0; 2];
    err.copy_from_slice(&data[2..4]);
    err[0] = err[0] & CORRECTED_ERR_MASK;

    let corrected_errors = Cursor::new(&err).read_u16::<BigEndian>().map_err(|_| PacketDecodeError::BadFormat)?;

    Ok(Packet::Ack(AckPacket {
        packet_idx,
        nack,
        response,
        pending_response,
        corrected_errors
    }))
}

fn decode_ctrl<'a>(data: &'a [u8]) -> Result<Packet<'a>, PacketDecodeError> {
    if data.len() < 6 {
        return Err(PacketDecodeError::BadFormat)
    }

    let ctrl_type = match data[0] & CONTROL_TYPE_MASK {
        0 => ControlType::Reserved,
        1 => ControlType::LinkRequest,
        2 => ControlType::LinkOpened,
        3 => ControlType::LinkClose,
        4 => ControlType::LinkClear,
        5 => ControlType::NodeWaiting,
        6 => ControlType::Notification,
        _ => return Err(PacketDecodeError::BadFormat)
    };

    let session_id = Cursor::new(&data[1..3]).read_u16::<BigEndian>().map_err(|_| PacketDecodeError::BadFormat)?;
    
    let callsign_block = &data[3..];

    //We separate callsigns by the null terminator
    let callsign_split = callsign_block.iter().position(|v| *v == 0);

    let (source_callsign, dest_callsign) = if let Some(idx) = callsign_split {
        callsign_block.split_at(idx)
    } else {
        return Err(PacketDecodeError::BadFormat)
    };

    let dest_callsign = &dest_callsign[1..];

    Ok(Packet::Control(ControlPacket {
        ctrl_type,
        session_id,
        source_callsign,
        dest_callsign
    }))
}

#[cfg(test)]
mod test {
    use super::*;

    fn verify_packet(packet: Packet, max_err: usize) {
        verify_packet_internal(packet.clone(), true, max_err);
        verify_packet_internal(packet.clone(), false, max_err);
    }

    fn verify_packet_internal(packet: Packet, fec: bool, mut max_err: usize) {
        let mut scratch = vec!();

        let written = if let &Packet::Data(ref header, _) = &packet {
            encode_data(header.clone(), fec, 4096, &mut Cursor::new(&[]), &mut scratch).unwrap().0
        } else {
            encode(&packet.clone(), fec, &mut scratch).unwrap()
        };

        assert_eq!(written, scratch.len());

        //FEC tests takes considerable time so only test in release
        if cfg!(debug_assertions) {
            max_err = 0;
        }

        let is_data = if let &Packet::Data(_,_) = &packet {
            true
        } else {
            false
        };

        if fec {
            for e in 0..max_err {
                let mut stride = if !is_data {
                    scratch.len() - e - FEC_CRC_BYTES
                } else {
                    3 - e
                };

                for i in 0..stride {
                    let mut corrupt = scratch.clone();

                    for j in 0..e {
                        corrupt[j+i] = !corrupt[j+i];
                    }

                    verify_decode(corrupt, packet.clone(), fec, e);
                }
            }
        } else {
            verify_decode(scratch, packet, fec, 0);
        }
    }

    fn verify_decode(mut data: Vec<u8>, packet: Packet, fec: bool, errs: usize) {
        match decode(&mut data[..], fec) {
            Ok((Packet::Data(header, _),e)) => {
                if let Packet::Data(match_header,_) = packet {
                    assert_eq!(header, match_header);
                    assert_eq!(e, errs);
                } else {
                    panic!();
                }
            },
            Ok((p,e)) => {
                assert_eq!(p, packet);
                assert_eq!(e, errs);
            },
            Err(e) => panic!("{:?}", e)
        };
    }

    #[test]
    fn test_ack() {
        for i in 0..16384 {
            let packet = Packet::Ack(AckPacket {
                packet_idx: i,
                nack: true,
                response: true,
                pending_response: true,
                corrected_errors: 3
            });

            verify_packet(packet, 3);
        }

        for i in 0..520 {
            let packet = Packet::Ack(AckPacket {
                packet_idx: 16000,
                nack: true,
                response: true,
                pending_response: true,
                corrected_errors: i
            });

            verify_packet(packet, 3);
        }

        {
            let packet = Packet::Ack(AckPacket {
                packet_idx: 16000,
                nack: false,
                response: false,
                pending_response: false,
                corrected_errors: 3
            });

            verify_packet(packet, 3);
        }

        {
            let packet = Packet::Ack(AckPacket {
                packet_idx: 16000,
                nack: true,
                response: false,
                pending_response: false,
                corrected_errors: 3
            });

            verify_packet(packet, 3);
        }

        {
            let packet = Packet::Ack(AckPacket {
                packet_idx: 16000,
                nack: false,
                response: true,
                pending_response: false,
                corrected_errors: 3
            });

            verify_packet(packet, 3);
        }

        {
            let packet = Packet::Ack(AckPacket {
                packet_idx: 16000,
                nack: false,
                response: false,
                pending_response: true,
                corrected_errors: 3
            });

            verify_packet(packet, 3);
        }
    }

    #[test]
    fn test_ctrl() {
        let control_types = [
                ControlType::LinkRequest,
                ControlType::LinkOpened,
                ControlType::LinkClose,
                ControlType::LinkClear,
                ControlType::NodeWaiting,
                ControlType::Notification
            ];

        let scratch_callsign = "ki7est";

        for ctype in control_types.iter().cloned() {
            verify_packet(Packet::Control(ControlPacket {
                ctrl_type: ctype,
                session_id: 1000,
                source_callsign: scratch_callsign.as_bytes(),
                dest_callsign: scratch_callsign.as_bytes()
            }), 3 + scratch_callsign.as_bytes().len() * 2)
        }

        for i in 1..10 {
            for j in 1..10 {
                let source_callsign = (1..(i+1)).collect::<Vec<u8>>();
                let dest_callsign = (1..(j+1)).collect::<Vec<u8>>();

                verify_packet(Packet::Control(ControlPacket {
                    ctrl_type: ControlType::LinkRequest,
                    session_id: 1000,
                    source_callsign: &source_callsign[..],
                    dest_callsign: &dest_callsign[..]
                }), (3 + i + j) as usize);
            }
        }

        for i in 0..16384 {
            verify_packet(Packet::Control(ControlPacket {
                ctrl_type: ControlType::LinkRequest,
                session_id: i,
                source_callsign: &[1],
                dest_callsign: &[1]
            }), 3 + 2)
        }
    }

    #[test]
    fn test_broadcast() {
        let scratch_callsign = "ki7est";

        let base_packet = BroadcastPacket {
            fec_enabled: false,
            retry_enabled: false,
            major_ver: 0,
            minor_ver: 0,
            link_width: 0,
            callsign: &scratch_callsign.as_bytes()
        };

        {
            let packet = base_packet.clone();
            verify_packet(Packet::Broadcast(packet), 5 + scratch_callsign.len());
        }

        {
            let mut packet = base_packet.clone();
            packet.fec_enabled = true;
            verify_packet(Packet::Broadcast(packet), 5 + scratch_callsign.len());
        }

        {
            let mut packet = base_packet.clone();
            packet.retry_enabled = true;
            verify_packet(Packet::Broadcast(packet), 5 + scratch_callsign.len());
        }

        for i in 0..255 {
            let mut packet = base_packet.clone();
            packet.major_ver = i;
            verify_packet(Packet::Broadcast(packet), 5 + scratch_callsign.len());
        }

        for i in 0..255 {
            let mut packet = base_packet.clone();
            packet.minor_ver = i;
            verify_packet(Packet::Broadcast(packet), 5 + scratch_callsign.len());
        }

        for i in 0..16384 {
            let mut packet = base_packet.clone();
            packet.link_width = i;
            verify_packet(Packet::Broadcast(packet), 5 + scratch_callsign.len());
        }
    }

    #[test]
    fn test_data() {
        //Only tests data header, payload test in data_payload
        let base_packet = DataPacket {
            packet_idx: 0,
            fec_bytes: 0,
            start_flag: false,
            end_flag: false
        };

        for i in 0..16384 {
            let mut packet = base_packet.clone();
            packet.packet_idx = i;
            verify_packet(Packet::Data(packet, &[]), 3);
        }

        for i in 0..64 {
            let mut packet = base_packet.clone();
            packet.fec_bytes = i;
            verify_packet(Packet::Data(packet, &[]), 3);
        }

        {
            let mut packet = base_packet.clone();
            packet.start_flag = true;
            verify_packet(Packet::Data(packet, &[]), 3);
        }

        {
            let mut packet = base_packet.clone();
            packet.end_flag = true;
            verify_packet(Packet::Data(packet, &[]), 3);
        }
    }

    #[test]
    fn test_data_payload() {
        let base_packet = DataPacket {
            packet_idx: 0,
            fec_bytes: 0,
            start_flag: false,
            end_flag: false
        };

        for i in 1..20 {
            for f in 0..64 {
                let mut packet = base_packet.clone();
                packet.fec_bytes = f;
                test_data_payload_size(packet, i*100);
            }
        }
    }

    fn test_data_payload_size(packet: DataPacket, size: usize) {
        let data = (0..size).map(|v| (v & 0xFF) as u8).collect::<Vec<u8>>();

        verify_data_packet(&packet, &data[..], true, size);
        verify_data_packet(&packet, &data[..], false, size);
    }

    fn verify_data_packet(packet: &DataPacket, data: &[u8], fec: bool, size: usize) {
        let mut scratch = vec!();

        let (written, data_written, eof) = encode_data(packet.clone(), fec, 4096, &mut Cursor::new(data), &mut scratch).unwrap();
        assert_eq!(written, scratch.len());

        //FEC tests takes considerable time so only test in release
        let mut max_err = get_fec_bytes(packet.fec_bytes) / 2;
        if cfg!(debug_assertions) {
            max_err = 0;
        }

        if fec && scratch.len() > 9 {
            for e in 0..max_err {
                let mut stride = scratch.len() - e - FEC_CRC_BYTES - 3;

                for i in 3..stride {
                    if i-3 % 100 != 0 {
                        continue
                    }

                    let mut corrupt = scratch.clone();

                    for j in 0..e {
                        corrupt[j+i+3] = !corrupt[j+i+3];
                    }

                    verify_data_packet_decode(&mut corrupt[..], data, fec, e)
                }
            }
        } else {
            verify_data_packet_decode(&mut scratch[..], data, fec, 0)
        }
    }

    fn verify_data_packet_decode(encoded: &mut [u8], data: &[u8], fec: bool, fec_errors: usize) {
        let (header, payload) = match decode(encoded, fec) {
            Ok((Packet::Data(header, payload), errs)) => {
                assert_eq!(errs,0);
                (header, payload)
            },
            o => panic!("{:?}", o)
        };

        verify_data_blocks(&header, payload, data, fec, fec_errors);
    }

    fn verify_data_blocks(packet: &DataPacket, payload: &[u8], data: &[u8], fec:bool, err: usize) {
        let mut decoded = vec!();

        let fec_err = decode_data_blocks(packet, payload, fec, &mut decoded).unwrap();

        assert_eq!(fec_err, err);
        assert_eq!(&decoded[..], data);
    }

    #[test]
    fn test_data_payload_fec() {
        let base_packet = DataPacket {
            packet_idx: 0,
            fec_bytes: 0,
            start_flag: false,
            end_flag: false
        };

        for f in 0..64 {
            let mut packet = base_packet.clone();
            packet.fec_bytes = f;
            let data = (0..512).map(|v| v as u8).collect::<Vec<u8>>();

            let mut scratch = vec!();
            let (written, data_written, eof) = encode_data(packet.clone(), true, BLOCK_SIZE, &mut Cursor::new(data), &mut scratch).unwrap();

            assert_eq!(data_written, BLOCK_SIZE - get_fec_bytes(f) - 9);
            assert_eq!(written, BLOCK_SIZE);
            assert_eq!(eof, false);
        }
    }

    #[test]
    fn test_data_payload_eof() {
        let base_packet = DataPacket {
            packet_idx: 0,
            fec_bytes: 0,
            start_flag: false,
            end_flag: false
        };

        let mut packet = base_packet.clone();
        packet.fec_bytes = 0;
        let data = (0..300).map(|v| v as u8).collect::<Vec<u8>>();

        let mut scratch = vec!();
        let mut read = Cursor::new(data);
        {
            let (written, data_written, eof) = encode_data(packet.clone(), true, 256, &mut read, &mut scratch).unwrap();

            assert_eq!(data_written, 256 - get_fec_bytes(packet.fec_bytes) - 9);
            assert_eq!(written, 256);
            assert_eq!(eof, false);
        }

        {
            let (written, data_written, eof) = encode_data(packet.clone(), true, 256, &mut read, &mut scratch).unwrap();

            let data_size = 300 - (256 - get_fec_bytes(packet.fec_bytes) - 9);
            assert_eq!(data_written, data_size);
            assert_eq!(written, data_size + 2 + 9);
            assert_eq!(eof, true);
        }
    }
}