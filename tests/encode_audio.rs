//! RIIR: https://github.com/FFmpeg/FFmpeg/blob/master/doc/examples/encode_audio.c

use anyhow::{Context, Result};
use rsmpeg::{
    avcodec::{AVCodec, AVCodecContext},
    avutil::{AVChannelLayout, AVFrame},
    ffi,
};

use cstr::cstr;
use rsmpeg::avformat::AVFormatContextOutput;
use rsmpeg::avutil::ra;
use rsmpeg::error::RsmpegError;
use std::f32::consts::PI;
use std::ffi::CStr;

/// 生成正弦波音频样本
fn generate_sine_wave(frame: &mut AVFrame, frequency: f32, sample_rate: i32) {
    let _duration = frame.nb_samples as f32 / sample_rate as f32;
    let channels = frame.ch_layout.nb_channels as usize;

    let frame_ptr = frame.as_mut_ptr();

    // 仅处理浮点平面格式（如FLTP）
    for ch in 0..channels {
        let buffer = unsafe {
            std::slice::from_raw_parts_mut(
                (*frame_ptr).data[ch] as *mut f32,
                frame.nb_samples as usize,
            )
        };

        for i in 0..frame.nb_samples {
            let t = i as f32 / sample_rate as f32;
            let value = (2.0 * PI * frequency * t).sin();
            buffer[i as usize] = value * 0.5; // 降低音量避免削波
        }
    }
}

/// Return boolean: if data is written.
fn encode_audio_frame(
    frame: Option<&AVFrame>,
    ofctx: &mut AVFormatContextOutput,
    encode_context: &mut AVCodecContext,
) -> Result<()> {
    encode_context.send_frame(frame)?;

    loop {
        let mut packet = match encode_context.receive_packet() {
            Ok(packet) => packet,
            Err(RsmpegError::EncoderDrainError) | Err(RsmpegError::EncoderFlushedError) => {
                break;
            }
            Err(e) => Err(e).context("Could not encode frame")?,
        };

        ofctx
            .write_frame(&mut packet)
            .context("Could not write frame")?;
    }
    Ok(())
}

fn encode_audio(output_file: &CStr) -> Result<()> {
    // 初始化编码器（AAC）
    let codec =
        AVCodec::find_encoder(ffi::AV_CODEC_ID_AAC).context("Failed to find AAC encoder")?;

    // 创建编码器上下文
    let mut encode_context = AVCodecContext::new(&codec);

    // 配置编码参数
    encode_context.set_ch_layout(AVChannelLayout::from_nb_channels(2).into_inner()); // 立体声
    encode_context.set_sample_rate(44100); // 44.1kHz
    encode_context.set_sample_fmt(ffi::AV_SAMPLE_FMT_FLTP); // 浮点平面格式
    encode_context.set_bit_rate(128000); // 128kbps

    // 打开编码器
    encode_context.open(None).context("Failed to open codec")?;

    // 创建输出文件
    let mut ofctx =
        AVFormatContextOutput::create(output_file, None).context("Failed to open output file.")?;
    {
        // Create a new audio stream in the output file container.
        let mut stream = ofctx.new_stream();
        stream.set_codecpar(encode_context.extract_codecpar());
        // Set the sample rate for the container.
        stream.set_time_base(ra(1, encode_context.sample_rate));
    }

    ofctx
        .write_header(&mut None)
        .context("Could not write output file header")?;

    // 创建音频帧
    let mut frame = AVFrame::new();
    frame.set_nb_samples(encode_context.frame_size.min(1024)); // AAC通常1024样本/帧
    frame.set_ch_layout(encode_context.ch_layout);
    frame.set_format(encode_context.sample_fmt);
    frame.alloc_buffer().context("Failed to allocate frame")?;

    // 编码循环（生成1秒音频）
    let total_samples = encode_context.sample_rate * encode_context.ch_layout.nb_channels;
    let mut pts = 0;

    for _ in 0..(total_samples / frame.nb_samples) {
        // 生成正弦波样本（440Hz）
        generate_sine_wave(&mut frame, 440.0, encode_context.sample_rate);
        frame.set_pts(pts);

        pts += frame.nb_samples as i64;

        encode_audio_frame(Some(&frame), &mut ofctx, &mut encode_context)?
    }

    // Flush encode context
    encode_audio_frame(None, &mut ofctx, &mut encode_context)?;

    ofctx.write_trailer()?;

    Ok(())
}

#[test]
fn test_encode_audio() {
    encode_audio(cstr!("/tmp/output.aac")).unwrap();
}
