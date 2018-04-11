use message;

use std::io;

pub struct RequestBuilder {
    data: Vec<u8>,
    position: usize
}

pub struct ResponseReceiver {
    buffer: Vec<u8>
}

pub struct RequestResponse {
    pub request: RequestBuilder,
    pub response: ResponseReceiver
}

impl RequestResponse {
    pub fn new() -> RequestResponse {
        RequestResponse {
            request: RequestBuilder {
                data: vec!(),
                position: 0
            },
            response: ResponseReceiver {
                buffer: vec!()
            }
        }
    }

    pub fn new_request(&mut self, 
            _ver: (u8, u8),
            callsign: &str,
            sequence_id: u16,
            method: message::RESTMethod,
            url: &str,
            headers: &str,
            body: &[u8],
            private_key: &[u8]) -> io::Result<()> {
        self.request.data.clear();
        self.request.position = 0;

        self.response.buffer.clear();

        //Assemble request
        let msg = message::RequestMessage {
            sequence_id,
            addr: callsign,
            req_type: message::RequestType::REST {
                method,
                url,
                headers,
                body
            }
        };

        message::encode_request_message(&msg, private_key, &mut vec!(), &mut self.request.data)
    }
}

impl io::Read for RequestBuilder {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let start = self.position;
        let mut cursor = io::Cursor::new(&self.data[start..]);
        cursor.read(out)?;

        self.position += cursor.position() as usize;

        Ok(self.position - start)
    }
}

impl io::Write for ResponseReceiver {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.buffer.flush()
    }
}

impl ResponseReceiver {
    pub fn decode(&mut self) -> io::Result<message::ResponseMessage> {
        message::decode_response_message(&self.buffer[..])
    }

    pub fn get_data(&self) -> &[u8] { 
        &self.buffer
    }
}

impl RequestBuilder {
    pub fn get_data(&self) -> &[u8] {
        &self.data
    }
}