use rsmedia::mux::{DemuxResult, Demuxer, Muxer};
use rsmedia::{
    EncoderBuilder, MediaType, PixelFormat, SampleFormat, StreamReader, StreamWriterBuilder,
};
use rsmpeg::avcodec::AVCodec;
use rsmpeg::ffi;

use anyhow::Context;
use std::path::Path;

fn main() {
    let input_path = Path::new("/tmp/bear.mp4");
    let stream_reader = StreamReader::new(input_path).unwrap();
    let mut demuxer = Demuxer::from_reader(stream_reader, None).unwrap();

    let output_path = Path::new("/tmp/output.mov");
    let stream_writer = StreamWriterBuilder::new(output_path)
        .with_format("mov")
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
                    let codec_id = stream_info.codec_id as ffi::AVCodecID;
                    AVCodec::find_encoder(codec_id)
                        .context("Failed to find decoder")
                        .unwrap()
                };

                EncoderBuilder::new()
                    // other
                    .with_media_type(stream_info.media_type)
                    .with_bit_rate(stream_info.bit_rate)
                    .with_codec_name(codec.name().to_string_lossy().to_string())
                    // video
                    .with_video_size(stream_info.width as u32, stream_info.height as u32)
                    .with_time_base(stream_info.time_base.den)
                    .with_frame_rate(stream_info.frame_rate.den)
                    .with_pixel_format(PixelFormat::from(stream_info.format))
                    .build()
                    .unwrap()
            } else if stream_info.media_type == MediaType::AUDIO {
                // build audio encoder
                let codec = {
                    let codec_id = stream_info.codec_id as ffi::AVCodecID;
                    AVCodec::find_encoder(codec_id)
                        .context("Failed to find decoder")
                        .unwrap()
                };

                EncoderBuilder::new()
                    // other
                    .with_media_type(stream_info.media_type)
                    .with_bit_rate(stream_info.bit_rate)
                    .with_codec_name(codec.name().to_string_lossy().to_string())
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
            DemuxResult::Frame {
                stream_index,
                frame,
            } => {
                println!("stream index:{}, {:?}", stream_index, frame);
                let _ = muxer.mux(frame, stream_index).unwrap();
            }
            DemuxResult::NeedMore => {
                println!("Need more data, continuing...");
                continue;
            }
            DemuxResult::Eof => {
                println!("End of stream reached");
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
