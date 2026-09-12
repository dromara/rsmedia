//! Professional color capabilities.
//!
//! Demonstrates three enhancements built on the color-ecosystem crates:
//! 1. **Perceptual color difference** (CIEDE2000) via `palette`.
//! 2. **Explicit RGB -> YUV color matrices** (BT.601 / BT.709 / BT.2020) via `yuv`.
//! 3. **Colormap pseudo-color rendering** via `colorous`.

use rsmedia::{PixelFormat, colors, frame::MediaFrame, time};

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
    println!("auto   -> {:?}", auto.format());
    println!("BT709  -> {:?}", bt709.format());
    println!("BT2020 -> {:?}", bt2020.format());

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
    let heat = grayscale_to_colormap(&colorous::VIRIDIS, &gray, 0.0, 100.0);
    println!(
        "colormap frame dims: {}x{}x{}",
        heat.dim().0,
        heat.dim().1,
        heat.dim().2
    );

    Ok(())
}

/// Render an entire grayscale frame (`Array2`) to an RGB frame using a colormap.
///
/// `gray` values in `[min, max]` are mapped to colors; the result is an
/// `Array3` of shape `(height, width, 3)` suitable for constructing a
/// `MediaFrame` (RGB24) or saving as an image.
pub fn grayscale_to_colormap<T>(
    gradient: &colorous::Gradient,
    gray: &ndarray::Array2<T>,
    min: f64,
    max: f64,
) -> ndarray::Array3<u8>
where
    T: num_traits::NumCast + Copy,
{
    let (h, w) = gray.dim();
    ndarray::Array3::from_shape_fn((h, w, 3), |(y, x, c)| {
        let v = num_traits::cast::<T, f64>(gray[[y, x]]).unwrap_or(0.0);
        let (r, g, b) = colormap_lookup(gradient, v, min, max);
        [r, g, b][c]
    })
}

/// Map a scalar in `[min, max]` to an RGB color from the given colormap.
///
/// Useful for rendering grayscale / depth / heatmap data as pseudo-color
/// visualizations. Values outside `[min, max]` are clamped to the range.
pub fn colormap_lookup(
    gradient: &colorous::Gradient,
    value: f64,
    min: f64,
    max: f64,
) -> (u8, u8, u8) {
    let span = max - min;
    let t = if span <= f64::EPSILON || !span.is_finite() {
        0.0
    } else {
        ((value - min) / span).clamp(0.0, 1.0)
    };
    let steps = 255usize;
    let c = gradient.eval_rational((t * steps as f64).round() as usize, steps);
    (c.r, c.g, c.b)
}

#[test]
fn test_grayscale_to_colormap() {
    let g = colorous::MAGMA;
    let gray = ndarray::Array2::from_shape_fn((4, 5), |(y, x)| (y * 5 + x) as u8);
    let rgb = grayscale_to_colormap(&g, &gray, 0.0, 19.0);
    assert_eq!(rgb.dim(), (4, 5, 3));
    // 最暗处与最亮处的映射颜色不同
    assert_ne!(rgb[[0, 0, 0]], rgb[[3, 4, 0]]);
}

#[test]
fn test_colormap_lookup() {
    let g = colorous::VIRIDIS;
    let lo = colormap_lookup(&g, 0.0, 0.0, 1.0);
    let hi = colormap_lookup(&g, 1.0, 0.0, 1.0);
    assert_ne!(lo, hi);
    // 越界值被钳制到端点
    assert_eq!(colormap_lookup(&g, -10.0, 0.0, 1.0), lo);
    assert_eq!(colormap_lookup(&g, 99.0, 0.0, 1.0), hi);
}
