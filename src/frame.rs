use anyhow::{Context, Result};
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
/// An ffmpeg-native `AvFrame`.
pub fn convert_ndarray_to_frame_rgb24(frame_array: &FrameArray) -> Result<AVFrame, RsmpegError> {
    unsafe {
        assert!(frame_array.is_standard_layout());

        let (frame_height, frame_width, _) = frame_array.dim();

        // Temporary frame structure to place correctly formatted data and linesize stuff in, which
        // we'll copy later.
        let mut frame_tmp = AVFrame::new();
        let frame_tmp_ptr = frame_tmp.as_mut_ptr();

        // This does not copy the data, but it sets the `frame_tmp` data and linesize pointers
        // correctly.
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
            return Err(RsmpegError::from(bytes_copied));
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
pub fn convert_frame_to_ndarray_rgb24(frame: &AVFrame) -> Result<FrameArray, RsmpegError> {
    unsafe {
        let frame_width: i32 = frame.width;
        let frame_height: i32 = frame.height;

        // 创建一个新的 RGB24 格式的帧（如果需要转换）
        let rgb_frame = if frame.format != ffi::AV_PIX_FMT_RGB24 {
            avframe_to_rgb24(frame).unwrap()
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
            Err(RsmpegError::from(bytes_copied))
        }
    }
}

/// 将 AVFrame YUV420P 转换为 RGB24 格式
pub fn avframe_to_rgb24(yuv_frame: &AVFrame) -> Result<AVFrame> {
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
    .context("Failed to create a swscale context.")?;

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
