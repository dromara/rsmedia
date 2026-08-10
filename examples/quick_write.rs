//! 开箱即用地快速写视频：无需手动封装 writer，也无需手动设置 pts。
//!
//! 相比 video-rs（用户常需自定义 `VideoWriter` 来管理时间戳），本示例展示
//! rsmedia 的 `EncoderWrapper::write_frame` + `EncoderBuilder::preset_h264_yuv420p`
//! 即可逐帧写出，时间戳自动维护。

use rsmedia::{frame::MediaFrame, time, EncoderBuilder, PixelFormat};

fn main() -> anyhow::Result<()> {
    rsmedia::init()?;

    let (width, height) = (320, 240);
    let fps = 30f32;

    // 一键预设 + 逐帧快速写入
    let mut encoder = EncoderBuilder::new_video(width, height)
        .with_fps(fps)
        .build_wrapped(std::path::Path::new("/tmp/quick_write.mp4"))?;

    for i in 0..60 {
        let frame = rainbow_frame(width, height, i as f32 / 60.0);
        // 无需 set_pts，write_frame 自动按帧率递增
        encoder.write_frame(frame)?;
    }

    encoder.finish()?;
    println!(
        "Wrote /tmp/quick_write.mp4 ({}x{} @ {}fps)",
        width, height, fps
    );
    Ok(())
}

fn rainbow_frame(width: usize, height: usize, p: f32) -> MediaFrame<u8> {
    let rgb = rsmedia::colors::hsv_to_rgb(p * 360.0, 100.0, 100.0);
    let mut frame = MediaFrame::<u8>::new_video_frame(
        width,
        height,
        PixelFormat::RGB24,
        time::new_rational(1, 30),
    )
    .unwrap();
    for y in 0..height {
        for x in 0..width {
            frame.data[[y, x, 0]] = rgb[0];
            frame.data[[y, x, 1]] = rgb[1];
            frame.data[[y, x, 2]] = rgb[2];
        }
    }
    frame
}
