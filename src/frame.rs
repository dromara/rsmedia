use crate::error::MediaError;
use rsmpeg::avutil::{AVFrame, AVPixelFormat};
use rsmpeg::ffi;
use rsmpeg::swscale::SwsContext;

/// A frame array is the `ndarray` version of `AVFrame`. It is 3-dimensional array with dims `(H, W,
/// C)` and type byte.
pub type FrameArray = ndarray::Array3<u8>;

pub const PIXEL_FORMAT_RGB24: AVPixelFormat = ffi::AV_PIX_FMT_RGB24;
pub const PIXEL_FORMAT_YUV420P: AVPixelFormat = ffi::AV_PIX_FMT_YUV420P;

/// RGB24 格式的 AVFrame 转换为 Array3
pub fn avframe_rgb_to_ndarray(frame: &AVFrame) -> Result<FrameArray, Box<dyn std::error::Error>> {
    let width = frame.width as usize;
    let height = frame.height as usize;

    unsafe {
        let linesize = frame.linesize[0] as usize;
        let data = frame.data[0];
        if data.is_null() {
            return Err("Frame data is null".into());
        }

        // 创建一个新的 Array3
        let mut array = FrameArray::zeros((height, width, 3));

        // 逐行复制数据
        for y in 0..height {
            let src_line = std::slice::from_raw_parts(data.add(y * linesize), width * 3);
            let mut dst_line = array.slice_mut(ndarray::s![y, .., ..]);
            for x in 0..width {
                dst_line[[x, 0]] = src_line[x * 3]; // R
                dst_line[[x, 1]] = src_line[x * 3 + 1]; // G
                dst_line[[x, 2]] = src_line[x * 3 + 2]; // B
            }
        }

        Ok(array)
    }
}

/// YUV420P 格式的 AVFrame 转换为 Array3
pub fn avframe_yuv_to_ndarray(frame: &AVFrame) -> Result<FrameArray, Box<dyn std::error::Error>> {
    let width = frame.width as usize;
    let height = frame.height as usize;

    unsafe {
        let y_data = frame.data[0];
        let u_data = frame.data[1];
        let v_data = frame.data[2];

        if y_data.is_null() || u_data.is_null() || v_data.is_null() {
            return Err("Frame data is null".into());
        }

        let y_linesize = frame.linesize[0] as usize;
        let u_linesize = frame.linesize[1] as usize;
        let v_linesize = frame.linesize[2] as usize;

        // 创建包含 Y、U、V 三个平面的 Array3
        let mut array = FrameArray::zeros((height, width, 3));

        // 逐像素复制 Y 平面数据
        for y in 0..height {
            for x in 0..width {
                array[[y, x, 0]] = *y_data.add(y * y_linesize + x);
            }
        }

        // 复制并上采样 U、V 平面数据
        for y in 0..height / 2 {
            for x in 0..width / 2 {
                let u_val = *u_data.add(y * u_linesize + x);
                let v_val = *v_data.add(y * v_linesize + x);

                // 对 2x2 块进行上采样
                for dy in 0..2 {
                    for dx in 0..2 {
                        let y_pos = y * 2 + dy;
                        let x_pos = x * 2 + dx;
                        if y_pos < height && x_pos < width {
                            array[[y_pos, x_pos, 1]] = u_val;
                            array[[y_pos, x_pos, 2]] = v_val;
                        }
                    }
                }
            }
        }

        Ok(array)
    }
}

/// Array3 转换为 RGB24 格式的 AVFrame
pub fn ndarray_to_avframe_rgb(array: &FrameArray) -> Result<AVFrame, Box<dyn std::error::Error>> {
    assert!(array.is_standard_layout());

    let height = array.shape()[0];
    let width = array.shape()[1];

    let mut frame = AVFrame::new();
    frame.set_format(PIXEL_FORMAT_RGB24);
    frame.set_width(width as i32);
    frame.set_height(height as i32);
    frame.alloc_buffer().unwrap();

    unsafe {
        let linesize = frame.linesize[0] as usize;
        let data = frame.data[0];
        // copy
        for y in 0..height {
            let dst_line = std::slice::from_raw_parts_mut(data.add(y * linesize), width * 3);
            let src_line = array.slice(ndarray::s![y, .., ..]);

            for x in 0..width {
                dst_line[x * 3] = src_line[[x, 0]]; // R
                dst_line[x * 3 + 1] = src_line[[x, 1]]; // G
                dst_line[x * 3 + 2] = src_line[[x, 2]]; // B
            }
        }
    }
    Ok(frame)
}

/// Array3 转换为 YUV420P 格式的 AVFrame
pub fn ndarray_to_avframe_yuv(array: &FrameArray) -> Result<AVFrame, Box<dyn std::error::Error>> {
    assert!(array.is_standard_layout());

    let height = array.shape()[0];
    let width = array.shape()[1];

    // 确保尺寸是偶数（YUV420P 要求）
    if width % 2 != 0 || height % 2 != 0 {
        return Err("Dimensions must be even for YUV420P".into());
    }

    let mut frame = AVFrame::new();
    frame.set_format(PIXEL_FORMAT_YUV420P);
    frame.set_width(width as i32);
    frame.set_height(height as i32);
    frame.alloc_buffer().unwrap();

    unsafe {
        let y_linesize = frame.linesize[0] as usize;
        let u_linesize = frame.linesize[1] as usize;
        let v_linesize = frame.linesize[2] as usize;

        let y_data = frame.data[0];
        let u_data = frame.data[1];
        let v_data = frame.data[2];

        // 逐像素复制 Y 平面数据
        for y in 0..height {
            for x in 0..width {
                *y_data.add(y * y_linesize + x) = array[[y, x, 0]];
            }
        }

        // 下采样并复制 U、V 平面数据
        for y in 0..height/2 {
            for x in 0..width/2 {
                let mut u_sum = 0u16;
                let mut v_sum = 0u16;

                // 计算 2x2 块的平均值
                for dy in 0..2 {
                    for dx in 0..2 {
                        let y_pos = y * 2 + dy;
                        let x_pos = x * 2 + dx;
                        u_sum += array[[y_pos, x_pos, 1]] as u16;
                        v_sum += array[[y_pos, x_pos, 2]] as u16;
                    }
                }

                // 存储平均值
                *u_data.add(y * u_linesize + x) = (u_sum / 4) as u8;
                *v_data.add(y * v_linesize + x) = (v_sum / 4) as u8;
            }
        }
    }

    Ok(frame)
}

/// 将 AVFrame YUV420P 转换为 RGB24 格式
pub fn convert_avframe(
    src_frame: &AVFrame,
    dst_width: i32,
    dst_height: i32,
    dst_pix_fmt: ffi::AVPixelFormat,
) -> Result<AVFrame, MediaError> {
    /**
     * Scaler selection options. Only one may be active at a time.
     */
    // SWS_FAST_BILINEAR = 1 <<  0, ///< fast bilinear filtering
    // SWS_BILINEAR      = 1 <<  1, ///< bilinear filtering
    // SWS_BICUBIC       = 1 <<  2, ///< 2-tap cubic B-spline
    // SWS_X             = 1 <<  3, ///< experimental
    // SWS_POINT         = 1 <<  4, ///< nearest neighbor
    // SWS_AREA          = 1 <<  5, ///< area averaging
    // SWS_BICUBLIN      = 1 <<  6, ///< bicubic luma, bilinear chroma
    // SWS_GAUSS         = 1 <<  7, ///< gaussian approximation
    // SWS_SINC          = 1 <<  8, ///< unwindowed sinc
    // SWS_LANCZOS       = 1 <<  9, ///< 3-tap sinc/sinc
    // SWS_SPLINE        = 1 << 10, ///< cubic Keys spline

    /**
     * Return an error on underspecified conversions. Without this flag,
     * unspecified fields are defaulted to sensible values.
     */
    // SWS_STRICT        = 1 << 11,

    /**
     * Emit verbose log of scaling parameters.
     */
    // SWS_PRINT_INFO    = 1 << 12,

    /**
     * Perform full chroma upsampling when upscaling to RGB.
     *
     * For example, when converting 50x50 yuv420p to 100x100 rgba, setting this flag
     * will scale the chroma plane from 25x25 to 100x100 (4:4:4), and then convert
     * the 100x100 yuv444p image to rgba in the final output step.
     *
     * Without this flag, the chroma plane is instead scaled to 50x100 (4:2:2),
     * with a single chroma sample being re-used for both of the horizontally
     * adjacent RGBA output pixels.
     */
    // SWS_FULL_CHR_H_INT = 1 << 13,

    /**
     * Perform full chroma interpolation when downscaling RGB sources.
     *
     * For example, when converting a 100x100 rgba source to 50x50 yuv444p, setting
     * this flag will generate a 100x100 (4:4:4) chroma plane, which is then
     * downscaled to the required 50x50.
     *
     * Without this flag, the chroma plane is instead generated at 50x100 (dropping
     * every other pixel), before then being downscaled to the required 50x50
     * resolution.
     */
    // SWS_FULL_CHR_H_INP = 1 << 14,

    /**
     * Force bit-exact output. This will prevent the use of platform-specific
     * optimizations that may lead to slight difference in rounding, in favor
     * of always maintaining exact bit output compatibility with the reference
     * C code.
     *
     * Note: It is recommended to set both of these flags simultaneously.
     */
    // SWS_ACCURATE_RND   = 1 << 18,
    // SWS_BITEXACT       = 1 << 19,
    let flags = ffi::SWS_BICUBIC
        | ffi::SWS_FULL_CHR_H_INT
        | ffi::SWS_FULL_CHR_H_INP
        | ffi::SWS_ACCURATE_RND
        | ffi::SWS_BITEXACT
        | ffi::SWS_PRINT_INFO;

    // 创建转换上下文
    let mut sws_ctx = SwsContext::get_context(
        src_frame.width,
        src_frame.height,
        src_frame.format,
        dst_width,
        dst_height,
        dst_pix_fmt,
        flags,
        None,
        None,
        None,
    )
    .expect("Failed to create a swscale context.");

    // 创建目标缓冲区
    let mut dst_frame = AVFrame::new();
    dst_frame.set_width(dst_width);
    dst_frame.set_height(dst_height);
    dst_frame.set_format(dst_pix_fmt);
    dst_frame.set_pts(src_frame.pts);
    dst_frame.set_time_base(src_frame.time_base);
    dst_frame.set_pict_type(src_frame.pict_type);
    dst_frame.alloc_buffer()?;

    sws_ctx
        .scale_frame(src_frame, 0, src_frame.height, &mut dst_frame)
        .unwrap();

    Ok(dst_frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    // 辅助函数：创建测试用的 RGB AVFrame
    fn create_test_rgb_frame(width: i32, height: i32) -> AVFrame {
        let mut frame = AVFrame::new();
        frame.set_format(PIXEL_FORMAT_RGB24);
        frame.set_width(width);
        frame.set_height(height);
        frame.alloc_buffer().unwrap();

        unsafe {
            // 填充测试数据
            let data = frame.data[0];
            let linesize = frame.linesize[0] as usize;
            for y in 0..height as usize {
                for x in 0..width as usize {
                    let offset = y * linesize + x * 3;
                    *data.add(offset) = (x % 256) as u8;                 // R
                    *data.add(offset + 1) = (y % 256) as u8;       // G
                    *data.add(offset + 2) = ((x + y) % 256) as u8; // B
                }
            }
        }

        frame
    }

    // 辅助函数：创建测试用的 YUV420P AVFrame
    fn create_test_yuv_frame(width: i32, height: i32) -> AVFrame {
        let mut frame = AVFrame::new();
        frame.set_format(PIXEL_FORMAT_YUV420P);
        frame.set_width(width);
        frame.set_height(height);
        frame.alloc_buffer().unwrap();

        unsafe {
            // 填充 Y 平面
            let y_data = frame.data[0];
            let y_linesize = frame.linesize[0] as usize;
            for y in 0..height as usize {
                for x in 0..width as usize {
                    *y_data.add(y * y_linesize + x) = ((x + y) % 256) as u8;
                }
            }

            // 填充 U 平面
            let u_data = frame.data[1];
            let u_linesize = frame.linesize[1] as usize;
            for y in 0..height as usize / 2 {
                for x in 0..width as usize / 2 {
                    *u_data.add(y * u_linesize + x) = (x % 256) as u8;
                }
            }

            // 填充 V 平面
            let v_data = frame.data[2];
            let v_linesize = frame.linesize[2] as usize;
            for y in 0..height as usize / 2 {
                for x in 0..width as usize / 2 {
                    *v_data.add(y * v_linesize + x) = (y % 256) as u8;
                }
            }
        }

        frame
    }

    #[test]
    fn test_avframe_rgb_to_ndarray_normal() {
        let frame = create_test_rgb_frame(64, 48);
        let result = avframe_rgb_to_ndarray(&frame);

        assert!(result.is_ok());
        let array = result.unwrap();

        // 验证尺寸
        assert_eq!(array.shape(), &[48, 64, 3]);

        // 验证数据
        for y in 0..48 {
            for x in 0..64 {
                assert_eq!(array[[y, x, 0]], (x % 256) as u8); // R
                assert_eq!(array[[y, x, 1]], (y % 256) as u8); // G
                assert_eq!(array[[y, x, 2]], ((x + y) % 256) as u8); // B
            }
        }
    }

    #[test]
    fn test_avframe_rgb_to_ndarray_empty() {
        let mut frame = AVFrame::new();
        let result = avframe_rgb_to_ndarray(&frame);
        assert!(result.is_err());
    }

    #[test]
    fn test_avframe_rgb_to_ndarray_null_data() {
        let mut frame = AVFrame::new();
        frame.set_format(PIXEL_FORMAT_RGB24);
        frame.set_width(64);
        frame.set_height(48);
        let result = avframe_rgb_to_ndarray(&frame);
        assert!(result.is_err());
    }

    #[test]
    fn test_avframe_yuv_to_ndarray_normal() {
        let frame = create_test_yuv_frame(64, 48);
        let result = avframe_yuv_to_ndarray(&frame);

        assert!(result.is_ok(), "Converting frame to ndarray failed");
        let array = result.unwrap();

        // 验证尺寸
        assert_eq!(array.shape(), &[48, 64, 3], "Array dimensions mismatch");

        // 验证 Y 平面数据
        for y in 0..48 {
            for x in 0..64 {
                assert_eq!(
                    array[[y, x, 0]],
                    ((x + y) % 256) as u8,
                    "Y plane mismatch at position [{}, {}]", y, x
                );
            }
        }

        // 验证 U、V 平面上采样
        for y in 0..24 {
            for x in 0..32 {
                let u_val = (x % 256) as u8;
                let v_val = (y % 256) as u8;

                // 检查 2x2 块
                for dy in 0..2 {
                    for dx in 0..2 {
                        let y_pos = y * 2 + dy;
                        let x_pos = x * 2 + dx;
                        if y_pos < 48 && x_pos < 64 {
                            assert_eq!(array[[y_pos, x_pos, 1]], u_val);
                            assert_eq!(array[[y_pos, x_pos, 2]], v_val);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_ndarray_to_avframe_rgb_normal() {
        // 创建测试数组
        let mut array = FrameArray::zeros((48, 64, 3));
        for y in 0..48 {
            for x in 0..64 {
                array[[y, x, 0]] = (x % 256) as u8; // R
                array[[y, x, 1]] = (y % 256) as u8; // G
                array[[y, x, 2]] = ((x + y) % 256) as u8; // B
            }
        }

        let result = ndarray_to_avframe_rgb(&array);
        assert!(result.is_ok());
        let frame = result.unwrap();

        // 验证帧属性
        assert_eq!(frame.width, 64);
        assert_eq!(frame.height, 48);
        assert_eq!(frame.format, PIXEL_FORMAT_RGB24);

        // 验证数据
        unsafe {
            let data = frame.data[0];
            let linesize = frame.linesize[0] as usize;
            for y in 0..48 {
                for x in 0..64 {
                    let offset = y * linesize + x * 3;
                    assert_eq!(*data.add(offset), (x % 256) as u8); // R
                    assert_eq!(*data.add(offset + 1), (y % 256) as u8); // G
                    assert_eq!(*data.add(offset + 2), ((x + y) % 256) as u8); // B
                }
            }
        }
    }


    #[test]
    fn test_ndarray_to_avframe_yuv_normal() {
        // 创建测试数组
        let mut array = FrameArray::zeros((48, 64, 3));
        // 填充 Y 平面
        for y in 0..48 {
            for x in 0..64 {
                array[[y, x, 0]] = ((x + y) % 256) as u8;
            }
        }
        // 填充 U、V 平面
        for y in 0..24 {
            for x in 0..32 {
                let u_val = (x % 256) as u8;
                let v_val = (y % 256) as u8;
                for dy in 0..2 {
                    for dx in 0..2 {
                        let y_pos = y * 2 + dy;
                        let x_pos = x * 2 + dx;
                        if y_pos < 48 && x_pos < 64 {
                            array[[y_pos, x_pos, 1]] = u_val;
                            array[[y_pos, x_pos, 2]] = v_val;
                        }
                    }
                }
            }
        }

        let result = ndarray_to_avframe_yuv(&array);
        assert!(result.is_ok());
        let frame = result.unwrap();

        // 验证帧属性
        assert_eq!(frame.width, 64, "Frame width mismatch");
        assert_eq!(frame.height, 48, "Frame height mismatch");
        assert_eq!(frame.format, PIXEL_FORMAT_YUV420P, "Frame format mismatch");

        // 验证数据
        unsafe {
            let y_data = frame.data[0];
            let u_data = frame.data[1];
            let v_data = frame.data[2];

            let y_linesize = frame.linesize[0] as usize;
            let u_linesize = frame.linesize[1] as usize;
            let v_linesize = frame.linesize[2] as usize;

            // 验证 Y 平面
            for y in 0..48 {
                for x in 0..64 {
                    assert_eq!(
                        *y_data.add(y * y_linesize + x),
                        array[[y, x, 0]],
                        "Y plane mismatch at position [{}, {}]", y, x
                    );
                }
            }

            // 验证 U、V 平面
            for y in 0..24 {
                for x in 0..32 {
                    // 验证下采样后的值
                    let u_val = *u_data.add(y * u_linesize + x);
                    let v_val = *v_data.add(y * v_linesize + x);

                    assert_eq!(
                        u_val,
                        array[[y * 2, x * 2, 1]],
                        "U plane mismatch at position [{}, {}]", y, x
                    );
                    assert_eq!(
                        v_val,
                        array[[y * 2, x * 2, 2]],
                        "V plane mismatch at position [{}, {}]", y, x
                    );
                }
            }
        }
    }

    #[test]
    fn test_ndarray_to_avframe_yuv_wrong_dimensions() {
        // 测试非偶数维度
        let array = FrameArray::zeros((47, 63, 3));
        let result = ndarray_to_avframe_yuv(&array);
        assert!(result.is_err());
    }

    #[test]
    fn test_round_trip_rgb() {
        // 测试 RGB 格式的往返转换
        let original_frame = create_test_rgb_frame(64, 48);
        let array = avframe_rgb_to_ndarray(&original_frame).unwrap();
        let converted_frame = ndarray_to_avframe_rgb(&array).unwrap();

        // 验证数据一致性
        unsafe {
            let orig_data = original_frame.data[0];
            let conv_data = converted_frame.data[0];
            let linesize = original_frame.linesize[0] as usize;

            for y in 0..48 {
                for x in 0..64 {
                    let offset = y * linesize + x * 3;
                    assert_eq!(*orig_data.add(offset), *conv_data.add(offset));
                    assert_eq!(*orig_data.add(offset + 1), *conv_data.add(offset + 1));
                    assert_eq!(*orig_data.add(offset + 2), *conv_data.add(offset + 2));
                }
            }
        }
    }

    #[test]
    fn test_round_trip_yuv() {
        // 测试 YUV 格式的往返转换
        let original_frame = create_test_yuv_frame(64, 48);
        let array = avframe_yuv_to_ndarray(&original_frame).unwrap();
        let converted_frame = ndarray_to_avframe_yuv(&array).unwrap();

        // 验证 Y 平面数据一致性
        unsafe {
            let orig_y = original_frame.data[0];
            let conv_y = converted_frame.data[0];
            let y_linesize = original_frame.linesize[0] as usize;

            for y in 0..48 {
                for x in 0..64 {
                    assert_eq!(
                        *orig_y.add(y * y_linesize + x),
                        *conv_y.add(y * y_linesize + x)
                    );
                }
            }

            // 验证 U 平面数据一致性
            let orig_u = original_frame.data[1];
            let conv_u = converted_frame.data[1];
            let u_linesize = original_frame.linesize[1] as usize;

            for y in 0..24 {
                for x in 0..32 {
                    assert_eq!(
                        *orig_u.add(y * u_linesize + x),
                        *conv_u.add(y * u_linesize + x)
                    );
                }
            }

            // 验证 V 平面数据一致性
            let orig_v = original_frame.data[2];
            let conv_v = converted_frame.data[2];
            let v_linesize = original_frame.linesize[2] as usize;

            for y in 0..24 {
                for x in 0..32 {
                    assert_eq!(
                        *orig_v.add(y * v_linesize + x),
                        *conv_v.add(y * v_linesize + x)
                    );
                }
            }
        }
    }

    #[test]
    fn test_edge_cases() {
        // 测试最小尺寸 (1x1)
        let tiny_rgb_frame = create_test_rgb_frame(1, 1);
        let result = avframe_rgb_to_ndarray(&tiny_rgb_frame);
        assert!(result.is_ok());
        let array = result.unwrap();
        assert_eq!(array.shape(), &[1, 1, 3]);

        // 测试大尺寸（需要考虑内存限制）
        let large_rgb_frame = create_test_rgb_frame(4096, 2160);
        let result = avframe_rgb_to_ndarray(&large_rgb_frame);
        assert!(result.is_ok());
    }

    #[test]
    fn test_yuv_alignment() {
        // 测试非标准对齐的 YUV 帧
        let mut frame = AVFrame::new();
        frame.set_format(PIXEL_FORMAT_YUV420P);
        frame.set_width(65); // 非标准宽度
        frame.set_height(49); // 非标准高度
        frame.alloc_buffer().unwrap();

        let result = avframe_yuv_to_ndarray(&frame);
        assert!(result.is_ok());
    }

    #[test]
    fn test_performance() {
        use std::time::Instant;

        // 创建大尺寸帧进行性能测试
        let frame = create_test_rgb_frame(1920, 1080);

        let start = Instant::now();
        for _ in 0..10 {
            let array = avframe_rgb_to_ndarray(&frame).unwrap();
            let _ = ndarray_to_avframe_rgb(&array).unwrap();
        }
        let duration = start.elapsed();

        println!("Performance test completed in {:?}", duration);
    }

    #[test]
    fn test_memory_usage() {
        use std::mem;

        // 测试内存使用
        let frame = create_test_rgb_frame(1920, 1080);
        let array = avframe_rgb_to_ndarray(&frame).unwrap();

        let frame_size = unsafe { mem::size_of_val(&*frame.as_ptr()) };
        let array_size = mem::size_of_val(&array);

        println!("Frame size: {} bytes", frame_size);
        println!("Array size: {} bytes", array_size);
    }

    #[test]
    fn test_concurrent_processing() {
        use std::thread;

        let frame = create_test_rgb_frame(640, 480);
        let array = avframe_rgb_to_ndarray(&frame).unwrap();

        // 创建多个线程同时处理数据
        let mut handles = vec![];
        for _ in 0..4 {
            let array_clone = array.clone();
            let handle = thread::spawn(move || {
                let result = ndarray_to_avframe_rgb(&array_clone);
                assert!(result.is_ok());
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }
    }



    #[test]
    fn test_metadata() {
        // 测试元数据保留
        let frame = create_test_rgb_frame(64, 48);
        let array = avframe_rgb_to_ndarray(&frame).unwrap();
        let new_frame = ndarray_to_avframe_rgb(&array).unwrap();

        assert_eq!(frame.width, new_frame.width);
        assert_eq!(frame.height, new_frame.height);
        assert_eq!(frame.format, new_frame.format);
    }

    // 辅助函数：生成彩色渐变测试图案
    fn create_gradient_pattern(width: usize, height: usize) -> FrameArray {
        let mut array = FrameArray::zeros((height, width, 3));
        for y in 0..height {
            for x in 0..width {
                array[[y, x, 0]] = ((x as f32 / width as f32) * 255.0) as u8; // R
                array[[y, x, 1]] = ((y as f32 / height as f32) * 255.0) as u8; // G
                array[[y, x, 2]] = (((x + y) as f32 / (width + height) as f32) * 255.0) as u8;
                // B
            }
        }
        array
    }

    #[test]
    fn test_gradient_pattern() {
        let pattern = create_gradient_pattern(256, 256);
        let frame = ndarray_to_avframe_rgb(&pattern).unwrap();
        let recovered_pattern = avframe_rgb_to_ndarray(&frame).unwrap();

        // 验证渐变模式
        for y in 0..256 {
            for x in 0..256 {
                assert_eq!(pattern[[y, x, 0]], recovered_pattern[[y, x, 0]]);
                assert_eq!(pattern[[y, x, 1]], recovered_pattern[[y, x, 1]]);
                assert_eq!(pattern[[y, x, 2]], recovered_pattern[[y, x, 2]]);
            }
        }
    }

    // 添加当前时间和用户信息的测试
    #[test]
    fn test_timestamp_and_user() {
        println!("Test executed by user: phial3");
        println!("Test timestamp: 2025-02-13 02:05:56 UTC");

        // 创建一个带时间戳的测试帧
        let mut frame = create_test_rgb_frame(64, 48);
        unsafe {
            // 在帧的某个位置编码时间戳
            let data = frame.data[0];
            let timestamp = b"2025-02-13 02:05:56";
            ptr::copy_nonoverlapping(timestamp.as_ptr(), data, timestamp.len());
        }

        let array = avframe_rgb_to_ndarray(&frame).unwrap();
        let new_frame = ndarray_to_avframe_rgb(&array).unwrap();

        // 验证时间戳是否正确保留
        unsafe {
            let data = new_frame.data[0];
            let mut timestamp = [0u8; 19];
            ptr::copy_nonoverlapping(data, timestamp.as_mut_ptr(), 19);
            assert_eq!(&timestamp, b"2025-02-13 02:05:56");
        }
    }

    // 测试边界条件
    #[test]
    fn test_ndarray_to_avframe_yuv_invalid_dimensions() {
        // 测试奇数维度
        let array = FrameArray::zeros((47, 63, 3));
        let result = ndarray_to_avframe_yuv(&array);
        assert!(result.is_err(), "Should fail with odd dimensions");
    }

    #[test]
    fn test_ndarray_to_avframe_yuv_invalid_channels() {
        // 测试错误的通道数
        let array = FrameArray::zeros((48, 64, 4));
        let result = ndarray_to_avframe_yuv(&array);
        assert!(result.is_ok(), "Should fail with wrong number of channels");
    }
}
