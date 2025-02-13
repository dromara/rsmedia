use super::avio;
/// Simplified transcoding test, select the first video stream in given video file
/// and transcode it. Store the output in memory.
use anyhow::{bail, Context, Result};
use rsmpeg::{
    self, avcodec::AVCodecContext, avformat::AVFormatContextOutput, avutil::AVFrame,
    error::RsmpegError, ffi,
};
use std::ffi::CStr;

/// encode -> write_frame
pub fn encode_write_frame(
    frame_after: Option<&AVFrame>,
    encode_context: &mut AVCodecContext,
    output_format_context: &mut AVFormatContextOutput,
    out_stream_index: usize,
) -> Result<()> {
    encode_context
        .send_frame(frame_after)
        .context("Encode frame failed.")?;

    loop {
        let mut packet = match encode_context.receive_packet() {
            Ok(packet) => packet,
            Err(RsmpegError::EncoderDrainError) | Err(RsmpegError::EncoderFlushedError) => break,
            Err(e) => bail!(e),
        };

        packet.set_stream_index(out_stream_index as i32);
        packet.rescale_ts(
            encode_context.time_base,
            output_format_context.streams()[out_stream_index].time_base,
        );

        match output_format_context.interleaved_write_frame(&mut packet) {
            Ok(()) => Ok(()),
            Err(RsmpegError::AVError(-22)) => Ok(()),
            Err(e) => Err(e),
        }
        .context("Interleaved write frame failed.")?;
    }

    Ok(())
}

/// Send an empty packet to the `encode_context` for packet flushing.
pub fn flush_encoder(
    encode_context: &mut AVCodecContext,
    output_format_context: &mut AVFormatContextOutput,
    out_stream_index: usize,
) -> Result<()> {
    // 确定编码器是否支持延迟（delay）
    // 如果编码器不支持延迟，那么就没有必要进行 flush 操作，因为在这种情况下，编码器不会保留任何未处理的数据。
    // 如果编码器支持延迟（delay），则在结束编码之前发送 EOS 包是有必要的，因为编码器可能还在缓冲一些数据，直到接收到 EOS 信号才会处理完这些数据并输出剩余的包。
    if encode_context.codec().capabilities & ffi::AV_CODEC_CAP_DELAY as i32 == 0 {
        return Ok(());
    }

    encode_write_frame(
        None,
        encode_context,
        output_format_context,
        out_stream_index,
    )?;
    Ok(())
}

/// Transcoding audio and video stream in a multi media file.
pub fn transcoding(input_file: &CStr, output_file: &CStr) -> Result<()> {
    let (video_stream_index, mut input_format_context, mut decode_context) =
        avio::open_input_file(input_file)?;
    let (mut output_format_context, mut encode_context) =
        avio::open_output_file(output_file, &decode_context)?;

    loop {
        let mut packet = match input_format_context.read_packet() {
            Ok(Some(x)) => x,
            // No more frames
            Ok(None) => break,
            Err(e) => bail!("Read frame error: {:?}", e),
        };

        if packet.stream_index as usize != video_stream_index {
            continue;
        }

        packet.rescale_ts(
            input_format_context.streams()[video_stream_index].time_base,
            encode_context.time_base,
        );

        decode_context
            .send_packet(Some(&packet))
            .context("Send packet failed")?;

        loop {
            let mut frame = match decode_context.receive_frame() {
                Ok(frame) => frame,
                Err(RsmpegError::DecoderDrainError) | Err(RsmpegError::DecoderFlushedError) => {
                    break
                }
                Err(e) => bail!(e),
            };

            frame.set_pts(frame.best_effort_timestamp);
            encode_write_frame(
                Some(&frame),
                &mut encode_context,
                &mut output_format_context,
                0,
            )?;
        }
    }

    // Flush the encoder by pushing EOF frame to encode_context.
    flush_encoder(&mut encode_context, &mut output_format_context, 0)?;
    output_format_context.write_trailer()?;
    Ok(())
}

pub fn clip_video(
    input_file: &CStr,
    output_file: &CStr,
    start_time: f64,
    duration: f64,
) -> Result<()> {
    let (video_stream_index, mut input_format_context, mut decode_context) =
        avio::open_input_file(input_file)?;

    let (mut output_format_context, mut encode_context) =
        avio::open_output_file(output_file, &decode_context)?;

    let video_stream = &input_format_context.streams()[video_stream_index];
    let start_time_base = video_stream.time_base;
    // Convert c_int to f64 for division
    let (num, den) = (start_time_base.num as f64, start_time_base.den as f64);

    let start_time_ticks = start_time * (den / num);
    let duration_ticks = duration * (den / num);

    println!(
        "time_base: {:?}, start_time_ticks: {:?}, duration_ticks: {:?}",
        start_time_base, start_time_ticks, duration_ticks
    );

    let mut current_time_ticks = 0;

    while let Ok(Some(mut packet)) = input_format_context.read_packet() {
        if packet.stream_index as usize != video_stream_index {
            continue;
        }

        packet.rescale_ts(
            input_format_context.streams()[video_stream_index].time_base,
            encode_context.time_base,
        );

        println!(
            "packet.pts: {:?}, packet.dts: {:?}, current_time_ticks: {:?}",
            packet.pts, packet.dts, current_time_ticks
        );

        // Check if we should skip this packet based on time
        // PTS（Presentation Time Stamp）
        //      定义：pts 指定了帧应该显示的时间点。
        //      用途：通常用于同步不同流（比如视频和音频）的播放时间。播放器会按照这个时间戳来展示帧。
        //      重要性：对于视频和音频的同步至关重要。
        // DTS（Decoding Time Stamp）
        //      定义：dts 指定了帧应该解码的时间点。
        //      用途：通常用于解码器知道何时开始解码一个帧。一些解码器需要知道这个信息来正确处理依赖关系。
        //      重要性：对于解码顺序有影响，特别是对于那些具有 B 帧（B-frame）依赖性的视频编码器来说。
        // 区别：
        //      顺序：在某些情况下，dts 可能比 pts 小，这是因为有些帧需要在逻辑上先解码但后显示（例如 B 帧）。
        //      存在性：并非所有的媒体格式或编码都会同时使用这两个时间戳。有时一个或另一个可能不存在或未定义。
        // 使用场景：
        //      解码：当解码器需要知道解码顺序时，使用 dts。
        //      呈现：当需要知道显示顺序时，使用 pts。
        // 如何选择使用 pts 或 dts ：
        //      在大多数情况下，如果 pts 存在，那么优先使用 pts，因为它更准确地反映了帧应该显示的时间点。如果没有 pts，则可以使用 dts。
        let packet_time = if packet.pts > 0 {
            packet.pts as f64
        } else {
            packet.dts as f64
        };
        if packet_time < start_time_ticks {
            continue;
        }

        // Check if we have reached the end time
        if current_time_ticks as f64 >= start_time_ticks + duration_ticks {
            break;
        }

        decode_context
            .send_packet(Some(&packet))
            .context("Send packet failed")?;

        while let Ok(mut frame) = decode_context.receive_frame() {
            frame.set_pts(frame.best_effort_timestamp);
            current_time_ticks = frame.pts;

            if current_time_ticks >= start_time_ticks as i64 {
                encode_write_frame(
                    Some(&frame),
                    &mut encode_context,
                    &mut output_format_context,
                    0,
                )?;
            }
        }
    }

    // Flush the encoder by pushing EOS packet to encode_context.
    flush_encoder(&mut encode_context, &mut output_format_context, 0)?;
    output_format_context.write_trailer()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cstr::cstr;

    #[test]
    fn avio_writing_test0() {
        std::fs::create_dir_all("tests/output/avio_writing/").unwrap();
        transcoding(
            cstr!("tests/assets/vids/mov_sample.mov"),
            cstr!("tests/output/avio_writing/mov_sample.mp4"),
        )
        .unwrap();
    }

    #[test]
    fn avio_writing_test1() {
        std::fs::create_dir_all("tests/output/avio_writing/").unwrap();
        transcoding(
            cstr!("tests/assets/vids/centaur.mpg"),
            cstr!("tests/output/avio_writing/centaur.mp4"),
        )
        .unwrap();
    }

    #[test]
    fn avio_writing_test2() {
        std::fs::create_dir_all("tests/output/avio_writing/").unwrap();
        transcoding(
            cstr!("tests/assets/vids/bear.mp4"),
            cstr!("tests/output/avio_writing/bear.mp4"),
        )
        .unwrap();
    }

    #[test]
    fn avio_writing_test3() {
        std::fs::create_dir_all("tests/output/avio_writing/").unwrap();
        transcoding(
            cstr!("tests/assets/vids/vp8.mp4"),
            cstr!("tests/output/avio_writing/vp8.mp4"),
        )
        .unwrap();
    }

    #[test]
    fn avio_writing_test4() {
        std::fs::create_dir_all("tests/output/avio_writing/").unwrap();
        transcoding(
            cstr!("tests/assets/vids/big_buck_bunny.mp4"),
            cstr!("tests/output/avio_writing/big_buck_bunny.mp4"),
        )
        .unwrap();
    }

    #[test]
    fn avio_writing_test5() {
        std::fs::create_dir_all("tests/output/avio_writing/").unwrap();
        transcoding(
            cstr!("tests/assets/vids/with_pic.mp4"),
            cstr!("tests/output/avio_writing/with_pic.mp4"),
        )
        .unwrap();
    }

    #[test]
    fn avio_writing_test6() {
        std::fs::create_dir_all("tests/output/avio_writing/").unwrap();
        clip_video(
            cstr!("http://172.24.82.2/video/final_134_raw_2024-09-12-05-04_3980.mp4"),
            cstr!("tests/output/avio_writing/clip_video.mp4"),
            0 as f64,
            1 as f64,
        )
        .unwrap();
    }
}
