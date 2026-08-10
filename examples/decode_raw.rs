//! 演示解码器的「原始帧」路径。
//!
//! 与高层 `decode::<T>()` 返回 ndarray 的 [`MediaFrame`] 不同，原始路径直接返回
//! FFmpeg 的 `AVFrame` / `AVPacket`，适合需要精确控制帧数据或做底层处理的场景。
//!
//! 覆盖的 API：
//! - `DecoderWrapper::decode_raw` —— 逐帧解码为原始 `AVFrame`
//! - `DecoderWrapper::into_parts` —— 解构出底层 `Decoder` 和 `Reader`
//! - `Decoder::decode_raw_packet` / `Decoder::drain_raw` —— 逐 packet 送入解码器并排空
//! - `Decoder::stream_index` —— 解码器所属的流索引

use rsmedia::{DecoderBuilder, MediaType, Reader};

use anyhow::Result;
use rsmpeg::avutil::AVFrame;
use std::path::Path;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    rsmedia::init()?;

    let source = Path::new("/tmp/test.mp4");

    // 方式一：高层便捷路径，直接拿到原始 AVFrame
    let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_wrapped(source)?;
    let mut raw_count = 0;
    while let Some(frame) = decoder.decode_raw()? {
        println!(
            "[decode_raw] pts={}, {}x{}",
            frame.pts, frame.width, frame.height
        );
        raw_count += 1;
        if raw_count >= 10 {
            break;
        }
    }

    // 方式二：解构出底层 Decoder + Reader，手动逐 packet 送入解码器
    let (mut raw_decoder, mut reader) = decoder.into_parts();
    let stream_index = raw_decoder.stream_index();
    let mut packet_count = 0;

    while let Some((stream, packet)) = reader.read_packet()? {
        // 跳过其它流的 packet
        if stream.index() != stream_index {
            continue;
        }
        if let Some(frame) = raw_decoder.decode_raw_packet(&packet)? {
            print_frame("packet", &frame);
            packet_count += 1;
            if packet_count >= 10 {
                break;
            }
        }
    }

    // 方式三：解码器排空 —— 取出缓冲在解码器内部、未随 packet 输出的帧
    while let Some(frame) = raw_decoder.drain_raw()? {
        print_frame("drain", &frame);
    }

    println!("decoded {raw_count} raw frames, {packet_count} via packets");
    Ok(())
}

fn print_frame(tag: &str, frame: &AVFrame) {
    println!(
        "[{tag}] pts={}, format={}, {}x{}",
        frame.pts, frame.format, frame.width, frame.height
    );
}
