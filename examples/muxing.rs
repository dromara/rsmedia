use rsmedia::{
    hwaccel::HWDeviceType,
    mux::{Demuxer, Muxer},
    EncoderBuilder, MediaType, Options, PixelFormat, SampleFormat, StreamWriterBuilder,
};

use std::path::Path;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339())
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .init();

    rsmedia::init().unwrap();

    let input_path = Path::new("/tmp/test.mp4");
    let mut demuxer = Demuxer::new(input_path).unwrap();

    let output_path = Path::new("/tmp/output.mov");
    let stream_writer = StreamWriterBuilder::new(output_path)
        .with_format("mov")
        .with_options(Options::preset_avformat_fragmented_mov())
        .build()
        .unwrap();
    let mut muxer = Muxer::new_from_writer(stream_writer);

    // add all streams from input to output muxer
    for in_stream in demuxer.streams() {
        let stream_info = &in_stream.stream_info;

        let encoder = {
            if stream_info.media_type == MediaType::VIDEO {
                // build video encoder
                EncoderBuilder::new_video(stream_info.width as usize, stream_info.height as usize)
                    // cuda acceleration
                    .with_hardware_device(Some(HWDeviceType::CUDA.auto_best_config().unwrap()))
                    .with_codec_name("h264_nvenc".to_string())
                    // notes: options must be match with input video encoder codec,
                    .with_options(Options::preset_h264_nvenc())
                    .with_bit_rate(stream_info.bit_rate)
                    // video
                    .with_time_base_ra(stream_info.time_base)
                    .with_frame_rate_ra(stream_info.frame_rate)
                    .with_pixel_format(PixelFormat::from(stream_info.format))
                    .build()
                    .unwrap()
            } else if stream_info.media_type == MediaType::AUDIO {
                // build audio encoder
                EncoderBuilder::new_audio(
                    stream_info.bit_rate,
                    stream_info.channel_layout.nb_channels,
                    stream_info.sample_rate,
                    SampleFormat::from(stream_info.format),
                )
                .build()
                .unwrap()
            } else {
                panic!("Unsupported media type: {:?}", stream_info.media_type);
            }
        };

        let stream_index = muxer.add_stream(encoder).unwrap();
        muxer.dump(stream_index).unwrap()
    }

    // demux and mux all frames from input to output muxer
    loop {
        match demuxer.demux() {
            Ok(Some((stream_index, frame))) => {
                println!("stream index:{}, {:?}", stream_index, frame);
                let _ = muxer.mux(frame, stream_index).unwrap();
            }
            Ok(None) => {
                log::info!("End of input file");
                break;
            }
            Err(e) => {
                eprintln!("Demuxing error: {}", e);
                break;
            }
        }
    }

    // finish muxing
    muxer.finish().unwrap();
}
