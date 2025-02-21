use anyhow::Context;
use image::{ImageBuffer, Rgb};
use rsmedia::{frame, DecoderBuilder};
use std::error::Error;
use tokio::task;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    rsmedia::init()?;

    let source = "https://img.qunliao.info/4oEGX68t_9505974551.mp4"
        .parse::<url::Url>()
        .unwrap();
    // let source = std::path::Path::new("rainbow.mp4");
    let mut decoder = DecoderBuilder::new(source)
        // .with_hardware_device(HWDeviceType::VAAPI)
        .build()
        .context("failed to create decoder")?;

    let output_folder = "frames_video_rs";
    std::fs::create_dir_all(output_folder).context("failed to create output directory")?;

    let (width, height) = decoder.size();
    let frame_rate = decoder.frame_rate(); // Assuming 30 FPS if not available

    let max_duration = 20.0; // Max duration in seconds
    let _max_frames = (frame_rate * max_duration).ceil() as usize;

    let mut frame_count = 0;
    let mut elapsed_time = 0.0;

    let mut tasks = vec![];

    for frame_result in decoder.decode_iter() {
        match frame_result {
            Ok((_timestamp, yuv_frame)) => {
                if elapsed_time > max_duration {
                    break;
                }

                // Notes: yuv frame
                let rgb_frame = frame::convert_ndarray_yuv_to_rgb(&yuv_frame).unwrap();

                let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
                    ImageBuffer::from_raw(width, height, rgb_frame.as_slice().unwrap().to_vec())
                        .context("failed to create image buffer")?;

                let frame_path = format!("{}/frame_{:05}.png", output_folder, frame_count);

                let task = task::spawn_blocking(move || {
                    img.save(&frame_path).expect("failed to save frame");
                });

                tasks.push(task);

                frame_count += 1;
                elapsed_time += 1.0 / frame_rate;
            }
            Err(e) => {
                // AV ERROR(-541478725): `End of file` is EOF
                println!("Error decoding frame: {}", e);
                break;
            }
        }
    }

    // Await all tasks to finish
    for task in tasks {
        task.await.expect("task failed");
    }

    println!(
        "Saved {} frames in the '{}' directory",
        frame_count, output_folder
    );
    Ok(())
}
