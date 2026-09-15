//! Encoder options must actually reach the encoder.
//!
//! A builder setter that never reaches FFmpeg is invisible: the stream still
//! encodes, just with the wrong keyframe interval or the wrong number of
//! B-frames. So instead of asserting on the builder, each test here encodes a
//! real stream and reads the picture types back out of it.

mod common;

use anyhow::Result;
use common::{gradient_video_frame, remove_test_output, test_output_path};
use rsmedia::{DecoderBuilder, EncoderBuilder, MediaType, Muxer, PixelFormat, StreamReader};
use rsmpeg::ffi;

const WIDTH: usize = 64;
const HEIGHT: usize = 64;
const FPS: f32 = 25.0;
const FRAMES: i64 = 24;

/// Encodes `FRAMES` frames with the given options and returns every decoded
/// frame's `pict_type`, in presentation order.
///
/// `Ok(None)` means this FFmpeg build has no `libx264`, so there is nothing to
/// assert about encoder options — the caller treats that as a skip.
fn picture_types(
    file: &str,
    gop_size: Option<i32>,
    max_b_frames: Option<i32>,
) -> Result<Option<Vec<ffi::AVPictureType>>> {
    let path = test_output_path("encoder_options", file);

    let mut builder = EncoderBuilder::new_video(WIDTH, HEIGHT)
        .with_fps(FPS)
        .with_pix_fmt(PixelFormat::YUV420P);
    if let Some(gop_size) = gop_size {
        builder = builder.with_gop_size(gop_size);
    }
    if let Some(max_b_frames) = max_b_frames {
        builder = builder.with_max_b_frames(max_b_frames);
    }
    let encoder = match builder.with_codec_name(Some("libx264".to_string())).build() {
        Ok(encoder) => encoder,
        Err(e) if e.is_codec_not_found() => {
            println!("SKIP: libx264 is not available in this build ({e})");
            return Ok(None);
        }
        Err(e) => return Err(e.into()),
    };

    let mut muxer = Muxer::new(&path)?;
    let index = muxer.add_encoder(encoder)?;
    for frame_index in 0..FRAMES {
        let mut frame = gradient_video_frame(WIDTH, HEIGHT, frame_index as f32 / FRAMES as f32);
        frame.set_pts(frame_index);
        muxer.mux(frame.to_avframe()?, index)?;
    }
    muxer.finish()?;

    // 读回：解码器按**显示顺序**输出，因此 pict_type 序列就是实际的帧类型序列。
    let mut reader = StreamReader::new(&path)?;
    let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
    let mut types = Vec::new();
    while let Some(frame) = decoder.decode_raw(&mut reader)? {
        types.push(frame.pict_type);
    }

    remove_test_output(&path);
    Ok(Some(types))
}

/// The picture type of a decoded frame, and the two kinds these tests look for.
///
/// These are spelled `ffi::AVPictureType` — the alias FFmpeg's own header uses —
/// rather than a concrete integer width. bindgen maps a C enum to the target's
/// C `int`, and that differs between platforms (`c_uint` on Unix, `c_int` under
/// MSVC), so naming `u32` compiles on one and not the other. The alias resolves
/// per target, and the constants are declared with it too, so both sides of the
/// comparison always agree.
const fn is_i(ty: ffi::AVPictureType) -> bool {
    ty == ffi::AV_PICTURE_TYPE_I
}
const fn is_b(ty: ffi::AVPictureType) -> bool {
    ty == ffi::AV_PICTURE_TYPE_B
}

/// `with_gop_size` sets the keyframe interval. Left unset, the codec's own
/// default applies (libx264: 250) and a 24-frame clip has a single keyframe —
/// which is exactly the difference the 594-byte `gop_size: 0` bug used to hide.
#[test]
fn gop_size_sets_the_keyframe_interval() -> Result<()> {
    let Some(default_types) = picture_types("gop_default.mp4", None, None)? else {
        return Ok(());
    };
    let Some(gop_types) = picture_types("gop_4.mp4", Some(4), None)? else {
        return Ok(());
    };

    assert_eq!(default_types.len(), FRAMES as usize, "frame count");
    assert_eq!(gop_types.len(), FRAMES as usize, "frame count");

    let default_keyframes = default_types.iter().filter(|t| is_i(**t)).count();
    let gop_keyframes: Vec<usize> = gop_types
        .iter()
        .enumerate()
        .filter(|(_, t)| is_i(**t))
        .map(|(index, _)| index)
        .collect();

    assert_eq!(
        default_keyframes, 1,
        "with the codec default, a {FRAMES}-frame clip should have one keyframe"
    );
    // 不比对精确下标：x264 会用 scenecut 提前插关键帧，且 B 帧重排序让显示位置的
    // 间隔不完全均匀。真正要保证的是"关键帧间隔被 gop_size 卡住"。
    assert_eq!(
        gop_keyframes.first(),
        Some(&0),
        "the first frame is a keyframe"
    );
    assert!(
        gop_keyframes.len() >= (FRAMES as usize / 4),
        "with_gop_size(4) should give at least {} keyframes, got {gop_keyframes:?}",
        FRAMES / 4
    );
    let widest_gap = gop_keyframes
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .max()
        .unwrap_or(0);
    assert!(
        widest_gap <= 4,
        "no keyframe gap may exceed gop_size=4, got {widest_gap} in {gop_keyframes:?}"
    );
    Ok(())
}

/// `with_max_b_frames(0)` really disables B-frames, and *not* setting it leaves
/// the codec default in place (libx264: 3, so B-frames do appear).
///
/// This is the bug the wrapper used to have: `max_b_frames` defaulted to `0`,
/// which is an explicit *disable*, so no stream ever contained a B-frame.
#[test]
fn max_b_frames_zero_disables_b_frames() -> Result<()> {
    let Some(default_types) = picture_types("bframes_default.mp4", None, None)? else {
        return Ok(());
    };
    let Some(no_b_types) = picture_types("bframes_0.mp4", None, Some(0))? else {
        return Ok(());
    };

    assert!(
        default_types.iter().any(|t| is_b(*t)),
        "the codec default must produce B-frames, got {default_types:?}"
    );
    assert!(
        no_b_types.iter().all(|t| !is_b(*t)),
        "with_max_b_frames(0) must disable B-frames, got {no_b_types:?}"
    );
    // 无 B 帧时每一帧都是关键帧或 P 帧，数量与输入帧数一致。
    assert_eq!(no_b_types.len(), FRAMES as usize, "frame count");
    Ok(())
}
