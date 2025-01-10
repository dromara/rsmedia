extern crate clap;
extern crate env_logger;
extern crate metadata;

use crate::clap::Parser;
use metadata::{Cli, MediaFileMetadata, Render};
use std::io;
use std::path::Path;
use std::process;

fn main() {
    process::exit(if run_main() { 0 } else { 1 });
}

fn run_main() -> bool {
    env_logger::init();
    let cli = Cli::try_parse().unwrap();

    let mut successful = true;

    if ffmpeg::init().is_err() {
        eprintln!("Error: failed to initialize libav*");
        return false;
    }
    unsafe {
        ffmpeg::ffi::av_log_set_level(ffmpeg::ffi::AV_LOG_FATAL as i32);
    }

    let build_media_file_metadata = |file: &str| -> io::Result<MediaFileMetadata> {
        let mut meta = MediaFileMetadata::new(&file)?;
        meta.include_checksum(cli.checksum)?
            .include_tags(cli.tags)
            .include_all_tags(cli.all_tags);
        Ok(meta)
    };

    for file in cli.files.iter() {
        if !Path::new(file).is_file() {
            eprintln!("Error: \"{}\" does not exist or is not a file", file);
            successful = false;
            continue;
        }
        match build_media_file_metadata(&file) {
            Ok(m) => match m.render_default() {
                Ok(rendered) => println!("{}", rendered),
                Err(_) => {
                    eprintln!("Error: failed to render metadata for \"{}\"", file);
                    successful = false;
                }
            },
            Err(error) => {
                eprintln!("Error: {}", error);
                successful = false;
            }
        }
    }

    successful
}
