use rsmedia::{
    DecoderBuilder, EncoderBuilder, MediaType, PixelFormat, colors,
    frame::MediaFrame,
    hwaccel::{HWDeviceConfig, HWDeviceType},
    time,
};
use std::path::PathBuf;

fn rainbow_frame(width: usize, height: usize, p: f32) -> MediaFrame<u8> {
    let rgb = colors::hsv_to_rgb(p * 360.0, 100.0, 100.0);
    let mut frame = MediaFrame::<u8>::new_video_frame(
        width,
        height,
        PixelFormat::RGB24,
        time::new_rational(1, 24),
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

fn check(codec: &'static str, ext: &str) -> anyhow::Result<()> {
    let (w, h, n) = (320usize, 240usize, 60usize);
    let path = PathBuf::from(format!("/tmp/hw_vt_{codec}.{ext}"));
    let _ = std::fs::remove_file(&path);

    let dev = HWDeviceConfig::auto_platform_with(&[HWDeviceType::VIDEOTOOLBOX])?;
    let enc = EncoderBuilder::new_video(w, h)
        .with_fps(30.0)
        .with_codec_name(codec.to_string())
        .with_hardware_device(Some(dev))
        .build()?;
    let enc_tb = enc.time_base();
    let mut muxer = rsmedia::mux::Muxer::new(&path)?;
    let v_idx = muxer.add_encoder(enc)?;
    for i in 0..n {
        let mut av = rainbow_frame(w, h, i as f32 / n as f32).to_avframe()?;
        av.set_pts(i as i64);
        av.set_time_base(enc_tb);
        muxer.mux(av, v_idx)?;
    }
    muxer.finish()?;

    let mut reader = rsmedia::StreamReader::new(&path)?;
    let mut dec = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
    let mut count = 0usize;
    while let Some(f) = dec.decode_frame(&mut reader)? {
        count += 1;
        if count == 1 {
            println!("[{codec}] decoded[0]: {}x{}", f.width, f.height);
        }
    }
    println!("[{codec}] encoded {n} frames, decoded {count} frames");
    let _ = std::fs::remove_file(&path);
    Ok(())
}

fn main() -> anyhow::Result<()> {
    rsmedia::init()?;
    check("h264_videotoolbox", "mp4")?;
    check("hevc_videotoolbox", "mp4")?;
    check("prores_videotoolbox", "mov")?;
    Ok(())
}
