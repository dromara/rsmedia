<h1 align="center">
  <code>rsmedia</code>
</h1>

Low / High-level video toolkit based on [rsmpeg](https://github.com/larksuite/rsmpeg).

ffmpeg 6.x, 7.x is supported based [rusty_ffmpeg](https://github.com/CCExtractor/rusty_ffmpeg)

## 🎬 Introduction

`rsmedia` is a general-purpose video/audio media library for Rust that uses the
`libav`-family libraries from `ffmpeg`.

It aims to provide a stable and Rusty interface to many common media tasks,
such as reading, writing, muxing, encoding, decoding, Picture Quality Enhancement and Image Processing.

## 🛠 S️️tatus

⚠️ This project is still a work-in-progress, and will contain bugs. Some parts
of the API have not been flushed out yet. Use with caution.

Supported Platforms:

| Platform | Arch    | Linking  | Toolchain   | Build Options | pkg Manager | Support | Notes                          |
|----------|---------|----------|-------------|---------------|-------------|---------|--------------------------------|
| Linux    | x86_64  | Static   | GCC/Clang   | Default       | apt, yum    | ✅       | `pkg-config` + `glibc`        |
|          | x86_64  | Dynamic  | GCC/Clang   | Default       | apt, yum    | ✅       | `pkg-config` + `glibc`        |
|          | aarch64 | Static   | GCC/Clang   | Default       | apt, yum    | ⚠️      | `pkg-config` + `glibc`        |
|          | aarch64 | Dynamic  | GCC/Clang   | Default       | apt, yum    | ⚠️      | `pkg-config` + `glibc`        |
| macOS    | x86_64  | Static   | Apple Clang | ⚠️            | Homebrew    | ❌       | `pkg-config`                  |
|          | x86_64  | Dynamic  | Apple Clang | Default       | Homebrew    | ✅       | `pkg-config`                  |
|          | aarch64 | Static   | Apple Clang | ⚠️            | Homebrew    | ❌       | `pkg-config`                  |
|          | aarch64 | Dynamic  | Apple Clang | Default       | Homebrew    | ✅       | `pkg-config`                  |
| Windows  | x86_64  | Static   | MSVC/MinGW  | `+crt-static` | vcpkg       | ✅       | `vs-2022` + `llvm` + `clang`  |
|          | x86_64  | Dynamic  | MSVC/MinGW  | Default       | vcpkg       | ✅       | `vs-2022` + `llvm` + `clang`  |
|          | aarch64 | Static   | MSVC        | `+crt-static` | vcpkg       | ✅       | `vs-2022` + `llvm` + `clang`  |
|          | aarch64 | Dynamic  | MSVC        | Default       | vcpkg       | ✅       | `vs-2022` + `llvm` + `clang`  |

Hardware acceleration:

| API        | Platform  | Arch    | Hardware Requirements          | Support         | Notes                   |
|------------|-----------|---------|--------------------------------|-----------------|-------------------------|
| VDPAU      | Linux     | x86_64  | NVIDIA GPU                     | ⚠️ Full         | `nvidia-vdpau-driver`   |
|            | Linux     | aarch64 | NVIDIA GPU                     | ⚠️ Full         | Jetson AGX support      |
| CUDA       | Linux     | x86_64  | NVIDIA GPU (Compute ≥3.5)      | ✅ Full          | Container-ready         |
|            | Linux     | aarch64 | NVIDIA GPU (Compute ≥3.5)      | ✅ Full          | Jetson/Orin             |
|            | Windows   | x86_64  | NVIDIA GPU (Compute ≥3.5)      | ✅ Full          |                         |
|            | Windows   | aarch64 | NVIDIA GPU (Compute ≥3.5)      | ⚠️ Partial      | Limited driver support  |
| VAAPI      | Linux     | x86_64  | Intel/AMD/Integrated GPU       | ⚠️ Full         | `intel-media-driver`    |
|            | Linux     | aarch64 | Mali/AMD GPU                   | ⚠️ Partial      | Kernel 5.15+ required   |
| DXVA2      | Windows   | x86_64  | DX11-compatible GPU            | ⚠️ Full         | WDDM 2.0+               |
| QSV        | Linux     | x86_64  | Intel iGPU (≥6th Gen)          | ⚠️ Full         | `intel-media-va-driver` |
|            | Windows   | x86_64  | Intel iGPU (≥6th Gen)          | ⚠️ Full         | Intel Media SDK         |
| TOOLBOX    | macOS     | x86_64  | Intel GPU                      | ✅ Native        | macOS 10.13+            |
|            | macOS     | aarch64 | Apple Silicon GPU (M series)   | ✅ Native        |                         |
| D3D11VA    | Windows   | x86_64  | DX11-compatible GPU            | ⚠️ Full         |                         |
|            | Windows   | aarch64 | DX11-compatible GPU            | ⚠️ Partial      | ARM64 Windows 11        |
| DRM        | Linux     | x86_64  | AMD/NVIDIA GPU                 | ⚠️ Partial      | `libdrm` + KMS          |
|            | Linux     | aarch64 | Mali GPU                       | ⚠️ Partial      |                         |
| MEDIACODEC | Android   | arm64   | Hardware decoder               | ⚠️ Full         | Android 12+             |
| D3D12VA    | Windows   | x86_64  | DX12-compatible GPU            | ⚠️ Experimental | FFmpeg 7.0+             |
|            | Windows   | aarch64 | DX12-compatible GPU            | ⚠️ Experimental | FFmpeg 7.0+             |

> **Note:**
- ✅ Full support / Successful
- ❌ Not support / Failed
- ⚠️ Partially supported / Not clear

## Wiki

- [Home](https://github.com/phial3/rsmedia/wiki/rsmedia-Home)
- [hardware-acceleration](https://github.com/phial3/rsmedia/wiki/rsmedia-Home#hardware-acceleration)

## FFmpeg Documentation

- [Official Documentation](https://ffmpeg.org/documentation.html)
- [FFmpeg WIKI](https://trac.ffmpeg.org/wiki)
- [Hardware acceleration](https://trac.ffmpeg.org/wiki/HWAccelIntro)
- [FFmpeg API Documentation](https://ffmpeg.org/doxygen/trunk/)

## FFI bindingss

- [rusty_ffmpeg](https://github.com/CCExtractor/rusty_ffmpeg)
- [rust-ffmpeg](https://github.com/meh/rust-ffmpeg/)
- [ffmpeg-sys-next](https://github.com/zmwangx/rust-ffmpeg-sys)
- [ffmpeg-the-third](https://github.com/shssoichiro/ffmpeg-the-third)

## See also
>
> <https://github.com/zmwangx/rust-ffmpeg>
>
> <https://github.com/larksuite/rsmpeg>
>
> <https://github.com/oddity-ai/video-rs>


##  📦 Advanced usage

1. FFmpeg linking: refer to [`rusty_ffmpeg`](https://github.com/CCExtractor/rusty_ffmpeg)'s documentation for how to use environment variables to statically or dynamically link FFmpeg.

2. Advanced usage of rsmpeg: Check out the `examples` folder.

## ⚙️ Setup

dynamic linking with pkg-config(unix) or vcpkg(windows):
```bash
export FFMPEG_DIR=/path/to/ffmpeg
export FFMPEG_LIBS_DIR=$FFMPEG_DIR/lib
export FFMPEG_INCLUDE_DIR=$FFMPEG_DIR/include
## (unix recommended):
export FFMPEG_PKG_CONFIG_PATH=$FFMPEG_DIR/lib/pkgconfig
## (windows recommended):
## notes: if you install ffmpeg with vcpkg, you can add `$FFMPEG_DIR/bin` to system PATH.
## manually set dylib path:
## dynamic linking for linux:
export FFMPEG_DLL_PATH=$FFMPEG_LIBS_DIR/libffmpeg.so
## dynamic linking for macos:
export FFMPEG_DLL_PATH=$FFMPEG_LIBS_DIR/libffmpeg.dylib
## dynamic linking for windows:
export FFMPEG_DLL_PATH=$FFMPEG_DIR/lib/libffmpeg.dll
```

## Features

- `ndarray`: enable support to use raw frames with the [`ndarray`](https://github.com/rust-ndarray/ndarray)

- `ffmpeg6`: enable support for `ffmpeg` 6.x.

- `ffmpeg7`: enable support for `ffmpeg` 7.x.

- `link_system_ffmpeg`: unxi system linking ffmpeg with pkg-config.

- `link_vcpkg_ffmpeg`: windows linking ffmpeg with vcpkg.

> usage:
>
> - ffmpeg 7.x for unix:
>
> ```toml
> ## default feature is ok for ffmpeg 7.x unix:
> rsmedia = { git = "https://github.com/phial3/rsmedia", branch = "rsmpeg" }
> ## or like this:
> rsmedia = { git = "https://github.com/phial3/rsmedia", branch = "rsmpeg", default-features = false, features = ["ndarray", "ffmpeg7", "link_system_ffmpeg"] }
> ```
> - ffmpeg 6.x for unix:
> ```toml
> rsmedia = { git = "https://github.com/phial3/rsmedia", branch = "rsmpeg", default-features = false, features = ["ndarray", "ffmpeg6", "link_system_ffmpeg"] }
> ```
> - ffmpeg 7.x for windows:
> ```toml
> rsmedia = { git = "https://github.com/phial3/rsmedia", branch = "rsmpeg", default-features = false, features = ["ndarray", "ffmpeg7", "link_vcpkg_ffmpeg"] }
> ```
> - ffmpeg 6.x for windows:
> ```toml
> rsmedia = { git = "https://github.com/phial3/rsmedia", branch = "rsmpeg", default-features = false, features = ["ndarray", "ffmpeg6", "link_vcpkg_ffmpeg"] }
> ```

## 📖 Examples

### 1. Demux and mux a video:

```rust
use rsmedia::{
  mux::{DemuxResult, Demuxer, Muxer},
  EncoderBuilder, MediaType, Options, PixelFormat, SampleFormat, StreamReader,
  StreamWriterBuilder,
};
use rsmpeg::avcodec::AVCodec;

use anyhow::Context;
use std::path::Path;

fn main() {
  rsmedia::init().unwrap();

  let input_path = Path::new("/tmp/bear.mp4");
  let stream_reader = StreamReader::new(input_path).unwrap();
  let mut demuxer = Demuxer::from_reader(stream_reader, None, None).unwrap();

  let output_path = Path::new("/tmp/output.mov");
  let stream_writer = StreamWriterBuilder::new(output_path)
          .with_format("mov")
          .with_options(Options::preset_avformat_fragmented_mov())
          .build()
          .unwrap();
  let mut muxer = Muxer::from_writer(stream_writer);

  // add all streams from input to output muxer
  for in_stream in demuxer.streams() {
    let stream_info = &in_stream.stream_info;

    let encoder = {
      if stream_info.media_type == MediaType::VIDEO {
        // build video encoder
        let codec = {
          // set custom video codec name, eg: libx264, libx265,
          // Notes: options muse be match with input video encoder codec,
          // Or if you just want to transcode, the codec stay the same,
          // just do get codec from input stream_info.codec_id
          // ```
          // AVCodec::find_encoder(stream_info.codec_id);
          // ```
          // or set by codec name:
          AVCodec::find_encoder_by_name(cstr::cstr!("libx264"))
                  .context("Failed to find decoder")
                  .unwrap()
        };

        EncoderBuilder::new()
                // cuda accel
                // .with_hardware_device(Some(HWDeviceType::CUDA))
                // .with_codec_name("h264_nvenc".to_string())
                // .with_options(Options::preset_h264_nvenc())
                // other
                // notes: options must be match with input video encoder codec,
                .with_options(Some(Options::preset_h264()))
                .with_media_type(stream_info.media_type)
                .with_bit_rate(stream_info.bit_rate)
                .with_codec_name(Some(codec.name().to_str().unwrap().to_string()))
                // video
                .with_video_size(stream_info.width as u32, stream_info.height as u32)
                .with_time_base_ra(stream_info.time_base)
                .with_frame_rate_ra(stream_info.frame_rate)
                .with_pixel_format(PixelFormat::from(stream_info.format))
                .build()
                .unwrap()
      } else if stream_info.media_type == MediaType::AUDIO {
        // build audio encoder
        let codec = {
          // set custom audio codec name, eg: aac, libmp3lame,
          // Notes: options muse be match with input audio encoder codec,
          // Or if you just want to transcode, the codec stay the same,
          // just do get codec from input stream_info.codec_id
          // ```
          // AVCodec::find_encoder(stream_info.codec_id);
          // ```
          // or set by codec name:
          AVCodec::find_encoder_by_name(cstr::cstr!("aac"))
                  .context("Failed to find decoder")
                  .unwrap()
        };

        EncoderBuilder::new()
                // other
                .with_media_type(stream_info.media_type)
                .with_bit_rate(stream_info.bit_rate)
                .with_codec_name(Some(codec.name().to_str().unwrap().to_string()))
                // audio
                .with_nb_channels(stream_info.channel_layout.nb_channels as u32)
                .with_sample_format(SampleFormat::from(stream_info.format))
                .with_sample_rate(stream_info.sample_rate as u32)
                .build()
                .unwrap()
      } else {
        panic!("Unsupported media type: {:?}", stream_info.media_type);
      }
    };

    let _stream_index = muxer.add_stream(encoder).unwrap();
  }

  // demux and mux all frames from input to output muxer
  loop {
    match demuxer.demux() {
      DemuxResult::Frame(stream_index, frame) => {
        println!("stream index:{}, {:?}", stream_index, frame);
        let _ = muxer.mux(frame, stream_index).unwrap();
      }
      DemuxResult::Drain => {
        println!("Need more data, continuing...");
        continue;
      }
      DemuxResult::Flushed => {
        println!("Input stream EOF reached");
        break;
      }
      DemuxResult::Error(e) => {
        eprintln!("Demuxing error: {}", e);
        break;
      }
    }
  }

  // finish muxer
  muxer.finish().unwrap();
}
```

### 2. Decode a video and print the RGB value for the top left pixel:

```rust
use image::{ImageBuffer, Rgb};
use rsmedia::decode::Decoder;
use rsmedia::frame;
use std::error::Error;
use tokio::task;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    rsmedia::init()?;

    let source = "https://img.qunliao.info/4oEGX68t_9505974551.mp4"
        .parse::<Url>()
        .unwrap();
    let mut decoder = Decoder::new(source).expect("failed to create decoder");

    let output_folder = "frames_video_rs";
    std::fs::create_dir_all(output_folder).expect("failed to create output directory");

    let (width, height) = decoder.size();
    let frame_rate = decoder.frame_rate(); // Assuming 30 FPS if not available

    let max_duration = 20.0; // Max duration in seconds
    let _max_frames = (frame_rate * max_duration).ceil() as usize;

    let mut frame_count = 0;
    let mut elapsed_time = 0.0;
    let mut tasks = vec![];

    for frame in decoder.decode_iter() {
        if let Ok((_timestamp, yuv_frame)) = frame {
            if elapsed_time > max_duration {
                break;
            }

            // Notes: yuv frame
            let rgb_frame = frame::convert_ndarray_yuv_to_rgb(&yuv_frame).unwrap();

            let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
                ImageBuffer::from_raw(width, height, rgb_frame.as_slice().unwrap().to_vec())
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

### 3. Encode a 🌈 video, using `ndarray` to create each frame:

```rust
use rsmedia::io::private::{Output, Write};
use rsmedia::time::Time;
use rsmedia::{colors, StreamWriter};
use rsmedia::{EncoderBuilder, FrameArray};

use anyhow::Context;
use rsmedia::stream::StreamInfo;
use std::path::Path;

fn main() {
  rsmedia::init().unwrap();

  let mut encoder = EncoderBuilder::new()
          .with_video_size(1280, 720)
          // use hwaccel cuda
          // .with_hardware_device(HWDeviceType::CUDA)
          // libx264, libx265, h264_nvenc, h264_vaapi etc.
          // .with_codec_name("h264_nvenc".to_string())
          // .with_codec_options(&Options::preset_h264_nvenc())
          .build()
          .expect("failed to create encoder");

  let output_path = Path::new("/tmp/rainbow.mp4");
  let mut stream_writer = StreamWriter::new(output_path).unwrap();
  let video_index = stream_writer.add_stream(encoder.codecpar(), encoder.time_base().into());
  let stream_info = StreamInfo::from_writer(&stream_writer, video_index).unwrap();

  // Write the header to the output file.
  stream_writer.write_header().unwrap();

  let duration: Time = Time::from_nth_of_a_second(24);
  let mut position = Time::zero();

  for i in 0..256 {
    // This will create a smooth rainbow animation video!
    let frame = rainbow_frame(i as f32 / 256.0);

    match encoder.encode(&frame, position) {
      Ok(Some(mut packet)) => {
        packet.set_pos(-1);
        packet.set_stream_index(video_index as i32);
        packet.rescale_ts(encoder.time_base(), stream_info.time_base);
        stream_writer
                .write_frame(&mut packet)
                .context("failed to write frame")
                .unwrap();
      }
      Ok(None) => {
        println!("No packet received from encoder.");
      }
      Err(e) => {
        println!("Error encoding frame: {:?}", e);
      }
    }

    // Update the current position and add the inter-frame duration to it.
    position = position.aligned_with(duration).add();
  }

  encoder.flush().expect("failed to finish encoder");
  stream_writer.write_trailer().unwrap();
}

fn rainbow_frame(p: f32) -> FrameArray {
  // This is what generated the rainbow effect!
  // We loop through the HSV color spectrum and convert to RGB.
  let rgb = colors::hsv_to_rgb(p * 360.0, 100.0, 100.0);

  // This creates a frame with height 720, width 1280 and three channels. The RGB values for each
  // pixel are equal, and determined by the `rgb` we chose above.
  FrameArray::from_shape_fn((720, 1280, 3), |(_y, _x, c)| rgb[c])
}
```

## 🪲 Debugging

Ffmpeg does not always produce useful error messages directly. It is
recommended to turn on tracing if you run into an issue to see if there is
extra information present in the log messages.

Add the following packages to `Cargo.toml`:

```toml
[dependencies]
tracing = "0.1"
tracing-subscriber = "0.3"
```

And add the following to your main functions:

```rust
fn main() {
    tracing_subscriber::fmt::init();

    // ...
}
```

Set the `RUST_LOG` environment variable to display tracing messages:

```sh
RUST_LOG=video=debug cargo run
```

## ✨ Credits

`rsmedia` only exists thanks to the following organizations and people:

* All [video-rs contributors](https://github.com/oddity-ai/video-rs/graphs/contributors) for their work!
* All [rsmpeg contributors](https://github.com/larksuite/rsmpeg) for maintaining.
* The [FFmpeg project](https://ffmpeg.org/) for `ffmpeg` and the `ffmpeg` libraries.

## ⚖️ License

Licensed under either of

* Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.