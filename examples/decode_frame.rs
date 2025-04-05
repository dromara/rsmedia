use image::{ImageBuffer, Rgb};

use rsmedia::{DecoderBuilder, MediaFrame, MediaType, Resize, StreamReader};

use anyhow::{Context, Result};
use futures::future::join_all;

use once_cell::sync::Lazy;
use std::sync::Mutex;
use tokio::task;

const OUTPUT_DIR: &'static str = "output";
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

    // let source = std::path::Path::new("/tmp/bear.mp4");

    // 640x360 mp4
    let source = "https://img.qunliao.info/4oEGX68t_9505974551.mp4"
        .parse::<url::Url>()
        .unwrap();

    let mut stream_reader = StreamReader::new(source)?;
    let mut decoder = DecoderBuilder::new(MediaType::VIDEO)
        .with_resize(Some(Resize::Fit(1280, 720)))
        // decoder with CUDA acceleration
        // .with_hardware_device(Some(HWDeviceType::CUDA.auto_best_config().unwrap()))
        // .with_codec_name(Some("h264_cuvid".to_string()))
        .build(&stream_reader)
        .context("failed to create decoder")?;

    std::fs::create_dir_all(OUTPUT_DIR).context("failed to create output directory")?;

    loop {
        match decoder.decode::<u8>(&mut stream_reader) {
            Ok(Some(yuv_frame)) => {
                println!("decoded frame:{:?}", yuv_frame);
                let (width, height) = (decoder.width(), decoder.height());
                process_frame(yuv_frame, width as u32, height as u32)?;
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

fn process_frame(yuv_frame: MediaFrame<u8>, width: u32, height: u32) -> Result<()> {
    let rgb_frame = yuv_frame.convert_yuv_to_rgb()?;

    let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_raw(width, height, rgb_frame.data.as_slice().unwrap().to_vec())
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
