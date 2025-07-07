//! For tracking conversion progress and aborting early

#[cfg(feature = "pbr")]
#[doc(hidden)]
#[deprecated(note = "The pbr dependency is no longer exposed. Please use a newtype pattern and write your own trait impl for it")]
pub use pbr::ProgressBar;

use std::os::raw::{c_int, c_char, c_void};
use std::ffi::CString;

/// A trait that is used to report progress to some consumer.
pub trait ProgressReporter: Send {
    /// Called after each frame has been written.
    ///
    /// This method may return `false` to abort processing.
    fn increase(&mut self) -> bool;

    /// File size so far
    fn written_bytes(&mut self, _current_file_size_in_bytes: u64) {}

    /// Log an incorrect use of the library
    #[cold]
    fn error(&mut self, _message: String) {}

    /// Not used :(
    /// Writing is done when `Writer::write()` call returns
    fn done(&mut self, _msg: &str) {}
}

/// No-op progress reporter
pub struct NoProgress {}

/// For C
#[derive(Clone)]
pub struct ProgressCallback {
    callback: unsafe extern "C" fn(*mut c_void) -> c_int,
    user_data: *mut c_void,
}

#[derive(Clone)]
pub(crate) struct ErrorCallback {
    pub callback: unsafe extern "C" fn(*const c_char, *mut c_void),
    pub user_data: *mut c_void,
}

#[derive(Clone)]
pub(crate) struct CCallbacks {
    pub progress: Option<ProgressCallback>,
    pub error: Option<ErrorCallback>,
}

unsafe impl Send for ProgressCallback {}
unsafe impl Send for ErrorCallback {}

impl ProgressCallback {
    /// The callback must be thread-safe
    pub fn new(callback: unsafe extern "C" fn(*mut c_void) -> c_int, arg: *mut c_void) -> Self {
        Self { callback, user_data: arg }
    }
}

impl ProgressReporter for NoProgress {
    fn increase(&mut self) -> bool {
        true
    }

    fn done(&mut self, _msg: &str) {}
}

impl ProgressReporter for ProgressCallback {
    fn increase(&mut self) -> bool {
        unsafe { (self.callback)(self.user_data) == 1 }
    }

    fn done(&mut self, _msg: &str) {}
}

impl ProgressReporter for CCallbacks {
    fn increase(&mut self) -> bool {
        if let Some(p) = &mut self.progress {
            p.increase()
        } else {
            true
        }
    }

    #[cold]
    fn error(&mut self, mut msg: String) {
        msg.reserve_exact(1);
        if let Some(err) = &self.error {
            let cstring = CString::new(msg);
            let cstring = cstring.as_deref().unwrap_or_default();
            unsafe { (err.callback)(cstring.as_ptr(), err.user_data) }
        } else {
            use std::io::Write;
            msg.push('\n');
            let _ = std::io::stderr().write_all(msg.as_bytes());
        }
    }

    fn done(&mut self, _msg: &str) {}
}

/// Implement the progress reporter trait for a progress bar,
/// to make it usable for frame processing reporting.
#[cfg(feature = "pbr")]
impl<T> ProgressReporter for ProgressBar<T> where T: std::io::Write + Send {
    fn increase(&mut self) -> bool {
        self.inc();
        true
    }

    fn done(&mut self, msg: &str) {
        self.finish_print(msg);
    }
}
