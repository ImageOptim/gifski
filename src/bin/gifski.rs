extern crate gifski;
#[macro_use] extern crate clap;
#[macro_use] extern crate error_chain;


use gifski::progress::{NoProgress, ProgressReporter, ProgressBar};

mod error;
use error::*;
use error::ResultExt;

use clap::*;

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
                        .arg(Arg::with_name("FRAMES")
                            .help("PNG files for animation frames")
                            .min_values(1)
                            .empty_values(false)
                            .use_delimiter(false)
                            .required(true)
                            )
                        .get_matches();

    let frames = matches.values_of_os("FRAMES").ok_or("Missing files")?;
    let output_path = Path::new(matches.value_of_os("output").ok_or("Missing output")?);
    let once = matches.is_present("once");
    let quiet = matches.is_present("quiet");
    let fps: usize = matches.value_of("fps").ok_or("Missing fps")?.parse().chain_err(|| "FPS must be a number")?;
    let (mut collector, writer) = gifski::new()?;

    let mut frame_count = 0;
    for (i, frame) in frames.enumerate() {
        let delay = ((i + 1) * 100 / fps) - (i * 100 / fps); // See telecine/pulldown.
        collector.add_frame_png_file(PathBuf::from(frame), delay as u16);
        frame_count += 1;
    }
    drop(collector); // necessary to prevent writer waiting for more frames forever

    let mut progress: Box<ProgressReporter> = if quiet {
        Box::new(NoProgress {})
    } else {
        let mut pb = ProgressBar::new(frame_count);
        pb.format("[#_.]");
        pb.message("Frame ");
        Box::new(pb)
    };

    writer.write(File::create(output_path)?, once, &mut progress)?;
    progress.done();

    if !quiet {
        println!("Created {}", output_path.display());
    }
    Ok(())
}
