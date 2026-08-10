//! 演示解码器的定位（seek）能力。
//!
//! 覆盖的 API：
//! - `DecoderWrapper::seek_to_frame` —— 定位到指定帧号
//! - `DecoderWrapper::seek_to_timestamp` —— 定位到指定时间戳（毫秒）
//! - `DecoderWrapper::seek_to_start` —— 回到开头

use rsmedia::{DecoderBuilder, MediaType, Reader};

use anyhow::Result;
use rsmedia::decode::DecoderWrapper;
use std::path::Path;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    rsmedia::init()?;

    let source = Path::new("/tmp/test.mp4");
    let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_wrapped(source)?;

    // 1. 定位到第 3 帧（关键帧，可直接解码）
    decoder.seek_to_frame(3)?;
    println!("seek_to_frame(3) -> pts={}", next_pts(&mut decoder)?);

    // 2. 定位到 10 秒处（按时间戳 seek，落到最近关键帧）
    decoder.seek_to_timestamp(10_000)?;
    println!(
        "seek_to_timestamp(10_000ms) -> pts={}",
        next_pts(&mut decoder)?
    );

    // 3. 回到开头
    decoder.seek_to_start()?;
    println!("seek_to_start() -> pts={}", next_pts(&mut decoder)?);

    Ok(())
}

/// 逐步读取，直到解码出下一帧并返回其 PTS。
fn next_pts<R>(decoder: &mut DecoderWrapper<R>) -> Result<i64>
where
    R: Reader,
{
    for _ in 0..100 {
        if let Some(frame) = decoder.decode_frame()? {
            return Ok(frame.pts);
        }
    }
    Err(anyhow::anyhow!("no frame after seek"))
}
