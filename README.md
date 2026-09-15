<h1 align="center">
  <code>rsmedia</code>
</h1>

Low / High-level video & audio toolkit based on [rsmpeg](https://github.com/larksuite/rsmpeg).

FFmpeg 6.x / 7.x / 8.x / 9.x is supported based on [rusty_ffmpeg](https://github.com/CCExtractor/rusty_ffmpeg).

## 🎬 Introduction

`rsmedia` is a general-purpose video/audio media library for Rust that uses the
`libav`-family libraries from `ffmpeg`. It aims to provide a stable and Rusty
interface to many common media tasks, such as reading, writing, muxing, encoding,
decoding, filtering, scaling, resampling, subtitle handling and image processing.

The crate is usable as soon as it is linked. Sources are opened through one uniform abstraction, `Location`, which
accepts a local path, a `PathBuf` or a URL, so `StreamReader::new("/tmp/a.mp4")`
and `StreamReader::new("http://host/a.mp4")` are the same call — as are the
write side (`Muxer::new(...)`, `StreamWriter::new(...)`) and the builders.

## 🖥 Platform support

Legend: 

✅ supported **and** covered by the CI test matrix.

⚠️ supported but **not** CI-covered.

❌ not supported.

| Platform | x86_64 | arm64 | FFmpeg versions | Linking |
|---|:---:|:---:|---|---|
| **Linux** | ✅ | ✅ | 6.1 / 7.1 / 8.1 / 9.0 | pkg-config |
| **macOS** | ⚠️ | ✅ | 6 / 7 / 8 / 9 | pkg-config (Homebrew) |
| **Windows** | ✅ | ✅ | 6.1 / 7.1 / 8.1 / 9.0 | vcpkg / prebuilt |

- ✅ **Tested**: Linux (amd64 + arm64), Windows (amd64 + arm64) and macOS (arm64) run the
  full test matrix on every push — see [ci.yml](.github/workflows/ci.yml).
- ⚠️ **Supported, untested**: macOS x86_64. Homebrew still ships FFmpeg for it and the crate
  builds against it, but GitHub retired its Intel macOS runners, so nothing tests this
  combination automatically — treat breakage as possible, and please open an issue if you
  hit one. Currently the only ⚠️ cell; nothing is ❌.
- MSRV: **Rust 1.89** (edition 2024).

## ✨ Core features

| Feature | What it does | Main entry points |
|---|---|---|
| **Decoding** | Turns video / audio / subtitle streams into Rust data: ndarray-backed frames (`MediaFrame`), raw `AVFrame`s, or subtitle segments. Output pixel/sample format can be unified per builder. | `DecoderBuilder` → `Decoder::decode::<T>`, `decode_raw`, `decode_subtitle_segment` |
| **Encoding** | FFmpeg encoders with FFmpeg-aligned defaults (unset options defer to the codec) and automatic pts numbering. | `EncoderBuilder::new_video` / `new_audio` / `new_subtitle` → `Encoder` |
| **Mux / demux** | Remux or transcode containers; each output stream is either an encoder stream or a bit-exact copy stream. Interleaved writing, metadata, chapters, cover art. | `Demuxer`, `Muxer::add_encoder` / `add_copy_stream` / `mux` / `finish` |
| **IO & seek** | One abstraction for local paths and URLs, with interruptible reads, frame-number and timestamp seeking. | `StreamReader(Builder)`, `StreamWriter(Builder)`, `Seekable`, `Interrupt` |
| **Filters** | FFmpeg filter graphs attached to either builder; typed builders for the common video/audio filters plus an escape hatch for raw specs. | `filter::video::*`, `filter::audio::*`, `Filter`, `.with_filters(...)` |
| **Scaling & resampling** | A reusable scaler (swscale) and resampler (swresample) that hold their kernel/quality policy across frames. | `Scaler`, `ScaleAlgorithm`, `ScaleQuality`, `Resampler`, `resample::convert_frame` |
| **Frames & images** | ndarray-backed frames with full pixel-format conversion (any pair swscale can reach), thumbnails, and color science (HSV/HSL/LAB/XYZ, CIEDE2000). | `MediaFrame::convert_to`, `thumbnail`, `imgutils`, `colors` |
| **Subtitles** | Decode subtitle streams into timed segments, and encode them into any container that carries a subtitle track (ASS/MOV text/...). | `SubtitleSegment`, `Decoder::decode_subtitle_segment`, `Muxer::mux_subtitle_segment`, `EncoderBuilder::new_subtitle` |
| **Hardware acceleration** | Decode/encode on GPU through FFmpeg's hwaccel (VideoToolbox, CUDA/NVENC, VAAPI, QSV, Vulkan, AMF), with device auto-detection and frame download/upload. | `HWDeviceConfig` (`cuda` / `vaapi` / `auto_platform` …), `HWDeviceType`, `.with_hardware_device(...)` |
| **Everything else** | PCM capture/playback, codec/container introspection, option presets. | `PcmSink`, `CodecConfig`, `Options` presets |

The full module map is in the API docs (`cargo doc --open`)

## ⚙️ Cargo features

Pick **exactly one** FFmpeg version feature;

| Feature | Default | Effect |
|---|---|---|
| `ndarray` | ✅ | `MediaFrame` (ndarray-backed frames), `frame::*`, `MediaFrame` conversions, the `decoding`/`encoding` examples |
| `ffmpeg6` | | build against FFmpeg 6.x |
| `ffmpeg7` | | build against FFmpeg 7.x |
| `ffmpeg8` | | build against FFmpeg 8.x |
| `ffmpeg9` | ✅ | build against FFmpeg 9.x |
| `link_system_ffmpeg` | ✅ | link via `pkg-config` (unix) |
| `link_vcpkg_ffmpeg` | | link via `vcpkg` (windows) |

```toml
# FFmpeg 9 on unix (default features are fine)
rsmedia = "0.7"

# FFmpeg 7 on unix
rsmedia = { version = "0.7", default-features = false, features = ["ndarray", "ffmpeg7", "link_system_ffmpeg"] }

# FFmpeg 6 on windows, linking with vcpkg
rsmedia = { version = "0.7", default-features = false, features = ["ndarray", "ffmpeg6", "link_vcpkg_ffmpeg"] }
```

## 📦 Setup

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

Further linking details (static vs. dynamic, custom builds, `FFMPEG_*`
environment variables) live in [`rusty_ffmpeg`](https://github.com/CCExtractor/rusty_ffmpeg)'s
documentation.

## 🚀 Quick start

There is no setup call to make — reach for the type you need and go.

### 1. Decode frames

```rust
use rsmedia::{DecoderBuilder, MediaType, StreamReader};

fn main() -> anyhow::Result<()> {
    // Local path or URL — the same call.
    let mut reader = StreamReader::new("/tmp/test.mp4")?;
    let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;

    // `decode_frame` yields `MediaFrame<u8>` (ndarray-backed; video defaults to
    // planar YUV420P, see `DecoderBuilder::with_pix_fmt`); `decode::<f32>()`
    // does the same for audio.
    while let Some(frame) = decoder.decode_frame(&mut reader)? {
        println!(
            "{}x{} pts={} planes={}",
            frame.width, frame.height, frame.pts, frame.data.num_planes()
        );
    }
    Ok(())
}
```

Need the raw FFmpeg frame instead? Use the raw path — same builder, no ndarray:

```rust
let mut reader = StreamReader::new("/tmp/test.mp4")?;
let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;

while let Some(raw) = decoder.decode_raw(&mut reader)? {
    println!("raw AVFrame: {}x{} {} pts={}", raw.width, raw.height, raw.format, raw.pts);
}
```

### 2. Encode and mux a 🌈 video

```rust
use rsmedia::mux::Muxer;
use rsmedia::{colors, EncoderBuilder, PixelFormat, frame::MediaFrame};

fn main() -> anyhow::Result<()> {
    let (width, height) = (640, 360);

    let encoder = EncoderBuilder::new_video(width, height)
        .with_fps(30.0)                     // encoder time base becomes 1/30
        .with_codec_name("libx264".to_string())
        .build()?;
    let enc_tb = encoder.time_base();

    let mut muxer = Muxer::new("/tmp/rainbow.mp4")?;
    let v_idx = muxer.add_encoder(encoder)?;

    for i in 0..60i64 {
        // No time base to pass: the encoder interprets pts in its own input time
        // base (1/fps here) and overwrites the frame's.
        let mut frame = MediaFrame::<u8>::new_video_frame(width, height, PixelFormat::RGB24)?;
        let rgb = colors::hsv_to_rgb(i as f32 / 60.0 * 360.0, 100.0, 100.0);
        let samples = frame.data.as_packed_mut().expect("RGB24 is interleaved");
        for y in 0..height {
            for x in 0..width {
                for c in 0..3 {
                    samples[[y, x, c]] = rgb[c];
                }
            }
        }

        let mut av = frame.to_avframe()?;
        av.set_pts(i);       // optional: `Encoder` numbers pts itself when left unset
        av.set_time_base(enc_tb);
        muxer.mux(av, v_idx)?;
    }

    muxer.finish()?;
    Ok(())
}
```

### 3. Transcode / transmux

`Demuxer` decodes every stream of the input container and hands the frames to a
`Muxer`, which re-encodes them through the `Encoder` registered per output
stream:

```rust
use rsmedia::{mux::{Demuxer, Muxer}, EncoderBuilder, MediaType, SampleFormat};

fn main() -> anyhow::Result<()> {
    let mut demuxer = Demuxer::new("/tmp/test.mp4")?;
    let mut muxer = Muxer::new("/tmp/output.mov")?;

    let mut in_to_out = Vec::new();
    for stream in demuxer.streams() {
        let info = &stream.stream_info;
        let encoder = match info.media_type {
            MediaType::VIDEO => EncoderBuilder::new_video(
                info.width as usize,
                info.height as usize,
            )
            .build()?,
            MediaType::AUDIO => EncoderBuilder::new_audio(
                info.bit_rate,
                info.channel_layout.nb_channels,
                info.sample_rate,
                info.format.into_sample().unwrap_or(SampleFormat::NONE),
            )
            .build()?,
            _ => continue,
        };
        in_to_out.push((info.index, muxer.add_encoder(encoder)?));
    }

    while let Some((in_index, frame)) = demuxer.demux()? {
        let (_, out_index) = in_to_out
            .iter()
            .find(|(i, _)| *i == in_index)
            .expect("every input stream got an encoder");
        muxer.mux(frame, *out_index)?;
    }

    muxer.finish()?;
    Ok(())
}
```

To copy a stream bit-exactly instead of re-encoding it, register it with
`Muxer::add_copy_stream(&stream_info)` and push packets with `Muxer::mux_packet`.

### 4. Filters

```rust
use rsmedia::{filter, DecoderBuilder, MediaType, StreamReader};

let filters = vec![
    filter::video::scale(1280, 720, Some("bicubic")),
    filter::video::crop(20, 20, 640, 640),
    filter::video::fps(30.0),
    filter::video::hqdn3d(3.0, 2.0), // denoise
];

let reader = StreamReader::new("/tmp/test.mp4")?;
let mut decoder = DecoderBuilder::new(MediaType::VIDEO)
    .with_filters(filters)
    .build_from_reader(&reader)?;

// `decoder` now yields filtered frames (the graph runs inside the decoder).
```

`filter::video` also carries `format`, `drawbox`, `drawtext` (`DrawText`
builder), `delogo`, `zoompan`, `transpose`, `rotate`, `hflip`/`vflip`,
`fade_in`/`fade_out`, `unsharp`, `blur`, `eq`, `yadif`, `pad`, `subtitles`,
`hue`, `negate`, `noise`, `nlmeans`, `gamma`, `saturation`, … and
`filter::audio` the audio side; an escape hatch builds any filter from its spec
string with `Filter::new(name, media_type, spec)`.

## 📖 More examples

The [`examples`](examples) directory is the runnable companion to this README —
every feature above has a worked example there, from the shortest encode path to
color science:

```bash
cargo run --example quick_write     # write /tmp/quick_write.mp4
cargo run --example muxing          # needs /tmp/test.mp4 as input
cargo run --example decoding        # needs the `ndarray` feature (default)
cargo run --example seek_demo -- assets/mp4.mp4
```

Notable ones: [`decode_raw`](examples/decode_raw.rs) (raw `AVFrame` path with
manual draining), [`auto_pts`](examples/auto_pts.rs) (zero-configuration
timestamps), [`pcm_recorder`](examples/pcm_recorder.rs) (audio capture),
[`color_professional`](examples/color_professional.rs) (CIEDE2000, RGB→YUV
matrices), [`hw_vt_check`](examples/hw_vt_check.rs) (VideoToolbox probe), and
[`examples/misc`](examples/misc) (lower-level tutorials: `avio_reading`,
`av_convert`, `thumbnail`, …).

## 🔀 FFmpeg version compatibility

`rsmedia` compiles against FFmpeg 6 → 9 from one source tree. Where the `libav*`
APIs changed, the code is gated per feature, and the gates are checked against
the bindgen dumps kept per release in [`data/ffmpeg-*`](data). In short:
swresample and the `sws_getContext`-based scaler are the same across 6–9; the
modern `sws_scale_frame` path and `SwsContext` fields arrive with FFmpeg 8; the
`SwsScaler`/`SwsBackend` selection types are FFmpeg 9 only, where `ScaleQuality`
gains `STRICT`/`UNSTABLE`.

## 🧪 Tests and benchmarks

```bash
cargo test                     # lib tests + integration suites
cargo test -- --ignored        # hardware suites (VAAPI / CUDA / VideoToolbox)
cargo bench                    # criterion: encode / mux pipelines
cargo clippy --all-targets
```

**Unit tests** live next to the code in `src/**` and cover the core methods in
isolation — format negotiation, time base/bit rate derivation, frame validation,
PTS assignment. **Functional tests** live in `tests/**` and drive the public API
end to end: the encode-side ones generate their own media, the decode/transcode
ones read the small samples in [`assets/`](assets), and hardware suites are
`#[ignore]`d by default.

## 🪲 Debugging

FFmpeg does not always produce useful error messages directly. Turn on `tracing`
to see the extra information it logs:

```toml
[dependencies]
tracing = "0.1"
tracing-subscriber = "0.3"
```

```rust
fn main() {
    tracing_subscriber::fmt::init();
    // ...
}
```

```sh
RUST_LOG=rsmedia=debug cargo run
```

## 🔗 Resources

- [Wiki](https://github.com/dromara/rsmedia/wiki/rsmedia-Home)
  including [hardware acceleration](https://github.com/dromara/rsmedia/wiki/rsmedia-Home#hardware-acceleration)

- FFmpeg: 

  [documentation](https://ffmpeg.org/documentation.html) ·
  [wiki](https://trac.ffmpeg.org/wiki) ·
  [HWAccel intro](https://trac.ffmpeg.org/wiki/HWAccelIntro) ·
  [API docs](https://ffmpeg.org/doxygen/trunk/)

- FFI bindings: 

  [rusty_ffmpeg](https://github.com/CCExtractor/rusty_ffmpeg) ·
  [ffmpeg-the-third](https://github.com/shssoichiro/ffmpeg-the-third) ·
  [ffmpeg-sys-next](https://github.com/zmwangx/rust-ffmpeg-sys)

- Similar crates: 
  
  [avio](https://github.com/itsakeyfut/avio) ·
  [ez-ffmpeg](https://github.com/YeautyYE/ez-ffmpeg) ·
  [video-rs](https://github.com/oddity-ai/video-rs) ·
  [rust-ffmpeg](https://github.com/zmwangx/rust-ffmpeg) ·
  [video_reader-rs](https://github.com/gcanat/video_reader-rs) ·
  [rust-ac-ffmpeg](https://github.com/angelcam/rust-ac-ffmpeg)

## ✨ Credits

`rsmedia` only exists thanks to the following organizations and people:

* All [video-rs contributors](https://github.com/oddity-ai/video-rs/graphs/contributors) for their work!
* All [rsmpeg contributors](https://github.com/larksuite/rsmpeg) for maintaining.
* The [FFmpeg project](https://ffmpeg.org/) for `ffmpeg` and the `ffmpeg` libraries.

## ⚖️ License

Licensed under either of

* Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE.md) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
