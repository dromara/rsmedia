//! RIIR: https://github.com/FFmpeg/FFmpeg/blob/master/doc/examples/encode_audio.c

use rsmpeg::{
    avcodec::{AVCodec, AVCodecContext},
    avformat::AVFormatContextOutput,
    avutil::{self, AVChannelLayout, AVFrame},
    error::RsmpegError,
    ffi,
};

use anyhow::{Context, Result};
use std::ffi::CStr;

/// 编码器能力描述结构体
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CodecConfig {
    /// 支持的帧率列表（视频编码器）
    pub supported_frame_rates: Option<Vec<ffi::AVRational>>,
    /// 支持的采样率列表（音频编码器）
    pub supported_sample_rates: Vec<i32>,
    /// 支持的像素格式列表（视频编码器）
    pub supported_pix_fmts: Vec<ffi::AVPixelFormat>,
    /// 支持的采样格式列表（音频编码器）
    pub supported_sample_fmts: Vec<ffi::AVSampleFormat>,
    /// 典型帧大小（单位：样本数/视频帧）
    pub frame_size: i32,
}

impl CodecConfig {
    /// 从AVCodec动态获取编码器配置
    pub fn new(codec: &AVCodec) -> Self {
        // 获取视频相关参数
        let supported_frame_rates = codec.supported_framerates().map(|rates| rates.to_vec());

        let supported_pix_fmts = codec
            .pix_fmts()
            .unwrap_or(&[])
            .iter()
            .filter(|&&fmt| fmt != ffi::AV_PIX_FMT_NONE)
            .cloned()
            .collect();

        // 获取音频相关参数
        let supported_sample_fmts = codec
            .sample_fmts()
            .unwrap_or(&[])
            .iter()
            .filter(|&&fmt| fmt != ffi::AV_SAMPLE_FMT_NONE)
            .cloned()
            .collect();

        let supported_sample_rates = codec
            .supported_samplerates()
            .unwrap_or(&[44_100, 48_000])
            .to_vec();

        let frame_size = if codec.capabilities & ffi::AV_CODEC_CAP_VARIABLE_FRAME_SIZE as i32 != 0 {
            1024
        } else {
            match codec.id {
                ffi::AV_CODEC_ID_FLAC => 4608,
                ffi::AV_CODEC_ID_ALAC => 4096,
                ffi::AV_CODEC_ID_WMAV2 => 2048,
                ffi::AV_CODEC_ID_AC3 => 1536,
                ffi::AV_CODEC_ID_MP2 => 1152,
                ffi::AV_CODEC_ID_MP3 => 1152,
                ffi::AV_CODEC_ID_OPUS => 960,
                ffi::AV_CODEC_ID_DTS => 512,
                ffi::AV_CODEC_ID_AMR_NB => 160,
                ffi::AV_CODEC_ID_VORBIS => 64,
                _ => 1024,
            }
        };

        Self {
            supported_frame_rates,
            supported_sample_rates,
            supported_pix_fmts,
            supported_sample_fmts,
            frame_size,
        }
    }

    /// 检查采样格式是否支持
    pub fn is_sample_fmt_supported(&self, fmt: ffi::AVSampleFormat) -> bool {
        self.supported_sample_fmts.contains(&fmt)
    }

    /// 获取默认采样格式（优先级：FLTP > S16P > 首个支持格式）
    #[allow(dead_code)]
    pub fn default_sample_fmt(&self) -> Option<ffi::AVSampleFormat> {
        [ffi::AV_SAMPLE_FMT_FLTP, ffi::AV_SAMPLE_FMT_S16P]
            .iter()
            .find(|&&fmt| self.supported_sample_fmts.contains(&fmt))
            .copied()
            .or_else(|| self.supported_sample_fmts.first().copied())
    }
}

/// 生成正弦波音频样本（优化内存访问）
fn generate_sine_wave(frame: &mut AVFrame, frequency: f64, sample_rate: i32) -> Result<()> {
    let sample_fmt = frame.format;
    let channels = frame.ch_layout.nb_channels as usize;
    let sample_count = frame.nb_samples as usize;
    let is_planar = matches!(
        sample_fmt,
        ffi::AV_SAMPLE_FMT_U8P
            | ffi::AV_SAMPLE_FMT_S16P
            | ffi::AV_SAMPLE_FMT_S32P
            | ffi::AV_SAMPLE_FMT_S64P
            | ffi::AV_SAMPLE_FMT_FLTP
            | ffi::AV_SAMPLE_FMT_DBLP
    );

    // 公共样本生成逻辑
    let generate_samples = |buffer: &mut [f64], channel_offset: usize| {
        for (i, sample) in buffer.iter_mut().enumerate() {
            let t = (i * channels + channel_offset) as f64 / sample_rate as f64;
            *sample = (2.0 * std::f64::consts::PI * frequency * t).sin() * 0.5;
        }
    };

    match sample_fmt {
        ffi::AV_SAMPLE_FMT_U8 | ffi::AV_SAMPLE_FMT_U8P => process_sample_data::<u8>(
            frame,
            is_planar,
            channels,
            sample_count,
            generate_samples,
            |v| (v * 127.5 + 127.5).round() as u8,
        ),

        ffi::AV_SAMPLE_FMT_S16 | ffi::AV_SAMPLE_FMT_S16P => process_sample_data::<i16>(
            frame,
            is_planar,
            channels,
            sample_count,
            generate_samples,
            |v| (v * i16::MAX as f64).round() as i16,
        ),

        ffi::AV_SAMPLE_FMT_S32 | ffi::AV_SAMPLE_FMT_S32P => process_sample_data::<i32>(
            frame,
            is_planar,
            channels,
            sample_count,
            generate_samples,
            |v| (v * i32::MAX as f64).round() as i32,
        ),

        ffi::AV_SAMPLE_FMT_S64 | ffi::AV_SAMPLE_FMT_S64P => process_sample_data::<i64>(
            frame,
            is_planar,
            channels,
            sample_count,
            generate_samples,
            |v| (v * i64::MAX as f64).round() as i64,
        ),

        ffi::AV_SAMPLE_FMT_FLT | ffi::AV_SAMPLE_FMT_FLTP => process_sample_data::<f32>(
            frame,
            is_planar,
            channels,
            sample_count,
            generate_samples,
            |v| v as f32,
        ),

        ffi::AV_SAMPLE_FMT_DBL | ffi::AV_SAMPLE_FMT_DBLP => process_sample_data::<f64>(
            frame,
            is_planar,
            channels,
            sample_count,
            |buf, ch| {
                for (i, v) in buf.iter_mut().enumerate() {
                    let t = (i * channels + ch) as f64 / sample_rate as f64;
                    *v = (2.0 * std::f64::consts::PI * frequency * t).sin() * 0.5;
                }
            },
            |v| v,
        ),

        _ => return Err(anyhow::anyhow!("Unsupported format")),
    }

    Ok(())
}

/// 通用样本数据处理函数
fn process_sample_data<T: Copy>(
    frame: &mut AVFrame,
    is_planar: bool,
    channels: usize,
    sample_count: usize,
    generate_samples: impl Fn(&mut [f64], usize),
    scale: impl Fn(f64) -> T,
) {
    let frame_ptr = frame.as_mut_ptr();

    if is_planar {
        // 平面格式处理
        for channel in 0..channels {
            let data_ptr = unsafe { (*frame_ptr).data[channel] };
            assert!(!data_ptr.is_null(), "Channel {} data is null", channel);

            let buffer =
                unsafe { std::slice::from_raw_parts_mut(data_ptr as *mut T, sample_count) };

            let mut float_buffer = vec![0.0; sample_count];
            generate_samples(&mut float_buffer, channel);

            for (i, &v) in float_buffer.iter().enumerate() {
                buffer[i] = scale(v);
            }
        }
    } else {
        // 打包格式处理
        let buffer = unsafe {
            std::slice::from_raw_parts_mut((*frame_ptr).data[0] as *mut T, sample_count * channels)
        };

        let mut float_buffer = vec![0.0; sample_count * channels];
        for channel in 0..channels {
            generate_samples(&mut float_buffer[channel..], channel);
        }

        for (i, &v) in float_buffer.iter().enumerate() {
            buffer[i] = scale(v);
        }
    }
}

/// 编码帧处理（强化错误处理）
fn encode_frame(
    encode_ctx: &mut AVCodecContext,
    format_ctx: &mut AVFormatContextOutput,
    frame: Option<&AVFrame>,
) -> Result<()> {
    encode_ctx
        .send_frame(frame)
        .context("Failed to send frame")?;

    loop {
        let mut packet = match encode_ctx.receive_packet() {
            Ok(packet) => packet,
            Err(RsmpegError::EncoderDrainError) | Err(RsmpegError::EncoderFlushedError) => break,
            Err(e) => return Err(e).context("Encoding error"),
        };

        format_ctx
            .write_frame(&mut packet)
            .context("Failed to write packet")?;
    }

    Ok(())
}

/// 音频编码主流程（强化资源管理）
fn encode_audio(
    output_path: &CStr,
    codec_id: ffi::AVCodecID,
    sample_format: i32,
    bit_rate: i64,
    nb_channels: i32,
) -> Result<()> {
    // 初始化编码器上下文
    let codec =
        AVCodec::find_encoder(codec_id).context(format!("Failed to find encoder: {}", codec_id))?;
    let mut encode_ctx = AVCodecContext::new(&codec);

    let codec_config = CodecConfig::new(&codec);
    let sample_rate = codec_config
        .supported_sample_rates
        .first()
        .copied()
        .unwrap();

    println!(
        "encode_audio: output_path:{}, codec_id:{}, sample_format:{}, codec_config: {:#?}",
        output_path.to_string_lossy(),
        codec_id,
        sample_format,
        codec_config
    );

    assert!(
        codec_config.is_sample_fmt_supported(sample_format),
        "Unsupported sample format"
    );

    // 配置编码参数
    encode_ctx.set_ch_layout(AVChannelLayout::from_nb_channels(nb_channels).into_inner());
    encode_ctx.set_sample_rate(sample_rate);
    encode_ctx.set_time_base(avutil::ra(1, sample_rate));
    encode_ctx.set_sample_fmt(sample_format);
    // max_bit_rate = (bit_rate * sample_rate) / frame_size
    encode_ctx.set_bit_rate(bit_rate);

    encode_ctx
        .open(None)
        .context("Failed to initialize encoder")?;

    // 初始化输出容器
    let mut format_ctx =
        AVFormatContextOutput::create(output_path).context("Failed to create output context")?;
    {
        // 创建音频流
        let mut stream = format_ctx.new_stream();
        stream.set_codecpar(encode_ctx.extract_codecpar());
        stream.set_time_base(encode_ctx.time_base);
    }

    format_ctx
        .write_header(&mut None)
        .context("Failed to write file header")?;

    // 初始化音频帧（带自动内存管理）
    let mut frame = AVFrame::new();
    frame.set_nb_samples(codec_config.frame_size);
    frame.set_ch_layout(encode_ctx.ch_layout);
    frame.set_format(encode_ctx.sample_fmt);
    frame
        .alloc_buffer()
        .context("Failed to allocate frame buffer")?;

    // 编码 5 秒音频数据
    let duration_seconds = 5;
    let total_samples = duration_seconds * encode_ctx.sample_rate;
    let samples_per_frame = frame.nb_samples;

    for pts in (0..total_samples).step_by(samples_per_frame as usize) {
        // 生成正弦波数据
        generate_sine_wave(&mut frame, 440.0, encode_ctx.sample_rate)
            .context("Failed to generate samples")?;

        // 设置精确时间戳
        frame.set_pts(pts as i64);

        // 编码处理
        encode_frame(&mut encode_ctx, &mut format_ctx, Some(&frame))
            .context("Frame encoding failed")?;
    }

    // 冲刷编码器缓冲区
    encode_frame(&mut encode_ctx, &mut format_ctx, None).context("Encoder flush failed")?;

    format_ctx
        .write_trailer()
        .context("Failed to write file trailer")?;

    Ok(())
}

#[test]
#[ignore = "Ignore the test for now"]
fn test_encode_audio_aac() {
    // aac 有损格式 (192kbps)
    encode_audio(
        c"/tmp/encode_audio_output.aac",
        ffi::AV_CODEC_ID_AAC,
        ffi::AV_SAMPLE_FMT_FLTP,
        192_000,
        2,
    )
    .unwrap();
}

#[test]
#[ignore = "Ignore the test for now"]
fn test_encode_audio_m4a() {
    // m4a AAC容器 (256kbps)
    encode_audio(
        c"/tmp/encode_audio_output.m4a",
        ffi::AV_CODEC_ID_AAC,
        ffi::AV_SAMPLE_FMT_FLTP,
        256_000,
        2,
    )
    .unwrap();
}

#[test]
#[ignore = "Ignore the test for now"]
fn test_encode_audio_caf() {
    // caf ALAC无损格式
    encode_audio(
        c"/tmp/encode_audio_output.caf",
        ffi::AV_CODEC_ID_ALAC,
        ffi::AV_SAMPLE_FMT_S32P,
        0,
        2,
    )
    .unwrap();
}

#[test]
#[ignore = "Ignore the test for now"]
fn test_encode_audio_mp3() {
    // mp3 有损格式 (128kbps)
    encode_audio(
        c"/tmp/encode_audio_output.mp3",
        ffi::AV_CODEC_ID_MP3,
        ffi::AV_SAMPLE_FMT_S16P,
        128_000,
        2,
    )
    .unwrap();
}

#[test]
#[ignore = "Ignore the test for now"]
fn test_encode_audio_flac() {
    // flac 无损格式 (24-bit)
    encode_audio(
        c"/tmp/encode_audio_output.flac",
        ffi::AV_CODEC_ID_FLAC,
        ffi::AV_SAMPLE_FMT_S16,
        0,
        2,
    )
    .unwrap();
}

#[test]
#[ignore = "Ignore the test for now"]
fn test_encode_audio_wav() {
    // wav - EBU R128标准 (24-bit/48kHz)
    encode_audio(
        c"/tmp/encode_audio_output.wav",
        ffi::AV_CODEC_ID_PCM_S24LE,
        ffi::AV_SAMPLE_FMT_S32,
        2_304_000,
        2,
    )
    .unwrap();
}

#[test]
#[ignore = "Ignore the test for now"]
fn test_encode_audio_wav_16bit() {
    // WAV - 16-bit PCM
    encode_audio(
        c"/tmp/encode_audio_output_16bit.wav",
        ffi::AV_CODEC_ID_PCM_S16LE,
        ffi::AV_SAMPLE_FMT_S16,
        1_536_000, // 48kHz * 16bit * 2ch
        2,
    )
    .unwrap();
}

#[test]
#[ignore = "Ignore the test for now"]
fn test_encode_audio_ac3() {
    // AC3 - 5.1声道 (640kbps)
    encode_audio(
        c"/tmp/encode_audio_output.ac3",
        ffi::AV_CODEC_ID_AC3,
        ffi::AV_SAMPLE_FMT_FLTP,
        640_000,
        6,
    )
    .unwrap();
}

#[test]
#[ignore = "Ignore the test for now"]
fn test_encode_audio_opus() {
    // Opus - 低延迟语音编码 (64kbps)
    encode_audio(
        c"/tmp/encode_audio_output.opus",
        ffi::AV_CODEC_ID_OPUS,
        ffi::AV_SAMPLE_FMT_FLT,
        64_000,
        2,
    )
    .unwrap();
}

#[test]
#[ignore = "Ignore the test for now"]
fn test_encode_audio_ogg_vorbis() {
    // Vorbis - OGG容器 (128kbps)
    encode_audio(
        c"/tmp/encode_audio_output.ogg",
        ffi::AV_CODEC_ID_VORBIS,
        ffi::AV_SAMPLE_FMT_FLTP,
        128_000,
        2,
    )
    .unwrap();
}

#[test]
#[ignore = "Ignore the test for now"]
fn test_encode_audio_wmav2() {
    // WMA - Windows Media Audio (128kbps)
    encode_audio(
        c"/tmp/encode_audio_output.wma",
        ffi::AV_CODEC_ID_WMAV2,
        ffi::AV_SAMPLE_FMT_FLTP,
        128_000,
        2,
    )
    .unwrap();
}

#[test]
#[ignore = "Ignore the test for now"]
fn test_encode_audio_aiff() {
    // AIFF - Apple无压缩格式 (24-bit)
    encode_audio(
        c"/tmp/encode_audio_output.aiff",
        ffi::AV_CODEC_ID_PCM_S24BE,
        ffi::AV_SAMPLE_FMT_S32,
        0,
        2,
    )
    .unwrap();
}

#[test]
#[ignore = "[libopencore_amrnb] Only 8000Hz sample rate supported"]
fn test_encode_audio_amr() {
    // AMR-NB - 移动语音编码 (12.2kbps)
    encode_audio(
        c"/tmp/encode_audio_output.amr",
        ffi::AV_CODEC_ID_AMR_NB,
        ffi::AV_SAMPLE_FMT_S16,
        12200,
        1,
    )
    .unwrap();
}
