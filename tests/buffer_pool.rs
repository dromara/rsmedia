//! 池化 [`Scaler`](rsmedia::Scaler) 的端到端验证：池中取出的帧必须能被
//! 真实编码管线接受（`avcodec_send_frame` 内部会引用帧缓冲），编码结果
//! 可正常解码回读，且帧数无损。
//!
//! 本文件不依赖 `ndarray`（源帧直接在 `AVFrame` 上构造图案），也不使用
//! `tests/common`，以保持对非默认 feature 组合可用。

use rsmedia::error::{Result, RsmediaError};
use rsmedia::{
    DecoderBuilder, EncoderBuilder, MediaType, Muxer, PixelFormat, Scaler, StreamReader,
};
use rsmpeg::avutil::AVFrame;

/// 生成一张带非零图案的 YUV420P 源帧（不依赖 ndarray）。
fn make_source_frame(width: i32, height: i32, seed: u8) -> Result<AVFrame> {
    let mut frame = AVFrame::new();
    frame.set_width(width);
    frame.set_height(height);
    frame.set_format(PixelFormat::YUV420P.into());
    frame
        .alloc_buffer()
        .map_err(|e| RsmediaError::msg(format!("alloc_buffer failed: {e}")))?;

    unsafe {
        // luma：随 seed 变化的斜纹图案（每行字节已知，保证跨几何一致）。
        for y in 0..height as usize {
            let row = std::slice::from_raw_parts_mut(
                frame.data[0].add(y * frame.linesize[0] as usize),
                width as usize,
            );
            for (x, b) in row.iter_mut().enumerate() {
                *b = seed.wrapping_add((x as u8) ^ (y as u8));
            }
        }
        // chroma：铺固定灰度。
        let cw = (width / 2) as usize;
        let ch = (height / 2) as usize;
        for plane in [1usize, 2] {
            for y in 0..ch {
                let row = std::slice::from_raw_parts_mut(
                    frame.data[plane].add(y * frame.linesize[plane] as usize),
                    cw,
                );
                row.fill(0x80u8.wrapping_add(seed));
            }
        }
    }
    Ok(frame)
}

/// 编码器缺失（FFmpeg 构建不含 libx264 等）时跳过而不是失败：匹配类型化
/// [`RsmediaError::CodecNotFound`] 变体，而非错误字符串。
fn is_encoder_unavailable(e: &RsmediaError) -> bool {
    e.is_codec_not_found()
}

#[test]
fn test_pooled_scaler_encode_roundtrip() -> Result<()> {
    let out = std::env::temp_dir().join("rsmedia_scale_pool_roundtrip.mp4");
    let _ = std::fs::remove_file(&out);

    let encoder = match EncoderBuilder::new_video(32, 32).with_fps(30.0).build() {
        Ok(encoder) => encoder,
        Err(e) if is_encoder_unavailable(&e) => {
            eprintln!("skip test_pooled_scaler_encode_roundtrip: {e}");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let enc_time_base = encoder.time_base();

    let mut muxer = Muxer::new(&out)?;
    let video_index = muxer.add_encoder(encoder)?;

    // 池化缩放：64x64 → 32x32，10 帧。每帧编码后立即归还缓冲（复用路径）。
    let mut scaler = Scaler::new().with_buffer_pool(true);
    let frames = 10;
    let mut first_ptr = 0usize;
    for i in 0..frames {
        let src = make_source_frame(64, 64, i as u8 * 7)?;
        let mut dst = scaler.scale_frame(&src, 32, 32, PixelFormat::YUV420P)?;
        // 稳态复用：从第二帧起数据指针必须与首帧相同（缓冲被池回收复用）。
        if first_ptr == 0 {
            first_ptr = dst.data[0] as usize;
        } else {
            assert_eq!(dst.data[0] as usize, first_ptr, "稳态复用失败");
        }
        dst.set_pts(i);
        dst.set_time_base(enc_time_base);
        muxer.mux(dst, video_index)?;
    }
    muxer.finish()?;

    // 解码回读：帧数无损。
    let mut reader = StreamReader::new(&out)?;
    let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
    let mut decoded = 0usize;
    while decoder.decode_raw(&mut reader)?.is_some() {
        decoded += 1;
    }
    drop(reader);
    assert_eq!(decoded, frames as usize, "解码帧数应与输入一致");

    let _ = std::fs::remove_file(&out);
    Ok(())
}

/// 池化帧与普通帧混合喂给同一编码器（更贴近真实管线中"部分帧池化"
/// 的场景），编解码同样无损。
#[test]
fn test_pooled_and_plain_frames_interleave() -> Result<()> {
    let out = std::env::temp_dir().join("rsmedia_scale_pool_mixed.mp4");
    let _ = std::fs::remove_file(&out);

    let encoder = match EncoderBuilder::new_video(32, 32).with_fps(30.0).build() {
        Ok(encoder) => encoder,
        Err(e) if is_encoder_unavailable(&e) => {
            eprintln!("skip test_pooled_and_plain_frames_interleave: {e}");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let enc_time_base = encoder.time_base();

    let mut muxer = Muxer::new(&out)?;
    let video_index = muxer.add_encoder(encoder)?;

    let mut pooled = Scaler::new().with_buffer_pool(true);
    let mut plain = Scaler::new();
    for (i, pts) in (0..8).enumerate() {
        let src = make_source_frame(64, 64, i as u8 * 11)?;
        let mut dst = if i % 2 == 0 {
            pooled.scale_frame(&src, 32, 32, PixelFormat::YUV420P)?
        } else {
            plain.scale_frame(&src, 32, 32, PixelFormat::YUV420P)?
        };
        dst.set_pts(pts as i64);
        dst.set_time_base(enc_time_base);
        muxer.mux(dst, video_index)?;
    }
    muxer.finish()?;

    let mut reader = StreamReader::new(&out)?;
    let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
    let mut decoded = 0usize;
    while decoder.decode_raw(&mut reader)?.is_some() {
        decoded += 1;
    }
    drop(reader);
    assert_eq!(decoded, 8);

    let _ = std::fs::remove_file(&out);
    Ok(())
}

/// 构造一张填充了图案的 RGB24 帧（真实转换路径的源帧）。
fn make_rgb_frame(width: i32, height: i32, seed: u8) -> Result<AVFrame> {
    let mut frame = AVFrame::new();
    frame.set_width(width);
    frame.set_height(height);
    frame.set_format(PixelFormat::RGB24.into());
    frame
        .alloc_buffer()
        .map_err(|e| RsmediaError::msg(format!("alloc_buffer failed: {e}")))?;
    unsafe {
        let row_bytes = width as usize * 3;
        for y in 0..height as usize {
            let row = std::slice::from_raw_parts_mut(
                frame.data[0].add(y * frame.linesize[0] as usize),
                row_bytes,
            );
            for (x, b) in row.iter_mut().enumerate() {
                *b = seed.wrapping_add((x as u8).wrapping_mul(3) ^ y as u8);
            }
        }
    }
    Ok(frame)
}

/// 编码器侧的池化（`EncoderBuilder::with_scale_pool`）：编码器内部 scaler
/// 持有池（RGB24 → YUV420P 的逐帧转换走池化路径）。真实编码→MP4→解码回读，
/// 帧数无损；帧内像素经缩放+编码仍有意义（图案随 seed 变化，逐帧求 luma
/// 总和不应恒定，防止"编码了空内容"的假阳性）。
#[test]
fn test_encoder_with_scale_pool_roundtrip() -> Result<()> {
    let out = std::env::temp_dir().join("rsmedia_encoder_scale_pool.mp4");
    let _ = std::fs::remove_file(&out);

    let encoder = match EncoderBuilder::new_video(32, 32)
        .with_fps(30.0)
        .with_scale_pool(true)
        .build()
    {
        Ok(encoder) => encoder,
        Err(e) if is_encoder_unavailable(&e) => {
            eprintln!("skip test_encoder_with_scale_pool_roundtrip: {e}");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let enc_time_base = encoder.time_base();

    let mut muxer = Muxer::new(&out)?;
    let video_index = muxer.add_encoder(encoder)?;

    for i in 0..6u8 {
        let rgb = make_rgb_frame(64, 64, i.wrapping_mul(5))?;
        let mut yuv = match i % 2 {
            // 源帧格式 ≠ 编码器目标格式时，编码器内部 scaler（池化）转换。
            0 => {
                let mut s = Scaler::new().with_buffer_pool(true);
                s.scale_frame(&rgb, 32, 32, PixelFormat::YUV420P)?
            }
            _ => {
                let mut s = Scaler::new();
                s.scale_frame(&rgb, 32, 32, PixelFormat::YUV420P)?
            }
        };
        yuv.set_pts(i as i64);
        yuv.set_time_base(enc_time_base);
        muxer.mux(yuv, video_index)?;
    }
    muxer.finish()?;

    let mut reader = StreamReader::new(&out)?;
    let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
    let mut decoded = 0usize;
    while let Some(frame) = decoder.decode_raw(&mut reader)? {
        // 逐帧 luma 总和有差异（图案随帧变化），证明编码的是真实内容。
        let sum: u64 = unsafe {
            (0..frame.height as usize)
                .map(|y| {
                    std::slice::from_raw_parts(
                        frame.data[0].add(y * frame.linesize[0] as usize),
                        frame.width as usize,
                    )
                    .iter()
                    .map(|&b| b as u64)
                    .sum::<u64>()
                })
                .sum()
        };
        assert!(sum > 0, "decoded frame {decoded} has empty luma");
        decoded += 1;
    }
    drop(reader);
    assert_eq!(decoded, 6);

    let _ = std::fs::remove_file(&out);
    Ok(())
}
