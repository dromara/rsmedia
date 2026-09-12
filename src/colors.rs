//! Color utilities built on top of [`colorous`] and [`colorutils_rs`].
//!
//! [`colorous::Color`] is a plain `{ r, g, b: u8 }` struct with no alpha and no
//! room for helper methods (it is a foreign type, so inherent impls and orphan
//! trait impls are rejected by the compiler). This module therefore defines its
//! own `Color` — same channel fields **plus** an alpha channel — and bridges to
//! colorous with lossless `From` conversions, so gradient/palette output
//! converts with a single `.into()`.

use std::fmt::{LowerHex, UpperHex};

/// An RGBA color.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// extend
impl Color {
    /// Creates a color from the three channels (alpha = `0xFF`).
    #[inline]
    #[must_use]
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 0xFF }
    }

    /// Creates a color from all four channels.
    #[inline]
    #[must_use]
    pub const fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Returns `[r, g, b, a]`.
    #[inline]
    #[must_use]
    pub const fn as_array(&self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Returns `(r, g, b, a)`.
    #[inline]
    #[must_use]
    pub const fn as_tuple(&self) -> (u8, u8, u8, u8) {
        (self.r, self.g, self.b, self.a)
    }

    /// Returns `(r, g, b, a)`.
    #[inline]
    #[must_use]
    pub const fn rgba(&self) -> (u8, u8, u8, u8) {
        self.as_tuple()
    }

    /// Returns `(r, g, b)`.
    #[inline]
    #[must_use]
    pub const fn rgb(&self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }

    /// Red channel accessor (kept for API compatibility with the previous
    /// tuple-based implementation).
    #[inline]
    #[must_use]
    pub const fn r(&self) -> u8 {
        self.r
    }

    /// Green channel accessor.
    #[inline]
    #[must_use]
    pub const fn g(&self) -> u8 {
        self.g
    }

    /// Blue channel accessor.
    #[inline]
    #[must_use]
    pub const fn b(&self) -> u8 {
        self.b
    }

    /// Alpha channel accessor.
    #[inline]
    #[must_use]
    pub const fn a(&self) -> u8 {
        self.a
    }

    /// Returns the same color with a new alpha value.
    #[inline]
    #[must_use]
    pub const fn with_alpha(mut self, a: u8) -> Self {
        self.a = a;
        self
    }

    /// Formats the color as `#RRGGBBAA` (`uppercase` selects the letter case).
    #[inline]
    #[must_use]
    pub fn hex(&self, uppercase: bool) -> String {
        if uppercase {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    /// HSV
    pub fn from_hsv(h: u16, s: u16, v: u16) -> Self {
        let rgb = colorutils_rs::Hsv::new(h, s, v).to_rgb8();
        Self::from_rgb(rgb.r, rgb.g, rgb.b)
    }

    pub fn to_hsv(&self) -> colorutils_rs::Hsv {
        let (r, g, b, _a) = self.as_tuple();
        colorutils_rs::Rgb::<u8>::new(r, g, b).to_hsv()
    }

    /// HSL
    pub fn from_hsl(h: u16, s: u16, l: u16) -> Self {
        let rgb = colorutils_rs::Hsl::new(h, s, l).to_rgb8();
        Self::from_rgb(rgb.r, rgb.g, rgb.b)
    }

    pub fn to_hsl(&self) -> colorutils_rs::Hsl {
        let (r, g, b, _a) = self.as_tuple();
        colorutils_rs::Rgb::<u8>::new(r, g, b).to_hsl()
    }

    /// LAB
    pub fn from_lab(l: f32, a: f32, b: f32) -> Self {
        let rgb = colorutils_rs::Lab::new(l, a, b).to_rgb8();
        Self::from_rgb(rgb.r, rgb.g, rgb.b)
    }

    pub fn to_lab(&self) -> colorutils_rs::Lab {
        let (r, g, b, _a) = self.as_tuple();
        colorutils_rs::Rgb::<u8>::new(r, g, b).to_lab()
    }

    /// XYB
    pub fn from_xyb(x: f32, y: f32, b: f32) -> Self {
        let rgb = colorutils_rs::Xyb::new(x, y, b).to_rgb(colorutils_rs::TransferFunction::Srgb);
        Self::from_rgb(rgb.r, rgb.g, rgb.b)
    }

    pub fn to_xyb(&self) -> colorutils_rs::Xyb {
        let (r, g, b, _a) = self.as_tuple();
        let rgb = colorutils_rs::Rgb::<u8>::new(r, g, b);
        colorutils_rs::Xyb::from_rgb(rgb, colorutils_rs::TransferFunction::Srgb)
    }

    /// XYZ
    pub fn from_xyz(x: f32, y: f32, z: f32) -> Self {
        let rgb = colorutils_rs::Xyz::new(x, y, z).to_srgb();
        Self::from_rgb(rgb.r, rgb.g, rgb.b)
    }

    pub fn to_xyz(&self) -> colorutils_rs::Xyz {
        let (r, g, b, _a) = self.as_tuple();
        let rgb = colorutils_rs::Rgb::<u8>::new(r, g, b);
        colorutils_rs::Xyz::from_srgb(rgb)
    }
}

impl LowerHex for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{:02x}{:02x}{:02x}{:02x}",
            self.r, self.g, self.b, self.a
        )
    }
}

impl UpperHex for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{:02X}{:02X}{:02X}{:02X}",
            self.r, self.g, self.b, self.a
        )
    }
}

/// convert
impl From<u32> for Color {
    /// `0xRRGGBBAA` packing (what [`Color::hex`] emits).
    fn from(x: u32) -> Self {
        Self::from_rgba((x >> 24) as u8, (x >> 16) as u8, (x >> 8) as u8, x as u8)
    }
}

impl From<(u8, u8, u8)> for Color {
    fn from((r, g, b): (u8, u8, u8)) -> Self {
        Self::from_rgb(r, g, b)
    }
}

impl From<[u8; 3]> for Color {
    fn from(c: [u8; 3]) -> Self {
        Self::from((c[0], c[1], c[2]))
    }
}

impl From<(u8, u8, u8, u8)> for Color {
    fn from((r, g, b, a): (u8, u8, u8, u8)) -> Self {
        Self::from_rgba(r, g, b, a)
    }
}

impl From<[u8; 4]> for Color {
    fn from(c: [u8; 4]) -> Self {
        Self::from((c[0], c[1], c[2], c[3]))
    }
}

impl From<Color> for (u8, u8, u8, u8) {
    fn from(color: Color) -> Self {
        color.rgba()
    }
}

impl From<Color> for [u8; 4] {
    fn from(color: Color) -> Self {
        color.as_array()
    }
}

impl From<Color> for (u8, u8, u8) {
    fn from(color: Color) -> Self {
        color.rgb()
    }
}

impl From<Color> for [u8; 3] {
    fn from(color: Color) -> Self {
        let (r, g, b) = color.rgb();
        [r, g, b]
    }
}

/// Lossless RGB conversion from a colorous gradient/palette output
/// (`TURBO.eval_rational(..).into()`); alpha is set to fully opaque.
impl From<colorous::Color> for Color {
    fn from(c: colorous::Color) -> Self {
        Self::from_rgb(c.r, c.g, c.b)
    }
}

/// Converts back into colorous (e.g. to feed other colorous APIs); the alpha
/// channel is dropped, since colorous colors carry no alpha.
impl From<Color> for colorous::Color {
    fn from(c: Color) -> Self {
        Self {
            r: c.r,
            g: c.g,
            b: c.b,
        }
    }
}

impl TryFrom<&str> for Color {
    type Error = &'static str;

    /// Parses `#RRGGBBAA` (the leading `#` is optional; a 6-digit value is
    /// treated as fully opaque).
    fn try_from(x: &str) -> std::result::Result<Self, Self::Error> {
        let hex = x.trim_start_matches('#');
        let hex = match hex.len() {
            6 => format!("{hex}ff"),
            8 => hex.to_string(),
            _ => return Err("Failed to convert `Color` from str: invalid length"),
        };

        u32::from_str_radix(&hex, 16)
            .map(Self::from)
            .map_err(|_| "Failed to convert `Color` from str: invalid hex")
    }
}

/// Declares a named [`Color`] constant from an `(r, g, b)` tuple (alpha
/// defaults to fully opaque). An optional 4th element sets the alpha.
///
/// The tuple form keeps the color tables below compact; each entry expands to
/// `Color::from_rgb(..)` / `Color::from_rgba(..)`, so the constants stay
/// evaluable at compile time.
macro_rules! color {
    ($name:ident, ($r:expr, $g:expr, $b:expr)) => {
        pub const $name: Color = Color::from_rgb($r, $g, $b);
    };
    ($name:ident, ($r:expr, $g:expr, $b:expr, $a:expr)) => {
        pub const $name: Color = Color::from_rgba($r, $g, $b, $a);
    };
}

// rainbow
color!(RED, (255, 0, 0));
color!(ORANGE, (255, 165, 0));
color!(YELLOW, (255, 255, 0));
color!(GREEN, (0, 128, 0));
color!(BLUE, (0, 0, 255));
color!(INDIGO, (75, 0, 130));
color!(VIOLET, (238, 130, 238));
// base
color!(PURPLE, (128, 0, 128));
color!(MAGENTA, (255, 0, 255));
color!(CYAN, (0, 255, 255));
color!(LIME, (0, 255, 0));
color!(TEAL, (0, 128, 128));
color!(BLACK, (0, 0, 0));
color!(WHITE, (255, 255, 255));
color!(GRAY, (128, 128, 128));
color!(SILVER, (192, 192, 192));
color!(MAROON, (128, 0, 0));
color!(OLIVE, (128, 128, 0));
color!(NAVY, (0, 0, 128));
// extension
color!(PINK, (255, 192, 203));
color!(BROWN, (165, 42, 42));
color!(GOLD, (255, 215, 0));
color!(TURQUOISE, (64, 224, 208));
color!(LAVENDER, (230, 230, 250));
color!(CORAL, (255, 127, 80));
color!(SALMON, (250, 128, 114));
color!(CRIMSON, (220, 20, 60));
color!(KHAKI, (240, 230, 140));
color!(PLUM, (221, 160, 221));

/// Convert RGB to HSV color space.
///
/// 返回 `[h, s, v]`，其中 `h` 为 0-360 度，`s`/`v` 为 0-100
pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> [f32; 3] {
    let hsv = colorutils_rs::Rgb::<u8>::new(r, g, b).to_hsv();
    [hsv.h, hsv.s * 100.0, hsv.v * 100.0]
}

/// Convert HSV to RGB color space.
///
/// 输入 `h` 为 0-360 度，`s`/`v` 为 0-100
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let rgb = colorutils_rs::Hsv::new(h as u16, s as u16, v as u16).to_rgb8();
    [rgb.r, rgb.g, rgb.b]
}

/// Calculate the Euclidean distance between two colors in RGB space.
pub fn color_distance(c1: &Color, c2: Color) -> f32 {
    let dr = (c1.r() as f32 - c2.r() as f32).powi(2);
    let dg = (c1.g() as f32 - c2.g() as f32).powi(2);
    let db = (c1.b() as f32 - c2.b() as f32).powi(2);
    (dr + dg + db).sqrt()
}

/// Calculate the perceptual color difference (CIEDE2000) in CIE Lab space.
///
/// Unlike the plain RGB [`color_distance`], CIEDE2000 is aligned with human
/// color perception, making it suitable for quantitative video-quality
/// assessment, watermark-removal evaluation, and palette discrimination.
///
/// Reference thresholds: `<1` imperceptible, `1~2` subtle, `2~10` noticeable,
/// `>10` very different.
pub fn color_delta_e(c1: &Color, c2: Color) -> f32 {
    use palette::color_difference::Ciede2000;
    use palette::{IntoColor, Lab, Srgb};

    let lab1: Lab = Srgb::new(
        c1.r() as f32 / 255.0,
        c1.g() as f32 / 255.0,
        c1.b() as f32 / 255.0,
    )
    .into_color();
    let lab2: Lab = Srgb::new(
        c2.r() as f32 / 255.0,
        c2.g() as f32 / 255.0,
        c2.b() as f32 / 255.0,
    )
    .into_color();
    lab1.difference(lab2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use palette::chromatic_adaptation::AdaptIntoUnclamped;
    use palette::convert::{FromColorUnclamped, IntoColorUnclamped};
    use palette::{Hsl, IntoColor, Lab, LinSrgb, Oklab, Oklch, Srgb, Xyz};

    #[test]
    fn test_color_to_hex() {
        let color = Color::from_rgb(255, 0, 0);
        assert_eq!(color.hex(true), "#FF0000FF");

        let color = Color::from_rgb(0, 255, 0);
        assert_eq!(color.hex(true), "#00FF00FF");

        let color = Color::from_rgb(0, 0, 255);
        assert_eq!(color.hex(true), "#0000FFFF");

        let color = Color::from_rgb(255, 255, 0);
        assert_eq!(color.hex(false), "#ffff00ff");
    }

    #[test]
    fn test_hex_to_color() {
        assert_eq!(
            Color::try_from("#FF0000").unwrap(),
            Color::from_rgb(255, 0, 0)
        );
        assert_eq!(
            Color::try_from("00FF00").unwrap(),
            Color::from_rgb(0, 255, 0)
        );
        assert_eq!(
            Color::try_from("#0000FF").unwrap(),
            Color::from_rgb(0, 0, 255)
        );
    }

    #[test]
    fn test_color_constants() {
        assert_eq!(RED, Color::from_rgb(255, 0, 0));
        assert_eq!(GREEN, Color::from_rgb(0, 128, 0));
        assert_eq!(BLUE, Color::from_rgb(0, 0, 255));
    }

    #[test]
    fn test_color_macro_tuple_and_rgba_forms() {
        // 3-element form: opaque alpha.
        assert_eq!(CRIMSON, Color::from_rgba(220, 20, 60, 0xFF));
        // 4-element form: explicit alpha.
        color!(GHOST, (248, 248, 255, 128));
        assert_eq!(GHOST, Color::from_rgba(248, 248, 255, 128));
        assert_eq!(GHOST.a, 128);
    }

    #[test]
    fn test_rgb_hsv_known_values() {
        // (r, g, b, 期望 h/s/v)
        let cases: &[(u8, u8, u8, f32, f32, f32)] = &[
            (255, 0, 0, 0.0, 100.0, 100.0),     // 纯红
            (0, 255, 0, 120.0, 100.0, 100.0),   // 纯绿
            (0, 0, 255, 240.0, 100.0, 100.0),   // 纯蓝
            (255, 255, 0, 60.0, 100.0, 100.0),  // 黄
            (255, 0, 255, 300.0, 100.0, 100.0), // 品红
            (0, 255, 255, 180.0, 100.0, 100.0), // 青
            (0, 0, 0, 0.0, 0.0, 0.0),           // 纯黑
            (255, 255, 255, 0.0, 0.0, 100.0),   // 纯白
            (128, 128, 128, 0.0, 0.0, 50.0),    // 中灰
        ];

        for &(r, g, b, eh, es, ev) in cases {
            let [h, s, v] = rgb_to_hsv(r, g, b);
            assert!(
                (h - eh).abs() < 2.0,
                "Hue mismatch for RGB({r},{g},{b}): {h} vs {eh}"
            );
            assert!(
                (s - es).abs() <= 2.0,
                "S mismatch for RGB({r},{g},{b}): {s} vs {es}"
            );
            assert!(
                (v - ev).abs() <= 2.0,
                "V mismatch for RGB({r},{g},{b}): {v} vs {ev}"
            );

            // 往返转换允许 ±2 的取整误差
            let [r2, g2, b2] = hsv_to_rgb(h, s, v);
            assert!(
                (r as i32 - r2 as i32).abs() <= 2
                    && (g as i32 - g2 as i32).abs() <= 2
                    && (b as i32 - b2 as i32).abs() <= 2,
                "roundtrip mismatch for RGB({r},{g},{b}): [{r2},{g2},{b2}]"
            );
        }
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

            // 允许 ±3 的误差（浮点舍入 + u8 整型截断）
            assert!((r as i32 - r2 as i32).abs() <= 3);
            assert!((g as i32 - g2 as i32).abs() <= 3);
            assert!((b as i32 - b2 as i32).abs() <= 3);
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
            // HSV 输出必须落在合法范围内
            assert!(
                (0.0..360.0).contains(&hsv_f32[0]),
                "Hue out of range for RGB({}, {}, {}): {}",
                r,
                g,
                b,
                hsv_f32[0]
            );
            assert!(
                (0.0..=100.0).contains(&hsv_f32[1]),
                "Saturation out of range for RGB({}, {}, {}): {}",
                r,
                g,
                b,
                hsv_f32[1]
            );
            assert!(
                (0.0..=100.0).contains(&hsv_f32[2]),
                "Value out of range for RGB({}, {}, {}): {}",
                r,
                g,
                b,
                hsv_f32[2]
            );
        }
    }

    #[test]
    fn test_boundary_conditions() {
        // V=0 时必须输出黑色（无论色相/饱和度）
        assert_eq!(hsv_to_rgb(180.0, 50.0, 0.0), [0, 0, 0]);
        assert_eq!(hsv_to_rgb(0.0, 100.0, 0.0), [0, 0, 0]);

        // 接近零的值，饱和度应为正且不越界
        let hsv = rgb_to_hsv(1, 0, 0);
        assert!(hsv[1] > 0.0 && hsv[1] <= 100.0);

        // 近似相等的值，饱和度应落在 [0, 100]
        let hsv = rgb_to_hsv(128, 128, 127);
        assert!(hsv[0] >= 0.0 && hsv[0] <= 360.0);
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
        let xyz_d50: Xyz<D50, f32> = xyz_d65.adapt_into_unclamped();
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
        let xyz_d65_from_d50: Xyz<D65, f32> = xyz_d50_from_lab.adapt_into_unclamped();
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

    #[test]
    fn test_color_delta_e() {
        // 相同颜色 CIEDE2000 色差近似为 0
        let a = Color::from_rgb(120, 80, 200);
        assert!(color_delta_e(&a, a) < 0.01);

        // 明显不同的颜色（黑 vs 白）色差应非常大
        let black = Color::from_rgb(0, 0, 0);
        let white = Color::from_rgb(255, 255, 255);
        let de = color_delta_e(&black, white);
        assert!(de > 50.0, "black vs white deltaE should be large, got {de}");
    }
}
