//! 开箱即用地快速写视频：无需手动封装 writer，也无需手动设置 pts。
//!
//! 相比 video-rs（用户常需自定义 `VideoWriter` 来管理时间戳），本示例展示
//! rsmedia 的裸 `Encoder` + `Muxer`（`EncoderBuilder::preset_h264_yuv420p` +
//! `add_stream` + `mux`）即可逐帧写出，pts 手动按帧率递增。

use rsmedia::{EncoderBuilder, PixelFormat, frame::MediaFrame, mux::Muxer, time};

fn main() -> anyhow::Result<()> {
    rsmedia::init()?;

    let (width, height) = (320, 240);
    let fps = 30f32;

    // 一键预设 + 逐帧快速写入
    let encoder = EncoderBuilder::new_video(width, height)
        .with_fps(fps)
        .build()?;
    let enc_tb = encoder.time_base();
    let mut muxer = Muxer::new("/tmp/quick_write.mp4")?;
    let v_idx = muxer.add_encoder(encoder)?;

    for i in 0..60 {
        let frame = rainbow_frame(width, height, i as f32 / 60.0);
        let mut av = frame.to_avframe()?;
        // 编码器 time_base = 1/fps，帧索引即 pts（每帧 1 tick = 1/fps 秒）
        av.set_pts(i as i64);
        av.set_time_base(enc_tb);
        muxer.mux(av, v_idx)?;
    }

    muxer.finish()?;
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
