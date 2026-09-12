//! `MediaFrame` <-> `AVFrame` interop functional tests.
//!
//! Moved out of `src/frame.rs` (which keeps only unit tests of the pure
//! ndarray data model: construction, data access, timestamps, format
//! validation): everything here drives the real FFmpeg side of the frame API —
//! `to_avframe` / `from_avframe` round-trips, RGB<->YUV conversion with real
//! planes and linesize, buffer allocation and dynamic-image conversion.
//!
//! Requires the `ndarray` feature (`frame` itself is behind it).

#![cfg(feature = "ndarray")]

use rsmedia::colors::Color;
use rsmedia::error::Context;
use rsmedia::imgutils;
use rsmedia::{FrameFormat, MediaFrame, MediaType, PixelFormat, Result, SampleFormat};
use rsmpeg::avutil::{AVChannelLayout, AVFrame};
use rsmpeg::ffi;
use yuv::YuvStandardMatrix;

// ====================================================================
// 公共测试辅助（与 src/frame.rs 的单元测试各留一份）
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
            frame.data[[y, x, 0]] = c.r();
            frame.data[[y, x, 1]] = c.g();
            frame.data[[y, x, 2]] = c.b();
        }
    }
    (r, g, b)
}

const TEST_WIDTH: usize = 320;

const TEST_HEIGHT: usize = 240;

const TIME_BASE: ffi::AVRational = ffi::AVRational { num: 1, den: 30 }; // 30 fps

/// 断言 `b` 与 `a` 逐像素差值不超过 `max_diff`（用于有损的颜色空间转换）。
fn assert_pixel_close(b: &MediaFrame<u8>, a: &MediaFrame<u8>, max_diff: i16) {
    for y in 0..a.height {
        for x in 0..a.width {
            for c in 0..3 {
                let diff = (b.data[[y, x, c]] as i16 - a.data[[y, x, c]] as i16).abs();
                assert!(
                    diff <= max_diff,
                    "Color difference too large: {diff} at [{y}, {x}, {c}]"
                );
            }
        }
    }
}

/// 创建测试用的 packed 8bit AVFrame（按 (x*ch+c+y) 生成确定性数据）
fn create_test_packed_frame(fmt: PixelFormat, width: usize, height: usize) -> AVFrame {
    let ch = fmt.packed_channels().expect("packed format");
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

/// 创建测试用的 YUV420P AVFrame
fn create_test_yuv_frame(width: usize, height: usize) -> AVFrame {
    let mut frame = AVFrame::new();
    frame.set_format(PixelFormat::YUV420P.into());
    frame.set_width(width as i32);
    frame.set_height(height as i32);
    frame.alloc_buffer().unwrap();

    unsafe {
        // 填充 Y 平面
        let y_data = frame.data[0];
        let y_linesize = frame.linesize[0] as usize;
        for y in 0..height {
            for x in 0..width {
                *y_data.add(y * y_linesize + x) = ((x + y) % 256) as u8;
            }
        }

        // 填充 U 平面
        let u_data = frame.data[1];
        let u_linesize = frame.linesize[1] as usize;
        for y in 0..height / 2 {
            for x in 0..width / 2 {
                *u_data.add(y * u_linesize + x) = (x % 256) as u8;
            }
        }

        // 填充 V 平面
        let v_data = frame.data[2];
        let v_linesize = frame.linesize[2] as usize;
        for y in 0..height / 2 {
            for x in 0..width / 2 {
                *v_data.add(y * v_linesize + x) = (y % 256) as u8;
            }
        }
    }

    frame
}

#[test]
fn test_rgb_yuv_roundtrip() -> Result<()> {
    let mut rgb =
        MediaFrame::<u8>::new_video_frame(TEST_WIDTH, TEST_HEIGHT, PixelFormat::RGB24, TIME_BASE)?;
    fill_rgb_data(&mut rgb, TEST_WIDTH, TEST_HEIGHT);

    // 单次往返：RGB -> YUV -> RGB
    let yuv = rgb.convert_rgb_to_yuv()?;
    assert_eq!(yuv.format, FrameFormat::Pixel(PixelFormat::YUV420P));
    let back = yuv.convert_yuv_to_rgb()?;
    assert_eq!(back.format, FrameFormat::Pixel(PixelFormat::RGB24));
    assert_pixel_close(&back, &rgb, 3);

    // 多次链式转换（容差放宽）
    let chained = rgb
        .convert_rgb_to_yuv()?
        .convert_yuv_to_rgb()?
        .convert_rgb_to_yuv()?
        .convert_yuv_to_rgb()?;
    assert_pixel_close(&chained, &rgb, 5);

    Ok(())
}

#[test]
fn test_rgb_yuv_conversion_with_matrix() -> Result<()> {
    let mut rgb_frame =
        MediaFrame::<u8>::new_video_frame(TEST_WIDTH, TEST_HEIGHT, PixelFormat::RGB24, TIME_BASE)?;
    let _ = fill_rgb_data(&mut rgb_frame, TEST_WIDTH, TEST_HEIGHT);

    // 显式指定不同色彩矩阵，均应输出 YUV420P
    let yuv709 = rgb_frame.convert_rgb_to_yuv_with_matrix(YuvStandardMatrix::Bt709)?;
    assert_eq!(yuv709.format, FrameFormat::Pixel(PixelFormat::YUV420P));

    let yuv2020 = rgb_frame.convert_rgb_to_yuv_with_matrix(YuvStandardMatrix::Bt2020)?;
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

    for (i, &(r, g, b)) in test_colors.iter().enumerate() {
        let y = i / 2;
        let x = i % 2;
        rgb_frame.data[[y, x, 0]] = r;
        rgb_frame.data[[y, x, 1]] = g;
        rgb_frame.data[[y, x, 2]] = b;
    }

    // 验证颜色值
    for (i, &(r, g, b)) in test_colors.iter().enumerate() {
        let y = i / 2;
        let x = i % 2;
        assert_eq!(rgb_frame.data[[y, x, 0]], r, "Red value mismatch");
        assert_eq!(rgb_frame.data[[y, x, 1]], g, "Green value mismatch");
        assert_eq!(rgb_frame.data[[y, x, 2]], b, "Blue value mismatch");
    }
}

#[test]
fn test_create_yuv420p_frame() -> Result<()> {
    let width = 640;
    let height = 360;

    // 创建空的YUV420P帧
    let yuv_frame = create_test_yuv_frame(width, height);

    let mut frame = MediaFrame::<u8>::from_avframe(&yuv_frame)?;

    // array 重新 填充一些测试数据
    for y in 0..height {
        for x in 0..width {
            frame.data[[y, x, 0]] = (x + y) as u8; // Y
            if y % 2 == 0 && x % 2 == 0 {
                frame.data[[y, x, 1]] = 128u8; // U
                frame.data[[y, x, 2]] = 128u8; // V
            }
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

    // 验证维度
    assert_eq!(media_frame.data.dim(), (240, 320, 3));
    assert_eq!(media_frame.width, 320);
    assert_eq!(media_frame.height, 240);

    // 验证数据
    let first_pixel = media_frame.data.slice(ndarray::s![0, 0, ..]);
    assert_eq!(first_pixel.to_vec(), vec![0, 1, 2]);

    Ok(())
}

#[test]
fn test_video_yuv420p_frame_conversion() -> Result<()> {
    let width = 320;
    let height = 240;

    let mut frame = AVFrame::new();
    frame.set_width(width);
    frame.set_height(height);
    frame.set_format(ffi::AV_PIX_FMT_YUV420P);
    frame.alloc_buffer()?;

    // 填充测试数据
    // Y 平面使用全分辨率 (width × height)
    // U 平面使用 1/4 分辨率 ((width/2) × (height/2))
    // V 平面使用 1/4 分辨率 ((width/2) × (height/2))
    //
    // 使用 imgutils::fill_plane_with 逐行按 data[p]+y*linesize[p] 写入，
    // 避免漏写 av_frame_get_buffer 对齐 linesize 后行末尾的填充字节（valgrind uninit）。
    unsafe {
        imgutils::fill_plane_with(&frame, 0, width as usize, height as usize, |x, y| {
            ((y * width as usize + x) as u8) % 255
        });
        imgutils::fill_plane_with(
            &frame,
            1,
            width as usize / 2,
            height as usize / 2,
            |x, y| ((y * (width as usize / 2) + x) as u8).wrapping_add(85) % 255,
        );
        imgutils::fill_plane_with(
            &frame,
            2,
            width as usize / 2,
            height as usize / 2,
            |x, y| ((y * (width as usize / 2) + x) as u8).wrapping_add(170) % 255,
        );
    }

    // 转换为 MediaFrame
    let media_frame = MediaFrame::<u8>::from_avframe(&frame)?;

    // 验证维度
    assert_eq!(media_frame.data.dim(), (height as usize, width as usize, 3));
    assert_eq!(media_frame.width, width as usize);
    assert_eq!(media_frame.height, height as usize);

    // 验证数据
    unsafe {
        // 验证 Y 分量
        let y_val = *frame.data[0];
        assert_eq!(media_frame.data[[0, 0, 0]], y_val);

        // 验证 U 分量 (2x2块使用相同的值)
        let u_val = *frame.data[1];
        assert_eq!(media_frame.data[[0, 0, 1]], u_val);
        assert_eq!(media_frame.data[[0, 1, 1]], u_val);
        assert_eq!(media_frame.data[[1, 0, 1]], u_val);
        assert_eq!(media_frame.data[[1, 1, 1]], u_val);

        // 验证 V 分量 (2x2块使用相同的值)
        let v_val = *frame.data[2];
        assert_eq!(media_frame.data[[0, 0, 2]], v_val);
        assert_eq!(media_frame.data[[0, 1, 2]], v_val);
        assert_eq!(media_frame.data[[1, 0, 2]], v_val);
        assert_eq!(media_frame.data[[1, 1, 2]], v_val);
    }

    // 验证上采样是否正确
    // 检查第一个2x2块的 U 分量
    let u_block = media_frame.data.slice(ndarray::s![0..2, 0..2, 1]);
    let u_val = u_block[(0, 0)];
    for y in 0..2 {
        for x in 0..2 {
            assert_eq!(u_block[[y, x]], u_val, "U value mismatch at [{}, {}]", y, x);
        }
    }

    // 检查第一个2x2块的 V 分量
    let v_block = media_frame.data.slice(ndarray::s![0..2, 0..2, 2]);
    let v_val = v_block[(0, 0)];
    for y in 0..2 {
        for x in 0..2 {
            assert_eq!(v_block[[y, x]], v_val, "V value mismatch at [{}, {}]", y, x);
        }
    }

    // 验证转换回 AVFrame
    let converted_frame = media_frame.to_avframe()?;
    assert_eq!(converted_frame.format, ffi::AV_PIX_FMT_YUV420P);
    assert_eq!(converted_frame.width, width);
    assert_eq!(converted_frame.height, height);

    // 验证转换后的数据,考虑了行步长(linesize)的影响
    // 分别处理每个平面的数据,逐行比较而不是整块比较
    // 为 UV 平面使用正确的宽度和高度（原尺寸的一半）
    unsafe {
        // 验证 Y 平面
        let y_linesize = frame.linesize[0] as usize;
        let converted_y_linesize = converted_frame.linesize[0] as usize;

        // 逐行比较 Y 平面数据
        for y in 0..height as usize {
            let original_line = std::slice::from_raw_parts(
                frame.data[0].add(y * y_linesize) as *const u8,
                width as usize,
            );
            let converted_line = std::slice::from_raw_parts(
                converted_frame.data[0].add(y * converted_y_linesize) as *const u8,
                width as usize,
            );
            assert_eq!(
                original_line, converted_line,
                "Y plane mismatch at line {}",
                y
            );
        }

        // 验证 U 平面
        let u_linesize = frame.linesize[1] as usize;
        let converted_u_linesize = converted_frame.linesize[1] as usize;
        let uv_height = height as usize / 2;
        let uv_width = width as usize / 2;

        // 逐行比较 U 平面数据
        for y in 0..uv_height {
            let original_line = std::slice::from_raw_parts(
                frame.data[1].add(y * u_linesize) as *const u8,
                uv_width,
            );
            let converted_line = std::slice::from_raw_parts(
                converted_frame.data[1].add(y * converted_u_linesize) as *const u8,
                uv_width,
            );
            assert_eq!(
                original_line, converted_line,
                "U plane mismatch at line {}",
                y
            );
        }

        // 验证 V 平面
        let v_linesize = frame.linesize[2] as usize;
        let converted_v_linesize = converted_frame.linesize[2] as usize;

        // 逐行比较 V 平面数据
        for y in 0..uv_height {
            let original_line = std::slice::from_raw_parts(
                frame.data[2].add(y * v_linesize) as *const u8,
                uv_width,
            );
            let converted_line = std::slice::from_raw_parts(
                converted_frame.data[2].add(y * converted_v_linesize) as *const u8,
                uv_width,
            );
            assert_eq!(
                original_line, converted_line,
                "V plane mismatch at line {}",
                y
            );
        }
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

    // 验证维度
    assert_eq!(
        media_frame.data.dim(),
        (1, nb_samples as usize, nb_channels as usize)
    );
    assert_eq!(media_frame.nb_samples, nb_samples as u32);
    assert_eq!(media_frame.nb_channels, nb_channels as u32);

    // 验证数据
    let first_sample = media_frame.data.slice(ndarray::s![0, 0, ..]);
    assert_eq!(
        first_sample.to_vec(),
        vec![0.0f32, 1.0f32 / total_samples as f32]
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

    // 验证维度
    assert_eq!(
        media_frame.data.dim(),
        (1, nb_samples as usize, nb_channels as usize)
    );
    assert_eq!(media_frame.nb_samples, nb_samples as u32);
    assert_eq!(media_frame.nb_channels, nb_channels as u32);

    // 验证数据
    let first_sample = media_frame.data.slice(ndarray::s![0, 0, ..]);
    assert_eq!(
        first_sample.to_vec(),
        vec![0.0f32, 1.0f32 / total_samples as f32]
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
    let mut frame =
        MediaFrame::<u8>::new_video_frame(TEST_WIDTH, TEST_HEIGHT, PixelFormat::RGB24, TIME_BASE)?;
    let (r, g, b) = fill_rgb_data(&mut frame, TEST_WIDTH, TEST_HEIGHT);

    // MediaFrame -> DynamicImage
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
    let back = MediaFrame::<u8>::from_dynamic_image(&img, TIME_BASE)?;
    assert_eq!(back.format, FrameFormat::Pixel(PixelFormat::RGB24));
    assert_eq!(back.data.dim(), (TEST_HEIGHT, TEST_WIDTH, 3));
    assert_eq!(back.data, frame.data);

    // RGBA 输入也应能正确转回 RGB24
    let rgba = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        TEST_WIDTH as u32,
        TEST_HEIGHT as u32,
        image::Rgba([10, 20, 30, 255]),
    ));
    let from_rgba = MediaFrame::<u8>::from_dynamic_image(&rgba, TIME_BASE)?;
    assert_eq!(from_rgba.data[[0, 0, 0]], 10);
    assert_eq!(from_rgba.data[[0, 0, 1]], 20);
    assert_eq!(from_rgba.data[[0, 0, 2]], 30);

    Ok(())
}

#[test]
fn test_rgb24_to_avframe_respects_linesize() -> Result<()> {
    // 使用不满足 32 字节对齐的宽高，强制 av_frame_get_buffer 填充 linesize，
    // 验证 to_avframe 按行拷贝而非错误的连续内存拷贝。
    let width = 63usize;
    let height = 47usize;
    let mut frame =
        MediaFrame::<u8>::new_video_frame(width, height, PixelFormat::RGB24, TIME_BASE)?;
    for y in 0..height {
        for x in 0..width {
            frame.data[[y, x, 0]] = (x % 256) as u8;
            frame.data[[y, x, 1]] = (y % 256) as u8;
            frame.data[[y, x, 2]] = ((x + y) % 256) as u8;
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

#[test]
fn test_yuv420p_odd_dimensions_rejected() -> Result<()> {
    // 奇数尺寸 YUV420P：from_avframe 应报错
    let mut frame = AVFrame::new();
    frame.set_format(ffi::AV_PIX_FMT_YUV420P);
    frame.set_width(63);
    frame.set_height(47);
    frame.alloc_buffer()?;
    unsafe {
        std::ptr::write_bytes(
            frame.data[0],
            0,
            frame.linesize[0] as usize * frame.height as usize,
        );
        for p in 1..3 {
            std::ptr::write_bytes(
                frame.data[p],
                128,
                frame.linesize[p] as usize * (frame.height as usize / 2),
            );
        }
    }
    assert!(MediaFrame::<u8>::from_avframe(&frame).is_err());

    // 奇数尺寸 YUV420P MediaFrame：to_avframe 应报错
    let media = MediaFrame::<u8>::new_video_frame(63, 47, PixelFormat::YUV420P, TIME_BASE)?;
    assert!(media.to_avframe().is_err());

    Ok(())
}

/// 全部 packed 8bit 格式（GRAY8/YUYV422/UYVY422/RGB24/BGR24/RGBA/BGRA/ARGB/ABGR）的
/// `AVFrame → MediaFrame → AVFrame` 无损往返：构造确定数据 → ndarray →
/// 逐字节比对。C 维度必须等于 `packed_channels`（1/2/3/4）。
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
        let ch = fmt.packed_channels().expect("packed format");

        let av = create_test_packed_frame(fmt, width, height);
        let media = MediaFrame::<u8>::from_avframe(&av)?;
        assert_eq!(
            media.data.dim(),
            (height, width, ch),
            "{fmt:?}: ndarray shape mismatch"
        );
        assert_eq!(media.format, FrameFormat::Pixel(fmt));

        let back = media.to_avframe()?;
        assert_eq!(back.format, i32::from(fmt));
        assert_eq!(back.width as usize, width);
        assert_eq!(back.height as usize, height);

        // 逐字节比对（按行 linesize 布局）
        unsafe {
            let (src, dst) = (av.data[0], back.data[0]);
            let linesize = av.linesize[0] as usize;
            for y in 0..height {
                for i in 0..width * ch {
                    assert_eq!(
                        *src.add(y * linesize + i),
                        *dst.add(y * linesize + i),
                        "{fmt:?}: byte mismatch at row {y} offset {i}"
                    );
                }
            }
        }
    }
    Ok(())
}

/// RGB24 视频帧 `MediaFrame → AVFrame → MediaFrame` 全量往返：
/// 验证像素数据逐点无损、以及时间戳/格式/时间基等元数据完整保留。
#[test]
fn test_video_rgb24_data_roundtrip() -> Result<()> {
    let width = 65usize; // 非 32 对齐，强制 linesize padding，考察按行拷贝
    let height = 49usize;
    let mut media =
        MediaFrame::<u8>::new_video_frame(width, height, PixelFormat::RGB24, TIME_BASE)?;
    for y in 0..height {
        for x in 0..width {
            media.data[[y, x, 0]] = (x % 256) as u8;
            media.data[[y, x, 1]] = (y % 256) as u8;
            media.data[[y, x, 2]] = ((x + y) % 256) as u8;
        }
    }
    media.set_pts(12345);

    let av = media.to_avframe()?;
    let back = MediaFrame::<u8>::from_avframe(&av)?;

    // 数据无损往返
    assert_eq!(back.data, media.data, "RGB24 像素数据往返出现偏差");
    // 维度
    assert_eq!(back.data.dim(), (height, width, 3));
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

/// YUV420P 视频帧往返：因 `[h,w,3]` 中的 U/V 存储的是 2x2 块的代表值；只有
/// 每个 2x2 色度块取值一致时，downsample（取左上角）→ upsample（填满 2x2）才无损。
/// 该测试锁定这一往返行为，防止色度上下采样在数据搬运层级漂移。
#[test]
fn test_video_yuv420p_data_roundtrip() -> Result<()> {
    let width = 64usize;
    let height = 48usize;
    let mut media =
        MediaFrame::<u8>::new_video_frame(width, height, PixelFormat::YUV420P, TIME_BASE)?;
    // Y：逐像素变化；U/V：每个 2x2 块内取相同值（合法 4:2:0 色度）。
    for y in 0..height {
        for x in 0..width {
            media.data[[y, x, 0]] = (x + y) as u8;
            media.data[[y, x, 1]] = ((x / 2 + y / 2) % 256) as u8;
            media.data[[y, x, 2]] = ((x / 2) % 256) as u8;
        }
    }

    let av = media.to_avframe()?;
    let back = MediaFrame::<u8>::from_avframe(&av)?;

    assert_eq!(back.data.dim(), (height, width, 3));
    // 逐个像素断言，若上下采样对称性被破坏可精确定位
    for y in 0..height {
        for x in 0..width {
            let got = back.data[[y, x, 0]];
            assert_eq!(got, media.data[[y, x, 0]], "Y 平面丢失于 [{y},{x}]");
            let got = back.data[[y, x, 1]];
            assert_eq!(got, media.data[[y, x, 1]], "U 平面丢失于 [{y},{x}]");
            let got = back.data[[y, x, 2]];
            assert_eq!(got, media.data[[y, x, 2]], "V 平面丢失于 [{y},{x}]");
        }
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
    let mut media = MediaFrame::new_audio_frame(
        SampleFormat::FLTP,
        nb_channels,
        nb_samples,
        sample_rate,
        TIME_BASE,
    )?;
    // 填充有区分度的样本（每采样点不同，且声道间不同）
    for s in 0..nb_samples as usize {
        for ch in 0..nb_channels as usize {
            media.data[[0, s, ch]] = (s * 1000 + ch) as f32 / 1000.0;
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
    assert_eq!(
        back.data.dim(),
        (1, nb_samples as usize, nb_channels as usize)
    );
    for s in 0..nb_samples as usize {
        for ch in 0..nb_channels as usize {
            assert_eq!(
                back.data[[0, s, ch]],
                media.data[[0, s, ch]],
                "音频样本丢失 @ [0,{s},{ch}]"
            );
        }
    }

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
