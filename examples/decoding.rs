use image::{ImageBuffer, Rgb};

use rsmedia::{filter, DecoderBuilder, MediaFrame, MediaType};

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

    let source = std::path::Path::new("/tmp/test.mp4");

    // 640x360 mp4
    // let source = "https://img.qunliao.info/4oEGX68t_9505974551.mp4"
    //     .parse::<url::Url>()
    //     .unwrap();

    let filters = vec![
        filter::video::scale(640, 360, None),
        filter::video::fps(30.0),
    ];

    let mut decoder = DecoderBuilder::new(MediaType::VIDEO)
        // decoder with CUDA acceleration
        // .with_hardware_device(Some(HWDeviceType::CUDA.auto_best_config().unwrap()))
        // .with_codec_name("h264_cuvid".to_string())
        .with_filters(filters)
        .build_wrapped(source)
        .context("failed to create decoder")?;

    std::fs::create_dir_all(OUTPUT_DIR).context("failed to create output directory")?;

    // seek to the 20th frame
    decoder.seek_to_frame(20).unwrap();

    loop {
        match decoder.decode_frame() {
            Ok(Some(yuv_frame)) => {
                println!(
                    "decoded frame pts: {}, type: {:?}, format:{:?}",
                    yuv_frame.pts,
                    yuv_frame.media_type,
                    yuv_frame
                        .video_format()
                        .map(|f| f.get_pix_fmt_name())
                        .unwrap_or_else(|| "n/a".to_string())
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
