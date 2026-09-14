//! Common-container round-trip matrix.
//!
//! For every widely used container this encodes the stream set the container
//! supports — video, audio, subtitles — writes it to a local file, then reads
//! that file back with the demuxer and the decoders and checks what came out:
//!
//! * **structure** — which streams the container ended up carrying, and under
//!   which codec (`codecpar`);
//! * **video** — decoded frame count and geometry, on non-blank pixels;
//! * **audio** — decoded sample rate, channel count and sample total;
//! * **subtitles** — decoded text and timing, verbatim.
//!
//! Containers whose codec is missing from the linked FFmpeg build report
//! `SKIP <container>: ...` instead of failing, so one suite runs on every
//! platform; at least one container must round-trip, so an environment where
//! nothing works cannot pass silently.
//!
//! Requires the `ndarray` feature (frames come from the high-level API).

#![cfg(feature = "ndarray")]

mod common;

use std::path::Path;

use rsmedia::fmt::AVFormatFlag;
use rsmedia::strutils;
use rsmedia::{
    CodecConfig, DecoderBuilder, ElementType, EncoderBuilder, MediaType, Muxer, Reader, Result,
    RsmediaError, SampleFormat, StreamReader, SubtitleSegment,
};
use rsmpeg::avcodec::AVCodec;
use rsmpeg::avutil::AVMediaType;
use rsmpeg::ffi;

const WIDTH: usize = 320;
const HEIGHT: usize = 240;
const FPS: f32 = 25.0;
/// 0.4 s of video — long enough to decode a real sequence, short enough to keep
/// the whole matrix fast.
const VIDEO_FRAMES: i64 = 10;
/// Audio length to aim for, whatever the codec's frame size: codecs differ a
/// lot (AAC 1024 samples, FLAC 4608), so counting frames would give a 0.23 s
/// file for one container and a 1.05 s file for the next.
const AUDIO_SECONDS: f64 = 0.4;
const CHANNELS: u32 = 2;
/// Both cues live inside the video's duration.
const CUES: [(i64, i64, &str); 2] = [
    (0, 150, "first cue"),
    (200, 350, "second cue, with, commas"),
];
/// Subtitle encoders refuse to open without a complete ASS script header.
const SUBTITLE_HEADER: &str = "[Script Info]\nScriptType: v4.00+\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,16,&Hffffff,&Hffffff,&H0,&H0,0,0,0,0,100,100,0,0,1,1,0,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";

/// One row of the matrix: a container plus the encoders to write it with.
struct ContainerSpec {
    /// Extension, which also picks the muxer.
    name: &'static str,
    /// Encoder names; `None` = the container cannot carry that stream kind.
    video: Option<&'static str>,
    audio: Option<&'static str>,
    subtitle: Option<&'static str>,
    /// Audio sample rate: the Opus and AC-3 families are fixed at 48 kHz.
    sample_rate: u32,
    /// Raw elementary stream: the output format has no global-header concept, so
    /// the encoder must keep its parameter sets in-band. The builder *guesses*
    /// `AV_CODEC_FLAG_GLOBAL_HEADER` by default (right for containers, wrong
    /// here), so the matrix says so explicitly — see REVIEW.md 5.10.
    raw: bool,
}

const fn spec(
    name: &'static str,
    video: Option<&'static str>,
    audio: Option<&'static str>,
    subtitle: Option<&'static str>,
    sample_rate: u32,
) -> ContainerSpec {
    ContainerSpec {
        name,
        video,
        audio,
        subtitle,
        sample_rate,
        raw: false,
    }
}

/// A bare elementary stream (`.h264`, `.h265`): video only, no global header.
const fn raw_spec(name: &'static str, video: &'static str) -> ContainerSpec {
    ContainerSpec {
        name,
        video: Some(video),
        audio: None,
        subtitle: None,
        sample_rate: 0,
        raw: true,
    }
}

/// The commonly used containers, by the stream sets they accept.
const CONTAINERS: &[ContainerSpec] = &[
    // Video + audio + subtitles (subtitle muxing is container-specific).
    spec(
        "mp4",
        Some("libx264"),
        Some("aac"),
        Some("mov_text"),
        44_100,
    ),
    spec(
        "mov",
        Some("libx264"),
        Some("aac"),
        Some("mov_text"),
        44_100,
    ),
    spec("mkv", Some("libx264"), Some("aac"), Some("ass"), 44_100),
    // Video + audio.
    spec("webm", Some("libvpx-vp9"), Some("libopus"), None, 48_000),
    spec("ts", Some("libx264"), Some("aac"), None, 44_100),
    spec("flv", Some("libx264"), Some("aac"), None, 44_100),
    spec("avi", Some("libx264"), Some("aac"), None, 44_100),
    // MPEG-PS (VCD/DVD-era): mpeg2video + mp2 is its native pairing -- it only
    // accepts mp1/mp2/mp3/pcm_dvd/pcm_s16be/ac3/dts audio.
    spec("mpg", Some("mpeg2video"), Some("mp2"), None, 44_100),
    spec("3gp", Some("libx264"), Some("aac"), None, 44_100),
    // Video only: animation, the lossless intermediate, and raw elementary
    // streams (no container at all -- their `codecpar` carries no pixel format,
    // so they also pin that a formatless stream can still be opened).
    spec("gif", Some("gif"), None, None, 0),
    spec("y4m", Some("rawvideo"), None, None, 0),
    raw_spec("h264", "libx264"),
    raw_spec("h265", "libx265"),
    // Audio only: lossy, lossless, and uncompressed PCM.
    spec("m4a", None, Some("aac"), None, 44_100),
    spec("aac", None, Some("aac"), None, 44_100),
    spec("mp3", None, Some("libmp3lame"), None, 44_100),
    spec("ogg", None, Some("libopus"), None, 48_000),
    spec("opus", None, Some("libopus"), None, 48_000),
    spec("ac3", None, Some("ac3"), None, 48_000),
    spec("wma", None, Some("wmav2"), None, 44_100),
    spec("flac", None, Some("flac"), None, 44_100),
    spec("wav", None, Some("pcm_s16le"), None, 44_100),
    spec("caf", None, Some("pcm_s16le"), None, 44_100),
];

/// The codec each encoder writes, so the round trip can assert that the
/// container really carries it (`codecpar().codec_id`).
fn codec_id(encoder: &str) -> ffi::AVCodecID {
    match encoder {
        "libx264" => ffi::AV_CODEC_ID_H264,
        "libx265" => ffi::AV_CODEC_ID_HEVC,
        "libvpx-vp9" => ffi::AV_CODEC_ID_VP9,
        "mpeg2video" => ffi::AV_CODEC_ID_MPEG2VIDEO,
        "gif" => ffi::AV_CODEC_ID_GIF,
        "rawvideo" => ffi::AV_CODEC_ID_RAWVIDEO,
        "aac" => ffi::AV_CODEC_ID_AAC,
        "libmp3lame" => ffi::AV_CODEC_ID_MP3,
        "mp2" => ffi::AV_CODEC_ID_MP2,
        "libopus" => ffi::AV_CODEC_ID_OPUS,
        "ac3" => ffi::AV_CODEC_ID_AC3,
        "wmav2" => ffi::AV_CODEC_ID_WMAV2,
        "flac" => ffi::AV_CODEC_ID_FLAC,
        "pcm_s16le" => ffi::AV_CODEC_ID_PCM_S16LE,
        "mov_text" => ffi::AV_CODEC_ID_MOV_TEXT,
        "ass" => ffi::AV_CODEC_ID_ASS,
        other => panic!("no codec id mapped for encoder `{other}`"),
    }
}

/// Picks the sample format and rate the encoder really supports.
///
/// The sample format is a *codec* property, not a container one: AAC wants FLTP,
/// Opus s16/flt, MP2 s32p. Frames are handed over as FLTP regardless and the
/// encoder converts them, but its own format has to be requested correctly or
/// the builder rejects the configuration. The rate prefers the one the matrix
/// asks for and falls back to the codec's list (Opus is fixed at 48 kHz).
fn negotiate_audio(codec: &str, preferred_rate: u32) -> Result<(SampleFormat, u32)> {
    let Some(encoder) = AVCodec::find_encoder_by_name(&strutils::str_to_cstring(codec)) else {
        return Err(RsmediaError::codec_not_found(format!(
            "encoder {codec} not available in this FFmpeg build"
        )));
    };
    let config = CodecConfig::from_codec(encoder);

    let sample_format = config
        .supported_sample_formats()?
        .and_then(|formats| formats.first().copied())
        .map(SampleFormat::from)
        .ok_or_else(|| {
            RsmediaError::custom(format!("encoder {codec} has no supported sample format"))
        })?;

    let sample_rate = match config.supported_sample_rates()? {
        Some(rates) if !rates.is_empty() && !rates.contains(&(preferred_rate as i32)) => {
            rates[0] as u32
        }
        _ => preferred_rate,
    };

    Ok((sample_format, sample_rate))
}

/// What was written, so the decoder side can assert against it.
struct Written {
    /// Samples per audio frame as handed to the encoder (fixed-frame-size
    /// codecs re-chunk; lossy ones add padding, hence the tolerance below).
    frame_samples: u32,
    audio_samples: u64,
    sample_rate: u32,
    channels: u32,
}

/// Decodes the audio stream into its **native** element type, returning
/// `(samples, peak)` with the peak normalised to ±1.
///
/// The decoder leaves audio sample formats alone unless asked otherwise, so the
/// element type here has to match what the codec decodes to, which
/// `codecpar().format` announces: AAC and AC-3 come out as `fltp`, MP2 as
/// `s16p`. [`audio_summary_unified`] covers the other route — asking the decoder
/// to unify the output, like `with_pix_fmt` does for video.
fn audio_summary(path: &Path, written: &Written) -> Result<(u64, f32)> {
    let reader = StreamReader::new(path)?;
    let native = reader
        .input()
        .streams()
        .iter()
        .find(|stream| stream.codecpar().codec_type().is_audio())
        .map(|stream| SampleFormat::from(stream.codecpar().format))
        .expect("the audio stream was verified present");

    match native {
        SampleFormat::U8 | SampleFormat::U8P => summarize::<u8>(path, written, 128.0, None),
        SampleFormat::S16 | SampleFormat::S16P => {
            summarize::<i16>(path, written, i16::MAX as f32, None)
        }
        SampleFormat::S32 | SampleFormat::S32P => {
            summarize::<i32>(path, written, i32::MAX as f32, None)
        }
        SampleFormat::FLT | SampleFormat::FLTP => summarize::<f32>(path, written, 1.0, None),
        other => Err(RsmediaError::custom(format!(
            "the matrix has no element type for decoded audio format {other:?}"
        ))),
    }
}

/// Decodes the audio stream with the output unified to `FLTP`.
///
/// Whatever the codec decodes to natively, `with_sample_fmt` makes the frames
/// come out as `FLTP`, so `decode::<f32>` works without inspecting
/// `codecpar().format` first — that is the point of the option.
fn audio_summary_unified(path: &Path, written: &Written) -> Result<(u64, f32)> {
    summarize::<f32>(path, written, 1.0, Some(SampleFormat::FLTP))
}

/// Decodes with `element`, optionally asking the decoder for `sample_fmt`.
fn summarize<T: ElementType>(
    path: &Path,
    written: &Written,
    full_scale: f32,
    sample_fmt: Option<SampleFormat>,
) -> Result<(u64, f32)> {
    let mut reader = StreamReader::new(path)?;
    let mut builder = DecoderBuilder::new(MediaType::AUDIO);
    if let Some(sample_fmt) = sample_fmt {
        builder = builder.with_sample_fmt(sample_fmt);
    }
    let mut decoder = builder.build_from_reader(&reader)?;
    let mut samples = 0u64;
    let mut peak = 0f32;
    while let Some(frame) = decoder.decode::<T>(&mut reader)? {
        assert_eq!(
            frame.sample_rate, written.sample_rate,
            "decoded sample rate"
        );
        assert_eq!(frame.nb_channels, written.channels, "decoded channel count");
        if let Some(sample_fmt) = sample_fmt {
            assert_eq!(
                frame.format().and_then(|f| f.into_sample()),
                Some(sample_fmt),
                "decoded frame was not converted to the requested sample format"
            );
        }
        let plane = frame.data.plane(0).expect("decoded audio has a plane");
        for value in plane.iter() {
            let magnitude = num_traits::cast::<T, f32>(*value).unwrap_or(0.0).abs() / full_scale;
            peak = peak.max(magnitude);
        }
        samples += frame.nb_samples as u64;
    }
    Ok((samples, peak))
}

/// Encodes the stream set `spec` supports into `path`.
fn write_container(path: &Path, spec: &ContainerSpec) -> Result<Written> {
    let mut muxer = Muxer::new(path)?;

    let video_index = match spec.video {
        Some(codec) => {
            // A raw elementary stream has no global header, so the encoder has to
            // keep its parameter sets (SPS/PPS) in-band; NOTIMESTAMPS is the flag
            // the raw muxer itself declares, and it implies "no global header".
            let mut builder = EncoderBuilder::new_video(WIDTH, HEIGHT).with_fps(FPS);
            if spec.raw {
                builder = builder.with_oformat_flags(AVFormatFlag::NO_TIMESTAMPS);
            }
            let encoder = builder.with_codec_name(Some(codec.to_string())).build()?;
            Some(muxer.add_encoder(encoder)?)
        }
        None => None,
    };

    let (audio_index, frame_samples, sample_rate) = match spec.audio {
        Some(codec) => {
            let (sample_format, sample_rate) = negotiate_audio(codec, spec.sample_rate)?;
            let encoder = EncoderBuilder::new_audio(
                128_000,
                CHANNELS as i32,
                sample_rate as i32,
                sample_format,
            )
            .with_codec_name(Some(codec.to_string()))
            .build()?;
            // Fixed-frame-size codecs (AAC 1024, Opus 960, MP3 1152) want exactly
            // this many samples per frame; the rest take whole frames as they come.
            let frame_size = encoder.frame_size();
            let frame_samples = if frame_size > 0 {
                frame_size as u32
            } else {
                1024
            };
            (
                Some(muxer.add_encoder(encoder)?),
                frame_samples,
                sample_rate,
            )
        }
        None => (None, 0, 0),
    };

    let subtitle_index = match spec.subtitle {
        Some(codec) => {
            let encoder = EncoderBuilder::new_subtitle()
                .with_codec_name(Some(codec.to_string()))
                .with_subtitle_header(SUBTITLE_HEADER)
                .build()?;
            Some(muxer.add_encoder(encoder)?)
        }
        None => None,
    };

    let audio_frames = if frame_samples > 0 {
        ((AUDIO_SECONDS * sample_rate as f64) as u64).div_ceil(frame_samples as u64)
    } else {
        0
    };

    // Video and audio are interleaved so the muxer sees a monotonic timeline.
    let mut audio_written = 0u64;
    for frame_index in 0..VIDEO_FRAMES {
        if let Some(index) = video_index {
            let frame = common::gradient_video_frame(
                WIDTH,
                HEIGHT,
                frame_index as f32 / VIDEO_FRAMES as f32,
            );
            muxer.mux(frame.to_avframe()?, index)?;
        }

        if let Some(index) = audio_index {
            let target = ((frame_index + 1) as f64 / FPS as f64 * sample_rate as f64) as u64;
            while audio_written + frame_samples as u64 <= target {
                let audio = common::sine_audio_frame(440.0, CHANNELS, frame_samples, sample_rate);
                muxer.mux(audio.to_avframe()?, index)?;
                audio_written += frame_samples as u64;
            }
        }
    }
    // Top the audio up to the full frame count when video did not already.
    if let Some(index) = audio_index {
        while audio_written < audio_frames * frame_samples as u64 {
            let audio = common::sine_audio_frame(440.0, CHANNELS, frame_samples, sample_rate);
            muxer.mux(audio.to_avframe()?, index)?;
            audio_written += frame_samples as u64;
        }
    }

    if let Some(index) = subtitle_index {
        for (start_ms, end_ms, text) in CUES {
            muxer.mux_subtitle_segment(&SubtitleSegment::new(start_ms, end_ms, text), index)?;
        }
    }

    muxer.finish()?;

    Ok(Written {
        frame_samples,
        audio_samples: audio_written,
        sample_rate,
        channels: CHANNELS,
    })
}

/// Reads `path` back and asserts it matches `spec` / `written`.
fn verify_container(path: &Path, spec: &ContainerSpec, written: &Written) -> Result<()> {
    // ---- structure: the container carries exactly the streams that were written ----
    let reader = StreamReader::new(path)?;
    let streams = reader.input().streams();
    let of_type = |is_kind: fn(&AVMediaType) -> bool| {
        streams
            .iter()
            .filter(|stream| is_kind(&stream.codecpar().codec_type()))
            .count()
    };
    let expected_streams = spec.video.is_some() as usize
        + spec.audio.is_some() as usize
        + spec.subtitle.is_some() as usize;
    assert_eq!(
        streams.len(),
        expected_streams,
        "stream count in the container"
    );
    assert_eq!(
        of_type(AVMediaType::is_video),
        spec.video.is_some() as usize
    );
    assert_eq!(
        of_type(AVMediaType::is_audio),
        spec.audio.is_some() as usize
    );
    assert_eq!(
        of_type(AVMediaType::is_subtitle),
        spec.subtitle.is_some() as usize
    );

    for (index, stream) in streams.iter().enumerate() {
        let parameters = stream.codecpar();
        let codec_type = parameters.codec_type();
        let expected = if codec_type.is_video() {
            spec.video.map(codec_id)
        } else if codec_type.is_audio() {
            spec.audio.map(codec_id)
        } else if codec_type.is_subtitle() {
            spec.subtitle.map(codec_id)
        } else {
            None
        };
        if let Some(expected) = expected {
            assert_eq!(
                parameters.codec_id, expected,
                "stream {index} carries the wrong codec"
            );
        }
    }

    // ---- video: every frame comes back, at the right size, with real pixels ----
    if spec.video.is_some() {
        let mut reader = StreamReader::new(path)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
        let mut decoded = 0i64;
        while let Some(frame) = decoder.decode_frame(&mut reader)? {
            assert_eq!(
                (frame.width, frame.height),
                (WIDTH, HEIGHT),
                "frame {decoded} geometry"
            );
            // Layout-agnostic: plane 0 is the luma plane for planar formats and
            // the packed array for interleaved ones.
            let plane = frame.data.plane(0).expect("decoded video has a plane");
            let min = *plane.iter().min().expect("non-empty frame");
            let max = *plane.iter().max().expect("non-empty frame");
            assert!(max - min > 32, "frame {decoded} looks blank: {min}..{max}");
            decoded += 1;
        }
        assert_eq!(decoded, VIDEO_FRAMES, "decoded video frame count");
    }

    // ---- audio: rate and channels survive; the samples are really there ----
    if spec.audio.is_some() {
        let (samples, peak) = audio_summary(path, written)?;
        assert!(
            peak > 0.1,
            "decoded audio looks silent: normalised peak={peak}"
        );
        // The decoded total sits within a frame or two of what was handed in:
        // lossy codecs pad the tail (encoder delay) and the demuxer trims their
        // priming (WMA v2 skips one 2048-sample frame), so both directions need
        // slack — but a lost stream or a dropped tail is off by far more.
        let tolerance = 3 * written.frame_samples as u64;
        assert!(
            samples + tolerance >= written.audio_samples,
            "decoded {samples} samples, {} were written",
            written.audio_samples
        );
        assert!(
            samples <= written.audio_samples + tolerance,
            "decoded {samples} samples for {} written (+{tolerance} allowed)",
            written.audio_samples
        );

        // Asking for a unified output format must not change any of that: only
        // the sample format is converted (FLTP here), so rate, channel count and
        // sample total stay the codec's, and the peak matches up to the scaling
        // factor swr uses (1/32768 rather than 1/i16::MAX).
        let (unified_samples, unified_peak) = audio_summary_unified(path, written)?;
        assert_eq!(
            unified_samples, samples,
            "unified output changed the decoded sample count"
        );
        assert!(
            (unified_peak - peak).abs() < 1e-3,
            "unified output changed the peak: {unified_peak} vs {peak}"
        );
    }

    // ---- subtitles: text and timing are lossless ----
    if let Some(codec) = spec.subtitle {
        let mut reader = StreamReader::new(path)?;
        let mut decoder = DecoderBuilder::new(MediaType::SUBTITLE)
            .with_codec_name(Some(codec.to_string()))
            .build_from_reader(&reader)?;
        let mut decoded = Vec::new();
        while let Some(segment) = decoder.decode_subtitle_segment(&mut reader)? {
            decoded.push(segment);
        }
        assert_eq!(decoded.len(), CUES.len(), "decoded cue count");
        for (got, (start_ms, end_ms, text)) in decoded.iter().zip(CUES) {
            assert_eq!(got.text, text, "cue text must round-trip verbatim");
            assert_eq!((got.start_ms, got.end_ms), (start_ms, end_ms), "cue timing");
        }
    }

    Ok(())
}

/// Encodes and reads back one container, leaving no artifact behind.
fn roundtrip(spec: &ContainerSpec) -> Result<()> {
    let path = common::test_output_path("container_roundtrip", &format!("roundtrip.{}", spec.name));
    common::remove_test_output(&path);

    let written = write_container(&path, spec)?;
    verify_container(&path, spec, &written)?;

    common::remove_test_output(&path);
    Ok(())
}

/// The library reports a codec it cannot find with this phrasing, which is the
/// only condition under which a container is skipped rather than failed.
fn is_encoder_unavailable(error: &RsmediaError) -> bool {
    error
        .to_string()
        .contains("not available in this FFmpeg build")
}

/// Walks the matrix; a codec missing from this FFmpeg build skips its container,
/// anything else fails the suite.
#[test]
fn test_common_containers_roundtrip() {
    let mut passed = Vec::new();
    let mut skipped = Vec::new();
    let mut failed = Vec::new();

    for spec in CONTAINERS {
        match roundtrip(spec) {
            Ok(()) => {
                println!("{} ok", spec.name);
                passed.push(spec.name);
            }
            Err(error) if is_encoder_unavailable(&error) => {
                println!("SKIP {}: {error:#}", spec.name);
                skipped.push(spec.name);
            }
            Err(error) => {
                println!("FAIL {}: {error:#}", spec.name);
                failed.push((spec.name, format!("{error:#}")));
            }
        }
    }

    println!(
        "containers: {} passed {passed:?}, {} skipped {skipped:?}, {} failed",
        passed.len(),
        skipped.len(),
        failed.len()
    );
    assert!(failed.is_empty(), "containers failed: {failed:#?}");
    assert!(
        !passed.is_empty(),
        "no container round-tripped at all — check the FFmpeg build"
    );
}
