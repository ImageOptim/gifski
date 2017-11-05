use std::io::Stdout;
pub use pbr::ProgressBar;

/// A trait that is used to report progress to some consumer.
pub trait ProgressReporter: Send {
    /// Increase the progress counter.
    fn increase(&mut self);

    /// Mark the progress as done.
    fn done(&mut self, msg: &str);
}

pub struct NoProgress {}

impl ProgressReporter for NoProgress {
    fn increase(&mut self) {}
    fn done(&mut self, _msg: &str) {}
}

/// Implement the progress reporter trait for a progress bar,
/// to make it usable for frame processing reporting.
impl ProgressReporter for ProgressBar<Stdout> {
    fn increase(&mut self) {
        self.inc();
    }

    fn done(&mut self, msg: &str) {
        self.finish_print(msg);
    }
}
