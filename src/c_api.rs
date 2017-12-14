//! How to use from C
//!
//! ```c
//! gifski *g = gifski_new(&settings);
//!
//! // Call on decoder thread:
//! gifski_add_frame_rgba(g, i, width, height, buffer, 5);
//! gifski_end_adding_frames(g);
//!
//! // Call on encoder thread:
//! gifski_write(g, "file.gif");
//! gifski_drop(g);
//! ```

use super::*;
use std::os::raw::c_char;
use std::ptr;
use std::slice;
use std::fs::File;
use std::ffi::CStr;
use std::path::{PathBuf, Path};

/// Settings for creating a new encoder instance. See `gifski_new`
#[repr(C)]
pub struct GifskiSettings {
    /// Resize to max this width if non-0
    pub width: u32,
    /// Resize to max this height if width is non-0. Note that aspect ratio is not preserved.
    pub height: u32,
    /// 1-100. Recommended to set to 100.
    pub quality: u8,
    /// If true, looping is disabled
    pub once: bool,
    /// Lower quality, but faster encode
    pub fast: bool,
}

/// Opaque handle used in methods
pub struct GifskiHandle {
    writer: Option<Writer>,
    collector: Option<Collector>,
}

/// Call to start the process
///
/// See `gifski_add_frame_png_file` and `gifski_end_adding_frames`
#[no_mangle]
pub extern "C" fn gifski_new(settings: *const GifskiSettings) -> *mut GifskiHandle {
    let settings = unsafe {if let Some(s) = settings.as_ref() {s} else {
        return ptr::null_mut();
    }};
    let s = Settings {
        width: if settings.width > 0 {Some(settings.width)} else {None},
        height: if settings.height > 0 {Some(settings.height)} else {None},
        quality: settings.quality,
        once: settings.once,
        fast: settings.fast,
    };

    if let Ok((collector, writer)) = new(s) {
        Box::into_raw(Box::new(GifskiHandle {
            writer: Some(writer),
            collector: Some(collector),
        }))
    } else {
        ptr::null_mut()
    }
}

/// File path must be valid UTF-8. This function is asynchronous.
///
/// Delay is in 1/100ths of a second
///
/// Call `gifski_end_adding_frames()` after you add all frames. See also `gifski_write()`
#[no_mangle]
pub extern "C" fn gifski_add_frame_png_file(handle: *mut GifskiHandle, index: u32, file_path: *const c_char, delay: u16) -> bool {
    if file_path.is_null() {
        return false;
    }
    let g = unsafe {handle.as_mut().unwrap()};
    let path = PathBuf::from(unsafe {
        CStr::from_ptr(file_path).to_str().unwrap()
    });
    if let Some(ref mut c) = g.collector {
        c.add_frame_png_file(index as usize, path, delay).is_ok()
    } else {
        false
    }
}

/// Pixels is an array width×height×4 bytes large. The array is copied, so you can free/reuse it immediately.
///
/// Delay is in 1/100ths of a second
///
/// The call may block and wait until the encoder thread needs more frames.
///
/// Call `gifski_end_adding_frames()` after you add all frames. See also `gifski_write()`
#[no_mangle]
pub extern "C" fn gifski_add_frame_rgba(handle: *mut GifskiHandle, index: u32, width: u32, height: u32, pixels: *const RGBA8, delay: u16) -> bool {
    if pixels.is_null() {
        return false;
    }
    let g = unsafe {handle.as_mut().unwrap()};
    if let Some(ref mut c) = g.collector {
        let px = unsafe {
            slice::from_raw_parts(pixels, width as usize * height as usize)
        };
        c.add_frame_rgba(index as usize, ImgVec::new(px.to_owned(), width as usize, height as usize), delay).is_ok()
    } else {
        false
    }
}

/// You must call it at some point (after all frames are set), otherwise `gifski_write()` will never end!
#[no_mangle]
pub extern "C" fn gifski_end_adding_frames(handle: *mut GifskiHandle) -> bool {
    let g = unsafe {handle.as_mut().unwrap()};
    g.collector.take().is_some()
}

/// Write frames to `destination` and keep waiting for more frames until `gifski_end_adding_frames` is called.
#[no_mangle]
pub extern "C" fn gifski_write(handle: *mut GifskiHandle, destination: *const c_char) -> bool {
    if destination.is_null() {
        return false;
    }
    let g = unsafe {handle.as_mut().unwrap()};
    let path = Path::new(unsafe {
        CStr::from_ptr(destination).to_str().unwrap()
    });
    if let Ok(file) = File::create(path) {
        if let Some(writer) = g.writer.take() {
            return writer.write(file, &mut NoProgress {}).is_ok();
        }
    }
    false
}

/// Call to free all memory
#[no_mangle]
pub extern "C" fn gifski_drop(g: *mut GifskiHandle) {
    if !g.is_null() {
        unsafe {
            Box::from_raw(g);
        }
    }
}

#[test]
fn c() {
    let g = gifski_new(&GifskiSettings {
        width: 0, height: 0,
        quality: 100,
        once: false,
        fast: true,
    });
    assert!(!g.is_null());
    assert!(gifski_add_frame_rgba(g, 0, 1, 1, &RGBA::new(0,0,0,0), 5));
    assert!(gifski_end_adding_frames(g));
    gifski_drop(g);
}
