use crate::PixelFormat;

use rsmpeg::avcodec::AVCodecContext;
use rsmpeg::avutil::{AVFrame, AVSamples};
use rsmpeg::ffi;
use rsmpeg::swresample::SwrContext;
use rsmpeg::swscale::SwsContext;

use anyhow::{Context, Error, Result};

///////////////////////////////////////////////////////////////////////////////////////////////////
////////////////////////////// Video Scaler SwsContext ////////////////////////////////////////////
///////////////////////////////////////////////////////////////////////////////////////////////////

fn setup_scaler(
    src_width: i32,
    src_height: i32,
    src_pix_fmt: ffi::AVPixelFormat,
    dst_width: i32,
    dst_height: i32,
    dst_pix_fmt: ffi::AVPixelFormat,
) -> Result<SwsContext> {
    /*
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

    /*
     * Return an error on underspecified conversions. Without this flag,
     * unspecified fields are defaulted to sensible values.
     */
    // SWS_STRICT        = 1 << 11,

    /*
     * Emit verbose log of scaling parameters.
     */
    // SWS_PRINT_INFO    = 1 << 12,

    /*
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

    /*
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

    /*
     * Force bit-exact output. This will prevent the use of platform-specific
     * optimizations that may lead to slight difference in rounding, in favor
     * of always maintaining exact bit output compatibility with the reference
     * C code.
     *
     * Note: It is recommended to set both of these flags simultaneously.
     */
    // SWS_ACCURATE_RND   = 1 << 18,
    // SWS_BITEXACT       = 1 << 19,

    // 考虑性能和质量平衡
    let flags =
        ffi::SWS_BICUBIC | ffi::SWS_FULL_CHR_H_INT | ffi::SWS_ACCURATE_RND | ffi::SWS_BITEXACT;

    // 创建转换上下文
    let sws_ctx = SwsContext::get_context(
        src_width,
        src_height,
        src_pix_fmt,
        dst_width,
        dst_height,
        dst_pix_fmt,
        flags,
        None,
        None,
        None,
    )
    .context("Failed to create a swscale context.")?;

    Ok(sws_ctx)
}

/// # Safety
///
/// ffi::sws_scale_frame
pub fn scale(
    src_frame: &AVFrame,
    dst_width: i32,
    dst_height: i32,
    dst_pix_fmt: PixelFormat,
) -> Result<AVFrame> {
    if !src_frame.hw_frames_ctx.is_null() {
        anyhow::bail!("Hardware frames are not supported in this software scalar");
    }

    let mut dst_frame = AVFrame::new();
    // copy props
    let ret = unsafe { ffi::av_frame_copy_props(dst_frame.as_mut_ptr(), src_frame.as_ptr()) };
    if ret < 0 {
        return Err(Error::msg(format!("Failed to copy props, ret: {}", ret)));
    }

    dst_frame
        .alloc_buffer()
        .context("Failed to allocate destination frame buffer")?;

    let mut sws_ctx = setup_scaler(
        src_frame.width,
        src_frame.height,
        src_frame.format,
        dst_width,
        dst_height,
        dst_pix_fmt.into(),
    )
    .context("Failed to create swscale context.")?;

    let ret = unsafe {
        ffi::sws_scale_frame(
            sws_ctx.as_mut_ptr(),
            dst_frame.as_mut_ptr(),
            src_frame.as_ptr(),
        )
    };
    if ret < 0 {
        return Err(Error::msg(format!("Failed to scale frame, ret: {}", ret)));
    }

    Ok(dst_frame)
}

/// 将 AVFrame YUV420P 转换为 RGB24 格式
pub fn scale_frame(
    src_frame: &AVFrame,
    dst_width: i32,
    dst_height: i32,
    dst_pix_fmt: PixelFormat,
) -> Result<AVFrame> {
    if !src_frame.hw_frames_ctx.is_null() {
        anyhow::bail!("Hardware frames are not supported in this software scalar");
    }

    let mut dst_frame = AVFrame::new();
    dst_frame.set_width(dst_width);
    dst_frame.set_height(dst_height);
    dst_frame.set_format(dst_pix_fmt.into());
    // copy props
    let ret = unsafe { ffi::av_frame_copy_props(dst_frame.as_mut_ptr(), src_frame.as_ptr()) };
    if ret < 0 {
        return Err(Error::msg(format!("Failed to copy props, ret: {}", ret)));
    }

    dst_frame
        .alloc_buffer()
        .context("Failed to allocate destination frame buffer")?;

    let mut sws_ctx = setup_scaler(
        src_frame.width,
        src_frame.height,
        src_frame.format,
        dst_width,
        dst_height,
        dst_pix_fmt.into(),
    )
    .context("Failed to create swscale context.")?;

    sws_ctx
        .scale_frame(src_frame, 0, src_frame.height, &mut dst_frame)
        .context(format!(
            "Failed to scale frame from [fmt:{}, size:{}x{}] to [fmt:{:?}, size:{}x{}]",
            src_frame.format, src_frame.width, src_frame.height, dst_pix_fmt, dst_width, dst_height
        ))?;

    Ok(dst_frame)
}

///////////////////////////////////////////////////////////////////////////////////////////////////
////////////////////////////// Audio Resampler SwrContext /////////////////////////////////////////
///////////////////////////////////////////////////////////////////////////////////////////////////

fn setup_resampler(
    out_ch_layout: &ffi::AVChannelLayout,
    out_sample_fmt: ffi::AVSampleFormat,
    out_sample_rate: i32,
    in_ch_layout: &ffi::AVChannelLayout,
    in_sample_fmt: ffi::AVSampleFormat,
    in_sample_rate: i32,
) -> Result<SwrContext> {
    let mut resample_context = SwrContext::new(
        out_ch_layout,
        out_sample_fmt,
        out_sample_rate,
        in_ch_layout,
        in_sample_fmt,
        in_sample_rate,
    )
    .context("Could not allocate resample context")?;

    resample_context
        .init()
        .context("Could not open resample context")?;

    Ok(resample_context)
}

/// Audio resampling frame
pub fn convert(
    decode_context: &AVCodecContext,
    encode_context: &AVCodecContext,
    src_frame: &AVFrame,
) -> Result<AVSamples> {
    if !src_frame.hw_frames_ctx.is_null() {
        anyhow::bail!("Hardware frames are not supported in this software re-sampler");
    }

    let sample_fmt = rsmpeg::avutil::get_sample_fmt_name(src_frame.format);
    if src_frame.sample_rate < 0 || sample_fmt.is_none() {
        return Err(Error::msg("Invalid input frame."));
    }

    let mut resample_context = setup_resampler(
        &encode_context.ch_layout,
        encode_context.sample_fmt,
        encode_context.sample_rate,
        &decode_context.ch_layout,
        decode_context.sample_fmt,
        decode_context.sample_rate,
    )
    .context("Failed to create resample context.")?;

    let mut output_samples = AVSamples::new(
        encode_context.ch_layout.nb_channels,
        src_frame.nb_samples,
        encode_context.sample_fmt,
        0,
    )
    .context("Create samples buffer failed.")?;

    let ret = unsafe {
        resample_context
            .convert(
                output_samples.audio_data.as_mut_ptr(),
                output_samples.nb_samples,
                src_frame.extended_data as *const _,
                src_frame.nb_samples,
            )
            .context("Could not convert input samples")?
    };
    if ret < 0 {
        return Err(Error::msg(format!(
            "Failed to convert input samples, ret: {}",
            ret
        )));
    }

    Ok(output_samples)
}

/// Audio resampling frame
pub fn convert_frame(
    decode_context: &AVCodecContext,
    encode_context: &AVCodecContext,
    src_frame: &AVFrame,
) -> Result<AVFrame> {
    if !src_frame.hw_frames_ctx.is_null() {
        anyhow::bail!("Hardware frames are not supported in this software re-sampler");
    }

    let sample_fmt = rsmpeg::avutil::get_sample_fmt_name(src_frame.format);
    if src_frame.sample_rate < 0 || sample_fmt.is_none() {
        return Err(Error::msg("Invalid input frame."));
    }

    let resample_context = setup_resampler(
        &encode_context.ch_layout,
        encode_context.sample_fmt,
        encode_context.sample_rate,
        &decode_context.ch_layout,
        decode_context.sample_fmt,
        decode_context.sample_rate,
    )
    .context("Failed to create resample context.")?;

    let mut dst_frame = AVFrame::new();
    let ret = unsafe { ffi::av_frame_copy_props(dst_frame.as_mut_ptr(), src_frame.as_ptr()) };
    if ret < 0 {
        return Err(Error::msg(format!("Failed to copy props, ret: {}", ret)));
    }

    dst_frame.set_format(encode_context.sample_fmt);
    dst_frame.set_ch_layout(encode_context.ch_layout);
    dst_frame.set_sample_rate(encode_context.sample_rate);
    dst_frame
        .alloc_buffer()
        .context("Failed to allocate destination frame buffer")?;

    // 转换输入 AVFrame 中的样本并将其写入输出 AVFrame。
    // 输入和输出 AVFrame 必须设置通道布局、采样率和格式。
    // 如果输出 AVFrame 没有分配数据指针，则将在调用 av_frame_get_buffer() 分配帧时设置 nb_samples 字段。
    // 输出的 AVFrame 可以是 NULL，或者分配的样本少于所需的数量。在这种情况下，未写入输出的剩余样本将被添加到内部 FIFO 缓冲区，在下次调用此函数或 swr_convert() 时返回。
    // 如果转换采样率，内部重采样延迟缓冲区中可能会有剩余数据。要以输出方式获取这些数据，请调用此函数或 swr_convert()，并输入 NULL。
    resample_context
        .convert_frame(Some(src_frame), &mut dst_frame)
        .context("Failed to convert frame.")?;

    Ok(dst_frame)
}
