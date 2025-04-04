use crate::PixelFormat;

use rsmpeg::avutil::{self, AVFrame, AVSamples};
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
    in_ch_layout: ffi::AVChannelLayout,
    in_sample_fmt: ffi::AVSampleFormat,
    in_sample_rate: i32,
    out_ch_layout: ffi::AVChannelLayout,
    out_sample_fmt: ffi::AVSampleFormat,
    out_sample_rate: i32,
) -> Result<SwrContext> {
    let mut resample_context = SwrContext::new(
        &out_ch_layout,
        out_sample_fmt,
        out_sample_rate,
        &in_ch_layout,
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
    src_frame: &AVFrame,
    out_ch_layout: ffi::AVChannelLayout,
    out_sample_fmt: ffi::AVSampleFormat,
    out_sample_rate: i32,
) -> Result<AVSamples> {
    if !src_frame.hw_frames_ctx.is_null() {
        anyhow::bail!("Hardware frames are not supported in this software re-sampler");
    }

    let sample_fmt = avutil::get_sample_fmt_name(src_frame.format);
    if src_frame.sample_rate < 0 || sample_fmt.is_none() {
        return Err(Error::msg("Invalid input frame."));
    }

    let mut resample_context = setup_resampler(
        src_frame.ch_layout,
        src_frame.format,
        src_frame.sample_rate,
        out_ch_layout,
        out_sample_fmt,
        out_sample_rate,
    )
    .context("Failed to create resample context.")?;

    let mut output_samples = AVSamples::new(
        out_ch_layout.nb_channels,
        src_frame.nb_samples,
        out_sample_fmt,
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
///
/// # Arguments
///
///
pub fn convert_frame(
    src_frame: &AVFrame,
    out_ch_layout: ffi::AVChannelLayout,
    out_sample_fmt: ffi::AVSampleFormat,
    out_sample_rate: i32,
) -> Result<AVFrame> {
    if !src_frame.hw_frames_ctx.is_null() {
        anyhow::bail!("Hardware frames are not supported in this software re-sampler");
    }

    let sample_fmt = avutil::get_sample_fmt_name(src_frame.format);
    if src_frame.sample_rate < 0 || sample_fmt.is_none() {
        return Err(Error::msg("Invalid input frame."));
    }

    let resample_context = setup_resampler(
        src_frame.ch_layout,
        src_frame.format,
        src_frame.sample_rate,
        out_ch_layout,
        out_sample_fmt,
        out_sample_rate,
    )
    .context("Failed to create resample context.")?;

    let mut dst_frame = AVFrame::new();
    let ret = unsafe { ffi::av_frame_copy_props(dst_frame.as_mut_ptr(), src_frame.as_ptr()) };
    if ret < 0 {
        return Err(Error::msg(format!("Failed to copy props, ret: {}", ret)));
    }

    dst_frame.set_format(out_sample_fmt);
    dst_frame.set_ch_layout(out_ch_layout);
    dst_frame.set_nb_samples(src_frame.nb_samples);
    dst_frame.set_sample_rate(out_sample_rate);
    dst_frame.set_time_base(avutil::ra(1, out_sample_rate));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SampleFormat;
    use anyhow::{Context, Result};
    use rsmpeg::avutil::AVChannelLayout;
    use rsmpeg::ffi;

    /// 音频格式特征描述
    #[warn(dead_code)]
    struct AudioFormatDesc {
        format: ffi::AVSampleFormat,
        name: &'static str,
        bytes_per_sample: usize,
    }

    impl std::fmt::Debug for AudioFormatDesc {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.name)
        }
    }

    /// 定义所有支持的音频格式
    const AUDIO_FORMATS: &[AudioFormatDesc] = &[
        AudioFormatDesc {
            format: ffi::AV_SAMPLE_FMT_U8,
            name: "U8",
            bytes_per_sample: 1,
        },
        AudioFormatDesc {
            format: ffi::AV_SAMPLE_FMT_U8P,
            name: "U8P",
            bytes_per_sample: 1,
        },
        AudioFormatDesc {
            format: ffi::AV_SAMPLE_FMT_S16,
            name: "S16",
            bytes_per_sample: 2,
        },
        AudioFormatDesc {
            format: ffi::AV_SAMPLE_FMT_S16P,
            name: "S16P",
            bytes_per_sample: 2,
        },
        AudioFormatDesc {
            format: ffi::AV_SAMPLE_FMT_S32,
            name: "S32",
            bytes_per_sample: 4,
        },
        AudioFormatDesc {
            format: ffi::AV_SAMPLE_FMT_S32P,
            name: "S32P",
            bytes_per_sample: 4,
        },
        AudioFormatDesc {
            format: ffi::AV_SAMPLE_FMT_FLT,
            name: "FLT",
            bytes_per_sample: 4,
        },
        AudioFormatDesc {
            format: ffi::AV_SAMPLE_FMT_FLTP,
            name: "FLTP",
            bytes_per_sample: 4,
        },
        AudioFormatDesc {
            format: ffi::AV_SAMPLE_FMT_DBL,
            name: "DBL",
            bytes_per_sample: 8,
        },
        AudioFormatDesc {
            format: ffi::AV_SAMPLE_FMT_DBLP,
            name: "DBLP",
            bytes_per_sample: 8,
        },
        AudioFormatDesc {
            format: ffi::AV_SAMPLE_FMT_S64,
            name: "S64",
            bytes_per_sample: 8,
        },
        AudioFormatDesc {
            format: ffi::AV_SAMPLE_FMT_S64P,
            name: "S64P",
            bytes_per_sample: 8,
        },
    ];

    /// 安全地填充测试数据
    unsafe fn fill_test_data(frame: &mut AVFrame, format_desc: &AudioFormatDesc) -> Result<()> {
        let nb_samples = frame.nb_samples as usize;
        let nb_channels = frame.ch_layout.nb_channels as usize;
        let is_planar = SampleFormat::from(format_desc.format).is_planar();

        macro_rules! fill_samples {
            ($type:ty, $max_val:expr) => {
                if is_planar {
                    for ch in 0..nb_channels {
                        let data = std::slice::from_raw_parts_mut(
                            frame.data[ch] as *mut $type,
                            nb_samples,
                        );
                        for (i, sample) in data.iter_mut().enumerate() {
                            *sample = ((i * nb_channels + ch) as f64
                                / (nb_samples * nb_channels) as f64
                                * $max_val as f64) as $type;
                        }
                    }
                } else {
                    let data = std::slice::from_raw_parts_mut(
                        frame.data[0] as *mut $type,
                        nb_samples * nb_channels,
                    );
                    for i in 0..(nb_samples * nb_channels) {
                        data[i] = (i as f64 / (nb_samples * nb_channels) as f64 * $max_val as f64)
                            as $type;
                    }
                }
            };
        }

        match format_desc.format {
            ffi::AV_SAMPLE_FMT_U8 | ffi::AV_SAMPLE_FMT_U8P => {
                fill_samples!(u8, u8::MAX)
            }
            ffi::AV_SAMPLE_FMT_S16 | ffi::AV_SAMPLE_FMT_S16P => {
                fill_samples!(i16, i16::MAX)
            }
            ffi::AV_SAMPLE_FMT_S32 | ffi::AV_SAMPLE_FMT_S32P => {
                fill_samples!(i32, i32::MAX)
            }
            ffi::AV_SAMPLE_FMT_FLT | ffi::AV_SAMPLE_FMT_FLTP => {
                fill_samples!(f32, 1.0)
            }
            ffi::AV_SAMPLE_FMT_DBL | ffi::AV_SAMPLE_FMT_DBLP => {
                fill_samples!(f64, 1.0)
            }
            ffi::AV_SAMPLE_FMT_S64 | ffi::AV_SAMPLE_FMT_S64P => {
                fill_samples!(i64, i64::MAX)
            }
            _ => return Err(Error::msg("Unsupported sample format")),
        }
        Ok(())
    }

    fn create_test_frame(
        format_desc: &AudioFormatDesc,
        sample_rate: i32,
        nb_channels: i32,
        nb_samples: i32,
    ) -> Result<AVFrame> {
        let mut frame = AVFrame::new();

        frame.set_format(format_desc.format);
        frame.set_ch_layout(AVChannelLayout::from_nb_channels(nb_channels).into_inner());
        frame.set_nb_samples(nb_samples);
        frame.set_sample_rate(sample_rate);
        frame.set_time_base(avutil::ra(1, sample_rate));

        frame
            .alloc_buffer()
            .context("Failed to allocate frame buffer")?;

        unsafe {
            fill_test_data(&mut frame, format_desc).context("Failed to fill test data")?;
        }

        Ok(frame)
    }

    #[test]
    #[ignore = "linux ffmpeg/7.1: AVFrame buffer allocating with incorrect parameters."]
    fn test_format_conversion() -> Result<()> {
        let sample_rate = 44100;
        let nb_samples = 1024;
        let nb_channels = 2;

        // 测试所有格式组合
        for in_fmt in AUDIO_FORMATS {
            println!("\nTesting input format: {:?}", in_fmt);

            let src_frame = create_test_frame(in_fmt, sample_rate, nb_channels, nb_samples)
                .with_context(|| format!("Failed to create source frame for {:?}", in_fmt))?;

            for out_fmt in AUDIO_FORMATS {
                println!("  Converting to format: {:?}", out_fmt);

                let ch_layout = AVChannelLayout::from_nb_channels(nb_channels).into_inner();

                let result = convert_frame(&src_frame, ch_layout, out_fmt.format, sample_rate)
                    .with_context(|| {
                        format!("Failed to convert from {:?} to {:?}", in_fmt, out_fmt)
                    })?;

                // 验证转换结果
                assert_eq!(
                    result.format, out_fmt.format,
                    "Format mismatch converting from {:?} to {:?}",
                    in_fmt, out_fmt
                );
                assert_eq!(
                    result.nb_samples, src_frame.nb_samples,
                    "Sample count mismatch converting from {:?} to {:?}",
                    in_fmt, out_fmt
                );
                assert_eq!(
                    result.ch_layout.nb_channels, src_frame.ch_layout.nb_channels,
                    "Channel count mismatch converting from {:?} to {:?}",
                    in_fmt, out_fmt
                );
            }
        }

        Ok(())
    }

    #[test]
    #[ignore = "linux ffmpeg/7.1: AVFrame buffer allocating with incorrect parameters."]
    fn test_format_conversion_with_different_rates() -> Result<()> {
        let sample_rates = &[44100, 48000, 96000];
        let nb_samples = 1024;
        let nb_channels = 2;

        for in_fmt in AUDIO_FORMATS {
            for &in_rate in sample_rates {
                let src_frame = create_test_frame(in_fmt, in_rate, nb_channels, nb_samples)?;

                for out_fmt in AUDIO_FORMATS {
                    for &out_rate in sample_rates {
                        if in_rate == out_rate {
                            continue;
                        }

                        println!(
                            "Converting {:?} @{}Hz to {:?} @{}Hz",
                            in_fmt, in_rate, out_fmt, out_rate
                        );

                        let ch_layout = AVChannelLayout::from_nb_channels(nb_channels).into_inner();

                        let result =
                            convert_frame(&src_frame, ch_layout, out_fmt.format, out_rate)?;

                        // 验证转换结果
                        assert_eq!(result.format, out_fmt.format);
                        assert_eq!(result.sample_rate, out_rate);
                        assert_eq!(result.ch_layout.nb_channels, nb_channels);
                    }
                }
            }
        }

        Ok(())
    }

    #[test]
    #[ignore = "linux ffmpeg/7.1: AVFrame buffer allocating with incorrect parameters."]
    fn test_channel_conversion() -> Result<()> {
        let nb_samples = 1024;
        let channel_layouts = &[1, 2];
        let sample_rates = &[44100, 48000, 96000];

        for in_fmt in AUDIO_FORMATS {
            for &in_rate in sample_rates {
                for &in_channels in channel_layouts {
                    println!(
                        "\nSource: format={:?}, rate={}Hz, channels={}",
                        in_fmt, in_rate, in_channels
                    );

                    let src_frame = create_test_frame(&in_fmt, in_rate, in_channels, nb_samples)?;

                    assert_eq!(src_frame.ch_layout.nb_channels, in_channels);

                    for out_fmt in AUDIO_FORMATS {
                        for &out_rate in sample_rates {
                            for &out_channels in channel_layouts {
                                // 跳过相同的配置
                                if in_fmt.format == out_fmt.format
                                    && in_rate == out_rate
                                    && in_channels == out_channels
                                {
                                    continue;
                                }

                                println!(
                                    "Converting to: format={:?}, rate={}Hz, channels={}",
                                    out_fmt, out_rate, out_channels
                                );

                                let result = convert_frame(
                                    &src_frame,
                                    AVChannelLayout::from_nb_channels(out_channels).into_inner(),
                                    out_fmt.format,
                                    out_rate,
                                )?;

                                // 验证基本参数
                                assert_eq!(result.format, out_fmt.format);
                                assert_eq!(result.sample_rate, out_rate);
                                assert_eq!(result.ch_layout.nb_channels, out_channels);

                                // 验证数据有效性
                                unsafe {
                                    if SampleFormat::from(out_fmt.format).is_planar() {
                                        for ch in 0..out_channels as usize {
                                            assert!(
                                                !result.data[ch].is_null(),
                                                "Channel {} data pointer is null",
                                                ch
                                            );

                                            let data = std::slice::from_raw_parts(
                                                result.data[ch],
                                                result.nb_samples as usize
                                                    * out_fmt.bytes_per_sample,
                                            );

                                            assert!(
                                                data.iter().any(|&x| x != 0),
                                                "Channel {} contains all zeros (total size: {})",
                                                ch,
                                                data.len()
                                            );
                                        }
                                    } else {
                                        assert!(!result.data[0].is_null(), "Data pointer is null");
                                        let data = std::slice::from_raw_parts(
                                            result.data[0],
                                            result.nb_samples as usize
                                                * out_channels as usize
                                                * out_fmt.bytes_per_sample,
                                        );

                                        assert!(
                                            data.iter().any(|&x| x != 0),
                                            "Output buffer contains all zeros (total size: {})",
                                            data.len()
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
