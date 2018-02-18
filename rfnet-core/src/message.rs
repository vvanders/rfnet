use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian};

use std::io;

#[derive(PartialEq, Debug)]
pub enum MessageType {
    Reserved,
    REST,
    Raw
}

#[derive(PartialEq, Debug)]
pub struct RequestMessage<'a> {
    signature: &'a [u8],
    sequence_id: u16,
    req_type: MessageType,
    addr: &'a [u8],
    payload: &'a [u8]
}

#[derive(PartialEq, Debug)]
pub struct ResponseMessage<'a> {
    addr: &'a [u8],
    sequence_id: u16,
    resp_type: MessageType,
    payload: &'a [u8]
}

fn decode_type(flag: u8) -> io::Result<MessageType> {
    match flag {
        0 => Ok(MessageType::Reserved),
        1 => Ok(MessageType::REST),
        2 => Ok(MessageType::Raw),
        o => Err(io::Error::new(io::ErrorKind::InvalidData, format!("Invalid message type {}", o)))
    }
}

fn decode_addr<'a>(data: &'a [u8]) -> io::Result<&'a [u8]> {
    let mut len = 0;
    
    while len < data.len() {
        if data[len] == 0 && len > 1 {
            return Ok(&data[..len])
        }

        len += 1;
    }

    Err(io::Error::new(io::ErrorKind::InvalidData, "Unable to find null terminator in address"))
}

pub fn decode_request_message<'a>(data: &'a [u8]) -> io::Result<RequestMessage<'a>> {
    let mut offset = 0;

    if data.len() < 64 + 2 + 1 + 1 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "truncated"))
    }

    let signature = &data[..64];
    offset += 64;

    let sequence_id = io::Cursor::new(&data[offset..]).read_u16::<BigEndian>()?;
    offset += 2;

    let req_type = decode_type(data[offset])?;
    offset += 1;

    let addr = decode_addr(&data[offset..])?;
    offset += addr.len()+1;

    let payload = &data[offset..];

    Ok(RequestMessage {
        signature,
        sequence_id,
        req_type,
        addr,
        payload
    })
}

pub fn decode_response_message<'a>(data: &'a [u8]) -> io::Result<ResponseMessage<'a>> {
    let mut offset = 0;

    let addr = decode_addr(data)?;
    offset += addr.len()+1;

    if offset + 2 >= data.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "truncated"))
    }

    let sequence_id = io::Cursor::new(&data[offset..]).read_u16::<BigEndian>()?;
    offset += 2;

    let resp_type = decode_type(data[offset])?;
    offset += 1;

    let payload = &data[offset..];

    Ok(ResponseMessage {
        addr,
        sequence_id,
        resp_type,
        payload
    })
}

fn encode_type(ty: &MessageType) -> u8 {
    match ty {
        &MessageType::Reserved => 0,
        &MessageType::REST => 1,
        &MessageType::Raw => 2
    }
}

fn encode_addr<W>(addr: &[u8], writer: &mut W) -> io::Result<()> where W: io::Write {
    writer.write_all(addr)?;
    writer.write_all(&[0])?;

    Ok(())
}

pub fn encode_request_message<W>(msg: &RequestMessage, writer: &mut W) -> io::Result<()> where W: io::Write {
    writer.write_all(msg.signature)?;
    writer.write_u16::<BigEndian>(msg.sequence_id)?;
    writer.write_all(&[encode_type(&msg.req_type)])?;
    encode_addr(msg.addr, writer)?;
    writer.write_all(msg.payload)?;

    Ok(())
}


pub fn encode_response_message<W>(msg: &ResponseMessage, writer: &mut W) -> io::Result<()> where W: io::Write {
    encode_addr(msg.addr, writer)?;
    writer.write_u16::<BigEndian>(msg.sequence_id)?;
    writer.write_all(&[encode_type(&msg.resp_type)])?;
    writer.write_all(msg.payload)?;

    Ok(())
}

#[test]
fn test_request() {
    let callsign = b"KI7EST@rfnet.net";
    let payload = (0..200).collect::<Vec<u8>>();
    let signature = (0..64).collect::<Vec<u8>>();

    let req = RequestMessage {
        signature: &signature[..],
        sequence_id: 1000,
        req_type: MessageType::REST,
        addr: &callsign[..],
        payload: &payload[..]
    };

    let mut encode = vec!();
    encode_request_message(&req, &mut encode).unwrap();
    let decoded = decode_request_message(&encode[..]).unwrap();

    assert_eq!(req, decoded);
}

#[test]
fn test_response() {
    let callsign = b"KI7EST@rfnet.net";
    let payload = (0..200).collect::<Vec<u8>>();
    let signature = (0..64).collect::<Vec<u8>>();

    let resp = ResponseMessage {
        sequence_id: 1000,
        resp_type: MessageType::REST,
        addr: &callsign[..],
        payload: &payload[..]
    };

    let mut encode = vec!();
    encode_response_message(&resp, &mut encode).unwrap();
    let decoded = decode_response_message(&encode[..]).unwrap();

    assert_eq!(resp, decoded);
}