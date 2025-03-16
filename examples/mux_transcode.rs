use rsmedia::mux::{Demuxer, Muxer};
use rsmedia::{StreamReader, StreamWriterBuilder};
use rsmpeg::error::RsmpegError;
use std::path::Path;

fn main() {
    let input_path = Path::new("/tmp/bear.mp4");
    let stream_reader = StreamReader::new(input_path).unwrap();
    let mut demuxer = Demuxer::from_reader(stream_reader).unwrap();

    let output_path = Path::new("/tmp/output.mov");
    let stream_writer = StreamWriterBuilder::new(output_path)
        .with_format("mov")
        .build()
        .unwrap();
    let mut muxer = Muxer::from_writer(stream_writer);

    // add all streams from input to output muxer
    for in_stream in demuxer.streams() {
        let _stream_index = muxer
            .add_stream_from_info(in_stream.stream_info.clone())
            .unwrap();
    }

    // demux and mux all frames from input to output muxer
    loop {
        match demuxer.demux() {
            Ok(Some((stream_index, frame))) => {
                println!("stream index:{}, {:?}", stream_index, frame);
                let _ = muxer.mux(frame, stream_index).unwrap();
            }
            Ok(None) => {
                println!("No more packets to demux, Reader exhausted.");
                break;
            }
            Err(e) => {
                if let Some(mpeg_error) = e.downcast_ref::<RsmpegError>() {
                    match mpeg_error {
                        RsmpegError::DecoderDrainError => {
                            continue;
                        }
                        RsmpegError::DecoderFlushedError => {
                            println!("Decoder flushed, no more frames to decode.");
                            break;
                        }
                        _ => {
                            log::error!("Error decoding frame: {}", e);
                            break;
                        }
                    }
                } else {
                    log::error!("Error decoding frame: {}", e);
                    break;
                }
            }
        }
    }

    // finish muxing
    muxer.finish().unwrap();
}
