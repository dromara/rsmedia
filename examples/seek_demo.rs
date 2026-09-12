//! Demonstrates the seek capability of a reader.
//!
//! The same `StreamReader` + `Seekable` code works for both a local mp4 and an
//! http/rtsp source: whether a seek succeeds depends on the **runtime source**,
//! so every call reports failure through `Result`, and
//! `Seekable::is_byte_seekable` gives a cheap pre-check of the IO layer.
//!
//! APIs covered:
//! - `Seekable::is_byte_seekable` — cheap pre-check for seek support
//! - `Seekable::seek_to_frame` — seek to a frame number
//! - `Seekable::seek_to_timestamp` — seek to a timestamp (milliseconds)
//! - `Seekable::seek_to_start` — rewind to the beginning
//!
//! Usage: `cargo run --example seek [local path | http(s)/rtsp URL]`
//! Defaults to `assets/mp4.mp4` when the argument is omitted.

use rsmedia::io::{AVSeekFlag, Seekable};
use rsmedia::{Decoder, DecoderBuilder, MediaType, Reader, StreamReader};

use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    rsmedia::init()?;

    let arg = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "assets/mp4.mp4".to_string());
    let mut reader = StreamReader::new(arg.clone())?;

    // Cheap pre-check of the IO layer: true for local files, in-memory data and
    // HTTP servers honouring `Accept-Ranges`; false for live streams / pipes /
    // most RTSP (whose demuxer may still seek by timestamp — opaque in FFmpeg
    // 5.0+, so a `false` here does not mean seek_to_timestamp fails).
    if reader.is_byte_seekable() {
        println!("reader reports byte-seekable: {arg}");
    } else {
        println!("reader does not report byte-seekable; seek may fail or be imprecise: {arg}");
    }

    let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
    let stream_index = decoder.stream_index();

    // 1. Seek to frame 3 (by frame number, with AVSEEK_FLAG_FRAME).
    //    Frame-precise seeking is source-dependent: MP4/AVC demuxers do not
    //    support it, so `seek_to_frame` legitimately reports failure here while
    //    raw formats (e.g. .h264) accept it. Treat it as fallible, not a panic.
    match reader.seek_to_frame(stream_index, 3, AVSeekFlag::FRAME) {
        Ok(()) => {
            println!(
                "seek_to_frame(3) -> pts={}",
                next_pts(&mut decoder, &mut reader)?
            );
        }
        Err(e) => {
            println!("seek_to_frame(3) unsupported by this source: {e}");
        }
    }

    // 2. Seek to 10s (by timestamp, landing on the nearest keyframe).
    reader.seek_to_timestamp(10_000)?;
    println!(
        "seek_to_timestamp(10_000ms) -> pts={}",
        next_pts(&mut decoder, &mut reader)?
    );

    // 3. Rewind to the beginning.
    reader.seek_to_start()?;
    println!(
        "seek_to_start() -> pts={}",
        next_pts(&mut decoder, &mut reader)?
    );

    Ok(())
}

/// Reads until the next frame is decoded and returns its PTS.
fn next_pts<R>(decoder: &mut Decoder, reader: &mut R) -> Result<i64>
where
    R: Reader,
{
    for _ in 0..100 {
        if let Some(frame) = decoder.decode_frame(reader)? {
            return Ok(frame.pts);
        }
    }
    Err(anyhow::anyhow!("no frame after seek"))
}
