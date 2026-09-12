use rsmedia::{
    EncoderBuilder, MediaType, Options, PixelFormat, SampleFormat, StreamWriterBuilder,
    mux::{Demuxer, Muxer},
};

/// This example demonstrates a **transmux + re-encode** pipeline driven purely by
/// [`Muxer`] and [`Demuxer`]:
///
/// 1. A [`Demuxer`] reads and *decodes* every stream from the input container.
/// 2. For each input stream a matching [`Encoder`] is built and handed to a
///    [`Muxer`] via [`Muxer::add_encoder`], which wires it into the output
///    container.
/// 3. Frames decoded in step 1 are fed into the output [`Muxer`] stream with
///    [`Muxer::mux`] (the encoder inside the muxer re-encodes them).
///
/// It stays portable by using FFmpeg's software H.264 encoder (`libx264`, the
/// default), so it runs on any platform without CUDA/nvenc hardware.
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339())
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .init();

    rsmedia::init()?;

    let mut demuxer = Demuxer::new("/tmp/test.mp4")?;

    let stream_writer = StreamWriterBuilder::new("/tmp/output.mov")
        .with_format("mov")
        .with_options(Options::preset_avformat_fragmented_mov())
        .build()?;
    let mut muxer = Muxer::new_from_writer(stream_writer);

    // Build one encoder per input stream and register it with the muxer.
    // `add_encoder` returns the *output* stream index, which may differ from the
    // input stream index, so we keep an explicit input -> output mapping.
    let mut in_to_out: Vec<(usize, usize)> = Vec::new();
    for in_stream in demuxer.streams() {
        let info = &in_stream.stream_info;

        let encoder = if info.media_type == MediaType::VIDEO {
            EncoderBuilder::new_video(info.width as usize, info.height as usize)
                .with_codec_name("libx264".to_string())
                .with_bit_rate(info.bit_rate)
                .with_pixel_format(info.format.into_pixel().unwrap_or(PixelFormat::YUV420P))
                .build()?
        } else if info.media_type == MediaType::AUDIO {
            EncoderBuilder::new_audio(
                info.bit_rate,
                info.channel_layout.nb_channels,
                info.sample_rate,
                info.format.into_sample().unwrap_or(SampleFormat::NONE),
            )
            .build()?
        } else {
            anyhow::bail!(
                "Unsupported media type in input stream {}: {:?}",
                info.index,
                info.media_type
            );
        };

        let out_index = muxer.add_encoder(encoder)?;
        in_to_out.push((info.index, out_index));
        muxer.dump(out_index)?;
    }

    // Demux (decode) one frame at a time and mux (re-encode) it into the output.
    // `demuxer.demux()` reports the *input* stream index; look up the output index.
    loop {
        match demuxer.demux() {
            Ok(Some((in_index, frame))) => {
                let out_index = in_to_out
                    .iter()
                    .find(|(i, _)| *i == in_index)
                    .map(|&(_, o)| o)
                    .ok_or_else(|| {
                        rsmedia::Error::custom(format!(
                            "decoded frame of unmapped input stream {in_index}"
                        ))
                    })?;
                muxer.mux(frame, out_index)?;
            }
            Ok(None) => {
                log::info!("End of input file");
                break;
            }
            Err(e) => {
                eprintln!("Demuxing error: {e}");
                return Err(e.into());
            }
        }
    }

    // Flush the muxer / write the container trailer.
    muxer.finish()?;
    Ok(())
}
