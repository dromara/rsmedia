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

    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    let value = max;

    [hue, saturation, value]
}

/// Convert HSV to RGB color space.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    [
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    ]
}

/// Calculate the distance between two colors in RGB space.
pub fn color_distance(c1: &Rgb<u8>, c2: &Rgb<u8>) -> u8 {
    ((c1[0] as i16 - c2[0] as i16).abs()
        + (c1[1] as i16 - c2[1] as i16).abs()
        + (c1[2] as i16 - c2[2] as i16).abs()) as u8
        / 3
}
