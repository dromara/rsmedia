//! 滤镜（Filter）使用示例
//!
//! 演示两件事：
//! 1. **真正解码视频并通过滤镜链后处理**：读取输入视频，应用
//!    `缩放 + 去水印 + 降噪 + 饱和度增强` 滤镜链，逐帧验证处理后的
//!    尺寸/格式变化，并把前几帧保存为图片。
//! 2. 打印 `filter::video` / `filter::audio` 提供的常用滤镜 API 目录。
//!
//! 运行：`cargo run --example filter_demo -- /tmp/test.mp4`

use image::{ImageBuffer, Rgb};

use rsmedia::{filter, DecoderBuilder, MediaFrame, MediaType};

use anyhow::{Context, Result};

const OUTPUT_DIR: &str = "output_filter_demo";
const SAVE_FRAMES: usize = 3;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339())
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    rsmedia::init().unwrap();

    let source = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/test.mp4"));
    if !source.exists() {
        anyhow::bail!(
            "input video not found: {}, pass a video path as the first argument",
            source.display()
        );
    }

    // ---- 1) 真实处理视频：解码 + 滤镜链 ----
    // 缩放 -> 裁剪 -> 去水印 -> 视频降噪 -> 饱和度增强
    // 注意：delogo 区域必须离帧边界至少 band(默认1) 像素，故 x/y 从 10 开始。
    let filters = vec![
        filter::video::scale(426, 240, None),  // 缩放到 426x240
        filter::video::crop(0, 0, 400, 200),   // 裁剪出 400x200 区域
        filter::video::delogo(10, 10, 60, 20), // 去除左上角水印
        filter::video::hqdn3d(3.0, 2.0),       // 视频降噪
        filter::video::saturation(1.3),        // 画质增强：饱和度
    ];

    let filter_count = filters.len();
    let mut decoder = DecoderBuilder::new(MediaType::VIDEO)
        .with_filters(filters)
        .build_wrapped(source.as_path())
        .context("failed to create decoder")?;

    std::fs::create_dir_all(OUTPUT_DIR).context("failed to create output directory")?;

    println!(
        "processing '{}' with {} filters...",
        source.display(),
        filter_count
    );

    let mut decoded = 0usize;
    let mut saved = 0usize;
    loop {
        match decoder.decode_frame() {
            Ok(Some(frame)) => {
                let fmt = frame
                    .video_format()
                    .map(|f| f.get_pix_fmt_name().to_string())
                    .unwrap_or_else(|| "n/a".to_string());
                println!(
                    "frame[{decoded}] pts={} size={}x{} fmt={}",
                    frame.pts, frame.width, frame.height, fmt
                );

                if saved < SAVE_FRAMES {
                    save_frame(&frame, saved)?;
                    saved += 1;
                }
                decoded += 1;
                if decoded >= 30 {
                    println!("stopped after {decoded} frames (demo)");
                    break;
                }
            }
            Ok(None) => {
                println!("decoder reached end of stream");
                break;
            }
            Err(e) => {
                println!("decode error: {e}");
                break;
            }
        }
    }

    println!(
        "decoded {decoded} frames, saved {saved} to '{}' (note: size={}x{} is the filtered result)",
        OUTPUT_DIR, 400, 200
    );

    // ---- 2) 打印滤镜 API 目录（仅展示 spec，不实际运行）----
    print_filter_catalog();

    Ok(())
}

/// 把处理后的帧另存为 PNG（RGB 转换后再写）。
fn save_frame(frame: &MediaFrame<u8>, index: usize) -> Result<()> {
    let rgb = frame.convert_yuv_to_rgb()?;
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_raw(
        frame.width as u32,
        frame.height as u32,
        rgb.data.as_slice().unwrap().to_vec(),
    )
    .context("failed to build image buffer")?;
    let path = format!("{OUTPUT_DIR}/filtered_{:03}.png", index);
    img.save(&path).context("failed to save frame")?;
    println!("  saved {path}");
    Ok(())
}

/// 打印 `filter` 模块提供的滤镜 API 及其生成的 FFmpeg spec。
fn print_filter_catalog() {
    println!("\n===== video filters =====");
    let v = vec![
        (
            "scale(1280,720,None)",
            filter::video::scale(1280, 720, None).spec(),
        ),
        (
            "crop(x,y,w,h)",
            filter::video::crop(10, 10, 640, 360).spec(),
        ),
        ("fade_in(30)", filter::video::fade_in(30).spec()),
        ("fade_out(30,30)", filter::video::fade_out(30, 30).spec()),
        ("unsharp()", filter::video::unsharp().spec()),
        ("blur(2.0)", filter::video::blur(2.0).spec()),
        (
            "eq(brightness,contrast)",
            filter::video::eq(0.1, 1.2).spec(),
        ),
        ("fps(30)", filter::video::fps(30.0).spec()),
        (
            "yadif(\"send_frame\")",
            filter::video::yadif("send_frame").spec(),
        ),
        (
            "pad(w,h,x,y,color)",
            filter::video::pad(1920, 1080, 0, 0, "black").spec(),
        ),
        ("setdar(16,9)", filter::video::setdar(16, 9).spec()),
        ("setsar(1,1)", filter::video::setsar(1, 1).spec()),
        ("hue(30)", filter::video::hue(30).spec()),
        ("negate()", filter::video::negate().spec()),
        ("noise(10)", filter::video::noise(10).spec()),
        ("hqdn3d(3.0,2.0)", filter::video::hqdn3d(3.0, 2.0).spec()),
        ("nlmeans(1.5)", filter::video::nlmeans(1.5).spec()),
        ("gamma(1.2)", filter::video::gamma(1.2).spec()),
        ("saturation(1.3)", filter::video::saturation(1.3).spec()),
        ("vibrance(0.4)", filter::video::vibrance(0.4).spec()),
        ("deblock()", filter::video::deblock().spec()),
        ("DrawText::new(...).time_text(\"%{localtime}\")", {
            filter::video::DrawText::new("", 10, 10, 24, "white")
                .time_text("%{localtime}")
                .build()
                .spec()
        }),
        ("transpose(1)", filter::video::transpose(1).spec()),
        (
            "delogo(0,0,100,50)",
            filter::video::delogo(0, 0, 100, 50).spec(),
        ),
        (
            "Delogo::new().add_region(x2).band(2)",
            filter::video::Delogo::new()
                .add_region(0, 0, 100, 50)
                .add_region(640, 0, 100, 50)
                .band(2)
                .build()
                .into_iter()
                .map(|f| f.spec())
                .collect::<Vec<_>>()
                .join(" , "),
        ),
    ];
    for (name, spec) in v {
        println!("  {name}\n    -> {spec}");
    }

    println!("\n===== audio filters =====");
    let a = vec![
        ("volume(1.5)", filter::audio::volume(1.5).spec()),
        ("loudnorm(-16.0)", filter::audio::loudnorm(-16.0).spec()),
        ("highpass(80)", filter::audio::highpass(80).spec()),
        ("lowpass(4000)", filter::audio::lowpass(4000).spec()),
        ("atempo(1.25)", filter::audio::atempo(1.25).spec()),
        (
            "fft_denoise(20,-50)",
            filter::audio::fft_denoise(20, -50).spec(),
        ),
        ("denoise(15.0)", filter::audio::denoise(15.0).spec()),
        ("compressor(3.0,Some(30),Some(200))", {
            filter::audio::compressor(3.0, Some(30.0), Some(200.0))
                .unwrap()
                .spec()
        }),
    ];
    for (name, spec) in a {
        println!("  {name}\n    -> {spec}");
    }
}
