use crate::BinResult;
use gifski::Collector;
use imgref::*;
use rgb::*;
use crate::source::*;
use std::path::Path;

pub struct FfmpegDecoder {
    input_context: ffmpeg::format::context::Input,
    frames: u64,
    speed: f32,
    pts_frame_step: f64,
    min_pts: f64,
}

impl Source for FfmpegDecoder {
    fn total_frames(&self) -> u64 {
        self.frames
    }
    fn collect(&mut self, dest: Collector) -> BinResult<()> {
        self.collect_frames(dest)
    }
}

impl FfmpegDecoder {
    pub fn new(path: &Path, fps: f32, speed: f32) -> BinResult<Self> {
        ffmpeg::init().map_err(|e| format!("Unable to initialize ffmpeg: {}", e))?;
        let input_context = ffmpeg::format::input(&path)
            .map_err(|e| format!("Unable to open video file {}: {}", path.display(), e))?;
        // take fps override into account
        let stream = input_context.streams().best(ffmpeg::media::Type::Video).ok_or("The file has no video tracks")?;
        let rate = stream.avg_frame_rate().numerator() as f64 / stream.avg_frame_rate().denominator() as f64;
        let max_fps = rate as f32 * speed;
        if fps > max_fps {
            eprintln!("Target frame rate is at most {:.2}fps based on the average frame rate of input source.\n\
                       The specified target will net be reached. It will still be used to filter out higher-speed portions in the input.", max_fps=max_fps);
        }
        let frames = stream.frames() as u64;
        Ok(Self {
            input_context,
            frames: (frames as f64 / rate * match fps > max_fps { true => max_fps, false => fps } as f64 / speed as f64) as u64,
            speed,
            pts_frame_step: 1.0 / fps as f64,
            min_pts: 0.0,
        })
    }

    pub fn collect_frames(&mut self, mut dest: Collector) -> BinResult<()> {
        let (stream_index, mut decoder, mut converter, time_base) = {
            let stream = self.input_context.streams().best(ffmpeg::media::Type::Video).ok_or("The file has no video tracks")?;

            let decoder = stream.codec().decoder().video().map_err(|e| format!("Unable to decode the codec used in the video: {}", e))?;

            let converter = decoder.converter(ffmpeg::util::format::pixel::Pixel::RGBA)?;
            (stream.index(), decoder, converter, stream.time_base())
        };

        let mut i = 0;
        for (s, packet) in self.input_context.packets() {
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

            let stride = rgba_frame.stride(0) as usize / 4;
            let rgba_frame = ImgVec::new_stride(
                rgba_frame.data(0).as_rgba().to_owned(),
                rgba_frame.width() as usize,
                rgba_frame.height() as usize,
                stride,
            );

            let timestamp = vid_frame.timestamp();
            vid_frame.set_pts(timestamp);
            let pts = vid_frame.pts().unwrap();
            let ptsf = (pts as u64 * time_base.numerator() as u64) as f64 / f64::from(time_base.denominator()) / self.speed as f64;

            if ptsf >= self.min_pts {
                dest.add_frame_rgba(i, rgba_frame, ptsf)?;
                i += 1;
                self.min_pts += self.pts_frame_step;
            }
        }
        Ok(())
    }
}
