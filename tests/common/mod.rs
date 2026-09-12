// Shared helpers for *integration* tests (`tests/*.rs`). This module is
// intentionally independent of the library crate (`src/`): integration tests
// `mod common;` and use `common::test_output_path(...)` / the frame generators.
//
// Note: library unit tests (`src/`) keep their own `test_support` module and do
// not include this file, so there is no dependency between the two test suites.

/// Returns a standardized test output path under `tests/output/{category}/`,
/// creating the directory if needed. The path is relative to the package root,
/// which `cargo test` uses as the working directory on all platforms
/// (macOS / Linux / Windows).
///
/// # Arguments
/// * `category` - The subdirectory name (e.g., "encode_video", "transcode")
/// * `filename` - The output filename
pub fn test_output_path(category: &str, filename: &str) -> std::path::PathBuf {
    let output_dir = std::path::PathBuf::from("tests/output").join(category);
    std::fs::create_dir_all(&output_dir).ok();
    output_dir.join(filename)
}

/// Removes a test output file if it exists.
/// Only some test crates use this, so suppress dead-code warnings in the rest.
#[allow(dead_code)]
pub fn remove_test_output(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// Synthetic frame generators (ndarray-gated, used by integration tests that
// drive the high-level MediaFrame API, e.g. tests/codec_matrix.rs).
// ---------------------------------------------------------------------------

/// Generates a video test frame (RGB24): horizontal red / vertical green
/// gradients plus a phase-shifted blue channel, so round-trips exercise every
/// plane. Requires the `ndarray` feature.
#[allow(dead_code)]
#[cfg(feature = "ndarray")]
pub fn gradient_video_frame(width: usize, height: usize, phase: f32) -> rsmedia::MediaFrame<u8> {
    let mut frame = rsmedia::MediaFrame::<u8>::new_video_frame(
        width,
        height,
        rsmedia::PixelFormat::RGB24,
        rsmedia::time::new_rational(1, 25),
    )
    .expect("video frame allocation");
    for y in 0..height {
        for x in 0..width {
            frame.data[[y, x, 0]] = ((x as f32 / width as f32) * 255.0) as u8;
            frame.data[[y, x, 1]] = ((y as f32 / height as f32) * 255.0) as u8;
            frame.data[[y, x, 2]] = (phase * 255.0) as u8;
        }
    }
    frame
}

/// Generates one stereo sine-wave audio frame (FLTP, f32 samples).
/// The encoder's native sample format is negotiated and converted
/// automatically, so this single layout feeds every audio codec.
/// Requires the `ndarray` feature.
#[allow(dead_code)]
#[cfg(feature = "ndarray")]
pub fn sine_audio_frame(
    freq: f32,
    channels: u32,
    nb_samples: u32,
    sample_rate: u32,
) -> rsmedia::MediaFrame<f32> {
    let mut frame = rsmedia::MediaFrame::<f32>::new_audio_frame(
        rsmedia::SampleFormat::FLTP,
        channels,
        nb_samples,
        sample_rate,
        rsmedia::time::new_rational(1, sample_rate as i32),
    )
    .expect("audio frame allocation");
    for i in 0..nb_samples as usize {
        let t = i as f32 / sample_rate as f32;
        for c in 0..channels as usize {
            frame.data[[0, i, c]] = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
        }
    }
    frame
}
