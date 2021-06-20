use std::io::{Seek, SeekFrom, Read, Result, Write};

pub struct MetadataSpace<S: Read + Write + Seek> {
    stream: S,
}

impl<S: Read + Write + Seek> MetadataSpace<S> {
    pub fn new(mut stream: S, location: u64) -> Self {
        stream.seek(SeekFrom::Start(location * 512)).unwrap();
        Self {
            stream,
        }
    }
}

impl<S: Read + Write + Seek> Read for MetadataSpace<S> {
    fn read(&mut self, mut buf: &mut [u8]) -> Result<usize> {
        let mut total = 0;

        let position = self.stream.stream_position()?;

        let sector_bytes_left = 496 - (position % 512); //Bytes left to read before we come across byte number 496 (mod 512)

        loop {
            let buffer_left = buf.len();

            //Figure out how many bytes to read
            let read_length = if buffer_left < 496 {
                buffer_left as u64
            } else {
                sector_bytes_left
            };

            total += self.stream.read(& mut buf[..read_length as usize])?;

            buf = & mut buf[read_length as usize..];

            //If the position is of the form 512n + 496, skip the next 16 bytes
            if self.stream.stream_position()? % 512 == 496 {
                self.stream.seek(SeekFrom::Current(16))?;
            }

            //If we've read buf.len bytes, stop
            if buf.is_empty() {
                break;
            }
        }

        Ok(total)
    }
}

impl<S: Read + Write + Seek> Write for MetadataSpace<S> {
    fn write(&mut self, mut buf: & [u8]) -> Result<usize> {
        let mut total = 0;

        let position = self.stream.stream_position()?;

        let sector_bytes_left = 496 - (position % 512); //Bytes left to read before we come across byte number 496 (mod 512)

        loop {
            let buffer_left = buf.len();

            //Figure out how many bytes to read
            let read_length = if buffer_left < 496 {
                buffer_left as u64
            } else {
                sector_bytes_left
            };

            total += self.stream.write(& buf[..read_length as usize])?;

            buf = & buf[read_length as usize..];

            //If the position is of the form 512n + 496, skip the next 16 bytes
            if self.stream.stream_position()? % 512 == 496 {
                self.stream.seek(SeekFrom::Current(16))?;
            }

            //If we've written buf.len bytes, stop
            if buf.is_empty() {
                break;
            }
        }

        Ok(total)
    }

    fn flush(&mut self) -> Result<()> {
        self.stream.flush()
    }
}
