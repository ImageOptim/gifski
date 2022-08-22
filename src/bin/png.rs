use crate::source::Fps;
use crate::source::Source;
use crate::BinResult;
use gifski::Collector;
use gifski::Error::ThreadSend;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;

pub struct Lodecoder {
    frames: Vec<PathBuf>,
    fps: f32,
}

impl Lodecoder {
    pub fn new(frames: Vec<PathBuf>, params: &Fps) -> Self {
        Self { frames, fps: params.fps }
    }
}

impl Source for Lodecoder {
    fn total_frames(&self) -> u64 {
        self.frames.len() as u64
    }

    fn collect(&mut self, dest: &mut Collector) -> BinResult<()> {
        // Rayon causes a deadlock here, because it doesn't follow frame order.
        // add_frame_png_file will block if the buffer is full, and cause all rayon threads to hang.
        let (even, odd): (Vec<_>, Vec<_>) = self.frames.drain(..).enumerate().partition(|(n,_)| n & 1 != 0);
        let dest = &*dest;
        let fps = self.fps as f64;
        // failure on one thread must kill other threads
        let failed = AtomicBool::new(false);
        Ok(crossbeam_utils::thread::scope(|s| {
            let handles: Result<Vec<_>, _> = [even, odd].into_iter().enumerate().map(|(i, files)| s.builder().name(format!("decode{i}")).spawn(|_| {
                files.into_iter()
                    .take_while(|_| !failed.load(SeqCst))
                    .try_for_each(|(i, frame)| {
                        dest.add_frame_png_file(i, frame, i as f64 / fps).map_err(|e| {
                            failed.store(true, SeqCst);
                            e
                        })
                    })
            }).map_err(|_| ThreadSend)).collect();

            handles?.into_iter().try_for_each(|h| h.join().map_err(|_| ThreadSend)?)
        }).map_err(|_| ThreadSend)??)
    }
}
