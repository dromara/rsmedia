use rsmedia::{
    mux::{DemuxResult, Demuxer, Muxer},
    EncoderBuilder, MediaType, Options, PixelFormat, SampleFormat, StreamReader,
    StreamWriterBuilder,
};
use rsmpeg::avcodec::AVCodec;

use anyhow::Context;
use std::path::Path;

fn main() {
    rsmedia::init().unwrap();

    let input_path = Path::new("/tmp/bear.mp4");
    let stream_reader = StreamReader::new(input_path).unwrap();
    let mut demuxer = Demuxer::from_reader(stream_reader, None, None).unwrap();

    let output_path = Path::new("/tmp/output.mov");
    let stream_writer = StreamWriterBuilder::new(output_path)
        .with_format("mov")
        .with_options(Options::preset_avformat_fragmented_mov())
        .build()
        .unwrap();
    let mut muxer = Muxer::from_writer(stream_writer);

    // add all streams from input to output muxer
    for in_stream in demuxer.streams() {
        let stream_info = &in_stream.stream_info;

        let encoder = {
            if stream_info.media_type == MediaType::VIDEO {
                // build video encoder
                let codec = {
                    // set custom video codec name, eg: libx264, libx265,
                    // Notes: options muse be match with input video encoder codec,
                    // Or if you just want to transcode, the codec stay the same,
                    // just do get codec from input stream_info.codec_id
                    // ```
                    // AVCodec::find_encoder(stream_info.codec_id);
                    // ```
                    // or set by codec name:
                    AVCodec::find_encoder_by_name(cstr::cstr!("libx264"))
                        .context("Failed to find decoder")
                        .unwrap()
                };

                EncoderBuilder::new()
                    // cuda acceleration
                    // .with_hardware_device(Some(HWDeviceType::CUDA))
                    // .with_codec_name("h264_nvenc".to_string())
                    // .with_options(Options::preset_h264_nvenc())
                    // notes: options must be match with input video encoder codec,
                    .with_options(Some(Options::preset_h264()))
                    .with_media_type(stream_info.media_type)
                    .with_bit_rate(stream_info.bit_rate)
                    .with_codec_name(Some(codec.name().to_str().unwrap().to_string()))
                    // video
                    .with_video_size(stream_info.width as u32, stream_info.height as u32)
                    .with_time_base_ra(stream_info.time_base)
                    .with_frame_rate_ra(stream_info.frame_rate)
                    .with_pixel_format(PixelFormat::from(stream_info.format))
                    .build()
                    .unwrap()
            } else if stream_info.media_type == MediaType::AUDIO {
                // build audio encoder
                let codec = {
                    // set custom audio codec name, eg: aac, libmp3lame,
                    // Notes: options muse be match with input audio encoder codec,
                    // Or if you just want to transcode, the codec stay the same,
                    // just do get codec from input stream_info.codec_id
                    // ```
                    // AVCodec::find_encoder(stream_info.codec_id);
                    // ```
                    // or set by codec name:
                    AVCodec::find_encoder_by_name(cstr::cstr!("aac"))
                        .context("Failed to find decoder")
                        .unwrap()
                };

                EncoderBuilder::new()
                    // other
                    .with_media_type(stream_info.media_type)
                    .with_bit_rate(stream_info.bit_rate)
                    .with_codec_name(Some(codec.name().to_str().unwrap().to_string()))
                    // audio
                    .with_nb_channels(stream_info.channel_layout.nb_channels as u32)
                    .with_sample_format(SampleFormat::from(stream_info.format))
                    .with_sample_rate(stream_info.sample_rate as u32)
                    .build()
                    .unwrap()
            } else {
                panic!("Unsupported media type: {:?}", stream_info.media_type);
            }
        };

        let _stream_index = muxer.add_stream(encoder).unwrap();
    }

    // demux and mux all frames from input to output muxer
    loop {
        match demuxer.demux() {
            DemuxResult::Frame(stream_index, frame) => {
                println!("stream index:{}, {:?}", stream_index, frame);
                let _ = muxer.mux(frame, stream_index).unwrap();
            }
            DemuxResult::Drain => {
                println!("Need more data, continuing...");
                continue;
            }
            DemuxResult::Flushed => {
                println!("Input stream EOF reached");
                break;
            }
            DemuxResult::Error(e) => {
                eprintln!("Demuxing error: {}", e);
                break;
            }
        }
    }

    // finish muxing
    muxer.finish().unwrap();
}
