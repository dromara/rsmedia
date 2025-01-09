use ndarray::Array3;
use rsmedia::encode::{Encoder, Settings};
use rsmedia::time::Time;
use std::path::Path;

fn main() {
    rsmedia::init().unwrap();

    let settings = Settings::preset_h264_yuv420p(1280, 720, false);
    let mut encoder =
        Encoder::new(Path::new("rainbow.mp4"), settings).expect("failed to create encoder");

    let duration: Time = Time::from_nth_of_a_second(24);
    let mut position = Time::zero();
    for i in 0..256 {
        // This will create a smooth rainbow animation video!
        let frame = rainbow_frame(i as f32 / 256.0);

        encoder
            .encode(&frame, position)
            .expect("failed to encode frame");

        // Update the current position and add the inter-frame duration to it.
        position = position.aligned_with(duration).add();
    }

    encoder.finish().expect("failed to finish encoder");
}

fn rainbow_frame(p: f32) -> Array3<u8> {
    // This is what generated the rainbow effect! We loop through the HSV color spectrum and convert
    // to RGB.
    let rgb = hsv_to_rgb(p * 360.0, 100.0, 100.0);

    // This creates a frame with height 720, width 1280 and three channels. The RGB values for each
    // pixel are equal, and determined by the `rgb` we chose above.
    Array3::from_shape_fn((720, 1280, 3), |(_y, _x, c)| rgb[c])
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let s = s / 100.0;
    let v = v / 100.0;
    let c = s * v;
    let x = c * (1.0 - (((h / 60.0) % 2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if (0.0..60.0).contains(&h) {
        (c, x, 0.0)
    } else if (60.0..120.0).contains(&h) {
        (x, c, 0.0)
    } else if (120.0..180.0).contains(&h) {
        (0.0, c, x)
    } else if (180.0..240.0).contains(&h) {
        (0.0, x, c)
    } else if (240.0..300.0).contains(&h) {
        (x, 0.0, c)
    } else if (300.0..360.0).contains(&h) {
        (c, 0.0, x)
    } else {
        (0.0, 0.0, 0.0)
    };
    [
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    ]
}
