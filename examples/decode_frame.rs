use image::{ImageBuffer, Rgb};
use rsmedia::decode::Decoder;
use std::error::Error;
use tokio::task;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    rsmedia::init()?;

    let source = "https://img.qunliao.info/4oEGX68t_9505974551.mp4"
        .parse::<Url>()
        .unwrap();
    let mut decoder = Decoder::new(source).expect("failed to create decoder");

    let output_folder = "frames_video_rs";
    std::fs::create_dir_all(output_folder).expect("failed to create output directory");

    let (width, height) = decoder.size();
    let frame_rate = decoder.frame_rate(); // Assuming 30 FPS if not available

    let max_duration = 20.0; // Max duration in seconds
    let _max_frames = (frame_rate * max_duration).ceil() as usize;

    let mut frame_count = 0;
    let mut elapsed_time = 0.0;
    let mut tasks = vec![];

    for frame in decoder.decode_iter() {
        if let Ok((_timestamp, frame)) = frame {
            if elapsed_time > max_duration {
                break;
            }

            let rgb = frame.slice(ndarray::s![.., .., 0..3]).to_slice().unwrap();

            let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
                ImageBuffer::from_raw(width, height, rgb.to_vec())
                    .expect("failed to create image buffer");

            let frame_path = format!("{}/frame_{:05}.png", output_folder, frame_count);

            let task = task::spawn_blocking(move || {
                img.save(&frame_path).expect("failed to save frame");
            });

            tasks.push(task);

            frame_count += 1;
            elapsed_time += 1.0 / frame_rate;
        } else {
            break;
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
