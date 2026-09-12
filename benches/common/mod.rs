//! Shared encode workloads for the benchmark targets.
//!
//! Every workload writes its output to `bench_dir()` and the caller is
//! responsible for removing the file ([`remove_file`]).

#![allow(dead_code)]

use anyhow::Result;
use rsmedia::encode::EncoderBuilder;
use rsmedia::frame::MediaFrame;
use rsmedia::io::StreamWriter;
use rsmedia::mux::Muxer;
use rsmedia::pixel::PixelFormat;
use rsmedia::subtitle::{SubtitleSegment, encode_subtitle_segments};
use rsmedia::{MediaType, SampleFormat, time};

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

/// Video job: 2 s of 320x240 @ 30 fps (60 frames, pts left unset so the
/// encoder's automatic numbering is exercised).
pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 240;
pub const FPS: f32 = 30.0;
pub const VIDEO_FRAMES: usize = 60;
pub const VIDEO_MEDIA_SECS: f64 = 2.0;

/// Audio job: ~3 s of 44.1 kHz stereo AAC with variable input frame sizes, so
/// the encoder's sample FIFO has to split and merge.
pub const SAMPLE_RATE: u32 = 44_100;
pub const CHANNELS: u32 = 2;
pub const AUDIO_FRAMES: usize = 100;
pub const AUDIO_SAMPLES_MAX: u32 = 1200;
pub const AUDIO_MEDIA_SECS: f64 = 3.0;

/// Subtitle job: 30 mov_text segments, one cue every 2 s (60 s of media).
pub const SEGMENTS_PER_JOB: usize = 30;
pub const SUBTITLE_MEDIA_SECS: f64 = 3.0;

/// Jobs of each kind per pool batch in `encode_pipeline`.
pub const JOBS_PER_KIND: usize = 4;

pub const ASS_HEADER: &str = "[Script Info]\n\
     ScriptType: v4.00+\n\
     \n\
     [V4+ Styles]\n\
     Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, \
     OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, \
     ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, \
     Alignment, MarginL, MarginR, MarginV, Encoding\n\
     Style: Default,Arial,16,&Hffffff,&Hffffff,&H0,&H0,0,0,0,0,100,100,0,0,1,1,0,2,10,10,10,1\n\
     \n\
     [Events]\n\
     Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";

/// Media duration encoded by one job of `kind` (for `Throughput::Elements`).
pub fn media_secs(kind: MediaType) -> f64 {
    match kind {
        MediaType::VIDEO => VIDEO_MEDIA_SECS,
        MediaType::AUDIO => AUDIO_MEDIA_SECS,
        MediaType::SUBTITLE => SUBTITLE_MEDIA_SECS,
        _ => 0.0, // DATA / unknown: no media duration
    }
}

/// Logical core count: the benchmark's parallelism knob.
pub fn cores() -> usize {
    thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or_else(|_| num_cpus::get())
}

/// Writable directory shared by all bench targets.
pub fn bench_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("rsmedia_bench");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Unique output path per call, so concurrent pool workers never share a file.
/// MP4 is used for every kind: it accepts H.264, AAC and mov_text, and lets the
/// muxer infer the format from the extension.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    dir.join(format!("{name}_{n:05}.mp4"))
}

pub fn remove_file(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// One encode job of `kind` written to `path`; `enc_threads` is forwarded to
/// [`EncoderBuilder::with_thread_count`].
pub fn run_job(kind: MediaType, path: &Path, enc_threads: usize) -> Result<()> {
    match kind {
        MediaType::VIDEO => encode_video(path, enc_threads),
        MediaType::AUDIO => encode_audio(path, enc_threads),
        MediaType::SUBTITLE => encode_subtitle(path, enc_threads),
        _ => Err(anyhow::anyhow!("no encoder workload for {kind:?}")),
    }
}

/// Drain `jobs` through a fixed pool of `workers` threads, every encoder
/// running with `enc_threads` threads. The two knobs trade off against each
/// other: their product must not exceed the core count.
pub fn run_pool(jobs: &[(MediaType, PathBuf)], workers: usize, enc_threads: usize) -> Result<()> {
    let queue = Mutex::new(jobs.iter());
    let error = Mutex::new(None);

    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let job = {
                        let mut queue = queue.lock().unwrap();
                        queue.next().cloned()
                    };
                    let Some((kind, path)) = job else { break };
                    if let Err(e) = run_job(kind, &path, enc_threads) {
                        error.lock().unwrap().get_or_insert(e.to_string());
                    }
                    remove_file(&path);
                }
            });
        }
    });

    match error.into_inner().unwrap() {
        Some(e) => Err(anyhow::anyhow!(e)),
        None => Ok(()),
    }
}

pub fn encode_video(path: &Path, enc_threads: usize) -> Result<()> {
    let encoder = EncoderBuilder::new_video(WIDTH, HEIGHT)
        .with_fps(FPS)
        .with_thread_count(enc_threads)
        .build()?;
    let mut muxer = Muxer::new(path)?;
    let v_idx = muxer.add_encoder(encoder)?;

    for i in 0..VIDEO_FRAMES {
        // pts left unset on purpose: the encoder numbers frames automatically.
        let frame = rainbow_frame(i as f32 / VIDEO_FRAMES as f32);
        muxer.mux(frame.to_avframe()?, v_idx)?;
    }
    muxer.finish()?;
    Ok(())
}

pub fn encode_audio(path: &Path, enc_threads: usize) -> Result<()> {
    let encoder = EncoderBuilder::new_audio(
        128_000,
        CHANNELS as i32,
        SAMPLE_RATE as i32,
        SampleFormat::FLTP,
    )
    .with_thread_count(enc_threads)
    .build()?;
    let mut muxer = Muxer::new(path)?;
    let a_idx = muxer.add_encoder(encoder)?;

    // Variable input sizes on purpose: the encoder's sample FIFO splits and
    // merges them into fixed 1024-sample AAC frames.
    for i in 0..AUDIO_FRAMES {
        let nb = 700u32 + ((i as u32 * 173) % AUDIO_SAMPLES_MAX);
        let frame = sine_audio_frame(nb);
        muxer.mux(frame.to_avframe()?, a_idx)?;
    }
    muxer.finish()?;
    Ok(())
}

pub fn encode_subtitle(path: &Path, enc_threads: usize) -> Result<()> {
    // Subtitle encoding is a synchronous API without internal buffering, so the
    // thread count cannot parallelize a single job; it is set anyway for
    // uniformity, and job-level parallelism comes from the worker pool.
    let mut encoder = EncoderBuilder::new_subtitle()
        .with_codec_name(Some("mov_text".to_string()))
        .with_subtitle_header(ASS_HEADER)
        .with_thread_count(enc_threads)
        .build()?;

    let segments: Vec<SubtitleSegment> = (0..SEGMENTS_PER_JOB)
        .map(|i| {
            SubtitleSegment::new(
                (i * 2000) as i64,
                (i * 2000 + 1800) as i64,
                format!("Line {i} of the benchmark"),
            )
        })
        .collect();

    let mut writer = StreamWriter::new(path)?;
    encode_subtitle_segments(&mut writer, &mut encoder, &segments)?;
    Ok(())
}

fn rainbow_frame(p: f32) -> MediaFrame<u8> {
    let rgb = rsmedia::colors::hsv_to_rgb(p * 360.0, 100.0, 100.0);
    let mut frame = MediaFrame::<u8>::new_video_frame(
        WIDTH,
        HEIGHT,
        PixelFormat::RGB24,
        time::new_rational(1, FPS as i32),
    )
    .unwrap();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            frame.data[[y, x, 0]] = rgb[0];
            frame.data[[y, x, 1]] = rgb[1];
            frame.data[[y, x, 2]] = rgb[2];
        }
    }
    frame
}

fn sine_audio_frame(nb_samples: u32) -> MediaFrame<f32> {
    let mut frame = MediaFrame::<f32>::new_audio_frame(
        SampleFormat::FLT,
        CHANNELS,
        nb_samples,
        SAMPLE_RATE,
        time::new_rational(1, SAMPLE_RATE as i32),
    )
    .unwrap();
    for i in 0..nb_samples as usize {
        let t = i as f32 / SAMPLE_RATE as f32;
        let v = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
        for c in 0..CHANNELS as usize {
            frame.data[[0, i, c]] = v;
        }
    }
    frame
}
