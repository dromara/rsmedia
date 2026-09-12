use rsmedia::{EncoderBuilder, HWDeviceConfig, PixelFormat, StreamWriterBuilder, Writer};
use rsmedia::{colors, filter, frame::MediaFrame, time};

use rsmpeg::avfilter::AVFilter;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339())
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .init();

    rsmedia::init().unwrap();

    let width = 640;
    let height = 640;

    let mut filters = vec![
        filter::video::scale(1280, 720, Some("bicubic")),
        filter::video::crop(20, 20, width, height),
        filter::video::hqdn3d(3.0, 2.0), // 视频降噪
    ];

    // `drawtext` 需要 FFmpeg 编译时启用 libfreetype，并非所有构建都支持。
    // 运行前检测，若不支持则跳过时间水印，保证示例在各种 FFmpeg 上可运行。
    if AVFilter::get_by_name(c"drawtext").is_some() {
        filters.push(
            filter::video::DrawText::new("", 50, 50, 18, "white@0.5")
                .time_text("%{localtime}") // 当前时间水印
                .build(),
        );
    } else {
        eprintln!(
            "WARN: drawtext filter unavailable (FFmpeg built without libfreetype), skipping watermark"
        );
    }

    let output_path = "/tmp/rainbow.mp4";
    let mut writer = StreamWriterBuilder::new(output_path)
        .build()
        .expect("failed to create stream writer");

    let mut encoder = EncoderBuilder::new_video(width as usize, height as usize)
        // encoder with CUDA acceleration
        .with_hardware_device(Some(HWDeviceConfig::auto_platform()?))
        // libx264, libx265, h264_nvenc, h264_vaapi
        // .with_codec_name("h264_nvenc".to_string())
        // .with_options(Options::preset_h264_nvenc())
        .with_filters(filters)
        .build()
        .expect("failed to create encoder");

    // 为输出容器添加一条视频流，并写出容器头。
    let stream_idx = writer.add_stream(encoder.codecpar(), encoder.time_base());
    writer.write_header()?;

    // 容器（MP4 的 movenc）可能在 `write_header` 时重设流时间基，因此写包前
    // 实时取一次输出流时间基。
    let out_stream_time_base = writer.stream_time_base(stream_idx);

    let mut total_bytes = 0u64;
    let mut lost = 0usize;
    for i in 0..256 {
        // 每一帧对应彩虹色轮上的一个相位，逐帧渐变，生成平滑动画。
        let frame = rainbow_frame(width as usize, height as usize, i as f32 / 256.0);
        // 编码后立即交给 StreamWriter 写盘：先完成 `encode()`，再把每次返回的
        // packet 写到容器，这样不丢包。
        let packets = encoder.encode(frame)?; // 编码：由 Encoder 产出 packet
        let n_packets = packets.len();
        if packets.is_empty() {
            lost += 1; // 仍在编码器缓冲中，尚未产出 packet（关键帧延迟等）
            continue;
        }

        for mut p in packets {
            total_bytes += p.size as u64;
            p.set_pos(-1);
            p.set_stream_index(stream_idx as i32);
            // 把 packet 时间戳从编码器时间基换算到输出流时间基
            p.rescale_ts(encoder.time_base(), out_stream_time_base);
            // 写出到容器：由 StreamWriter 承接，不丢包
            writer.write_frame(&mut p)?;
        }

        println!(
            "Encoded frame {i}: {n} packets, cumulated {total_bytes} bytes",
            n = n_packets
        );
    }

    encoder.flush(&mut writer, false, stream_idx, out_stream_time_base)?;

    writer.write_trailer()?;

    println!(
        "Encoded {total_bytes} bytes to {:?} via Encoder + StreamWriter ({} frames, {} frames were buffered)",
        output_path, 256, lost,
    );

    Ok(())
}

fn rainbow_frame(width: usize, height: usize, p: f32) -> MediaFrame<u8> {
    // This is what generated the rainbow effect!
    // We loop through the HSV color spectrum and convert to RGB.
    let rgb = colors::hsv_to_rgb(p * 360.0, 100.0, 100.0);

    // This creates a frame with height 720, width 1280 and three channels. The RGB values for each
    // pixel are equal, and determined by the `rgb` we chose above.
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
