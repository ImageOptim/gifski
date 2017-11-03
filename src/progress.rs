/// A trait that is used to report progress to some consumer.
pub trait ProgressReporter {
    /// Increase the progress counter.
    fn increase(&mut self);
}

/// A basic progress struct, that reports it's progress to the console.
pub struct BasicProgress {
    /// The current progress value
    current: usize,

    // The total progress value
    total: usize,
}

impl BasicProgress {
    /// Constructor.
    pub fn new(total: usize) -> Self {
        BasicProgress {
            current: 0,
            total,
        }
    }

    /// Report the current progress
    pub fn report(&self) {
        println!("Frame {} of {}", self.current, self.total);
    }
}

impl ProgressReporter for BasicProgress {
    fn increase(&mut self) {
        // Increase the current and report
        self.current += 1;
        self.report();
    }
}
