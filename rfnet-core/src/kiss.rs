//! Implements KISS HLDC framing for communcation with TNCs that implement KISS protocol
use std::io;

///Frame delimiter code, used to represent start and end of frames.
const FEND: u8 = 0xC0;

///Frame escape code, used to escape FESC and FEND codes if they are found in byte stream
const FESC: u8 = 0xDB;

///Escaped FEND value
const TFEND: u8 = 0xDC;

///Escaped FESC value
const TFESC: u8 = 0xDD;

///This frame contains data that should be sent out of the TNC. The maximum number of bytes is determined by the amount of memory in the TNC.
#[allow(dead_code)]
pub const CMD_DATA: u8 = 0x00;
///The amount of time to wait between keying the transmitter and beginning to send data (in 10 ms units).
#[allow(dead_code)]
pub const CMD_TX_DELAY: u8 = 0x01;
///The persistence parameter. Persistence=Data*256-1. Used for CSMA.
#[allow(dead_code)]
pub const CMD_PERSISTENCE: u8 = 0x02;
///Slot time in 10 ms units. Used for CSMA.
#[allow(dead_code)]
pub const CMD_SLOT_TIME: u8 = 0x03;
///The length of time to keep the transmitter keyed after sending the data (in 10 ms units).
#[allow(dead_code)]
pub const CMD_TX_TAIL: u8 = 0x04;
///0 means half duplex, anything else means full duplex.
#[allow(dead_code)]
pub const CMD_DUPLEX: u8 = 0x05;
//Exit KISS mode. This applies to all ports.
#[allow(dead_code)]
pub const CMD_RETURN: u8 = 0xFF;

/// Encodes a series of bytes into a KISS frame.
#[allow(dead_code)]
pub fn encode<R,W>(mut data: R, mut encoded: W, port: u8) -> io::Result<usize> where R: io::Read, W: io::Write {
    let mut written: usize = 0;

    //Data frame command, port is high part of the nibble
    match encoded.write_all(&[FEND, CMD_DATA | ((port & 0x0F) << 4)]) {
        Ok(()) => written += 2,
        Err(e) => {
            error!("Unable to write bytes {:?}", e);
            return Err(e);
        }
    }

    //Process KISS frame in 256 byte increments
    const SCRATCH_SIZE: usize = 256;
    let mut scratch: [u8; SCRATCH_SIZE] = unsafe { ::std::mem::uninitialized() };

    loop {
        match data.read(&mut scratch) {
            Ok(n) => {
                match n {
                    0 => break,
                    _ => {
                        match encode_part(&scratch[..n], &mut encoded) {
                            Ok(w) => written += w,
                            Err(e) => return Err(e)
                        }
                    }
                }
            },
            Err(e) => return Err(e)
        }
    }

    match encoded.write_all(&[FEND]) {
        Ok(()) => written += 1,
        Err(e) => {
            error!("Unable to write bytes {:?}", e);
            return Err(e);
        }
    }

    debug!("Encoded KISS frame of {} bytes for port {}", written, port);
    Ok(written)
}

pub fn encode_part<W>(data: &[u8], mut encoded: W) -> io::Result<usize> where W: io::Write {
    let encode = data.iter().cloned().map(|byte| {
        match byte {
            FEND => (FESC, Some(TFEND)),
            FESC => (FESC, Some(TFESC)),
            _ => (byte, None)
        }
    });

    let mut written = 0;
    for (b1, b2) in encode {
        match b2 {
            Some(data) => {
                try!(encoded.write_all(&[b1, data]));
                written += 2;
            },
            None => {
                try!(encoded.write_all(&[b1]));
                written += 1;
            }
        }
    }

    Ok(written)
}

/// Encodes a command to be sent to the KISS TNC.
#[allow(dead_code)]
pub fn encode_cmd(encoded: &mut Vec<u8>, cmd: u8, data: u8, port: u8) {
    trace!("Encoding KISS command {} {} for port {}", cmd, data, port);

    encoded.push(FEND);

    match cmd {
        //Return uses 0xF0 since it impacts all ports
        CMD_RETURN => encoded.push(CMD_RETURN),
        //Port is high part of the nibble
        _ => {
            encoded.push(cmd | ((port & 0x0F) << 4));
            encoded.push(data);
        }
    }

    encoded.push(FEND);

    debug!("Encoded KISS command {} {} for port {}", cmd, data, port);
}

/// Result from a decode operation
pub struct DecodedFrame {
    /// Port that this frame was decoded from
    pub port: u8,
    /// Number of bytes read from the iterator that was passed to decode(). The calling client is responsible for advancing the interator `bytes_read` after the decode operation.
    pub bytes_read: usize,
    /// Number of bytes in the payload(bytes_read - escape/control bytes)
    pub payload_size: usize
}

/// Decode a KISS frame into a series of bytes.
///
/// Appends all bytes decoded to decoded. If no KISS frames are found in the iterator then returns `None`.
/// Otherwise returns an `Option` of `DecodedFrame`.
#[allow(dead_code)]
pub fn decode<T>(data: T, decoded: &mut Vec<u8>) -> Option<DecodedFrame> where T: Iterator<Item=u8> {
    let (reserved, _) = data.size_hint();
    decoded.reserve(reserved);

    let decoded_start = decoded.len();

    //trace!("Decoding KISS frames");

    //Possible tokens in decoded KISS frame stream
    enum Token {
        Start(usize),   //Frame start at this idx(FEND)
        End(usize),     //Frame end at this idx(FEND followed by 1+ bytes followed by FEND)
        Port(u8),       //First byte of valid frame is a port
        Byte(u8),       //Byte data(item inside two FEND values)
        Empty           //Data before or after FEND pairs
    }

    let (port, start_idx, end_idx) = data.enumerate()    //Keep track of idx so we can return the last idx we processed to the caller
        //Find our first valid start + end frame
        .scan((None, None), |&mut (ref mut start_frame, ref mut end_frame), (idx, byte)| {
            //If we've already found a valid range then stop iterating
            let value =
                //Looking for start of the frame
                if start_frame.is_none() {
                    if byte == FEND {
                        //trace!("Found start frame at {} idx", idx);
                        *start_frame = Some(idx);
                        Token::Start(idx)
                    } else {
                        Token::Empty
                    }
                } else if end_frame.is_none() {   //Looking for the end
                    if byte == FEND {
                        //Empty frame, just restart the scan
                        if start_frame.unwrap()+1 == idx {
                            //trace!("Found empty frame at {} idx, moving to next frame", idx);
                            *start_frame = Some(idx);
                            Token::Start(idx)
                        } else {
                            //trace!("Found end frame at {} idx", idx);
                            *end_frame = Some(idx);
                            Token::End(idx)
                        }
                    } else {
                        if start_frame.unwrap()+1 == idx {
                            let port = byte >> 4;
                            //trace!("Decoded port is {}", port);

                            Token::Port(port)
                        } else {
                            Token::Byte(byte)
                        }
                    }
                } else {
                    return None
                };

            //Still a valid stream
            Some(value)
        })
        //Decode escaped values
        .scan(false, |was_esc, token| {
            let value = match token {
                Token::Byte(byte) => {
                    if byte == FESC {
                        *was_esc = true;
                        None    //Don't include escaped characters
                    } else if *was_esc {
                        *was_esc = false;

                        match byte {
                            TFEND => Some(Token::Byte(FEND)),
                            TFESC => Some(Token::Byte(FESC)),
                            _ => None //This is a bad value, just discard the byte for now since we don't know how to handle it
                        }
                    } else {
                        Some(Token::Byte(byte))
                    }
                },
                _ => Some(token)
            };

            Some(value)
        })
        .filter_map(|x| x)  //Remove escaped characters
        //Aggregate our data and start + end frames. If we don't have both this isn't a valid frame
        .fold((None, None, None), |(port, start_idx, end_idx), token| {
            match token {
                Token::Byte(byte) => {
                    decoded.push(byte);
                    (port, start_idx, end_idx)
                },
                Token::Start(idx) => (port, Some(idx+1), end_idx),
                Token::End(idx) => (port, start_idx, Some(idx-1)),
                Token::Port(port_num) => (Some(port_num), start_idx, end_idx),
                Token::Empty => (port, start_idx, end_idx)
            }
        });

    //Check if we found anything
    port.and_then(|port| {
        end_idx.and_then(|end_idx| {
            start_idx.and_then(|start_idx| {
                let payload_size = end_idx - start_idx;

                debug!("Decoded KISS frame of {} bytes on port {}", payload_size, port);

                Some(DecodedFrame {
                    port: port,
                    bytes_read: end_idx+2,   //Note that since we truncate the FEND we need to add an extra offset here
                    payload_size: decoded.len() - decoded_start
                })
            })
        })
    }).or_else(|| {
        debug!("Empty or incomplete frame, skipping decode");
        None
    })
}


#[test]
fn test_encode() {
    use std::io::Cursor;

    {
        let mut data = vec!();
        encode(&mut Cursor::new(['T', 'E', 'S', 'T'].iter().map(|chr| *chr as u8).collect::<Vec<_>>()), &mut data, 0).unwrap();
        assert_eq!(data, vec!(FEND, CMD_DATA, 'T' as u8, 'E' as u8, 'S' as u8, 'T' as u8, FEND));
    }

    {
        let mut data = vec!();
        encode(&mut Cursor::new(['H', 'E', 'L', 'L', 'O'].iter().map(|chr| *chr as u8).collect::<Vec<_>>()), &mut data, 5).unwrap();
        assert_eq!(data, vec!(FEND, CMD_DATA | 0x50, 'H' as u8, 'E' as u8, 'L' as u8, 'L' as u8, 'O' as u8, FEND));
    }

    {
        let mut data = vec!();
        encode(&mut Cursor::new([FEND, FESC]), &mut data, 0).unwrap();
        assert_eq!(data, vec!(FEND, CMD_DATA, FESC, TFEND, FESC, TFESC, FEND));
    }

    {
        let mut data = vec!();
        encode_cmd(&mut data, CMD_TX_DELAY, 4, 0);
        assert_eq!(data, vec!(FEND, CMD_TX_DELAY, 0x04, FEND));
    }

    {
        let mut data = vec!();
        encode_cmd(&mut data, CMD_TX_DELAY, 4, 6);
        assert_eq!(data, vec!(FEND, CMD_TX_DELAY | 0x60, 0x04, FEND));
    }

    {
        let mut data = vec!();
        encode_cmd(&mut data, CMD_RETURN, 4, 2);
        assert_eq!(data, vec!(FEND, CMD_RETURN, FEND));
    }
}

#[cfg(test)]
fn test_encode_decode_single<T>(source: T) where T: Iterator<Item=u8> {
    use std::io::Cursor;

    let mut data = vec!();
    let mut decoded = vec!();
    let expected: Vec<u8> = source.collect();

    encode(&mut Cursor::new(&expected), &mut data, 5).unwrap();
    match decode(data.iter().cloned(), &mut decoded) {
        Some(result) => {
            assert_eq!(result.port, 5);
            assert_eq!(result.bytes_read, data.len());
            assert_eq!(expected, decoded);
        },
        None => assert!(false)
    }
}

#[cfg(test)]
fn test_decode_single(data: &mut Vec<u8>, expected: &[u8], port: u8) {
    let mut decoded = vec!();

    match decode(data.iter().cloned(), &mut decoded) {
        Some(result) => {
            assert_eq!(result.port, port);
            assert_eq!(expected, decoded.as_slice());
            assert_eq!(result.payload_size, expected.len());

            //Remove the data so subsequent reads work
            data.drain(0..result.bytes_read);
        },
        None => assert!(false)
    }
}

#[test]
fn test_encode_decode() {
    test_encode_decode_single(['T', 'E', 'S', 'T'].iter().map(|chr| *chr as u8));
    test_encode_decode_single(['H', 'E', 'L', 'L', 'O'].iter().map(|chr| *chr as u8));
    test_encode_decode_single([FEND, FESC].iter().map(|data| *data));
}

#[test]
fn test_empty_frame() {
    use std::io::Cursor;

    let mut data = vec!();
    let expected: Vec<u8> = ['T', 'E', 'S', 'T'].iter().map(|chr| *chr as u8).collect();

    data.push(FEND);
    data.push(FEND);
    data.push(FEND);

    encode(&mut Cursor::new(&expected), &mut data, 0).unwrap();
    
    let mut decoded = vec!();
    match decode(data.iter().cloned(), &mut decoded) {
        Some(result) => {
            assert_eq!(result.bytes_read, data.len());
            assert_eq!(result.payload_size, expected.len());
            assert_eq!(result.port, 0);

            assert!(expected.iter().cloned().eq(decoded.into_iter()));
        },
        None => assert!(false)
    }
}

#[test]
fn test_multi_frame() {
    use std::io::Cursor;

    let expected_one: Vec<u8> = ['T', 'E', 'S', 'T'].iter().map(|chr| *chr as u8).collect();
    let expected_two: Vec<u8> = ['H', 'E', 'L', 'L', 'O'].iter().map(|chr| *chr as u8).collect();
    let expected_three = [FEND, FESC];

    let mut data = vec!();

    encode(&mut Cursor::new(&expected_one), &mut data, 0).unwrap();
    encode(&mut Cursor::new(&expected_two), &mut data, 0).unwrap();
    encode(&mut Cursor::new(&expected_three), &mut data, 0).unwrap();

    test_decode_single(&mut data, &expected_one, 0);
    test_decode_single(&mut data, &expected_two, 0);
    test_decode_single(&mut data, &expected_three, 0);
}

#[test]
fn pre_kiss_data() {
    use std::io::Cursor;

    let expected_one: Vec<u8> = ['T', 'E', 'S', 'T'].iter().map(|chr| *chr as u8).collect();
    let mut data = vec!(1, 2, 3);

    encode(&mut Cursor::new(&expected_one), &mut data, 0).unwrap();
    test_decode_single(&mut data, &expected_one, 0);
}

#[test]
fn post_kiss_data() {
    use std::io::Cursor;

    let expected_one: Vec<u8> = ['T', 'E', 'S', 'T'].iter().map(|chr| *chr as u8).collect();
    let mut data = vec!();
    encode(&mut Cursor::new(&expected_one), &mut data, 0).unwrap();

    data.extend_from_slice(&[1, 2, 3]);

    test_decode_single(&mut data, &expected_one, 0);

    let mut decoded = vec!();
    match decode(data.iter().cloned(), &mut decoded) {
        Some(_) => assert!(false),
        None => ()
    }
}