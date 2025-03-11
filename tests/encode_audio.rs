//! RIIR: https://github.com/FFmpeg/FFmpeg/blob/master/doc/examples/encode_audio.c

use anyhow::{Context, Result};
use rsmpeg::{
    avcodec::{AVCodec, AVCodecContext},
    avutil::{self, AVChannelLayout, AVFrame},
    ffi,
};

use cstr::cstr;
use rsmpeg::avformat::AVFormatContextOutput;
use rsmpeg::error::RsmpegError;
use std::f32::consts::PI;
use std::ffi::CStr;

/// 获取编码器配置: (frame_size, sample_rates, sample_fmt)
fn get_codec_params(codec_id: ffi::AVCodecID) -> (i32, Vec<i32>, Vec<ffi::AVSampleFormat>) {
    match codec_id {
        // MP3 (MPEG Layer-3)
        ffi::AV_CODEC_ID_MP3 => (
            1152,
            vec![44100, 48000, 32000],
            vec![ffi::AV_SAMPLE_FMT_S16P, ffi::AV_SAMPLE_FMT_FLTP],
        ),

        // WMA (Windows Media Audio)
        ffi::AV_CODEC_ID_WMAV2 => (
            2048,
            vec![44100, 48000, 32000, 22050, 16000, 11025, 8000],
            vec![ffi::AV_SAMPLE_FMT_FLTP],
        ),

        // ALAC (Apple Lossless)
        ffi::AV_CODEC_ID_ALAC => (
            4096,
            vec![44100, 48000, 88200, 96000, 176400, 192000],
            vec![
                ffi::AV_SAMPLE_FMT_S16P,
                ffi::AV_SAMPLE_FMT_S32P,
                ffi::AV_SAMPLE_FMT_FLTP,
            ],
        ),

        // FLAC
        ffi::AV_CODEC_ID_FLAC => (
            576,
            vec![
                8000, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000,
            ],
            vec![
                ffi::AV_SAMPLE_FMT_S16,
                ffi::AV_SAMPLE_FMT_S32,
                ffi::AV_SAMPLE_FMT_S16P,
                ffi::AV_SAMPLE_FMT_S32P,
            ],
        ),

        // AC3 (Dolby Digital)
        ffi::AV_CODEC_ID_AC3 => (
            1536,
            vec![48000, 44100, 32000],
            vec![ffi::AV_SAMPLE_FMT_FLTP, ffi::AV_SAMPLE_FMT_S16P],
        ),

        // PCM_S16LE
        ffi::AV_CODEC_ID_PCM_S16LE => (
            1024,
            vec![
                8000, 11025, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000,
            ],
            vec![ffi::AV_SAMPLE_FMT_S16, ffi::AV_SAMPLE_FMT_S16P],
        ),

        // PCM formats (S24LE, S32LE)
        ffi::AV_CODEC_ID_PCM_S24LE | ffi::AV_CODEC_ID_PCM_S32LE => (
            1024,
            vec![
                8000, 11025, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000,
            ],
            vec![
                ffi::AV_SAMPLE_FMT_S32,
                ffi::AV_SAMPLE_FMT_S32P,
                ffi::AV_SAMPLE_FMT_S16,
                ffi::AV_SAMPLE_FMT_S16P,
            ],
        ),

        // PCM_S24BE
        ffi::AV_CODEC_ID_PCM_S24BE => (
            1024,
            vec![
                8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200, 96000,
            ],
            vec![
                ffi::AV_SAMPLE_FMT_S32,
                ffi::AV_SAMPLE_FMT_S32P,
                ffi::AV_SAMPLE_FMT_S16,
                ffi::AV_SAMPLE_FMT_S16P,
            ],
        ),

        // AAC
        ffi::AV_CODEC_ID_AAC => (
            1024,
            vec![
                8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200, 96000,
            ],
            vec![ffi::AV_SAMPLE_FMT_FLTP],
        ),

        // Opus
        ffi::AV_CODEC_ID_OPUS => (
            960,
            vec![48000, 24000, 16000, 12000, 8000],
            vec![
                ffi::AV_SAMPLE_FMT_S16,
                ffi::AV_SAMPLE_FMT_FLT,
                ffi::AV_SAMPLE_FMT_S16P,
                ffi::AV_SAMPLE_FMT_FLTP,
            ],
        ),

        // Vorbis
        ffi::AV_CODEC_ID_VORBIS => (64, vec![44100, 48000, 32000], vec![ffi::AV_SAMPLE_FMT_FLTP]),

        // DTS
        ffi::AV_CODEC_ID_DTS => (
            512,
            vec![
                8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 88200, 96000, 192000,
            ],
            vec![
                ffi::AV_SAMPLE_FMT_S16P,
                ffi::AV_SAMPLE_FMT_S32P,
                ffi::AV_SAMPLE_FMT_FLTP,
            ],
        ),

        // MP2
        ffi::AV_CODEC_ID_MP2 => (
            1152,
            vec![44100, 48000, 32000],
            vec![ffi::AV_SAMPLE_FMT_S16P, ffi::AV_SAMPLE_FMT_FLTP],
        ),

        // AMR-NB
        ffi::AV_CODEC_ID_AMR_NB => (
            160,
            vec![8000],
            vec![ffi::AV_SAMPLE_FMT_S16, ffi::AV_SAMPLE_FMT_FLTP],
        ),

        // 默认配置
        _ => (
            1024,
            vec![44100, 48000],
            vec![
                ffi::AV_SAMPLE_FMT_FLTP,
                ffi::AV_SAMPLE_FMT_S16P,
                ffi::AV_SAMPLE_FMT_S16,
            ],
        ),
    }
}

/// 生成正弦波音频样本（优化内存访问）
fn generate_sine_wave(frame: &mut AVFrame, frequency: f32, sample_rate: i32) -> Result<()> {
    let sample_fmt = frame.format;
    let channels = frame.ch_layout.nb_channels as usize;
    let sample_count = frame.nb_samples as usize;
    let is_planar = matches!(
        sample_fmt,
        ffi::AV_SAMPLE_FMT_U8P
            | ffi::AV_SAMPLE_FMT_S16P
            | ffi::AV_SAMPLE_FMT_S32P
            | ffi::AV_SAMPLE_FMT_FLTP
            | ffi::AV_SAMPLE_FMT_DBLP
    );

    // 公共样本生成逻辑
    let generate_samples = |buffer: &mut [f32], channel_offset: usize| {
        for (i, sample) in buffer.iter_mut().enumerate() {
            let t = (i * channels + channel_offset) as f32 / sample_rate as f32;
            *sample = (2.0 * PI * frequency * t).sin() * 0.5;
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
            |v| (v * i16::MAX as f32).round() as i16,
        ),

        ffi::AV_SAMPLE_FMT_S32 | ffi::AV_SAMPLE_FMT_S32P => process_sample_data::<i32>(
            frame,
            is_planar,
            channels,
            sample_count,
            generate_samples,
            |v| (v * i32::MAX as f32).round() as i32,
        ),

        ffi::AV_SAMPLE_FMT_FLT | ffi::AV_SAMPLE_FMT_FLTP => process_sample_data::<f32>(
            frame,
            is_planar,
            channels,
            sample_count,
            generate_samples,
            |v| v,
        ),

        ffi::AV_SAMPLE_FMT_DBL | ffi::AV_SAMPLE_FMT_DBLP => process_sample_data::<f64>(
            frame,
            is_planar,
            channels,
            sample_count,
            |buf, ch| {
                for (i, v) in buf.iter_mut().enumerate() {
                    let t = (i * channels + ch) as f64 / sample_rate as f64;
                    *v = (2.0 * std::f64::consts::PI * frequency as f64 * t).sin() as f32 * 0.5;
                }
            },
            |v| v as f64,
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
    generate_samples: impl Fn(&mut [f32], usize),
    scale: impl Fn(f32) -> T,
) {
    let frame_ptr = frame.as_mut_ptr();

    if is_planar {
        // 平面格式处理
        for channel in 0..channels {
            let buffer = unsafe {
                std::slice::from_raw_parts_mut((*frame_ptr).data[channel] as *mut T, sample_count)
            };

            let mut float_buffer = vec![0.0f32; sample_count];
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

        let mut float_buffer = vec![0.0f32; sample_count * channels];
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

    // 获取编码器要求的帧大小
    let (frame_size, sample_rates, supported_sample_fmts) = get_codec_params(codec_id);
    assert!(
        supported_sample_fmts.contains(&sample_format),
        "Unsupported sample format"
    );

    // 配置编码参数
    encode_ctx.set_ch_layout(AVChannelLayout::from_nb_channels(nb_channels).into_inner());
    encode_ctx.set_sample_rate(sample_rates[0]);
    encode_ctx.set_time_base(avutil::ra(1, sample_rates[0])); // eg: 时间基 1/44100
    encode_ctx.set_sample_fmt(sample_format);
    // TODO:
    // max_bit_rate = (bit_rate * sample_rate) / frame_size_bites_per_second
    encode_ctx.set_bit_rate(bit_rate);

    encode_ctx
        .open(None)
        .context("Failed to initialize encoder")?;

    // 初始化输出容器
    let mut format_ctx = AVFormatContextOutput::create(output_path, None)
        .context("Failed to create output context")?;
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
    frame.set_nb_samples(frame_size);
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
fn test_encode_audio() {
    // aac 有损格式 (192kbps)
    encode_audio(
        cstr!("/tmp/encode_audio_output.aac"),
        ffi::AV_CODEC_ID_AAC,
        ffi::AV_SAMPLE_FMT_FLTP,
        192_000,
        2,
    )
    .unwrap();

    // m4a AAC容器 (256kbps)
    encode_audio(
        cstr!("/tmp/encode_audio_output.m4a"),
        ffi::AV_CODEC_ID_AAC,
        ffi::AV_SAMPLE_FMT_FLTP,
        256_000,
        2,
    )
    .unwrap();

    // caf ALAC无损格式
    encode_audio(
        cstr!("/tmp/encode_audio_output.caf"),
        ffi::AV_CODEC_ID_ALAC,
        ffi::AV_SAMPLE_FMT_S32P,
        0,
        2,
    )
    .unwrap();

    // mp3 有损格式 (128kbps)
    encode_audio(
        cstr!("/tmp/encode_audio_output.mp3"),
        ffi::AV_CODEC_ID_MP3,
        ffi::AV_SAMPLE_FMT_S16P,
        128_000,
        2,
    )
    .unwrap();

    // flac 无损格式 (24-bit)
    encode_audio(
        cstr!("/tmp/encode_audio_output.flac"),
        ffi::AV_CODEC_ID_FLAC,
        ffi::AV_SAMPLE_FMT_S32,
        0,
        2,
    )
    .unwrap();

    // wav - EBU R128标准 (24-bit/48kHz)
    encode_audio(
        cstr!("/tmp/encode_audio_output.wav"),
        ffi::AV_CODEC_ID_PCM_S24LE,
        ffi::AV_SAMPLE_FMT_S32,
        2304_000,
        2,
    )
    .unwrap();

    // AC3 - 5.1声道 (640kbps)
    encode_audio(
        cstr!("/tmp/encode_audio_output.ac3"),
        ffi::AV_CODEC_ID_AC3,
        ffi::AV_SAMPLE_FMT_FLTP,
        640_000,
        6,
    )
    .unwrap();

    // Opus - 低延迟语音编码 (64kbps)
    encode_audio(
        cstr!("/tmp/encode_audio_output.opus"),
        ffi::AV_CODEC_ID_OPUS,
        ffi::AV_SAMPLE_FMT_FLT,
        64_000,
        2,
    )
    .unwrap();

    // Vorbis - OGG容器 (128kbps)
    encode_audio(
        cstr!("/tmp/encode_audio_output.ogg"),
        ffi::AV_CODEC_ID_VORBIS,
        ffi::AV_SAMPLE_FMT_FLTP,
        128_000,
        2,
    )
    .unwrap();

    // WMA - Windows Media Audio (128kbps)
    encode_audio(
        cstr!("/tmp/encode_audio_output.wma"),
        ffi::AV_CODEC_ID_WMAV2,
        ffi::AV_SAMPLE_FMT_FLTP,
        128_000,
        2,
    )
    .unwrap();

    // WAV - 16-bit PCM
    encode_audio(
        cstr!("/tmp/encode_audio_output_16bit.wav"),
        ffi::AV_CODEC_ID_PCM_S16LE,
        ffi::AV_SAMPLE_FMT_S16,
        1536_000, // 48kHz * 16bit * 2ch
        2,
    )
    .unwrap();

    // AIFF - Apple无压缩格式 (24-bit)
    encode_audio(
        cstr!("/tmp/encode_audio_output.aiff"),
        ffi::AV_CODEC_ID_PCM_S24BE,
        ffi::AV_SAMPLE_FMT_S32,
        0,
        2,
    )
    .unwrap();

    // AMR-NB - 移动语音编码 (12.2kbps)
    encode_audio(
        cstr!("/tmp/encode_audio_output.amr"),
        ffi::AV_CODEC_ID_AMR_NB,
        ffi::AV_SAMPLE_FMT_S16,
        12200,
        1,
    )
    .unwrap();
}
