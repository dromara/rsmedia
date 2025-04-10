use crate::colors;
use crate::PixelFormat;

use anyhow::Error;
use image::DynamicImage;
use rsmpeg::ffi;

/// Processing-friendly image structure
/// with separate channels in ARGB or RGB order
/// in linear color space with alpha premultiplied.
///
/// If you are not using the [image] library,
/// you will have to implement the conversion
/// to this structure and back to your image
/// format yourself.
#[derive(Clone, Debug)]
pub struct FullImage {
    pub width: usize,
    pub height: usize,
    pub has_alpha: bool,
    pub channels: Vec<ImageChannel>,
}

impl FullImage {
    pub fn new(width: usize, height: usize, channel_count: usize, has_alpha: bool) -> Self {
        Self {
            width,
            height,
            has_alpha,
            channels: vec![ImageChannel::new(width, height); channel_count],
        }
    }
}

#[allow(clippy::identity_op)]
impl From<&DynamicImage> for FullImage {
    fn from(input: &DynamicImage) -> Self {
        let (width, height) = (input.width() as usize, input.height() as usize);
        let has_alpha = input.color().has_alpha();
        let channel_count = if has_alpha { 4 } else { 3 };
        let mut output = FullImage::new(width, height, channel_count, has_alpha);
        if has_alpha {
            let data = input.to_rgba32f().into_vec();
            for i in 0..width * height {
                let r = data[i * 4 + 0];
                let g = data[i * 4 + 1];
                let b = data[i * 4 + 2];
                let a = data[i * 4 + 3];
                output.channels[0].data[i] = a;
                output.channels[1].data[i] = colors::srgb_to_lrgb(r) * a;
                output.channels[2].data[i] = colors::srgb_to_lrgb(g) * a;
                output.channels[3].data[i] = colors::srgb_to_lrgb(b) * a;
            }
        } else {
            let data = input.to_rgb32f().into_vec();
            for i in 0..width * height {
                let r = data[i * 3 + 0];
                let g = data[i * 3 + 1];
                let b = data[i * 3 + 2];
                output.channels[0].data[i] = colors::srgb_to_lrgb(r);
                output.channels[1].data[i] = colors::srgb_to_lrgb(g);
                output.channels[2].data[i] = colors::srgb_to_lrgb(b);
            }
        }
        output
    }
}

#[allow(clippy::identity_op)]
impl From<&FullImage> for DynamicImage {
    fn from(input: &FullImage) -> Self {
        if input.channels.len() == 3 && !input.has_alpha {
            let mut buf = vec![0; input.width * input.height * 3];
            for i in 0..input.width * input.height {
                buf[i * 3 + 0] = colors::lrgb_to_srgb8(input.channels[0].data[i]);
                buf[i * 3 + 1] = colors::lrgb_to_srgb8(input.channels[1].data[i]);
                buf[i * 3 + 2] = colors::lrgb_to_srgb8(input.channels[2].data[i]);
            }
            DynamicImage::ImageRgb8(
                image::RgbImage::from_raw(input.width as u32, input.height as u32, buf).unwrap(),
            )
        } else if input.channels.len() == 4 && input.has_alpha {
            let mut buf = vec![0; input.width * input.height * 4];
            for i in 0..input.width * input.height {
                let a = input.channels[0].data[i] + f32::EPSILON;
                let r = input.channels[1].data[i];
                let g = input.channels[2].data[i];
                let b = input.channels[3].data[i];
                buf[i * 4 + 0] = colors::lrgb_to_srgb8(r / a);
                buf[i * 4 + 1] = colors::lrgb_to_srgb8(g / a);
                buf[i * 4 + 2] = colors::lrgb_to_srgb8(b / a);
                buf[i * 4 + 3] = (a * 255.0 + 0.5) as u8;
            }
            DynamicImage::ImageRgba8(
                image::RgbaImage::from_raw(input.width as u32, input.height as u32, buf).unwrap(),
            )
        } else {
            panic!("This is not ARGB or RGB image");
        }
    }
}

impl From<FullImage> for DynamicImage {
    fn from(input: FullImage) -> Self {
        (&input).into()
    }
}

/// Separate color channel, dimensions must match the entire image
#[derive(Clone, Debug)]
pub struct ImageChannel {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

#[derive(Copy, Clone, Default)]
struct Area([f32; 9]);

impl Area {
    fn map<F: FnMut(f32) -> f32>(&self, f: &mut F) -> Self {
        let mut out = Self::default();
        for i in 0..9 {
            out.0[i] = f(self.0[i]);
        }
        out
    }
    fn zip_map<F: FnMut(f32, f32) -> f32>(&self, other: &Self, f: &mut F) -> Self {
        let mut out = Self::default();
        for i in 0..9 {
            out.0[i] = f(self.0[i], other.0[i]);
        }
        out
    }
    fn center(&self) -> f32 {
        self.0[4]
    }
    fn borders(&self) -> f32 {
        self.0[1] + self.0[3] + self.0[5] + self.0[7]
    }
    fn corners(&self) -> f32 {
        self.0[0] + self.0[2] + self.0[6] + self.0[8]
    }
    fn integral(&self) -> f32 {
        (self.center() * 36.0 + self.borders() * 6.0 + self.corners()) / 64.0
    }
    fn get(&self, x: usize, y: usize) -> f32 {
        self.0[y * 3 + x]
    }
}

impl ImageChannel {
    pub fn new(width: usize, height: usize) -> Self {
        ImageChannel {
            width,
            height,
            data: vec![0.0; width * height],
        }
    }
    pub fn get(&self, x: usize, y: usize) -> f32 {
        self.data[y * self.width + x]
    }
    fn get_area(&self, x: usize, y: usize) -> Area {
        let x = [x.saturating_sub(1), x, (x + 1).min(self.width - 1)];
        let y = [y.saturating_sub(1), y, (y + 1).min(self.height - 1)];

        Area([
            self.get(x[0], y[0]),
            self.get(x[1], y[0]),
            self.get(x[2], y[0]),
            self.get(x[0], y[1]),
            self.get(x[1], y[1]),
            self.get(x[2], y[1]),
            self.get(x[0], y[2]),
            self.get(x[1], y[2]),
            self.get(x[2], y[2]),
        ])
    }
    pub fn set(&mut self, x: usize, y: usize, value: f32) {
        self.data[y * self.width + x] = value
    }
}

fn sharp(
    input_channel: &ImageChannel,
    target: &ImageChannel,
    alpha_channel: Option<&ImageChannel>,
    output_channel: &mut ImageChannel,
) {
    for y in 0..input_channel.height {
        for x in 0..input_channel.width {
            let area = input_channel.get_area(x, y);

            let target = target.get(x, y);
            let borders = area.borders();
            let corners = area.corners();
            let result = (target * 64.0 - borders * 6.0 - corners) / 36.0;

            let max = if let Some(alpha_channel) = alpha_channel {
                alpha_channel.get(x, y)
            } else {
                1.0
            };
            let result = result.clamp(0.0, max);

            output_channel.set(x, y, result);
        }
    }
}

fn adjust_3x3(target: f32, area: Area, alpha: Area) -> Area {
    let current = area.integral();
    if current > target {
        let k = target / current;
        return area.map(&mut |v| v * k);
    }

    let max = alpha.integral();
    if max > current {
        let k = (target - current) / (max - current);
        return area.zip_map(&alpha, &mut |v, a| v * (1.0 - k) + a * k);
    }

    area
}

#[derive(Clone, Debug)]
struct Segment {
    output_index: usize,
    interpolation_factor: f32,
    size: f32,
}

#[derive(Clone, Debug)]
struct IntersectedPixels([Vec<Segment>; 2]);

impl IntersectedPixels {
    fn new(old: usize, new: usize, inp_idx: usize) -> Self {
        let old_div_new = old as f32 / new as f32;
        let new_div_old = 1.0 / old_div_new;

        let before_center = {
            // (idx * new / old).floor()
            let start = (inp_idx * new) / old;
            // ((idx + 0.5) * new / old).ceil()
            // ((idx + 1) * new / (old * 2)).ceil()
            let end = ((inp_idx * 2 + 1) * new).div_ceil(old * 2);

            (start..end)
                .map(|out_idx| {
                    let segment_start = out_idx as f32 * old_div_new;
                    let segment_end = segment_start + old_div_new;

                    let segment_start = segment_start.max(inp_idx as f32);
                    let segment_end = segment_end.min(inp_idx as f32 + 0.5);

                    let size = (segment_end - segment_start) * new_div_old;

                    let center = (segment_start + segment_end) * 0.5;
                    let interpolation_factor = center + 0.5 - inp_idx as f32;

                    Segment {
                        output_index: out_idx,
                        interpolation_factor,
                        size,
                    }
                })
                .collect()
        };

        let after_center = {
            // ((idx + 0.5) * new / old).floor()
            // ((idx + 1) * new / (old * 2)).floor()
            let start = ((inp_idx * 2 + 1) * new) / (old * 2);
            // ((idx + 1) * new / old).ceil()
            let end = ((inp_idx + 1) * new).div_ceil(old);

            (start..end)
                .map(|out_idx| {
                    let segment_start = out_idx as f32 * old_div_new; // 0
                    let segment_end = segment_start + old_div_new; // 2

                    let segment_start = segment_start.max(inp_idx as f32 + 0.5);
                    let segment_end = segment_end.min(inp_idx as f32 + 1.0);

                    let size = (segment_end - segment_start) * new_div_old;

                    let center = (segment_start + segment_end) * 0.5;
                    let lerp_k = center - 0.5 - inp_idx as f32;

                    Segment {
                        output_index: out_idx,
                        interpolation_factor: lerp_k,
                        size,
                    }
                })
                .collect()
        };

        IntersectedPixels([before_center, after_center])
    }
}

fn bilinear_interpolation(a: f32, b: f32, c: f32, d: f32, tx: f32, ty: f32) -> f32 {
    let ab = (b - a) * tx + a;
    let cd = (d - c) * tx + c;
    (cd - ab) * ty + ab
}

/// Performs image resampling.
///
/// `input` - something implementing the [Into]<[FullImage]> trait,
/// this trait is implemented for [image::DynamicImage];
///
/// `width` and `height` are the dimensions of the output image, must not be zero;
///
/// the output type is [FullImage], so the `into()` method must be called to convert it to [image::DynamicImage].
///
/// Usage:
/// ```rust,ignore
/// # let (width, height) = (1, 1);
/// let input_image = image::open("input.png").unwrap();
/// let resized_image: image::DynamicImage = imgutils::resize(&input_image, width, height).into();
/// resized_image.save("output.png").unwrap();
/// ```
pub fn resize(input: impl Into<FullImage>, width: usize, height: usize) -> FullImage {
    assert!(width > 0, "image width must > 0");
    assert!(height > 0, "image height muse > 0");

    let input = input.into();
    let mut sharpened_image = input.clone();
    let mut temp_channel = sharpened_image.channels[0].clone();

    let steps = 8;
    // independed channels
    for z in 0..input.channels.len() {
        if input.has_alpha && z != 0 {
            continue;
        }
        for _ in 0..steps {
            let target = &input.channels[z];
            let current_channel = &mut sharpened_image.channels[z];
            sharp(current_channel, target, None, &mut temp_channel);
            sharp(&temp_channel, target, None, current_channel);
        }
    }
    // alpha depended channels
    if input.has_alpha {
        for z in 1..input.channels.len() {
            for _ in 0..steps {
                let target = &input.channels[z];
                let current_channel = &mut sharpened_image.channels[z];
                let max = Some(&input.channels[0]);
                sharp(current_channel, target, max, &mut temp_channel);
                sharp(&temp_channel, target, max, current_channel);
            }
        }
    }

    let intersected_by_x: Vec<_> = (0..input.width)
        .map(|inp_idx| IntersectedPixels::new(input.width, width, inp_idx))
        .collect();

    let intersected_by_y: Vec<_> = (0..input.height)
        .map(|inp_idx| IntersectedPixels::new(input.height, height, inp_idx))
        .collect();

    let mut output_image = FullImage::new(width, height, input.channels.len(), input.has_alpha);

    for (y, intersected_y) in intersected_by_y.iter().enumerate().take(input.height) {
        for (x, intersected_x) in intersected_by_x.iter().enumerate().take(input.width) {
            let mut alpha_area = Area([1.0; 9]);

            for z in 0..input.channels.len() {
                let area = sharpened_image.channels[z].get_area(x, y);
                let target = input.channels[z].get(x, y);
                let area = adjust_3x3(target, area, alpha_area);
                if input.has_alpha && z == 0 {
                    alpha_area = area;
                }
                for h in 0..2 {
                    for w in 0..2 {
                        for y_segment in intersected_y.0[h].iter() {
                            for x_segment in intersected_x.0[w].iter() {
                                #[allow(clippy::identity_op)]
                                let result = bilinear_interpolation(
                                    area.get(0 + w, 0 + h),
                                    area.get(1 + w, 0 + h),
                                    area.get(0 + w, 1 + h),
                                    area.get(1 + w, 1 + h),
                                    x_segment.interpolation_factor,
                                    y_segment.interpolation_factor,
                                ) * y_segment.size
                                    * x_segment.size;
                                let x = x_segment.output_index;
                                let y = y_segment.output_index;
                                let old = output_image.channels[z].get(x, y);
                                output_image.channels[z].set(x, y, old + result);
                            }
                        }
                    }
                }
            }
        }
    }

    output_image
}

/// Fill plane linesizes for an image with pixel format pix_fmt and width width.
///
/// # Arguments
///
/// * `pix_fmt` - The pixel format of the image.
/// * `width` - The width of the image in pixels.
///
/// Returns an array of four integers representing the linesizes for each plane of the image.
pub fn fill_linesizes(pix_fmt: PixelFormat, width: i32) -> anyhow::Result<[i32; 4]> {
    let mut linesizes = [0; 4];
    let ret =
        unsafe { ffi::av_image_fill_linesizes(linesizes.as_mut_ptr(), pix_fmt.into(), width) };

    // >= 0 in case of success, a negative error code otherwise
    if ret < 0 {
        return Err(Error::msg(format!("Failed to fill linesizes: {}", ret)));
    }

    Ok(linesizes)
}

/// Compute the size of an image line with format pix_fmt and width width for the plane plane.
///
/// # Arguments
/// * `pix_fmt` - The pixel format of the image.
/// * `width` - The width of the image in pixels.
/// * `plane` - The index of the plane to compute the size for.
///
/// Returns The size of the image line in bytes for the specified plane.
pub fn get_linesize(pix_fmt: PixelFormat, width: u32, plane: usize) -> anyhow::Result<usize> {
    // Safe because format is a valid format and this function is pure computation.
    let ret = unsafe { ffi::av_image_get_linesize(pix_fmt.into(), width as _, plane as _) };

    // returns the computed size in bytes
    if ret <= 0 {
        return Err(Error::msg(format!("Failed to get line size, ret: {}", ret)));
    }

    Ok(ret as usize)
}

/// Fill plane sizes for an image with pixel format pix_fmt and height height.
///
/// # Arguments
///
/// * `format` - The pixel format of the image.
/// * `linesizes` - An iterator of the linesizes for each plane of the image.
/// * `height` - The height of the image in pixels.
///
/// Returns an array to be filled with the size of each image plane
pub fn fill_plane_sizes<I: IntoIterator<Item = u32>>(
    format: PixelFormat,
    linesizes: I,
    height: u32,
) -> anyhow::Result<Vec<usize>> {
    const MAX_FFMPEG_PLANES: usize = 4;

    let mut linesizes_buf = [0; MAX_FFMPEG_PLANES];
    let mut planes = 0;
    for (i, linesize) in linesizes.into_iter().take(MAX_FFMPEG_PLANES).enumerate() {
        linesizes_buf[i] = linesize as _;
        planes += 1;
    }
    let mut plane_sizes_buf = [0; MAX_FFMPEG_PLANES];

    // Safe because plane_sizes_buf and linesizes_buf have the size specified by the API, format is
    // valid, and this function doesn't have any side effects other than writing to plane_sizes_buf.
    let ret = unsafe {
        ffi::av_image_fill_plane_sizes(
            plane_sizes_buf.as_mut_ptr(),
            format.into(),
            height as _,
            linesizes_buf.as_ptr(),
        )
    };

    // >= 0 in case of success, a negative error code otherwise
    if ret < 0 {
        return Err(Error::msg(format!(
            "Failed to fill plane sizes, ret: {}",
            ret
        )));
    }

    Ok(plane_sizes_buf
        .into_iter()
        .map(|x| x as _)
        .take(planes)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_image_linesize_planar() -> Result<()> {
        // --------------------------
        // 测试用例1: YUV420P 格式
        // --------------------------
        // 输入：3个平面（Y/U/V）的行大小 [640, 320, 320]，高度 480。
        // 输出：平面大小计算规则：
        //      Y平面：行大小 * 高度 → 640 * 480 = 307200
        //      U/V平面：行大小 * (高度 / 2) → 320 * 240 = 76800（因色度子采样）
        let yuv_fmt = PixelFormat::YUV420P;
        let yuv_width = 640;
        let yuv_height = 480;

        // 步骤1：获取各平面行大小
        let yuv_linesizes = fill_linesizes(yuv_fmt, yuv_width)?;
        assert_eq!(
            yuv_linesizes,
            [640, 320, 320, 0],
            "YUV420P linesizes mismatch"
        );

        // 步骤2：验证 av_image_line_size 返回值
        assert_eq!(
            get_linesize(yuv_fmt, yuv_width as u32, 0)?,
            640,
            "Y plane linesize incorrect"
        );
        assert_eq!(
            get_linesize(yuv_fmt, yuv_width as u32, 1)?,
            320,
            "U plane linesize incorrect"
        );
        assert_eq!(
            get_linesize(yuv_fmt, yuv_width as u32, 2)?,
            320,
            "V plane linesize incorrect"
        );

        // 步骤3：计算平面大小
        let plane_sizes = fill_plane_sizes(
            yuv_fmt,
            yuv_linesizes[..3].iter().map(|&x| x as u32),
            yuv_height as u32,
        )?;
        // 预期结果：
        // Y: 640 * 480 = 307200
        // U: 320 * 240 = 76800
        // V: 320 * 240 = 76800
        assert_eq!(plane_sizes.len(), 3);
        assert_eq!(plane_sizes[0], 307200);
        assert_eq!(plane_sizes[1], 76800);
        assert_eq!(plane_sizes[2], 76800);

        // --------------------------
        // 测试用例2: RGBA 格式
        // --------------------------
        // 输入：单平面行大小 1280
        // 输出：单平面大小 1280 * 720 = 921600
        let rgba_fmt = PixelFormat::RGBA;
        let rgba_width = 320;
        let rgba_height = 720;

        // 步骤1：获取行大小（单平面）
        let rgba_linesizes = fill_linesizes(rgba_fmt, rgba_width)?;
        assert_eq!(rgba_linesizes, [1280, 0, 0, 0], "RGBA linesizes mismatch");

        // 步骤2：验证 av_image_line_size
        assert_eq!(
            get_linesize(rgba_fmt, rgba_width as u32, 0)?,
            1280,
            "RGBA plane linesize incorrect"
        );

        // 步骤3：计算平面大小
        let plane_sizes =
            fill_plane_sizes(rgba_fmt, vec![rgba_linesizes[0] as u32], rgba_height as u32)?;
        // 预期结果：1280 * 720 = 921600
        assert_eq!(plane_sizes.len(), 1);
        assert_eq!(plane_sizes[0], 921600);

        // --------------------------
        // 测试用例3: NV12 格式（YUV420半平面，UV交错）
        // --------------------------
        let nv12_fmt = PixelFormat::NV12;
        let nv12_width = 640;
        let nv12_height = 480;

        // 步骤1：获取各平面行大小
        let linesizes = fill_linesizes(nv12_fmt, nv12_width)?;
        assert_eq!(
            linesizes,
            [640, 640, 0, 0], // NV12只有两个平面：Y（行640）、UV（行640）
            "NV12 linesizes mismatch"
        );

        // 步骤2：验证 av_image_line_size 返回值
        assert_eq!(
            get_linesize(nv12_fmt, nv12_width as u32, 0)?,
            640,
            "NV12 Y plane linesize incorrect"
        );
        assert_eq!(
            get_linesize(nv12_fmt, nv12_width as u32, 1)?,
            640,
            "NV12 UV plane linesize incorrect"
        );

        // 错误测试：访问不存在的平面（索引2）
        assert!(
            get_linesize(nv12_fmt, nv12_width as u32, 2).is_err(),
            "NV12 should reject plane index 2"
        );

        // 步骤3：计算平面大小
        let plane_sizes = fill_plane_sizes(
            nv12_fmt,
            vec![linesizes[0] as u32, linesizes[1] as u32], // 传入两个平面
            nv12_height as u32,
        )?;

        // 预期结果：
        // Y平面：640 * 480 = 307200
        // UV平面：640 * (480 / 2) = 153600
        assert_eq!(plane_sizes.len(), 2);
        assert_eq!(plane_sizes[0], 307200);
        assert_eq!(plane_sizes[1], 153600);

        Ok(())
    }

    #[test]
    fn test_image_linesize_error() -> Result<()> {
        let yuv_fmt = PixelFormat::YUV420P;

        // --------------------------
        // 测试用例3: 错误场景
        // --------------------------
        // 错误1：无效像素格式
        assert!(
            get_linesize(PixelFormat::NONE, 640, 0).is_err(),
            "None format should fail"
        );

        // 错误2：越界平面索引（YUV420P只有3个平面）
        assert!(
            get_linesize(yuv_fmt, 640, 3).is_err(),
            "Plane index 3 should be invalid for YUV420P"
        );

        // 错误3：非法宽度（0或负数）
        assert!(get_linesize(yuv_fmt, 0, 0).is_err(), "Width 0 should fail");

        // 错误4：传入过多平面（超过4个）
        let oversized_input = vec![640, 320, 320, 128, 64];
        assert!(
            fill_plane_sizes(yuv_fmt, oversized_input, 480).is_ok(),
            "Should truncate to first 4 planes"
        );

        Ok(())
    }

    #[test]
    #[ignore = "need an image file"]
    fn test_image_resize() -> Result<()> {
        let input_image = image::open("assets/cat.jpg")?;
        let resized_image = resize(&input_image, 640, 640);
        let resized_image: DynamicImage = resized_image.into();
        resized_image.save("/tmp/output.png")?;
        Ok(())
    }
}
