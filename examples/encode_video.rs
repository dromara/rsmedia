use rsmedia::encode::Settings;
use rsmedia::time::Time;
use rsmedia::{EncoderBuilder, RawFrame};
use std::path::Path;

fn main() {
    rsmedia::init().unwrap();

    let settings = Settings::preset_h264_yuv420p(1280, 720, false);
    let mut encoder = EncoderBuilder::new(Path::new("rainbow.mp4"), settings)
        .with_format("mp4")
        .build()
        .expect("failed to create encoder");

    let duration: Time = Time::from_nth_of_a_second(30);
    let mut position = Time::zero();

    let mut frame = RawFrame::new();
    frame.set_format(encoder.pix_fmt());
    frame.set_width(encoder.width());
    frame.set_height(encoder.height());
    frame
        .alloc_buffer()
        .expect("Could not allocate the video frame data");

    let height = encoder.height() as usize;

    for i in 0..128 {
        frame
            .make_writable()
            .expect("Failed to make frame writable");

        // prepare colorful frame
        {
            let data = frame.data;
            let linesize = frame.linesize;
            let linesize_y = linesize[0] as usize;
            let linesize_cb = linesize[1] as usize;
            let linesize_cr = linesize[2] as usize;
            let y_data = unsafe { std::slice::from_raw_parts_mut(data[0], height * linesize_y) };
            let cb_data =
                unsafe { std::slice::from_raw_parts_mut(data[1], height / 2 * linesize_cb) };
            let cr_data =
                unsafe { std::slice::from_raw_parts_mut(data[2], height / 2 * linesize_cr) };
            // prepare a dummy image
            for y in 0..height {
                for x in 0..height {
                    y_data[y * linesize_y + x] = (x + y + i * 3) as u8;
                }
            }

            for y in 0..height / 2 {
                for x in 0..height / 2 {
                    cb_data[y * linesize_cb + x] = (128 + y + i * 2) as u8;
                    cr_data[y * linesize_cr + x] = (64 + x + i * 5) as u8;
                }
            }
        }

        let pos = position.into_value().unwrap();
        println!("frame pos:{}", pos);
        frame.set_pts(pos);

        encoder.encode_raw(&frame).expect("failed to encode frame");

        // Update the current position and add the inter-frame duration to it.
        position = position.aligned_with(duration).add();
    }

    encoder.finish().expect("failed to finish encoder");
}
