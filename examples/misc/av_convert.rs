use anyhow::Context;
use anyhow::{Error, Result};
use image::RgbImage;
use opencv::{core::Mat, imgproc, prelude::*};
use rsmpeg::{avutil::AVFrame, ffi, swscale::SwsContext};

/// 将 RgbImage 转换为 AVFrame
pub fn image_rgb_to_avframe_rgb24(image: &RgbImage, frame_pts: i64) -> Result<AVFrame> {
    let (width, height) = image.dimensions();

    // 2. 创建源 AVFrame，并分配缓冲区
    let mut frame = AVFrame::new();
    frame.set_width(width as i32);
    frame.set_height(height as i32);
    frame.set_format(ffi::AV_PIX_FMT_RGB24);
    frame.set_pts(frame_pts);
    frame.alloc_buffer().unwrap();

    // 3. 将 image 的 RGB 数据拷贝到 src_frame 中
    // let data_arr = ndarray::Array3::from_shape_vec((height as usize, width as usize, 3), image.into_raw())
    //     .expect("Failed to create ndarray from raw image data");
    unsafe {
        let rgb_data = image.as_raw();
        let buffer_slice = std::slice::from_raw_parts_mut(frame.data[0], rgb_data.len());
        buffer_slice.copy_from_slice(rgb_data);
    }

    Ok(frame)
}

/// 将 RgbImage 转换为 AVFrame
pub fn image_rgb_to_avframe_yuv420p(image: &RgbImage, frame_pts: i64) -> Result<AVFrame> {
    let rgb_frame = image_rgb_to_avframe_rgb24(image, frame_pts)?;
    avframe_convert(
        &rgb_frame,
        rgb_frame.width,
        rgb_frame.height,
        ffi::AV_PIX_FMT_YUV420P,
    )
}

/// 将 AVFrame RGB24 转换为 RgbImage
/// 按行处理数据，跳过每行末尾的对齐字节，确保只有有效的像素数据被用来创建图像，因此能生成正确的 RGB 图像。
pub fn avframe_rgb24_to_image_rgb(rgb_frame: &AVFrame) -> Result<RgbImage> {
    // 确保 AVFrame 的格式是 RGB24
    if rgb_frame.format != ffi::AV_PIX_FMT_RGB24 {
        return Err(anyhow::anyhow!("Unsupported pixel format"));
    }

    let width = rgb_frame.width as usize;
    let height = rgb_frame.height as usize;
    let frame_data = rgb_frame.data[0];
    let linesize = rgb_frame.linesize[0] as usize;

    // 方法一：
    // 存在的问题： 假设图像数据是连续的，并且 line_size == width * 3，但实际情况并非总是如此。
    // 如果图像有对齐字节，直接按 line_size * height 来处理会包含额外的数据，导致图像显示错误。
    // let buffer = unsafe { std::slice::from_raw_parts(frame_data as *const u8, linesize * height) };

    // 方法二：
    // 按行处理数据，跳过每行末尾的对齐字节，确保只有有效的像素数据被用来创建图像，因此能生成正确的 RGB 图像
    let mut buffer: Vec<u8> = Vec::with_capacity(width * height * 3);
    // 逐行读取 AVFrame 的数据，确保正确处理每行的 linesize
    for y in 0..height {
        let offset = y * linesize;
        let src = unsafe { std::slice::from_raw_parts(frame_data.add(offset), width * 3) };
        buffer.extend_from_slice(src);
    }

    // 使用 buffer 数据创建 RgbImage
    // 第一种方式（RgbImage） 更简洁、明确，并且适用于绝大多数场景，因为它将通道类型和缓冲区类型都固定为常见的组合（Rgb<u8> 和 Vec<u8>）。
    let rgb_image = RgbImage::from_raw(width as u32, height as u32, buffer)
        .ok_or_else(|| "Failed to create RgbImage")
        .unwrap();

    // 第二种方式（ImageBuffer<Rgb<u8>, _>） 更加通用。你可以使用不同类型的缓冲区（如 &[u8]、Box<[u8]> 等），
    // 而不仅仅是 Vec<u8>。它为你提供了更大的灵活性，但也稍微冗长。
    // let image_buffer: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_raw(width as u32, height as u32, buffer)
    //     .ok_or_else(|| "Failed to create image buffer").unwrap();

    Ok(rgb_image)
}

/// 将 AVFrame YUV420P 转换为 RgbImage
pub fn avframe_yuv420p_to_image_rgb(frame: &AVFrame) -> Result<RgbImage> {
    let rgb_frame =
        avframe_convert(&frame, frame.width, frame.height, ffi::AV_PIX_FMT_RGB24).unwrap();
    avframe_rgb24_to_image_rgb(&rgb_frame)
}

/// 将 AVFrame RGB24 转换为 YUV420P 格式
pub fn avframe_convert(
    src_frame: &AVFrame,
    dst_width: i32,
    dst_height: i32,
    dst_format: i32,
) -> Result<AVFrame> {
    // 创建目标 AVFrame
    let mut dst_frame = AVFrame::new();
    dst_frame.set_width(dst_width);
    dst_frame.set_height(dst_height);
    dst_frame.set_format(dst_format);
    dst_frame.set_pts(src_frame.pts);
    dst_frame.set_time_base(src_frame.time_base);
    dst_frame.set_pict_type(src_frame.pict_type);
    dst_frame.alloc_buffer()?;

    // 5. 创建 sws_context
    let mut sws_context = SwsContext::get_context(
        src_frame.width,
        src_frame.height,
        src_frame.format,
        dst_width,
        dst_height,
        dst_format,
        ffi::SWS_BILINEAR | ffi::SWS_FULL_CHR_H_INT | ffi::SWS_ACCURATE_RND | ffi::SWS_BITEXACT,
        None,
        None,
        None,
    )
    .context("Failed to create SwsContext")?;

    // scale_frame
    sws_context
        .scale_frame(src_frame, 0, src_frame.height, &mut dst_frame)
        .context("Failed to scale frame")?;

    Ok(dst_frame)
}

/// AVFrame rgb24 converter to OpenCV Mat
pub fn avframe_to_mat(frame: &AVFrame) -> Result<Mat> {
    // 获取 frame 的基本信息
    let width = frame.width;
    let height = frame.height;
    let format = frame.format;

    // 根据源格式决定目标 OpenCV 格式
    let (cv_type, need_convert) = match format {
        // BGR24 格式可以直接转换
        f if f == ffi::AV_PIX_FMT_BGR24 => (opencv::core::CV_8UC3, false),
        // BGR32/BGRA 格式可以直接转换
        f if f == ffi::AV_PIX_FMT_BGR32 || f == ffi::AV_PIX_FMT_BGRA => {
            (opencv::core::CV_8UC4, false)
        }
        // GRAY8 格式可以直接转换
        f if f == ffi::AV_PIX_FMT_GRAY8 => (opencv::core::CV_8UC1, false),
        // 其他格式需要转换到 BGR24
        _ => (opencv::core::CV_8UC3, true),
    };

    // 如果需要格式转换
    let frame = if need_convert {
        // 转换为 BGR24 格式
        avframe_convert(frame, width, height, ffi::AV_PIX_FMT_BGR24)?
    } else {
        frame.clone()
    };

    let mat = unsafe {
        // 创建 Mat
        let mut mat = Mat::new_rows_cols(height, width, cv_type)?;

        // 获取 frame 的数据指针和行大小
        let frame_ptr = frame.as_ptr();
        let src_linesize = (*frame_ptr).linesize[0] as usize;
        let src_data = (*frame_ptr).data[0];

        // 获取 mat 的数据指针和行大小
        let dst_data = mat.data_mut();
        let dst_step = mat.step1_def()? as usize;

        // 逐行复制数据
        for y in 0..height as usize {
            let src_line = src_data.add(y * src_linesize);
            let dst_line = dst_data.add(y * dst_step);
            std::ptr::copy_nonoverlapping(
                src_line,
                dst_line,
                width as usize * (cv_type >> 3) as usize,
            );
        }
        mat
    };

    // OpenCV 默认的 BGR 格式
    let mut bgr_mat = Mat::default();
    imgproc::cvt_color_def(&mat, &mut bgr_mat, imgproc::COLOR_RGB2BGR)?;

    Ok(bgr_mat)
}

pub fn mat_to_avframe(mat: &Mat) -> Result<AVFrame> {
    // 获取 mat 的基本信息
    let width = mat.cols();
    let height = mat.rows();
    let channels = mat.channels();

    // 根据 Mat 类型决定 AVFrame 格式
    let av_format = match channels {
        1 => ffi::AV_PIX_FMT_GRAY8,
        3 => ffi::AV_PIX_FMT_BGR24,
        4 => ffi::AV_PIX_FMT_BGRA,
        _ => return Err(Error::msg("Unsupported Mat format")),
    };

    // 创建 AVFrame
    let mut frame = AVFrame::new();
    frame.set_width(width);
    frame.set_height(height);
    frame.set_format(av_format);
    frame.alloc_buffer()?;

    unsafe {
        // 获取 frame 的数据指针和行大小
        let frame_ptr = frame.as_mut_ptr();
        let dst_linesize = (*frame_ptr).linesize[0] as usize;
        let dst_data = (*frame_ptr).data[0];

        // 获取 mat 的数据指针和行大小
        let src_data = mat.data();
        let src_step = mat.step1(0)? as usize;

        // 逐行复制数据
        for y in 0..height as usize {
            let src_line = src_data.add(y * src_step);
            let dst_line = dst_data.add(y * dst_linesize);
            std::ptr::copy_nonoverlapping(src_line, dst_line, width as usize * channels as usize);
        }
    }

    Ok(frame)
}

/// RgbImage 转换为 OpenCV Mat
pub fn image_to_mat(img: &RgbImage) -> Result<Mat> {
    let width = img.width() as i32;
    let height = img.height() as i32;

    // 创建 RGB Mat
    let rgb_mat = unsafe {
        Mat::new_rows_cols_with_data_unsafe_def(
            height,
            width,
            opencv::core::CV_8UC3,
            img.as_raw().as_ptr() as *mut _,
        )
        .context("Failed to create RGB Mat")?
    };

    // 转换为 openCV 默认 BGR 格式
    let mut bgr_mat = Mat::default();
    imgproc::cvt_color_def(&rgb_mat, &mut bgr_mat, imgproc::COLOR_RGB2BGR)
        .context("Failed to convert RGB to BGR")?;

    Ok(bgr_mat)
}

/// OpenCV Mat 转换为 RgbImage
pub fn mat_to_image(mat: &Mat) -> Result<RgbImage> {
    if mat.empty() {
        return Err(anyhow::anyhow!("Input Mat is empty"));
    }

    // 转换为 RGB
    let mut rgb_mat = Mat::default();
    imgproc::cvt_color_def(mat, &mut rgb_mat, imgproc::COLOR_BGR2RGB)
        .context("Failed to convert BGR to RGB")?;

    let width = rgb_mat.cols() as u32;
    let height = rgb_mat.rows() as u32;

    // 获取连续数据
    let buffer = rgb_mat
        .data_bytes()
        .context("Failed to get mat data")?
        .to_vec();

    RgbImage::from_raw(width, height, buffer).context("Failed to create RgbImage from Mat data")
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencv::core::{Point, Scalar, Vec3b};
    use opencv::{core::MatTraitConst, imgcodecs};

    // 测试辅助函数：创建示例 RgbImage
    fn create_rgb_image() -> RgbImage {
        let width = 320;
        let height = 240;
        let mut img = RgbImage::new(width, height);

        // 创建一个简单的渐变图案
        for y in 0..height {
            for x in 0..width {
                let r = (x as f32 / width as f32 * 255.0) as u8;
                let g = (y as f32 / height as f32 * 255.0) as u8;
                let b = ((x + y) as f32 / (width + height) as f32 * 255.0) as u8;
                img.put_pixel(x, y, image::Rgb([r, g, b]));
            }
        }
        img
    }

    // 测试辅助函数：创建示例 AVFrame
    fn create_yuv_avframe() -> Result<AVFrame> {
        let width = 320;
        let height = 240;
        let pixel_format = ffi::AV_PIX_FMT_YUV420P;

        unsafe {
            // 创建一个新的 AVFrame
            let mut yuv_frame = AVFrame::new();
            yuv_frame.set_width(width);
            yuv_frame.set_height(height);
            yuv_frame.set_format(pixel_format);
            yuv_frame
                .alloc_buffer()
                .context("frame alloc_buffer failed, error.")
                .unwrap();

            Ok(yuv_frame)
        }
    }

    // 测试辅助函数：创建示例 Mat
    fn create_test_mat() -> Result<Mat> {
        let width = 320;
        let height = 240;

        // 创建一个 3 通道的空白图像
        let mut mat = unsafe {
            Mat::new_rows_cols(height, width, opencv::core::CV_8UC3)
                .context("Failed to create Mat")?
        };

        // 创建渐变效果
        for y in 0..height {
            for x in 0..width {
                let b = (y * 255 / height) as u8;
                let g = (x * 255 / width) as u8;
                let r = ((x + y) * 255 / (width + height)) as u8;

                // 使用 Vec3b 设置像素值
                let color = Vec3b::from([b, g, r]);
                mat.at_2d_mut::<Vec3b>(y, x)
                    .context("Failed to set pixel value")?
                    .copy_from_slice(&color.0);
            }
        }

        // 添加一些测试图形
        // 1. 画一个矩形
        imgproc::rectangle_points(
            &mut mat,
            Point::new(50, 50),
            Point::new(100, 100),
            Scalar::new(0.0, 0.0, 255.0, 0.0), // 红色
            2,                                 // 线宽
            imgproc::LINE_8,
            0,
        )
        .context("Failed to draw rectangle")?;

        // 2. 画一个圆
        imgproc::circle(
            &mut mat,
            Point::new(width / 2, height / 2),
            40,
            Scalar::new(0.0, 255.0, 0.0, 0.0), // 绿色
            2,                                 // 线宽
            imgproc::LINE_8,
            0,
        )
        .context("Failed to draw circle")?;

        // 3. 画一条线
        imgproc::line(
            &mut mat,
            Point::new(0, 0),
            Point::new(width - 1, height - 1),
            Scalar::new(255.0, 0.0, 0.0, 0.0), // 蓝色
            2,                                 // 线宽
            imgproc::LINE_8,
            0,
        )
        .context("Failed to draw line")?;

        Ok(mat)
    }

    #[test]
    fn test_avframe_to_image() -> Result<()> {
        let yuv_frame = create_yuv_avframe()?;

        let img = avframe_yuv420p_to_image_rgb(&yuv_frame)?;

        assert_eq!(img.width(), yuv_frame.width as u32);
        assert_eq!(img.height(), yuv_frame.height as u32);

        img.save("/tmp/test_avframe_to_image.png")
            .expect("avframe_to_image error");

        Ok(())
    }

    #[test]
    fn test_image_to_avframe() -> Result<()> {
        let rgb_img = create_rgb_image();

        let frame = image_rgb_to_avframe_rgb24(&rgb_img, 0)?;

        assert_eq!(frame.width as u32, rgb_img.width());
        assert_eq!(frame.height as u32, rgb_img.height());
        assert_eq!(frame.format, ffi::AV_PIX_FMT_RGB24 as i32);
        println!(
            "frame.width: {}, frame.height: {}, frame.format: {}",
            frame.width, frame.height, frame.format
        );

        Ok(())
    }

    #[test]
    fn test_mat_to_image() -> Result<()> {
        let mat = create_test_mat()?;

        let img = mat_to_image(&mat)?;

        assert_eq!(img.width(), mat.cols() as u32);
        assert_eq!(img.height(), mat.rows() as u32);

        img.save("/tmp/test_mat_to_image.png")
            .expect("mat_to_image error");

        Ok(())
    }

    #[test]
    fn test_image_to_mat() -> Result<()> {
        let img = create_rgb_image();

        let mat = image_to_mat(&img)?;

        assert_eq!(mat.cols(), img.width() as i32);
        assert_eq!(mat.rows(), img.height() as i32);
        assert_eq!(mat.channels(), 3);

        println!("test_image_to_mat depth:{}", mat.depth());

        Ok(())
    }

    #[test]
    fn test_avframe_to_mat() -> Result<()> {
        let frame = create_yuv_avframe()?;

        let mat = avframe_to_mat(&frame)?;

        assert_eq!(mat.cols(), frame.width);
        assert_eq!(mat.rows(), frame.height);
        assert_eq!(mat.channels(), 3);

        Ok(())
    }

    #[test]
    fn test_mat_to_avframe() -> Result<()> {
        // BGR
        let mat = create_test_mat()?;

        // BGR24
        let frame = mat_to_avframe(&mat)?;

        assert_eq!(frame.width, mat.cols());
        assert_eq!(frame.height, mat.rows());
        assert_eq!(frame.format, ffi::AV_PIX_FMT_BGR24);

        Ok(())
    }

    #[test]
    fn test_roundtrip_conversions() -> Result<()> {
        // Test image -> AVFrame -> image
        let original_img = create_rgb_image();

        let rgb_frame = image_rgb_to_avframe_rgb24(&original_img, 0)?;
        let converted_img = avframe_rgb24_to_image_rgb(&rgb_frame)?;

        assert_eq!(original_img.dimensions(), converted_img.dimensions());

        // Test image -> Mat -> image
        let mat = image_to_mat(&original_img)?;
        let converted_img2 = mat_to_image(&mat)?;

        assert_eq!(original_img.dimensions(), converted_img2.dimensions());

        Ok(())
    }
}
