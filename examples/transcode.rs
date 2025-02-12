use rsmedia::decode::Decoder;
use rsmedia::encode::Settings;
use rsmedia::{Encoder, Time};
use std::path::Path;

fn main() {

    let source = Path::new("/tmp/copied_video.mp4");
    let mut decoder = Decoder::new(source).expect("failed to create decoder");

    let settings = Settings::preset_h264_yuv420p(1280, 720, false);
    let mut encoder = Encoder::new(Path::new("rainbow.mp4"), settings).expect("failed to create encoder");

    let duration = Time::from_nth_of_a_second(24);
    let mut position = Time::zero();

    for frame in decoder.decode_iter() {
        if let Ok((_, frame)) = frame {
            let rgb = frame.slice(ndarray::s![0, 0, ..]).to_slice().unwrap();
            println!("pixel at 0, 0: {}, {}, {}", rgb[0], rgb[1], rgb[2],);
            encoder.encode(&frame, duration).expect("failed to encode frame");
        } else {
            break;
        }

        // Update the current position and add the inter-frame duration to it.
        position = position.aligned_with(duration).add();
    }

    // for frame in decoder.decode_raw_iter() {
    //     if let Ok(raw_frame) = frame {
    //         println!("frame width: {}, height: {}, pix_format: {}", raw_frame.width, raw_frame.height, raw_frame.format);
    //         encoder.encode_raw(&raw_frame).expect("failed to encode frame");
    //     } else {
    //         break;
    //     }
    // }

    encoder.finish().unwrap();
}