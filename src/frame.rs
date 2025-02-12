use crate::error::MediaError;
use ndarray::Array3;
use rsmpeg::avutil::AVFrame;
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi;
use rsmpeg::swscale::SwsContext;

/// A frame array is the `ndarray` version of `AVFrame`. It is 3-dimensional array with dims `(H, W,
/// C)` and type byte.
pub type FrameArray = Array3<u8>;

/// Converts an `ndarray` to an RGB24 video `AVFrame` for ffmpeg.
///
/// # Arguments
///
/// * `frame_array` - Video frame to convert. The frame format must be `(H, W, C)`.
///
/// # Return value
///
/// 将 ndarray::Array3<u8> (HWC 格式) 转换为 ffmpeg AVFrame
/// H: Height (高度)
/// W: Width (宽度)
/// C: Channels (通道数，期望为3，表示RGB)
pub fn convert_ndarray_to_frame_rgb24(frame_array: &FrameArray) -> Result<AVFrame, MediaError> {
    assert!(frame_array.is_standard_layout());

    unsafe {
        let (frame_height, frame_width, _) = frame_array.dim();

        // Temporary frame structure to place correctly formatted data and linesize stuff in, which
        // we'll copy later.
        let mut frame_tmp = AVFrame::new();
        let frame_tmp_ptr = frame_tmp.as_mut_ptr();

        // This does not copy the data,
        // but it sets the `frame_tmp` data and linesize pointers correctly.
        let bytes_copied = ffi::av_image_fill_arrays(
            (*frame_tmp_ptr).data.as_ptr() as *mut *mut u8,
            (*frame_tmp_ptr).linesize.as_ptr() as *mut i32,
            frame_array.as_ptr(),
            ffi::AV_PIX_FMT_RGB24,
            frame_width as i32,
            frame_height as i32,
            1,
        );

        if bytes_copied != frame_array.len() as i32 {
            return Err(MediaError::BackendError(RsmpegError::from(bytes_copied)));
        }

        let mut frame = AVFrame::new();
        frame.set_format(ffi::AV_PIX_FMT_RGB24);
        frame.set_width(frame_width as i32);
        frame.set_height(frame_height as i32);
        let frame_ptr = frame.as_mut_ptr();

        // Do the actual copying.
        ffi::av_image_copy(
            (*frame_ptr).data.as_ptr() as *mut *mut u8,
            (*frame_ptr).linesize.as_ptr() as *mut i32,
            (*frame_tmp_ptr).data.as_ptr() as *mut *const u8,
            (*frame_tmp_ptr).linesize.as_ptr(),
            ffi::AV_PIX_FMT_RGB24,
            frame_width as i32,
            frame_height as i32,
        );

        Ok(frame)
    }
}

/// Converts an RGB24 video `AVFrame` produced by ffmpeg to an `ndarray`.
///
/// # Arguments
///
/// * `frame` - Video frame to convert.
///
/// # Return value
///
/// A three-dimensional `ndarray` with dimensions `(H, W, C)` and type byte.
pub fn convert_frame_to_ndarray_rgb24(frame: &AVFrame) -> Result<FrameArray, MediaError> {
    unsafe {
        let frame_width: i32 = frame.width;
        let frame_height: i32 = frame.height;

        // 创建一个新的 RGB24 格式的帧（如果需要转换）
        let rgb_frame = if frame.format != ffi::AV_PIX_FMT_RGB24 {
            convert_avframe(frame, frame_width, frame_height, ffi::AV_PIX_FMT_RGB24).unwrap()
        } else {
            frame.clone()
        };

        let mut frame_array =
            FrameArray::default((frame_height as usize, frame_width as usize, 3_usize));

        // 复制图像数据到缓冲区
        let bytes_copied = ffi::av_image_copy_to_buffer(
            frame_array.as_mut_ptr() as *mut u8,
            frame_array.len() as i32,
            rgb_frame.as_ptr() as *const *const u8,
            rgb_frame.linesize.as_ptr(),
            ffi::AV_PIX_FMT_RGB24,
            frame_width,
            frame_height,
            1,
        );

        if bytes_copied == frame_array.len() as i32 {
            Ok(frame_array)
        } else {
            Err(MediaError::BackendError(RsmpegError::from(bytes_copied)))
        }
    }
}

/// 将 AVFrame YUV420P 转换为 RGB24 格式
pub fn convert_avframe(
    src_frame: &AVFrame,
    width: i32,
    height: i32,
    dst_pix_fmt: ffi::AVPixelFormat,
) -> Result<AVFrame, MediaError> {
    let src_pix_fmt = src_frame.format;

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

    let flags = ffi::SWS_BICUBIC |
        ffi::SWS_FULL_CHR_H_INT |
        ffi::SWS_FULL_CHR_H_INP |
        ffi::SWS_ACCURATE_RND |
        ffi::SWS_BITEXACT |
        ffi::SWS_PRINT_INFO;

    // 创建转换上下文，将帧转换为 RGB 格式
    let mut sws_ctx = SwsContext::get_context(
        src_frame.width,
        src_frame.height,
        src_pix_fmt,
        width,
        height,
        dst_pix_fmt,
        flags,
        None,
        None,
        None,
    )
    .expect("Failed to create a swscale context.");

    // 创建目标缓冲区
    let mut dist_frame = AVFrame::new();
    dist_frame.set_width(src_frame.width);
    dist_frame.set_height(src_frame.height);
    dist_frame.set_format(dst_pix_fmt);
    dist_frame.set_pts(src_frame.pts);
    dist_frame.set_time_base(src_frame.time_base);
    dist_frame.set_pict_type(src_frame.pict_type);
    dist_frame.alloc_buffer()?;

    sws_ctx.scale_frame(src_frame, 0, height, &mut dist_frame)?;

    Ok(dist_frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array3_to_avframe() -> Result<(), Box<dyn std::error::Error>> {
        // 创建一个小的测试数组
        let array = Array3::<u8>::zeros((32, 32, 3));

        // 转换为 AVFrame
        let frame = convert_ndarray_to_frame_rgb24(&array)?;

        // 验证转换结果
        assert_eq!(frame.width, 32);
        assert_eq!(frame.height, 32);
        assert_eq!(frame.format, ffi::AV_PIX_FMT_RGB24);

        Ok(())
    }

    #[test]
    fn test_invalid_channels() {
        // 创建一个通道数错误的数组
        let array = Array3::<u8>::zeros((32, 32, 4));

        // 应该返回错误
        assert!(convert_ndarray_to_frame_rgb24(&array).is_err());
    }
}
