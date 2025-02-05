use ffmpeg::{rescale, Rescale};
use oidn::{Device, RayTracing};
use rsmedia::{encode::Settings, Decoder, Encoder};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // oidn reference:
    // https://github.com/RenderKit/oidn
    // https://github.com/Twinklebear/oidn-rs
    // export OIDN_DIR=/opt/homebrew/Cellar/open-image-denoise/2.3.2/
    let device;
    let cuda = Device::cuda();
    if cuda.is_some() {
        device = cuda.unwrap();
    } else {
        device = Device::cpu();
    }

    let mut decoder = Decoder::new(Path::new("assets/video.mp4"))?;
    let (width, height) = decoder.size();

    let mut encoder = Encoder::new(
        Path::new("/tmp/denoised.mp4"),
        Settings::preset_h264_yuv420p(width as usize, height as usize, false),
    )?;

    let mut i = 0;
    while let Ok(mut frame) = decoder.decode_raw() {
        i += 1;

        println!("{:?} {i} {:?}", frame.planes(), frame.pts());
        println!(
            "get data pix_fmt:{:?}, size:{}*{}",
            frame.format(),
            frame.width(),
            frame.height()
        );

        let mut data: Vec<f32> = frame
            .data(0)
            .iter()
            .map(|n| *n as f32 / 1000.0 * 3.90625)
            .collect();

        println!("denoise");

        RayTracing::new(&device)
            .srgb(true)
            .image_dimensions(frame.width() as usize, frame.height() as usize)
            .albedo(&mut data)
            .filter_in_place(&mut data)
            .unwrap();

        if let Err(e) = device.get_error() {
            println!("Error denosing image: {}", e.1);
        }

        println!("save");

        frame.data_mut(0).copy_from_slice(
            &data
                .iter()
                .map(|n| (*n / 3.90625 * 1000.0) as u8)
                .collect::<Vec<u8>>(),
        );

        let pts = frame
            .pts()
            .map(|pts| pts.rescale(decoder.time_base(), rescale::TIME_BASE));
        frame.set_pts(pts);

        encoder.encode_raw(frame)?;
    }

    encoder.finish().expect("Failed to finish encoding");

    Ok(())
}
