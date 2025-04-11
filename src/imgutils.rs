use crate::PixelFormat;

use anyhow::{Error, Result};
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
        return Err(Error::msg(format!("Failed to fill linesizes: {}", ret)));
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
        return Err(Error::msg(format!("Failed to get line size, ret: {}", ret)));
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

pub fn get_buffer_size(frame: &AVFrame, align: i32) -> Result<usize> {
    if align <= 0 {
        return Err(anyhow::anyhow!("Invalid alignment"));
    }

    unsafe {
        let size = ffi::av_image_get_buffer_size(
            frame.format,
            frame.width,
            frame.height,
            align, // alignment
        );

        if size <= 0 {
            Err(anyhow::anyhow!("Failed to get buffer size: {}", size))
        } else {
            Ok(size as usize)
        }
    }
}

/// frame => Vec<u8>
pub fn copy_frame_to_buffer(frame: &AVFrame) -> Result<Vec<u8>> {
    let frame_width: i32 = frame.width;
    let frame_height: i32 = frame.height;
    if frame_width * frame_height <= 0 {
        return Err(anyhow::anyhow!("Invalid frame dimensions"));
    }

    unsafe {
        let buf_size = get_buffer_size(frame, 1)?;
        let mut buffer = vec![0u8; buf_size as usize];
        let bytes = ffi::av_image_copy_to_buffer(
            buffer.as_mut_ptr(),
            buf_size as i32,
            (*frame.as_ptr()).data.as_ptr() as *const *const u8,
            (*frame.as_ptr()).linesize.as_ptr(),
            frame.format,
            frame.width,
            frame.height,
            1,
        );

        if bytes > 0 {
            buffer.truncate(bytes as usize);
            Ok(buffer)
        } else {
            Err(Error::msg(format!("Failed to copy image:{}", bytes)))
        }
    }
}

/// 完整复制 AVFrame（包括数据和属性）
pub fn copy_frame(src: &AVFrame, dst: &mut AVFrame) -> Result<()> {
    // 目标 AVFrame 需已分配内存
    assert!(dst.is_allocated(), "destination frame is not allocated");

    unsafe {
        // 复制数据
        let ret = ffi::av_frame_copy(dst.as_mut_ptr(), src.as_ptr());
        if ret < 0 {
            return Err(anyhow::anyhow!("Failed to copy frame data: {}", ret));
        }

        // 复制属性
        let ret = ffi::av_frame_copy_props(dst.as_mut_ptr(), src.as_ptr());
        if ret < 0 {
            return Err(anyhow::anyhow!("Failed to copy frame properties: {}", ret));
        }

        Ok(())
    }
}

/// 获取指定帧的指定平面的实际数据，不包含额外的填充字节
pub fn get_plane_buffer(frame: &AVFrame, plane_idx: usize) -> Result<Vec<u8>> {
    if frame.width * frame.height <= 0 {
        return Err(anyhow::anyhow!("Invalid frame dimensions"));
    }

    // count planes of format
    let planes = PixelFormat::from(frame.format).count_planes()?;
    if plane_idx >= planes as usize {
        return Err(anyhow::anyhow!(
            "Invalid plane index: {}, max planes: {}",
            plane_idx,
            planes
        ));
    }
    if frame.data[plane_idx].is_null() {
        return Err(anyhow::anyhow!(
            "Null plane data pointer for plane {}",
            plane_idx
        ));
    }

    let buf_ptr = unsafe { ffi::av_frame_get_plane_buffer(frame.as_ptr(), plane_idx as i32) };
    if buf_ptr.is_null() {
        return Err(anyhow::anyhow!(
            "Null plane buffer pointer for plane {}",
            plane_idx
        ));
    }

    // 获取像素格式的描述信息
    let desc = PixelFormat::from(frame.format).descriptor();

    // 计算平面的实际尺寸
    let plane_height = if desc.log2_chroma_h > 0 && plane_idx > 0 {
        frame.height >> desc.log2_chroma_h
    } else {
        frame.height
    };

    let plane_width = if desc.log2_chroma_w > 0 && plane_idx > 0 {
        frame.width >> desc.log2_chroma_w
    } else {
        frame.width
    };

    // 计算每个像素的字节数
    let bytes_per_pixel = if desc.comp[plane_idx].step > 0 {
        desc.comp[plane_idx].step
    } else {
        1
    };

    // 计算平面数据的实际大小
    // 这种计算方式假设平面数据是连续存储的，没有考虑 FFmpeg 中的 linesize （行步长）。
    // 在 FFmpeg 中，每行数据可能会有额外的填充字节用于内存对齐，
    // 这意味着实际的行大小（ frame.linesize[plane_idx] ）可能大于计算出的行大小（ plane_width * bytes_per_pixel ）
    // 正确的做法是考虑 linesize 并计算实际的行大小
    // let plane_size = plane_height as usize * plane_width as usize * bytes_per_pixel as usize;
    //
    // 获取行步长
    let linesize = frame.linesize[plane_idx] as usize;

    // 创建一个新的缓冲区，只包含实际的像素数据（不包括填充）
    let mut result =
        Vec::with_capacity(plane_height as usize * plane_width as usize * bytes_per_pixel as usize);

    unsafe {
        // 计算平面数据在缓冲区中的偏移量
        let data_offset = frame.data[plane_idx].offset_from((*buf_ptr).data) as usize;
        if data_offset >= (*buf_ptr).size {
            return Err(anyhow::anyhow!(
                "Invalid data offset for plane {}",
                plane_idx
            ));
        }

        // 计算平面数据地址加上偏移量
        let src_ptr = (*buf_ptr).data.add(data_offset);

        // 确保不会超出缓冲区的大小
        let bytes_per_row = plane_width as usize * bytes_per_pixel as usize;
        if data_offset + (plane_height as usize - 1) * linesize + bytes_per_row > (*buf_ptr).size {
            return Err(anyhow::anyhow!("Buffer too small for plane {}", plane_idx));
        }

        // 逐行复制数据，跳过填充字节
        for y in 0..plane_height as usize {
            let row_ptr = src_ptr.add(y * linesize);
            let row_data = std::slice::from_raw_parts(row_ptr, bytes_per_row);
            result.extend_from_slice(row_data);
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
    src: &[u8],
    src_linesize: usize,
) -> Result<()> {
    // 基本参数检查
    if frame.width * frame.height <= 0 {
        return Err(Error::msg("Invalid frame dimensions"));
    }
    if !frame.is_writable()? {
        return Err(Error::msg("Frame is not writable"));
    }

    // 获取格式描述符
    let desc = PixelFormat::from(frame.format).descriptor();
    let planes = PixelFormat::from(frame.format).count_planes()?;

    // 检查平面索引
    if plane_idx >= planes as usize {
        return Err(Error::msg(format!(
            "Invalid plane index: {}, max planes: {}",
            plane_idx, planes
        )));
    }

    // 检查目标平面指针是否有效
    if frame.data[plane_idx].is_null() {
        return Err(Error::msg(format!(
            "Null plane data pointer for plane {}",
            plane_idx
        )));
    }

    // 计算平面尺寸
    let plane_height = if desc.log2_chroma_h > 0 && plane_idx > 0 {
        frame.height >> desc.log2_chroma_h
    } else {
        frame.height
    };

    let plane_width = if desc.log2_chroma_w > 0 && plane_idx > 0 {
        frame.width >> desc.log2_chroma_w
    } else {
        frame.width
    };

    // 计算每个像素的字节数
    let bytes_per_pixel = if desc.comp[plane_idx].step > 0 {
        desc.comp[plane_idx].step
    } else {
        1
    };

    // 计算实际数据宽度（字节数）
    let byte_width = plane_width * bytes_per_pixel;
    let dst_linesize = frame.linesize[plane_idx];

    // 验证行大小
    if src_linesize < byte_width as usize {
        return Err(anyhow::anyhow!(
            "Source linesize {} is less than required byte width {}",
            src_linesize,
            byte_width
        ));
    }

    // 验证 byte_width 是否满足 FFmpeg 的要求
    if byte_width > dst_linesize.abs() || byte_width > src_linesize as i32 {
        return Err(anyhow::anyhow!(
            "byte_width {} exceeds linesize limits (dst: {}, src: {})",
            byte_width,
            dst_linesize,
            src_linesize
        ));
    }

    // 计算所需的最小源数据大小（考虑行填充）
    let required_size = (plane_height as usize) * src_linesize;
    if src.len() < required_size {
        return Err(anyhow::anyhow!(
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
            byte_width,            // 要复制的宽度（字节数）
            plane_height,          // 平面高度
        );
    }

    Ok(())
}

/// 将buffer数据填充到frame中
pub fn fill_frame_from_buffer(frame: &mut AVFrame, buffer: &[u8]) -> Result<()> {
    unsafe {
        let data_ptr = frame.data_mut().as_mut_ptr();
        let linesize_ptr = frame.linesize_mut().as_mut_ptr();

        let ret = ffi::av_image_fill_arrays(
            data_ptr,
            linesize_ptr,
            buffer.as_ptr(),
            frame.format,
            frame.width,
            frame.height,
            1,
        );

        if ret < 0 {
            Err(anyhow::anyhow!("Failed to fill frame from buffer: {}", ret))
        } else {
            Ok(())
        }
    }
}

/// 将 AVFrame 转换为 ndarray::Array3
pub fn to_ndarray(frame: &AVFrame) -> Result<ndarray::Array3<u8>> {
    let (height, width) = (frame.height as usize, frame.width as usize);

    match frame.format {
        // RGB 格式：交错存储，直接复制
        f if f == ffi::AV_PIX_FMT_RGB24 => {
            let buffer = copy_frame_to_buffer(frame)?;
            ndarray::Array3::from_shape_vec((height, width, 3), buffer)
                .map_err(|e| anyhow::anyhow!("Failed to convert RGB ndarray: {}", e))
        }
        // YUV 格式：平面存储，需要分别处理每个平面并上采样
        f if f == ffi::AV_PIX_FMT_YUV420P => {
            let mut array = ndarray::Array3::zeros((height, width, 3));

            unsafe {
                // 复制 Y 平面到通道 0
                let y_plane =
                    std::slice::from_raw_parts(frame.data[0], height * frame.linesize[0] as usize);
                for (y, src_row) in y_plane
                    .chunks(frame.linesize[0] as usize)
                    .take(height)
                    .enumerate()
                {
                    array
                        .slice_mut(ndarray::s![y, .., 0])
                        .assign(&ndarray::ArrayView1::from(&src_row[..width]));
                }

                // 复制 U 和 V 平面到通道 1 和 2，并进行上采样
                for (plane_idx, &plane_ptr) in [frame.data[1], frame.data[2]].iter().enumerate() {
                    let uv_plane = std::slice::from_raw_parts(
                        plane_ptr,
                        (height / 2) * frame.linesize[1] as usize,
                    );
                    for (y, src_row) in uv_plane
                        .chunks(frame.linesize[1] as usize)
                        .take(height / 2)
                        .enumerate()
                    {
                        for (x, &val) in src_row.iter().take(width / 2).enumerate() {
                            let c = plane_idx + 1; // 通道索引：U=1, V=2
                            let y2 = y * 2;
                            let x2 = x * 2;
                            array[[y2, x2, c]] = val;
                            array[[y2, x2 + 1, c]] = val;
                            array[[y2 + 1, x2, c]] = val;
                            array[[y2 + 1, x2 + 1, c]] = val;
                        }
                    }
                }
            }

            Ok(array)
        }
        _ => Err(anyhow::anyhow!(
            "Unsupported pixel format to ndarray: {}",
            frame.format
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_glyph::PxScale;
    use anyhow::Context;
    use image::{ImageBuffer, Rgb};

    const OUTPUT_DIR: &str = "output";

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
        let rgb = create_image_with_text(640, 480, "Hello, world!");
        rgb.save(format!("{}/image_with_text.png", OUTPUT_DIR))?;
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
        let size = get_buffer_size(&frame, 1)?;
        assert_eq!(size, 640 * 480 * 3);

        // 测试YUV420P格式
        let frame = create_test_frame(640, 480, ffi::AV_PIX_FMT_YUV420P)?;
        let size = get_buffer_size(&frame, 1)?;
        assert_eq!(size, 640 * 480 * 3 / 2); // YUV420P 大小是 RGB24 的3/2

        Ok(())
    }

    #[test]
    fn test_copy_to_buffer() {
        // 测试RGB24格式
        let mut frame = create_test_frame(320, 240, ffi::AV_PIX_FMT_RGB24).unwrap();

        // 填充测试数据
        let rgb_data = vec![128u8; 320 * 240 * 3];
        fill_frame_from_buffer(&mut frame, &rgb_data).unwrap();

        let buffer = copy_frame_to_buffer(&frame).unwrap();
        assert_eq!(buffer.len(), 320 * 240 * 3);
        assert_eq!(buffer[0], 128);

        // 测试YUV420P格式
        let mut frame = create_test_frame(320, 240, ffi::AV_PIX_FMT_YUV420P).unwrap();

        // 使用 copy_plane 分别填充 Y、U、V 平面
        let y_data = vec![128u8; 320 * 240];
        let u_data = vec![128u8; 160 * 120];
        let v_data = vec![128u8; 160 * 120];

        fill_plane_from_buffer(&mut frame, 0, &y_data, 320).unwrap();
        fill_plane_from_buffer(&mut frame, 1, &u_data, 160).unwrap();
        fill_plane_from_buffer(&mut frame, 2, &v_data, 160).unwrap();

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
        fill_plane_from_buffer(&mut frame, 0, &y_data, width as usize).unwrap();
        fill_plane_from_buffer(&mut frame, 1, &u_data, uv_width as usize).unwrap();
        fill_plane_from_buffer(&mut frame, 2, &v_data, uv_width as usize).unwrap();

        // 直接从帧数据指针读取数据进行验证
        unsafe {
            // 验证 Y 平面数据
            let y_linesize = frame.linesize[0] as usize;
            let y_ptr = frame.data[0] as *const u8;
            for y in 0..height as usize {
                let row_ptr = y_ptr.add(y * y_linesize);
                let row = std::slice::from_raw_parts(row_ptr, width as usize);
                for x in 0..width as usize {
                    assert_eq!(row[x], 100, "Y plane data mismatch at ({}, {})", x, y);
                }
            }

            // 验证 U 平面数据
            let u_linesize = frame.linesize[1] as usize;
            let u_ptr = frame.data[1] as *const u8;
            for y in 0..uv_height as usize {
                let row_ptr = u_ptr.add(y * u_linesize);
                let row = std::slice::from_raw_parts(row_ptr, uv_width as usize);
                for x in 0..uv_width as usize {
                    assert_eq!(row[x], 150, "U plane data mismatch at ({}, {})", x, y);
                }
            }

            // 验证 V 平面数据
            let v_linesize = frame.linesize[2] as usize;
            let v_ptr = frame.data[2] as *const u8;
            for y in 0..uv_height as usize {
                let row_ptr = v_ptr.add(y * v_linesize);
                let row = std::slice::from_raw_parts(row_ptr, uv_width as usize);
                for x in 0..uv_width as usize {
                    assert_eq!(row[x], 200, "V plane data mismatch at ({}, {})", x, y);
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
        fill_frame_from_buffer(&mut src_frame, &test_data)?;

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
        copy_frame(&src_frame, &mut dst_frame)?;

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

    #[test]
    fn test_fill_frame_from_buffer() -> Result<()> {
        let mut frame = create_test_frame(320, 240, ffi::AV_PIX_FMT_RGB24)?;

        // 创建正确大小的buffer
        let buffer_size = get_buffer_size(&frame, 1)?;
        let buffer = vec![128u8; buffer_size];

        // 测试正常填充
        fill_frame_from_buffer(&mut frame, &buffer)?;

        // 验证数据是否正确填充
        let result_buffer = copy_frame_to_buffer(&frame)?;
        assert_eq!(result_buffer[0], 128);

        Ok(())
    }

    #[test]
    fn test_to_ndarray() {
        let width = 320_usize;
        let height = 240_usize;

        // 测试 RGB24 格式
        let mut frame =
            create_test_frame(width as i32, height as i32, ffi::AV_PIX_FMT_RGB24).unwrap();

        // 填充测试数据
        let rgb_data = vec![128u8; width * height * 3];
        fill_frame_from_buffer(&mut frame, &rgb_data).unwrap();

        let array = to_ndarray(&frame).unwrap();
        assert_eq!(array.shape(), &[height, width, 3]);
        assert_eq!(array[[0, 0, 0]], 128);

        // 测试 YUV420P 格式
        let mut frame =
            create_test_frame(width as i32, height as i32, ffi::AV_PIX_FMT_YUV420P).unwrap();

        // 填充测试数据
        let y_data = vec![128u8; width * height];
        let u_data = vec![64u8; (width / 2) * (height / 2)];
        let v_data = vec![32u8; (width / 2) * (height / 2)];
        fill_plane_from_buffer(&mut frame, 0, &y_data, width).unwrap();
        fill_plane_from_buffer(&mut frame, 1, &u_data, width / 2).unwrap();
        fill_plane_from_buffer(&mut frame, 2, &v_data, width / 2).unwrap();

        // 转换为 ndarray
        let array = to_ndarray(&frame).unwrap();

        // 验证 Y 平面
        for y in 0..height {
            for x in 0..width {
                assert_eq!(array[[y, x, 0]], 128);
            }
        }

        // 验证 U 平面
        for y in (0..height).step_by(2) {
            for x in (0..width).step_by(2) {
                assert_eq!(array[[y, x, 1]], 64);
                assert_eq!(array[[y + 1, x, 1]], 64);
                assert_eq!(array[[y, x + 1, 1]], 64);
                assert_eq!(array[[y + 1, x + 1, 1]], 64);
            }
        }

        // 验证 V 平面
        for y in (0..height).step_by(2) {
            for x in (0..width).step_by(2) {
                assert_eq!(array[[y, x, 2]], 32);
                assert_eq!(array[[y + 1, x, 2]], 32);
                assert_eq!(array[[y, x + 1, 2]], 32);
                assert_eq!(array[[y + 1, x + 1, 2]], 32);
            }
        }
    }

    #[test]
    fn test_frame_integration() {
        // 测试完整的操作流程
        let mut src_frame = create_test_frame(320, 240, ffi::AV_PIX_FMT_RGB24).unwrap();

        // 1. 填充原始数据
        let test_data = vec![128u8; 320 * 240 * 3];
        fill_frame_from_buffer(&mut src_frame, &test_data).unwrap();

        // 2. 复制到buffer
        let buffer = copy_frame_to_buffer(&src_frame).unwrap();

        // 3. 从buffer创建新frame
        let mut dst_frame = create_test_frame(320, 240, ffi::AV_PIX_FMT_RGB24).unwrap();
        assert!(fill_frame_from_buffer(&mut dst_frame, &buffer).is_ok());

        // 4. 转换为ndarray
        let array = to_ndarray(&dst_frame).unwrap();
        assert_eq!(array.shape(), &[240, 320, 3]);
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
        fill_plane_from_buffer(&mut frame, 0, &y_data, width as usize)?;
        fill_plane_from_buffer(&mut frame, 1, &u_data, uv_width as usize)?;
        fill_plane_from_buffer(&mut frame, 2, &v_data, uv_width as usize)?;

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
                for x in 0..width as usize {
                    assert_eq!(row[x], 100, "Y plane data mismatch at ({}, {})", x, y);
                }
            }

            // 验证 U 平面
            let u_linesize = frame.linesize[1] as usize;
            let u_ptr = frame.data[1] as *const u8;
            for y in 0..uv_height as usize {
                let row_ptr = u_ptr.add(y * u_linesize);
                let row = std::slice::from_raw_parts(row_ptr, uv_width as usize);
                for x in 0..uv_width as usize {
                    assert_eq!(row[x], 150, "U plane data mismatch at ({}, {})", x, y);
                }
            }

            // 验证 V 平面
            let v_linesize = frame.linesize[2] as usize;
            let v_ptr = frame.data[2] as *const u8;
            for y in 0..uv_height as usize {
                let row_ptr = v_ptr.add(y * v_linesize);
                let row = std::slice::from_raw_parts(row_ptr, uv_width as usize);
                for x in 0..uv_width as usize {
                    assert_eq!(row[x], 200, "V plane data mismatch at ({}, {})", x, y);
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

        fill_plane_from_buffer(&mut frame, 0, &y_data, width as usize)?;
        fill_plane_from_buffer(&mut frame, 1, &u_data, width as usize)?;
        fill_plane_from_buffer(&mut frame, 2, &v_data, width as usize)?;

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
                for x in 0..width as usize {
                    assert_eq!(
                        row[x], 100,
                        "YUV444P Y plane data mismatch at ({}, {})",
                        x, y
                    );
                }
            }

            // 验证 U 平面
            let u_linesize = frame.linesize[1] as usize;
            let u_ptr = frame.data[1] as *const u8;
            for y in 0..height as usize {
                let row_ptr = u_ptr.add(y * u_linesize);
                let row = std::slice::from_raw_parts(row_ptr, width as usize);
                for x in 0..width as usize {
                    assert_eq!(
                        row[x], 150,
                        "YUV444P U plane data mismatch at ({}, {})",
                        x, y
                    );
                }
            }

            // 验证 V 平面
            let v_linesize = frame.linesize[2] as usize;
            let v_ptr = frame.data[2] as *const u8;
            for y in 0..height as usize {
                let row_ptr = v_ptr.add(y * v_linesize);
                let row = std::slice::from_raw_parts(row_ptr, width as usize);
                for x in 0..width as usize {
                    assert_eq!(
                        row[x], 200,
                        "YUV444P V plane data mismatch at ({}, {})",
                        x, y
                    );
                }
            }
        }

        // 测试用例3: RGBA 格式
        let mut frame = create_test_frame(width, height, ffi::AV_PIX_FMT_RGBA)?;

        // RGBA 是单平面格式，每个像素4字节
        let rgba_size = width as usize * height as usize * 4;
        let rgba_data = vec![128_u8; rgba_size];

        fill_plane_from_buffer(&mut frame, 0, &rgba_data, (width * 4) as usize)?;

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
                for x in 0..width as usize * 4 {
                    assert_eq!(row[x], 128, "RGBA plane data mismatch at ({}, {})", x, y);
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
            fill_plane_from_buffer(&mut frame, 3, &data, width as usize).is_err(),
            "Should fail for invalid plane index"
        );

        // 错误2: 不匹配的源数据大小
        let y_data = vec![100_u8; width as usize * height as usize / 2]; // 数据太小
        assert!(
            fill_plane_from_buffer(&mut frame, 0, &y_data, width as usize).is_err(),
            "Should fail for insufficient source data"
        );

        // 错误3: 不匹配的行大小
        let y_data = vec![100_u8; width as usize * height as usize];
        assert!(
            fill_plane_from_buffer(&mut frame, 0, &y_data, (width / 2) as usize).is_err(),
            "Should fail for mismatched linesize"
        );

        // 错误4: 空数据
        let empty_data = vec![];
        assert!(
            fill_plane_from_buffer(&mut frame, 0, &empty_data, width as usize).is_err(),
            "Should fail for empty data"
        );

        Ok(())
    }
}
