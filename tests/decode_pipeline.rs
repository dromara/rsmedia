//! Decode-pipeline functional tests.
//!
//! Moved out of `src/decode.rs`: these cases build their own media through the
//! high-level `MediaFrame` API (Encoder + Muxer), so keeping them in the
//! decoder's unit tests would make `decode` reference `encode` — the two
//! modules stay independent and this suite runs as an integration test.
//!
//! Requires the `ndarray` feature (frames come from the high-level API).

#![cfg(feature = "ndarray")]

mod common;

use rsmedia::colors;
use rsmedia::{
    DecoderBuilder, EncoderBuilder, Filter, MediaFrame, MediaType, Muxer, PixelFormat, Result,
    StreamReader,
};

/// 生成 `n_frames` 帧、`fps` 帧率的纯色小视频（不含 B 帧），供延迟滤镜 EOF 回归测试使用。
///
/// 使用裸 [`Encoder`](rsmedia::encode::Encoder) + [`Muxer`] 写入：需要手动维护
/// 每帧的 pts（`Muxer::mux` 不会像旧的 EncoderWrapper 那样自动设置）。
fn make_test_video(
    path: &std::path::Path,
    width: usize,
    height: usize,
    n_frames: usize,
    fps: f32,
) -> Result<()> {
    let video_encoder = EncoderBuilder::new_video(width, height)
        .with_fps(fps)
        .build()?;
    let encoder_time_base = video_encoder.time_base();
    let mut muxer = Muxer::new(path)?;
    let video_index = muxer.add_encoder(video_encoder)?;
    for i in 0..n_frames {
        let rgb = colors::hsv_to_rgb(i as f32 / n_frames as f32 * 360.0, 100.0, 100.0);
        let mut frame = MediaFrame::<u8>::new_video_frame(
            width,
            height,
            PixelFormat::RGB24,
            rsmedia::time::new_rational(1, 24),
        )?;
        for y in 0..height {
            for x in 0..width {
                frame.data[[y, x, 0]] = rgb[0];
                frame.data[[y, x, 1]] = rgb[1];
                frame.data[[y, x, 2]] = rgb[2];
            }
        }
        let mut avframe = frame.to_avframe()?;
        // 编码器 time_base = 1/fps，帧索引即 pts（每帧 1 tick = 1/fps 秒）
        avframe.set_pts(i as i64);
        avframe.set_time_base(encoder_time_base);
        muxer.mux(avframe, video_index)?;
    }
    muxer.finish()?;
    Ok(())
}

/// 自包含回归测试：验证解码器带「延迟滤镜」时，EOF 阶段的 filter flush 不会报错或丢帧。
/// 延迟滤镜（如 `framerate` 缓冲插值帧、`setpts` 重排帧）需在解码器 EOF 后逐帧冲刷；
/// 若 flush 重复向 buffersrc 发送 EOF，会得到 `AVERROR_EOF` 并中断解码（即已修复的
/// filter-EOF 类 BUG）。
#[test]
fn test_decode_delayed_filter_eof() -> Result<()> {
    let width = 64usize;
    let height = 64usize;
    // (滤镜名, 参数, 输入帧数, fps, 期望最小输出帧数)
    let cases: &[(&str, &str, usize, f32, usize)] = &[
        ("framerate", "framerate=fps=30", 30, 30.0, 30),
        ("setpts", "setpts=PTS*2", 24, 24.0, 23),
    ];

    for (i, (name, spec, n_frames, fps, min_frames)) in cases.iter().enumerate() {
        let path = common::test_output_path("decode", &format!("rsmedia_decode_delayed_{i}.mp4"));
        common::remove_test_output(&path);
        make_test_video(&path, width, height, *n_frames, *fps)?;

        let filters = vec![Filter::new(name, MediaType::VIDEO, spec.to_string())];
        let mut reader = StreamReader::new(&path)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO)
            .with_filters(filters)
            .build_from_reader(&reader)?;
        let mut count = 0usize;
        while let Some(_f) = decoder.decode_raw(&mut reader)? {
            count += 1;
        }
        assert!(
            count >= *min_frames,
            "{name} delayed-filter decode dropped frames: got {count}, expected >= {min_frames}"
        );

        let _ = std::fs::remove_file(&path);
    }
    Ok(())
}
