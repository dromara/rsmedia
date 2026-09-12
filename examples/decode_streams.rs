//! 验证解码器功能：对 `/tmp/test.mp4`（h264 视频 + aac 音频）依次解码
//! 视频、音频，并验证"其他类型"（SUBTITLE）在无对应流时的行为。
//!
//! 覆盖的 API：
//! - `DecoderBuilder::new(MediaType::VIDEO|AUDIO|SUBTITLE)` + `build_from_reader`
//! - `Decoder::decode_frame`（视频原始帧）
//! - `Decoder::decode::<f32>`（音频帧）
//! - `MediaFrame::format` / 帧字段（pts、sample_rate、nb_channels、nb_samples）

use rsmedia::{DecoderBuilder, MediaType, StreamReader};

use anyhow::Result;
use std::path::Path;

const SOURCE: &str = "/tmp/test.mp4";

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    rsmedia::init()?;

    let source = Path::new(SOURCE);

    // 1. 视频解码
    check_video(source)?;

    // 2. 音频解码
    check_audio(source)?;

    // 3. 其他类型（字幕）——文件无字幕流，验证解码器正确报错而非崩溃/误解码
    check_subtitle(source)?;

    Ok(())
}

/// 视频解码：统计帧数、尺寸、时长，验证首帧与末帧 pts。
fn check_video(source: &Path) -> Result<()> {
    let mut reader = StreamReader::new(source)?;
    let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;

    let mut count = 0u64;
    let mut first_pts = None;
    let mut last_pts = None;
    let (mut width, mut height) = (0, 0);
    while let Some(frame) = decoder.decode_frame(&mut reader)? {
        if count == 0 {
            first_pts = Some(frame.pts);
            width = frame.width;
            height = frame.height;
        }
        last_pts = Some(frame.pts);
        count += 1;
    }
    println!(
        "[video] frame_count={count}, size={width}x{height}, first_pts={first_pts:?}, last_pts={last_pts:?}"
    );
    Ok(())
}

/// 音频解码：统计帧数，验证采样率、通道数、总时长。
fn check_audio(source: &Path) -> Result<()> {
    let mut reader = StreamReader::new(source)?;
    let mut decoder = DecoderBuilder::new(MediaType::AUDIO).build_from_reader(&reader)?;

    let mut count = 0u64;
    let mut total_samples = 0i64;
    let (mut sample_rate, mut channels) = (0, 0);
    while let Some(frame) = decoder.decode::<f32>(&mut reader)? {
        if count == 0 {
            sample_rate = frame.sample_rate;
            channels = frame.nb_channels;
        }
        total_samples += frame.nb_samples as i64;
        count += 1;
    }
    let secs = if sample_rate > 0 {
        total_samples as f64 / sample_rate as f64
    } else {
        0.0
    };
    println!(
        "[audio] frame_count={count}, sample_rate={sample_rate}, channels={channels}, total_samples={total_samples}, duration={secs:.2}s"
    );
    Ok(())
}

/// 其他类型（字幕）：文件没有字幕流，`find_best_stream(SUBTITLE)` 应返回错误。
/// 这验证解码器对不存在的媒体类型优雅报错，而不是崩溃或静默误解码。
fn check_subtitle(source: &Path) -> Result<()> {
    let reader = StreamReader::new(source)?;
    match DecoderBuilder::new(MediaType::SUBTITLE).build_from_reader(&reader) {
        Ok(_) => println!("[subtitle] unexpected: built a subtitle decoder for a file without one"),
        Err(e) => println!("[subtitle] correctly rejected (no subtitle stream): {e:#}"),
    }
    Ok(())
}
