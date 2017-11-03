extern crate gifski;
#[macro_use] extern crate clap;
#[macro_use] extern crate error_chain;

#[cfg(feature = "video")]
extern crate ffmpeg;
extern crate imgref;
extern crate rgb;

#[cfg(feature = "video")]
mod video;


use gifski::progress::{NoProgress, ProgressBar, ProgressReporter};

mod error;
use error::*;
use error::ResultExt;

use clap::*;

use std::time::Duration;
use std::path::{Path, PathBuf};
use std::fs::File;

#[cfg(feature = "video")]
const VIDEO_FRAMES_ARG_HELP: &'static str = "one MP4/WebM video, or multiple PNG animation frames";
#[cfg(not(feature = "video"))]
const VIDEO_FRAMES_ARG_HELP: &'static str = "PNG animation frames";

quick_main!(bin_main);

fn bin_main() -> BinResult<()> {
     let matches = App::new(crate_name!())
                        .version(crate_version!())
                        .about("https://gif.ski by Kornel Lesiński")
                        .setting(AppSettings::UnifiedHelpMessage)
                        .setting(AppSettings::DeriveDisplayOrder)
                        .setting(AppSettings::ArgRequiredElseHelp)
                        .arg(Arg::with_name("output")
                            .long("output")
                            .short("o")
                            .help("Destination file to write to")
                            .empty_values(false)
                            .takes_value(true)
                            .value_name("a.gif")
                            .required(true))
                        .arg(Arg::with_name("fps")
                            .long("fps")
                            .help("Animation frames per second (for PNG frames only)")
                            .empty_values(false)
                            .value_name("num")
                            .default_value("20"))
                        .arg(Arg::with_name("fast")
                            .long("fast")
                            .help("3 times faster encoding, but 10% lower quality and bigger file"))
                        .arg(Arg::with_name("quality")
                            .long("quality")
                            .value_name("0-100")
                            .takes_value(true)
                            .help("Lower quality may give smaller file"))
                        .arg(Arg::with_name("width")
                            .long("width")
                            .short("W")
                            .takes_value(true)
                            .value_name("px")
                            .help("Maximum width"))
                        .arg(Arg::with_name("height")
                            .long("height")
                            .short("H")
                            .takes_value(true)
                            .value_name("px")
                            .help("Maximum height (if width is also set)"))
                        .arg(Arg::with_name("once")
                            .long("once")
                            .help("Do not loop the GIF"))
                        .arg(Arg::with_name("quiet")
                            .long("quiet")
                            .help("Don not show a progress bar"))
                        .arg(Arg::with_name("FRAMES")
                            .help(VIDEO_FRAMES_ARG_HELP)
                            .min_values(1)
                            .empty_values(false)
                            .use_delimiter(false)
                            .required(true))
                        .get_matches();

    let frames: Vec<_> = matches.values_of_os("FRAMES").ok_or("Missing files")?.collect();
    let output_path = Path::new(matches.value_of_os("output").ok_or("Missing output")?);
    let settings = gifski::Settings {
        width: parse_opt(matches.value_of("width")).chain_err(|| "Invalid width")?,
        height: parse_opt(matches.value_of("height")).chain_err(|| "Invalid height")?,
        quality: parse_opt(matches.value_of("quality")).chain_err(|| "Invalid quality")?.unwrap_or(100).min(100),
        once: matches.is_present("once"),
        fast: matches.is_present("fast"),
    };
    let quiet = matches.is_present("quiet");
    let fps: usize = matches.value_of("fps").ok_or("Missing fps")?.parse().chain_err(|| "FPS must be a number")?;
    let (mut collector, writer) = gifski::new(settings)?;

    let mut progress: Box<ProgressReporter> = if quiet {
        Box::new(NoProgress {})
    } else {
        let mut pb = ProgressBar::new(frames.len() as u64);
        pb.show_speed = false;
        pb.show_percent = false;
        pb.format(" #_. ");
        pb.message("Frame ");
        pb.set_max_refresh_rate(Some(Duration::from_millis(250)));
        Box::new(pb)
    };

    if frames.len() == 1 {
        decode_video(Path::new(frames[0]), collector)?;
    } else {
        for (i, frame) in frames.into_iter().enumerate() {
            let delay = ((i + 1) * 100 / fps) - (i * 100 / fps); // See telecine/pulldown.
            collector.add_frame_png_file(PathBuf::from(frame), delay as u16);
        }
        drop(collector); // necessary to prevent writer waiting for more frames forever
    }

    writer.write(File::create(output_path).chain_err(|| format!("Can't write to {}", output_path.display()))?, &mut *progress)?;

    progress.done(&format!("gifski created {}", output_path.display()));
    Ok(())
}

fn parse_opt<T: ::std::str::FromStr<Err=::std::num::ParseIntError>>(s: Option<&str>) -> BinResult<Option<T>> {
    match s {
        Some(s) => Ok(Some(s.parse()?)),
        None => Ok(None),
    }
}

#[cfg(feature = "video")]
fn decode_video(path: &Path, collector: gifski::Collector) -> BinResult<()> {
    let vid = video::Decoder::new()?;
    vid.collect_frames_async(path, collector)
}

#[cfg(not(feature = "video"))]
fn decode_video(_: &Path, _: gifski::Collector) -> BinResult<()> {
    Err("This executable has been compiled without video support")?
}
