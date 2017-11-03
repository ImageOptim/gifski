extern crate gifski;
#[macro_use] extern crate clap;
#[macro_use] extern crate error_chain;


use gifski::progress::{NoProgress, ProgressBar, ProgressReporter};

mod error;
use error::*;
use error::ResultExt;

use clap::*;

use std::time::Duration;
use std::path::{Path, PathBuf};
use std::fs::File;

quick_main!(bin_main);

fn bin_main() -> BinResult<()> {
     let matches = App::new(crate_name!())
                        .version(crate_version!())
                        .about("https://gif.ski")
                        .setting(AppSettings::DeriveDisplayOrder)
                        .arg(Arg::with_name("fps")
                            .long("fps")
                            .help("Animation frames per second")
                            .required(false)
                            .empty_values(false)
                            .value_name("num")
                            .default_value("20"))
                        .arg(Arg::with_name("once")
                            .long("once")
                            .help("Do not loop the GIF"))
                        .arg(Arg::with_name("output")
                            .long("output")
                            .short("o")
                            .help("Destination file to write to")
                            .empty_values(false)
                            .takes_value(true)
                            .value_name("a.gif")
                            .required(true))
                        .arg(Arg::with_name("quiet")
                            .long("quiet")
                            .help("Don not show a progress bar"))
                        .arg(Arg::with_name("fast")
                            .long("fast")
                            .help("3 times faster encoding, but 10% lower quality and bigger file"))
                        .arg(Arg::with_name("FRAMES")
                            .help("PNG files for animation frames")
                            .min_values(1)
                            .empty_values(false)
                            .use_delimiter(false)
                            .required(true))
                        .get_matches();

    let frames: Vec<_> = matches.values_of_os("FRAMES").ok_or("Missing files")?.collect();
    let output_path = Path::new(matches.value_of_os("output").ok_or("Missing output")?);
    let settings = gifski::Settings {
        width: None, height: None,
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

    for (i, frame) in frames.into_iter().enumerate() {
        let delay = ((i + 1) * 100 / fps) - (i * 100 / fps); // See telecine/pulldown.
        collector.add_frame_png_file(PathBuf::from(frame), delay as u16);
    }
    drop(collector); // necessary to prevent writer waiting for more frames forever

    writer.write(File::create(output_path)?, &mut progress)?;

    progress.done(&format!("gifski created {}", output_path.display()));
    Ok(())
}
