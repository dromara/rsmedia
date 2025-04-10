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

/// 将数据复制到指定的帧平面中
///
/// # Arguments
///
/// * `frame` - 目标 AVFrame
/// * `plane_idx` - 平面索引 (例如 YUV420P 格式中：0=Y, 1=U, 2=V)
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
    src_linesize: i32,
) -> Result<()> {
    unsafe {
        // 检查帧是否有效
        if frame.width * frame.height <= 0 {
            return Err(Error::msg("Invalid frame dimensions"));
        }

        // 获取该格式的平面数
        let planes = ffi::av_pix_fmt_count_planes(frame.format);
        if planes < 0 {
            return Err(Error::msg("Invalid pixel format"));
        }

        // 检查平面索引是否有效
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

        if src_linesize <= 0 {
            return Err(anyhow::anyhow!("Invalid source linesize"));
        }

        // 计算这个平面的实际高度
        // 对于YUV420P格式，U和V平面的高度是Y平面的一半
        let plane_height = if frame.format == ffi::AV_PIX_FMT_YUV420P && plane_idx > 0 {
            frame.height / 2
        } else {
            frame.height
        };

        // 确保帧是可写的
        if !frame.is_writable()? {
            return Err(Error::msg("Frame is not writable"));
        }

        // 复制平面数据
        ffi::av_image_copy_plane(
            frame.data[plane_idx],                       // 目标数据指针
            frame.linesize[plane_idx],                   // 目标行大小
            src.as_ptr(),                                // 源数据指针
            src_linesize,                                // 源数据行大小
            frame.linesize[plane_idx].min(src_linesize), // 使用较小的行大小
            plane_height,                                // 平面高度
        );

        Ok(())
    }
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
        f if f == ffi::AV_PIX_FMT_RGB24 => {
            // RGB24 格式：直接将帧数据转化为 (height, width, 3) 布局
            let buffer = copy_frame_to_buffer(frame)?;
            ndarray::Array3::from_shape_vec((height, width, 3), buffer)
                .map_err(|e| anyhow::anyhow!("Failed to convert RGB ndarray: {}", e))
        }
        f if f == ffi::AV_PIX_FMT_YUV420P => {
            // YUV420P 格式：需要处理 Y、U、V 平面并上采样
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
            "Unsupported pixel format: {}",
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

        // 使用 fill_frame_from_buffer 填充测试数据
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
        let mut frame = create_test_frame(320, 240, ffi::AV_PIX_FMT_YUV420P).unwrap();

        // 测试Y平面
        let y_data = vec![128u8; 320 * 240];
        assert!(fill_plane_from_buffer(&mut frame, 0, &y_data, 320).is_ok());

        // 测试U平面
        let u_data = vec![128u8; 160 * 120];
        assert!(fill_plane_from_buffer(&mut frame, 1, &u_data, 160).is_ok());

        //? 测试无效平面索引
        assert!(fill_plane_from_buffer(&mut frame, 4, &y_data, 320).is_err());

        //? 测试无效的src_linesize
        assert!(fill_plane_from_buffer(&mut frame, 0, &y_data, -1).is_err());
    }

    #[test]
    fn test_frame_copy() -> Result<()> {
        // 创建源frame和目标frame
        let mut src_frame = create_test_frame(320, 240, ffi::AV_PIX_FMT_RGB24)?;
        let mut dst_frame = create_test_frame(320, 240, ffi::AV_PIX_FMT_RGB24)?;

        // 使用 fill_frame_from_buffer 填充源frame
        let test_data = vec![128u8; 320 * 240 * 3];
        fill_frame_from_buffer(&mut src_frame, &test_data)?;

        // 测试复制
        copy_frame(&src_frame, &mut dst_frame)?;

        // 验证数据是否正确复制
        let dst_buffer = copy_frame_to_buffer(&dst_frame)?;
        assert_eq!(dst_buffer[0], 128);

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

        // 使用 fill_frame_from_buffer 填充测试数据
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
        fill_plane_from_buffer(&mut frame, 0, &y_data, width as i32).unwrap();
        fill_plane_from_buffer(&mut frame, 1, &u_data, (width / 2) as i32).unwrap();
        fill_plane_from_buffer(&mut frame, 2, &v_data, (width / 2) as i32).unwrap();

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
}
