use anyhow::Result;
use image::RgbImage;
use rsmedia::{PixelFormat, frame};
use rsmpeg::{avutil::AVFrame, ffi};

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
    frame::convert_avframe(&rgb_frame, rgb_frame.width, rgb_frame.height, PixelFormat::YUV420P)
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
    let rgb_frame = frame::convert_avframe(&frame, frame.width, frame.height, PixelFormat::RGB24)?;
    avframe_rgb24_to_image_rgb(&rgb_frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    /// create RgbImage
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

    /// create AVFrame
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
    fn test_roundtrip_conversions() -> Result<()> {
        // Test image -> AVFrame -> image
        let original_img = create_rgb_image();

        let rgb_frame = image_rgb_to_avframe_rgb24(&original_img, 0)?;
        let converted_img = avframe_rgb24_to_image_rgb(&rgb_frame)?;
        assert_eq!(original_img.dimensions(), converted_img.dimensions());

        // Test image -> Mat -> image
        // let mat = image_to_mat(&original_img)?;
        // let converted_img2 = mat_to_image(&mat)?;
        // assert_eq!(original_img.dimensions(), converted_img2.dimensions());

        Ok(())
    }
}
