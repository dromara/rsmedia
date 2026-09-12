//! 演示音频的编码与解码。
//!
//! 覆盖的 API：
//! - `EncoderBuilder::new_audio` —— 音频编码器
//! - `MediaFrame::new_audio_frame` —— 创建音频帧（数据布局 `(1, nb_samples, nb_channels)`）
//! - 裸 `Encoder` + `Muxer`（`add_stream` + `mux` + `finish`）—— 音频封装
//! - `DecoderBuilder::new(MediaType::AUDIO)` + `build_from_reader` —— 音频解码器
//! - `Decoder::decode::<f32>` —— 解码为音频 [`MediaFrame`]
//! - `MediaFrame::format` —— 读取音频帧的采样格式（`MediaFrameFormat::Sample` 变体）

use rsmedia::time;
use rsmedia::{DecoderBuilder, EncoderBuilder, mux::Muxer};
use rsmedia::{FrameFormat, MediaFrame, MediaType, SampleFormat, StreamReader};

use anyhow::Result;

const SAMPLE_RATE: u32 = 44_100;
const CHANNELS: u32 = 2;
const NB_SAMPLES: u32 = 1024;
/// 编码时长（秒），>10s 便于用播放器/ffprobe 验证。
const DURATION_SEC: u32 = 12;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    rsmedia::init()?;

    // 1. 演示音频编码（生成 12 秒正弦波写入 m4a，aac 编码器的标准容器）
    encode_audio("/tmp/sine_12s.m4a")?;

    // 2. 演示音频解码
    decode_audio("/tmp/sine_12s.m4a")?;

    Ok(())
}

/// 编码 `DURATION_SEC` 秒正弦波音频。
///
/// 使用裸 `Encoder` + `Muxer`，pts 以样本数为单位按帧大小递增（音频的
/// 时间基为 `1/sample_rate`）。
fn encode_audio(output: &'static str) -> Result<()> {
    let encoder = EncoderBuilder::new_audio(
        128_000,
        CHANNELS as i32,
        SAMPLE_RATE as i32,
        SampleFormat::FLTP,
    )
    .build()?;
    let enc_tb = encoder.time_base();
    let mut muxer = Muxer::new(output)?;
    let a_idx = muxer.add_encoder(encoder)?;

    let total_frames = SAMPLE_RATE * DURATION_SEC / NB_SAMPLES;
    let mut total_pts: i64 = 0;
    for _ in 0..total_frames {
        let frame = sine_frame(total_pts as f32 / SAMPLE_RATE as f32)?;
        let mut av = frame.to_avframe()?;
        // 手动以样本数递增 pts（音频时间基为 1/sample_rate）
        av.set_pts(total_pts);
        av.set_time_base(enc_tb);
        total_pts += NB_SAMPLES as i64;
        muxer.mux(av, a_idx)?;
    }

    muxer.finish()?;
    println!(
        "encoded {total_frames} frames ({DURATION_SEC}s) audio to {:?}",
        output
    );
    Ok(())
}

/// 解码音频流，打印每帧的采样格式信息。
fn decode_audio(source: &'static str) -> Result<()> {
    let mut reader = StreamReader::new(source)?;
    let mut decoder = DecoderBuilder::new(MediaType::AUDIO).build_from_reader(&reader)?;

    let mut frames = 0;
    let mut total_samples = 0i64;
    while let Some(frame) = decoder.decode::<f32>(&mut reader)? {
        let fmt = frame
            .format()
            .map(|f| match f {
                FrameFormat::Sample(s) => s.get_sample_fmt_name().to_string(),
                _ => "N/A".to_string(),
            })
            .unwrap_or_else(|| "N/A".to_string());
        println!(
            "[decode] pts={}, sample_rate={}, channels={}, samples={}, format={fmt}",
            frame.pts, frame.sample_rate, frame.nb_channels, frame.nb_samples
        );
        total_samples += frame.nb_samples as i64;
        frames += 1;
        if frames >= 10 {
            break;
        }
    }
    println!(
        "decoded {frames} audio frames (first 10), ~{:.2}s",
        total_samples as f64 / SAMPLE_RATE as f64
    );
    Ok(())
}

/// 生成一段正弦波音频帧（FLTP 平面格式，数据布局 `(1, nb_samples, channels)`）。
fn sine_frame(t_start: f32) -> Result<MediaFrame<f32>> {
    let mut frame = MediaFrame::<f32>::new_audio_frame(
        SampleFormat::FLTP,
        CHANNELS,
        NB_SAMPLES,
        SAMPLE_RATE,
        time::new_rational(1, SAMPLE_RATE as i32),
    )?;

    let two_pi_f = 2.0 * std::f32::consts::PI * 440.0;
    for ch in 0..CHANNELS as usize {
        for i in 0..NB_SAMPLES as usize {
            let t = t_start + i as f32 / SAMPLE_RATE as f32;
            frame.data[[0, i, ch]] = (two_pi_f * t).sin() * 0.8;
        }
    }
    Ok(frame)
}
