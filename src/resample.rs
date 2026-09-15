use crate::error::{Context, Result, RsmediaError};
use crate::{SampleFormat, imgutils, time};

use rsmpeg::avutil::{AVChannelLayout, AVFrame, AVSamples};
use rsmpeg::ffi;
use rsmpeg::swresample::SwrContext;

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
    let mut swr_ctx = SwrContext::new(
        &out_ch_layout,
        out_sample_fmt,
        out_sample_rate,
        &in_ch_layout,
        in_sample_fmt,
        in_sample_rate,
    )
    .context("Could not allocate resample context")?;

    swr_ctx.init().context("Could not open resample context")?;

    Ok(swr_ctx)
}

/// 校验重采样输入帧：不支持硬件帧，且采样率/样本数必须有效。
fn check_resampler_input(src_frame: &AVFrame) -> Result<()> {
    if !src_frame.hw_frames_ctx.is_null() {
        return Err(RsmediaError::unsupported(
            "Hardware frames are not supported in this software re-sampler",
        ));
    }

    if src_frame.sample_rate < 1 || src_frame.nb_samples < 1 {
        return Err(RsmediaError::msg("Invalid input frame."));
    }
    Ok(())
}

/// The default channel layout for `layout`, or [`None`] when it is already
/// specified (or names no channels at all).
///
/// Frames decoded from containers that carry no channel mask (a plain WAV, say)
/// report `AV_CHANNEL_ORDER_UNSPEC`. `swr_alloc_set_opts2` replaces such an order
/// with the default layout for the channel count when it builds the context, and
/// `swr_convert` then rejects every frame whose order still says UNSPEC with
/// `AVERROR_INPUT_CHANGED` / `AVERROR_OUTPUT_CHANGED`. Interpreting the
/// unspecified layout as the default one is what FFmpeg itself does; this is the
/// one place that decision is made, for input frames and output layouts alike.
fn default_layout_for(layout: ffi::AVChannelLayout) -> Option<ffi::AVChannelLayout> {
    (layout.order == ffi::AV_CHANNEL_ORDER_UNSPEC && layout.nb_channels > 0)
        .then(|| AVChannelLayout::from_nb_channels(layout.nb_channels).into_inner())
}

/// Runs `convert` with `frame`'s unspecified channel layout filled in.
///
/// The clone only happens in that one case; a frame whose layout is specified is
/// passed through untouched, samples and all.
fn with_normalized_layout<R>(
    frame: &AVFrame,
    convert: impl FnOnce(&AVFrame) -> Result<R>,
) -> Result<R> {
    let normalized;
    let frame = match default_layout_for(frame.ch_layout) {
        Some(layout) => {
            let mut copy = frame.clone();
            copy.set_ch_layout(layout);
            normalized = copy;
            &normalized
        }
        None => frame,
    };
    convert(frame)
}

/// Resamples one frame into a new frame, in one call.
///
/// The context is created fresh for this call, so this is for isolated
/// conversions; a stream should use [`Resampler`] instead, whose delay buffers
/// carry samples across calls (and whose [`flush`](Resampler::flush) drains the
/// tail).
///
/// The output is allocated at the *upper bound* of what the conversion can
/// produce, so `dst.nb_samples` after the call is the real count. Metadata is
/// copied from the source frame (pts included), and the result's time base is
/// `1 / out_sample_rate`.
///
/// # Arguments
///
/// * `src_frame` - Decoded frame to convert; must be a software frame with a
///   valid sample rate and at least one sample.
/// * `out_ch_layout` - Channel layout of the output.
/// * `out_sample_fmt` - Sample format of the output.
/// * `out_sample_rate` - Sample rate of the output.
pub fn convert_frame(
    src_frame: &AVFrame,
    out_ch_layout: ffi::AVChannelLayout,
    out_sample_fmt: ffi::AVSampleFormat,
    out_sample_rate: i32,
) -> Result<AVFrame> {
    check_resampler_input(src_frame)?;

    let normalized = with_normalized_layout(src_frame, |frame| Ok(frame.clone()))?;
    let src_frame = &normalized;

    // 输出布局同样要归一化：上下文按它构建，目标帧每次调用又把它交回 swr，
    // 未指定的 order 会触发 `AVERROR_OUTPUT_CHANGED`。
    let out_ch_layout = default_layout_for(out_ch_layout).unwrap_or(out_ch_layout);

    let mut resampler = Resampler::new(
        src_frame.ch_layout,
        src_frame.format,
        src_frame.sample_rate,
        out_ch_layout,
        out_sample_fmt,
        out_sample_rate,
    )?;

    let mut dst_frame = AVFrame::new();
    // copy props
    imgutils::copy_frame_metadata(src_frame, &mut dst_frame, false)?;
    dst_frame.set_format(out_sample_fmt);
    dst_frame.set_ch_layout(out_ch_layout);
    // 输出帧的缓冲必须按重采样后的输出样本数分配，而不是简单地使用输入样本数。
    // 当输入/输出采样率不同时，swr_convert() 会写入比输入样本数更多的输出样本，
    // 但 FFmpeg 不会自动扩大已分配的输出缓冲（只把放不下的部分存入内部 FIFO），
    // 若这里 nb_samples 设得过小，将导致 swr_convert 越界写。
    // 用 swr_get_out_samples() 得到所需输出样本数的上界来分配缓冲。
    let out_samples = resampler.get_out_samples(src_frame.nb_samples).max(1);
    dst_frame.set_nb_samples(out_samples);
    dst_frame.set_sample_rate(out_sample_rate);
    dst_frame.set_time_base(time::new_rational(1, out_sample_rate));
    dst_frame
        .alloc_buffer()
        .context("Failed to allocate destination frame buffer")?;

    // 转换输入 AVFrame 中的样本并将其写入输出 AVFrame。
    // 输入和输出 AVFrame 必须设置通道布局、采样率和格式。
    // 如果输出 AVFrame 没有分配数据指针，则将在调用 av_frame_get_buffer() 分配帧时设置 nb_samples 字段。
    // 输出的 AVFrame 可以是 NULL，或者分配的样本少于所需的数量。在这种情况下，未写入输出的剩余样本将被添加到内部 FIFO 缓冲区，在下次调用此函数时返回。
    // 如果转换采样率，内部重采样延迟缓冲区中可能会有剩余数据。要以输出方式获取这些数据，请调用此函数，并输入 NULL。
    resampler
        .convert_frame(src_frame, &mut dst_frame)
        .context("Failed to convert frame.")?;

    log::debug!(
        "Swr convert_frame from src:[{}, {:?}, {}] to dst:[{}, {:?}, {}]",
        src_frame.ch_layout.nb_channels,
        SampleFormat::from(src_frame.format),
        src_frame.sample_rate,
        out_ch_layout.nb_channels,
        SampleFormat::from(out_sample_fmt),
        out_sample_rate
    );

    Ok(dst_frame)
}

/// Persistent streaming resampler.
///
/// Unlike [`convert_frame`] (which creates a temporary context on each call),
/// `Resampler` holds a reusable `SwrContext` for continuous streaming input:
/// when resampling (different input/output sample rates), the internal filter
/// delay buffers samples between calls and outputs them with subsequent data;
/// at the end, [`Resampler::flush`] must be called to drain the tail, otherwise
/// the last few milliseconds of samples will be lost.
pub struct Resampler {
    swr: SwrContext,
}

impl Resampler {
    pub fn new(
        in_ch_layout: ffi::AVChannelLayout,
        in_sample_fmt: ffi::AVSampleFormat,
        in_sample_rate: i32,
        out_ch_layout: ffi::AVChannelLayout,
        out_sample_fmt: ffi::AVSampleFormat,
        out_sample_rate: i32,
    ) -> Result<Self> {
        Ok(Self {
            swr: setup_resampler(
                in_ch_layout,
                in_sample_fmt,
                in_sample_rate,
                out_ch_layout,
                out_sample_fmt,
                out_sample_rate,
            )?,
        })
    }

    /// Upper bound estimate of the number of output samples for the given number of input samples.
    pub fn get_out_samples(&self, in_samples: i32) -> i32 {
        self.swr.get_out_samples(in_samples).max(1)
    }

    /// Convert an input frame into an output frame with allocated buffer
    /// (persistent context: samples that cannot be written with `swr_convert` due to
    /// insufficient output capacity remain internally buffered and are returned with subsequent calls).
    ///
    /// The caller must set format/layout/sample_rate/nb_samples on `dst` and
    /// call `alloc_buffer`; after conversion, `dst.nb_samples` is the actual
    /// number of output samples.
    pub fn convert_frame(&mut self, src: &AVFrame, dst: &mut AVFrame) -> Result<()> {
        with_normalized_layout(src, |src| {
            self.swr
                .convert_frame(Some(src), dst)
                .context("Failed to convert frame with streaming resampler")
        })
    }

    /// Raw sample conversion (a direct `swr_convert`), for caller-managed sample
    /// buffers such as [`AVSamples`].
    ///
    /// Returns the buffer and **the number of samples per channel it holds**. The
    /// buffer is allocated at the upper bound from
    /// [`get_out_samples`](Self::get_out_samples) — upsampling produces more
    /// samples than went in — so the count, not the capacity, says how much of it
    /// is valid:
    ///
    /// * `> 0` — that many samples were written for each channel;
    /// * `0` — the input had samples but nothing came out yet: swr holds them in
    ///   its internal delay buffer and emits them with the following input
    ///   (possible whenever the sample rate changes).
    ///
    /// A negative `swr_convert` result becomes an error, never a count.
    pub fn convert(
        &mut self,
        src_frame: &AVFrame,
        out_ch_layout: ffi::AVChannelLayout,
        out_sample_fmt: ffi::AVSampleFormat,
    ) -> Result<(AVSamples, i32)> {
        let out_ch_layout = default_layout_for(out_ch_layout).unwrap_or(out_ch_layout);

        with_normalized_layout(src_frame, |src_frame| {
            // 容量按输出样本数的上界分配，避免上采样（in < out）时尾部样本被丢弃。
            let capacity = self.get_out_samples(src_frame.nb_samples);
            let mut out_samples =
                AVSamples::new(out_ch_layout.nb_channels, capacity, out_sample_fmt, 0)
                    .context("Create samples buffer failed.")?;

            let converted = unsafe {
                self.swr
                    .convert(
                        out_samples.audio_data.as_mut_ptr(),
                        capacity,
                        src_frame.extended_data as *const _,
                        src_frame.nb_samples,
                    )
                    .context("Could not convert input samples")?
            };

            // `AVSamples::nb_samples` 是**容量**（rsmpeg 的约定），所以实际样本数
            // 单独返回，调用方无需猜测缓冲区里有多少是有效的。
            Ok((out_samples, converted))
        })
    }

    /// Drain the remaining samples from the resampler (EOF flush).
    ///
    /// `dst` must already have an allocated buffer; after conversion,
    /// `dst.nb_samples` is the actual number of samples (possibly 0).
    /// Repeat until 0 samples are returned to fully drain.
    pub fn flush(&mut self, dst: &mut AVFrame) -> Result<()> {
        self.swr
            .convert_frame(None, dst)
            .context("Failed to flush streaming resampler")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{Context, Result};
    use crate::{SampleFormat, time};
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
                        let data = unsafe {
                            std::slice::from_raw_parts_mut(frame.data[ch] as *mut $type, nb_samples)
                        };
                        for (i, sample) in data.iter_mut().enumerate() {
                            *sample = ((i * nb_channels + ch) as f64
                                / (nb_samples * nb_channels) as f64
                                * $max_val as f64) as $type;
                        }
                    }
                } else {
                    let data = unsafe {
                        std::slice::from_raw_parts_mut(
                            frame.data[0] as *mut $type,
                            nb_samples * nb_channels,
                        )
                    };
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
            _ => return Err(RsmediaError::msg("Unsupported sample format")),
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
        frame.set_time_base(time::new_rational(1, sample_rate));

        frame
            .alloc_buffer()
            .context("Failed to allocate frame buffer")?;

        unsafe {
            fill_test_data(&mut frame, format_desc).context("Failed to fill test data")?;
        }

        Ok(frame)
    }

    /// 无声道掩码的容器（如不带 `dwChannelMask` 的 WAV）解码出的帧布局是
    /// `AV_CHANNEL_ORDER_UNSPEC`；重采样器必须接受它——此前 `swr_convert`
    /// 会以 `AVERROR_INPUT_CHANGED`/`OUTPUT_CHANGED` 拒绝每一帧。
    #[test]
    fn test_convert_frame_with_unspec_channel_layout() -> Result<()> {
        let mut frame = create_test_frame(&AUDIO_FORMATS[2], 44100, 2, 1024)?;
        let mut unspec = AVChannelLayout::from_nb_channels(2).into_inner();
        unspec.order = ffi::AV_CHANNEL_ORDER_UNSPEC;
        unspec.u.mask = 0;
        frame.set_ch_layout(unspec);

        // 输入与输出布局都按 UNSPEC 传入：两侧都要被归一化。
        let out = convert_frame(
            &frame,
            AVChannelLayout::from_nb_channels(2).into_inner(),
            ffi::AV_SAMPLE_FMT_FLTP,
            44100,
        )?;
        assert_eq!(out.format, ffi::AV_SAMPLE_FMT_FLTP);
        assert_eq!(out.ch_layout.order, ffi::AV_CHANNEL_ORDER_NATIVE);
        assert_eq!(out.ch_layout.nb_channels, 2);
        assert_eq!(out.nb_samples, frame.nb_samples);
        Ok(())
    }

    #[test]
    fn test_format_conversion() -> Result<()> {
        let sample_rate = 44100;
        let nb_samples = 1024;
        let nb_channels = 2;

        for in_fmt in AUDIO_FORMATS {
            println!("\nTesting input format: {:?}", in_fmt);

            let src_frame = create_test_frame(in_fmt, sample_rate, nb_channels, nb_samples)
                .with_context(|| format!("Failed to create source frame for {:?}", in_fmt))?;

            for out_fmt in AUDIO_FORMATS {
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

                    let src_frame = create_test_frame(in_fmt, in_rate, in_channels, nb_samples)?;

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

    /// 流式重采样器会在内部保留采样率换算的余数，跨调用携带；`flush` 必须把尾巴
    /// 排空。否则每个 chunk 都会丢掉不足一个输出样本的余量，长音频累计起来就是
    /// 可听的时长缺失——一次性 `convert_frame` 每次新建上下文，暴露不出这个问题。
    #[test]
    fn test_streaming_resampler_carries_delay_and_flushes() -> Result<()> {
        let (in_rate, out_rate) = (48_000, 44_100);
        let (channels, in_samples, chunks) = (2, 1024, 5);
        let layout = || AVChannelLayout::from_nb_channels(channels).into_inner();

        let mut resampler = Resampler::new(
            layout(),
            ffi::AV_SAMPLE_FMT_FLTP,
            in_rate,
            layout(),
            ffi::AV_SAMPLE_FMT_FLTP,
            out_rate,
        )?;

        let mut produced = 0i64;
        for chunk in 0..chunks {
            let src = create_test_frame(&AUDIO_FORMATS[7], in_rate, channels, in_samples)?;
            let (_, converted) = resampler.convert(&src, layout(), ffi::AV_SAMPLE_FMT_FLTP)?;
            assert!(
                converted >= 0,
                "chunk {chunk}: swr_convert returned {converted}"
            );
            produced += i64::from(converted);
        }

        // 排空延迟缓冲：容量按输出上界分配，`nb_samples` 回填实际数量。
        let mut tail = AVFrame::new();
        tail.set_format(ffi::AV_SAMPLE_FMT_FLTP);
        tail.set_ch_layout(layout());
        tail.set_sample_rate(out_rate);
        tail.set_nb_samples(resampler.get_out_samples(in_samples));
        tail.alloc_buffer()
            .context("Failed to allocate flush frame")?;
        resampler.flush(&mut tail)?;
        assert!(
            tail.nb_samples > 0,
            "flush produced nothing: the delay line was never drained"
        );
        produced += i64::from(tail.nb_samples);

        let expected = f64::from(in_samples * chunks) * f64::from(out_rate) / f64::from(in_rate);
        assert!(
            (produced as f64 - expected).abs() <= 2.0,
            "expected ~{expected:.0} samples across {chunks} chunks + flush, got {produced}"
        );
        Ok(())
    }
}
