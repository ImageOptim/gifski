use crate::source::{DEFAULT_FPS, Fps, Source};
use crate::{BinResult, SrcPath};
use gifski::Collector;
use imgref::ImgVec;
use rgb::RGBA8;
use std::io::{BufRead, BufReader, ErrorKind};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RawSize {
    width: u16,
    height: u16,
}

impl RawSize {
    fn width(self) -> usize {
        usize::from(self.width)
    }

    fn height(self) -> usize {
        usize::from(self.height)
    }

    fn pixel_count(self) -> Result<usize, &'static str> {
        self.width().checked_mul(self.height()).ok_or("Raw RGBA dimensions are too large")
    }

    fn frame_bytes(self) -> Result<usize, &'static str> {
        self.pixel_count()?.checked_mul(4).ok_or("Raw RGBA frame size is too large")
    }
}

pub fn parse_size(value: &str) -> Result<RawSize, String> {
    let value = value.trim();
    let (width, height) = value.split_once('x')
        .or_else(|| value.split_once('X'))
        .ok_or_else(|| format!("raw RGBA size must be WIDTHxHEIGHT, not '{value}'"))?;
    let width = width.trim().parse::<u32>().map_err(|_| format!("invalid raw RGBA width in '{value}'"))?;
    let height = height.trim().parse::<u32>().map_err(|_| format!("invalid raw RGBA height in '{value}'"))?;
    if width == 0 || height == 0 {
        return Err("raw RGBA width and height must both be greater than zero".into());
    }
    if width > u32::from(u16::MAX) || height > u32::from(u16::MAX) {
        return Err("raw RGBA width and height must not exceed 65535 pixels".into());
    }
    let size = RawSize {
        width: width as u16,
        height: height as u16,
    };
    size.frame_bytes().map_err(|error| error.to_owned())?;
    Ok(size)
}

pub struct RawRgbaDecoder {
    reader: Box<dyn BufRead>,
    size: RawSize,
    pixel_count: usize,
    frames_per_second: f64,
    total_frames: Option<u64>,
}

impl RawRgbaDecoder {
    pub fn new(src: SrcPath, size: Option<RawSize>, rate: Fps) -> BinResult<Self> {
        let size = size.ok_or("Raw RGBA input requires --raw-rgba and --raw-size WIDTHxHEIGHT")?;
        let pixel_count = size.pixel_count()?;
        let frame_bytes = size.frame_bytes()?;
        let (reader, total_frames) = match src {
            SrcPath::Path(path) => {
                let metadata = std::fs::metadata(&path)?;
                if metadata.is_dir() {
                    return Err(format!("{} is a directory, not a raw RGBA file", path.display()).into());
                }
                let total_frames = if metadata.is_file() {
                    let frame_bytes = frame_bytes as u64;
                    if metadata.len() % frame_bytes != 0 {
                        return Err(format!(
                            "Raw RGBA file {} is {} bytes long, which is not a multiple of the {frame_bytes}-byte frame size",
                            path.display(), metadata.len(),
                        ).into());
                    }
                    Some(metadata.len() / frame_bytes)
                } else {
                    None
                };
                let file = std::fs::File::open(&path)?;
                (Box::new(BufReader::new(file)) as Box<dyn BufRead>, total_frames)
            },
            SrcPath::Stdin(reader) => (Box::new(reader) as Box<dyn BufRead>, None),
        };

        let frames_per_second = f64::from(rate.fps.unwrap_or(DEFAULT_FPS)) * f64::from(rate.speed);
        if !frames_per_second.is_finite() || frames_per_second <= 0. {
            return Err("Raw RGBA frame rate must be a positive finite number".into());
        }

        Ok(Self {
            reader,
            size,
            pixel_count,
            frames_per_second,
            total_frames,
        })
    }
}

impl Source for RawRgbaDecoder {
    fn total_frames(&self) -> Option<u64> {
        self.total_frames
    }

    fn collect(&mut self, c: &mut Collector) -> BinResult<()> {
        let mut frame_index = 0;
        while let Some(pixels) = read_frame(&mut *self.reader, self.pixel_count, frame_index)? {
            let pixels = ImgVec::new(pixels, self.size.width(), self.size.height());
            c.add_frame_rgba(frame_index, pixels, frame_index as f64 / self.frames_per_second)?;
            frame_index = frame_index.checked_add(1).ok_or("Too many raw RGBA frames")?;
        }
        Ok(())
    }
}

fn read_frame(reader: &mut dyn BufRead, pixel_count: usize, frame_index: usize) -> BinResult<Option<Vec<RGBA8>>> {
    loop {
        match reader.fill_buf() {
            Ok(buffer) if buffer.is_empty() => return Ok(None),
            Ok(_) => break,
            Err(err) if err.kind() == ErrorKind::Interrupted => {},
            Err(err) => return Err(format!("Unable to read raw RGBA frame {frame_index}: {err}").into()),
        }
    }

    let mut pixels = Vec::new();
    pixels.try_reserve_exact(pixel_count)?;
    pixels.resize(pixel_count, RGBA8::new(0, 0, 0, 0));
    let bytes: &mut [u8] = rgb::bytemuck::cast_slice_mut(&mut pixels);
    let mut bytes_read = 0;
    while bytes_read < bytes.len() {
        match reader.read(&mut bytes[bytes_read..]) {
            Ok(0) => return Err(format!(
                "Raw RGBA frame {frame_index} is truncated: expected {} bytes, received {bytes_read}",
                bytes.len(),
            ).into()),
            Ok(read) => bytes_read += read,
            Err(err) if err.kind() == ErrorKind::Interrupted => {},
            Err(err) => return Err(format!("Unable to read raw RGBA frame {frame_index}: {err}").into()),
        }
    }
    Ok(Some(pixels))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor, Read};

    #[test]
    fn parses_size() {
        assert_eq!(parse_size("640x360").unwrap(), RawSize { width: 640, height: 360 });
        assert_eq!(parse_size("1X2").unwrap(), RawSize { width: 1, height: 2 });
        assert!(parse_size("640").is_err());
        assert!(parse_size("0x360").is_err());
        assert!(parse_size("640x0").is_err());
        assert!(parse_size("65536x1").is_err());
        assert!(parse_size("1x65536").is_err());
        assert!(parse_size("one-by-two").is_err());
    }

    #[test]
    fn reads_rgba_components_and_frame_boundaries() {
        let data = vec![
            1, 2, 3, 4, 5, 6, 7, 8,
            9, 10, 11, 12, 13, 14, 15, 16,
        ];
        let mut reader = Cursor::new(data);
        assert_eq!(read_frame(&mut reader, 2, 0).unwrap().unwrap(), vec![
            RGBA8::new(1, 2, 3, 4),
            RGBA8::new(5, 6, 7, 8),
        ]);
        assert_eq!(read_frame(&mut reader, 2, 1).unwrap().unwrap(), vec![
            RGBA8::new(9, 10, 11, 12),
            RGBA8::new(13, 14, 15, 16),
        ]);
        assert!(read_frame(&mut reader, 2, 2).unwrap().is_none());
    }

    #[test]
    fn reports_a_truncated_frame() {
        let mut reader = Cursor::new(vec![0; 7]);
        let error = read_frame(&mut reader, 2, 3).unwrap_err().to_string();
        assert!(error.contains("frame 3 is truncated"));
        assert!(error.contains("expected 8 bytes, received 7"));
    }

    struct ChunkedReader {
        data: Cursor<Vec<u8>>,
        interrupt_next: bool,
    }

    impl Read for ChunkedReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if std::mem::take(&mut self.interrupt_next) {
                return Err(ErrorKind::Interrupted.into());
            }
            let len = buf.len().min(2);
            self.data.read(&mut buf[..len])
        }
    }

    #[test]
    fn handles_short_and_interrupted_reads() {
        let reader = ChunkedReader {
            data: Cursor::new(vec![1, 2, 3, 4, 5, 6, 7, 8]),
            interrupt_next: true,
        };
        let mut reader = BufReader::with_capacity(2, reader);
        assert_eq!(read_frame(&mut reader, 2, 0).unwrap().unwrap(), vec![
            RGBA8::new(1, 2, 3, 4),
            RGBA8::new(5, 6, 7, 8),
        ]);
        assert!(read_frame(&mut reader, 2, 1).unwrap().is_none());
    }
}
