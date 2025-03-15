use anyhow::{Context, Result};
use image::{ImageBuffer, Rgb};
use rsmedia::{frame, DecoderBuilder, MediaType, Reader, Resize, StreamReader};
use tokio::task;

#[tokio::main]
async fn main() -> Result<()> {
    rsmedia::init().unwrap();

    // 640x360 mp4
    // let source = std::path::Path::new("rainbow.mp4");
    let source = "https://img.qunliao.info/4oEGX68t_9505974551.mp4"
        .parse::<url::Url>()
        .unwrap();

    let mut stream_reader = StreamReader::new(source)?;
    let mut decoder = DecoderBuilder::new()
        // use hwaccel cuda
        // .with_hardware_device(HWDeviceType::CUDA)
        // .with_codec_name("h264_cuvid".to_string())
        .with_resize(Resize::Fit(1280, 720))
        .build(&stream_reader)
        .context("failed to create decoder")?;

    let output_folder = "frames_video_rs";
    std::fs::create_dir_all(output_folder).context("failed to create output directory")?;

    let (width, height) = decoder.size();
    let frame_rate = 24.0; // Assuming 30 FPS if not available
    let mut frame_count = 0;
    let mut elapsed_time = 0.0;

    let mut tasks = vec![];

    loop {
        match stream_reader.read_packet() {
            Ok(Some((stream, mut packet))) => {
                // println!("packet: {:?}", packet);
                // 这里需要注意，reader 读取到的包是没有解码的所有通道的数据包
                // 如果是视频流，需要先判断是否是视频流，然后再decode
                if decoder.stream_index() == stream.index() {
                    let (_t, yuv_frame) = decoder.decode(&mut packet)?;
                    println!(
                        "{:?} #{}, {:?}",
                        MediaType::from(stream.parameters().codec_type),
                        stream.index(),
                        packet
                    );

                    // Notes: yuv frame
                    let rgb_frame = frame::convert_ndarray_yuv_to_rgb(&yuv_frame).unwrap();

                    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_raw(
                        width,
                        height,
                        rgb_frame.as_slice().unwrap().to_vec(),
                    )
                    .context("failed to create image buffer")?;

                    let frame_path = format!("{}/frame_{:05}.png", output_folder, frame_count);

                    let task = task::spawn_blocking(move || {
                        img.save(&frame_path).expect("failed to save frame");
                    });

                    tasks.push(task);

                    frame_count += 1;
                    elapsed_time += 1.0 / frame_rate;
                }
            }
            Ok(None) => {
                println!("No more packets");
                break;
            }
            Err(e) => {
                log::error!("Error reading packet: {}", e);
                return Err(e);
            }
        }
    }

    // Await all tasks to finish
    for task in tasks {
        task.await.expect("task failed");
    }

    println!(
        "Saved {} frames in the '{}' directory, cost: {} seconds",
        frame_count, output_folder, elapsed_time
    );

    Ok(())
}
