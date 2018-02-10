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

pub struct KISSFramedWrite<W> where W: io::Write {
    kiss_tnc: W,
    port: u8,
    pending_frame: Vec<u8>
}

impl<W> KISSFramedWrite<W> where W: io::Write {
    pub fn new(kiss_tnc: W, port: u8) -> KISSFramedWrite<W> {
        KISSFramedWrite {
            kiss_tnc,
            port,
            pending_frame: vec!()
        }
    }
}

impl<W> FramedWrite for KISSFramedWrite<W> where W: io::Write {
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

impl<W> io::Write for KISSFramedWrite<W> where W: io::Write {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending_frame.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.pending_frame.flush()
    }
}

//For testing
impl FramedWrite for Vec<u8> {
    fn start_frame(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn end_frame(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn test_encode_kiss() {
    let data = (0..256).map(|v| v as u8).collect::<Vec<u8>>();

    let mut encoded = vec!();
    {
        let mut encoder = KISSFramedWrite::new(&mut encoded, 0);
        encoder.write_frame(&data[..]).unwrap();
    }

    let mut decoded_data = vec!();
    let decoded = kiss::decode(encoded.into_iter(), &mut decoded_data);

    if let Some(frame) = decoded {
        assert_eq!(frame.port, 0);
        assert_eq!(frame.payload_size, data.len());
        assert_eq!(data, decoded_data);
    } else {
        panic!();
    }
}