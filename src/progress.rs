extern crate pbr;

use std::io::Stdout;

use self::pbr::ProgressBar;

/// A trait that is used to report progress to some consumer.
pub trait ProgressReporter {
    /// Increase the progress counter.
    fn increase(&mut self);

    /// Mark the progress as done.
    fn done(&mut self);
}

/// A basic progress struct, that reports it's progress to the console.
pub struct BasicProgress {
    /// The current progress value
    current: u64,

    // The total progress value
    total: u64,
}

impl BasicProgress {
    /// Constructor.
    pub fn new(total: u64) -> BasicProgress {
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

    fn done(&mut self) {
        println!("Processed {} frames", self.total);
    }
}

/// Implement the progress reporter trait for a progress bar,
/// to make it usable for frame processing reporting.
impl ProgressReporter for ProgressBar<Stdout> {
    fn increase(&mut self) {
        // Increase the progress bar
        self.inc();
    }

    fn done(&mut self) {
        // Get the total
        let total = self.total;

        // Finish
        self.finish_print(format!("Processed {} frames", total).as_str());
    }
}
