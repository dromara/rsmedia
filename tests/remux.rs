//! RIIR: https://github.com/FFmpeg/FFmpeg/blob/master/doc/examples/remux.c
mod common;
use anyhow::{Context, Result};
use common::test_output_path;
use rsmpeg::{
    avcodec::AVPacket,
    avformat::{AVFormatContextInput, AVFormatContextOutput},
    avutil::{ts2str, ts2timestr},
    ffi::AVRational,
};
use std::ffi::{CStr, CString};

fn log_packet(time_base: AVRational, pkt: &AVPacket, tag: &str) {
    println!(
        "{}: pts:{} pts_time:{} dts:{} dts_time:{} duration:{} duration_time:{} stream_index:{}",
        tag,
        ts2str(pkt.pts),
        ts2timestr(pkt.pts, time_base),
        ts2str(pkt.dts),
        ts2timestr(pkt.dts, time_base),
        ts2str(pkt.duration),
        ts2timestr(pkt.duration, time_base),
        pkt.stream_index
    );
}

fn remux(input_path: &CStr, output_path: &CStr) -> Result<()> {
    let mut input_format_context =
        AVFormatContextInput::open(input_path).context("Create input format context failed.")?;
    input_format_context
        .dump(0, input_path)
        .context("Dump input format context failed.")?;
    let mut output_format_context = AVFormatContextOutput::create(output_path)
        .context("Create output format context failed.")?;
    let stream_mapping: Vec<_> = {
        let mut stream_index = 0usize;
        input_format_context
            .streams()
            .iter()
            .map(|stream| {
                let codec_type = stream.codecpar().codec_type();
                if !codec_type.is_video() && !codec_type.is_audio() && !codec_type.is_subtitle() {
                    None
                } else {
                    output_format_context
                        .new_stream()
                        .set_codecpar(stream.codecpar().clone());
                    stream_index += 1;
                    Some(stream_index - 1)
                }
            })
            .collect()
    };
    output_format_context
        .dump(0, output_path)
        .context("Dump output format context failed.")?;

    output_format_context
        .write_header(&mut None)
        .context("Writer header failed.")?;

    while let Some(mut packet) = input_format_context
        .read_packet()
        .context("Read packet failed.")?
    {
        let input_stream_index = packet.stream_index as usize;
        let Some(output_stream_index) = stream_mapping[input_stream_index] else {
            continue;
        };
        {
            let input_stream = &input_format_context.streams()[input_stream_index];
            let output_stream = &output_format_context.streams()[output_stream_index];
            log_packet(input_stream.time_base, &packet, "in");
            packet.rescale_ts(input_stream.time_base, output_stream.time_base);
            packet.set_stream_index(output_stream_index as i32);
            packet.set_pos(-1);
            log_packet(output_stream.time_base, &packet, "out");
        }
        output_format_context
            .interleaved_write_frame(&mut packet)
            .context("Interleaved write frame failed.")?;
    }
    output_format_context
        .write_trailer()
        .context("Write trailer failed.")
}

/// Remux MP4 to MOV, with h.264 codec.
#[test]
fn remux_test0() {
    let output_path = test_output_path("remux", "mp4.mov");
    let output_path_c = CString::new(output_path.to_string_lossy().as_bytes()).unwrap();
    remux(c"assets/mp4.mp4", &output_path_c).unwrap();
}

/// 高层 API remux 往返：用 `Demuxer::new_passthrough` + `Muxer::add_copy_stream`
/// + `mux_packet` 把 MP4 转封装为 MKV（完全内存内），再读回验证可解封装。
///
/// 覆盖"理想形态 2：remux passthrough"的端到端路径。
#[test]
fn remux_passthrough_roundtrip() {
    use rsmedia::io::{BufferReader, BufferWriter};
    use rsmedia::mux::{Demuxer, Muxer};

    let src = std::fs::read("assets/mp4.mp4").unwrap();
    let reader = BufferReader::new(src).unwrap();
    let mut demuxer = Demuxer::new_passthrough(reader).unwrap();

    // 为每条输入流建立对应的输出透传流。
    let nb_in = demuxer.nb_streams();
    let infos: Vec<_> = (0..nb_in)
        .map(|i| demuxer.stream_info(i).unwrap())
        .collect();
    let mut muxer = Muxer::new_from_writer(BufferWriter::new("matroska").unwrap());
    muxer.set_interleaved(true);
    let dst_indices: Vec<_> = infos
        .iter()
        .map(|info| muxer.add_copy_stream(info).unwrap())
        .collect();

    // 逐一读包写入输出
    while let Some(pkt) = demuxer.demux_packet().unwrap() {
        let (src_idx, mut packet) = pkt;
        muxer.mux_packet(&mut packet, dst_indices[src_idx]).unwrap();
    }
    muxer.finish().unwrap();
    let out_bytes = muxer.into_writer().into_bytes();
    assert!(!out_bytes.is_empty(), "remux produced no output bytes");

    // 读回验证输出容器有效且可解封装
    let vreader = BufferReader::new(out_bytes).unwrap();
    let vdemuxer = Demuxer::new_passthrough(vreader).unwrap();
    assert!(
        vdemuxer.nb_streams() >= 1,
        "remuxed container should contain at least one stream"
    );
}
