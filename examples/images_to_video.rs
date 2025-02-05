use std::{cell::RefCell, path::Path, rc::Rc};

use ffmpeg::frame::Video;
use rsmedia::{encode::Encoder, encode::Settings, frame::PixelFormat, Options};
use tracing::{error, info};

fn main() {}

// https://stackoverflow.com/questions/77847158/encode-frames-to-mp4-video-in-rust
pub fn images_to_video(frames: Vec<Video>, output: &str) {
    let width = 100;
    let height = 100;
    let mut encoder = Encoder::new(
        Path::new(output),
        // Settings::preset_h264_yuv420p(width, height, true),
        Settings::preset_h264_custom(width, height, PixelFormat::NV12, Options::default()),
    )
    .unwrap();
    for frame in frames.clone() {
        let result = encoder.encode_raw(frame.clone());
        match result {
            Ok(_) => {
                info!("succesfully ecoded a frame");
            }
            Err(e) => {
                info!("error occured when encoding {}", e);
            }
        }
    }
    let result = encoder.finish();
    match result {
        Ok(_) => {
            info!("succesfully ecoded all");
        }
        Err(e) => {
            error!("error occured when finishing encoding {}", e);
        }
    }
    info!("number of frames {}", frames.len());
}
