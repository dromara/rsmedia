use anyhow::Context;
use image::RgbImage;
use opencv::{core::Mat, imgproc, prelude::*};
use rsmpeg::{
    avutil::{AVFrame, AVFrameWithImage, AVImage},
    error::RsmpegError,
    ffi,
    swscale::SwsContext,
};

/// 将 RgbImage 转换为 AVFrame
pub fn rgb_image_to_avframe_rgb24(image: &RgbImage, frame_pts: i64) -> AVFrame {
    let (width, height) = image.dimensions();

    // 2. 创建源 AVFrame，并分配缓冲区
    let mut src_frame = AVFrame::new();
    src_frame.set_width(width as i32);
    src_frame.set_height(height as i32);
    src_frame.set_format(ffi::AV_PIX_FMT_RGB24);
    src_frame.set_pts(frame_pts);
    src_frame.alloc_buffer().unwrap();

    // 3. 将 image 的 RGB 数据拷贝到 src_frame 中
    // let data_arr = ndarray::Array3::from_shape_vec((height as usize, width as usize, 3), image.into_raw())
    //     .expect("Failed to create ndarray from raw image data");
    unsafe {
        let rgb_data = image.as_raw();
        let buffer_slice = std::slice::from_raw_parts_mut(src_frame.data[0], rgb_data.len());
        buffer_slice.copy_from_slice(rgb_data);
    }

    src_frame
}

/// 将 RgbImage 转换为 AVFrame
pub fn rgb_image_to_avframe_yuv420p(image: &RgbImage, frame_pts: i64) -> AVFrame {
    let rgb_frame = rgb_image_to_avframe_rgb24(image, frame_pts);
    avframe_rgb24_to_yuv420p(&rgb_frame).unwrap()
}

/// 将 AVFrame 转换为 RgbImage
/// 按行处理数据，跳过每行末尾的对齐字节，确保只有有效的像素数据被用来创建图像，因此能生成正确的 RGB 图像。
pub fn avframe_rgb24_to_rgb_image(rgb_frame: &AVFrame) -> anyhow::Result<RgbImage> {
    // 确保 AVFrame 的格式是 RGB24
    if rgb_frame.format != ffi::AV_PIX_FMT_RGB24 {
        return Err(anyhow::anyhow!("Unsupported pixel format"));
    }

    let width = rgb_frame.width as usize;
    let height = rgb_frame.height as usize;

    let frame_data = rgb_frame.data[0];
    let linesize = rgb_frame.linesize[0] as usize;

    // 创建一个缓冲区用于存储 RGB 数据
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

/// 将 AVFrame 转换为 RgbImage
/// 存在的问题： 假设图像数据是连续的，并且 line_size == width * 3，但实际情况并非总是如此。
/// 如果图像有对齐字节，直接按 line_size * height 来处理会包含额外的数据，导致图像显示错误。
///  let data_slice = unsafe {
///  // 提取整个图像数据，注意这里是 line_size * height，包含了可能的对齐字节
///  std::slice::from_raw_parts(data, line_size * height)
///  };
///
pub fn avframe_rgb24_to_rgb_image_2(rgb_frame: &AVFrame) -> anyhow::Result<RgbImage> {
    // 确保 AVFrame 的格式是 RGB24
    if rgb_frame.format != ffi::AV_PIX_FMT_RGB24 {
        return Err(anyhow::anyhow!("Unsupported pixel format"));
    }

    let width = rgb_frame.width as usize;
    let height = rgb_frame.height as usize;

    let data_ptr = rgb_frame.data[0] as *const u8;
    let linesize = rgb_frame.linesize[0] as usize;

    let buffer = unsafe { std::slice::from_raw_parts(data_ptr, linesize * height) };

    let image = RgbImage::from_raw(width as u32, height as u32, buffer.to_vec())
        .context("Failed to create RGB image")?;

    Ok(image)
}

/// 将 AVFrame RGB24 转换为 YUV420P 格式
pub fn avframe_rgb24_to_yuv420p(rgb_frame: &AVFrame) -> anyhow::Result<AVFrame> {
    // 确保 AVFrame 的格式是 RGB24
    if rgb_frame.format != ffi::AV_PIX_FMT_RGB24 {
        return Err(anyhow::anyhow!("Unsupported pixel format"));
    }

    // 定义输出格式
    let src_format = rgb_frame.format;
    let dst_format = ffi::AV_PIX_FMT_YUV420P;

    // 4. 创建目标 AVFrame (YUV420P 格式)
    let mut dst_frame = AVFrame::new();
    dst_frame.set_width(rgb_frame.width);
    dst_frame.set_height(rgb_frame.height);
    dst_frame.set_format(dst_format);
    dst_frame.set_pts(rgb_frame.pts);
    dst_frame.set_time_base(rgb_frame.time_base);
    dst_frame.set_pict_type(rgb_frame.pict_type);
    dst_frame.alloc_buffer()?;

    // 5. 创建 sws_context
    let mut sws_context = SwsContext::get_context(
        rgb_frame.width,
        rgb_frame.height,
        src_format,
        rgb_frame.width,
        rgb_frame.height,
        dst_format,
        ffi::SWS_BILINEAR | ffi::SWS_FULL_CHR_H_INT | ffi::SWS_ACCURATE_RND,
        None,
        None,
        None,
    )
    .context("Failed to create SwsContext")
    .unwrap();

    // 6. 执行 sws_context.scale 转换
    unsafe {
        let src_stride = &rgb_frame.linesize[0] as *const i32; // 源图像的每行步幅
        let dst_stride = &dst_frame.linesize[0] as *const i32; // 目标图像的每行步幅

        // 使用 scale 函数进行图像转换 (RGB -> YUV420P)
        let _ = sws_context.scale(
            rgb_frame.data.as_ptr() as *const *const u8, // 源图像数据
            src_stride,                                  // 源图像每行步幅
            0,                                           // 开始处理的行
            rgb_frame.height,                            // 要处理的行数
            dst_frame.data.as_ptr() as *const *mut u8,   // 目标图像数据
            dst_stride,                                  // 目标图像每行步幅
        )?;
    }

    Ok(dst_frame)
}

/// 将 AVFrame YUV 转换为 RGB24 格式
pub fn avframe_yuv_to_rgb24(yuv_frame: &AVFrame) -> anyhow::Result<AVFrameWithImage> {
    // 定义输出格式
    let src_pix_fmt = yuv_frame.format;
    let dst_pix_fmt = ffi::AV_PIX_FMT_RGB24;

    let mut sws_context = SwsContext::get_context(
        yuv_frame.width,
        yuv_frame.height,
        src_pix_fmt,
        yuv_frame.width,
        yuv_frame.height,
        dst_pix_fmt,
        ffi::SWS_BILINEAR | ffi::SWS_FULL_CHR_H_INT,
        None,
        None,
        None,
    )
    .context("Invalid swscontext parameter.")?;

    let image_buffer = AVImage::new(dst_pix_fmt, yuv_frame.width, yuv_frame.height, 1)
        .context("Image buffer parameter invalid.")?;

    let mut rgb_frame = AVFrameWithImage::new(image_buffer);

    let _ = sws_context
        .scale_frame(&yuv_frame, 0, yuv_frame.height, &mut rgb_frame)
        .context("Failed to scale frame")?;

    Ok(rgb_frame)
}

/// 将 AVFrame YUV420P 转换为 RGB24 格式
pub fn avframe_yuv_to_rgb24_2(yuv_frame: &AVFrame) -> anyhow::Result<AVFrame> {
    // 定义输出格式
    let src_pix_fmt = yuv_frame.format;
    let dst_pix_fmt = ffi::AV_PIX_FMT_RGB24;

    // 创建转换上下文，将帧转换为 RGB 格式
    let mut sws_ctx = SwsContext::get_context(
        yuv_frame.width,
        yuv_frame.height,
        src_pix_fmt,
        yuv_frame.width,
        yuv_frame.height,
        dst_pix_fmt,
        ffi::SWS_BILINEAR | ffi::SWS_FULL_CHR_H_INT,
        None,
        None,
        None,
    )
    .ok_or(RsmpegError::Unknown)?;

    // 创建目标缓冲区
    let mut rgb_frame = AVFrame::new();
    rgb_frame.set_format(dst_pix_fmt);
    rgb_frame.set_width(yuv_frame.width);
    rgb_frame.set_height(yuv_frame.height);
    rgb_frame.set_pts(yuv_frame.pts);
    rgb_frame.set_time_base(yuv_frame.time_base);
    rgb_frame.set_pict_type(yuv_frame.pict_type);
    rgb_frame.alloc_buffer()?;

    // 设置边距
    let src_slice = yuv_frame.data.as_ptr() as *const *const u8;
    let dest_slice = rgb_frame.data.as_ptr() as *const *mut u8;

    // 转换为 RGB 格式
    unsafe {
        let _ = sws_ctx.scale(
            src_slice,
            yuv_frame.linesize.as_ptr(),
            0,
            yuv_frame.height,
            dest_slice,
            rgb_frame.linesize.as_ptr(),
        )?;
    }

    Ok(rgb_frame)
}

/// AVFrame rgb24 converter to OpenCV Mat
pub fn avframe_rgb24_to_mat(frame: &AVFrame) -> opencv::Result<Mat> {
    // Only support RGB24
    if frame.format != ffi::AV_PIX_FMT_RGB24 as i32 {
        return Err(opencv::Error::new(
            opencv::core::StsBadArg,
            "Unsupported pixel format",
        ));
    }

    let width = frame.width as i32;
    let height = frame.height as i32;
    let linesize = frame.linesize[0] as i32;
    println!(
        "avframe_rgb24_to_mat width: {}, height: {}, linesize: {}",
        width, height, linesize
    );

    // convert the frame data to a slice of bytes
    let data = unsafe { std::slice::from_raw_parts(frame.data[0], (linesize * height) as usize) };

    // 3 channels (RGB)
    let mat = Mat::from_slice(data)?.reshape(3, height)?.try_clone()?;

    // Convert RGB to BGR
    let mut bgr_mat = Mat::default();
    imgproc::cvt_color_def(&mat, &mut bgr_mat, imgproc::COLOR_RGB2BGR)?;

    Ok(bgr_mat)
}

//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////
/// 图像格式转换器
pub struct AVConverter;

impl AVConverter {
    pub fn new() -> Self {
        AVConverter {}
    }
}

impl AVConverter {
    /// AVFrame (YUV) 转换为 RgbImage
    pub fn avframe_to_image(&self, frame: &AVFrame) -> anyhow::Result<RgbImage> {
        let width = frame.width as u32;
        let height = frame.height as u32;

        let dist_pix_fmt = ffi::AV_PIX_FMT_RGB24;

        // 创建 RGB 格式的目标图像缓冲区
        let rgb_image = AVImage::new(dist_pix_fmt, width as i32, height as i32, 1)
            .context("Failed to create RGB image buffer")?;

        // 创建带有图像缓冲区的目标帧
        let mut rgb_frame = AVFrameWithImage::new(rgb_image);

        // 创建缩放上下文
        let mut sws_context = SwsContext::get_context(
            width as i32,
            height as i32,
            unsafe { std::mem::transmute(frame.format) },
            width as i32,
            height as i32,
            dist_pix_fmt,
            ffi::SWS_BILINEAR,
            None,
            None,
            None,
        )
        .context("Failed to create scaling context")?;

        // 执行颜色空间转换
        sws_context
            .scale_frame(frame, 0, height as i32, &mut rgb_frame)
            .context("Failed to convert frame to RGB")?;

        // 安全地获取 RGB 数据
        let rgb_data = unsafe {
            // RGB 每像素3字节
            std::slice::from_raw_parts(rgb_frame.data[0], width as usize * height as usize * 3)
        }
        .to_vec();

        // 将数据转换为 RgbImage
        RgbImage::from_raw(width, height, rgb_data)
            .context("Failed to create RgbImage from frame data")
    }

    /// RgbImage 转换为 AVFrame (YUV420P)
    pub fn image_to_avframe(&self, img: &RgbImage) -> anyhow::Result<AVFrame> {
        Ok(rgb_image_to_avframe_yuv420p(img, 0))
    }

    /// OpenCV Mat 转换为 RgbImage
    pub fn mat_to_image(&self, mat: &Mat) -> anyhow::Result<RgbImage> {
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

    /// RgbImage 转换为 OpenCV Mat
    pub fn image_to_mat(&self, img: &RgbImage) -> anyhow::Result<Mat> {
        let width = img.width() as i32;
        let height = img.height() as i32;

        // 创建 RGB Mat
        let rgb_mat = unsafe {
            Mat::new_rows_cols_with_data_unsafe(
                height,
                width,
                opencv::core::CV_8UC3,
                img.as_raw().as_ptr() as *mut _,
                opencv::core::Mat_AUTO_STEP,
            )
            .context("Failed to create RGB Mat")?
        };

        // 转换为 BGR
        let mut bgr_mat = Mat::default();
        imgproc::cvt_color_def(&rgb_mat, &mut bgr_mat, imgproc::COLOR_RGB2BGR)
            .context("Failed to convert RGB to BGR")?;

        Ok(bgr_mat)
    }

    /// AVFrame 直接转换为 OpenCV Mat (通过 RgbImage 作为中间桥梁)
    pub fn avframe_to_mat(&self, frame: &AVFrame) -> anyhow::Result<Mat> {
        let img = Self::avframe_to_image(self, frame)?;
        Self::image_to_mat(self, &img)
    }

    /// OpenCV Mat 直接转换为 AVFrame (通过 RgbImage 作为中间桥梁)
    pub fn mat_to_avframe(&self, mat: &Mat) -> anyhow::Result<AVFrame> {
        let img = Self::mat_to_image(self, mat)?;
        Self::image_to_avframe(self, &img)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Error, Result};
    use opencv::core::{Point, Scalar, Vec3b};
    use opencv::{core::MatTraitConst, imgcodecs};

    // 测试辅助函数：创建示例 RgbImage
    fn create_test_image() -> RgbImage {
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
    fn create_test_avframe() -> Result<AVFrame> {
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
        let frame = create_test_avframe()?;

        let converter = AVConverter::new();
        let img = converter.avframe_to_image(&frame)?;

        assert_eq!(img.width(), frame.width as u32);
        assert_eq!(img.height(), frame.height as u32);

        img.save("/tmp/test_avframe_to_image.png")
            .expect("avframe_to_image error");

        Ok(())
    }

    #[test]
    fn test_image_to_avframe() -> Result<()> {
        let img = create_test_image();

        let converter = AVConverter::new();
        let frame = converter.image_to_avframe(&img)?;

        assert_eq!(frame.width as u32, img.width());
        assert_eq!(frame.height as u32, img.height());
        assert_eq!(frame.format, ffi::AV_PIX_FMT_YUV420P as i32);
        println!(
            "frame.width: {}, frame.height: {}, frame.format: {}",
            frame.width, frame.height, frame.format
        );

        Ok(())
    }

    #[test]
    fn test_mat_to_image() -> Result<()> {
        let mat = create_test_mat()?;

        let converter = AVConverter::new();
        let img = converter.mat_to_image(&mat)?;

        assert_eq!(img.width(), mat.cols() as u32);
        assert_eq!(img.height(), mat.rows() as u32);

        img.save("/tmp/test_mat_to_image.png")
            .expect("mat_to_image error");

        Ok(())
    }

    #[test]
    fn test_image_to_mat() -> Result<()> {
        let img = create_test_image();

        let converter = AVConverter::new();
        let mat = converter.image_to_mat(&img)?;

        assert_eq!(mat.cols(), img.width() as i32);
        assert_eq!(mat.rows(), img.height() as i32);
        assert_eq!(mat.channels(), 3);

        println!("test_image_to_mat depth:{}", mat.depth());

        Ok(())
    }

    #[test]
    fn test_avframe_to_mat() -> Result<()> {
        let frame = create_test_avframe()?;

        let converter = AVConverter::new();
        let mat = converter.avframe_to_mat(&frame)?;

        assert_eq!(mat.cols(), frame.width);
        assert_eq!(mat.rows(), frame.height);
        assert_eq!(mat.channels(), 3);

        Ok(())
    }

    #[test]
    fn test_mat_to_avframe() -> Result<()> {
        let mat = create_test_mat()?;

        let converter = AVConverter::new();
        let frame = converter.mat_to_avframe(&mat)?;

        assert_eq!(frame.width, mat.cols());
        assert_eq!(frame.height, mat.rows());
        assert_eq!(frame.format, ffi::AV_PIX_FMT_YUV420P as i32);

        Ok(())
    }

    #[test]
    fn test_roundtrip_conversions() -> Result<()> {
        // Test image -> AVFrame -> image
        let original_img = create_test_image();

        let converter = AVConverter::new();
        let frame = converter.image_to_avframe(&original_img)?;
        let converted_img = converter.avframe_to_image(&frame)?;

        assert_eq!(original_img.dimensions(), converted_img.dimensions());

        // Test image -> Mat -> image
        let mat = converter.image_to_mat(&original_img)?;
        let converted_img2 = converter.mat_to_image(&mat)?;

        assert_eq!(original_img.dimensions(), converted_img2.dimensions());

        Ok(())
    }
}
