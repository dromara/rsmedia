<h1 align="center">
  <code>rsmedia</code>
</h1>

Low / High-level video toolkit based on ffmpeg.

ffmpeg 5.x, 6.x, 7.x is supported based [`rusty_ffmpeg`](https://github.com/CCExtractor/rusty_ffmpeg)

## FFI bindings

- [rusty_ffmpeg](https://github.com/CCExtractor/rusty_ffmpeg)
- [rust-ffmpeg](https://github.com/meh/rust-ffmpeg/)
- [ffmpeg-sys-next](https://github.com/zmwangx/rust-ffmpeg-sys)
- [ffmpeg-the-third](https://github.com/shssoichiro/ffmpeg-the-third)

## See also: 
> https://github.com/zmwangx/rust-ffmpeg
> 
> https://github.com/larksuite/rsmpeg
>
> https://github.com/oddity-ai/video-rs
> 
> https://github.com/remotia/remotia-ffmpeg-codecs

## Test on:

### ffmpeg container [jrottenberg/ffmpeg](https://github.com/jrottenberg/ffmpeg):
> ffmpeg:5.1-ubuntu [🟢]
> 
> ffmpeg:6.1-ubuntu [🟢]
>
> ffmpeg:7.1-ubuntu [🟢]

### Architecture:
> ubuntu-latest: [🟢]
> 
> macos-latest: [🟢]
>
> windows-latest: [🟢]

## Status
> ⛔ 格式不正确
> 
> ✔️ 注册成功
>
> ⭕ 成功
> 
> 🔴 构建失败
> 
> 🟢 测试通过


## Advanced usage

1. FFmpeg linking: refer to [`rusty_ffmpeg`](https://github.com/CCExtractor/rusty_ffmpeg)'s documentation for how to use environment variables to statically or dynamically link FFmpeg.

2. Advanced usage of rsmpeg: Check out the `examples` folder.

## usage

```toml
rsmedia = "0.1.0"
```

## Features

- `ndarray`:
Use the `ndarray` feature to be able to use raw frames with the
[`ndarray`](https://github.com/rust-ndarray/ndarray) crate:

```toml
rsmedia = { version = "0.1.0", features = ["ndarray"] }
```

- `ffmpeg5`:
    use `ffmpeg5` feature to enable ffmpeg 5.x

```toml
rsmedia = { version = "0.1.0", features = ["ffmpeg5"] }
```

- `ffmpeg6`:
    use `ffmpeg6` feature to enable ffmpeg 6.x

```toml
rsmedia = { version = "0.1.0", features = ["ffmpeg6"] }
```

- `ffmpeg7`:
    use `ffmpeg7` feature to enable ffmpeg 7.x

```toml
rsmedia = { version = "0.1.0", features = ["ffmpeg7"] }
```

## 📖 Examples

Decode a video and print the RGB value for the top left pixel:

```rust
use std::error::Error;
use rsmedia::decode::Decoder;
use url::Url;
use image::{ImageBuffer, Rgb};
use tokio::task;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>>  {
    rsmedia::init()?;

    let source = "https://img.qunliao.info/4oEGX68t_9505974551.mp4".parse::<Url>().unwrap();
    let mut decoder = Decoder::new(source).expect("failed to create decoder");

    let output_folder = "frames_video_rs";
    std::fs::create_dir_all(output_folder).expect("failed to create output directory");

    let (width, height) = decoder.size();
    let frame_rate = decoder.frame_rate(); // Assuming 30 FPS if not available

    let max_duration = 20.0; // Max duration in seconds
    let max_frames = (frame_rate * max_duration).ceil() as usize;

    let mut frame_count = 0;
    let mut elapsed_time = 0.0;
    let mut tasks = vec![];

    for frame in decoder.decode_iter() {
        if let Ok((_timestamp, frame)) = frame {
            if elapsed_time > max_duration {
                break;
            }

            let rgb = frame.slice(ndarray::s![.., .., 0..3]).to_slice().unwrap();

            let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_raw(width, height, rgb.to_vec())
                .expect("failed to create image buffer");

            let frame_path = format!("{}/frame_{:05}.png", output_folder, frame_count);

            let task = task::spawn_blocking(move || {
                img.save(&frame_path).expect("failed to save frame");
            });

            tasks.push(task);

            frame_count += 1;
            elapsed_time += 1.0 / frame_rate;
        } else {
            break;
        }
    }

    // Await all tasks to finish
    for task in tasks {
        task.await.expect("task failed");
    }

    println!("Saved {} frames in the '{}' directory", frame_count, output_folder);
    Ok(())
}
```

Encode a 🌈 video, using `ndarray` to create each frame:

```rust
use std::path::Path;
use ndarray::Array3;
use rsmedia::encode::{Encoder, Settings};
use rsmedia::time::Time;

fn main() {
    rsmedia::init().unwrap();

    let settings = Settings::preset_h264_yuv420p(1280, 720, false);
    let mut encoder =
        Encoder::new(Path::new("rainbow.mp4"), settings).expect("failed to create encoder");

    let duration: Time = Time::from_nth_of_a_second(24);
    let mut position = Time::zero();
    for i in 0..256 {
        // This will create a smooth rainbow animation video!
        let frame = rainbow_frame(i as f32 / 256.0);

        encoder
            .encode(&frame, position)
            .expect("failed to encode frame");

        // Update the current position and add the inter-frame duration to it.
        position = position.aligned_with(duration).add();
    }

    encoder.finish().expect("failed to finish encoder");
}

fn rainbow_frame(p: f32) -> Array3<u8> {
    // This is what generated the rainbow effect! We loop through the HSV color spectrum and convert
    // to RGB.
    let rgb = hsv_to_rgb(p * 360.0, 100.0, 100.0);

    // This creates a frame with height 720, width 1280 and three channels. The RGB values for each
    // pixel are equal, and determined by the `rgb` we chose above.
    Array3::from_shape_fn((720, 1280, 3), |(_y, _x, c)| rgb[c])
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let s = s / 100.0;
    let v = v / 100.0;
    let c = s * v;
    let x = c * (1.0 - (((h / 60.0) % 2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if (0.0..60.0).contains(&h) {
        (c, x, 0.0)
    } else if (60.0..120.0).contains(&h) {
        (x, c, 0.0)
    } else if (120.0..180.0).contains(&h) {
        (0.0, c, x)
    } else if (180.0..240.0).contains(&h) {
        (0.0, x, c)
    } else if (240.0..300.0).contains(&h) {
        (x, 0.0, c)
    } else if (300.0..360.0).contains(&h) {
        (c, 0.0, x)
    } else {
        (0.0, 0.0, 0.0)
    };
    [
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    ]
}
```
