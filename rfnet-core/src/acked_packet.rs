use std::io;

const MAX_RETRY: usize = 5;

pub enum AckResult {
    Waiting(usize),
    Failed
}

#[derive(Debug, Clone)]
pub struct AckedPacket {
    attempts: usize,
    last_attempt: usize,
    retry_timeout: usize
}

impl AckedPacket {
    pub fn new(retry_timeout: usize) -> AckedPacket {
        AckedPacket {
            retry_timeout,
            attempts: 0,
            last_attempt: retry_timeout
        }
    }

    pub fn tick<W>(&mut self, packet: &[u8], elapsed: usize, writer: &mut W) -> Result<AckResult, io::Error> where W: io::Write {
        if elapsed > self.last_attempt {
            if self.attempts > MAX_RETRY {
                Ok(AckResult::Failed)
            } else {
                self.attempts = self.attempts + 1;
                self.last_attempt = self.retry_timeout;
                writer.write(packet).map(|_| AckResult::Waiting(self.last_attempt))
            }
        } else {
            self.last_attempt = self.last_attempt - elapsed;
            Ok(AckResult::Waiting(self.last_attempt))
        }
    }
}