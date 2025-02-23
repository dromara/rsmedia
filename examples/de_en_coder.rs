use rsmedia::encode::Settings;
use rsmedia::hwaccel::HWDeviceType;
use rsmedia::{DecoderBuilder, EncoderBuilder, Options, Resize};
use std::path::Path;

fn main() {
    let source = Path::new("/tmp/bear.mp4");
    let mut decoder = DecoderBuilder::new(source)
        .with_resize(Resize::Exact(320, 180))
        .with_options(&Options::preset_h264())
        .with_hardware_device(HWDeviceType::VIDEOTOOLBOX)
        .build()
        .expect("failed to create decoder");

    let settings = Settings::preset_h264_yuv420p(320, 180, false).with_codec_name("h264_videotoolbox".to_string());
    let mut encoder = EncoderBuilder::new(Path::new("/tmp/output.mp4"), settings)
        .with_format("mp4")
        .with_interleaved()
        .with_options(&Options::preset_h264())
        .with_hardware_device(HWDeviceType::VIDEOTOOLBOX)
        .build()
        .expect("failed to create encoder");

    for frame in decoder.decode_raw_iter() {
        if let Ok(raw_frame) = frame {
            println!(
                "frame width: {}, height: {}, pix_format: {}, pts:{}, time_base:{:?}",
                raw_frame.width, raw_frame.height, raw_frame.format, raw_frame.pts, raw_frame.time_base
            );

            encoder.encode_raw(&raw_frame).expect("failed to encode frame");
        } else {
            break;
        }
    }

    encoder.finish().unwrap();
}
