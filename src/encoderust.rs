use crate::error::CatResult;
use crate::Encoder;
use crate::GIFFrame;
use crate::Settings;
use rgb::*;
use std::borrow::Cow;
use std::io::Write;

pub(crate) struct RustEncoder<W: Write> {
    writer: Option<W>,
    gif_enc: Option<gif::Encoder<W>>,
}

impl<W: Write> RustEncoder<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: Some(writer),
            gif_enc: None,
        }
    }
}

impl<W: Write> Encoder for RustEncoder<W> {
    fn write_frame(&mut self, f: &GIFFrame, delay: u16, settings: &Settings) -> CatResult<()> {
        let GIFFrame {left, top, ref pal, ref image, screen_width, screen_height, dispose} = *f;

        let writer = &mut self.writer;
        let enc = match self.gif_enc {
            None => {
                let w = writer.take().expect("writer");
                let mut enc = gif::Encoder::new(w, screen_width, screen_height, &[])?;
                if !settings.once {
                    enc.write_extension(gif::ExtensionData::Repetitions(gif::Repeat::Infinite))?;
                }
                self.gif_enc.get_or_insert(enc)
            },
            Some(ref mut enc) => enc,
        };

        let mut transparent_index = None;
        let mut pal_rgb = Vec::with_capacity(3 * pal.len());
        for (i, p) in pal.iter().enumerate() {
            if p.a == 0 {
                transparent_index = Some(i as u8);
            }
            pal_rgb.extend_from_slice([p.rgb()].as_bytes());
        }

        enc.write_frame(&gif::Frame {
            delay,
            dispose,
            transparent: transparent_index,
            needs_user_input: false,
            top,
            left,
            width: image.width() as u16,
            height: image.height() as u16,
            interlaced: false,
            palette: Some(pal_rgb),
            buffer: Cow::Borrowed(image.buf()),
        })?;
        Ok(())
    }
}
