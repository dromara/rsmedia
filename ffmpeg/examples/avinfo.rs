use std::env;
use std::path::Path;

type Result<T> = std::result::Result<T, ffmpeg::Error>;

fn codec_info(input_path: String) -> Result<()> {
    ffmpeg::init().unwrap();
    let input = Path::new(&input_path);

    match ffmpeg::format::input(&input) {
        Ok(context) => {
            for (k, v) in context.metadata().iter() {
                println!("{}: {}", k, v);
            }

            if let Some(stream) = context.streams().best(ffmpeg::media::Type::Video) {
                println!("Best video stream index: {}", stream.index());
            }

            if let Some(stream) = context.streams().best(ffmpeg::media::Type::Audio) {
                println!("Best audio stream index: {}", stream.index());
            }

            if let Some(stream) = context.streams().best(ffmpeg::media::Type::Subtitle) {
                println!("Best subtitle stream index: {}", stream.index());
            }

            println!(
                "duration (seconds): {:.2}",
                context.duration() as f64 / f64::from(ffmpeg::ffi::AV_TIME_BASE)
            );

            for stream in context.streams() {
                println!("stream index {}:", stream.index());
                println!("\ttime_base: {}", stream.time_base());
                println!("\tstart_time: {}", stream.start_time());
                println!("\tduration (stream timebase): {}", stream.duration());
                println!(
                    "\tduration (seconds): {:.2}",
                    stream.duration() as f64 * f64::from(stream.time_base())
                );
                println!("\tframes: {}", stream.frames());
                println!("\tdisposition: {:?}", stream.disposition());
                println!("\tdiscard: {:?}", stream.discard());
                println!("\trate: {}", stream.rate());

                let codec = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?;
                println!("\tmedium: {:?}", codec.medium());
                println!("\tid: {:?}", codec.id());

                if codec.medium() == ffmpeg::media::Type::Video {
                    if let Ok(video) = codec.decoder().video() {
                        println!("\tbit_rate: {}", video.bit_rate());
                        println!("\tmax_rate: {}", video.max_bit_rate());
                        println!("\tdelay: {}", video.delay());
                        println!("\tvideo.width: {}", video.width());
                        println!("\tvideo.height: {}", video.height());
                        println!("\tvideo.format: {:?}", video.format());
                        println!("\tvideo.has_b_frames: {}", video.has_b_frames());
                        println!("\tvideo.aspect_ratio: {}", video.aspect_ratio());
                        println!("\tvideo.color_space: {:?}", video.color_space());
                        println!("\tvideo.color_range: {:?}", video.color_range());
                        println!("\tvideo.color_primaries: {:?}", video.color_primaries());
                        println!(
                            "\tvideo.color_transfer_characteristic: {:?}",
                            video.color_transfer_characteristic()
                        );
                        println!("\tvideo.chroma_location: {:?}", video.chroma_location());
                        println!("\tvideo.references: {}", video.references());
                        println!("\tvideo.intra_dc_precision: {}", video.intra_dc_precision());
                    }
                } else if codec.medium() == ffmpeg::media::Type::Audio {
                    if let Ok(audio) = codec.decoder().audio() {
                        println!("\tbit_rate: {}", audio.bit_rate());
                        println!("\tmax_rate: {}", audio.max_bit_rate());
                        println!("\tdelay: {}", audio.delay());
                        println!("\taudio.rate: {}", audio.rate());
                        println!("\taudio.channels: {}", audio.channels());
                        println!("\taudio.format: {:?}", audio.format());
                        println!("\taudio.frames: {}", audio.frames());
                        println!("\taudio.align: {}", audio.align());
                        println!("\taudio.channel_layout: {:?}", audio.channel_layout());
                    }
                }
            }
        }

        Err(error) => println!("error: {}", error),
    }
    Ok(())
}

fn copy_video(input_path: &str, output_path: &str) -> Result<()> {
    println!("Input video path: {}", input_path);
    println!("Output video path: {}", output_path);

    // Initialize FFmpeg library
    ffmpeg::init().expect("Failed to initialize FFmpeg");

    // Open input video file
    let mut input_format = ffmpeg::format::input(&Path::new(input_path))
        .expect("Failed to open input video file");
    println!("Opened input video file.");

    // Create output format context
    let mut output_format = ffmpeg::format::output(&Path::new(output_path))
        .expect("Failed to create output format context");
    println!("Created output format context.");


    // Copy streams from input to output
    for (_stream_index, stream) in input_format.streams().enumerate() {
        let codec = ffmpeg::codec::encoder::find(ffmpeg::codec::Id::H264)
            .expect("Failed to find codec");
        let mut out_stream = output_format.add_stream(codec)
            .expect("Failed to add stream to output format");

        out_stream.set_parameters(stream.parameters());
    }

    // Write the header to the output file
    output_format.write_header().expect("Failed to write output format header");
    println!("Wrote header to the output file.");

    // Copy packets from input to output
    for (stream, mut packet) in input_format.packets() {
        let input_stream_index = stream.index();
        let output_stream = output_format.stream(input_stream_index)
            .expect("Failed to find corresponding output stream");

        packet.set_stream(output_stream.index());
        packet.rescale_ts(stream.time_base(), output_stream.time_base());

        packet.write_interleaved(&mut output_format)
            .expect("Failed to write packet to output");
    }

    // Write the trailer to the output file
    output_format.write_trailer().expect("Failed to write output trailer");
    println!("Wrote trailer to the output file.");

    Ok(())
}


fn trim_video(input_path: &str, output_path: &str, start_time: f64, duration: f64) -> Result<()> {
    ffmpeg::init().expect("Failed to initialize FFmpeg");

    let mut input_format = ffmpeg::format::input(&Path::new(input_path))
        .expect("Failed to open input video file");
    println!("Opened input video file.");

    // Create output format context
    let mut output_format = ffmpeg::format::output(&Path::new(output_path))
        .expect("Failed to create output format context");
    println!("Created output format context.");

    // Copy streams from input to output
    for (_, stream) in input_format.streams().enumerate() {
        let codec = ffmpeg::codec::encoder::find(ffmpeg::codec::Id::H264)
            .expect("Failed to find codec");
        let mut out_stream = output_format.add_stream(codec)
            .expect("Failed to add stream to output format");

        out_stream.set_parameters(stream.parameters());
    }

    // Write the header to the output file
    output_format.write_header().expect("Failed to write output format header");
    println!("Wrote header to the output file.");


    // Set the trim start and end times (in seconds)
    let trim_end = start_time + duration;
    // let start_pts = (start_time * f64::from(ffmpeg::ffi::AV_TIME_BASE) / f64::from(ffmpeg::ffi::AV_TIME_BASE)) as i64;
    // let end_pts = (trim_end * f64::from(ffmpeg::ffi::AV_TIME_BASE) / f64::from(ffmpeg::ffi::AV_TIME_BASE)) as i64;


    for (stream, mut packet) in input_format.packets() {
        let input_stream_index = stream.index();
        let output_stream = output_format.stream(input_stream_index)
            .expect("Failed to find corresponding output stream");
        // Convert packet timestamp to seconds
        let pts_seconds = packet.pts().unwrap_or(0) as f64 * f64::from(stream.time_base());

        // let time_base = stream.time_base();
        // let pts_seconds = packet.pts().unwrap_or(0) as f64 * f64::from(time_base.numerator()) / f64::from(time_base.denominator());


        // Check if the packet is within the trim range
        if pts_seconds >= start_time && pts_seconds <= trim_end {

            packet.rescale_ts(stream.time_base(), output_stream.time_base());
            packet.set_stream(output_stream.index());

            packet.write_interleaved(&mut output_format)
                .expect("Failed to write packet to output");
        }
        // Stop processing if we've reached the end time
        if pts_seconds > trim_end {
            break;
        }
    }

    // Write the output trailer
    output_format.write_trailer().expect("Failed to write output trailer");

    println!("Video trimmed successfully!");
    Ok(())
}


fn main()-> Result<()>  {
    let input_path = env::args().nth(1).expect("missing input file");

    let output_path1 = "/tmp/trim_video.mp4";
    let output_path2 = "/tmp/copied_video.mp4";

    let start_time = 0.0; // Start time in seconds
    let duration = 10.0;  // Duration in seconds

    trim_video(&input_path, &output_path1, start_time, duration)?;
    let _ = codec_info(input_path.clone());
    let _ = copy_video(&input_path, output_path2);

    Ok(())
}