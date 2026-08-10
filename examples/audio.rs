//! 演示音频的解码与编码。
//!
//! 覆盖的 API：
//! - `DecoderBuilder::new(MediaType::AUDIO)` + `build_wrapped` —— 音频解码器
//! - `DecoderWrapper::decode::<f32>` —— 解码为音频 [`MediaFrame`]
//! - `MediaFrame::audio_format` —— 读取音频帧的采样格式（类型安全访问器）
//! - `EncoderBuilder::new_audio` —— 音频编码器
//! - `MediaFrame::new_audio_frame` —— 创建音频帧（数据布局 `(1, nb_samples, nb_channels)`）
//! - `EncoderWrapper::encode` / `EncoderWrapper::finish`

use rsmedia::{DecoderBuilder, EncoderBuilder, MediaFrame, MediaType, SampleFormat};

use anyhow::Result;
use rsmedia::time::{self, Time};
use std::path::Path;

const SAMPLE_RATE: u32 = 44_100;
const CHANNELS: u32 = 2;
const NB_SAMPLES: u32 = 1024;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    rsmedia::init()?;

    // 1. 演示音频解码
    decode_audio(Path::new("/tmp/test.mp4"))?;

    // 2. 演示音频编码（生成 1 秒正弦波写入 wav）
    encode_audio(Path::new("/tmp/sine.wav"))?;

    Ok(())
}

/// 解码音频流，打印每帧的采样格式信息。
fn decode_audio(source: &Path) -> Result<()> {
    let mut decoder = DecoderBuilder::new(MediaType::AUDIO).build_wrapped(source)?;

    let mut frames = 0;
    while let Some(frame) = decoder.decode::<f32>()? {
        let fmt = frame
            .audio_format()
            .map(|f| f.get_sample_fmt_name())
            .unwrap_or_else(|| "n/a".to_string());
        println!(
            "[decode] pts={}, sample_rate={}, channels={}, samples={}, format={fmt}",
            frame.pts, frame.sample_rate, frame.nb_channels, frame.nb_samples
        );
        frames += 1;
        if frames >= 10 {
            break;
        }
    }
    println!("decoded {frames} audio frames");
    Ok(())
}

/// 编码 1 秒正弦波音频。
fn encode_audio(output: &Path) -> Result<()> {
    let mut encoder = EncoderBuilder::new_audio(
        128_000,
        CHANNELS as i32,
        SAMPLE_RATE as i32,
        SampleFormat::FLTP,
    )
    .build_wrapped(output)?;

    let frame_duration = Time::new(
        Some(NB_SAMPLES as i64),
        time::new_rational(1, SAMPLE_RATE as i32),
    );
    let mut position = Time::zero();

    let total_frames = SAMPLE_RATE / NB_SAMPLES;
    for i in 0..total_frames {
        let mut frame = sine_frame(i as f32 * NB_SAMPLES as f32 / SAMPLE_RATE as f32)?;
        frame.set_pts(
            position
                .aligned_with_rational(encoder.time_base())
                .into_value()
                .unwrap(),
        );

        encoder.encode(frame)?;
        position = position.aligned_with(frame_duration).add();
    }

    encoder.finish()?;
    println!("encoded audio to {:?}", output);
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
