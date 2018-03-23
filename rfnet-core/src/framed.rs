use std::io;
use kiss;

pub trait FramedWrite : io::Write {
    fn start_frame(&mut self) -> io::Result<()>;
    fn end_frame(&mut self) -> io::Result<()>;

    fn write_frame(&mut self, frame: &[u8]) -> io::Result<()> {
        self.start_frame()?;
        self.write_all(frame)?;
        self.end_frame()?;

        Ok(())
    }
}

pub trait FramedRead<T> {
    fn read_frame<'a>(&mut self, read_cache: &'a mut T) -> io::Result<Option<&'a mut [u8]>>;
}

pub struct KISSFramed<T> where T: io::Write + io::Read {
    kiss_tnc: T,
    port: u8,
    pending_frame: Vec<u8>, //@todo: Stream this via io::Write
    pending_recv: Vec<u8>
}

impl<T> KISSFramed<T> where T: io::Write + io::Read {
    pub fn new(kiss_tnc: T, port: u8) -> KISSFramed<T> {
        KISSFramed {
            kiss_tnc,
            port,
            pending_frame: vec!(),
            pending_recv: vec!()
        }
    }

    pub fn get_tnc(&self) -> &T {
        &self.kiss_tnc
    }

    pub fn get_tnc_mut(&mut self) -> &mut T {
        &mut self.kiss_tnc
    }
}

impl<T> FramedWrite for KISSFramed<T> where T: io::Write + io::Read {
    fn start_frame(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn end_frame(&mut self) -> io::Result<()> {
        kiss::encode(io::Cursor::new(&self.pending_frame[..]), &mut self.kiss_tnc, self.port)?;

        //@todo: We could change kiss::encode to be streaming and skip this step
        self.pending_frame.clear();

        Ok(())
    }
}

impl<T> FramedRead<Vec<u8>> for KISSFramed<T> where T: io::Write + io::Read {
    fn read_frame<'a>(&mut self, recv_buffer: &'a mut Vec<u8>) -> io::Result<Option<&'a mut [u8]>> {
        loop {
            let mut scratch: [u8; 256] = unsafe { ::std::mem::uninitialized() };
            match self.kiss_tnc.read(&mut scratch) {
                Ok(0) => break,
                Ok(n) => self.pending_recv.extend_from_slice(&scratch[..n]),
                Err(e) => match e.kind() {
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted => break,
                    _ => return Err(e)
                }
            }
        }

        recv_buffer.clear();
        if let Some(decoded) = kiss::decode(self.pending_recv.iter().cloned(), recv_buffer) {
            self.pending_recv.drain(..decoded.bytes_read);
            Ok(Some(&mut recv_buffer[..]))
        } else {
            Ok(None)
        }
    }
}

impl<T> io::Write for KISSFramed<T> where T: io::Write + io::Read {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending_frame.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.pending_frame.flush()
    }
}

//For testing
pub struct LoopbackIo {
    buffer: Vec<u8>
}

impl LoopbackIo {
    pub fn new() -> LoopbackIo {
        LoopbackIo {
            buffer: vec!()
        }
    }

    pub fn buffer_mut(&mut self) -> &mut Vec<u8> {
        &mut self.buffer
    }
}

impl io::Write for LoopbackIo {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl io::Read for LoopbackIo {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let count = ::std::cmp::min(self.buffer.len(), buf.len());
        buf[..count].copy_from_slice(&self.buffer[..count]);
        self.buffer.drain(..count);

        Ok(count)
    }
}

impl FramedWrite for Vec<u8> {
    fn start_frame(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn end_frame(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn test_encode_decode() {
    let data = (0..256).map(|v| v as u8).collect::<Vec<u8>>();
    let mut recv_buffer = vec!();

    let mut framed = KISSFramed::new(LoopbackIo::new(), 0);

    framed.write_frame(&data[..]).unwrap();
    assert_eq!(framed.read_frame(&mut recv_buffer).unwrap().unwrap(), &data[..]);
    assert!(framed.read_frame(&mut recv_buffer).unwrap().is_none());

    framed.write_frame(&data[..]).unwrap();
    framed.write_frame(&data[..]).unwrap();
    assert_eq!(framed.read_frame(&mut recv_buffer).unwrap().unwrap(), &data[..]);
    assert_eq!(framed.read_frame(&mut recv_buffer).unwrap().unwrap(), &data[..]);
    assert!(framed.read_frame(&mut recv_buffer).unwrap().is_none());
}