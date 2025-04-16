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

| API        | Platform  | Arch    | Hardware Requirements          | Support         | Notes                    |
|------------|-----------|---------|--------------------------------|-----------------|--------------------------|
| VDPAU      | Linux     | x86_64  | NVIDIA GPU                     | ⚠️ Full         | `nvidia-vdpau-driver`    |
|            | Linux     | aarch64 | NVIDIA GPU                     | ⚠️ Full         | Jetson AGX support       |
| CUDA       | Linux     | x86_64  | NVIDIA GPU (Compute ≥3.5)      | ✅ Full          | Container-ready          |
|            | Linux     | aarch64 | NVIDIA GPU (Compute ≥3.5)      | ✅ Full          | Jetson/Orin              |
|            | Windows   | x86_64  | NVIDIA GPU (Compute ≥3.5)      | ✅ Full          |                          |
|            | Windows   | aarch64 | NVIDIA GPU (Compute ≥3.5)      | ⚠️ Partial      | Limited driver support   |
| VAAPI      | Linux     | x86_64  | Intel/AMD/Integrated GPU       | ⚠️ Full         | `intel-media-driver`     |
|            | Linux     | aarch64 | Mali/AMD GPU                   | ⚠️ Partial      | Kernel 5.15+ required    |
| DXVA2      | Windows   | x86_64  | DX11-compatible GPU            | ⚠️ Full         | WDDM 2.0+                |
| QSV        | Linux     | x86_64  | Intel iGPU (≥6th Gen)          | ⚠️ Full         | `intel-media-va-driver`  |
|            | Windows   | x86_64  | Intel iGPU (≥6th Gen)          | ⚠️ Full         | Intel Media SDK          |
| TOOLBOX    | macOS     | x86_64  | Intel GPU                      | ✅ Native        | macOS 10.13+             |
|            | macOS     | aarch64 | Apple Silicon GPU (M series)   | ✅ Native        |                          |
| D3D11VA    | Windows   | x86_64  | DX11-compatible GPU            | ⚠️ Full         |                          |
|            | Windows   | aarch64 | DX11-compatible GPU            | ⚠️ Partial      | ARM64 Windows 11         |
| DRM        | Linux     | x86_64  | AMD/NVIDIA GPU                 | ⚠️ Partial      | `libdrm` + KMS           |
|            | Linux     | aarch64 | Mali GPU                       | ⚠️ Partial      |                          |
| MEDIACODEC | Android   | arm64   | Hardware decoder               | ⚠️ Full         | Android 12+              |
| D3D12VA    | Windows   | x86_64  | DX12-compatible GPU            | ⚠️ Experimental | FFmpeg 7.0+              |
|            | Windows   | aarch64 | DX12-compatible GPU            | ⚠️ Experimental | FFmpeg 7.0+              |

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
> 
> <https://github.com/gcanat/video_reader-rs>


##  📦 Advanced usage

1. FFmpeg linking: refer to [`rusty_ffmpeg`](https://github.com/CCExtractor/rusty_ffmpeg)'s documentation for how to use environment variables to statically or dynamically link FFmpeg.

2. Advanced usage of rsmpeg: Check out the `examples` folder.

## ⚙️ Setup

- (1) static linking with pkg-config(unix) or vcpkg(windows):
```bash
## (unix recommended):
export FFMPEG_DIR=/path/to/ffmpeg
export FFMPEG_INCLUDE_DIR=$FFMPEG_DIR/include
export FFMPEG_PKG_CONFIG_PATH=$FFMPEG_DIR/lib/pkgconfig
## (windows recommended):
## notes: if you install ffmpeg with vcpkg, you can add `$FFMPEG_DIR/bin` to system PATH.
export VCPKG_ROOT=/path/to/vcpkg
```

- (2) dynamic linking
```bash
export FFMPEG_DIR=/path/to/ffmpeg
export FFMPEG_INCLUDE_DIR=$FFMPEG_DIR/include
## manually set dylib path
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
fn main() {
  rsmedia::init().unwrap();

  let input_path = Path::new("/tmp/bear.mp4");
  let mut demuxer = Demuxer::new(input_path).unwrap();

  // demux and mux all streams frame
  loop {
    match demuxer.demux() {
      Ok(Some((stream_index, frame))) => {
        println!("stream index:{}, {:?}", stream_index, frame);
      }
      Ok(None) => {
        log::info!("End of input file");
        break;
      }
      Err(e) => {
        eprintln!("Demuxing error: {}", e);
        break;
      }
    }
  }
}
```

### 2. Decode a video and print the RGB value for the top left pixel:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    rsmedia::init()?;

    let source = "https://img.qunliao.info/4oEGX68t_9505974551.mp4"
        .parse::<Url>()
        .unwrap();

    let mut decoder = DecoderBuilder::new(MediaType::VIDEO)
          // decoder with CUDA acceleration
          // .with_hardware_device(Some(HWDeviceType::CUDA.auto_best_config().unwrap()))
          // .with_codec_name(Some("h264_cuvid".to_string()))
          .build_wrapped(source)
          .context("failed to create decoder")?;

  loop {
    match decoder.decode::<u8>() {
      Ok(Some(yuv_frame)) => {
        println!(
          "decoded frame pts: {}, type: {:?}, format:{:?}",
          yuv_frame.pts, yuv_frame.media_type, yuv_frame.format
        );
        
        // processing frame here...
        // process_frame(yuv_frame)?;
      }
      Ok(None) => {
        println!("Decoder has reached the end of the stream");
        break;
      }
      Err(e) => {
        println!("Error decoding frame: {}", e);
        break;
      }
    }
  }
  
  Ok(())
}
```

### 3. Encode a 🌈 video, using `ndarray` to create each frame:

```rust
fn main() -> Result<(), Box<dyn Error>> {
  rsmedia::init().unwrap();

  let output_path = Path::new("/tmp/rainbow.mp4");
  let mut encoder = EncoderBuilder::new_video(width as usize, height as usize)
          // encoder with CUDA acceleration
          // .with_hardware_device(Some(HWDeviceType::CUDA.auto_best_config().unwrap()))
          // libx264, libx265, h264_nvenc, h264_vaapi
          // .with_codec_name(Some("h264_nvenc".to_string()))
          // .with_options(Some(Options::preset_h264_nvenc()))
          .with_filters(Some(filters))
          .build_wrapped(output_path)
          .expect("failed to create encoder");

  let duration: Time = Time::from_nth_of_a_second(24);
  let mut position = Time::zero();

  for i in 0..256 {
    // This will create a smooth rainbow animation video!
    let mut frame = rainbow_frame(width as usize, height as usize, i as f32 / 256.0);

    frame.set_pts(
      position
              .aligned_with_rational(encoder.time_base())
              .into_value()
              .unwrap(),
    );

    encoder.encode(frame)?;

    println!("Encoded frame {} at position {}", i, position);

    // Update the current position and add the inter-frame duration to it.
    position = position.aligned_with(duration).add();
  }

  encoder.finish()?;

  Ok(())
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