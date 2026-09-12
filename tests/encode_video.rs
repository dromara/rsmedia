//! RIIR: https://github.com/FFmpeg/FFmpeg/blob/master/doc/examples/encode_video.c
mod common;
use anyhow::{Context, Result, anyhow};
use common::test_output_path;
use rsmedia::{EncoderBuilder, strutils};
use rsmpeg::{
    avcodec::{AVCodec, AVCodecContext},
    avutil::{AVFrame, opt_set, ra},
    error::RsmpegError,
    ffi,
};
use std::{
    ffi::CStr,
    fs::File,
    io::{BufWriter, Write},
};

const WIDTH: usize = 352;
const HEIGHT: usize = 288;
const FRAME_COUNT: usize = 25;

/// 向 YUV420P 帧填充渐变色块测试图像
fn fill_test_frame(frame: &mut AVFrame, frame_idx: usize) -> Result<()> {
    frame
        .make_writable()
        .context("Failed to make frame writable")?;
    let data = frame.data;
    let linesize = frame.linesize;
    let linesize_y = linesize[0] as usize;
    let linesize_cb = linesize[1] as usize;
    let linesize_cr = linesize[2] as usize;
    let y_data = unsafe { std::slice::from_raw_parts_mut(data[0], HEIGHT * linesize_y) };
    let cb_data = unsafe { std::slice::from_raw_parts_mut(data[1], HEIGHT / 2 * linesize_cb) };
    let cr_data = unsafe { std::slice::from_raw_parts_mut(data[2], HEIGHT / 2 * linesize_cr) };
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            y_data[y * linesize_y + x] = (x + y + frame_idx * 3) as u8;
        }
    }
    for y in 0..HEIGHT / 2 {
        for x in 0..WIDTH / 2 {
            cb_data[y * linesize_cb + x] = (128 + y + frame_idx * 2) as u8;
            cr_data[y * linesize_cr + x] = (64 + x + frame_idx * 5) as u8;
        }
    }
    Ok(())
}

/// 裸流编码：将编码码流直接写入文件（无容器封装）
fn encode(
    encode_context: &mut AVCodecContext,
    frame: Option<&AVFrame>,
    file: &mut BufWriter<File>,
) -> Result<()> {
    encode_context.send_frame(frame)?;
    loop {
        let packet = match encode_context.receive_packet() {
            Ok(packet) => packet,
            Err(RsmpegError::EncoderDrainError) | Err(RsmpegError::EncoderFlushedError) => break,
            Err(e) => return Err(e.into()),
        };
        let data = unsafe { std::slice::from_raw_parts(packet.data, packet.size as usize) };
        file.write_all(data)?;
    }
    Ok(())
}

fn encode_video(codec_name: &CStr, file_name: &str) -> Result<()> {
    let encoder =
        AVCodec::find_encoder_by_name(codec_name).context("Failed to find encoder codec")?;
    let mut encode_context = AVCodecContext::new(&encoder);
    encode_context.set_bit_rate(400000);
    encode_context.set_width(WIDTH as i32);
    encode_context.set_height(HEIGHT as i32);
    encode_context.set_time_base(ra(1, 25));
    encode_context.set_framerate(ra(25, 1));
    encode_context.set_gop_size(10);
    encode_context.set_max_b_frames(1);
    encode_context.set_pix_fmt(ffi::AV_PIX_FMT_YUV420P);
    if encoder.id == ffi::AV_CODEC_ID_H264 {
        // 测试场景使用 ultrafast 预设，兼顾速度与编码验证
        unsafe { opt_set(encode_context.priv_data, c"preset", c"ultrafast", 0) }
            .context("Set preset failed.")?;
    }
    encode_context.open(None).context("Could not open codec")?;

    let mut frame = AVFrame::new();
    frame.set_format(encode_context.pix_fmt);
    frame.set_width(encode_context.width);
    frame.set_height(encode_context.height);
    frame
        .alloc_buffer()
        .context("Could not allocate the video frame data")?;

    let file = File::create(file_name).with_context(|| anyhow!("Could not open: {}", file_name))?;
    let mut writer = BufWriter::new(file);

    for i in 0..FRAME_COUNT {
        fill_test_frame(&mut frame, i)?;
        frame.set_pts(i as i64);
        encode(&mut encode_context, Some(&frame), &mut writer)?;
    }
    encode(&mut encode_context, None, &mut writer)?;

    // MPEG 系裸流以 0x000001B7 结束码收尾；h264 等其它裸流不需要
    if matches!(
        encoder.id,
        ffi::AV_CODEC_ID_MPEG1VIDEO | ffi::AV_CODEC_ID_MPEG2VIDEO | ffi::AV_CODEC_ID_MPEG4
    ) {
        let endcode: [u8; 4] = [0, 0, 1, 0xb7];
        writer.write_all(&endcode).context("Write endcode failed")?;
    }

    writer.flush().context("Flush file failed.")
}

/// 市场上常见通用的视频容器与对应编码器（容器, 编码器名）。
/// 仅保留主流格式：冷门/需硬件设备/未编译的外部编码库不纳入测试。
const COMMON_VIDEO_CONTAINERS: &[(&str, &str)] = &[
    ("mp4", "libx264"),
    ("mkv", "libx264"),
    ("mov", "libx264"),
    ("m4v", "libx264"),
    ("ts", "libx264"),
    ("webm", "libvpx"),
    ("avi", "mpeg4"),
    ("flv", "flv"),
    ("wmv", "wmv2"),
    ("ogv", "libtheora"),
];

/// 使用 rsmedia `EncoderBuilder` 对常见视频容器做编码测试：
/// 25 帧 25fps 的 YUV420P 测试图封装到对应容器。
fn encode_video_container(container_type: &str, codec_name: &str) -> Result<()> {
    // 编码器是否存在取决于 FFmpeg 编译配置（如 libx264、libtheora），
    // 缺失时跳过该容器而不是失败
    if AVCodec::find_encoder_by_name(&strutils::str_to_cstring(codec_name)).is_none() {
        anyhow::bail!("encoder {codec_name} not available in this FFmpeg build");
    }

    let output_path = test_output_path("encode_video", &format!("test.{container_type}"));

    let encoder = EncoderBuilder::new_video(WIDTH, HEIGHT)
        .with_fps(25.0)
        .with_codec_name(codec_name.to_string())
        .build()?;
    let enc_tb = encoder.time_base();
    let mut muxer = rsmedia::mux::Muxer::new(&output_path)?;
    let v_idx = muxer.add_encoder(encoder)?;

    let mut frame = AVFrame::new();
    frame.set_format(ffi::AV_PIX_FMT_YUV420P as _);
    frame.set_width(WIDTH as i32);
    frame.set_height(HEIGHT as i32);
    frame
        .alloc_buffer()
        .context("Could not allocate the video frame data")?;

    for i in 0..FRAME_COUNT {
        fill_test_frame(&mut frame, i)?;
        frame.set_pts(i as i64);
        frame.set_time_base(enc_tb);
        muxer.mux(frame.clone(), v_idx)?;
    }

    // flush encoder and write trailer
    muxer.finish()?;

    Ok(())
}

/// 遍历常见视频容器逐一编码；编码器缺失的容器跳过并报告，
/// 但要求至少一个容器成功，防止环境异常时测试空壳通过。
#[test]
fn test_encode_video_containers() -> Result<()> {
    let mut skipped = Vec::new();
    let mut encoded = 0;

    for (container_type, codec_name) in COMMON_VIDEO_CONTAINERS {
        match encode_video_container(container_type, codec_name) {
            Ok(()) => encoded += 1,
            Err(e) if e.to_string().contains("not available in this FFmpeg build") => {
                skipped.push(*container_type)
            }
            Err(e) => return Err(anyhow!("encode {container_type} failed: {e:#}")),
        }
    }

    println!("encoded {encoded} containers, skipped: {skipped:?}");
    assert!(encoded > 0, "all video container encodings were skipped");
    Ok(())
}

#[test]
fn encode_video_test_h264() {
    if AVCodec::find_encoder_by_name(c"libx264").is_none() {
        println!("skip test: libx264 not available in this FFmpeg build");
        return;
    }
    let output_path = test_output_path("encode_video", "h264.h264");
    encode_video(c"libx264", output_path.to_str().unwrap()).unwrap();
}

#[test]
fn encode_video_test_mpeg4() {
    let output_path = test_output_path("encode_video", "mpeg4.m4v");
    encode_video(c"mpeg4", output_path.to_str().unwrap()).unwrap();
}
