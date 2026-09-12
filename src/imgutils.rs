use crate::PixelFormat;
use crate::error::{Result, RsmediaError, format_err};

use rsmpeg::avutil::AVFrame;
use rsmpeg::ffi;

/// Fill plane linesizes for an image with pixel format pix_fmt and width.
///
/// # Arguments
///
/// * `pix_fmt` - The pixel format of the image.
/// * `width` - The width of the image in pixels.
///
/// Returns an array of four integers representing the linesizes for each plane of the image.
pub fn fill_linesizes(pix_fmt: PixelFormat, width: i32) -> Result<[i32; 4]> {
    let mut linesizes = [0; 4];
    let ret =
        unsafe { ffi::av_image_fill_linesizes(linesizes.as_mut_ptr(), pix_fmt.into(), width) };

    // >= 0 in case of success, a negative error code otherwise
    if ret < 0 {
        return Err(RsmediaError::custom(format!(
            "Failed to fill linesizes: {ret}"
        )));
    }

    Ok(linesizes)
}

/// Compute the size of an image line with format pix_fmt and width
///
/// # Arguments
/// * `pix_fmt` - The pixel format of the image.
/// * `width` - The width of the image in pixels.
/// * `plane` - The index of the plane to compute the size for.
///
/// Returns The size of the image line in bytes for the specified plane.
pub fn get_linesize(pix_fmt: PixelFormat, width: u32, plane: usize) -> Result<usize> {
    // Safe because format is a valid format and this function is pure computation.
    let ret = unsafe { ffi::av_image_get_linesize(pix_fmt.into(), width as _, plane as _) };

    // returns the computed size in bytes
    if ret <= 0 {
        return Err(RsmediaError::custom(format!(
            "Failed to get line size, ret: {ret}"
        )));
    }

    Ok(ret as usize)
}

/// Fill plane sizes for an image with pixel format pix_fmt, linesizes and height.
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
) -> Result<Vec<usize>> {
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
        return Err(RsmediaError::custom(format!(
            "Failed to fill plane sizes, ret: {ret}"
        )));
    }

    Ok(plane_sizes_buf
        .into_iter()
        .map(|x| x as _)
        .take(planes)
        .collect())
}

/// frame data => `Vec<u8>`
pub fn copy_frame_to_buffer(frame: &AVFrame) -> Result<Vec<u8>> {
    let frame_width: i32 = frame.width;
    let frame_height: i32 = frame.height;
    if frame_width * frame_height <= 0 {
        return Err(format_err!("Invalid frame dimensions"));
    }

    let buf_size = frame.image_get_buffer_size(1)?;
    let mut buffer = vec![0u8; buf_size];
    let bytes = frame.image_copy_to_buffer(buffer.as_mut_slice(), 1)?;
    if bytes > 0 {
        buffer.truncate(bytes);
        Ok(buffer)
    } else {
        Err(RsmediaError::custom(format!(
            "Failed to copy image:{bytes}"
        )))
    }
}

/// 完整复制 AVFrame
///
/// # Arguments
///
/// * `src` - 源 AVFrame
/// * `dst` - 目标 AVFrame
/// * `copy_data` - 是否复制数据
pub fn copy_frame_metadata(src: &AVFrame, dst: &mut AVFrame, copy_data: bool) -> Result<()> {
    unsafe {
        if copy_data {
            // 目标 AVFrame 需已分配内存
            assert!(dst.is_allocated(), "destination frame is not allocated");

            // 复制数据
            let ret = ffi::av_frame_copy(dst.as_mut_ptr(), src.as_ptr());
            if ret < 0 {
                return Err(format_err!("Failed to copy frame data: {}", ret));
            }
        }

        // 复制属性：仅包含 metadata 和 side_data
        let ret = ffi::av_frame_copy_props(dst.as_mut_ptr(), src.as_ptr());
        if ret < 0 {
            return Err(format_err!("Failed to copy frame properties: {}", ret));
        }

        Ok(())
    }
}

/// 某个帧平面的几何信息：可见宽（像素）、可见高（行）与每像素字节数。
struct PlaneGeom {
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
}

/// 依据像素格式描述符，计算指定平面相对帧全分辨率的可见宽高与像素字节数。
///
/// 色度子采样平面（如 YUV420P 的 U/V）宽高按 `log2_chroma_*` 右移；无该描述时退回
/// `frame.width/height`。每像素字节数取自 `comp[plane].step`，缺失时按 1 处理。
fn plane_geom(frame: &AVFrame, plane_idx: usize) -> Result<PlaneGeom> {
    let desc = PixelFormat::from(frame.format).descriptor()?;

    let (shift_w, shift_h) = if plane_idx > 0 {
        (desc.log2_chroma_w as u32, desc.log2_chroma_h as u32)
    } else {
        (0, 0)
    };

    Ok(PlaneGeom {
        width: (frame.width as usize) >> shift_w,
        height: (frame.height as usize) >> shift_h,
        bytes_per_pixel: if desc.comp[plane_idx].step > 0 {
            desc.comp[plane_idx].step as usize
        } else {
            1
        },
    })
}

/// 获取指定帧的指定平面的实际数据，不包含额外的填充字节
pub fn get_plane_buffer(frame: &AVFrame, plane_idx: usize) -> Result<Vec<u8>> {
    if frame.width * frame.height <= 0 {
        return Err(format_err!("Invalid frame dimensions"));
    }

    // count planes of format
    let planes = PixelFormat::from(frame.format).count_planes()?;
    if plane_idx >= planes as usize {
        return Err(format_err!(
            "Invalid plane index: {}, max planes: {}",
            plane_idx,
            planes
        ));
    }
    if frame.data[plane_idx].is_null() {
        return Err(format_err!(
            "Null plane data pointer for plane {}",
            plane_idx
        ));
    }

    let buf_ptr = unsafe { ffi::av_frame_get_plane_buffer(frame.as_ptr(), plane_idx as i32) };
    if buf_ptr.is_null() {
        return Err(format_err!(
            "Null plane buffer pointer for plane {}",
            plane_idx
        ));
    }

    // 获取像素格式的描述信息
    let geom = plane_geom(frame, plane_idx)?;

    // 获取行步长
    let linesize = frame.linesize[plane_idx] as usize;

    // 创建一个新的缓冲区，只包含实际的像素数据（不包括填充）
    let bytes_per_row = geom.width * geom.bytes_per_pixel;
    let total_size = geom.height * bytes_per_row;
    let mut result = Vec::with_capacity(total_size);

    unsafe {
        // 计算平面数据在缓冲区中的偏移量
        let data_offset = frame.data[plane_idx].offset_from((*buf_ptr).data) as usize;
        if data_offset >= (*buf_ptr).size {
            return Err(format_err!("Invalid data offset for plane {}", plane_idx));
        }

        // 计算平面数据地址加上偏移量
        let src_ptr = (*buf_ptr).data.add(data_offset);

        // 确保不会超出缓冲区的大小
        if data_offset + (geom.height - 1) * linesize + bytes_per_row > (*buf_ptr).size {
            return Err(format_err!("Buffer too small for plane {}", plane_idx));
        }

        // Set the actual length
        result.set_len(total_size);

        // 使用批量复制操作逐行复制数据，跳过填充字节
        let dst_ptr: *mut u8 = result.as_mut_ptr();
        for y in 0..geom.height {
            let row_src_ptr = src_ptr.add(y * linesize);
            let row_dst_ptr = dst_ptr.add(y * bytes_per_row);
            std::ptr::copy_nonoverlapping(row_src_ptr, row_dst_ptr, bytes_per_row);
        }
    }

    Ok(result)
}

/// 将数据复制到指定的帧平面中
///
/// # Arguments
///
/// * `frame` - 目标 AVFrame
/// * `plane_idx` - 平面索引
/// * `src` - 源数据
/// * `src_linesize` - 源数据每行的字节数
///
/// # Safety
///
/// 调用者需要确保：
/// 1. plane_idx 是有效的（小于平面总数）
/// 2. src 包含足够的数据
/// 3. src_linesize 是正确的
pub fn fill_plane_from_buffer(
    frame: &mut AVFrame,
    plane_idx: usize,
    src: Vec<u8>,
    src_linesize: usize,
) -> Result<()> {
    // 基本参数检查
    if frame.width * frame.height <= 0 {
        return Err(RsmediaError::custom("Invalid frame dimensions"));
    }
    if !frame.is_writable()? {
        return Err(RsmediaError::custom("Frame is not writable"));
    }

    // 获取平面数量并检查平面索引
    let planes = PixelFormat::from(frame.format).count_planes()?;

    // 检查平面索引
    if plane_idx >= planes as usize {
        return Err(RsmediaError::custom(format!(
            "Invalid plane index: {plane_idx}, max planes: {planes}"
        )));
    }

    // 检查目标平面指针是否有效
    if frame.data[plane_idx].is_null() {
        return Err(RsmediaError::custom(format!(
            "Null plane data pointer for plane {plane_idx}"
        )));
    }

    // 计算平面尺寸（宽/高按像素格式色度子采样右移，每像素字节数取 desc.comp.step）
    let geom = plane_geom(frame, plane_idx)?;

    // 计算实际数据宽度（字节数）
    let byte_width = geom.width * geom.bytes_per_pixel;
    let dst_linesize = frame.linesize[plane_idx];

    // 验证行大小
    if src_linesize < byte_width {
        return Err(format_err!(
            "Source linesize {} is less than required byte width {}",
            src_linesize,
            byte_width
        ));
    }

    // 验证 byte_width 是否满足 FFmpeg 的要求
    if byte_width > dst_linesize.unsigned_abs() as usize || byte_width > src_linesize {
        return Err(format_err!(
            "byte_width {} exceeds linesize limits (dst: {}, src: {})",
            byte_width,
            dst_linesize,
            src_linesize
        ));
    }

    // 计算所需的最小源数据大小（考虑行填充）
    let required_size = geom.height * src_linesize;
    if src.len() < required_size {
        return Err(format_err!(
            "Incorrect source data size: got {}, need {}",
            src.len(),
            required_size
        ));
    }

    // 复制平面数据
    unsafe {
        ffi::av_image_copy_plane(
            frame.data[plane_idx], // 目标数据指针
            dst_linesize,          // 目标行大小
            src.as_ptr(),          // 源数据指针
            src_linesize as i32,   // 源数据行大小
            byte_width as i32,     // 要复制的宽度（字节数）
            geom.height as i32,    // 平面高度
        );
    }

    Ok(())
}

/// 用计算出的值按行填充 frame 的指定平面。
///
/// 注意：`av_frame_get_buffer` 会按 SIMD 对齐 linesize（可能大于平面实际宽度），若按一段
/// 连续内存写入会漏写对齐填充导致的行末尾字节未初始化（valgrind 会报
/// "Use of uninitialised value"）。因此本函数逐行按 `data[p] + y*linesize[p]` 写入，
/// 且只写宽度内字节，填充字节不会被触碰。
///
/// # Arguments
///
/// * `frame` - 目标 AVFrame（需已 alloc_buffer）
/// * `plane` - 平面索引
/// * `plane_w` - 平面实际可见宽度（像素/字节）
/// * `plane_h` - 平面实际可见高度（像素/行）
/// * `filler` - 计算函数，接收平面内坐标 `(x, y)`，返回该像素应写入的字节值
///
/// # Safety
///
/// 调用者需保证坐标 `(x, y)` 均满足 `x < plane_w`、`y < plane_h`，且
/// `frame.data[plane]` 指向已分配、可写的缓冲区。
pub unsafe fn fill_plane_with<F>(
    frame: &AVFrame,
    plane: usize,
    plane_w: usize,
    plane_h: usize,
    filler: F,
) where
    F: Fn(usize, usize) -> u8,
{
    unsafe {
        let linesize = frame.linesize[plane] as usize;
        let base = frame.data[plane].cast::<u8>();
        for y in 0..plane_h {
            let row = base.add(y * linesize);
            for x in 0..plane_w {
                *row.add(x) = filler(x, y);
            }
        }
    }
}

/// 将buffer数据填充到frame中
pub fn fill_frame_from_buffer(frame: &mut AVFrame, buffer: Vec<u8>) -> Result<()> {
    // 1. Basic validation
    if !frame.is_writable()? {
        return Err(RsmediaError::custom("Frame is not writable"));
    }
    if frame.data[0].is_null() {
        // This check implies the frame buffer hasn't been allocated properly
        // alloc_buffer should have been called before passing the frame here.
        return Err(RsmediaError::custom(
            "Frame buffer is not allocated (frame.data is null)",
        ));
    }

    // 2. Calculate the expected size of the contiguous buffer for the given format/dims
    let expected_size = frame.image_get_buffer_size(1)?;

    // 3. Validate input buffer size
    if buffer.len() < expected_size {
        return Err(RsmediaError::custom(format!(
            "Input buffer size mismatch. Expected at least {} bytes, got {}",
            expected_size,
            buffer.len()
        )));
    }

    unsafe {
        // 4. 目标指针/行宽直接取自 frame 本身（data/linesize 前 4 项数组指针）
        let dst_data = frame.data.as_ptr();
        let dst_linesizes = frame.linesize.as_ptr();

        // 5. Prepare source pointers and linesizes (describing the layout within the input `buffer`)
        // We need to calculate the layout as if it were tightly packed.
        let mut src_data = [std::ptr::null_mut(); 4];
        let mut src_linesizes = [0; 4];
        let pix_fmt = frame.format;
        let width = frame.width;
        let height = frame.height;

        // Use av_image_fill_arrays on a temporary structure to calculate
        // the packed layout pointers and linesizes within the source buffer.
        // This correctly handles planar vs packed logic based on pix_fmt.
        let ret_fill = ffi::av_image_fill_arrays(
            src_data.as_mut_ptr(),
            src_linesizes.as_mut_ptr(),
            buffer.as_ptr(),
            pix_fmt,
            width,
            height,
            1,
        );

        if ret_fill < 0 {
            return Err(RsmediaError::custom(format!(
                "Failed to calculate source layout using av_image_fill_arrays: {ret_fill}"
            )));
        }

        // 6. Perform the copy
        ffi::av_image_copy(
            dst_data,
            dst_linesizes,
            src_data.as_ptr() as *const *const u8,
            src_linesizes.as_ptr() as *const _,
            pix_fmt,
            width,
            height,
        );

        Ok(())
    }
}

/// 用「黑」填充整幅图像，适合清屏/占位。YUV 系会按 `color_range` 选取正确的黑值，
/// 带 alpha 的格式会将 alpha 置为不透明。
///
/// # Arguments
/// * `frame` - 目标 AVFrame（需已 alloc_buffer）
pub fn fill_black(frame: &mut AVFrame) -> Result<()> {
    if frame.data[0].is_null() {
        return Err(format_err!(
            "Frame buffer is not allocated (frame.data is null)"
        ));
    }
    let mut dst_linesizes = [0isize; 8];
    for i in 0..8 {
        dst_linesizes[i] = frame.linesize[i] as isize;
    }
    let ret = unsafe {
        ffi::av_image_fill_black(
            frame.data.as_ptr(),
            dst_linesizes.as_ptr(),
            frame.format,
            frame.color_range,
            frame.width,
            frame.height,
        )
    };
    if ret < 0 {
        return Err(format_err!("Failed to fill black, ret: {ret}"));
    }
    Ok(())
}

/// 用指定 RGBA 颜色填充整幅图像（子矩形内的 padding 不会被触碰）。
/// 颜色分量按 0..255 的整数值解释（见 `av_image_fill_color`）。
///
/// 注意：底层 `av_image_fill_color` 自 FFmpeg 7.0 起才提供，故该函数仅在
/// `ffmpeg7`/`ffmpeg8`/`ffmpeg9` feature 下可用。
///
/// # Arguments
/// * `frame` - 目标 AVFrame（需已 alloc_buffer）
/// * `r`/`g`/`b`/`a` - RGBA 分量（0..=255），`a` 为可选的 alpha
#[cfg(any(feature = "ffmpeg7", feature = "ffmpeg8", feature = "ffmpeg9"))]
pub fn fill_color(frame: &mut AVFrame, r: u8, g: u8, b: u8, a: u8) -> Result<()> {
    if frame.data[0].is_null() {
        return Err(format_err!(
            "Frame buffer is not allocated (frame.data is null)"
        ));
    }
    let mut dst_lines = [0isize; 8];
    for i in 0..8 {
        dst_lines[i] = frame.linesize[i] as isize;
    }
    let color = [r as u32, g as u32, b as u32, a as u32];
    let ret = unsafe {
        ffi::av_image_fill_color(
            frame.data.as_ptr(),
            dst_lines.as_ptr(),
            frame.format,
            color.as_ptr(),
            frame.width,
            frame.height,
            0,
        )
    };
    if ret < 0 {
        return Err(format_err!("Failed to fill color, ret: {ret}"));
    }
    Ok(())
}

/// 校验图像尺寸是否合法：所有平面的字节数都能被有符号 int 寻址，且
/// 不超过 `max_pixels`（`max_pixels <= 0` 表示不限制）。
///
/// # Arguments
/// * `width`/`height` - 像素尺寸（须为非零）
/// * `pix_fmt` - 像素格式（可传 `PixelFormat::NONE`）
/// * `max_pixels` - 允许的最大像素数；<= 0 表示不限制
pub fn check_image_size(
    width: u32,
    height: u32,
    pix_fmt: PixelFormat,
    max_pixels: i64,
) -> Result<()> {
    // `av_image_check_size2` 会把 `max_pixels` 当作硬上限，0 表示“0 个像素”。
    // 这里把 <=0 归一化为“不限制”，避免误伤。
    let max_pixels = if max_pixels > 0 { max_pixels } else { i64::MAX };
    let ret = unsafe {
        ffi::av_image_check_size2(
            width,
            height,
            max_pixels,
            pix_fmt.into(),
            0,
            std::ptr::null_mut(),
        )
    };
    // >= 0 表示合法
    if ret < 0 {
        return Err(format_err!(
            "Invalid image size {width}x{height} for {:?}: {ret}",
            pix_fmt
        ));
    }
    Ok(())
}

/// 根据 `frame.crop_*` 字段对帧应用裁剪，裁剪后的 `width/height` 会相应变小。
///
/// # Arguments
/// * `frame` - 需裁剪的 AVFrame
/// * `flags` - 传入 `ffi::AV_FRAME_CROP_UNALIGNED` 表示允许未对齐裁剪，否则按对齐约束
pub fn apply_cropping(frame: &mut AVFrame, flags: i32) -> Result<()> {
    if frame.data[0].is_null() {
        return Err(format_err!(
            "Frame buffer is not allocated (frame.data is null)"
        ));
    }
    let ret = unsafe { ffi::av_frame_apply_cropping(frame.as_mut_ptr(), flags) };
    if ret < 0 {
        return Err(format_err!("Failed to apply cropping, ret: {ret}"));
    }
    Ok(())
}

/// 将 `AVFrame` 转换为 `image::DynamicImage`。
///
/// packed 8bit 格式（RGB24/RGBA/GRAY8）直接从帧数据构建，其他格式
/// （YUV 系列、BGR 族等）经 swscale 统一转为 RGB24 再构建。
/// 不依赖 `ndarray` feature。硬件帧需先下载到内存（见
/// [`crate::hwaccel::HWContext::hw_download`]）。
pub fn to_dynamic_image(frame: &AVFrame) -> Result<image::DynamicImage> {
    let (width, height) = (frame.width as u32, frame.height as u32);
    if width == 0 || height == 0 {
        return Err(RsmediaError::custom("Invalid frame dimensions"));
    }

    let build =
        |pix_fmt: PixelFormat, buf: Vec<u8>| -> Option<image::DynamicImage> {
            match pix_fmt {
                PixelFormat::RGB24 => image::RgbImage::from_raw(width, height, buf)
                    .map(image::DynamicImage::ImageRgb8),
                PixelFormat::RGBA => image::RgbaImage::from_raw(width, height, buf)
                    .map(image::DynamicImage::ImageRgba8),
                PixelFormat::GRAY8 => image::GrayImage::from_raw(width, height, buf)
                    .map(image::DynamicImage::ImageLuma8),
                _ => None,
            }
        };

    let pix_fmt = PixelFormat::from(frame.format);
    match pix_fmt {
        PixelFormat::RGB24 | PixelFormat::RGBA | PixelFormat::GRAY8 => {
            let buf = copy_frame_to_buffer(frame)?;
            build(pix_fmt, buf)
                .ok_or_else(|| RsmediaError::custom("Failed to build image from frame data"))
        }
        _ => {
            // 其他格式（YUV/BGR 族等）：swscale 统一转 RGB24
            let rgb =
                crate::scale::scale_frame(frame, frame.width, frame.height, PixelFormat::RGB24)?;
            let buf = copy_frame_to_buffer(&rgb)?;
            image::RgbImage::from_raw(width, height, buf)
                .map(image::DynamicImage::ImageRgb8)
                .ok_or_else(|| RsmediaError::custom("Failed to build image from RGB24 data"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Context;
    use ab_glyph::PxScale;
    use image::{ImageBuffer, Rgb};

    /// Create an image with the given text and a gradient color.
    fn create_image_with_text(
        width: u32,
        height: u32,
        text: &str,
    ) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        let mut img = ImageBuffer::new(width, height);

        use palette::IntoColor;

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

    #[test]
    fn test_image_text() -> Result<()> {
        let output_path = crate::test_support::test_output_path("imgutils", "image_with_text.png");
        let rgb = create_image_with_text(640, 480, "Hello, world!");
        rgb.save(output_path)?;
        Ok(())
    }

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

    /// 创建测试用的AVFrame
    fn create_test_frame(width: i32, height: i32, format: i32) -> Result<AVFrame> {
        let mut frame = AVFrame::new();
        frame.set_width(width);
        frame.set_height(height);
        frame.set_format(format);

        // 分配帧缓冲区
        frame
            .alloc_buffer()
            .context("Failed to allocate frame buffer")?;

        Ok(frame)
    }

    #[test]
    fn test_get_buffer_size() -> Result<()> {
        // 正确的尺寸和格式
        let frame = create_test_frame(640, 480, ffi::AV_PIX_FMT_RGB24)?;
        let size = frame.image_get_buffer_size(1)?;
        assert_eq!(size, 640 * 480 * 3);

        // 测试YUV420P格式
        let frame = create_test_frame(640, 480, ffi::AV_PIX_FMT_YUV420P)?;
        let size = frame.image_get_buffer_size(1)?;
        assert_eq!(size, 640 * 480 * 3 / 2); // YUV420P 大小是 RGB24 的3/2

        Ok(())
    }

    #[test]
    fn test_copy_to_buffer() {
        // 测试RGB24格式
        let width = 320;
        let height = 240;
        let mut frame = create_test_frame(width, height, ffi::AV_PIX_FMT_RGB24).unwrap();

        // 填充测试数据
        let rgb_data = vec![128u8; width as usize * height as usize * 3];
        fill_frame_from_buffer(&mut frame, rgb_data).unwrap();

        let buffer = copy_frame_to_buffer(&frame).unwrap();
        assert_eq!(buffer.len(), width as usize * height as usize * 3);
        assert_eq!(buffer[0], 128);

        // 测试YUV420P格式
        let mut frame = create_test_frame(320, 240, ffi::AV_PIX_FMT_YUV420P).unwrap();

        // 使用 copy_plane 分别填充 Y、U、V 平面
        let y_data = vec![128u8; 320 * 240];
        let u_data = vec![128u8; 160 * 120];
        let v_data = vec![128u8; 160 * 120];

        fill_plane_from_buffer(&mut frame, 0, y_data, 320).unwrap();
        fill_plane_from_buffer(&mut frame, 1, u_data, 160).unwrap();
        fill_plane_from_buffer(&mut frame, 2, v_data, 160).unwrap();

        let buffer = copy_frame_to_buffer(&frame).unwrap();
        assert_eq!(buffer.len(), 320 * 240 * 3 / 2);
    }

    #[test]
    fn test_copy_plane() {
        let width = 320;
        let height = 240;
        let mut frame = create_test_frame(width, height, ffi::AV_PIX_FMT_YUV420P).unwrap();

        // 为每个平面创建测试数据
        // Y 平面 (全部填充为值 100)
        let y_size = width as usize * height as usize;
        let y_data = vec![100_u8; y_size];

        // U 平面 (全部填充为值 150)
        let uv_width = width / 2;
        let uv_height = height / 2;
        let uv_size = (uv_width * uv_height) as usize;
        let u_data = vec![150_u8; uv_size];

        // V 平面 (全部填充为值 200)
        let v_data = vec![200_u8; uv_size];

        // 填充数据到 AVFrame
        fill_plane_from_buffer(&mut frame, 0, y_data.clone(), width as usize).unwrap();
        fill_plane_from_buffer(&mut frame, 1, u_data.clone(), uv_width as usize).unwrap();
        fill_plane_from_buffer(&mut frame, 2, v_data.clone(), uv_width as usize).unwrap();

        // 直接从帧数据指针读取数据进行验证
        unsafe {
            // 验证 Y 平面数据
            let y_linesize = frame.linesize[0] as usize;
            let y_ptr = frame.data[0] as *const u8;
            for y in 0..height as usize {
                let row_ptr = y_ptr.add(y * y_linesize);
                let row = std::slice::from_raw_parts(row_ptr, width as usize);
                for (x, &val) in row.iter().enumerate() {
                    assert_eq!(val, 100, "Y plane data mismatch at ({}, {})", x, y);
                }
            }

            // 验证 U 平面数据
            let u_linesize = frame.linesize[1] as usize;
            let u_ptr = frame.data[1] as *const u8;
            for y in 0..uv_height as usize {
                let row_ptr = u_ptr.add(y * u_linesize);
                let row = std::slice::from_raw_parts(row_ptr, uv_width as usize);
                for (x, &val) in row.iter().enumerate() {
                    assert_eq!(val, 150, "U plane data mismatch at ({}, {})", x, y);
                }
            }

            // 验证 V 平面数据
            let v_linesize = frame.linesize[2] as usize;
            let v_ptr = frame.data[2] as *const u8;
            for y in 0..uv_height as usize {
                let row_ptr = v_ptr.add(y * v_linesize);
                let row = std::slice::from_raw_parts(row_ptr, uv_width as usize);
                for (x, &val) in row.iter().enumerate() {
                    assert_eq!(val, 200, "V plane data mismatch at ({}, {})", x, y);
                }
            }
        }

        // 获取并验证数据
        let y_buffer = get_plane_buffer(&frame, 0).unwrap();
        let u_buffer = get_plane_buffer(&frame, 1).unwrap();
        let v_buffer = get_plane_buffer(&frame, 2).unwrap();

        // 打印缓冲区大小和预期大小，帮助调试
        println!(
            "Y buffer size: {}, expected at least: {}",
            y_buffer.len(),
            y_size
        );
        println!(
            "U buffer size: {}, expected at least: {}",
            u_buffer.len(),
            uv_size
        );
        println!(
            "V buffer size: {}, expected at least: {}",
            v_buffer.len(),
            uv_size
        );

        // 验证数据 - 只检查实际数据部分，忽略可能的填充
        assert_eq!(&y_buffer[..y_size], &y_data[..], "Y value doesn't match");
        assert_eq!(&u_buffer[..uv_size], &u_data[..], "U value doesn't match");
        assert_eq!(&v_buffer[..uv_size], &v_data[..], "V value doesn't match");
    }

    #[test]
    fn test_frame_copy() -> Result<()> {
        // 创建源frame和目标frame
        let mut src_frame = create_test_frame(320, 240, ffi::AV_PIX_FMT_RGB24)?;
        let mut dst_frame = create_test_frame(320, 240, ffi::AV_PIX_FMT_RGB24)?;

        // 填充源frame
        let test_data = vec![128u8; 320 * 240 * 3];
        fill_frame_from_buffer(&mut src_frame, test_data)?;

        // 验证源frame数据
        let src_buffer = copy_frame_to_buffer(&src_frame)?;
        assert_eq!(
            src_buffer.len(),
            320 * 240 * 3,
            "Source frame buffer size mismatch"
        );
        assert!(
            src_buffer.iter().all(|&x| x == 128),
            "Source frame data mismatch"
        );

        // 测试复制
        copy_frame_metadata(&src_frame, &mut dst_frame, true)?;

        // 验证目标frame属性
        assert_eq!(dst_frame.width, 320, "Frame width mismatch");
        assert_eq!(dst_frame.height, 240, "Frame height mismatch");
        assert_eq!(
            dst_frame.format,
            ffi::AV_PIX_FMT_RGB24,
            "Frame format mismatch"
        );

        // 验证数据是否正确复制
        let dst_buffer = copy_frame_to_buffer(&dst_frame)?;
        assert_eq!(
            dst_buffer.len(),
            320 * 240 * 3,
            "Destination frame buffer size mismatch"
        );
        assert!(
            dst_buffer.iter().all(|&x| x == 128),
            "Destination frame data mismatch"
        );

        // 直接比较源和目标数据
        assert_eq!(
            src_buffer, dst_buffer,
            "Source and destination frame data mismatch"
        );

        // 验证每个平面的数据
        unsafe {
            let src_ptr = src_frame.data[0] as *const u8;
            let dst_ptr = dst_frame.data[0] as *const u8;
            let linesize = src_frame.linesize[0] as usize;

            for y in 0..240 {
                let src_row = std::slice::from_raw_parts(src_ptr.add(y * linesize), 320 * 3);
                let dst_row = std::slice::from_raw_parts(dst_ptr.add(y * linesize), 320 * 3);
                assert_eq!(src_row, dst_row, "Row {} data mismatch", y);
            }
        }

        Ok(())
    }

    #[cfg(feature = "ndarray")]
    #[test]
    fn test_fill_frame_from_buffer() -> Result<()> {
        let mut frame = create_test_frame(320, 240, ffi::AV_PIX_FMT_RGB24)?;

        // 创建正确大小的buffer
        let buffer_size = frame.image_get_buffer_size(1)?;
        let buffer = vec![128u8; buffer_size];

        // 测试正常填充
        fill_frame_from_buffer(&mut frame, buffer)?;

        // 验证数据是否正确填充
        let result_buffer = copy_frame_to_buffer(&frame)?;
        assert_eq!(result_buffer[0], 128);

        Ok(())
    }

    #[test]
    fn test_fill_plane_with() {
        // YUV420P：Y 平面全分辨率，UV 平面各 1/4
        let width = 320;
        let height = 240;
        let uv_w = width as usize / 2;
        let uv_h = height as usize / 2;
        let frame = create_test_frame(width, height, ffi::AV_PIX_FMT_YUV420P).unwrap();

        unsafe {
            fill_plane_with(&frame, 0, width as usize, height as usize, |x, y| {
                ((y * width as usize + x) as u8) % 255
            });
            fill_plane_with(&frame, 1, uv_w, uv_h, |x, y| {
                ((y * uv_w + x) as u8).wrapping_add(85) % 255
            });
            fill_plane_with(&frame, 2, uv_w, uv_h, |x, y| {
                ((y * uv_w + x) as u8).wrapping_add(170) % 255
            });
        }

        // 验证每个可见像素值；同时确保尾部填充字节未被触碰（保持未初始化也在 bounds 内，
        // fill_plane_with 只写可见宽度，不会越界）。
        unsafe {
            for p in 0..3 {
                let (pw, ph) = if p == 0 {
                    (width as usize, height as usize)
                } else {
                    (uv_w, uv_h)
                };
                let linesize = frame.linesize[p] as usize;
                let base = frame.data[p].cast::<u8>();
                for y in 0..ph {
                    let row = base.add(y * linesize);
                    for x in 0..pw {
                        let f = |off: u8| ((y * pw + x) as u8).wrapping_add(off) % 255;
                        let expect = if p == 0 {
                            f(0)
                        } else if p == 1 {
                            f(85)
                        } else {
                            f(170)
                        };
                        assert_eq!(*row.add(x), expect, "plane {p} at ({x},{y})");
                    }
                }
            }
        }
    }

    #[test]
    fn test_fill_plane_from_buffer() -> Result<()> {
        // 测试用例1: YUV422P 格式
        let width = 320;
        let height = 240;
        let mut frame = create_test_frame(width, height, ffi::AV_PIX_FMT_YUV422P)?;

        // Y 平面 (全部填充为值 100)
        let y_size = width as usize * height as usize;
        let y_data = vec![100_u8; y_size];

        // U 平面 (全部填充为值 150)
        let uv_width = width / 2;
        let uv_height = height;
        let uv_size = (uv_width * uv_height) as usize;
        let u_data = vec![150_u8; uv_size];

        // V 平面 (全部填充为值 200)
        let v_data = vec![200_u8; uv_size];

        // 填充数据到 AVFrame
        fill_plane_from_buffer(&mut frame, 0, y_data.clone(), width as usize)?;
        fill_plane_from_buffer(&mut frame, 1, u_data.clone(), uv_width as usize)?;
        fill_plane_from_buffer(&mut frame, 2, v_data.clone(), uv_width as usize)?;

        // 验证数据 - 方法1：使用 get_plane_buffer
        let y_buffer = get_plane_buffer(&frame, 0)?;
        let u_buffer = get_plane_buffer(&frame, 1)?;
        let v_buffer = get_plane_buffer(&frame, 2)?;

        // 只比较实际数据部分，忽略可能的填充
        assert_eq!(&y_buffer[..y_size], &y_data[..], "Y plane data mismatch");
        assert_eq!(&u_buffer[..uv_size], &u_data[..], "U plane data mismatch");
        assert_eq!(&v_buffer[..uv_size], &v_data[..], "V plane data mismatch");

        // 验证数据 - 方法2：直接访问帧数据
        unsafe {
            // 验证 Y 平面
            let y_linesize = frame.linesize[0] as usize;
            let y_ptr = frame.data[0] as *const u8;
            for y in 0..height as usize {
                let row_ptr = y_ptr.add(y * y_linesize);
                let row = std::slice::from_raw_parts(row_ptr, width as usize);
                for (x, &val) in row.iter().enumerate() {
                    assert_eq!(val, 100, "Y plane data mismatch at ({}, {})", x, y);
                }
            }

            // 验证 U 平面
            let u_linesize = frame.linesize[1] as usize;
            let u_ptr = frame.data[1] as *const u8;
            for y in 0..uv_height as usize {
                let row_ptr = u_ptr.add(y * u_linesize);
                let row = std::slice::from_raw_parts(row_ptr, uv_width as usize);
                for (x, &val) in row.iter().enumerate() {
                    assert_eq!(val, 150, "U plane data mismatch at ({}, {})", x, y);
                }
            }

            // 验证 V 平面
            let v_linesize = frame.linesize[2] as usize;
            let v_ptr = frame.data[2] as *const u8;
            for y in 0..uv_height as usize {
                let row_ptr = v_ptr.add(y * v_linesize);
                let row = std::slice::from_raw_parts(row_ptr, uv_width as usize);
                for (x, &val) in row.iter().enumerate() {
                    assert_eq!(val, 200, "V plane data mismatch at ({}, {})", x, y);
                }
            }
        }

        // 测试用例2: YUV444P 格式
        let mut frame = create_test_frame(width, height, ffi::AV_PIX_FMT_YUV444P)?;

        // 所有平面大小相同
        let plane_size = width as usize * height as usize;
        let y_data = vec![100_u8; plane_size];
        let u_data = vec![150_u8; plane_size];
        let v_data = vec![200_u8; plane_size];

        fill_plane_from_buffer(&mut frame, 0, y_data.clone(), width as usize)?;
        fill_plane_from_buffer(&mut frame, 1, u_data.clone(), width as usize)?;
        fill_plane_from_buffer(&mut frame, 2, v_data.clone(), width as usize)?;

        // 验证数据 - 方法1：使用 get_plane_buffer
        let y_buffer = get_plane_buffer(&frame, 0)?;
        let u_buffer = get_plane_buffer(&frame, 1)?;
        let v_buffer = get_plane_buffer(&frame, 2)?;

        assert_eq!(y_buffer, y_data, "YUV444P Y plane data mismatch");
        assert_eq!(u_buffer, u_data, "YUV444P U plane data mismatch");
        assert_eq!(v_buffer, v_data, "YUV444P V plane data mismatch");

        // 验证数据 - 方法2：直接访问帧数据
        unsafe {
            // 验证 Y 平面
            let y_linesize = frame.linesize[0] as usize;
            let y_ptr = frame.data[0] as *const u8;
            for y in 0..height as usize {
                let row_ptr = y_ptr.add(y * y_linesize);
                let row = std::slice::from_raw_parts(row_ptr, width as usize);
                for (x, &val) in row.iter().enumerate() {
                    assert_eq!(val, 100, "YUV444P Y plane data mismatch at ({}, {})", x, y);
                }
            }

            // 验证 U 平面
            let u_linesize = frame.linesize[1] as usize;
            let u_ptr = frame.data[1] as *const u8;
            for y in 0..height as usize {
                let row_ptr = u_ptr.add(y * u_linesize);
                let row = std::slice::from_raw_parts(row_ptr, width as usize);
                for (x, &val) in row.iter().enumerate() {
                    assert_eq!(val, 150, "YUV444P U plane data mismatch at ({}, {})", x, y);
                }
            }

            // 验证 V 平面
            let v_linesize = frame.linesize[2] as usize;
            let v_ptr = frame.data[2] as *const u8;
            for y in 0..height as usize {
                let row_ptr = v_ptr.add(y * v_linesize);
                let row = std::slice::from_raw_parts(row_ptr, width as usize);
                for (x, &val) in row.iter().enumerate() {
                    assert_eq!(val, 200, "YUV444P V plane data mismatch at ({}, {})", x, y);
                }
            }
        }

        // 测试用例3: RGBA 格式
        let mut frame = create_test_frame(width, height, ffi::AV_PIX_FMT_RGBA)?;

        // RGBA 是单平面格式，每个像素4字节
        let rgba_size = width as usize * height as usize * 4;
        let rgba_data = vec![128_u8; rgba_size];

        fill_plane_from_buffer(&mut frame, 0, rgba_data.clone(), (width * 4) as usize)?;

        // 验证数据 - 方法1：使用 get_plane_buffer
        let buffer = get_plane_buffer(&frame, 0)?;
        assert_eq!(buffer, rgba_data, "RGBA plane data mismatch");

        // 验证数据 - 方法2：直接访问帧数据
        unsafe {
            let linesize = frame.linesize[0] as usize;
            let ptr = frame.data[0] as *const u8;
            for y in 0..height as usize {
                let row_ptr = ptr.add(y * linesize);
                let row = std::slice::from_raw_parts(row_ptr, width as usize * 4);
                for (x, &val) in row.iter().enumerate() {
                    assert_eq!(val, 128, "RGBA plane data mismatch at ({}, {})", x, y);
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_fill_plane_from_buffer_errors() -> Result<()> {
        let width = 320;
        let height = 240;
        let mut frame = create_test_frame(width, height, ffi::AV_PIX_FMT_YUV420P)?;

        // 错误1: 无效的平面索引
        let data = vec![0_u8; 100];
        assert!(
            fill_plane_from_buffer(&mut frame, 3, data, width as usize).is_err(),
            "Should fail for invalid plane index"
        );

        // 错误2: 不匹配的源数据大小
        let y_data = vec![100_u8; width as usize * height as usize / 2]; // 数据太小
        assert!(
            fill_plane_from_buffer(&mut frame, 0, y_data, width as usize).is_err(),
            "Should fail for insufficient source data"
        );

        // 错误3: 不匹配的行大小
        let y_data = vec![100_u8; width as usize * height as usize];
        assert!(
            fill_plane_from_buffer(&mut frame, 0, y_data, (width / 2) as usize).is_err(),
            "Should fail for mismatched linesize"
        );

        // 错误4: 空数据
        let empty_data = vec![];
        assert!(
            fill_plane_from_buffer(&mut frame, 0, empty_data, width as usize).is_err(),
            "Should fail for empty data"
        );

        Ok(())
    }

    #[test]
    fn test_check_image_size() {
        // 合法尺寸
        assert!(check_image_size(320, 240, PixelFormat::YUV420P, 0).is_ok());
        // max_pixels 超限
        assert!(
            check_image_size(10000, 10000, PixelFormat::RGB24, 1000).is_err(),
            "Should reject size exceeding max_pixels"
        );
        // 0 宽或 0 高非法
        assert!(check_image_size(0, 240, PixelFormat::RGB24, 0).is_err());
        assert!(check_image_size(320, 0, PixelFormat::RGB24, 0).is_err());
    }

    #[test]
    fn test_fill_black_gray() -> Result<()> {
        // GRAY8 为有限范围（limited range）亮度，黑帧 Y = 16，非 0
        let width = 64;
        let height = 48;
        let mut frame = create_test_frame(width, height, ffi::AV_PIX_FMT_GRAY8)?;
        // 先写入非零值
        fill_plane_from_buffer(
            &mut frame,
            0,
            vec![255u8; (width * height) as usize],
            width as usize,
        )?;
        fill_black(&mut frame)?;
        let buf = get_plane_buffer(&frame, 0)?;
        assert_eq!(
            buf,
            vec![16u8; (width * height) as usize],
            "GRAY8 limited-range black frame should be all 16"
        );
        Ok(())
    }

    #[cfg(any(feature = "ffmpeg7", feature = "ffmpeg8", feature = "ffmpeg9"))]
    #[test]
    fn test_fill_color_gray() -> Result<()> {
        // GRAY8 用 fill_color 填灰 = 分量 r
        let width = 64;
        let height = 48;
        let mut frame = create_test_frame(width, height, ffi::AV_PIX_FMT_GRAY8)?;
        fill_color(&mut frame, 128, 0, 0, 255)?;
        let buf = get_plane_buffer(&frame, 0)?;
        assert!(buf.iter().all(|&v| v == 128), "GRAY8 fill should set luma");
        Ok(())
    }

    #[test]
    fn test_apply_cropping() -> Result<()> {
        let mut frame = create_test_frame(64, 48, ffi::AV_PIX_FMT_YUV420P)?;
        // 设置裁剪量（wrap 未提供字段 setter，直接经底层指针写入）
        unsafe {
            let raw = frame.as_mut_ptr();
            (*raw).crop_top = 4;
            (*raw).crop_bottom = 4;
            (*raw).crop_left = 4;
            (*raw).crop_right = 4;
        }
        // AV_FRAME_CROP_UNALIGNED 在 Windows/vcpkg 绑定中已是 i32，而在
        // Linux 上是 u32；`as i32` 在 Windows 会触发多余的 cast 警告 unnecessary (`i32` -> `i32`)
        #[allow(clippy::unnecessary_cast)]
        apply_cropping(&mut frame, ffi::AV_FRAME_CROP_UNALIGNED as i32)?;
        // 裁剪后尺寸变小
        assert_eq!(frame.width, 56, "width after cropping wrong");
        assert_eq!(frame.height, 40, "height after cropping wrong");
        Ok(())
    }
}
