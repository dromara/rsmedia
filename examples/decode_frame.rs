use image::{ImageBuffer, Rgb};

use rsmedia::{
    decode::DecodeResult, frame, DecoderBuilder, FrameArray, MediaType, Reader, Resize,
    StreamReader,
};

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
    rsmedia::init().unwrap();

    // let source = std::path::Path::new("/tmp/bear.mp4");

    // 640x360 mp4
    let source = "https://img.qunliao.info/4oEGX68t_9505974551.mp4"
        .parse::<url::Url>()
        .unwrap();

    let mut stream_reader = StreamReader::new(source)?;
    let mut decoder = DecoderBuilder::new()
        .with_resize(Some(Resize::Fit(1280, 720)))
        .build(&stream_reader)
        .context("failed to create decoder")?;

    std::fs::create_dir_all(OUTPUT_DIR).context("failed to create output directory")?;

    loop {
        match stream_reader.read_packet() {
            Ok(Some((stream, mut packet))) => {
                // println!("packet: {:?}", packet);
                // 这里需要注意，reader 读取到的包是没有解码的所有通道的数据包
                // 如果是视频流，需要先判断是否是视频流，然后再decode
                if decoder.stream_index() == stream.index() {
                    // 注意时间转换
                    packet.rescale_ts(stream.time_base(), decoder.time_base());

                    match decoder.decode(&packet) {
                        DecodeResult::Frame((_t, yuv_frame)) => {
                            println!(
                                "{:?} #{}, {:?}",
                                MediaType::from(stream.parameters().codec_type),
                                stream.index(),
                                packet
                            );

                            let (width, height) = decoder.size();
                            process_frame(yuv_frame, width, height)?;
                        }
                        DecodeResult::Drain => {
                            println!("Need more data for decoding");
                            continue;
                        }
                        DecodeResult::Flushed => {
                            println!("EOF reached, stopping decoding");
                            break;
                        }
                        DecodeResult::Error(e) => {
                            println!("Error decoding frame: {}", e);
                            break;
                        }
                    }
                } else {
                    println!("Packet for stream {} discarded", stream.index());
                }
            }
            Ok(None) => {
                println!("No more packets, Reader exhausted.");
                break;
            }
            Err(e) => {
                log::error!("Error on reading packet: {}", e);
                return Err(e);
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

fn process_frame(yuv_frame: FrameArray, width: u32, height: u32) -> Result<()> {
    let rgb_frame = frame::convert_ndarray_yuv_to_rgb(&yuv_frame).unwrap();

    let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_raw(width, height, rgb_frame.as_slice().unwrap().to_vec())
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
