use rsmedia::decode::Decoder;
use rsmedia::encode::Settings;
use rsmedia::EncoderBuilder;
use std::path::Path;

fn main() {
    let source = Path::new("/tmp/copied_video.mp4");
    let mut decoder = Decoder::new(source).expect("failed to create decoder");

    let settings = Settings::preset_h264_yuv420p(1280, 720, false);
    let mut encoder = EncoderBuilder::new(Path::new("person.mp4"), settings)
        .with_format("mp4")
        .build()
        .expect("failed to create encoder");

    for frame in decoder.decode_raw_iter() {
        if let Ok(raw_frame) = frame {
            println!(
                "frame width: {}, height: {}, pix_format: {}, pts:{}, time_base:{:?}",
                raw_frame.width,
                raw_frame.height,
                raw_frame.format,
                raw_frame.pts,
                raw_frame.time_base
            );

            encoder
                .encode_raw(&raw_frame)
                .expect("failed to encode frame");
        } else {
            break;
        }
    }

    encoder.finish().unwrap();
}
