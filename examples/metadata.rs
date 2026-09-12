//! 演示读取解码器/编码器的元数据与流信息 getter。
//!
//! 覆盖的 API：
//! - `StreamInfo::from_reader` —— 读取流的 `StreamInfo`（宽高、比特率、时长等）
//! - 裸 `Decoder` 的只读 getter：
//!   - `width` / `height` / `pix_fmt`
//!   - `sample_rate` / `sample_fmt` / `ch_layout`
//!   - `duration` / `time_base` / `frames` / `frame_rate` / `media_type` / `stream_index`

use rsmedia::stream::StreamInfo;
use rsmedia::{DecoderBuilder, MediaType, StreamReader};

use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    rsmedia::init()?;

    let reader = StreamReader::new("/tmp/test.mp4")?;
    let decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;

    // 1. 通过 StreamInfo 读取流的整体信息
    let info = StreamInfo::from_reader(&reader, decoder.stream_index())?;
    println!("=== StreamInfo ===");
    println!("codec_id: {}", info.codec_id);
    println!("width x height: {}x{}", info.width, info.height);
    println!("bit_rate: {}", info.bit_rate);
    println!("format: {:?}", info.format);
    println!("time_base: {}/{}", info.time_base.num, info.time_base.den);
    println!(
        "frame_rate: {}/{}",
        info.frame_rate.num, info.frame_rate.den
    );

    // 2. 直接读取底层解码器已打开的上下文参数
    println!("=== Decoder getters ===");
    println!("media_type: {:?}", decoder.media_type());
    println!("stream_index: {}", decoder.stream_index());
    println!("width: {}, height: {}", decoder.width(), decoder.height());
    println!("pix_fmt: {}", decoder.pix_fmt().get_pix_fmt_name());
    println!("duration: {:?}", decoder.duration());
    println!(
        "time_base: {}/{}",
        decoder.time_base().num,
        decoder.time_base().den
    );
    println!(
        "frame_rate: (real={}, avg={})",
        decoder.frame_rate().0,
        decoder.frame_rate().1
    );
    println!("frames: {}", decoder.frames());

    Ok(())
}
