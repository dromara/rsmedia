//! Zero-configuration timestamps: encode video and audio without ever setting pts.
//!
//! Every frame here leaves `MediaFrame.pts` at its default (`AV_NOPTS_VALUE`). The
//! encoder numbers them automatically — video gets one `1/fps` tick per frame, audio
//! gets sample positions in `1/sample_rate` — and the muxer rescales into the container
//! time base. Verify with:
//!
//! ```text
//! ffprobe -select_streams v -show_frames -show_entries frame=pts_time,pkt_duration_time /tmp/rsmedia_auto_pts.mp4
//! ffprobe -select_streams a -show_frames -show_entries frame=pts_time,nb_samples /tmp/rsmedia_auto_pts.m4a
//! ```

use rsmedia::frame::MediaFrame;
use rsmedia::mux::Muxer;
use rsmedia::{EncoderBuilder, PixelFormat, SampleFormat, time};

const WIDTH: usize = 320;
const HEIGHT: usize = 240;
const FPS: f32 = 30.0;
const VIDEO_FRAMES: usize = 90; // 3 s

const SAMPLE_RATE: u32 = 44_100;
const CHANNELS: u32 = 2;
const AUDIO_FRAMES: usize = 129; // variable sizes, sums to ~3 s
const FREQ: f32 = 440.0;

fn main() -> anyhow::Result<()> {
    rsmedia::init()?;

    encode_video()?;
    encode_audio()?;

    println!("done: /tmp/rsmedia_auto_pts.mp4, /tmp/rsmedia_auto_pts.m4a");
    Ok(())
}

/// 90 video frames, pts left unset on every one of them.
fn encode_video() -> anyhow::Result<()> {
    let encoder = EncoderBuilder::new_video(WIDTH, HEIGHT)
        .with_fps(FPS)
        .build()?;
    let mut muxer = Muxer::new("/tmp/rsmedia_auto_pts.mp4")?;
    let v_idx = muxer.add_encoder(encoder)?;

    for i in 0..VIDEO_FRAMES {
        // No `set_pts` anywhere: the encoder numbers frames 0, 1, 2, ... itself.
        let frame = rainbow_frame(i as f32 / VIDEO_FRAMES as f32);
        muxer.mux(frame.to_avframe()?, v_idx)?;
    }
    muxer.finish()?;
    Ok(())
}

/// Variable-size audio frames (so the encoder's sample FIFO has to split and merge),
/// pts left unset on every one of them.
fn encode_audio() -> anyhow::Result<()> {
    let encoder = EncoderBuilder::new_audio(
        128_000,
        CHANNELS as i32,
        SAMPLE_RATE as i32,
        SampleFormat::FLTP,
    )
    .build()?;
    let mut muxer = Muxer::new("/tmp/rsmedia_auto_pts.m4a")?;
    let a_idx = muxer.add_encoder(encoder)?;

    // Deliberately irregular sizes around the aac frame size (1024).
    let sizes: Vec<u32> = (0..AUDIO_FRAMES)
        .map(|i| 700u32 + ((i as u32 * 173) % 900))
        .collect();
    let total: u32 = sizes.iter().sum();
    println!("audio input: {AUDIO_FRAMES} frames, {total} samples");

    for &nb in &sizes {
        // No `set_pts` anywhere: sample positions are assigned automatically.
        let frame = sine_audio_frame(nb);
        muxer.mux(frame.to_avframe()?, a_idx)?;
    }
    muxer.finish()?;
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
        let v = (2.0 * std::f32::consts::PI * FREQ * t).sin() * 0.5;
        for c in 0..CHANNELS as usize {
            frame.data[[0, i, c]] = v;
        }
    }
    frame
}
