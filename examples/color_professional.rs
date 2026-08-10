//! Professional color capabilities.
//!
//! Demonstrates three enhancements built on the color-ecosystem crates:
//! 1. **Perceptual color difference** (CIEDE2000) via `palette`.
//! 2. **Explicit RGB -> YUV color matrices** (BT.601 / BT.709 / BT.2020) via `yuv`.
//! 3. **Colormap pseudo-color rendering** via `colorous`.

use rsmedia::{colors, frame::MediaFrame, time, PixelFormat};

fn main() -> anyhow::Result<()> {
    // 1. Perceptual color difference (CIEDE2000)
    let black = colors::Color::from_rgb(0, 0, 0);
    let white = colors::Color::from_rgb(255, 255, 255);
    let red = colors::Color::from_rgb(255, 0, 0);
    println!(
        "CIEDE2000: black vs white = {:.2}, black vs red = {:.2}",
        colors::color_delta_e(&black, white),
        colors::color_delta_e(&black, red),
    );

    // 2. Explicit RGB -> YUV color matrices
    const W: usize = 320;
    const H: usize = 180;
    let mut rgb =
        MediaFrame::<u8>::new_video_frame(W, H, PixelFormat::RGB24, time::new_rational(1, 30))?;
    for y in 0..H {
        for x in 0..W {
            let t = (x as f32 / W as f32 * 255.0) as u8;
            rgb.data[[y, x, 0]] = t;
            rgb.data[[y, x, 1]] = 128;
            rgb.data[[y, x, 2]] = 255 - t;
        }
    }
    let auto = rgb.convert_rgb_to_yuv()?; // SD resolution -> BT.601 (automatic)
    let bt709 = rgb.convert_rgb_to_yuv_with_matrix(yuv::YuvStandardMatrix::Bt709)?;
    let bt2020 = rgb.convert_rgb_to_yuv_with_matrix(yuv::YuvStandardMatrix::Bt2020)?;
    println!("auto   -> {:?}", auto.video_format());
    println!("BT709  -> {:?}", bt709.video_format());
    println!("BT2020 -> {:?}", bt2020.video_format());

    // 3. Colormap pseudo-color rendering (viridis)
    use ndarray::Array2;
    let mut gray = Array2::<f32>::zeros((200, 200));
    for y in 0..200 {
        for x in 0..200 {
            let dx = x as f32 - 100.0;
            let dy = y as f32 - 100.0;
            gray[[y, x]] = (dx * dx + dy * dy).sqrt().min(100.0);
        }
    }
    let heat = colors::grayscale_to_colormap(&colorous::VIRIDIS, &gray, 0.0, 100.0);
    println!(
        "colormap frame dims: {}x{}x{}",
        heat.dim().0,
        heat.dim().1,
        heat.dim().2
    );

    Ok(())
}
