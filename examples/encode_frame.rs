use rsmedia::{
    colors,
    encode::EncodeResult,
    frame::FrameArray,
    io::private::{Output, Write},
    time::Time,
    EncoderBuilder, StreamWriter,
};

use anyhow::Context;
use rsmedia::stream::StreamInfo;
use std::path::Path;

fn main() {
    rsmedia::init().unwrap();

    let mut encoder = EncoderBuilder::new()
        .with_video_size(1280, 720)
        // use hwaccel cuda
        // .with_hardware_device(HWDeviceType::CUDA)
        // libx264, libx265, h264_nvenc, h264_vaapi etc.
        // .with_codec_name("h264_nvenc".to_string())
        // .with_codec_options(&Options::preset_h264_nvenc())
        .build()
        .expect("failed to create encoder");

    let output_path = Path::new("/tmp/rainbow.mp4");
    let mut stream_writer = StreamWriter::new(output_path).unwrap();
    let video_index = stream_writer.add_stream(encoder.codecpar(), encoder.time_base().into());
    let stream_info = StreamInfo::from_writer(&stream_writer, video_index).unwrap();

    // Write the header to the output file.
    stream_writer.write_header().unwrap();

    let duration: Time = Time::from_nth_of_a_second(24);
    let mut position = Time::zero();

    for i in 0..256 {
        // This will create a smooth rainbow animation video!
        let frame = rainbow_frame(i as f32 / 256.0);

        match encoder.encode(&frame, position) {
            EncodeResult::Packet(mut packet) => {
                packet.set_pos(-1);
                packet.set_stream_index(video_index as i32);
                packet.rescale_ts(encoder.time_base(), stream_info.time_base);
                stream_writer
                    .write_frame(&mut packet)
                    .context("failed to write frame")
                    .unwrap();
            }
            EncodeResult::Drain => {
                println!("Encoder drained, try send new frame again.");
                continue;
            }
            EncodeResult::Flushed => {
                println!("Encoder flushed, EOF reached.");
                break;
            }
            EncodeResult::Error(e) => {
                println!("Error encoding frame: {:?}", e);
                break;
            }
        }

        println!("Encoded frame {} at position {}", i, position);

        // Update the current position and add the inter-frame duration to it.
        position = position.aligned_with(duration).add();
    }

    encoder
        .flush(
            &mut stream_writer,
            false,
            video_index,
            stream_info.time_base,
        )
        .expect("failed to finish encoder");

    stream_writer.write_trailer().unwrap();
}

fn rainbow_frame(p: f32) -> FrameArray {
    // This is what generated the rainbow effect!
    // We loop through the HSV color spectrum and convert to RGB.
    let rgb = colors::hsv_to_rgb(p * 360.0, 100.0, 100.0);

    // This creates a frame with height 720, width 1280 and three channels. The RGB values for each
    // pixel are equal, and determined by the `rgb` we chose above.
    FrameArray::from_shape_fn((720, 1280, 3), |(_y, _x, c)| rgb[c])
}
