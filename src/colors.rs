use image::Rgb;
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
    use palette::chromatic_adaptation::AdaptInto;
    use palette::convert::{FromColorUnclamped, IntoColorUnclamped};
    use palette::{Hsl, IntoColor, Lab, LinSrgb, Oklab, Oklch, Srgb, Xyz};

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
    fn test_palette_color_conversion() {
        // Example 1: SRGB to HSL
        let srgb_color: Srgb<f32> = Srgb::new(0.8, 0.2, 0.3);
        let hsl_color = Hsl::from_color_unclamped(srgb_color);
        println!("SRGB: {:?} -> HSL: {:?}", srgb_color, hsl_color);

        // Example 2: HSL back to SRGB
        let srgb_again: Srgb<f32> = hsl_color.into_color();
        println!("HSL: {:?} -> SRGB: {:?}", hsl_color, srgb_again);

        // Example 3: SRGB to Oklab (perceptually uniform)
        let oklab_color: Oklab<f32> = srgb_color.into_color();
        println!("SRGB: {:?} -> Oklab: {:?}", srgb_color, oklab_color);

        // Example 4: Oklab to Oklch (polar version of Oklab)
        let oklch_color: Oklch<f32> = oklab_color.into_color();
        println!("Oklab: {:?} -> Oklch: {:?}", oklab_color, oklch_color);

        // Example 5: Oklch back to SRGB
        let srgb_from_oklch: Srgb<f32> = oklch_color.into_color();
        println!("Oklch: {:?} -> SRGB: {:?}", oklch_color, srgb_from_oklch);

        // Example 6: Linear SRGB to standard SRGB
        let linear_srgb: LinSrgb<f32> = LinSrgb::new(0.5, 0.5, 0.5);
        let standard_srgb: Srgb<f32> = linear_srgb.into_color();
        println!(
            "Linear SRGB: {:?} -> SRGB: {:?}",
            linear_srgb, standard_srgb
        );

        println!("\n--- Component Type Conversions ---");

        // Example 7: SRGB f32 [0.0, 1.0] to SRGB u8 [0, 255]
        let srgb_f32: Srgb<f32> = Srgb::new(0.0, 0.5, 1.0);
        let rgb_u8 = Srgb::from_color_unclamped(srgb_f32);
        println!("SRGB f32: {:?} -> SRGB u8: {:?}", srgb_f32, rgb_u8);

        // Example 8: SRGB u8 [0, 255] to HSL f32 [0.0, 1.0] / [0.0, 360.0]
        // Note: Palette handles the u8 -> f32 scaling internally during conversion
        let hsl_f32_from_u8 = Hsl::from_color_unclamped(rgb_u8);
        println!("SRGB u8: {:?} -> HSL f32: {:?}", rgb_u8, hsl_f32_from_u8);

        // Example 9: SRGB u8 to Oklab f32
        let oklab_f32_from_u8: Oklab<f32> = rgb_u8.into_color();
        println!(
            "SRGB u8: {:?} -> Oklab f32: {:?}",
            rgb_u8, oklab_f32_from_u8
        );

        println!("\n--- Using Specific Encodings/White Points (Advanced) ---");
        // For Lab/Lch/Xyz, you might need to specify the white point if not using the default (D65)
        use palette::white_point::{D50, D65};

        // Convert SRGB (implicitly D65) to Lab with a D50 whitepoint via XYZ adaptation
        //  Step 1: Srgb (D65) -> Xyz (D65)
        let xyz_d65: Xyz<D65, f32> = srgb_color.into_color_unclamped();
        println!("SRGB (D65): {:?} -> Lab (D50): {:?}", srgb_color, xyz_d65);
        //  Step 2: Xyz (D65) -> Xyz (D50)
        let xyz_d50: Xyz<D50, f32> = xyz_d65.adapt_into();
        println!("Xyz (D65): {:?} -> Lab (D50): {:?}", xyz_d65, xyz_d50);
        // Step 3: Xyz (D50) -> Lab (D50)
        let lab_d50: Lab<D50, f32> = xyz_d50.into_color_unclamped();
        println!("Xyz (D50): {:?} -> Lab (D50): {:?}", xyz_d50, lab_d50);

        // Convert Lab D50 back to SRGB f32 (implicitly D65)
        // 1. Lab(D50) -> Xyz(D50)
        let xyz_d50_from_lab: Xyz<D50, f32> = lab_d50.into_color_unclamped();
        println!(
            "Lab (D50): {:?} -> Xyz (D50): {:?}",
            lab_d50, xyz_d50_from_lab
        );
        // 2. Xyz(D50) -> Xyz(D65) (Adapt back)
        let xyz_d65_from_d50: Xyz<D65, f32> = xyz_d50_from_lab.adapt_into();
        println!(
            "Xyz (D50): {:?} -> Xyz (D65): {:?}",
            xyz_d50_from_lab, xyz_d65_from_d50
        );
        // 3. Xyz(D65) -> Srgb(D65)
        let srgb_from_lab_d50: Srgb<f32> = xyz_d65_from_d50.into_color_unclamped();
        println!(
            "Xyz (D65): {:?} -> SRGB (D65): {:?}",
            xyz_d65_from_d50, srgb_from_lab_d50
        );
    }
}
