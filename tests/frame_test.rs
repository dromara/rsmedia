//! `MediaFrame` <-> `AVFrame` interop functional tests.
//!
//! `src/frame.rs` keeps the unit tests of the pure data model (layouts,
//! variants, construction, timestamps); everything here drives the real FFmpeg
//! side of the frame API — `to_avframe` / `from_avframe` round-trips, RGB<->YUV
//! conversion with real planes and linesize, buffer allocation and
//! dynamic-image conversion.
//!
//! Requires the `ndarray` feature (`frame` itself is behind it).

#![cfg(feature = "ndarray")]

use rsmedia::colors::Color;
use rsmedia::error::{Context, Result};
use rsmedia::{
    DataLayout, FrameFormat, FrameSideData, MediaFrame, MediaType, PixelFormat, SampleFormat,
};

use rsmpeg::avutil::{AVChannelLayout, AVFrame};
use rsmpeg::ffi;
use yuv::YuvStandardMatrix;

// ====================================================================
// 公共测试辅助
// ====================================================================

/// 用 `Color::from_rgb` 生成渐变测试图案并填充 RGB24 帧。
/// 返回 r/g/b 三个平面，方便调用方做断言。
fn fill_rgb_data(
    frame: &mut MediaFrame<u8>,
    width: usize,
    height: usize,
) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut r = vec![0u8; width * height];
    let mut g = vec![0u8; width * height];
    let mut b = vec![0u8; width * height];

    let rgb = frame
        .data
        .as_packed_mut()
        .expect("RGB24 frames are interleaved");
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let c = Color::from_rgb(
                ((x as f32 / width as f32) * 255.0) as u8,
                ((y as f32 / height as f32) * 255.0) as u8,
                (((x + y) as f32 / (width + height) as f32) * 255.0) as u8,
            );
            r[idx] = c.r();
            g[idx] = c.g();
            b[idx] = c.b();
            rgb[[y, x, 0]] = c.r();
            rgb[[y, x, 1]] = c.g();
            rgb[[y, x, 2]] = c.b();
        }
    }
    (r, g, b)
}

const TEST_WIDTH: usize = 320;

const TEST_HEIGHT: usize = 240;

/// 断言 `b` 与 `a` 逐像素差值不超过 `max_diff`（用于有损的颜色空间转换）。
fn assert_pixel_close(b: &MediaFrame<u8>, a: &MediaFrame<u8>, max_diff: i16) {
    let (b_rgb, a_rgb) = (
        b.data.as_packed().expect("RGB24 frames are interleaved"),
        a.data.as_packed().expect("RGB24 frames are interleaved"),
    );
    for y in 0..a.height {
        for x in 0..a.width {
            for c in 0..3 {
                let diff = (b_rgb[[y, x, c]] as i16 - a_rgb[[y, x, c]] as i16).abs();
                assert!(
                    diff <= max_diff,
                    "Color difference too large: {diff} at [{y}, {x}, {c}]"
                );
            }
        }
    }
}

/// 按布局填充一个 `AVFrame` 的每个平面：逐行写入、跳过行尾 padding，
/// 每个元素填一个确定性非零值，`element_bytes` 为 1（8bit）或 2（9..16bit）。
fn fill_frame(frame: &mut AVFrame, layout: &DataLayout, element_bytes: usize) {
    for plane in 0..layout.num_planes() {
        let (rows, samples_per_row) = layout.plane_row_extent(plane).expect("plane in range");
        let stride = frame.linesize[plane] as usize;
        let base = frame.data[plane];
        for row in 0..rows {
            for column in 0..samples_per_row {
                let value = ((row * 7 + column * 13 + plane * 31) % 250 + 1) as u8;
                for byte in 0..element_bytes {
                    // SAFETY: FFmpeg allocated `linesize * rows` bytes for this plane.
                    unsafe {
                        *base.add(row * stride + column * element_bytes + byte) = value;
                    }
                }
            }
        }
    }
}

/// 按布局把一个 `AVFrame` 平面读成紧凑字节（跳过行 padding），用于比对往返结果。
fn read_avframe_plane(
    frame: &AVFrame,
    plane: usize,
    layout: &DataLayout,
    element_bytes: usize,
) -> Vec<u8> {
    let (rows, samples_per_row) = layout.plane_row_extent(plane).expect("plane in range");
    let stride = frame.linesize[plane] as usize;
    let base = frame.data[plane];
    let mut out = Vec::with_capacity(rows * samples_per_row * element_bytes);
    for row in 0..rows {
        for byte in 0..samples_per_row * element_bytes {
            // SAFETY: FFmpeg allocated `linesize * rows` bytes for this plane.
            out.push(unsafe { *base.add(row * stride + byte) });
        }
    }
    out
}

/// 创建一个按 `fmt` 布局填充好确定性数据的 `AVFrame`。
fn create_test_frame(
    fmt: PixelFormat,
    width: usize,
    height: usize,
    element_bytes: usize,
) -> AVFrame {
    let layout = fmt.data_layout(width, height).expect("supported format");
    let mut frame = AVFrame::new();
    frame.set_format(fmt.into());
    frame.set_width(width as i32);
    frame.set_height(height as i32);
    frame.alloc_buffer().unwrap();
    fill_frame(&mut frame, &layout, element_bytes);
    frame
}

/// 创建测试用的 packed 8bit AVFrame（按 (x*ch+c+y) 生成确定性数据）
fn create_test_packed_frame(fmt: PixelFormat, width: usize, height: usize) -> AVFrame {
    let ch = match fmt.data_layout(width, height) {
        Some(DataLayout::Interleaved { components, .. }) => components,
        other => panic!("{fmt:?} should be an interleaved format, got {other:?}"),
    };
    let mut frame = AVFrame::new();
    frame.set_format(fmt.into());
    frame.set_width(width as i32);
    frame.set_height(height as i32);
    frame.alloc_buffer().unwrap();

    unsafe {
        let data = frame.data[0];
        let linesize = frame.linesize[0] as usize;
        for y in 0..height {
            for x in 0..width {
                for c in 0..ch {
                    *data.add(y * linesize + x * ch + c) = ((x + y * 3 + c * 7) % 256) as u8;
                }
            }
        }
    }
    frame
}

/// 创建测试用的 RGB AVFrame
fn create_test_rgb_frame(width: usize, height: usize) -> AVFrame {
    let mut frame = AVFrame::new();
    frame.set_format(PixelFormat::RGB24.into());
    frame.set_width(width as i32);
    frame.set_height(height as i32);
    frame.alloc_buffer().unwrap();

    unsafe {
        // 填充测试数据
        let data = frame.data[0];
        let linesize = frame.linesize[0] as usize;
        for y in 0..height {
            for x in 0..width {
                let offset = y * linesize + x * 3;
                *data.add(offset) = (x % 256) as u8; // R
                *data.add(offset + 1) = (y % 256) as u8; // G
                *data.add(offset + 2) = ((x + y) % 256) as u8; // B
            }
        }
    }

    frame
}

#[test]
fn test_rgb_yuv_roundtrip() -> Result<()> {
    let mut rgb = MediaFrame::<u8>::new_video_frame(TEST_WIDTH, TEST_HEIGHT, PixelFormat::RGB24)?;
    fill_rgb_data(&mut rgb, TEST_WIDTH, TEST_HEIGHT);

    // 单次往返：RGB -> YUV -> RGB
    let yuv = rgb.convert_rgb24_to_yuv420p()?;
    assert_eq!(yuv.format, FrameFormat::Pixel(PixelFormat::YUV420P));
    assert!(yuv.data.as_planes().is_some(), "YUV420P 应为平面布局");
    let back = yuv.convert_yuv420p_to_rgb24()?;
    assert_eq!(back.format, FrameFormat::Pixel(PixelFormat::RGB24));
    assert!(back.data.as_packed().is_some(), "RGB24 应为交错布局");
    assert_pixel_close(&back, &rgb, 3);

    // 多次链式转换（容差放宽）
    let chained = rgb
        .convert_rgb24_to_yuv420p()?
        .convert_yuv420p_to_rgb24()?
        .convert_rgb24_to_yuv420p()?
        .convert_yuv420p_to_rgb24()?;
    assert_pixel_close(&chained, &rgb, 5);

    Ok(())
}

#[test]
fn test_rgb_yuv_conversion_with_matrix() -> Result<()> {
    let mut rgb_frame =
        MediaFrame::<u8>::new_video_frame(TEST_WIDTH, TEST_HEIGHT, PixelFormat::RGB24)?;
    let _ = fill_rgb_data(&mut rgb_frame, TEST_WIDTH, TEST_HEIGHT);

    // 显式指定不同色彩矩阵，均应输出 YUV420P
    let yuv709 = rgb_frame.convert_rgb24_to_yuv420p_with_matrix(YuvStandardMatrix::Bt709)?;
    assert_eq!(yuv709.format, FrameFormat::Pixel(PixelFormat::YUV420P));

    let yuv2020 = rgb_frame.convert_rgb24_to_yuv420p_with_matrix(YuvStandardMatrix::Bt2020)?;
    assert_eq!(yuv2020.format, FrameFormat::Pixel(PixelFormat::YUV420P));

    // 不同色彩矩阵导致不同的 YUV 转换结果
    assert_ne!(yuv709.data, yuv2020.data);

    Ok(())
}

#[test]
fn test_rgb_value_transformations() {
    // 测试颜色值转换
    let rgb = create_test_rgb_frame(640, 640);
    let mut rgb_frame = MediaFrame::<u8>::from_avframe(&rgb).unwrap();

    // 测试一些典型的颜色值
    let test_colors = [
        (255, 0, 0),     // 红色
        (0, 255, 0),     // 绿色
        (0, 0, 255),     // 蓝色
        (255, 255, 255), // 白色
    ];

    let packed = rgb_frame.data.as_packed_mut().unwrap();
    for (i, &(r, g, b)) in test_colors.iter().enumerate() {
        let y = i / 2;
        let x = i % 2;
        packed[[y, x, 0]] = r;
        packed[[y, x, 1]] = g;
        packed[[y, x, 2]] = b;
    }

    // 验证颜色值
    let packed = rgb_frame.data.as_packed().unwrap();
    for (i, &(r, g, b)) in test_colors.iter().enumerate() {
        let y = i / 2;
        let x = i % 2;
        assert_eq!(packed[[y, x, 0]], r, "Red value mismatch");
        assert_eq!(packed[[y, x, 1]], g, "Green value mismatch");
        assert_eq!(packed[[y, x, 2]], b, "Blue value mismatch");
    }
}

#[test]
fn test_create_yuv420p_frame() -> Result<()> {
    let width = 640;
    let height = 360;

    // 创建填好数据的 YUV420P 帧
    let yuv_frame = create_test_frame(PixelFormat::YUV420P, width, height, 1);

    let mut frame = MediaFrame::<u8>::from_avframe(&yuv_frame)?;

    // 平面按原生尺寸存储：Y 满分辨率，U/V 各半分辨率
    let planes = frame.data.as_planes().expect("YUV420P is planar");
    assert_eq!(planes.len(), 3);
    assert_eq!(planes[1].dim(), (height / 2, width / 2));

    // 按平面重新填充一些测试数据
    let planes = frame.data.as_planes_mut().unwrap();
    for y in 0..height {
        for x in 0..width {
            planes[0][[y, x]] = (x + y) as u8; // Y
        }
    }
    for y in 0..height / 2 {
        for x in 0..width / 2 {
            planes[1][[y, x]] = 128u8; // U
            planes[2][[y, x]] = 128u8; // V
        }
    }

    // 验证
    assert_eq!(frame.width, width);
    assert_eq!(frame.height, height);
    assert_eq!(frame.format, FrameFormat::Pixel(PixelFormat::YUV420P));

    Ok(())
}

#[test]
fn test_video_rgb24_frame_conversion() -> Result<()> {
    // 创建测试视频帧
    let mut frame = AVFrame::new();
    frame.set_width(320);
    frame.set_height(240);
    frame.set_format(ffi::AV_PIX_FMT_RGB24);
    frame.alloc_buffer()?;

    // 填充测试数据
    unsafe {
        let data = std::slice::from_raw_parts_mut(
            frame.data[0].cast::<u8>(),
            frame.height as usize * frame.width as usize * 3,
        );
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = (i % 255) as u8;
        }
    }

    // 转换为 MediaFrame
    let media_frame = MediaFrame::<u8>::from_avframe(&frame)?;

    // 验证布局
    let packed = media_frame.data.as_packed().expect("RGB24 is interleaved");
    assert_eq!(packed.dim(), (240, 320, 3));
    assert_eq!(media_frame.width, 320);
    assert_eq!(media_frame.height, 240);

    // 验证数据
    let first_pixel = packed.slice(ndarray::s![0, 0, ..]);
    assert_eq!(first_pixel.to_vec(), vec![0, 1, 2]);

    Ok(())
}

#[test]
fn test_video_yuv420p_frame_conversion() -> Result<()> {
    let width = 320;
    let height = 240;

    // 每个平面用确定性数据填充，跳过行尾 padding
    let layout = PixelFormat::YUV420P.data_layout(width, height).unwrap();
    let frame = create_test_frame(PixelFormat::YUV420P, width, height, 1);

    // 转换为 MediaFrame
    let media_frame = MediaFrame::<u8>::from_avframe(&frame)?;

    // 平面保持原生尺寸：Y 满分辨率，U/V 各半分辨率（不再上采样复制成 2x2 块）
    assert_eq!(media_frame.data.num_planes(), 3);
    assert_eq!(
        media_frame.data.shapes(),
        vec![
            (height, width),
            (height / 2, width / 2),
            (height / 2, width / 2)
        ]
    );
    assert_eq!(media_frame.width, width);
    assert_eq!(media_frame.height, height);

    // 每个平面逐字节与源 AVFrame 一致（含色度，无上/下采样）
    let planes = media_frame.data.as_planes().unwrap();
    for (plane, array) in planes.iter().enumerate() {
        assert_eq!(
            read_avframe_plane(&frame, plane, &layout, 1),
            array.as_standard_layout().as_slice().unwrap().to_vec(),
            "plane {plane} mismatch"
        );
    }

    // 验证转换回 AVFrame：三个平面逐字节一致
    let converted_frame = media_frame.to_avframe()?;
    assert_eq!(converted_frame.format, ffi::AV_PIX_FMT_YUV420P);
    assert_eq!(converted_frame.width as usize, width);
    assert_eq!(converted_frame.height as usize, height);

    for plane in 0..layout.num_planes() {
        assert_eq!(
            read_avframe_plane(&frame, plane, &layout, 1),
            read_avframe_plane(&converted_frame, plane, &layout, 1),
            "plane {plane} changed across the round trip"
        );
    }

    Ok(())
}

/// * nb_samples: 是音频数据的逻辑单位,表示一帧音频中包含的采样点数量,
///   用于音频处理和时间计算, 与音频格式无关, 与时间相关：nb_samples/sample_rate = 帧的持续时间
/// * frame_size: 是内存/存储的物理单位,表示一帧音频数据的实际字节大小,
///   用于内存分配和缓冲区管理,依赖于具体的音频格式（平面/非平面）
///
/// (1)对于非平面格式（如 AV_SAMPLE_FMT_FLT）
/// frame_size = nb_samples * nb_channels * bytes_per_sample
/// 例如：1024 * 2 * 4 = 8192 bytes
///
/// (2)对于平面格式（如 AV_SAMPLE_FMT_FLTP）
/// frame_size = nb_samples * bytes_per_sample
/// 每个通道分别: 1024 * 4 = 4096 bytes
#[test]
fn test_audio_planar_frame_conversion() -> Result<()> {
    let nb_channels = 2;
    let nb_samples = 480; // 10ms 帧 (48000 × 0.01)
    let sample_rate = 48000; // 48kHz

    // 创建测试音频帧
    let mut frame = AVFrame::new();
    frame.set_format(ffi::AV_SAMPLE_FMT_FLTP);
    frame.set_nb_samples(nb_samples);
    frame.set_sample_rate(sample_rate);
    // 对于双声道
    let stereo_layout = AVChannelLayout::from_string(c"stereo").unwrap();
    frame.set_ch_layout(stereo_layout.into_inner());
    frame
        .alloc_buffer()
        .context("Failed to allocate buffer for AVFrame")?;

    // 填充测试数据
    // 平面格式 (AV_SAMPLE_FMT_FLTP) 的数据布局：
    // data[0]: [L1 L2 L3 ...] (左声道所有样本)
    // data[1]: [R1 R2 R3 ...] (右声道所有样本)
    let total_samples = (nb_samples * nb_channels) as usize;
    unsafe {
        for (ch, plane) in frame.data.iter().enumerate().take(nb_channels as usize) {
            let data = std::slice::from_raw_parts_mut(plane.cast::<f32>(), nb_samples as usize);
            for (i, sample) in data.iter_mut().enumerate() {
                *sample = (i * nb_channels as usize + ch) as f32 / total_samples as f32;
            }
        }
    }

    // 转换为 MediaFrame
    let media_frame = MediaFrame::<f32>::from_avframe(&frame)?;

    // 平面格式：每个声道一个 `(1, nb_samples)` 平面
    assert_eq!(media_frame.data.num_planes(), nb_channels as usize);
    assert_eq!(
        media_frame.data.shapes(),
        vec![(1, nb_samples as usize), (1, nb_samples as usize)]
    );
    assert_eq!(media_frame.nb_samples, nb_samples as u32);
    assert_eq!(media_frame.nb_channels, nb_channels as u32);

    // 验证数据（每个声道首样本）
    let planes = media_frame.data.as_planes().unwrap();
    assert_eq!(planes[0][[0, 0]], 0.0f32);
    assert_eq!(
        planes[1][[0, 0]],
        1.0f32 / total_samples as f32,
        "channel 1 first sample"
    );

    let converted_frame = media_frame.to_avframe().unwrap();
    assert_eq!(converted_frame.format, ffi::AV_SAMPLE_FMT_FLTP);
    assert_eq!(converted_frame.nb_samples, nb_samples);
    assert_eq!(converted_frame.sample_rate, sample_rate);

    // 验证转换后的数据
    // 注：不比较 linesize / data.len()，因为 FFmpeg 7+ on Linux 的
    // av_frame_get_buffer 会因 SIMD 对齐而 padding 出不同的 linesize 值；
    // data 是固定大小数组 [u8; 8] 没有意义。改用逐采样点比较确保数据完整性。
    unsafe {
        for ch in 0..nb_channels as usize {
            let original_data =
                std::slice::from_raw_parts(frame.data[ch] as *const f32, nb_samples as usize);
            let converted_data = std::slice::from_raw_parts(
                converted_frame.data[ch] as *const f32,
                nb_samples as usize,
            );

            // 验证每个样本
            for i in 0..nb_samples as usize {
                let orig = original_data[i];
                let conv = converted_data[i];
                let diff = (orig - conv).abs();
                assert!(
                    diff < 1e-6,
                    "Mismatch at channel {} sample {}: expected {}, got {}, diff {}",
                    ch,
                    i,
                    orig,
                    conv,
                    diff
                );
            }
        }
    }

    Ok(())
}

#[test]
fn test_audio_interleaved_frame_conversion() -> Result<()> {
    let nb_channels = 2;
    let nb_samples = 480;
    let sample_rate = 48000;

    let mut frame = AVFrame::new();
    frame.set_format(ffi::AV_SAMPLE_FMT_FLT);
    frame.set_nb_samples(nb_samples);
    frame.set_sample_rate(sample_rate);
    // 对于双声道
    let stereo_layout = AVChannelLayout::from_string(c"stereo").unwrap();
    frame.set_ch_layout(stereo_layout.into_inner());
    frame
        .alloc_buffer()
        .context("Failed to allocate buffer for AVFrame")?;

    // 填充测试数据
    // 交错格式 (AV_SAMPLE_FMT_FLT) 的数据布局：
    // data[0]: [L1 R1 L2 R2 L3 R3 ...] (左右声道交错)
    let total_samples = (nb_samples * nb_channels) as usize;
    unsafe {
        let data = std::slice::from_raw_parts_mut(frame.data[0].cast::<f32>(), total_samples);
        for (i, sample) in data.iter_mut().enumerate() {
            // 交错格式本身就是按照样本点交错排列的
            *sample = i as f32 / total_samples as f32;
        }
    }

    // 转换为 MediaFrame
    let media_frame = MediaFrame::<f32>::from_avframe(&frame)?;

    // 交错格式：单个 `(1, nb_samples, nb_channels)` 数组
    assert_eq!(media_frame.data.num_planes(), 1);
    let packed = media_frame.data.as_packed().expect("FLT is interleaved");
    assert_eq!(packed.dim(), (1, nb_samples as usize, nb_channels as usize));
    assert_eq!(media_frame.nb_samples, nb_samples as u32);
    assert_eq!(media_frame.nb_channels, nb_channels as u32);

    // 验证数据（首个采样点的两个声道）
    assert_eq!(packed[[0, 0, 0]], 0.0f32);
    assert_eq!(
        packed[[0, 0, 1]],
        1.0f32 / total_samples as f32,
        "channel 1 first sample"
    );

    let converted_frame = media_frame.to_avframe().unwrap();
    assert_eq!(converted_frame.format, ffi::AV_SAMPLE_FMT_FLT);
    assert_eq!(converted_frame.nb_samples, nb_samples);
    assert_eq!(converted_frame.sample_rate, sample_rate);

    // 验证转换后的数据
    // 注：不比较 linesize / data.len()，因为 FFmpeg 7+ on Linux 的
    // av_frame_get_buffer 会因 SIMD 对齐而 padding 出不同的 linesize 值；
    // data 是固定大小数组 [u8; 8] 没有意义。改用逐采样点比较确保数据完整性。
    unsafe {
        let original_data = std::slice::from_raw_parts(frame.data[0] as *const f32, total_samples);
        let converted_data =
            std::slice::from_raw_parts(converted_frame.data[0] as *const f32, total_samples);

        // 直接比较所有数据
        for i in 0..original_data.len() {
            assert_eq!(
                original_data[i], converted_data[i],
                "Mismatch at index {}",
                i
            );
        }
    }

    Ok(())
}

#[test]
fn test_dynamic_image_conversion() -> Result<()> {
    let mut frame = MediaFrame::<u8>::new_video_frame(TEST_WIDTH, TEST_HEIGHT, PixelFormat::RGB24)?;
    let (r, g, b) = fill_rgb_data(&mut frame, TEST_WIDTH, TEST_HEIGHT);

    // MediaFrame -> DynamicImage（格式由帧自身校验，不靠形状推断）
    let img = frame.to_dynamic_image()?;
    let rgb = img.to_rgb8();
    assert_eq!(rgb.dimensions(), (TEST_WIDTH as u32, TEST_HEIGHT as u32));
    for y in 0..TEST_HEIGHT {
        for x in 0..TEST_WIDTH {
            let idx = y * TEST_WIDTH + x;
            let px = rgb.get_pixel(x as u32, y as u32);
            assert_eq!(px.0, [r[idx], g[idx], b[idx]]);
        }
    }

    // DynamicImage -> MediaFrame
    let back = MediaFrame::<u8>::from_dynamic_image(&img)?;
    assert_eq!(back.format, FrameFormat::Pixel(PixelFormat::RGB24));
    assert_eq!(
        back.data.as_packed().map(|a| a.dim()),
        Some((TEST_HEIGHT, TEST_WIDTH, 3))
    );
    assert_eq!(back.data, frame.data);

    // RGBA 输入也应能正确转回 RGB24
    let rgba = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        TEST_WIDTH as u32,
        TEST_HEIGHT as u32,
        image::Rgba([10, 20, 30, 255]),
    ));
    let from_rgba = MediaFrame::<u8>::from_dynamic_image(&rgba)?;
    let packed = from_rgba.data.as_packed().unwrap();
    assert_eq!(packed[[0, 0, 0]], 10);
    assert_eq!(packed[[0, 0, 1]], 20);
    assert_eq!(packed[[0, 0, 2]], 30);

    Ok(())
}

#[test]
fn test_rgb24_to_avframe_respects_linesize() -> Result<()> {
    // 使用不满足 32 字节对齐的宽高，强制 av_frame_get_buffer 填充 linesize，
    // 验证 to_avframe 按行拷贝而非错误的连续内存拷贝。
    let width = 63usize;
    let height = 47usize;
    let mut frame = MediaFrame::<u8>::new_video_frame(width, height, PixelFormat::RGB24)?;
    let packed = frame.data.as_packed_mut().unwrap();
    for y in 0..height {
        for x in 0..width {
            packed[[y, x, 0]] = (x % 256) as u8;
            packed[[y, x, 1]] = (y % 256) as u8;
            packed[[y, x, 2]] = ((x + y) % 256) as u8;
        }
    }

    let av = frame.to_avframe()?;
    assert!(
        av.linesize[0] as usize >= width * 3,
        "linesize should absorb padding"
    );

    // 往返回来应逐像素一致
    let back = MediaFrame::<u8>::from_avframe(&av)?;
    assert_eq!(back.data, frame.data);

    Ok(())
}

#[test]
fn test_from_avframe_invalid_time_base() -> Result<()> {
    // 音频帧未设置 time_base，应推断为 1/sample_rate
    let mut aframe = AVFrame::new();
    aframe.set_format(ffi::AV_SAMPLE_FMT_FLTP);
    aframe.set_nb_samples(480);
    aframe.set_sample_rate(48000);
    let stereo = AVChannelLayout::from_string(c"stereo").unwrap();
    aframe.set_ch_layout(stereo.into_inner());
    aframe.alloc_buffer()?;
    let media = MediaFrame::<f32>::from_avframe(&aframe)?;
    assert_eq!(media.media_type, MediaType::AUDIO);
    assert_eq!(media.time_base.num, 1);
    assert_eq!(media.time_base.den, 48000);

    // 视频帧未设置 time_base，无法从帧内推断，保留原值（den==0）
    let mut vframe = AVFrame::new();
    vframe.set_format(ffi::AV_PIX_FMT_RGB24);
    vframe.set_width(320);
    vframe.set_height(240);
    vframe.alloc_buffer()?;
    let vmedia = MediaFrame::<u8>::from_avframe(&vframe)?;
    assert_eq!(vmedia.media_type, MediaType::VIDEO);
    assert_eq!(vmedia.time_base.num, 0); // 无效，保留原值

    Ok(())
}

#[test]
fn test_avframe_metadata_roundtrip() -> Result<()> {
    // 设置丰富的元数据，验证 from_avframe 读取、to_avframe 写回后能完整保留。
    let mut av = AVFrame::new();
    av.set_format(ffi::AV_PIX_FMT_RGB24);
    av.set_width(TEST_WIDTH as i32);
    av.set_height(TEST_HEIGHT as i32);
    unsafe {
        (*av.as_mut_ptr()).flags = ffi::AV_FRAME_FLAG_KEY as i32;
        (*av.as_mut_ptr()).quality = 12;
        (*av.as_mut_ptr()).repeat_pict = 1;
        (*av.as_mut_ptr()).colorspace = ffi::AVCOL_SPC_BT709;
        (*av.as_mut_ptr()).color_primaries = ffi::AVCOL_PRI_BT709;
        (*av.as_mut_ptr()).color_trc = ffi::AVCOL_TRC_BT709;
        (*av.as_mut_ptr()).color_range = ffi::AVCOL_RANGE_JPEG;
        (*av.as_mut_ptr()).sample_aspect_ratio = ffi::AVRational { num: 4, den: 3 };
        (*av.as_mut_ptr()).best_effort_timestamp = 42;
    }
    av.alloc_buffer()?;

    // AVFrame -> MediaFrame
    let media = MediaFrame::<u8>::from_avframe(&av)?;
    assert!(media.key_frame);
    assert_eq!(media.flags as u32, ffi::AV_FRAME_FLAG_KEY);
    assert_eq!(media.quality, 12);
    assert_eq!(media.repeat_pict, 1);
    assert_eq!(media.colorspace, ffi::AVCOL_SPC_BT709);
    assert_eq!(media.color_range, ffi::AVCOL_RANGE_JPEG);
    assert_eq!(media.sample_aspect_ratio.num, 4);
    assert_eq!(media.sample_aspect_ratio.den, 3);
    assert_eq!(media.best_effort_timestamp, 42);

    // MediaFrame -> AVFrame -> MediaFrame，验证写回的元数据能再次读回
    let av2 = media.to_avframe()?;
    let back = MediaFrame::<u8>::from_avframe(&av2)?;
    assert!(back.key_frame);
    assert_eq!(back.quality, 12);
    assert_eq!(back.repeat_pict, 1);
    assert_eq!(back.colorspace, ffi::AVCOL_SPC_BT709);
    assert_eq!(back.sample_aspect_ratio.num, 4);

    Ok(())
}

/// YUV420P 的色度平面用 `ceil` 右移，所以奇数尺寸也能表达；只有
/// RGB24 <-> YUV420P 的*转换*（`yuv` crate 的 2x2 抽取）要求偶数尺寸。
#[test]
fn test_yuv420p_odd_dimensions() -> Result<()> {
    let (width, height) = (65usize, 49usize);

    // 布局：色度平面为 ceil(w/2) x ceil(h/2)
    assert_eq!(
        PixelFormat::YUV420P.data_layout(width, height),
        Some(DataLayout::Planar(vec![(49, 65), (25, 33), (25, 33)]))
    );

    // 奇数尺寸的 YUV420P 帧可以无损往返
    let layout = PixelFormat::YUV420P.data_layout(width, height).unwrap();
    let mut av = AVFrame::new();
    av.set_format(ffi::AV_PIX_FMT_YUV420P);
    av.set_width(width as i32);
    av.set_height(height as i32);
    av.alloc_buffer()?;
    fill_frame(&mut av, &layout, 1);

    let media = MediaFrame::<u8>::from_avframe(&av)?;
    assert_eq!(media.data.shapes(), layout.shapes());
    let back = media.to_avframe()?;
    for plane in 0..layout.num_planes() {
        assert_eq!(
            read_avframe_plane(&av, plane, &layout, 1),
            read_avframe_plane(&back, plane, &layout, 1),
            "plane {plane} of a 65x49 YUV420P frame"
        );
    }

    // 但 RGB24 -> YUV420P 转换仍要求偶数尺寸
    let odd_rgb = MediaFrame::<u8>::new_video_frame(width, height, PixelFormat::RGB24)?;
    assert!(odd_rgb.convert_rgb24_to_yuv420p().is_err());

    Ok(())
}

/// 全部 packed 8bit 格式（GRAY8/YUYV422/UYVY422/RGB24/BGR24/RGBA/BGRA/ARGB/ABGR）的
/// `AVFrame → MediaFrame → AVFrame` 无损往返：构造确定数据 → ndarray →
/// 逐字节比对。第三个维度必须等于该格式的每像素元素数。
#[test]
fn test_packed_formats_lossless_roundtrip() -> Result<()> {
    let (width, height) = (65usize, 49usize); // 非 32 对齐，考察 linesize padding
    for fmt in [
        PixelFormat::GRAY8,
        PixelFormat::RGB24,
        PixelFormat::BGR24,
        PixelFormat::RGBA,
        PixelFormat::BGRA,
        PixelFormat::ARGB,
        PixelFormat::ABGR,
        PixelFormat::YUYV422,
        PixelFormat::UYVY422,
    ] {
        let layout = fmt.data_layout(width, height).expect("supported format");
        let elements = match layout {
            DataLayout::Interleaved { components, .. } => components,
            DataLayout::Planar(_) => panic!("{fmt:?} should be interleaved"),
        };

        let av = create_test_packed_frame(fmt, width, height);
        let media = MediaFrame::<u8>::from_avframe(&av)?;
        let packed = media
            .data
            .as_packed()
            .expect("packed formats are interleaved");
        assert_eq!(
            packed.dim(),
            (height, width, elements),
            "{fmt:?}: shape mismatch"
        );
        assert_eq!(
            media.data.num_planes(),
            1,
            "{fmt:?}: packed needs one plane"
        );
        assert_eq!(media.format, FrameFormat::Pixel(fmt));

        let back = media.to_avframe()?;
        assert_eq!(back.format, i32::from(fmt));
        assert_eq!(back.width as usize, width);
        assert_eq!(back.height as usize, height);

        // 逐字节比对（按行 linesize 布局）
        assert_eq!(
            read_avframe_plane(&av, 0, &layout, 1),
            read_avframe_plane(&back, 0, &layout, 1),
            "{fmt:?}: byte mismatch"
        );
    }
    Ok(())
}

/// 平面格式的 `AVFrame → MediaFrame → AVFrame` 无损往返。
///
/// 覆盖 4:2:0 / 4:2:2 / 4:4:4 / 半平面 / 平面 RGB，全部由像素格式描述符推导，
/// 代码里没有任何格式名分支。
#[test]
fn test_planar_formats_lossless_roundtrip() -> Result<()> {
    let (width, height) = (64usize, 50usize);
    for fmt in [
        PixelFormat::YUV420P,
        PixelFormat::YUV422P,
        PixelFormat::YUV444P,
        PixelFormat::NV12,
        PixelFormat::NV21,
        PixelFormat::GBRP,
    ] {
        let layout = fmt.data_layout(width, height).expect("supported format");
        let av = create_test_frame(fmt, width, height, 1);

        let media = MediaFrame::<u8>::from_avframe(&av)?;
        assert_eq!(media.format, FrameFormat::Pixel(fmt));
        assert_eq!(
            media.data.shapes(),
            layout.shapes(),
            "{fmt:?}: plane shapes"
        );
        assert_eq!(media.data.num_planes(), layout.num_planes());
        assert!(
            media.data.as_planes().is_some(),
            "{fmt:?}: planar formats must not be interleaved"
        );

        let back = media.to_avframe()?;
        assert_eq!(back.format, i32::from(fmt));
        for plane in 0..layout.num_planes() {
            assert_eq!(
                read_avframe_plane(&av, plane, &layout, 1),
                read_avframe_plane(&back, plane, &layout, 1),
                "{fmt:?}: plane {plane} changed across the round trip"
            );
        }
    }
    Ok(())
}

/// 9..16bit 平面格式：元素为 2 字节，`T` 必须是 `u16`，且 `u8` 必须被拒绝
/// （否则按 `u8` 读写会越界）。
#[test]
fn test_planar_16bit_roundtrip() -> Result<()> {
    let (width, height) = (64usize, 48usize);
    for fmt in [PixelFormat::YUV420P10LE, PixelFormat::GBRP16LE] {
        let layout = fmt.data_layout(width, height).expect("supported format");
        assert_eq!(fmt.bytes_per_component(), Some(2));
        let av = create_test_frame(fmt, width, height, 2);

        let media = MediaFrame::<u16>::from_avframe(&av)?;
        assert_eq!(media.data.shapes(), layout.shapes());

        let back = media.to_avframe()?;
        for plane in 0..layout.num_planes() {
            assert_eq!(
                read_avframe_plane(&av, plane, &layout, 2),
                read_avframe_plane(&back, plane, &layout, 2),
                "{fmt:?}: plane {plane} changed across the round trip"
            );
        }

        // 元素类型与格式的样本大小不符时必须失败
        assert!(
            MediaFrame::<u8>::from_avframe(&av).is_err(),
            "{fmt:?}: u8 must be rejected for a 16-bit format"
        );
    }
    Ok(())
}

/// RGB24 视频帧 `MediaFrame → AVFrame → MediaFrame` 全量往返：
/// 验证像素数据逐点无损、以及时间戳/格式/时间基等元数据完整保留。
#[test]
fn test_video_rgb24_data_roundtrip() -> Result<()> {
    let width = 65usize; // 非 32 对齐，强制 linesize padding，考察按行拷贝
    let height = 49usize;
    let mut media = MediaFrame::<u8>::new_video_frame(width, height, PixelFormat::RGB24)?;
    let packed = media.data.as_packed_mut().unwrap();
    for y in 0..height {
        for x in 0..width {
            packed[[y, x, 0]] = (x % 256) as u8;
            packed[[y, x, 1]] = (y % 256) as u8;
            packed[[y, x, 2]] = ((x + y) % 256) as u8;
        }
    }
    media.set_pts(12345);

    let av = media.to_avframe()?;
    let back = MediaFrame::<u8>::from_avframe(&av)?;

    // 数据无损往返
    assert_eq!(back.data, media.data, "RGB24 像素数据往返出现偏差");
    // 布局
    assert_eq!(back.data.num_planes(), 1);
    assert_eq!(
        back.data.as_packed().map(|a| a.dim()),
        Some((height, width, 3))
    );
    // 元数据
    assert_eq!(back.media_type, MediaType::VIDEO);
    assert_eq!(back.pts, 12345);
    assert_eq!(back.format, FrameFormat::Pixel(PixelFormat::RGB24));
    assert!(
        back.sample_aspect_ratio.num == 0 && back.sample_aspect_ratio.den == 0
            || back.sample_aspect_ratio.num == 0 && back.sample_aspect_ratio.den == 1,
        "sample_aspect_ratio 应保持 0/1 表示未知"
    );

    Ok(())
}

/// YUV420P 视频帧往返：平面按 FFmpeg 的原生尺寸存储（Y 满分辨率、U/V 各半），
/// 色度不再被复制成 2x2 块，因此三个平面都应逐字节无损。
#[test]
fn test_video_yuv420p_data_roundtrip() -> Result<()> {
    let width = 64usize;
    let height = 48usize;
    let mut media = MediaFrame::<u8>::new_video_frame(width, height, PixelFormat::YUV420P)?;
    // 平面原生尺寸写入：Y 逐像素变化，U/V 各自成平面。
    let planes = media.data.as_planes_mut().unwrap();
    for y in 0..height {
        for x in 0..width {
            planes[0][[y, x]] = (x + y) as u8;
        }
    }
    for y in 0..height / 2 {
        for x in 0..width / 2 {
            planes[1][[y, x]] = ((x + y) % 256) as u8;
            planes[2][[y, x]] = ((x / 2) % 256) as u8;
        }
    }

    let av = media.to_avframe()?;
    let back = MediaFrame::<u8>::from_avframe(&av)?;

    assert_eq!(back.data.num_planes(), 3);
    // 逐平面逐点断言：色度不再经过上/下采样，往返应逐字节无损
    let (before, after) = (
        media.data.as_planes().unwrap(),
        back.data.as_planes().unwrap(),
    );
    for plane in 0..3 {
        assert_eq!(
            after[plane], before[plane],
            "plane {plane} lost samples across the round trip"
        );
    }

    Ok(())
}

/// 音频（FLTP 平面 f32）帧往返：验证 `MediaFrame → AVFrame → MediaFrame`
/// 后，采样数据、采样率、采样数、声道数与格式均无损保留。
#[test]
fn test_audio_fltp_data_roundtrip() -> Result<()> {
    let nb_samples = 256u32;
    let nb_channels = 2u32;
    let sample_rate = 48000u32;
    let mut media =
        MediaFrame::new_audio_frame(SampleFormat::FLTP, nb_channels, nb_samples, sample_rate)?;
    // 填充有区分度的样本（每采样点不同，且声道间不同）
    {
        let planes = media.data.as_planes_mut().expect("FLTP is planar");
        for (ch, plane) in planes.iter_mut().enumerate() {
            for s in 0..nb_samples as usize {
                plane[[0, s]] = (s * 1000 + ch) as f32 / 1000.0;
            }
        }
    }
    media.set_pts(999);

    let av = media.to_avframe()?;
    let back = MediaFrame::<f32>::from_avframe(&av)?;

    assert_eq!(back.media_type, MediaType::AUDIO);
    assert_eq!(back.sample_rate, sample_rate);
    assert_eq!(back.nb_samples, nb_samples);
    assert_eq!(back.nb_channels, nb_channels);
    assert_eq!(back.pts, 999);
    // 平面格式：每个声道一个 `(1, nb_samples)` 平面
    assert_eq!(back.data.num_planes(), nb_channels as usize);
    assert_eq!(back.data.shapes(), vec![(1, nb_samples as usize); 2]);
    let (before, after) = (
        media.data.as_planes().unwrap(),
        back.data.as_planes().unwrap(),
    );
    for (ch, plane) in after.iter().enumerate() {
        assert_eq!(plane, &before[ch], "声道 {ch} 的音频样本丢失");
    }

    Ok(())
}

/// 交错音频（S16）往返：布局为单个 `(1, nb_samples, nb_channels)` 数组。
#[test]
fn test_audio_interleaved_data_roundtrip() -> Result<()> {
    let (nb_samples, nb_channels) = (128u32, 2u32);
    let mut media =
        MediaFrame::<i16>::new_audio_frame(SampleFormat::S16, nb_channels, nb_samples, 48000)?;
    let packed = media.data.as_packed_mut().expect("S16 is interleaved");
    for s in 0..nb_samples as usize {
        for ch in 0..nb_channels as usize {
            packed[[0, s, ch]] = (s as i16) * 10 + ch as i16;
        }
    }

    let av = media.to_avframe()?;
    let back = MediaFrame::<i16>::from_avframe(&av)?;
    assert_eq!(back.data.num_planes(), 1);
    assert_eq!(
        back.data.as_packed().map(|a| a.dim()),
        Some((1, nb_samples as usize, nb_channels as usize))
    );
    assert_eq!(back.data, media.data, "交错音频往返应无损");

    Ok(())
}

/// Every field `from_avframe` reads must be written back by `to_avframe`, so an
/// `AVFrame -> MediaFrame -> AVFrame` round trip is lossless for the modelled
/// fields. Guards the two defects that used to exist here: `pkt_dts` was never
/// written, and `duration` was fed from `pkt_duration`.
#[test]
fn test_avframe_roundtrip_preserves_all_modelled_fields() -> Result<()> {
    let mut av = AVFrame::new();
    av.set_format(ffi::AV_PIX_FMT_RGB24);
    av.set_width(16);
    av.set_height(16);
    unsafe {
        let p = av.as_mut_ptr();
        (*p).pkt_dts = 1234;
        (*p).duration = 7;
        (*p).best_effort_timestamp = 99;
        (*p).decode_error_flags = 0x2;
        (*p).chroma_location = ffi::AVCHROMA_LOC_CENTER;
        (*p).crop_top = 1;
        (*p).crop_bottom = 2;
        (*p).crop_left = 3;
        (*p).crop_right = 4;
        #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
        {
            (*p).alpha_mode = ffi::AVALPHA_MODE_STRAIGHT;
        }
        assert_eq!(
            ffi::av_dict_set(&mut (*p).metadata, c"title".as_ptr(), c"hello".as_ptr(), 0),
            0
        );
        let sd = ffi::av_frame_new_side_data(p, ffi::AV_FRAME_DATA_DISPLAYMATRIX, 4);
        assert!(!sd.is_null());
        std::ptr::copy_nonoverlapping([9u8, 8, 7, 6].as_ptr(), (*sd).data, 4);
    }
    av.alloc_buffer()?;

    // AVFrame -> MediaFrame
    let media = MediaFrame::<u8>::from_avframe(&av)?;
    assert_eq!(media.pkt_dts, 1234);
    assert_eq!(media.duration, 7);
    assert_eq!(media.pkt_duration, 7, "pkt_duration mirrors duration");
    assert_eq!(media.best_effort_timestamp, 99);
    assert_eq!(media.decode_error_flags, 0x2);
    assert_eq!(media.chroma_location, ffi::AVCHROMA_LOC_CENTER);
    assert_eq!(
        (
            media.crop_top,
            media.crop_bottom,
            media.crop_left,
            media.crop_right
        ),
        (1, 2, 3, 4)
    );
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    assert_eq!(media.alpha_mode, ffi::AVALPHA_MODE_STRAIGHT);
    assert_eq!(
        media.metadata.get("title"),
        Some("hello"),
        "metadata should be copied into the Options map"
    );
    assert_eq!(media.side_data.len(), 1);
    assert_eq!(media.side_data[0].type_, ffi::AV_FRAME_DATA_DISPLAYMATRIX);
    assert_eq!(media.side_data[0].data, [9u8, 8, 7, 6]);

    // Side data survives as owned copies.
    let entries: &[FrameSideData] = &media.side_data;
    assert_eq!(entries[0].data, vec![9, 8, 7, 6]);

    // MediaFrame -> AVFrame: the values must land back on the AVFrame verbatim.
    let back = media.to_avframe()?;
    assert_eq!(back.pkt_dts, 1234);
    assert_eq!(back.duration, 7);
    assert_eq!(back.best_effort_timestamp, 99);
    assert_eq!(back.decode_error_flags, 0x2);
    assert_eq!(back.chroma_location, ffi::AVCHROMA_LOC_CENTER);
    assert_eq!(
        (
            back.crop_top,
            back.crop_bottom,
            back.crop_left,
            back.crop_right
        ),
        (1, 2, 3, 4)
    );
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    assert_eq!(back.alpha_mode, ffi::AVALPHA_MODE_STRAIGHT);
    assert!(!back.metadata.is_null(), "metadata should be written back");
    assert_eq!(back.nb_side_data, 1);
    unsafe {
        assert_eq!((**back.side_data).type_, ffi::AV_FRAME_DATA_DISPLAYMATRIX);
        assert_eq!((**back.side_data).size, 4);
        assert_eq!(
            std::slice::from_raw_parts((**back.side_data).data, 4),
            &[9u8, 8, 7, 6]
        );
    }

    Ok(())
}

/// The audio path used to lose `duration`: it was read into `duration` but written
/// back from `pkt_duration`, which the audio branch never set.
#[test]
fn test_audio_roundtrip_preserves_duration() -> Result<()> {
    let mut av = AVFrame::new();
    av.set_format(ffi::AV_SAMPLE_FMT_FLTP);
    av.set_nb_samples(8);
    av.set_sample_rate(8000);
    av.set_ch_layout(AVChannelLayout::from_nb_channels(1).into_inner());
    unsafe {
        (*av.as_mut_ptr()).duration = 111;
    }
    av.alloc_buffer()?;

    let media = MediaFrame::<f32>::from_avframe(&av)?;
    assert_eq!(media.duration, 111);
    assert_eq!(media.pkt_duration, 111);

    let back = media.to_avframe()?;
    assert_eq!(back.duration, 111, "audio frame duration must survive");
    Ok(())
}
