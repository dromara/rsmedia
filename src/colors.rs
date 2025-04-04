use ab_glyph::PxScale;
use image::{GenericImage, ImageBuffer, Rgb};
use palette::{FromColor, IntoColor};
use rayon::prelude::*;
use std::collections::HashMap;

pub fn create_color_map() -> HashMap<String, Rgb<u8>> {
    let mut map = HashMap::new();

    // 基础颜色
    map.insert("red".to_string(), Rgb([255, 0, 0]));
    map.insert("green".to_string(), Rgb([0, 255, 0]));
    map.insert("blue".to_string(), Rgb([0, 0, 255]));
    map.insert("yellow".to_string(), Rgb([255, 255, 0]));
    map.insert("cyan".to_string(), Rgb([0, 255, 255]));
    map.insert("magenta".to_string(), Rgb([255, 0, 255]));
    map.insert("white".to_string(), Rgb([255, 255, 255]));
    map.insert("black".to_string(), Rgb([0, 0, 0]));

    // 金属色系
    map.insert("silver".to_string(), Rgb([192, 192, 192]));
    map.insert("gold".to_string(), Rgb([255, 215, 0]));
    map.insert("bronze".to_string(), Rgb([205, 127, 50]));
    map.insert("platinum".to_string(), Rgb([229, 228, 226]));

    // 灰度
    map.insert("gray".to_string(), Rgb([128, 128, 128]));
    map.insert("dark_gray".to_string(), Rgb([64, 64, 64]));
    map.insert("light_gray".to_string(), Rgb([192, 192, 192]));

    // 自然色系
    map.insert("brown".to_string(), Rgb([165, 42, 42]));
    map.insert("beige".to_string(), Rgb([245, 245, 220]));
    map.insert("ivory".to_string(), Rgb([255, 255, 240]));
    map.insert("coral".to_string(), Rgb([255, 127, 80]));
    map.insert("salmon".to_string(), Rgb([250, 128, 114]));

    // 深色系
    map.insert("navy".to_string(), Rgb([0, 0, 128]));
    map.insert("maroon".to_string(), Rgb([128, 0, 0]));
    map.insert("olive".to_string(), Rgb([128, 128, 0]));
    map.insert("teal".to_string(), Rgb([0, 128, 128]));
    map.insert("purple".to_string(), Rgb([128, 0, 128]));

    // 浅色系
    map.insert("pink".to_string(), Rgb([255, 192, 203]));
    map.insert("lavender".to_string(), Rgb([230, 230, 250]));
    map.insert("mint".to_string(), Rgb([189, 252, 201]));
    map.insert("peach".to_string(), Rgb([255, 218, 185]));
    map.insert("sky_blue".to_string(), Rgb([135, 206, 235]));

    // 特殊色
    map.insert("turquoise".to_string(), Rgb([64, 224, 208]));
    map.insert("indigo".to_string(), Rgb([75, 0, 130]));
    map.insert("violet".to_string(), Rgb([238, 130, 238]));
    map.insert("crimson".to_string(), Rgb([220, 20, 60]));
    map.insert("chartreuse".to_string(), Rgb([127, 255, 0]));

    map
}

/// Create an image with the given text and a gradient color.
pub fn create_image_with_text(
    width: u32,
    height: u32,
    text: &str,
) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let mut img = ImageBuffer::new(width, height);

    // create a gradient color
    for y in 0..height {
        let hue = (y as f32 / height as f32) * 360.0;
        let color = palette::Hsl::new(hue, 0.8, 0.5);
        let rgb: palette::Srgb = color.into_color();

        for x in 0..width {
            img.put_pixel(
                x,
                y,
                Rgb([
                    (rgb.red * 255.0) as u8,
                    (rgb.green * 255.0) as u8,
                    (rgb.blue * 255.0) as u8,
                ]),
            );
        }
    }

    let font = ab_glyph::FontArc::try_from_slice(include_bytes!("../fonts/Arial.ttf"))
        .map_err(|e| format!("Failed to load font: {}", e))
        .unwrap();

    // add text to the image
    imageproc::drawing::draw_text_mut(
        &mut img,
        Rgb([255, 255, 255]),
        10,
        10,
        PxScale::from(24.0),
        &font,
        text,
    );

    img
}

/// Apply a color processing function to an image.
///
/// # Arguments
///
/// * `P`: Pixel type (e.g. Rgb<u8>, Rgba<u8>, Luma<u8> etc.)
/// * `C`: container type for the image pixels (e.g. Vec<u8>)
/// * `F`: color processing function that takes a Srgb<f32> and returns a Srgb<f32>
/// * `I`: image type (e.g. ImageBuffer<Rgb<u8>, Vec<u8>>)
///
/// # Example
///
/// ```rust, ignore
/// use image::{ImageBuffer, RgbImage};
/// use palette::{Srgb, Hsv, Hsl, IntoColor};
///
/// let mut rgb_img: RgbImage = ImageBuffer::new(800, 600);
/// rsmedia::colors::image_processing(&mut rgb_img, |rgb| {
///         let mut hsl: Hsl = rgb.into_color();
///         hsl.saturation *= 1.2;
///         hsl.into_color()
///     });///
/// ```
pub fn image_processing<P, C, F, I>(image: &mut I, process_color: F)
where
    P: image::Pixel<Subpixel = u8> + Send + Sync + 'static,
    C: Clone + Send + Sync,
    F: Fn(C) -> C + Send + Sync,
    I: GenericImage<Pixel = P> + Send + Sync,
    // SRGB 支持
    palette::Srgb<f32>: FromColor<C>,
    palette::Srgba<f32>: FromColor<C>,
    C: FromColor<palette::Srgb<f32>>,
    C: FromColor<palette::Srgba<f32>>,
    // LinSrgb 支持
    palette::LinSrgb<f32>: FromColor<C>,
    palette::LinSrgba<f32>: FromColor<C>,
    C: FromColor<palette::LinSrgb<f32>>,
    C: FromColor<palette::LinSrgba<f32>>,
    // GammaSrgb 支持
    palette::GammaSrgb<f32>: FromColor<C>,
    palette::GammaSrgba<f32>: FromColor<C>,
    C: FromColor<palette::GammaSrgb<f32>>,
    C: FromColor<palette::GammaSrgba<f32>>,
{
    let (width, height) = image.dimensions();

    // 创建坐标和像素的映射
    let mut pixel_map: Vec<((u32, u32), P)> = Vec::with_capacity((width * height) as usize);

    for y in 0..height {
        for x in 0..width {
            let pixel = image.get_pixel(x, y);
            pixel_map.push(((x, y), pixel));
        }
    }

    // 并行处理像素
    let processed_pixels: Vec<((u32, u32), P)> = pixel_map
        .into_par_iter()
        .map(|((x, y), pixel)| {
            // 根据通道数选择合适的颜色转换
            let channels = pixel.channels();
            let color = match channels.len() {
                1 => {
                    // 灰度图像
                    let v = channels[0] as f32 / 255.0;
                    palette::Srgb::new(v, v, v).into_color()
                }
                3 => {
                    // RGB图像
                    palette::Srgb::new(
                        channels[0] as f32 / 255.0,
                        channels[1] as f32 / 255.0,
                        channels[2] as f32 / 255.0,
                    )
                    .into_color()
                }
                4 => {
                    // RGBA图像
                    palette::Srgba::new(
                        channels[0] as f32 / 255.0,
                        channels[1] as f32 / 255.0,
                        channels[2] as f32 / 255.0,
                        channels[3] as f32 / 255.0,
                    )
                    .into_color()
                }
                // White pixel default
                _ => {
                    log::warn!("Unsupported pixel format: {}", channels.len());
                    palette::Srgb::new(1.0, 1.0, 1.0).into_color()
                }
            };

            // 处理颜色
            let processed = process_color(color);

            // 转换回原始像素格式
            let mut new_pixel = pixel;
            let channels = new_pixel.channels_mut();

            match channels.len() {
                1 => {
                    // 灰度图像
                    let srgb: palette::Srgb<f32> = processed.into_color();
                    let gray =
                        (srgb.red * 0.2126 + srgb.green * 0.7152 + srgb.blue * 0.0722) * 255.0;
                    channels[0] = gray.clamp(0.0, 255.0) as u8;
                }
                3 => {
                    // RGB图像
                    let srgb: palette::Srgb<f32> = processed.into_color();
                    channels[0] = (srgb.red * 255.0).clamp(0.0, 255.0) as u8;
                    channels[1] = (srgb.green * 255.0).clamp(0.0, 255.0) as u8;
                    channels[2] = (srgb.blue * 255.0).clamp(0.0, 255.0) as u8;
                }
                4 => {
                    // RGBA图像
                    let srgba: palette::Srgba<f32> = processed.into_color();
                    channels[0] = (srgba.color.red * 255.0).clamp(0.0, 255.0) as u8;
                    channels[1] = (srgba.color.green * 255.0).clamp(0.0, 255.0) as u8;
                    channels[2] = (srgba.color.blue * 255.0).clamp(0.0, 255.0) as u8;
                    channels[3] = (srgba.alpha * 255.0).clamp(0.0, 255.0) as u8;
                }
                // do nothing
                _ => (),
            }

            ((x, y), new_pixel)
        })
        .collect();

    // 更新图像
    for ((x, y), pixel) in processed_pixels {
        image.put_pixel(x, y, pixel);
    }
}

/// Convert RGB to HSV color space.
pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> [f32; 3] {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;

    let max = r.max(g.max(b));
    let min = r.min(g.min(b));
    let delta = max - min;

    const EPSILON: f32 = 1e-10;

    let h = if delta < EPSILON {
        0.0
    } else if (max - r).abs() < EPSILON {
        60.0 * (((g - b) / delta + 6.0) % 6.0)
    } else if (max - g).abs() < EPSILON {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    // Ensures a range of 0-360 degrees
    let h = if h < 0.0 { h + 360.0 } else { h % 360.0 };
    // Saturation and value
    let s = if max < EPSILON {
        0.0
    } else {
        (delta / max * 100.0).clamp(0.0, 100.0)
    };
    let v = (max * 100.0).clamp(0.0, 100.0);

    [h, s, v]
}

/// Convert HSV to RGB color space.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = h % 360.0; // H limited to 0-360
    let s = s.clamp(0.0, 100.0); // S limited to 0-100
    let v = v.clamp(0.0, 100.0); // V limited to 0-100

    let s = s / 100.0;
    let v = v / 100.0;
    let c = s * v;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        5 => (c, 0.0, x),
        _ => (0.0, 0.0, 0.0),
    };

    [
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    ]
}

/// Calculate the distance between two colors in RGB space.
pub fn color_distance(c1: &Rgb<u8>, c2: &Rgb<u8>) -> u8 {
    ((c1[0] as i16 - c2[0] as i16).abs()
        + (c1[1] as i16 - c2[1] as i16).abs()
        + (c1[2] as i16 - c2[2] as i16).abs()) as u8
        / 3
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GrayImage, Rgb, RgbImage, Rgba, RgbaImage};

    const OUTPUT_DIR: &str = "output";

    #[ctor::ctor]
    fn before() {
        std::fs::create_dir_all(OUTPUT_DIR).unwrap();
    }

    #[ctor::dtor]
    fn after() {
        std::fs::remove_dir_all(OUTPUT_DIR).unwrap();
    }

    /// allow floating point error comparison
    macro_rules! assert_approx_eq {
        ($a:expr, $b:expr) => {
            assert!(($a - $b).abs() < f32::EPSILON, "{} ≈ {}", $a, $b);
        };
        ($a:expr, $b:expr, $eps:expr) => {
            assert!(($a - $b).abs() < $eps, "{} ≈ {}", $a, $b);
        };
    }

    #[test]
    fn test_pure_colors() {
        // 红色
        let [h, s, v] = rgb_to_hsv(255, 0, 0);
        assert_approx_eq!(h, 0.0);
        assert_approx_eq!(s, 100.0);
        assert_approx_eq!(v, 100.0);
        assert_eq!(hsv_to_rgb(h, s, v), [255, 0, 0]);

        // 绿色
        let [h, s, v] = rgb_to_hsv(0, 255, 0);
        assert_approx_eq!(h, 120.0);
        assert_approx_eq!(s, 100.0);
        assert_approx_eq!(v, 100.0);
        assert_eq!(hsv_to_rgb(h, s, v), [0, 255, 0]);

        // 蓝色
        let [h, s, v] = rgb_to_hsv(0, 0, 255);
        assert_approx_eq!(h, 240.0);
        assert_approx_eq!(s, 100.0);
        assert_approx_eq!(v, 100.0);
        assert_eq!(hsv_to_rgb(h, s, v), [0, 0, 255]);
    }

    #[test]
    fn test_grayscale() {
        // 纯黑
        let [h, s, v] = rgb_to_hsv(0, 0, 0);
        assert_approx_eq!(s, 0.0);
        assert_approx_eq!(v, 0.0);
        assert_eq!(hsv_to_rgb(h, s, v), [0, 0, 0]);

        // 纯白
        let [h, s, v] = rgb_to_hsv(255, 255, 255);
        assert_approx_eq!(s, 0.0);
        assert_approx_eq!(v, 100.0);
        assert_eq!(hsv_to_rgb(h, s, v), [255, 255, 255]);

        // 中灰
        let [h, s, v] = rgb_to_hsv(128, 128, 128);
        assert_approx_eq!(s, 0.0);
        assert_approx_eq!(v, 50.196078, 0.1); // 128/255 ≈ 50.196%
        assert_eq!(hsv_to_rgb(h, s, v), [128, 128, 128]);
    }

    #[test]
    fn test_special_colors() {
        // 黄色 (R+G)
        let [h, s, v] = rgb_to_hsv(255, 255, 0);
        assert_approx_eq!(h, 60.0);
        assert_approx_eq!(s, 100.0);
        assert_approx_eq!(v, 100.0);
        assert_eq!(hsv_to_rgb(h, s, v), [255, 255, 0]);

        // 品红色 (R+B)
        let [h, s, v] = rgb_to_hsv(255, 0, 255);
        assert_approx_eq!(h, 300.0);
        assert_approx_eq!(s, 100.0);
        assert_approx_eq!(v, 100.0);
        assert_eq!(hsv_to_rgb(h, s, v), [255, 0, 255]);
    }

    #[test]
    fn test_round_trip() {
        let test_colors = [
            [123, 45, 67],  // 随机颜色
            [255, 128, 0],  // 橙色
            [75, 200, 220], // 青色系
            [30, 150, 80],  // 绿色系
        ];

        for &[r, g, b] in &test_colors {
            let [h, s, v] = rgb_to_hsv(r, g, b);
            let [r2, g2, b2] = hsv_to_rgb(h, s, v);

            // 允许 ±1 的误差（因浮点舍入）
            assert!((r as i32 - r2 as i32).abs() <= 1);
            assert!((g as i32 - g2 as i32).abs() <= 1);
            assert!((b as i32 - b2 as i32).abs() <= 1);
        }
    }

    #[test]
    fn test_precision() {
        let test_cases = [
            (255, 0, 0),     // 纯红
            (0, 255, 0),     // 纯绿
            (0, 0, 255),     // 纯蓝
            (128, 128, 128), // 灰色
            (255, 255, 0),   // 黄色
            (255, 0, 255),   // 洋红
            (0, 255, 255),   // 青色
        ];

        for (r, g, b) in test_cases.iter() {
            let hsv_f32 = rgb_to_hsv(*r, *g, *b);
            println!("RGB({}, {}, {})", r, g, b);
            println!(
                "HSV f32: [{:.6}, {:.6}, {:.6}]",
                hsv_f32[0], hsv_f32[1], hsv_f32[2]
            );
        }
    }

    #[test]
    fn test_boundary_conditions() {
        // 测试超范围输入规范化
        assert_eq!(
            hsv_to_rgb(361.0, 110.0, 120.0), // 输入超出范围
            hsv_to_rgb(1.0, 100.0, 100.0)    // 预期等价于规范化后的值
        );

        // 测试负值输入规范化
        assert_eq!(
            hsv_to_rgb(-90.0, -50.0, -10.0), // 输入负值
            hsv_to_rgb(270.0, 0.0, 0.0)      // 预期等价于 (360-90)=270, 饱和度/明度归零
        );

        // 测试 V=0 时的输出
        assert_eq!(hsv_to_rgb(180.0, 50.0, 0.0), [0, 0, 0]); // V=0 必须输出黑色
        assert_eq!(hsv_to_rgb(0.0, 100.0, 0.0), [0, 0, 0]); // V=0 必须输出黑色

        // 测试接近零的值
        let hsv = rgb_to_hsv(1, 0, 0);
        assert!(hsv[1] > 0.0 && hsv[1] <= 100.0);

        // 测试近似相等的值
        let hsv = rgb_to_hsv(128, 128, 127);
        assert!(hsv[1] >= 0.0 && hsv[1] <= 100.0);
    }

    #[test]
    fn test_image_text() {
        let rgb = create_image_with_text(640, 480, "Hello, world!");
        rgb.save(format!("{}/image_with_text.png", OUTPUT_DIR))
            .unwrap()
    }

    #[test]
    fn test_image_processing_rgb() {
        // 创建一个简单的RGB测试图像
        let mut img = RgbImage::new(100, 100);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = Rgb([x as u8, y as u8, 100]);
        }

        // 增加亮度的处理函数
        let brighten = |color: palette::Srgb<f32>| {
            let mut color = color;
            color.red = (color.red * 1.2).min(1.0);
            color.green = (color.green * 1.2).min(1.0);
            color.blue = (color.blue * 1.2).min(1.0);
            color
        };

        // 处理图像
        image_processing(&mut img, brighten);

        // 保存结果
        img.save(format!("{}/output_rgb_brightness.png", OUTPUT_DIR))
            .unwrap();
    }

    #[test]
    fn test_image_processing_rgba() {
        // 创建一个简单的RGBA测试图像
        let mut img = RgbaImage::new(100, 100);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = image::Rgba([x as u8, y as u8, 100, 200]);
        }

        // 色相偏移处理函数
        let hue_shift = |color: palette::Srgba<f32>| {
            // 转换为HSL进行处理
            let hsl = palette::Hsl::from_color(color.color);
            // 色相偏移180度
            let shifted_hsl = palette::Hsl::new(hsl.hue + 180.0, hsl.saturation, hsl.lightness);
            // 转换回Srgb并保留原始alpha
            let new_rgb = palette::Srgb::from_color(shifted_hsl);
            palette::Srgba::new(new_rgb.red, new_rgb.green, new_rgb.blue, color.alpha)
        };

        // 处理图像
        image_processing(&mut img, hue_shift);

        // 保存结果
        img.save(format!("{}/output_rgba_hue_shift.png", OUTPUT_DIR))
            .unwrap();
    }

    #[test]
    fn test_image_processing_gray() {
        // 创建一个简单的灰度测试图像
        let mut img = GrayImage::new(100, 100);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = image::Luma([(x + y) as u8]);
        }

        // 反转处理函数
        let invert = |color: palette::LinSrgb<f32>| {
            palette::LinSrgb::new(1.0 - color.red, 1.0 - color.green, 1.0 - color.blue)
        };

        // 处理图像
        image_processing(&mut img, invert);

        // 保存结果
        img.save(format!("{}/output_gray_invert.png", OUTPUT_DIR))
            .unwrap();
    }

    #[test]
    fn test_image_processing_origin() {
        // 创建RGB图像
        let mut img = RgbImage::new(300, 300);

        // 填充彩色渐变
        for x in 0..300 {
            for y in 0..300 {
                let r = (x as f32 / 300.0 * 255.0) as u8;
                let g = (y as f32 / 300.0 * 255.0) as u8;
                let b = ((x + y) as f32 / 600.0 * 255.0) as u8;
                img.put_pixel(x, y, Rgb([r, g, b]));
            }
        }

        // 增加饱和度50%
        image_processing(&mut img, |color: palette::Srgb<f32>| {
            let mut hsv = palette::Hsv::from_color(color);
            hsv.saturation = (hsv.saturation * 1.5).min(1.0);
            palette::Srgb::from_color(hsv)
        });

        img.save(format!("{}/saturated.png", OUTPUT_DIR)).unwrap();

        // 对比原图
        let mut original = RgbImage::new(300, 300);
        for x in 0..300 {
            for y in 0..300 {
                let r = (x as f32 / 300.0 * 255.0) as u8;
                let g = (y as f32 / 300.0 * 255.0) as u8;
                let b = ((x + y) as f32 / 600.0 * 255.0) as u8;
                original.put_pixel(x, y, Rgb([r, g, b]));
            }
        }
        original
            .save(format!("{}/original.png", OUTPUT_DIR))
            .unwrap();
    }

    #[test]
    fn test_image_processing_brightness_gray() {
        // 创建10x10灰度图像
        let mut img = GrayImage::new(10, 10);

        // 填充灰度值
        for x in 0..10 {
            for y in 0..10 {
                img.put_pixel(x, y, image::Luma([127]));
            }
        }

        // 亮度增加50%
        image_processing(&mut img, |color: palette::Srgb<f32>| {
            let mut c = color;
            c.red = (c.red * 1.5).min(1.0);
            c.green = (c.green * 1.5).min(1.0);
            c.blue = (c.blue * 1.5).min(1.0);
            c
        });

        // 验证结果 - 应该接近 190 (127 * 1.5 限制在 255 以内)
        let result_pixel = img.get_pixel(5, 5);
        assert!(result_pixel[0] > 180 && result_pixel[0] < 200);

        // 保存结果
        img.save(format!("{}/brightness_gray.png", OUTPUT_DIR))
            .unwrap();
    }

    #[test]
    fn test_image_processing_hue_rotation() {
        // 创建RGB图像，红色
        let mut img = RgbImage::new(100, 100);
        for x in 0..100 {
            for y in 0..100 {
                img.put_pixel(x, y, image::Rgb([255, 0, 0]));
            }
        }

        // 色相旋转120度 (红色->绿色)
        image_processing(&mut img, |color: palette::Srgb<f32>| {
            // 转到HSL进行色相调整
            let mut hsl = palette::Hsl::from_color(color);
            hsl.hue = hsl.hue + 120.0;
            palette::Srgb::from_color(hsl)
        });

        // 验证结果 - 应该变成绿色 (0,255,0)附近
        let result_pixel = img.get_pixel(50, 50);
        assert!(result_pixel[0] < 50); // R接近0
        assert!(result_pixel[1] > 200); // G接近255
        assert!(result_pixel[2] < 50); // B接近0

        img.save(format!("{}/hue_rotation.png", OUTPUT_DIR))
            .unwrap();
    }

    #[test]
    fn test_image_processing_alpha_adjustment() {
        // 创建半透明蓝色RGBA图像
        let mut img = RgbaImage::new(100, 100);
        for x in 0..100 {
            for y in 0..100 {
                img.put_pixel(x, y, Rgba([0, 0, 255, 128]));
            }
        }

        // 降低50%透明度
        image_processing(&mut img, |color: palette::Srgba<f32>| {
            let mut c = color;
            c.alpha = c.alpha * 0.5;
            c
        });

        // 验证结果 - alpha应变为约64 (128 * 0.5)
        let result_pixel = img.get_pixel(50, 50);
        assert_eq!(result_pixel[0], 0); // R不变
        assert_eq!(result_pixel[1], 0); // G不变
        assert_eq!(result_pixel[2], 255); // B不变
        assert!(result_pixel[3] > 60 && result_pixel[3] < 68); // A约为64

        img.save(format!("{}/alpha_adjustment.png", OUTPUT_DIR))
            .unwrap();
    }

    #[test]
    fn test_rgb_processing() {
        // 创建测试图像
        let mut img = RgbImage::new(100, 100);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = Rgb([x as u8, y as u8, 100]);
        }

        // 使用Srgb处理 - 增加红色通道
        image_processing(&mut img, |color: palette::Srgb<f32>| {
            let mut new_color = color;
            new_color.red = (color.red * 1.5).min(1.0);
            new_color
        });

        img.save(format!("{}/output_rgb.png", OUTPUT_DIR)).unwrap();
    }

    #[test]
    fn test_hsl_processing() {
        let mut img = RgbImage::new(100, 100);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = Rgb([x as u8, y as u8, 100]);
        }

        // 使用HSL处理 - 旋转色相
        image_processing(&mut img, |color: palette::Srgb<f32>| {
            let mut color: palette::Hsl = palette::Hsl::from_color(color);
            color.hue = color.hue + 180.0; // 色相旋转180度
            palette::Srgb::from_color(color)
        });

        img.save(format!("{}/output_hsl.png", OUTPUT_DIR)).unwrap();
    }

    #[test]
    fn test_linear_rgb_processing() {
        let mut img = RgbImage::new(100, 100);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = Rgb([x as u8, y as u8, 100]);
        }

        // 使用线性RGB处理 - 在线性空间中混合颜色
        image_processing(&mut img, |color: palette::LinSrgb<f32>| {
            // 在线性空间中，颜色混合更准确
            let red = palette::LinSrgb::new(1.0, 0.0, 0.0);
            let mix_factor = 0.3;
            color * (1.0 - mix_factor) + red * mix_factor
        });

        img.save(format!("{}/output_linear_rgb.png", OUTPUT_DIR))
            .unwrap();
    }

    #[test]
    fn test_rgba_processing() {
        let mut img = RgbaImage::new(100, 100);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = Rgba([x as u8, y as u8, 100, 200]);
        }

        // 使用RGBA处理 - 调整透明度
        image_processing(&mut img, |color: palette::Srgba<f32>| {
            let mut new_color = color;
            // 基于位置调整透明度
            new_color.alpha = color.alpha * 0.8;
            new_color
        });

        img.save(format!("{}/output_rgba.png", OUTPUT_DIR)).unwrap();
    }

    #[test]
    fn test_gray_processing() {
        let mut img = GrayImage::new(100, 100);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = image::Luma([(x + y) as u8]);
        }

        // 使用Srgb处理灰度图像 - 增加对比度
        image_processing(&mut img, |color: palette::Srgb<f32>| {
            let color = color;
            // 增加对比度
            let gray = color.red; // 灰度图像RGB通道相同
            let contrast = 1.5;
            let new_gray = 0.5 + (gray - 0.5) * contrast;
            let new_gray = new_gray.max(0.0).min(1.0);

            palette::Srgb::new(new_gray, new_gray, new_gray)
        });

        img.save(format!("{}/output_gray.png", OUTPUT_DIR)).unwrap();
    }

    #[test]
    fn test_custom_processing() {
        let mut img = RgbImage::new(100, 100);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = Rgb([x as u8, y as u8, 100]);
        }

        // 自定义处理函数 - 创建渐变效果
        image_processing(&mut img, |color: palette::Srgb<f32>| {
            // 根据颜色的红色和绿色通道创建渐变
            let t = (color.red + color.green) / 2.0;
            let start_color = palette::Srgb::new(0.0, 0.0, 1.0); // 蓝色
            let end_color = palette::Srgb::new(1.0, 0.0, 0.0); // 红色

            start_color * (1.0 - t) + end_color * t
        });

        img.save(format!("{}/output_custom_gradient.png", OUTPUT_DIR))
            .unwrap();
    }

    #[test]
    fn test_large_image_processing() {
        let width = 4000;
        let height = 3000;
        let mut img = image::DynamicImage::new_rgb8(width, height);

        // 创建一个简单的渐变
        for y in 0..height {
            for x in 0..width {
                let r = (x as f32 / width as f32 * 255.0) as u8;
                let g = (y as f32 / height as f32 * 255.0) as u8;
                let b = 128u8;
                img.put_pixel(x, y, Rgba([r, g, b, 255]));
            }
        }

        // 测量处理时间
        let start = std::time::Instant::now();

        // 执行一个简单的处理
        image_processing(&mut img, |color: palette::Srgb<f32>| {
            let mut hsv = palette::Hsv::from_color(color);
            hsv.saturation *= 1.2;
            hsv.value *= 1.1;
            palette::Srgb::from_color(hsv)
        });

        let duration = start.elapsed();
        println!("large_image_processing time cost: {:?}", duration);
    }
}
