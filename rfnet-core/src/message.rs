use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian};

use rust_sodium::crypto::sign;

use std::io;

enum MessageType {
    Reserved,
    REST,
    Raw
}

#[derive(PartialEq, Debug, Clone)]
pub enum RESTMethod {
    GET,
    PUT,
    PATCH,
    POST,
    DELETE
}

#[derive(PartialEq, Debug)]
pub enum RequestType<'a> {
    Reserved,
    REST { method: RESTMethod, url: &'a str, headers: &'a str, body: &'a str },
    Raw(&'a [u8])
}

#[derive(PartialEq, Debug)]
pub enum ResponseType<'a> {
    Reserved,
    REST { code: u16, body: &'a str },
    Raw(&'a [u8])
}

#[derive(PartialEq, Debug)]
pub struct RequestMessage<'a> {
    pub sequence_id: u16,
    pub req_type: RequestType<'a>,
    pub addr: &'a str,
}

#[derive(PartialEq, Debug)]
pub struct RequestEnvelope<'a> {
    pub signature: &'a [u8],
    pub contents: &'a [u8],
    pub msg: RequestMessage<'a>
}

#[derive(PartialEq, Debug)]
pub struct ResponseMessage<'a> {
    pub resp_type: ResponseType<'a>,
}

pub const SIGNATURE_LEN: usize = 64;

fn decode_type(flag: u8) -> io::Result<MessageType> {
    match flag {
        0 => Ok(MessageType::Reserved),
        1 => Ok(MessageType::REST),
        2 => Ok(MessageType::Raw),
        o => Err(io::Error::new(io::ErrorKind::InvalidData, format!("Invalid message type {}", o)))
    }
}

fn decode_rest_method(data: &[u8]) -> io::Result<(RESTMethod, usize)> {
    let methods = [
        (&b"GET"[..], RESTMethod::GET),
        (&b"PUT"[..], RESTMethod::PUT),
        (&b"PATCH"[..], RESTMethod::PATCH),
        (&b"POST"[..], RESTMethod::POST),
        (&b"DELETE"[..], RESTMethod::DELETE)
    ];

    for (pattern, method) in methods.into_iter().cloned() {
        if &data[..pattern.len()] == pattern {
            return Ok((method, pattern.len()))
        }
    }

    return Err(io::Error::new(io::ErrorKind::InvalidData, "Unrecognized method"))
}

fn decode_null_delim<'a>(data: &'a [u8], offset: &mut usize) -> io::Result<&'a [u8]> {
    if let Some(pos) = data.iter().skip(*offset).position(|&v| v == 0) {
        let slice = &data[*offset..*offset+pos];
        *offset += pos+1;
        Ok(slice)
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, "Missing null terminator"))
    }
}

fn decode_null_delim_str<'a>(data: &'a [u8], offset: &mut usize) -> io::Result<&'a str> {
    let delim = decode_null_delim(data, offset)?;
    decode_str_slice(delim)
}

fn decode_str_slice<'a>(data: &'a [u8]) -> io::Result<&'a str> {
    ::std::str::from_utf8(data)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Unable to translate to utf8 string"))
}

pub fn decode_request_message<'a>(data: &'a [u8]) -> io::Result<RequestEnvelope<'a>> {
    let mut offset = 0;

    if data.len() < 64 + 2 + 1 + 1 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "truncated"))
    }

    let signature = &data[..64];
    offset += 64;

    let addr = decode_null_delim_str(data, &mut offset)?;

    let sequence_id = io::Cursor::new(&data[offset..]).read_u16::<BigEndian>()?;
    offset += 2;

    let msg_type = decode_type(data[offset])?;
    offset += 1;

    let payload = &data[offset..];

    let req_type = match msg_type {
        MessageType::Reserved => RequestType::Reserved,
        MessageType::Raw => RequestType::Raw(payload),
        MessageType::REST => {
            let (method, mut data_read) = decode_rest_method(payload)?;
            let url = decode_null_delim_str(payload, &mut data_read)?;
            let headers = decode_null_delim_str(payload, &mut data_read)?;
            let body = decode_str_slice(&payload[data_read..])?;

            RequestType::REST { method, url, headers, body }
        }
    };

    Ok(RequestEnvelope {
        signature,
        contents: &data[64..],
        msg: RequestMessage {
            sequence_id,
            req_type,
            addr
        }
    })
}

pub fn decode_response_message<'a>(data: &'a [u8]) -> io::Result<ResponseMessage<'a>> {
    if 2 + 1 >= data.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "truncated"))
    }

    let mut offset = 0;

    let msg_type = decode_type(data[offset])?;
    offset += 1;

    let payload = &data[offset..];

    let resp_type = match msg_type {
        MessageType::Reserved => ResponseType::Reserved,
        MessageType::Raw => ResponseType::Raw(payload),
        MessageType::REST => {
            let code = io::Cursor::new(&payload[..2]).read_u16::<BigEndian>()?;
            let body = decode_str_slice(&payload[2..])?;

            ResponseType::REST { code, body }
        }
    };

    Ok(ResponseMessage {
        resp_type
    })
}

pub fn verify_envelope(msg: &RequestEnvelope, public_key: &[u8]) -> bool {
    if let Some(ref public_key) = sign::PublicKey::from_slice(public_key) {
        if let Some(ref sig) = sign::Signature::from_slice(msg.signature) {
            sign::verify_detached(sig, msg.contents, public_key)
        } else {
            error!("Failed to read signature, not 64 bytes");
            false
        }
    } else {
        error!("Failed to read public key, not 32 bytes");
        false
    }
}

fn encode_type(ty: MessageType) -> u8 {
    match ty {
        MessageType::Reserved => 0,
        MessageType::REST => 1,
        MessageType::Raw => 2
    }
}

fn encode_str<W>(s: &str, writer: &mut W) -> io::Result<()> where W: io::Write {
    writer.write_all(s.as_bytes())?;
    writer.write_all(&[0])?;

    Ok(())
}

pub fn encode_request_message<W>(msg: &RequestMessage, key: &[u8], scratch: &mut Vec<u8>, writer: &mut W) -> io::Result<()> where W: io::Write {
    scratch.clear();
    encode_request_inner(msg, scratch)?;

    let key = sign::SecretKey::from_slice(key);

    let signature = if let Some(ref key) = key {
        sign::sign_detached(&scratch[..], key)
    } else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Private key was too short"))
    };

    writer.write_all(&signature[..])?;
    writer.write_all(&scratch[..])
}

fn encode_request_inner<W>(msg: &RequestMessage, writer: &mut W) -> io::Result<()> where W: io::Write {
    encode_str(msg.addr, writer)?;
    writer.write_u16::<BigEndian>(msg.sequence_id)?;
    match msg.req_type {
        RequestType::Reserved => {
            writer.write_all(&[encode_type(MessageType::Reserved)])?;
        },
        RequestType::Raw(ref payload) => {
            writer.write_all(&[encode_type(MessageType::Reserved)])?;
            writer.write_all(payload)?;
        },
        RequestType::REST { ref method, ref url, ref headers, ref body } => {
            writer.write_all(&[encode_type(MessageType::REST)])?;
            
            match method {
                &RESTMethod::GET => writer.write_all(&b"GET"[..])?,
                &RESTMethod::PUT => writer.write_all(&b"PUT"[..])?,
                &RESTMethod::POST => writer.write_all(&b"POST"[..])?,
                &RESTMethod::PATCH => writer.write_all(&b"PATCH"[..])?,
                &RESTMethod::DELETE => writer.write_all(&b"DELETE"[..])?
            }

            encode_str(url, writer)?;
            encode_str(headers, writer)?;
            writer.write_all(body.as_bytes())?;
        }
    }

    Ok(())
}

pub fn encode_response_message<W>(msg: &ResponseMessage, writer: &mut W) -> io::Result<()> where W: io::Write {
    match msg.resp_type {
        ResponseType::Reserved => {
            writer.write_all(&[encode_type(MessageType::Reserved)])?;
        },
        ResponseType::Raw(ref payload) => {
            writer.write_all(&[encode_type(MessageType::Reserved)])?;
            writer.write_all(payload)?;
        },
        ResponseType::REST { code, ref body } => {
            writer.write_all(&[encode_type(MessageType::REST)])?;
            writer.write_u16::<BigEndian>(code)?;
            writer.write_all(body.as_bytes())?;
        }
    }

    Ok(())
}

#[test]
fn test_request() {
    let callsign = "KI7EST@rfnet.net";

    let url = "http://rfnet.net/v1/endpoint";
    let headers = "key: value\nkey2: value2";
    let body = "body";

    let req = RequestMessage {
        sequence_id: 1000,
        addr: &callsign[..],
        req_type: RequestType::REST {
            method: RESTMethod::GET,
            url,
            headers,
            body
        }
    };

    let mut encode = vec!();
    encode_request_message(&req, &[0; 64], &mut vec!(), &mut encode).unwrap();
    let decoded = decode_request_message(&encode[..]).unwrap();

    assert_eq!(req, decoded.msg);
}

#[test]
fn test_verify() {
    let callsign = "KI7EST@rfnet.net";

    let url = "http://rfnet.net/v1/endpoint";
    let headers = "key: value\nkey2: value2";
    let body = "body";

    let req = RequestMessage {
        sequence_id: 1000,
        addr: &callsign[..],
        req_type: RequestType::REST {
            method: RESTMethod::GET,
            url,
            headers,
            body
        }
    };

    let (pk, sk) = sign::gen_keypair();

    let mut encode = vec!();
    encode_request_message(&req, &sk.0, &mut vec!(), &mut encode).unwrap();
    {
        let decoded = decode_request_message(&encode[..]).unwrap();
        assert!(verify_envelope(&decoded, &pk.0));
    }

    {
        //Flip sequence id
        let offset = 64 + callsign.len() + 1;
        encode[offset] = !encode[offset];

        let decoded = decode_request_message(&encode[..]).unwrap();
        assert!(!verify_envelope(&decoded, &pk.0));
    }
}

#[test]
fn test_response() {
    let resp = ResponseMessage {
        resp_type: ResponseType::REST {
            code: 200,
            body: "OK"
        }
    };

    let mut encode = vec!();
    encode_response_message(&resp, &mut encode).unwrap();
    let decoded = decode_response_message(&encode[..]).unwrap();

    assert_eq!(resp, decoded);
}