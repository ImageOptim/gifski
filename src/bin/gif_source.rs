//! This is for reading GIFs as an input for re-encoding as another GIF

use crate::source::{Fps, Source, DEFAULT_FPS};
use crate::{BinResult, SrcPath};
use gif::Decoder;
use gifski::Collector;
use std::io::Read;

pub struct GifDecoder {
    fps: Option<f32>,
    speed: f32,
    decoder: Decoder<Box<dyn Read>>,
    screen: gif_dispose::Screen,
}

impl GifDecoder {
    pub fn new(src: SrcPath, fps: Fps) -> BinResult<Self> {
        let input = match src {
            SrcPath::Path(path) => Box::new(std::fs::File::open(path)?) as Box<dyn Read>,
            SrcPath::Stdin(buf) => Box::new(buf),
        };

        let mut gif_opts = gif::DecodeOptions::new();
        // Important:
        gif_opts.set_color_output(gif::ColorOutput::Indexed);

        let decoder = gif_opts.read_info(input)?;
        let screen = gif_dispose::Screen::new_decoder(&decoder);

        Ok(Self {
            fps: fps.fps,
            speed: fps.speed,
            decoder,
            screen,
        })
    }
}

impl Source for GifDecoder {
    fn total_frames(&self) -> Option<u64> { None }
    fn collect(&mut self, c: &mut Collector) -> BinResult<()> {
        let mut idx = 0;
        let mut delay_ts = 0;
        let skip_frames = self.fps.is_some();
        let wanted_frame_time = 1. / f64::from(self.fps.unwrap_or(DEFAULT_FPS));
        let mut wanted_presentation_timestamp = 0.;
        while let Some(frame) = self.decoder.read_next_frame()? {
            self.screen.blit_frame(frame)?;
            let pixels = self.screen.pixels_rgba().map_buf(|b| b.to_owned());
            let presentation_timestamp = f64::from(delay_ts) * (1. / (100. * f64::from(self.speed)));
            delay_ts += u32::from(frame.delay);
            if skip_frames && presentation_timestamp < wanted_presentation_timestamp {
                continue
            }
            wanted_presentation_timestamp += wanted_frame_time;
            c.add_frame_rgba(idx, pixels, presentation_timestamp)?;
            idx += 1;
        }
        Ok(())
    }
}
