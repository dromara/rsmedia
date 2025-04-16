use rsmedia::{
    colors, filter,
    frame::MediaFrame,
    time::{self, Time},
    EncoderBuilder, PixelFormat,
};

use std::path::Path;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339())
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .init();

    rsmedia::init().unwrap();

    let filters = vec![
        filter::video::scale(1920, 1080, Some("bicubic")),
        filter::video::crop(100, 100, 1600, 900),
        filter::video::drawtext("Watermark", 50, 50, "fonts/Arial.ttf", 24, "white@0.5"),
        filter::video::hflip(),
    ];

    let width = 1280;
    let height = 720;
    let output_path = Path::new("/tmp/rainbow.mp4");

    let mut encoder = EncoderBuilder::new_video(width as usize, height as usize)
        // encoder with CUDA acceleration
        // .with_hardware_device(Some(HWDeviceType::CUDA.auto_best_config().unwrap()))
        // libx264, libx265, h264_nvenc, h264_vaapi
        // .with_codec_name(Some("h264_nvenc".to_string()))
        // .with_options(Some(Options::preset_h264_nvenc()))
        .with_filters(Some(filters))
        .build_wrapped(output_path)
        .expect("failed to create encoder");

    let duration: Time = Time::from_nth_of_a_second(24);
    let mut position = Time::zero();

    for i in 0..256 {
        // This will create a smooth rainbow animation video!
        let mut frame = rainbow_frame(width as usize, height as usize, i as f32 / 256.0);

        frame.set_pts(
            position
                .aligned_with_rational(encoder.time_base())
                .into_value()
                .unwrap(),
        );

        encoder.encode(frame)?;

        println!("Encoded frame {} at position {}", i, position);

        // Update the current position and add the inter-frame duration to it.
        position = position.aligned_with(duration).add();
    }

    encoder.finish()?;

    Ok(())
}

fn rainbow_frame(width: usize, height: usize, p: f32) -> MediaFrame<u8> {
    // This is what generated the rainbow effect!
    // We loop through the HSV color spectrum and convert to RGB.
    let rgb = colors::hsv_to_rgb(p * 360.0, 100.0, 100.0);

    // This creates a frame with height 720, width 1280 and three channels. The RGB values for each
    // pixel are equal, and determined by the `rgb` we chose above.
    let mut frame = MediaFrame::<u8>::new_video_frame(
        width,
        height,
        PixelFormat::RGB24,
        time::new_rational(1, 24),
    )
    .unwrap();
    for y in 0..height {
        for x in 0..width {
            frame.data[[y, x, 0]] = rgb[0];
            frame.data[[y, x, 1]] = rgb[1];
            frame.data[[y, x, 2]] = rgb[2];
        }
    }
    frame
}
