use ffmpeg;
use error::*;
use gifski::Collector;
use std::thread;
use std::path::Path;
use imgref::*;
use rgb::*;

pub struct Decoder {}

impl Decoder {
    pub fn new() -> BinResult<Self> {
        ffmpeg::init().chain_err(|| "Unable to initialize ffmpeg")?;
        Ok(Self {})
    }

    pub fn collect_frames_async(self, path: &Path, mut dest: Collector) -> BinResult<()> {
        let input_context = ffmpeg::format::input(&path)
            .chain_err(|| format!("Unable to open video file {}", path.display()))?;
        thread::spawn(move || {
            if let Err(e) = self.collect_frames(input_context, &mut dest) {
                dest.fail(e.to_string());
            }
            // dest is dropped here, which signals end of input
        });
        Ok(())
    }

    fn collect_frames(self, mut input_context: ffmpeg::format::context::Input, dest: &mut Collector) -> BinResult<()> {
        let (stream_index, mut decoder, mut converter, time_base) = {
            let stream = input_context.streams().best(ffmpeg::media::Type::Video).ok_or("The file has no video tracks")?;

            let mut decoder = stream.codec().decoder().video().chain_err(|| "Unable to decode the codec used in the video")?;

            let converter = decoder.converter(ffmpeg::util::format::pixel::Pixel::RGBA)?;
            (stream.index(), decoder, converter, stream.time_base())
        };

        let mut n = 0;
        let mut gif_delay_pts = 0;
        let mut prev_pts = 0;
        for (s, packet) in input_context.packets() {
            if s.index() != stream_index {
                continue;
            }
            let mut vid_frame = ffmpeg::util::frame::video::Video::empty();
            let decoded = decoder.decode(&packet, &mut vid_frame)?;
            if !decoded || 0 == vid_frame.width() {
                continue;
            }

            let mut rgba_frame = ffmpeg::util::frame::video::Video::empty();
            converter.run(&vid_frame, &mut rgba_frame)?;

            let stride = rgba_frame.width() as usize; // rgba_frame.stride(0) as usize /4
            let rgba_frame = ImgVec::new_stride(
                rgba_frame.data(0).as_rgba().to_owned(),
                rgba_frame.width() as usize,
                rgba_frame.height() as usize,
                stride);

            // FIXME: support fps override
            let pts = vid_frame.pts().unwrap_or(prev_pts + 1);
            let ptsf = (pts as u64 * time_base.numerator() as u64) as f64 / f64::from(time_base.denominator());
            let wanted_pts_gif = (ptsf * 100.0).ceil() as u32;
            let delay = if wanted_pts_gif > gif_delay_pts {wanted_pts_gif - gif_delay_pts} else {2} as u16;
            gif_delay_pts += u32::from(delay);

            prev_pts = pts;

            dest.add_frame_rgba_sync(rgba_frame, delay);

            if n > 325 {
                break;
            }
            n += 1;
        }
        Ok(())
    }
}
