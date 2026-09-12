use image::{ImageBuffer, Rgb};

use rsmedia::io::{Seekable, StreamReader};
use rsmedia::{DecoderBuilder, FrameFormat, HWDeviceConfig, MediaFrame, MediaType, filter};

use anyhow::{Context, Result};
use futures::future::join_all;

use once_cell::sync::Lazy;
use std::sync::Mutex;
use tokio::task;

const OUTPUT_DIR: &str = "output";
static FRAME_COUNT: Lazy<Mutex<u32>> = Lazy::new(|| Mutex::new(0));
static SAVE_TASKS: Lazy<Mutex<Vec<task::JoinHandle<()>>>> = Lazy::new(|| Mutex::new(Vec::new()));

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339())
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .init();

    rsmedia::init().unwrap();

    // 640x360 mp4
    // let source = "https://img.qunliao.info/4oEGX68t_9505974551.mp4"

    let source = "/tmp/test.mp4";

    let filters = vec![
        filter::video::scale(640, 360, None),
        filter::video::fps(30.0),
    ];

    let mut reader = StreamReader::new(source)?;
    let mut decoder = DecoderBuilder::new(MediaType::VIDEO)
        // decoder with CUDA acceleration
        .with_hardware_device(Some(HWDeviceConfig::auto_platform()?))
        // h264_cuvid decoder name
        // .with_codec_name("h264_cuvid".to_string())
        .with_filters(filters)
        .build_from_reader(&reader)
        .context("failed to create decoder")?;

    std::fs::create_dir_all(OUTPUT_DIR).context("failed to create output directory")?;

    // Seek near the 20th frame. MP4/AAC containers do not support frame-based
    // seeking (`AVSeekFlag::FRAME`), so use the reliable timestamp seek which
    // lands on the nearest keyframe (≈666ms at 30fps → before the 20th frame).
    reader
        .seek_to_timestamp(20 * 1000 / 30)
        .context("failed to seek to the 20th frame")?;

    loop {
        match decoder.decode_frame(&mut reader) {
            Ok(Some(yuv_frame)) => {
                println!(
                    "decoded frame pts: {}, type: {:?}, format:{:?}",
                    yuv_frame.pts,
                    yuv_frame.media_type,
                    yuv_frame
                        .format()
                        .map(|f| match f {
                            FrameFormat::Pixel(p) => p.get_pix_fmt_name(),
                            _ => "N/A",
                        })
                        .unwrap_or("N/A")
                );

                process_frame(yuv_frame)?;
            }
            Ok(None) => {
                println!("Decoder has reached the end of the stream");
                break;
            }
            Err(e) => {
                println!("Error decoding frame: {}", e);
                break;
            }
        }
    }

    {
        // Waiting for all tasks to be completed
        let tasks = SAVE_TASKS.lock().unwrap().drain(..).collect::<Vec<_>>();
        join_all(tasks).await;
    }

    println!(
        "Saved {} frames in the '{}' directory",
        FRAME_COUNT.lock().unwrap(),
        OUTPUT_DIR
    );

    Ok(())
}

fn process_frame(yuv_frame: MediaFrame<u8>) -> Result<()> {
    let rgb_frame = yuv_frame.convert_yuv_to_rgb()?;

    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_raw(
        yuv_frame.width as u32,
        yuv_frame.height as u32,
        rgb_frame.data.as_slice().unwrap().to_vec(),
    )
    .context("failed to create image buffer")?;

    let frame_path = format!(
        "{}/frame_{:05}.png",
        OUTPUT_DIR,
        FRAME_COUNT.lock().unwrap()
    );

    let task = task::spawn_blocking(move || {
        img.save(&frame_path).expect("failed to save frame");
    });

    SAVE_TASKS.lock().unwrap().push(task);

    *FRAME_COUNT.lock().unwrap() += 1;

    Ok(())
}
