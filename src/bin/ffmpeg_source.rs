extern crate ffmpeg_next as ffmpeg;
use crate::BinResult;
use gifski::Collector;
use imgref::*;
use rgb::*;
use crate::source::*;
use std::path::Path;

pub struct FfmpegDecoder {
    input_context: ffmpeg::format::context::Input,
    frames: u64,
    fps: f32,
    speed: f32,
    time_base: f64,
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
        let time_base = stream.time_base().numerator() as f64 / stream.time_base().denominator() as f64;
        let frames = (stream.duration() as f64 * time_base as f64 * (fps / speed) as f64).ceil() as u64;
        Ok(Self {
            input_context,
            frames,
            fps,
            speed,
            time_base,
        })
    }

    pub fn collect_frames(&mut self, mut dest: Collector) -> BinResult<()> {
        let (stream_index, mut decoder, mut filter, time_base) = {
            let stream = self.input_context.streams().best(ffmpeg::media::Type::Video).ok_or("The file has no video tracks")?;

            let decoder = stream.codec().decoder().video().map_err(|e| format!("Unable to decode the codec used in the video: {}", e))?;

            let time_base = self.time_base / self.speed as f64;
            let buffer_args = format!("width={}:height={}:pix_fmt={}:time_base={}:sar={}",
                decoder.width(),
                decoder.height(),
                decoder.format().descriptor().unwrap().name(),
                stream.time_base(),
                (|sar: ffmpeg::util::rational::Rational| match sar.numerator() {
                    0 => "1".to_string(),
                    _ => format!("{}/{}", sar.numerator(), sar.denominator()),
                })(decoder.aspect_ratio()),
            );
            let mut filter = ffmpeg::filter::Graph::new();
            filter.add(&ffmpeg::filter::find("buffer").unwrap(), "in", &buffer_args)?;
            filter.add(&ffmpeg::filter::find("buffersink").unwrap(), "out", "")?;
            filter.output("in", 0)?.input("out", 0)?.parse(&format!("fps=fps={},format=rgba", self.fps / self.speed)[..])?;
            filter.validate()?;
            (stream.index(), decoder, filter, time_base)
        };

        let mut i = 0;
        let mut delayed_frames = 0;
        let mut pts_last_packet = 0;
        let mut ptsf = 0;
        let ptsf_step = (1.0 / time_base / self.fps as f64) as i64;

        let mut vid_frame = ffmpeg::util::frame::Video::empty();
        let mut filt_frame = ffmpeg::util::frame::Video::empty();
        let mut add_frame = |rgba_frame: &ffmpeg::util::frame::Video, pts: f64, pos: i64| -> BinResult<()> {
            let stride = rgba_frame.stride(0) as usize / 4;
            let rgba_frame = ImgVec::new_stride(
                rgba_frame.data(0).as_rgba().to_owned(),
                rgba_frame.width() as usize,
                rgba_frame.height() as usize,
                stride,
            );
            Ok(dest.add_frame_rgba(pos as usize, rgba_frame, pts)?)
        };

        let mut packets = self.input_context.packets();
        while let item = packets.next() {
            let packet = if item.is_none() {
                ffmpeg::Packet::empty()
            } else {
                let (s, packet) = item.unwrap();
                if s.index() != stream_index {
                    continue;
                }
                pts_last_packet = packet.pts().unwrap() + packet.duration();
                packet
            };
            let decoded = decoder.decode(&packet, &mut vid_frame)?;
            if !decoded || 0 == vid_frame.width() {
                delayed_frames += 1;
                continue;
            }
            unsafe {
                if packet.is_empty() {
                    delayed_frames -= 1;
                }
            }

            filter.get("in").unwrap().source().add(&vid_frame).unwrap();
            while let Ok(..) = filter.get("out").unwrap().sink().frame(&mut filt_frame) {
                ptsf += ptsf_step;
                add_frame(&filt_frame, ptsf as f64 * time_base, i)?;
                i += 1;
            }
            if delayed_frames == 0 {
                break;
            }
        }

        filter.get("in").unwrap().source().close(pts_last_packet).unwrap();
        while let Ok(..) = filter.get("out").unwrap().sink().frame(&mut filt_frame) {
            ptsf += ptsf_step;
            add_frame(&filt_frame, ptsf as f64 * time_base, i)?;
        }
        Ok(())
    }
}
