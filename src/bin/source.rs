use crate::BinResult;
use gifski::Collector;


pub const DEFAULT_FPS: f32 = 20.;
pub trait Source {
    fn total_frames(&self) -> Option<u64>;
    fn collect(&mut self, dest: &mut Collector) -> BinResult<()>;
}

#[derive(Debug, Copy, Clone)]
pub struct Fps {
    /// output rate
    pub fps: Option<f32>,
    /// skip frames
    pub speed: f32,
}
