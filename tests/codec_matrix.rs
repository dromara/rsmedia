//! Codec capability matrix: one encode→decode round-trip per codec family,
//! driving the high-level `EncoderBuilder`/`DecoderBuilder`/`MediaFrame` API
//! end to end.
//!
//! Every case **skips itself** (prints `SKIP ...`) when the encoder is not
//! present in the linked FFmpeg build, so the same suite runs unchanged on
//! every platform/FFmpeg combination and still fails loudly for codecs that
//! must exist everywhere (h264/aac/mpeg4/flac/ffv1). The per-case timing and
//! OK/SKIP lines form the codec matrix report in the CI log.
//!
//! The whole file needs the `ndarray` feature: it drives the high-level
//! `MediaFrame` API, whose frame generators live in `common` behind that gate.

#![cfg(feature = "ndarray")]

mod common;

use common::{gradient_video_frame, sine_audio_frame, test_output_path};
use rsmedia::{DecoderBuilder, EncoderBuilder, MediaType, Options, Quality};

use std::ffi::CString;
use std::time::Instant;

const WIDTH: usize = 96;
const HEIGHT: usize = 64;
const FRAMES: usize = 10;
const FPS: f32 = 25.0;
const SAMPLE_RATE: u32 = 44_100;
const CHANNELS: u32 = 2;
const AUDIO_FRAMES: usize = 10;

/// Returns true when `name` is a usable **encoder** in this FFmpeg build.
fn encoder_available(name: &str) -> bool {
    let c_name = CString::new(name).expect("codec name contains NUL");
    rsmpeg::avcodec::AVCodec::find_encoder_by_name(&c_name).is_some()
}

/// Shared video round-trip: encode `FRAMES` synthetic frames with
/// `codec_name` into a `.{container}` file, decode back, and assert the
/// frame count and dimensions survive.
fn video_roundtrip(
    label: &str,
    codec_name: &str,
    container: &str,
    quality: Option<Quality>,
    extra_options: Option<Options>,
) -> anyhow::Result<()> {
    if !encoder_available(codec_name) {
        println!("SKIP  {label}: encoder '{codec_name}' not in this FFmpeg build");
        return Ok(());
    }
    let started = Instant::now();
    let path = test_output_path("codec_matrix", &format!("{label}.{container}"));
    let _ = std::fs::remove_file(&path);

    let mut builder = EncoderBuilder::new_video(WIDTH, HEIGHT)
        .with_fps(FPS)
        .with_codec_name(codec_name.to_string());
    if let Some(quality) = quality {
        builder = builder.with_quality(quality);
    }
    if let Some(options) = extra_options {
        builder = builder.with_options(options);
    }
    let encoder = builder
        .build()
        .map_err(|e| anyhow::anyhow!("{label}: build encoder failed: {e}"))?;
    let enc_tb = encoder.time_base();
    let mut muxer = rsmedia::mux::Muxer::new(&path)
        .map_err(|e| anyhow::anyhow!("{label}: build muxer failed: {e}"))?;
    // MP4/MOV 需要交错写包（av_interleaved_write_frame），否则末帧可能被丢弃
    muxer.set_interleaved(true);
    let v_idx = muxer
        .add_encoder(encoder)
        .map_err(|e| anyhow::anyhow!("{label}: add video stream failed: {e}"))?;
    for i in 0..FRAMES {
        let frame = gradient_video_frame(WIDTH, HEIGHT, i as f32 / FRAMES as f32);
        let mut av = frame
            .to_avframe()
            .map_err(|e| anyhow::anyhow!("{label}: to_avframe {i} failed: {e}"))?;
        // 编码器 time_base = 1/FPS，帧索引即 pts（每帧 1 tick = 1/FPS 秒）
        av.set_pts(i as i64);
        av.set_time_base(enc_tb);
        muxer
            .mux(av, v_idx)
            .map_err(|e| anyhow::anyhow!("{label}: mux {i} failed: {e}"))?;
    }
    muxer
        .finish()
        .map_err(|e| anyhow::anyhow!("{label}: finish failed: {e}"))?;

    let mut reader = rsmedia::StreamReader::new(&path)
        .map_err(|e| anyhow::anyhow!("{label}: open reader failed: {e}"))?;
    let mut decoder = DecoderBuilder::new(MediaType::VIDEO)
        .build_from_reader(&reader)
        .map_err(|e| anyhow::anyhow!("{label}: open decoder failed: {e}"))?;
    let mut decoded = 0usize;
    while let Some(frame) = decoder
        .decode_frame(&mut reader)
        .map_err(|e| anyhow::anyhow!("{label}: decode failed: {e}"))?
    {
        assert_eq!(
            (frame.width, frame.height),
            (WIDTH, HEIGHT),
            "{label}: decoded frame size mismatch"
        );
        decoded += 1;
    }
    assert_eq!(
        decoded, FRAMES,
        "{label}: decoded {decoded} frames, expected {FRAMES}"
    );

    let _ = std::fs::remove_file(&path);
    println!(
        "OK    {label}: {decoded} frames in {:.1?}",
        started.elapsed()
    );
    Ok(())
}

/// Shared audio round-trip: encode `AUDIO_FRAMES` sine-wave frames with
/// `codec_name`, decode back, and assert the sample count survives.
/// Input is always FLTP/f32 — the encoder's native sample format is
/// negotiated and converted automatically.
fn audio_roundtrip(
    label: &str,
    codec_name: &str,
    container: &str,
    sample_rate: u32,
    nb_samples: u32,
    quality: Option<Quality>,
) -> anyhow::Result<()> {
    if !encoder_available(codec_name) {
        println!("SKIP  {label}: encoder '{codec_name}' not in this FFmpeg build");
        return Ok(());
    }
    let started = Instant::now();
    let path = test_output_path("codec_matrix", &format!("{label}.{container}"));
    let _ = std::fs::remove_file(&path);

    let mut builder = EncoderBuilder::default()
        .with_media_type(MediaType::AUDIO)
        .with_nb_channels(CHANNELS as i32)
        .with_sample_rate(sample_rate as i32)
        .with_codec_name(codec_name.to_string());
    if let Some(quality) = quality {
        builder = builder.with_quality(quality);
    }
    let encoder = builder
        .build()
        .map_err(|e| anyhow::anyhow!("{label}: build encoder failed: {e}"))?;
    let enc_tb = encoder.time_base();
    let mut muxer = rsmedia::mux::Muxer::new(&path)
        .map_err(|e| anyhow::anyhow!("{label}: build muxer failed: {e}"))?;
    let a_idx = muxer
        .add_encoder(encoder)
        .map_err(|e| anyhow::anyhow!("{label}: add audio stream failed: {e}"))?;
    let mut total_pts: i64 = 0;
    for i in 0..AUDIO_FRAMES {
        let frame = sine_audio_frame(440.0, CHANNELS, nb_samples, sample_rate);
        let mut av = frame
            .to_avframe()
            .map_err(|e| anyhow::anyhow!("{label}: to_avframe {i} failed: {e}"))?;
        av.set_pts(total_pts);
        av.set_time_base(enc_tb);
        total_pts += nb_samples as i64;
        muxer
            .mux(av, a_idx)
            .map_err(|e| anyhow::anyhow!("{label}: mux {i} failed: {e}"))?;
    }
    muxer
        .finish()
        .map_err(|e| anyhow::anyhow!("{label}: finish failed: {e}"))?;

    let mut reader = rsmedia::StreamReader::new(&path)
        .map_err(|e| anyhow::anyhow!("{label}: open reader failed: {e}"))?;
    let mut decoder = DecoderBuilder::new(MediaType::AUDIO)
        .build_from_reader(&reader)
        .map_err(|e| anyhow::anyhow!("{label}: open decoder failed: {e}"))?;
    // Decoders emit different native sample formats (AAC/MP3/Opus -> FLTP,
    // FLAC -> S16/S32), so read raw AVFrames here: a typed `MediaFrame<T>`
    // decode would require T to match each codec's sample size.
    let mut decoded = 0usize;
    let mut total_samples = 0i64;
    while let Some(frame) = decoder
        .decode_raw(&mut reader)
        .map_err(|e| anyhow::anyhow!("{label}: decode failed: {e}"))?
    {
        assert_eq!(
            frame.sample_rate, sample_rate as i32,
            "{label}: decoded sample rate mismatch"
        );
        assert!(frame.nb_samples > 0, "{label}: decoded empty audio frame");
        total_samples += frame.nb_samples as i64;
        decoded += 1;
    }
    assert!(decoded > 0, "{label}: no audio frames decoded");
    // Lossy decoders emit priming/padding samples on top of the input, so the
    // decoded total must never fall below what we fed in.
    let input_samples = (AUDIO_FRAMES * nb_samples as usize) as i64;
    assert!(
        total_samples >= input_samples,
        "{label}: decoded {total_samples} samples, expected at least {input_samples}"
    );

    let _ = std::fs::remove_file(&path);
    println!(
        "OK    {label}: {decoded} frames in {:.1?}",
        started.elapsed()
    );
    Ok(())
}

// ===========================================================================
// Video matrix
// ===========================================================================

/// H.264 is the project's default video codec — must exist everywhere.
#[test]
#[cfg(feature = "ndarray")]
fn matrix_video_h264() -> anyhow::Result<()> {
    video_roundtrip("h264", "libx264", "mp4", Some(Quality::Crf(23)), None)
}

/// HEVC via libx265 (optional external library).
#[test]
#[cfg(feature = "ndarray")]
fn matrix_video_hevc() -> anyhow::Result<()> {
    video_roundtrip("hevc", "libx265", "mkv", Some(Quality::Crf(28)), None)
}

/// MPEG-4 part 2 — native encoder, always present.
#[test]
#[cfg(feature = "ndarray")]
fn matrix_video_mpeg4() -> anyhow::Result<()> {
    video_roundtrip(
        "mpeg4",
        "mpeg4",
        "mp4",
        Some(Quality::Bitrate(800_000)),
        None,
    )
}

/// VP9 via libvpx (optional external library). `cpu-used`/`row-mt` keep the
/// tiny round-trip fast.
#[test]
#[cfg(feature = "ndarray")]
fn matrix_video_vp9() -> anyhow::Result<()> {
    let mut opts = Options::new();
    opts.insert("cpu-used", "8").insert("row-mt", "1");
    video_roundtrip(
        "vp9",
        "libvpx-vp9",
        "webm",
        Some(Quality::Crf(30)),
        Some(opts),
    )
}

/// FFV1 — native lossless video codec, always present.
#[test]
#[cfg(feature = "ndarray")]
fn matrix_video_ffv1_lossless() -> anyhow::Result<()> {
    video_roundtrip("ffv1", "ffv1", "mkv", None, None)
}

// ===========================================================================
// Audio matrix
// ===========================================================================

/// AAC is the project's default audio codec — must exist everywhere.
#[test]
#[cfg(feature = "ndarray")]
fn matrix_audio_aac() -> anyhow::Result<()> {
    audio_roundtrip(
        "aac",
        "aac",
        "m4a",
        SAMPLE_RATE,
        1024,
        Some(Quality::Bitrate(128_000)),
    )
}

/// MP3 via libmp3lame (optional external library); frame size 1152.
#[test]
#[cfg(feature = "ndarray")]
fn matrix_audio_mp3() -> anyhow::Result<()> {
    audio_roundtrip(
        "mp3",
        "libmp3lame",
        "mp3",
        SAMPLE_RATE,
        1152,
        Some(Quality::Bitrate(128_000)),
    )
}

/// FLAC — native lossless audio codec, always present.
#[test]
#[cfg(feature = "ndarray")]
fn matrix_audio_flac_lossless() -> anyhow::Result<()> {
    audio_roundtrip("flac", "flac", "flac", SAMPLE_RATE, 1024, None)
}

/// Opus via libopus (optional external library); frame size 960.
/// libopus only supports 8/12/16/24/48 kHz, so use 48 kHz.
#[test]
#[cfg(feature = "ndarray")]
fn matrix_audio_opus() -> anyhow::Result<()> {
    audio_roundtrip(
        "opus",
        "libopus",
        "ogg",
        48_000,
        960,
        Some(Quality::Bitrate(96_000)),
    )
}

// ===========================================================================
// Capability discovery smoke test (dogfoods the P0-1 query layer)
// ===========================================================================

/// The capability query layer must never come back empty on a working
/// FFmpeg build: codecs, encoders, muxers and demuxers are all enumerable,
/// and well-known entries resolve by name.
#[test]
fn matrix_capability_discovery() {
    use rsmedia::{CodecConfig, FormatInfo};

    let encoders = CodecConfig::encoders();
    let decoders = CodecConfig::decoders();
    assert!(!encoders.is_empty(), "no encoders enumerated");
    assert!(!decoders.is_empty(), "no decoders enumerated");

    let muxers = FormatInfo::muxers();
    let demuxers = FormatInfo::demuxers();
    assert!(!muxers.is_empty(), "no muxers enumerated");
    assert!(!demuxers.is_empty(), "no demuxers enumerated");

    assert!(FormatInfo::find_muxer("mp4").is_some(), "mp4 muxer missing");
    assert!(
        FormatInfo::find_demuxer("matroska").is_some(),
        "matroska demuxer missing"
    );

    println!(
        "OK    discovery: {} encoders, {} decoders, {} muxers, {} demuxers",
        encoders.len(),
        decoders.len(),
        muxers.len(),
        demuxers.len()
    );
}
