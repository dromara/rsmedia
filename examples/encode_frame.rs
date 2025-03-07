use rsmedia::colors;
use rsmedia::hwaccel::HWDeviceType;
use rsmedia::time::Time;
use rsmedia::{EncoderBuilder, FrameArray, Options};
use std::path::Path;

fn main() {
    rsmedia::init().unwrap();

    let mut encoder = EncoderBuilder::new(Path::new("rainbow.mp4"), 1280, 720)
        .with_format("mp4")
        // libx264, h264_nvenc, h264_vaapi, h264_videotoolbox
        .with_codec_name("h264_nvenc".to_string())
        .with_codec_options(&Options::preset_h264_nvenc())
        .with_hardware_device(HWDeviceType::CUDA)
        .build()
        .expect("failed to create encoder");

    let duration: Time = Time::from_nth_of_a_second(24);
    let mut position = Time::zero();

    for i in 0..256 {
        // This will create a smooth rainbow animation video!
        let frame = rainbow_frame(i as f32 / 256.0);

        encoder
            .encode(&frame, position)
            .expect("failed to encode frame");
        println!("Encoded frame {} at position {:?}", i, position);

        // Update the current position and add the inter-frame duration to it.
        position = position.aligned_with(duration).add();
    }

    encoder.finish().expect("failed to finish encoder");
}

fn rainbow_frame(p: f32) -> FrameArray {
    // This is what generated the rainbow effect!
    // We loop through the HSV color spectrum and convert to RGB.
    let rgb = colors::hsv_to_rgb(p * 360.0, 1.0, 1.0);

    // This creates a frame with height 720, width 1280 and three channels. The RGB values for each
    // pixel are equal, and determined by the `rgb` we chose above.
    FrameArray::from_shape_fn((720, 1280, 3), |(_y, _x, c)| rgb[c])
}
