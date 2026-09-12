<h1 align="center">
  <code>rsmedia</code>
</h1>

Low / High-level video & audio toolkit based on [rsmpeg](https://github.com/larksuite/rsmpeg).

FFmpeg 6.x / 7.x / 8.x / 9.x is supported based on [rusty_ffmpeg](https://github.com/CCExtractor/rusty_ffmpeg).

## 🎬 Introduction

`rsmedia` is a general-purpose video/audio media library for Rust that uses the
`libav`-family libraries from `ffmpeg`.

It aims to provide a stable and Rusty interface to many common media tasks, such
as reading, writing, muxing, encoding, decoding, filtering, scaling, resampling,
subtitle handling, picture quality enhancement and image processing.

No initialisation call is required — the crate is usable as soon as it is
linked. Sources are opened through one uniform abstraction, `Location`, which
accepts a local path, a `PathBuf` or a URL, so `StreamReader::new("/tmp/a.mp4")`
and `StreamReader::new("http://host/a.mp4")` are the same call — as are the
write side (`Muxer::new(...)`, `StreamWriter::new(...)`) and the builders.

## ✨ What's inside

| Capability | API |
|---|---|
| Decode video / audio / subtitle streams | `DecoderBuilder`, `Decoder`, `Decoder::decode::<T>` (ndarray frames), `Decoder::decode_frame`, `Decoder::decode_raw` (raw `AVFrame`), `Decoder::decode_subtitle_segment` |
| Encode video / audio / subtitle | `EncoderBuilder::new_video` / `new_audio` / `new_subtitle`, `Encoder`, `Encoder::encode`, `Encoder::encode_subtitle_segment` |
| Mux / demux, transcode, transmux | `Demuxer`, `Muxer`, `Muxer::add_encoder`, `Muxer::add_copy_stream`, `Muxer::mux`, `Muxer::mux_packet`, `Muxer::finish` |
| Streaming IO, network sources, seek | `StreamReader` / `StreamReaderBuilder`, `StreamWriter` / `StreamWriterBuilder`, `Seekable`, `Location` |
| Filter graphs | `filter::video::*`, `filter::audio::*`, `Filter`, `Filter`, `.with_filters(...)` on both builders |
| Scaling (swscale) | `Scaler`, `ScaleAlgorithm` (one kernel), `ScaleQuality` (a set of flags), `scale::scale_frame` |
| Resampling (swresample) | `Resampler`, `resample::convert_frame` |
| Subtitles | `SubtitleSegment`, `Muxer::mux_subtitle_segment`, `EncoderBuilder::new_subtitle` |
| Audio capture / PCM writing | `PcmSink`, `PcmSpec` |
| Metadata, chapters, cover art | `Muxer::set_metadata`, `set_stream_metadata`, `Chapter`, `Muxer::add_chapter`, `Muxer::add_cover_art` |
| Hardware acceleration | `HWDeviceConfig`, `HWDeviceType`, `HWContext` |
| Frames, images, color | `MediaFrame` (ndarray), `FrameFormat` / `SampleFormat`, `imgutils`, `colors`, `PixelFormat`, `thumbnail` |
| Timestamps | `time::Time`, automatic pts numbering inside `Encoder` |
| Codec / container introspection | `CodecConfig`, `FormatInfo`, `Profile`, `stream::StreamInfo` |
| Options presets | `Options::preset_h264`, `preset_h264_realtime`, `preset_h264_nvenc`, `preset_avformat_fragmented_mov`, `preset_avformat_flv`, `preset_avformat_rtsp_transport_tcp`, `Metadata`, `Quality`, `VideoProfile` |

## 🛠 Status

Please use the latest release version.

Currently supported:

**FFmpeg 6 / 7 / 8 / 9** on **macOS, Linux, Windows** (x86_64 / arm64).

MSRV: **Rust 1.89** (edition 2024).

## ⚙️ Features

Pick **exactly one** FFmpeg version feature; the crate's version gates treat them
as alternatives (`ffmpeg6/7` = the legacy swscale path, `ffmpeg8/9` = the modern
one), so enabling two of them at once is not supported.

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
rsmedia = "0.4"

# FFmpeg 7 on unix
rsmedia = { version = "0.4", default-features = false, features = ["ndarray", "ffmpeg7", "link_system_ffmpeg"] }

# FFmpeg 6 on windows, linking with vcpkg
rsmedia = { version = "0.4", default-features = false, features = ["ndarray", "ffmpeg6", "link_vcpkg_ffmpeg"] }
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

    // `decode_frame` yields `MediaFrame<u8>` (ndarray-backed, RGB24 for video);
    // `decode::<f32>()` does the same for audio.
    while let Some(frame) = decoder.decode_frame(&mut reader)? {
        println!(
            "{}x{} pts={} channels={}",
            frame.width, frame.height, frame.pts, frame.data.shape()[2]
        );
    }
    Ok(())
}
```

Need the raw FFmpeg frame instead? Use the raw path — same builder, no ndarray:

```rust
use rsmedia::{DecoderBuilder, MediaType, StreamReader};

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
        let mut frame =
            MediaFrame::<u8>::new_video_frame(width, height, PixelFormat::RGB24, enc_tb)?;
        let rgb = colors::hsv_to_rgb(i as f32 / 60.0 * 360.0, 100.0, 100.0);
        for y in 0..height {
            for x in 0..width {
                for c in 0..3 {
                    frame.data[[y, x, c]] = rgb[c];
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

### 5. Scaling and resampling

The scaler is a standalone, reusable component: it holds the **policy** (one
`ScaleAlgorithm` kernel plus a set of `ScaleQuality` flags) and binds its
`SwsContext` lazily to the first frame it sees, rebuilding whenever the geometry
or pixel format changes. `Encoder` and `Decoder` each hold one, configured
through their builders:

```rust
use rsmedia::{DecoderBuilder, MediaType, Resize, ScaleAlgorithm, ScaleQuality, Scaler};

// Standalone: one context, reused for a whole stream.
let mut scaler = Scaler::new_with_options(
    ScaleAlgorithm::LANCZOS,
    [
        ScaleQuality::FULL_CHR_H_INT,
        ScaleQuality::ACCURATE_RND,
        ScaleQuality::BITEXACT,
    ],
);
// ... scaler.scale_frame(&frame, width, height, PixelFormat::RGB24)?

// Through a builder: resize + kernel + quality flags.
let decoder = DecoderBuilder::new(MediaType::VIDEO)
    .with_scale_algorithm(ScaleAlgorithm::BILINEAR)
    .with_scale_quality([ScaleQuality::BITEXACT])
    .with_resize(Resize::Exact(640, 360))
    .build("/tmp/test.mp4")?;
```

Note the asymmetry FFmpeg itself defines: **the algorithm is one bit** ("Scaler
selection options. Only one may be active at a time.") while **the quality flags
are a set** — they are passed as a list of `ScaleQuality` values and combined by
`ScaleQuality::mask`, so no invalid flag can be expressed.
`ScaleQuality::default_quality()` gives the FFmpeg-recommended baseline, and the
stateless helper `scale::scale_frame(&frame, w, h, PixelFormat::RGB24)` converts a
single frame without keeping a context.

Audio resampling is the sibling component:

```rust
use rsmedia::Resampler;

let mut resampler = Resampler::new(
    src.ch_layout, src.format, src.sample_rate,     // input
    out_layout, out_fmt, 48_000,                    // output
)?;
// dst must have format/layout/sample_rate/nb_samples set and a buffer allocated
resampler.convert_frame(&src, &mut dst)?;
// resampling buffers a tail internally — drain it at the end of the stream
resampler.flush(&mut dst)?;
```

### 6. Subtitles

```rust
use rsmedia::{EncoderBuilder, Muxer, SubtitleSegment};

// `ass_header` is the ASS script header ("[Script Info]" / "[V4+ Styles]" /
// "[Events]" format line) — required by text subtitle encoders. When transcoding,
// forward it from the decoded subtitle stream instead of hand-crafting one.
let ass_header: String = String::new();

let mut muxer = Muxer::new("/tmp/subtitled.mp4")?;
let video = muxer.add_encoder(EncoderBuilder::new_video(320, 240).build()?)?;
let subs = muxer.add_encoder(
    EncoderBuilder::new_subtitle()
        .with_codec_name("mov_text".to_string())    // MP4 subtitle codec
        .with_subtitle_header(ass_header)
        .build()?,
)?;
muxer.set_stream_metadata(subs, "language", "eng")?;

// One call per cue; time base is milliseconds.
muxer.mux_subtitle_segment(&SubtitleSegment::new(0, 1_200, "Hello, world"), subs)?;
muxer.finish()?;
```

Decoding subtitles uses the mirror API:

```rust
use rsmedia::{DecoderBuilder, MediaType, StreamReader};

let mut reader = StreamReader::new("/tmp/subtitled.mp4")?;
let mut decoder = DecoderBuilder::new(MediaType::SUBTITLE)
    .with_codec_name("mov_text".to_string())
    .build_from_reader(&reader)?;

while let Some(cue) = decoder.decode_subtitle_segment(&mut reader)? {
    println!("{}-{}ms: {}", cue.start_ms, cue.end_ms, cue.text);
}
```

### 7. Metadata, chapters, cover art

```rust
use rsmedia::{Chapter, EncoderBuilder, Muxer};

let mut muxer = Muxer::new("/tmp/tagged.mp4")?;
muxer.set_metadata("title", "My movie")?;
let v_idx = muxer.add_encoder(EncoderBuilder::new_video(640, 360).build()?)?;
muxer.set_stream_metadata(v_idx, "language", "eng")?;
muxer.add_chapter(Chapter::new("Intro", 0.0, 10.0))?;
// muxer.add_cover_art(cover_frame)?;  // a YUV420P `AVFrame`
muxer.finish()?;
```

`Muxer::add_cover_art` attaches a picture stream with the container default
(`mjpeg`); `Muxer::add_cover_art_with` lets you supply your own cover encoder.
`Demuxer::streams()`, `Demuxer::chapters()`, `StreamInfo` and the `Decoder`
getters (`width`, `height`, `pix_fmt`, `sample_rate`, `ch_layout`, `duration`,
`frame_rate`, `frames`, …) cover the read side.

### 8. Seek

```rust
use rsmedia::io::{AVSeekFlag, Seekable};
use rsmedia::StreamReader;

let mut reader = StreamReader::new("/tmp/test.mp4")?;

// Cheap pre-check of the IO layer.
if reader.is_byte_seekable() {
    // Milliseconds; lands on the nearest keyframe.
    reader.seek_to_timestamp(10_000)?;
}

// The low-level equivalent takes `(stream index, frame_ts, flags)`, where
// `frame_ts` is a timestamp in the *stream* time base, or a frame index when
// `AVSeekFlag::FRAME` is set.
reader.seek_to_frame(0, 300, AVSeekFlag::BACKWARD)?;

reader.seek_to_start()?;
```

Seeking works the same for a local file and an HTTP/RTSP source — whether a seek
succeeds depends on the runtime source, so every call reports failure through
`Result`. Support is also source-dependent for the *kind* of seek:
`AVSeekFlag::FRAME` (frame-index seeking) is honoured by raw demuxers such as
`.h264` / `.hevc` / raw PCM, but not by MP4/MOV/MKV, which index by timestamp —
prefer `seek_to_timestamp` for containers. Seeking is best-effort: it lands on
the nearest keyframe unless `AVSeekFlag::ANY` is combined.

### 9. Thumbnails and images

```rust
use rsmedia::thumbnail;

// decode 5s in, fit within 320x180, as an `image::DynamicImage`
let image = thumbnail("/tmp/test.mp4", Some(5_000), (320, 180))?;
image.save("/tmp/thumb.png")?;
```

`MediaFrame` converts in every direction (`to_avframe` / `from_avframe`,
`to_dynamic_image` / `from_dynamic_image`, explicit `convert_rgb_to_yuv_with_matrix`),
`imgutils` works on raw planes (`copy_frame_to_buffer`, `fill_frame_from_buffer`,
`to_dynamic_image`, …) and `colors` provides HSV/HSL/LAB/XYB/XYZ conversions plus
`color_delta_e` (CIEDE2000).

### 10. Hardware acceleration

```rust
use rsmedia::{DecoderBuilder, EncoderBuilder, HWDeviceConfig, HWDeviceType, MediaType};

// Detect a device, trying CUDA first and VAAPI second. Use
// `HWDeviceConfig::auto_platform()` to simply take the platform's best device
// (VideoToolbox on macOS, VAAPI/QSV on Linux, …), or `HWDeviceConfig::cuda(None)`
// to name one explicitly (`None` = that device type's default device).
let hw = HWDeviceConfig::auto_platform_with(&[HWDeviceType::CUDA, HWDeviceType::VAAPI])?;

let mut decoder = DecoderBuilder::new(MediaType::VIDEO)
    .with_hardware_device(Some(hw))
    .with_codec_name("h264_cuvid".to_string())   // device-specific decoder name
    .build_from_reader(&reader)?;

// Encode with nvenc on a CUDA device:
let encoder = EncoderBuilder::new_video(1280, 720)
    .with_hardware_device(Some(HWDeviceConfig::cuda(None)))
    .with_codec_name("h264_nvenc".to_string())
    .build()?;
```

`HWDeviceConfig` also has `vaapi` / `qsv` / `vulkan` / `amf` constructors taking
an optional device id, and `HWContext` exposes `hw_upload` / `hw_download` when
you need to move frames between the device and system memory yourself.

### 11. Audio capture

```rust
use rsmedia::{PcmSink, PcmSpec};

let spec = PcmSpec::new(48_000, 2);
let mut sink = PcmSink::new(muxer, audio_stream_idx, spec)?;
sink.write_f32(&interleaved_samples)?; // or write_i16 / write_u8
sink.finish()?;                       // drains the internal resampler
```

`PcmSink` owns a persistent `Resampler`, created lazily from the first buffer's
actual format, so any input sample format/rate can be written into a fixed PCM
output.

### 12. Presets

```rust
use rsmedia::{EncoderBuilder, Options, StreamWriterBuilder};

let writer = StreamWriterBuilder::new("/tmp/out.mov")
    .with_format("mov")
    .with_options(Options::preset_avformat_fragmented_mov())
    .build()?;

let encoder = EncoderBuilder::new_video(1920, 1080)
    // .with_options(Options::preset_h264_realtime())
    .build()?;
```

## 🧩 API overview

| Module | Contents |
|---|---|
| `decode` | `DecoderBuilder`, `Decoder`, `thumbnail` |
| `encode` | `EncoderBuilder`, `Encoder` |
| `mux` | `Demuxer`, `Muxer`, `MuxerStream`, `DemuxerStream`, `Chapter` |
| `io` | `StreamReader(Builder)`, `StreamWriter(Builder)`, `BufferReader/Writer`, `Reader`, `Writer`, `Seekable`, `AVSeekFlag`, `Interrupt` |
| `filter` | `Filter` (any spec), `filter::video::*`, `filter::audio::*`, `FilterGraph` |
| `scale` | `Scaler`, `ScaleAlgorithm`, `ScaleQuality`, `SwsDither/SwsAlphaBlend/SwsScaler/SwsIntent/SwsBackend` (version-dependent), `scale_frame`, `scale_with_flags` |
| `resample` | `Resampler`, `convert`, `convert_frame` |
| `frame` | `MediaFrame`, `MediaFrameType`, `FrameSideData` (feature `ndarray`) |
| `subtitle` | `SubtitleSegment` |
| `pcm` | `PcmSink`, `PcmSpec` |
| `imgutils` | frame/plane ⇄ buffer, image conversion helpers |
| `colors` | `Color`, HSV/HSL/LAB/XYB/XYZ conversions, CIEDE2000 |
| `pixel` / `fmt` | `PixelFormat`, `FrameFormat`, `SampleFormat` |
| `stream` | `StreamInfo`, `MediaType` |
| `codec` | `CodecConfig`, `FormatInfo`, `Profile`, `AVCodecFlag` |
| `hwaccel` | `HWDeviceConfig`, `HWDeviceType`, `HWContext` |
| `options` | `Options` (+ presets), `Metadata`, `Quality`, `VideoProfile` |
| `resize` | `Resize` strategies (`Exact`, `Fit`, `FitEven`) |
| `location` | `Location`, `Url` |
| `time` | `Time`, rational helpers |
| `strutils`, `error` | string/`CStr` helpers, `RsmediaError`, `Result`, `Error` |

## 📖 Examples

The [`examples`](examples) directory is the runnable companion to this README:

| Example | Shows |
|---|---|
| [`quick_write`](examples/quick_write.rs) | the shortest encode path: `EncoderBuilder` + `Muxer`, one call per frame |
| [`encoding`](examples/encoding.rs) | `Encoder` + `StreamWriter` with a filter chain and generated rainbow frames |
| [`decoding`](examples/decoding.rs) | `Decoder` + `StreamReader`, filters, seek, hardware device, PNG dump (async) |
| [`decode_raw`](examples/decode_raw.rs) | the raw `AVFrame` path, including manual packet feeding and draining |
| [`decode_streams`](examples/decode_streams.rs) | video, audio and subtitle decoding side by side |
| [`muxing`](examples/muxing.rs) | `Demuxer` → per-stream `Encoder` → `Muxer` transcode/transmux |
| [`auto_pts`](examples/auto_pts.rs) | zero-configuration timestamps for video and audio |
| [`filter_demo`](examples/filter_demo.rs) | a tour of the `filter::video` / `filter::audio` palette |
| [`seek_demo`](examples/seek_demo.rs) | seeking a local file and an HTTP/RTSP source |
| [`metadata`](examples/metadata.rs) | `StreamInfo` and the `Decoder` getters |
| [`audio_encoding`](examples/audio_encoding.rs) | audio encode (AAC/m4a) and decode round trip |
| [`pcm_recorder`](examples/pcm_recorder.rs), [`audio_recorder`](examples/audio_recorder.rs) | capture/playback driven by `PcmSink` and `cpal`/`rodio` |
| [`color_professional`](examples/color_professional.rs), [`colorous_render`](examples/colorous_render.rs) | color science: CIEDE2000, RGB→YUV matrices, colormaps |
| [`hw_vt_check`](examples/hw_vt_check.rs) | probe VideoToolbox availability |
| [`examples/misc`](examples/misc) | lower-level tutorials: `avio_reading`/`avio_writing`, `image_dump`, `thumbnail`, `av_convert`, `av_spliter` |

```bash
cargo run --example quick_write     # write /tmp/quick_write.mp4
cargo run --example muxing          # needs /tmp/test.mp4 as input
cargo run --example decoding        # needs the `ndarray` feature (default)
cargo run --example seek_demo -- assets/mp4.mp4
```

## 🔀 FFmpeg version compatibility

`rsmedia` compiles against FFmpeg 6 → 9 from one source tree. Where libav*
changed its API, the code is gated per feature, and the gates are checked
against the bindgen dumps kept per release in
[`data/ffmpeg-*`](data) (`binding.rs` per version):

| Symbol / field | Available from |
|---|---|
| `SWS_FAST_BILINEAR` … `SWS_SPLINE`, `SWS_PRINT_INFO`, `SWS_FULL_CHR_H_INT/INP`, `SWS_ACCURATE_RND`, `SWS_BITEXACT` | FFmpeg 6 |
| `sws_getContext`-based (legacy) scaler initialisation — the path used on 6/7 | FFmpeg 6 |
| `SwsContext` visible fields (`flags`, `threads`, `dither`, `alpha_blend`, `intent`), `SWS_STRICT`, `SWS_UNSTABLE`, `SWS_DITHER_*`, `SWS_ALPHA_BLEND_*`, `SWS_INTENT_*`, `SwsDither`, `SwsAlphaBlend`, `SwsIntent`, the `sws_alloc_context`/`sws_scale_frame` modern path — used on 8/9 | FFmpeg 8 |
| `SwsContext.scaler` / `scaler_sub` / `backends`, `SWS_SCALE_*`, `SWS_BACKEND_*`, `SwsScaler`, `SwsBackend` | FFmpeg 9 |
| swresample (`SwrContext`, `swr_alloc_set_opts2`, `swr_convert_frame`, `AVChannelLayout`, …) | FFmpeg 6 (unchanged across 6-9) |

So `ScaleQuality::STRICT`/`UNSTABLE` are compiled only for 8/9, and
`Scaler`'s explicit `SwsScaler`/`SwsBackend` selection types only for 9; on
FFmpeg 6/7 the scaling kernel is chosen through `ScaleAlgorithm` as before.

## 🧪 Tests and benchmarks

```bash
cargo test                     # lib tests + integration suites
cargo test -- --ignored        # hardware suites (VAAPI / CUDA / VideoToolbox)
cargo bench                    # criterion: encode / mux pipelines
cargo clippy --all-targets
```

The suites are split by what they need. **Unit tests** live next to the code in
`src/**` and cover the core methods in isolation — format negotiation, time
base/bit rate derivation, frame validation, PTS assignment — with no disk I/O;
they compile without the `ndarray` feature. **Functional tests** live in
`tests/**` and drive the public API end to end; the encode-side ones generate
their own media (gradient video, sine audio) into temporary files, the
decode/transcode ones read the small samples in [`assets/`](assets)
(`mp4.mp4`, `wav.wav`, `cat.jpg`), and hardware suites are `#[ignore]`d by
default (run them with `cargo test -- --ignored`).

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
RUST_LOG=rsmedia=debug cargo run
```

## Wiki

- [Home](https://github.com/dromara/rsmedia/wiki/rsmedia-Home)
- [hardware-acceleration](https://github.com/dromara/rsmedia/wiki/rsmedia-Home#hardware-acceleration)

## FFmpeg Documentation

- [Official Documentation](https://ffmpeg.org/documentation.html)
- [FFmpeg WIKI](https://trac.ffmpeg.org/wiki)
- [Hardware acceleration](https://trac.ffmpeg.org/wiki/HWAccelIntro)
- [FFmpeg API Documentation](https://ffmpeg.org/doxygen/trunk/)

## FFI bindings

- [rusty_ffmpeg](https://github.com/CCExtractor/rusty_ffmpeg)
- [rust-ffmpeg](https://github.com/meh/rust-ffmpeg/)
- [ffmpeg-sys-next](https://github.com/zmwangx/rust-ffmpeg-sys)
- [ffmpeg-the-third](https://github.com/shssoichiro/ffmpeg-the-third)

## See also
>
> <https://github.com/zmwangx/rust-ffmpeg>
>
> <https://github.com/oddity-ai/video-rs>
>
> <https://github.com/YeautyYE/ez-ffmpeg>
> 
> <https://github.com/gcanat/video_reader-rs>
>
> <https://github.com/angelcam/rust-ac-ffmpeg>
>
> <https://github.com/itsakeyfut/avio>

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
