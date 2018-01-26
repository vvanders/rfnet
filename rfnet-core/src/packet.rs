use std::io::{Cursor, Write};

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
    pub packet_id: u16,
    pub nack: bool,
    pub no_response: bool,
    pub corrected_errors: u8
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

pub fn decode_data_blocks<T>(header: &DataPacket, data: &[u8], out: &mut T) -> Result<usize, DataDecodeError> where T: Write {
    let mut acc_err = 0;

    let decoder = reed_solomon::Decoder::new(get_fec_bytes(header.fec_bytes));
    for block in data.chunks(256) {
        let (decoded, err) = decoder.correct_err_count(block, None).map_err(|_| DataDecodeError::TooManyFECErrors)?;
        out.write(decoded.data()).map_err(|e| DataDecodeError::Io(e))?;

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

//Ack
const NO_RESPONSE_MASK: u8 = 0b1000_0000;
const NEGATIVE_ACK_MASK: u8 = 0b0100_0000;
const CORRECTED_ERR_MASK: u8 = 0b0011_1111;

//Ctrl
const CONTROL_TYPE_MASK: u8 = 0b0011_0000;

fn get_fec_bytes(fec_count: u8) -> usize {
    (fec_count+1) as usize * 2
}

fn encode_fec<'a,T>(packet: &Packet<'a>, writer: &mut T) -> Result<usize, PacketEncodeError> where T: Write {
    match packet {
        &Packet::Data(ref header, ref content) => {
            let mut scratch_header: [u8; 3] = unsafe { ::std::mem::uninitialized() };
            {
                let mut cursor = Cursor::new(&mut scratch_header[..]);
                encode_inner(packet, &mut cursor)?;
            }
            
            let encoder = reed_solomon::Encoder::new(6);
            let encoded = encoder.encode(&scratch_header[..]);

            writer.write(&**encoded)?;

            let plen = encode_blocks(content, header.fec_bytes, writer)?;

            Ok(encoded.len() + plen)
        },
        _ => {
            //Max size of an inner frame at 2x FEC with 256b frame
            let mut scratch: [u8; 85] = unsafe { ::std::mem::uninitialized() };
            let len = {
                let mut cursor = Cursor::new(&mut scratch[..]);
                encode_inner(packet, &mut cursor)?;

                cursor.position() as usize
            };

            let encoder = reed_solomon::Encoder::new(len * 2);
            let encoded = encoder.encode(&scratch[..len]);

            writer.write(&**encoded)?;

            Ok(encoded.len())
        }
    }
}

fn encode_non_fec<'a,T>(packet: &Packet<'a>, writer: &mut T) -> Result<usize, PacketEncodeError> where T: Write {
    match packet {
        &Packet::Data(ref header, ref payload) => {
            let len = encode_inner(packet, writer)?;
            writer.write(payload)?;

            Ok(len+payload.len())
        },
        _ => encode_inner(packet, writer)
    }
}

fn encode_inner<'a,T>(packet: &Packet<'a>, writer: &mut T) -> Result<usize, PacketEncodeError> where T: Write {
    match packet {
        &Packet::Ack(ref header) => {
            let mut seq_id = header.packet_id & ((!DATA_MASK as u16) << 8 | 0xFF);
            seq_id = seq_id | ((ACK_MASK as u16) << 8);
            
            let mut errs_flags = header.corrected_errors & CORRECTED_ERR_MASK;
            if header.nack {
                errs_flags = errs_flags | NEGATIVE_ACK_MASK;
            }

            if header.no_response {
                errs_flags = errs_flags | NO_RESPONSE_MASK;
            }

            writer.write_u16::<BigEndian>(seq_id)?;
            writer.write_u8(errs_flags)?;

            Ok(3)
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
            writer.write(header.callsign)?;

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
                ControlType::Notification => 6,
                _ => return Err(PacketEncodeError::BadFormat)
            };
            let mut packet_type = CTRL_MASK & ctrl_type;

            writer.write_u8(packet_type)?;
            writer.write_u16::<BigEndian>(header.session_id)?;
            writer.write(header.source_callsign)?;
            writer.write_u8(0)?;
            writer.write(header.dest_callsign)?;

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

fn encode_blocks<T>(data: &[u8], fec_bytes: u8, writer: &mut T) -> Result<usize, PacketEncodeError> where T: Write {
    let fec_bytes = get_fec_bytes(fec_bytes);
    let encoder = reed_solomon::Encoder::new(fec_bytes);
    let mut encoded_size = 0;

    for block in data.chunks(256-fec_bytes) {
        let encoded = encoder.encode(block);
        writer.write(&**encoded)?;

        encoded_size += (**encoded).len();
    }

    Ok(encoded_size)
}

fn decode_fec<'a>(data: &'a mut [u8]) -> Result<(Packet<'a>, usize), PacketDecodeError> {
    //Possibly non-data packet, try that first
    if data.len() % 3 == 0 {
        let data_len = data.len() / 3;

        let decoder = reed_solomon::Decoder::new(data.len() - data_len);
        let decoded = decoder.correct_err_count(data, None).map_err(|_| PacketDecodeError::TooManyFECErrors);

        //We have non-data packet
        if let Ok((fixed, errs)) = decoded {
            if errs > 0 {
                data.copy_from_slice(&**fixed);
            }

            return decode_corrected(&data[..data_len]).map(|p| (p,errs))
        }

        trace!("Couldn't decoded non-data packet, trying as data");
    }

    if data.len() < 9 {
        trace!("Failed to decode packet, missing data header");
        return Err(PacketDecodeError::BadFormat)
    }

    //Header is 1 + 2 bytes
    let decoder = reed_solomon::Decoder::new(6);
    let decoded = decoder.correct_err_count(&data[..9], None).map_err(|_| PacketDecodeError::TooManyFECErrors);

    if let Ok((header, errs)) = decoded {
        return decode_data(&*header, &data[9..]).map(|p| (p,errs))
    } else {
        Err(PacketDecodeError::TooManyFECErrors)
    }
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

    let link_width = Cursor::new(&data[3..4]).read_u16::<BigEndian>().map_err(|_| PacketDecodeError::BadFormat)?;

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
    if data.len() != 3 {
        return Err(PacketDecodeError::BadFormat)
    }

    let packet_id = decode_sequence_id(&data[0..2])?;
    let nack = data[2] & NEGATIVE_ACK_MASK == NEGATIVE_ACK_MASK;
    let no_response = data[2] & NO_RESPONSE_MASK == NO_RESPONSE_MASK;
    let corrected_errors = data[2] & CORRECTED_ERR_MASK;

    Ok(Packet::Ack(AckPacket {
        packet_id,
        nack,
        no_response,
        corrected_errors
    }))
}

fn decode_ctrl<'a>(data: &'a [u8]) -> Result<Packet<'a>, PacketDecodeError> {
    if data.len() < 6 {
        return Err(PacketDecodeError::BadFormat)
    }

    let ctrl_type = match (data[0] & CONTROL_TYPE_MASK) >> 4 {
        0 => ControlType::Reserved,
        1 => ControlType::LinkRequest,
        2 => ControlType::LinkOpened,
        3 => ControlType::LinkClose,
        4 => ControlType::LinkClear,
        5 => ControlType::NodeWaiting,
        6 => ControlType::Notification,
        _ => return Err(PacketDecodeError::BadFormat)
    };

    let session_id = Cursor::new(&data[1..2]).read_u16::<BigEndian>().map_err(|_| PacketDecodeError::BadFormat)?;
    
    let callsign_block = &data[3..];

    //We separate callsigns by the null terminator
    let callsign_split = callsign_block.iter().position(|v| *v == 0);

    let (source_callsign, dest_callsign) = if let Some(idx) = callsign_split {
        callsign_block.split_at(idx)
    } else {
        return Err(PacketDecodeError::BadFormat)
    };

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

        let written = encode(&packet.clone(), fec, &mut scratch).unwrap();
        assert_eq!(written, scratch.len());

        //FEC tests takes considerable time so only test in release
        if cfg!(debug_assertions) {
            max_err = 0;
        }

        if fec {
            for e in 0..max_err {
                let stride = scratch.len() - e;

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
                packet_id: i,
                nack: true,
                no_response: true,
                corrected_errors: 3
            });

            verify_packet(packet, 3);
        }

    }
}