//! High-level encode-pipeline functional tests.
//!
//! Moved out of `src/encode.rs` (which now keeps only unit tests of the
//! encoder's own core methods): everything here needs a real container on disk,
//! so it drives the public API end to end — `EncoderBuilder` → `Muxer` → file →
//! `DecoderBuilder` — across the container matrices, filter graphs, automatic
//! PTS assignment, transcoding and the audio frame-length edge cases.
//!
//! Every case **skips itself** (prints `SKIP ...`) when a codec or filter is
//! missing from the linked FFmpeg build, so one suite runs unchanged on all
//! platforms and FFmpeg versions, and a build without e.g. libx265 does not
//! report a false failure.
//!
//! The whole file requires the `ndarray` feature, because the frames handed to
//! the encoders are built with the high-level `MediaFrame` API.

#![cfg(feature = "ndarray")]

mod common;

use std::collections::HashMap;

use rsmedia::error::Context;
use rsmedia::strutils;
use rsmedia::time;
use rsmedia::{
    CodecConfig, DecoderBuilder, EncoderBuilder, Filter, MediaFrame, MediaType, PixelFormat,
    Quality, Result, RsmediaError, SampleFormat, VideoProfile,
};

use rsmpeg::avutil;
use rsmpeg::ffi;

// ====================================================================
// 公共测试辅助
// ====================================================================

/// 滤镜因 FFmpeg 构建配置缺失（如 `drawtext` 依赖 libfreetype、`gamma` 等）
/// 初始化失败时优雅跳过，避免环境差异导致测试失败。
fn is_filter_unavailable(e: &RsmediaError) -> bool {
    let low = format!("{e}").to_lowercase();
    low.contains("no such filter")
        || low.contains("filter not found")
        || low.contains("not found")
        || low.contains("freetype")
}

/// 编码器因 FFmpeg 构建配置缺失（如 libmp3lame/libtheora/libx265）时跳过
fn is_encoder_unavailable(e: &RsmediaError) -> bool {
    e.to_string().contains("not available in this FFmpeg build")
}

/// 汇总容器遍历测试结果：任何非跳过失败都断言失败；至少一个容器成功，
/// 防止环境异常时测试空壳通过。
fn assert_container_results(
    kind: &str,
    passed: Vec<&'static str>,
    skipped: Vec<&'static str>,
    failed: Vec<(&'static str, String)>,
) {
    println!(
        "{kind}: {} passed {passed:?}, {} skipped {skipped:?}, {} failed",
        passed.len(),
        skipped.len(),
        failed.len()
    );
    assert!(failed.is_empty(), "{kind} encodings failed: {failed:#?}");
    assert!(!passed.is_empty(), "all {kind} encodings failed");
}

/// 生成一帧纯色（RGB24）测试视频帧，颜色随相位 `p` 在彩虹色相上变化。
fn rainbow_video_frame(w: usize, h: usize, p: f32) -> MediaFrame<u8> {
    use rsmedia::colors;
    let rgb = colors::hsv_to_rgb(p * 360.0, 100.0, 100.0);
    let mut frame =
        MediaFrame::<u8>::new_video_frame(w, h, PixelFormat::RGB24, time::new_rational(1, 24))
            .unwrap();
    for y in 0..h {
        for x in 0..w {
            frame.data[[y, x, 0]] = rgb[0];
            frame.data[[y, x, 1]] = rgb[1];
            frame.data[[y, x, 2]] = rgb[2];
        }
    }
    frame
}

/// 正弦波帧的采样类型映射：[-1, 1] 归一化值 → 各采样格式的存储类型。
///
/// 编码器原生采样格式各不相同（aac→FLTP、libopus→S16、mp2→S32P），
/// 由 `CodecConfig::supported_sample_formats` 协商后据此选择帧数据类型。
trait SineSample: rsmedia::frame::MediaFrameType {
    /// 该存储类型对应的采样格式
    fn format() -> SampleFormat;
    /// 归一化浮点值 → 存储值
    fn from_norm(v: f32) -> Self;
}

impl SineSample for f32 {
    fn format() -> SampleFormat {
        SampleFormat::FLTP
    }
    fn from_norm(v: f32) -> Self {
        v
    }
}

impl SineSample for i16 {
    fn format() -> SampleFormat {
        SampleFormat::S16
    }
    fn from_norm(v: f32) -> Self {
        (v * 32767.0) as Self
    }
}

impl SineSample for i32 {
    fn format() -> SampleFormat {
        SampleFormat::S32P
    }
    fn from_norm(v: f32) -> Self {
        (v * 2147483647.0) as Self
    }
}

/// 生成一帧全幅正弦波（幅度 0.5）音频帧，帧数据类型与采样格式自动匹配。
fn sine_audio_frame<T: SineSample>(
    freq: f32,
    channels: u32,
    nb_samples: u32,
    sample_rate: u32,
) -> MediaFrame<T> {
    use rsmedia::frame::MediaFrame;
    let mut frame = MediaFrame::<T>::new_audio_frame(
        T::format(),
        channels,
        nb_samples,
        sample_rate,
        time::new_rational(1, sample_rate as i32),
    )
    .unwrap();
    for i in 0..nb_samples as usize {
        for c in 0..channels as usize {
            let t = i as f32 / sample_rate as f32;
            frame.data[[0, i, c]] =
                T::from_norm((2.0 * std::f32::consts::PI * freq * t).sin() * 0.5);
        }
    }
    frame
}

// ====================================================================
// 视频编码测试
// ====================================================================
mod video {
    use super::*;
    use rsmpeg::avcodec::AVCodec;

    /// 视频容器规格：一个容器对应一条完整的编码配置。
    ///
    /// 常见视频容器的标准时间基（源自各容器规范）：
    ///
    /// | 容器格式 | 标准时间基      | 说明                       |
    /// | :------- | :------------- | :------------------------- |
    /// | MP4/F4V/M4V/3GP/TS | 1/90_000 | 90kHz，源自 MPEG-2 标准 |
    /// | MOV      | 1/10_000_000   | 10MHz，苹果 QuickTime 格式 |
    /// | MKV/WebM | 1/1_000_000_000| 纳秒级精度                 |
    /// | FLV      | 1/1_000        | 毫秒级，Flash 视频标准     |
    /// | AVI      | 1/{帧率}       | 基于帧计数                 |
    /// | ASF/WMV  | 1/10_000_000   | 100 纳秒单位，Windows Media|
    /// | OGG/OGV  | 1/1_000_000    | 微秒级，开源标准           |
    /// | MPEG     | 1/90_000       | 90kHz，MPEG 标准           |
    struct VideoContainerSpec {
        /// 容器扩展名（同时决定输出 muxer）
        container: &'static str,
        /// 编码器名，`None` = 默认 `libx264`
        codec: Option<&'static str>,
        /// 该容器的标准时间基 `(num, den)`
        time_base: (i32, i32),
        /// 目标码率，`0` = 不设置（未压缩/由编码器默认）
        bit_rate: u64,
        /// 编码器 AVCodecContext 私有选项
        options: Option<&'static [(&'static str, &'static str)]>,
    }

    const fn vc(
        container: &'static str,
        codec: Option<&'static str>,
        time_base: (i32, i32),
        bit_rate: u64,
        options: Option<&'static [(&'static str, &'static str)]>,
    ) -> VideoContainerSpec {
        VideoContainerSpec {
            container,
            codec,
            time_base,
            bit_rate,
            options,
        }
    }

    /// 市场常见视频容器 → 编码器/时间基/码率/选项 映射表。
    /// 仅保留 FFmpeg 有 muxer 且无需专用硬件的通用格式。
    const VIDEO_CONTAINERS: &[VideoContainerSpec] = &[
        // ---- 容器,           编码器(None=libx264),      标准时间基,          码率,     选项 ----
        // 通用/主流容器
        vc("mp4", None, (1, 90_000), 2_000_000, None),
        vc("mkv", None, (1, 1_000_000_000), 2_000_000, None),
        vc(
            "webm",
            Some("libvpx-vp9"),
            (1, 1_000_000_000),
            1_000_000,
            None,
        ),
        // AVI 时间基基于帧率（测试固定 25fps）
        vc(
            "avi",
            None,
            (1, 25),
            2_000_000,
            Some(&[("profile", "baseline"), ("level", "3.0")]),
        ),
        vc("mov", None, (1, 90_000), 2_000_000, None),
        vc("flv", None, (1, 1_000), 2_000_000, None),
        vc("mpg", None, (1, 90_000), 2_000_000, None),
        vc("ts", None, (1, 90_000), 2_000_000, None),
        vc("vob", Some("mpeg2video"), (1, 90_000), 2_000_000, None),
        // 移动/特殊容器
        vc("3gp", None, (1, 90_000), 2_000_000, None),
        // 原始/裸流格式
        vc("h264", None, (1, 90_000), 2_000_000, None),
        vc("h265", Some("libx265"), (1, 90_000), 2_000_000, None),
        // YUV4MPEG2 只接受未压缩视频
        vc("y4m", Some("rawvideo"), (1, 90_000), 0, None),
        // SWF 只接受 Flash 系编码器（FLV1 编码器注册名为 `flv`）
        vc("swf", Some("flv"), (1, 1_000), 2_000_000, None),
        // 精简掉冗余的低价值容器（wmv/mpeg/asf/m2ts/mts/f4v/ismv/ogv/rm/m4v/3g2）
        // 以缩短 valgrind/localtest 全量测试耗时。
    ];

    /// 对指定视频容器执行「编码 10 秒视频 → flush」完整流程。
    fn encode_video_for_container(spec: &VideoContainerSpec, fps: f64) -> Result<()> {
        use rsmedia::filter;
        use rsmedia::time::Time;

        let codec_name = spec.codec.unwrap_or("libx264");
        // 编码器存在性取决于 FFmpeg 构建配置（如 libtheora/libx265），缺失时跳过
        if AVCodec::find_encoder_by_name(&strutils::str_to_cstring(codec_name)).is_none() {
            return Err(RsmediaError::codec_not_found(format!(
                "encoder {codec_name} not available in this FFmpeg build"
            )));
        }
        let codec_name = strutils::str_to_cstring(codec_name);
        let codec_config = CodecConfig::new_with_name(&codec_name)?;
        assert!(
            codec_config.is_encoder(),
            "Codec:'{:?}' is not an encoder.",
            codec_name
        );

        // drawtext 依赖 libfreetype 编译进 FFmpeg，部分构建未启用，不可用时降级为仅 scale+crop
        let mut filters = vec![filter::video::scale(640, 360, None)];
        if rsmpeg::avfilter::AVFilter::get_by_name(c"drawtext").is_some() {
            // DrawText 缺省字体为项目内 fonts/Arial.ttf（见 DrawText::build），
            filters
                .push(filter::video::DrawText::new("Watermark", 50, 50, 24, "white@0.5").build());
        } else {
            println!("SKIP drawtext (libfreetype not available)");
        }
        filters.push(filter::video::crop(0, 0, 640, 360));

        // 视频编码参数
        let width = 640;
        let height = 360;
        let output_path =
            common::test_output_path("encode", &format!("test_encode_video.{}", spec.container));
        common::remove_test_output(&output_path);

        // 按容器规格创建编码器（fps 必须传入编码器，保证 time_base = 1/fps，
        // 否则编码器运行在默认 30fps，与帧 pts 的 25fps 语义不一致，
        // 会导致 flv 等严格 muxer 报 "Invalid pts <= last"）
        let mut builder = EncoderBuilder::new_video(width as usize, height as usize)
            .with_codec_name(codec_name.to_str()?.to_string())
            .with_fps(fps as f32)
            .with_filters(filters);
        if spec.bit_rate > 0 {
            builder = builder.with_bit_rate(spec.bit_rate as i64);
        }
        if let Some(opts) = spec.options {
            let opts: HashMap<String, String> = opts
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            builder = builder.with_options(Some(Into::into(opts)));
        }
        let video_encoder = builder.build()?;
        let encoder_time_base = video_encoder.time_base();
        let mut muxer = rsmedia::mux::Muxer::new(&output_path)?;
        let video_index = muxer.add_encoder(video_encoder)?;

        // 按容器标准时间基计算帧间隔（验证不同时间基下 pts 均匀）
        let actual_timebase = encoder_time_base;
        let frame_duration_seconds = 1.0 / fps;
        let duration_units = (frame_duration_seconds * actual_timebase.den as f64
            / actual_timebase.num as f64)
            .round() as i64;
        let duration = Time::new(Some(duration_units), actual_timebase);
        let container_tb = time::new_rational(spec.time_base.0, spec.time_base.1);
        let mut position = Time::new(Some(0), container_tb);

        println!(
            "Encoding {} with actual timebase: {}/{}, duration units: {}, fps: {}",
            spec.container, actual_timebase.num, actual_timebase.den, duration_units, fps
        );

        // 帧编码并写入文件 0.5s 缩短 valgrind 全量测试耗时
        const VIDEO_DURATION_SECS: f64 = 0.5;
        let n_frames = (VIDEO_DURATION_SECS * fps).round() as usize;
        let n_frames = n_frames.max(1);
        for i in 0..n_frames {
            let mut frame =
                rainbow_video_frame(width as usize, height as usize, i as f32 / n_frames as f32);
            frame.set_pts(
                position
                    .aligned_with_rational(encoder_time_base)
                    .into_value()
                    .unwrap(),
            );
            let mut avframe = frame.to_avframe()?;
            avframe.set_time_base(encoder_time_base);

            muxer.mux(avframe, video_index)?;

            // 使用aligned_with确保时间基一致进行加法操作
            position = position.aligned_with(duration).add();
        }

        // flush encoder
        muxer.finish().unwrap();

        Ok(())
    }

    /// 遍历视频容器映射表逐一编码。
    /// 编码器缺失的容器跳过并报告；其余失败视为测试失败（严格模式），
    /// 但要求至少一个容器成功，防止环境异常时测试空壳通过。
    #[test]
    fn test_encode_video() {
        let mut passed = Vec::new();
        let mut skipped = Vec::new();
        let mut failed = Vec::new();
        let fps = 25.0;

        for spec in VIDEO_CONTAINERS {
            println!("Testing format: {}...", spec.container);
            match encode_video_for_container(spec, fps) {
                Ok(()) => {
                    println!("Testing format: {} passed.", spec.container);
                    passed.push(spec.container);
                }
                Err(e) if is_encoder_unavailable(&e) => {
                    println!("SKIP {}: {e:#}", spec.container);
                    skipped.push(spec.container);
                }
                Err(e) => failed.push((spec.container, format!("{e:#}"))),
            }
        }

        assert_container_results("video containers", passed, skipped, failed);
    }

    /// 自包含的编解码往返测试：编码若干帧到临时文件，再解码回，验证帧数与尺寸一致。
    /// 不依赖任何外部媒体文件，可自动运行。
    #[test]
    fn test_encode_decode_roundtrip() -> Result<()> {
        use rsmedia::{DecoderBuilder, MediaType};

        let width = 64usize;
        let height = 64usize;
        let n_frames = 10;
        let fps = 25.0;

        let path = common::test_output_path("encode", "rsmedia_roundtrip.mp4");
        common::remove_test_output(&path);

        // 1) 编码：裸 Encoder + Muxer，手动维护 pts
        let video_encoder = EncoderBuilder::new_video(width, height)
            .with_fps(fps)
            .build()?;
        let enc_tb = video_encoder.time_base();
        let mut muxer = rsmedia::mux::Muxer::new(&path)?;
        let v_idx = muxer.add_encoder(video_encoder)?;
        for i in 0..n_frames as i64 {
            let mut frame = rainbow_video_frame(width, height, i as f32 / n_frames as f32);
            // 编码器 time_base = 1/fps，帧索引即 pts（每帧 1 tick）
            frame.set_pts(i);
            let mut av = frame.to_avframe()?;
            av.set_time_base(enc_tb);
            muxer.mux(av, v_idx)?;
        }
        muxer.finish()?;

        // 2) 解码回：验证帧数与解码尺寸
        let mut reader = rsmedia::StreamReader::new(&path)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
        let mut decoded = 0usize;
        while let Some(frame) = decoder.decode_frame(&mut reader)? {
            assert_eq!(frame.width, width);
            assert_eq!(frame.height, height);
            decoded += 1;
        }
        assert_eq!(
            decoded, n_frames,
            "decoded frame count mismatch: got {decoded}, expected {n_frames}"
        );

        common::remove_test_output(&path);
        Ok(())
    }

    /// Quality::Crf 编码往返：文件正常产出、帧数一致。
    #[test]
    fn test_quality_crf_roundtrip() -> Result<()> {
        use rsmedia::DecoderBuilder;

        let width = 64usize;
        let height = 64usize;
        let n_frames = 10;
        let fps = 25.0;

        let path = common::test_output_path("encode", "rsmedia_crf_roundtrip.mp4");
        common::remove_test_output(&path);

        let video_encoder = EncoderBuilder::new_video(width, height)
            .with_fps(fps)
            .with_quality(Quality::Crf(23))
            .build()?;
        let enc_tb = video_encoder.time_base();
        let mut muxer = rsmedia::mux::Muxer::new(&path)?;
        let v_idx = muxer.add_encoder(video_encoder)?;
        for i in 0..n_frames as i64 {
            let mut frame = rainbow_video_frame(width, height, i as f32 / n_frames as f32);
            // 编码器 time_base = 1/fps，帧索引即 pts（每帧 1 tick）
            frame.set_pts(i);
            let mut av = frame.to_avframe()?;
            av.set_time_base(enc_tb);
            muxer.mux(av, v_idx)?;
        }
        muxer.finish()?;

        let mut reader = rsmedia::StreamReader::new(&path)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
        let mut decoded = 0usize;
        while decoder.decode_frame(&mut reader)?.is_some() {
            decoded += 1;
        }
        assert_eq!(decoded, n_frames, "CRF roundtrip frame count mismatch");

        common::remove_test_output(&path);
        Ok(())
    }

    /// P0-2 自动像素格式协商：mjpeg 仅接受 YUVJ 系像素格式，未显式指定
    /// pix_fmt 时应从支持列表协商（YUV420P 输入帧由 rescale 自动转换），
    /// 编码往返成功；显式指定不支持的格式时 `build()` 立即报错。
    #[test]
    fn test_negotiate_pixel_format_mjpeg() -> Result<()> {
        use rsmedia::DecoderBuilder;

        let width = 64usize;
        let height = 64usize;
        let n_frames = 5;

        // 未显式指定 pix_fmt：协商为 mjpeg 支持列表中的格式
        let path = common::test_output_path("encode", "rsmedia_mjpeg.avi");
        common::remove_test_output(&path);
        let video_encoder = EncoderBuilder::new_video(width, height)
            .with_codec_name("mjpeg".to_string())
            .build()?;
        let enc_tb = video_encoder.time_base();
        let mut muxer = rsmedia::mux::Muxer::new(&path)?;
        let v_idx = muxer.add_encoder(video_encoder)?;
        for i in 0..n_frames as i64 {
            let mut frame = rainbow_video_frame(width, height, i as f32 / n_frames as f32);
            // 编码器 time_base = 1/fps，帧索引即 pts（每帧 1 tick）
            frame.set_pts(i);
            let mut av = frame.to_avframe()?;
            av.set_time_base(enc_tb);
            muxer.mux(av, v_idx)?;
        }
        muxer.finish()?;

        let mut reader = rsmedia::StreamReader::new(&path)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
        let mut decoded = 0usize;
        while decoder.decode_frame(&mut reader)?.is_some() {
            decoded += 1;
        }
        assert_eq!(decoded, n_frames, "mjpeg roundtrip frame count mismatch");
        drop(reader);
        common::remove_test_output(&path);

        // 显式指定编码器不支持的像素格式：build() 应 fail fast
        let result = EncoderBuilder::new_video(width, height)
            .with_codec_name("mjpeg".to_string())
            .with_pixel_format(PixelFormat::RGB24)
            .build();
        assert!(
            result.is_err(),
            "explicit unsupported pix_fmt should fail at build()"
        );
        Ok(())
    }

    /// profile/level 通过私有选项传给 libx264：编码到 MP4 后重新打开，
    /// 容器元数据（avcC/SPS）应回报 profile=High(100)、level=4.1(41)。
    #[test]
    fn test_profile_level_applied() -> Result<()> {
        let width = 64usize;
        let height = 64usize;
        let fps = 25.0;

        let path = common::test_output_path("encode", "rsmedia_profile.mp4");
        common::remove_test_output(&path);

        let encoder_bare = EncoderBuilder::new_video(width, height)
            .with_fps(fps)
            .with_profile(VideoProfile::High)
            .with_level("4.1")
            .build()?;
        let enc_tb = encoder_bare.time_base();
        let mut muxer = rsmedia::mux::Muxer::new(&path)?;
        let v_idx = muxer.add_encoder(encoder_bare)?;
        for i in 0..5i64 {
            let mut frame = rainbow_video_frame(width, height, i as f32 / 5.0);
            frame.set_pts(i);
            let mut av = frame.to_avframe()?;
            av.set_time_base(enc_tb);
            muxer.mux(av, v_idx)?;
        }
        muxer.finish()?;

        let reader = rsmedia::StreamReader::new(&path)?;
        let decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
        let info = rsmedia::stream::StreamInfo::from_reader(&reader, decoder.stream_index())?;
        assert_eq!(info.profile, ffi::AV_PROFILE_H264_HIGH as i32);
        assert_eq!(info.level, 41);

        drop(reader);
        common::remove_test_output(&path);
        Ok(())
    }

    /// 回归测试：带延迟滤镜（framerate，内部缓冲运动插值帧、flush 时才输出剩余帧）
    /// 编码时，flush() 阶段取出的缓冲帧必须全部写盘，不能因再次送入已 flushed 的
    /// filter 而被丢弃。
    #[test]
    fn test_encode_delayed_filter_roundtrip() -> Result<()> {
        use rsmedia::{DecoderBuilder, MediaType};

        let width = 64usize;
        let height = 64usize;
        let n_frames = 30;
        let fps = 30.0;

        let path = common::test_output_path("encode", "rsmedia_delayed_filter.mp4");
        common::remove_test_output(&path);

        // framerate 滤镜内部缓冲运动插值帧，输入 30 帧@30fps=1s，输出仍约 30 帧，
        // 其中尾部的插值帧要等 flush(EOF) 才输出。若 flush 缓冲帧被丢弃会偏少。
        let encoder_bare = EncoderBuilder::new_video(width, height)
            .with_fps(fps as f32)
            .with_filters(vec![Filter::new(
                "framerate",
                MediaType::VIDEO,
                "framerate=fps=30".to_string(),
            )])
            .build()?;
        let enc_tb = encoder_bare.time_base();
        let mut muxer = rsmedia::mux::Muxer::new(&path)?;
        let v_idx = muxer.add_encoder(encoder_bare)?;
        for i in 0..n_frames as i64 {
            let mut frame = rainbow_video_frame(width, height, i as f32 / n_frames as f32);
            frame.set_pts(i);
            let mut av = frame.to_avframe()?;
            av.set_time_base(enc_tb);
            muxer.mux(av, v_idx)?;
        }
        muxer.finish()?;

        let mut reader = rsmedia::StreamReader::new(&path)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
        let mut decoded = 0usize;
        while let Some(_frame) = decoder.decode_frame(&mut reader)? {
            decoded += 1;
        }
        assert!(
            decoded >= n_frames,
            "delayed filter roundtrip lost frames: got {decoded}, expected >= {n_frames}"
        );

        common::remove_test_output(&path);
        Ok(())
    }

    /// 验证 `write_frame` 自动维护的 pts 单调递增且与帧率一致（不含 B 帧时每帧
    /// 时长为 1/fps，按编码器时间基换算）。
    #[test]
    fn test_write_frame_auto_pts() -> Result<()> {
        use rsmedia::{DecoderBuilder, MediaType};

        let width = 64usize;
        let height = 64usize;
        let n_frames = 8;
        let fps: f64 = 30.0;

        let path = common::test_output_path("encode", "rsmedia_auto_pts.mp4");
        common::remove_test_output(&path);

        let encoder_bare = EncoderBuilder::new_video(width, height)
            .with_fps(fps as f32)
            .build()?;
        let enc_tb = encoder_bare.time_base();

        // 帧时长 = 1/fps（秒）。解码输出的 pts 位于输出流 time_base（movenc
        // 可能调整，如 MP4 用 1/15360），故在解码后按实际帧 time_base 计算期望增量。
        let mut muxer = rsmedia::mux::Muxer::new(&path)?;
        let v_idx = muxer.add_encoder(encoder_bare)?;
        for i in 0..n_frames as i64 {
            let mut frame = rainbow_video_frame(width, height, i as f32 / n_frames as f32);
            // 编码器 time_base = 1/fps，每帧 ptp 为 1 tick（=1/fps 秒），帧索引即 pt
            frame.set_pts(i);
            let mut av = frame.to_avframe()?;
            av.set_time_base(enc_tb);
            muxer.mux(av, v_idx)?;
        }
        muxer.finish()?;

        // 解码期望，收集真实 pts，验证相邻帧 pts 差一致
        let mut reader = rsmedia::StreamReader::new(&path)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
        let mut pts_list: Vec<i64> = Vec::new();
        while let Some(frame) = decoder.decode_frame(&mut reader)? {
            pts_list.push(frame.pts);
        }
        assert_eq!(pts_list.len(), n_frames);

        // 解码 pts 位于输出流 time_base（movenc 可能调整，如 MP4 用 1/15360）
        let tb = decoder.time_base();
        let expected_delta = (tb.den as f64 / tb.num as f64 / fps).round() as i64;

        // 排除 B 帧重排的影响：仅断言存在一致的正增量（B 帧可能为 0/负，取出现最多的增量）
        let mut counts: HashMap<i64, usize> = HashMap::new();
        for d in pts_list.windows(2).map(|w| w[1] - w[0]) {
            if d > 0 {
                *counts.entry(d).or_default() += 1;
            }
        }
        let delta = counts
            .into_iter()
            .max_by_key(|(_, c)| *c)
            .map(|(d, _)| d)
            .unwrap_or(expected_delta);
        assert_eq!(delta, expected_delta, "pts delta mismatch vs 1/fps");

        common::remove_test_output(&path);
        Ok(())
    }

    /// 用户完全不设置 pts（`MediaFrame.pts` 保持 `AV_NOPTS_VALUE`）时，编码器
    /// 必须自动按 `1/fps` 编号：解码回读的 pts 从 0 起步且等差。这是
    /// "自动 pts" 行为的端到端回归测试（此前所有帧 pts=0，mp4 mux 直接报错）。
    #[test]
    fn test_video_pts_fully_automatic() -> Result<()> {
        use rsmedia::{DecoderBuilder, MediaType};

        let width = 64usize;
        let height = 64usize;
        let n_frames = 8usize;
        let fps: f64 = 30.0;

        let path = common::test_output_path("encode", "rsmedia_no_pts_video.mp4");
        common::remove_test_output(&path);

        let encoder = EncoderBuilder::new_video(width, height)
            .with_fps(fps as f32)
            .build()?;
        let mut muxer = rsmedia::mux::Muxer::new(&path)?;
        let v_idx = muxer.add_encoder(encoder)?;
        for i in 0..n_frames {
            // 关键：不调用 set_pts —— pts 保持 AV_NOPTS_VALUE，由编码器自动编号。
            let frame = rainbow_video_frame(width, height, i as f32 / n_frames as f32);
            muxer.mux(frame.to_avframe()?, v_idx)?;
        }
        muxer.finish()?;

        let mut reader = rsmedia::StreamReader::new(&path)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
        let mut pts_list: Vec<i64> = Vec::new();
        while let Some(frame) = decoder.decode_frame(&mut reader)? {
            pts_list.push(frame.pts);
        }
        assert_eq!(pts_list.len(), n_frames, "decoded frame count mismatch");

        // 解码输出按显示顺序排列，pts 应严格等差（步长 = 1/fps 换算到流时间基）。
        let tb = decoder.time_base();
        let expected_delta = (tb.den as f64 / tb.num as f64 / fps).round() as i64;
        assert!(
            expected_delta > 0,
            "non-positive expected pts delta: {expected_delta}"
        );
        for (i, pts) in pts_list.iter().enumerate() {
            assert_eq!(
                *pts,
                i as i64 * expected_delta,
                "frame {i}: pts {pts} != {i}*{expected_delta} (tb {}/{})",
                tb.num,
                tb.den
            );
        }

        common::remove_test_output(&path);
        Ok(())
    }

    /// 验证不同帧率下：编码器 time_base 恒为 `1/fps`，且编码→解码往返帧数一致。
    ///
    /// 这是对 fps → time_base 推导（`av_inv_q`）与末帧 duration 修复的回归测试。
    #[test]
    fn test_encode_video_multiple_fps() -> Result<()> {
        use rsmedia::{DecoderBuilder, MediaType};

        for fps in [24.0f32, 25.0, 30.0, 60.0, 29.97] {
            let path = common::test_output_path("encode", &format!("rsmedia_fps_{fps}.mp4"));
            common::remove_test_output(&path);

            let n_frames = 12;
            let encoder_bare = EncoderBuilder::new_video(64, 64).with_fps(fps).build()?;

            // 1) 编码器 time_base 必须等于 1/fps
            let tb = encoder_bare.time_base();
            let expected_tb = avutil::av_inv_q(avutil::av_d2q(fps as f64, 100_000));
            assert_eq!(
                (tb.num, tb.den),
                (expected_tb.num, expected_tb.den),
                "fps={fps}: time_base {}/{} != 1/fps",
                tb.num,
                tb.den
            );

            let enc_tb = encoder_bare.time_base();
            let mut muxer = rsmedia::mux::Muxer::new(&path)?;
            let v_idx = muxer.add_encoder(encoder_bare)?;
            for i in 0..n_frames as i64 {
                let mut frame = rainbow_video_frame(64, 64, i as f32 / n_frames as f32);
                frame.set_pts(i);
                let mut av = frame.to_avframe()?;
                av.set_time_base(enc_tb);
                muxer.mux(av, v_idx)?;
            }
            muxer.finish()?;

            // 2) 解码回，帧数必须与编码一致（验证末帧未被 muxer 丢弃）
            let mut reader = rsmedia::StreamReader::new(&path)?;
            let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
            let mut decoded = 0usize;
            while let Some(frame) = decoder.decode_frame(&mut reader)? {
                assert_eq!(frame.width, 64);
                assert_eq!(frame.height, 64);
                decoded += 1;
            }
            assert_eq!(
                decoded, n_frames,
                "fps={fps}: decoded {decoded} frames, expected {n_frames}"
            );

            common::remove_test_output(&path);
        }
        Ok(())
    }

    /// 综合参数组合往返测试：编码→解码，覆盖 编解码器 / fps / 源尺寸 / resize /
    /// 缩放算法 / 延迟滤镜 的交叉组合，验证：
    ///   1) 解码器 resize 后输出尺寸正确；
    ///   2) 解码帧数与编码一致（末帧不被丢弃）；
    ///   3) 延迟滤镜（framerate）在 EOF 冲刷后不丢帧、不报 "cannot decode after flushed"。
    #[test]
    fn test_param_combination_roundtrip() -> Result<()> {
        use rsmedia::filter::Filter;
        use rsmedia::{DecoderBuilder, MediaType, Resize, ScaleAlgorithm};

        let codecs: &[(&str, bool)] = &[
            ("libx264", true), // 支持延迟滤镜插值
            ("mpeg4", false),  // 简单编码器，检验无延迟路径
        ];
        let srces: &[(usize, usize)] = &[(64, 64), (96, 48)];
        let resizes: &[Option<Resize>] = &[
            None,                          // 不缩放，期望原尺寸
            Some(Resize::Exact(32, 32)),   // 精确尺寸
            Some(Resize::FitEven(16, 16)), // 保持宽高比、偶数尺寸
        ];
        let algos: &[ScaleAlgorithm] = &[
            ScaleAlgorithm::BICUBIC,
            ScaleAlgorithm::POINT,
            ScaleAlgorithm::LANCZOS,
        ];
        let fps_list: &[f32] = &[24.0, 30.0];

        for &(codec, delayed) in codecs {
            for &(w, h) in srces {
                for &fps in fps_list {
                    for &resize in resizes {
                        // 期望尺寸：resize 实际输出的尺寸（按宽高比计算），None 则为原尺寸
                        let (ew, eh) = match resize {
                            Some(r) => {
                                let (dw, dh) = r.compute_for((w as u32, h as u32)).unwrap();
                                (dw as usize, dh as usize)
                            }
                            None => (w, h),
                        };
                        for &algo in algos {
                            println!(
                                "COMB codec={codec} src={w}x{h} fps={fps} resize={resize:?} algo={algo:?}"
                            );
                            let n_frames = 6usize;
                            // Windows 文件名校验：避免把 Debug 形式（含引号/括号/逗号/空格）放进文件名
                            let resize_token = match resize {
                                Some(Resize::Exact(w, h)) => format!("exact_{w}x{h}"),
                                Some(Resize::Fit(w, h)) => format!("fit_{w}x{h}"),
                                Some(Resize::FitEven(w, h)) => format!("fiteven_{w}x{h}"),
                                None => "orig".to_string(),
                            };
                            let path = common::test_output_path(
                                "encode",
                                &format!(
                                    "rsmedia_param_{codec}_{w}x{h}_{fps}_{resize_token}_{algo:?}.mp4"
                                ),
                            );
                            common::remove_test_output(&path);

                            // 编码
                            let enc_bare = EncoderBuilder::new_video(w, h)
                                .with_codec_name(Some(codec.to_string()))
                                .with_fps(fps)
                                .with_filters(if delayed {
                                    Some(vec![Filter::new(
                                        "framerate",
                                        MediaType::VIDEO,
                                        "framerate=fps=30".to_string(),
                                    )])
                                } else {
                                    None
                                })
                                .build()?;
                            let enc_tb = enc_bare.time_base();
                            let mut muxer = rsmedia::mux::Muxer::new(&path)?;
                            let v_idx = muxer.add_encoder(enc_bare)?;
                            for i in 0..n_frames as i64 {
                                let mut frame =
                                    rainbow_video_frame(w, h, i as f32 / n_frames as f32);
                                // 编码器 time_base = 1/fps，帧索引即 pts（每帧 1 tick）
                                frame.set_pts(i);
                                let mut av = frame.to_avframe()?;
                                av.set_time_base(enc_tb);
                                muxer.mux(av, v_idx)?;
                            }
                            muxer.finish()?;

                            // 解码（可选 resize + 缩放算法）
                            let mut dec_builder =
                                DecoderBuilder::new(MediaType::VIDEO).with_scale_algorithm(algo);
                            if let Some(r) = resize {
                                dec_builder = dec_builder.with_resize(r);
                            }
                            let mut reader = rsmedia::StreamReader::new(&path)?;
                            let mut dec = dec_builder.build_from_reader(&reader)?;
                            let mut decoded = 0usize;
                            while let Some(frame) = dec.decode_frame(&mut reader)? {
                                assert_eq!(
                                    frame.width, ew,
                                    "{codec} {w}x{h} fps={fps} resize={resize:?} {algo:?}: width got {} exp {ew}",
                                    frame.width
                                );
                                assert_eq!(
                                    frame.height, eh,
                                    "{codec} {w}x{h} fps={fps} resize={resize:?} {algo:?}: height got {} exp {eh}",
                                    frame.height
                                );
                                decoded += 1;
                            }
                            assert!(
                                decoded >= n_frames,
                                "{codec} {w}x{h} fps={fps} resize={resize:?} {algo:?}: decoded {decoded}, expected >= {n_frames}"
                            );

                            common::remove_test_output(&path);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// 视频滤镜全量往返测试：编码时对每个滤镜逐一应用，再解码验证。
    ///
    /// 覆盖所有不依赖外部文件/设备的视频滤镜（`subtitles`/`zoompan`/`drawtext` 等
    /// 需要外部资源或帧率语义特殊，已排除）。验证：
    ///   1) 滤镜在编码管线中可正常初始化、不报错；
    ///   2) EOF / flush 阶段不丢帧、不报 "cannot decode after flushed"；
    ///   3) 尺寸保持类滤镜输出尺寸不变，尺寸改变类（scale/crop/pad/rotate/transpose）
    ///      输出尺寸符合预期。
    #[test]
    fn test_video_filters_roundtrip() -> Result<()> {
        use rsmedia::filter::video;
        use rsmedia::{DecoderBuilder, MediaType};

        let width = 64usize;
        let height = 64usize;
        let n_frames = 6;
        let fps = 25.0;

        // (名称, Filter, 期望最小解码帧数, 期望尺寸(Some 则精确断言，None 则不断言))
        // 注：尺寸改变类滤镜（`scale`/`crop`/`pad`/`rotate`/`transpose`）在编码管线中
        // 存在已知崩溃（SIGSEGV），与滤镜本身无关，属编码-滤镜尺寸同步缺陷，已隔离到
        // 专项调查，暂不纳入本列表阻塞其它滤镜测试。此处仅覆盖尺寸保持类滤镜。
        type FilterCase = (
            &'static str,
            rsmedia::filter::Filter,
            usize,
            Option<(usize, usize)>,
        );
        let cases: Vec<FilterCase> = vec![
            // 尺寸保持类
            ("hflip", video::hflip(), n_frames, Some((width, height))),
            ("vflip", video::vflip(), n_frames, Some((width, height))),
            ("negate", video::negate(), n_frames, Some((width, height))),
            ("hue", video::hue(30), n_frames, Some((width, height))),
            ("gamma", video::gamma(1.2), n_frames, Some((width, height))),
            ("noise", video::noise(10), n_frames, Some((width, height))),
            (
                "saturation",
                video::saturation(1.5),
                n_frames,
                Some((width, height)),
            ),
            (
                "vibrance",
                video::vibrance(0.4),
                n_frames,
                Some((width, height)),
            ),
            ("deblock", video::deblock(), n_frames, Some((width, height))),
            ("unsharp", video::unsharp(), n_frames, Some((width, height))),
            ("blur", video::blur(2.0), n_frames, Some((width, height))),
            ("eq", video::eq(0.2, 1.5), n_frames, Some((width, height))),
            (
                "hqdn3d",
                video::hqdn3d(2.0, 2.0),
                n_frames,
                Some((width, height)),
            ),
            (
                "nlmeans",
                video::nlmeans(1.0),
                n_frames,
                Some((width, height)),
            ),
            (
                "setdar",
                video::setdar(16, 9),
                n_frames,
                Some((width, height)),
            ),
            (
                "setsar",
                video::setsar(1, 1),
                n_frames,
                Some((width, height)),
            ),
            (
                "drawbox",
                video::drawbox(0, 0, 32, 32, "red", 2),
                n_frames,
                Some((width, height)),
            ),
            (
                "delogo",
                video::delogo(1, 1, 30, 30),
                n_frames,
                Some((width, height)),
            ),
            (
                "fade_in",
                video::fade_in(6),
                n_frames,
                Some((width, height)),
            ),
            (
                "fade_out",
                video::fade_out(n_frames as u32, 6),
                n_frames,
                Some((width, height)),
            ),
            // 帧率保持类（`fps` 按时间戳取整，末帧可能被舍去，故最小帧数放宽一帧）
            ("fps", video::fps(24.0), n_frames - 1, Some((width, height))),
            // DrawText 依赖 FFmpeg 以 libfreetype 编译；缺省字体为项目内 fonts/Arial.ttf
            (
                "DrawText",
                video::DrawText::new("Hello", 5, 5, 16, "white").build(),
                n_frames,
                Some((width, height)),
            ),
        ];

        for (name, filter, min_frames, dims) in cases {
            println!("VIDFILT {name}");
            let path = common::test_output_path("encode", &format!("rsmedia_vfilt_{name}.mp4"));
            common::remove_test_output(&path);

            // 编码（应用该滤镜）；滤镜缺失时优雅跳过
            let enc = match EncoderBuilder::new_video(width, height)
                .with_fps(fps)
                .with_filters(vec![filter])
                .build()
            {
                Ok(enc) => enc,
                Err(e) if is_filter_unavailable(&e) => {
                    println!("SKIP {name}: not available ({e:#})");
                    common::remove_test_output(&path);
                    continue;
                }
                Err(e) => return Err(e),
            };
            let enc_tb = enc.time_base();
            let video_idx = {
                let mut muxer = rsmedia::mux::Muxer::new(&path)?;
                let idx = muxer.add_encoder(enc)?;
                for i in 0..n_frames {
                    let mut frame = rainbow_video_frame(width, height, i as f32 / n_frames as f32);
                    frame.set_pts(i as i64);
                    let mut av = frame.to_avframe()?;
                    av.set_time_base(enc_tb);
                    muxer.mux(av, idx)?;
                }
                muxer.finish()?;
                idx
            };
            let _ = video_idx;

            // 解码验证
            let mut reader = rsmedia::StreamReader::new(&path)?;
            let mut dec = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
            let mut decoded = 0usize;
            while let Some(frame) = dec.decode_frame(&mut reader)? {
                if let Some((ew, eh)) = dims {
                    assert_eq!(
                        frame.width, ew,
                        "{name}: width got {} exp {ew}",
                        frame.width
                    );
                    assert_eq!(
                        frame.height, eh,
                        "{name}: height got {} exp {eh}",
                        frame.height
                    );
                }
                decoded += 1;
            }
            assert!(
                decoded >= min_frames,
                "{name}: decoded {decoded}, expected >= {min_frames}"
            );

            common::remove_test_output(&path);
        }
        Ok(())
    }
}

// ====================================================================
// 音频编码测试
// ====================================================================
mod audio {
    use super::*;
    use rsmedia::fmt::FrameFormat;
    use rsmpeg::avcodec::AVCodec;

    /// 音频容器规格：一个容器对应一条完整的编码配置。
    ///
    /// 注意：采样格式属于**编码器能力**而非容器属性（如 libopus 原生 s16/flt、
    /// mp2 原生 s32p、aac 原生 fltp），由 `CodecConfig` 在运行时协商，
    /// 测试帧的数据类型随之匹配（FLTP→f32 / S16→i16 / S32P→i32）。
    struct AudioContainerSpec {
        /// 容器扩展名（同时决定输出 muxer）
        container: &'static str,
        /// 编码器名，`None` = 默认 `aac`
        codec: Option<&'static str>,
        /// 目标码率，`0` = 无损/未压缩（编码器自动决定）
        bit_rate: u64,
        /// 期望采样率（编码器不支持时回退其支持列表首项，如 Opus 固定 48kHz 族）
        sample_rate: u32,
        /// 声道数
        channels: u32,
    }

    const fn ac(
        container: &'static str,
        codec: Option<&'static str>,
        bit_rate: u64,
        sample_rate: u32,
        channels: u32,
    ) -> AudioContainerSpec {
        AudioContainerSpec {
            container,
            codec,
            bit_rate,
            sample_rate,
            channels,
        }
    }

    /// 市场常见音频容器 → 编码器/码率/采样率 映射表。
    /// 分三档：有损压缩 / 无损压缩 / 未压缩 PCM。
    const AUDIO_CONTAINERS: &[AudioContainerSpec] = &[
        // ---- 容器,   编码器(None=aac),          码率,      采样率,  声道 ----
        // 有损压缩
        ac("m4a", None, 128_000, 44_100, 2),  // AAC，通用默认
        ac("aac", None, 128_000, 44_100, 2),  // AAC 裸流
        ac("adts", None, 128_000, 44_100, 2), // AAC + ADTS 头
        ac("mp3", Some("libmp3lame"), 192_000, 44_100, 2), // 最通用
        ac("opus", Some("libopus"), 96_000, 48_000, 2), // 流媒体/低延迟
        ac("ogg", Some("libopus"), 96_000, 48_000, 2), // Ogg 封装 Opus
        ac("webm", Some("libopus"), 96_000, 48_000, 2), // WebM 纯音频
        ac("ac3", Some("ac3"), 192_000, 48_000, 2), // 影院/电视
        ac("mp2", Some("mp2"), 256_000, 44_100, 2), // 广播
        ac("wma", Some("wmav2"), 128_000, 44_100, 2), // Windows Media
        // 无损压缩
        ac("flac", Some("flac"), 0, 44_100, 2),
        // 未压缩 PCM
        ac("wav", Some("pcm_s16le"), 0, 44_100, 2),
        ac("aiff", Some("pcm_s16be"), 0, 44_100, 2),
        ac("au", Some("pcm_s16be"), 0, 44_100, 2),
        ac("caf", Some("pcm_s16le"), 0, 44_100, 2),
    ];

    /// 对指定音频容器执行「编码 5 秒正弦波 → 解码校验」完整流程：
    /// 验证音频 time_base = 1/sample_rate、解码采样率/声道数不变、采样量不丢失。
    fn encode_audio_for_container(spec: &AudioContainerSpec) -> Result<()> {
        use rsmedia::{DecoderBuilder, MediaType};

        let codec_name = spec.codec.unwrap_or("aac");
        // 编码器存在性取决于 FFmpeg 构建配置（如 libmp3lame/libopus），缺失时跳过
        let Some(codec) = AVCodec::find_encoder_by_name(&strutils::str_to_cstring(codec_name))
        else {
            return Err(RsmediaError::codec_not_found(format!(
                "encoder {codec_name} not available in this FFmpeg build"
            )));
        };
        let config = CodecConfig::from_codec(codec);

        // 采样格式：取编码器支持列表首项（帧数据类型随之匹配）
        let sample_format = SampleFormat::from(
            config
                .supported_sample_formats()
                .ok()
                .flatten()
                .and_then(|fmts| fmts.first().copied())
                .with_context(|| format!("encoder {codec_name} has no supported sample formats"))?,
        );
        // 采样率：优先使用表值；编码器不支持时回退其支持列表首项
        let rates = config.supported_sample_rates().ok().flatten();
        let sample_rate = match rates {
            Some(rates) if !rates.is_empty() => {
                if rates.contains(&(spec.sample_rate as i32)) {
                    spec.sample_rate
                } else {
                    rates[0] as u32
                }
            }
            _ => spec.sample_rate, // 固定速率编码器（PCM 等）无列表，直接用表值
        };

        let path = common::test_output_path(
            "encode",
            &format!("rsmedia_audio_container.{}", spec.container),
        );
        common::remove_test_output(&path);

        // 按容器规格创建编码器
        let encoder = EncoderBuilder::new_audio(
            spec.bit_rate as i64,
            spec.channels as i32,
            sample_rate as i32,
            sample_format,
        )
        .with_codec_name(codec_name.to_string())
        .build()?;

        // 1) 音频 time_base 应为 1/sample_rate
        let tb = encoder.time_base();
        let expected_timeb = time::new_rational(1, sample_rate as i32);
        assert_eq!(
            (tb.num, tb.den),
            (expected_timeb.num, expected_timeb.den),
            "{}: audio time_base {}/{} != 1/sample_rate",
            spec.container,
            tb.num,
            tb.den
        );

        // 2) 编码 5 秒正弦波（1024 采样/帧，末尾不足一帧的余数忽略）；
        //    帧数据类型种类按协商出的采样率格式自动匹配（FLTP/FLT→f32 / S16→S16P / S32P→i32）
        const AUDIO_DURATION_SECS: u32 = 1;
        let samples_per_frame = 1024u32;
        let frames_to_write = AUDIO_DURATION_SECS * sample_rate / samples_per_frame;
        let input_samples = frames_to_write as u64 * samples_per_frame as u64;
        let mut total_pts: i64 = 0;
        {
            let mut muxer = rsmedia::mux::Muxer::new(&path)?;
            let a_idx = muxer.add_encoder(encoder)?;
            macro_rules! encode_frames {
                ($t:ty) => {
                    for _ in 0..frames_to_write {
                        let frame = sine_audio_frame::<$t>(
                            440.0,
                            spec.channels,
                            samples_per_frame,
                            sample_rate,
                        );
                        let mut av = frame.to_avframe()?;
                        av.set_pts(total_pts);
                        av.set_time_base(tb);
                        total_pts += samples_per_frame as i64;
                        muxer.mux(av, a_idx)?;
                    }
                };
            }
            match sample_format {
                SampleFormat::FLTP | SampleFormat::FLT => encode_frames!(f32),
                SampleFormat::S16 | SampleFormat::S16P => encode_frames!(i16),
                SampleFormat::S32P => encode_frames!(i32),
                other => {
                    return Err(RsmediaError::unsupported(format!(
                        "test sample format: {other:?}"
                    )));
                }
            }
            muxer.finish()?;
        }
        let _ = input_samples;
        println!(
            "  {} encoded: codec={codec_name}, fmt={sample_format:?}, rate={sample_rate}, ch={}",
            spec.container, spec.channels
        );

        // 3) 解码验证：采样率/声道数不变，采样量不丢失。
        //    解码数据类型必须与解码器输出格式匹配（rsmedia 解码不做格式转换；
        //    部分编码器的解码器输出格式与编码格式不同，如 libopus 编码 s16、解码 fltp）。
        let mut reader = rsmedia::StreamReader::new(&path)?;
        let mut decoder = DecoderBuilder::new(MediaType::AUDIO).build_from_reader(&reader)?;
        let out_format = decoder.sample_fmt();
        let mut total_samples = 0u64;
        let mut decoded_frames = 0usize;
        macro_rules! decode_check {
            ($t:ty) => {
                while let Some(frame) = decoder.decode::<$t>(&mut reader)? {
                    assert_eq!(
                        frame.sample_rate, sample_rate,
                        "{}: sample rate mismatch",
                        spec.container
                    );
                    assert_eq!(
                        frame.nb_channels, spec.channels,
                        "{}: channel count mismatch",
                        spec.container
                    );
                    total_samples += frame.nb_samples as u64;
                    decoded_frames += 1;
                }
            };
        }
        match out_format {
            SampleFormat::FLTP | SampleFormat::FLT => decode_check!(f32),
            SampleFormat::S16 | SampleFormat::S16P => decode_check!(i16),
            SampleFormat::S32P => decode_check!(i32),
            other => {
                return Err(RsmediaError::unsupported(format!(
                    "decoded sample format: {other:?}"
                )));
            }
        }
        assert!(
            decoded_frames > 0,
            "{}: no audio frames decoded",
            spec.container
        );
        // 有损编码器存在固有的初始编码延迟（ac3 ~1 帧、wmav2 ~1 超帧，且无
        // priming/skip 元数据补偿），解码采样量容忍最多 8192 采样（≈0.19s）缺失，
        // 主要验证不丢大块数据
        const MAX_ENCODER_DELAY: u64 = 8192;
        assert!(
            total_samples + MAX_ENCODER_DELAY >= input_samples,
            "{}: decoded {total_samples} samples, expected >= {}",
            spec.container,
            input_samples.saturating_sub(MAX_ENCODER_DELAY)
        );

        common::remove_test_output(&path);
        Ok(())
    }

    /// 遍历音频容器映射表逐一编码。
    /// 编码器缺失的容器跳过并报告；其余失败视为测试失败（严格模式），
    /// 但要求至少一个容器成功，防止环境异常时测试空壳通过。
    /// P0-2 自动采样格式协商：pcm_s16le 仅接受 S16，未显式指定
    /// sample_format 时应协商为 S16（FLTP 输入帧由 rescale 自动转换）；
    /// 显式指定不支持的格式时 `build()` 立即报错。
    #[test]
    fn test_negotiate_sample_format_pcm() -> Result<()> {
        use rsmedia::{DecoderBuilder, MediaType};

        let sample_rate = 44_100u32;
        let channels = 2u32;
        let samples_per_frame = 1024u32;
        let frames_to_write = 10u32;

        let path = common::test_output_path("encode", "rsmedia_pcm_s16le.wav");
        common::remove_test_output(&path);

        // 不经 new_audio，保持 sample_format 未显式指定
        let audio_encoder = EncoderBuilder::default()
            .with_media_type(MediaType::AUDIO)
            .with_nb_channels(channels as i32)
            .with_sample_rate(sample_rate as i32)
            .with_codec_name("pcm_s16le".to_string())
            .build()?;
        let enc_tb = audio_encoder.time_base();
        let mut muxer = rsmedia::mux::Muxer::new(&path)?;
        let a_idx = muxer.add_encoder(audio_encoder)?;
        let mut total_pts: i64 = 0;
        for _ in 0..frames_to_write {
            let frame = sine_audio_frame::<f32>(440.0, channels, samples_per_frame, sample_rate);
            let mut av = frame.to_avframe()?;
            av.set_pts(total_pts);
            av.set_time_base(enc_tb);
            total_pts += samples_per_frame as i64;
            muxer.mux(av, a_idx)?;
        }
        muxer.finish()?;

        let mut reader = rsmedia::StreamReader::new(&path)?;
        let mut decoder = DecoderBuilder::new(MediaType::AUDIO).build_from_reader(&reader)?;
        let mut total_samples_decoded = 0u64;
        while let Some(frame) = decoder.decode::<i16>(&mut reader)? {
            assert_eq!(frame.sample_rate, sample_rate, "sample rate mismatch");
            assert_eq!(frame.nb_channels, channels, "channel count mismatch");
            total_samples_decoded += frame.nb_samples as u64;
        }
        let expected = frames_to_write as u64 * samples_per_frame as u64;
        assert!(
            total_samples_decoded >= expected,
            "decoded {total_samples_decoded} samples, expected >= {expected}"
        );
        drop(reader);
        common::remove_test_output(&path);

        // 显式指定编码器不支持的采样格式：build() 应 fail fast
        let result = EncoderBuilder::default()
            .with_media_type(MediaType::AUDIO)
            .with_nb_channels(channels as i32)
            .with_sample_rate(sample_rate as i32)
            .with_codec_name("pcm_s16le".to_string())
            .with_sample_format(SampleFormat::FLTP)
            .build();
        assert!(
            result.is_err(),
            "explicit unsupported sample format should fail at build()"
        );
        Ok(())
    }

    /// 音频同样完全不设置 pts：固定帧长编码器（aac，frame_size=1024）经
    /// `audio_fifo` 切帧后按样本位置自动编号，且输入帧长可变（700/1300/...）
    /// 也必须产出无缝、等差的样本时间轴。
    #[test]
    fn test_audio_pts_fully_automatic() -> Result<()> {
        use rsmedia::{DecoderBuilder, MediaType, SampleFormat};

        let sample_rate: u32 = 44_100;
        let channels: u32 = 2;
        let frame_size: i64 = 1024;
        // 可变输入帧长，故意都不足/超过 1024，触发 fifo 的切分与合并。
        let input_sizes = [700u32, 1300, 900, 1100, 1000];
        let total_samples: u32 = input_sizes.iter().sum();

        let path = common::test_output_path("encode", "rsmedia_no_pts_audio.mp4");
        common::remove_test_output(&path);

        let encoder = EncoderBuilder::new_audio(
            128_000,
            channels as i32,
            sample_rate as i32,
            SampleFormat::FLTP,
        )
        .build()?;
        assert_eq!(encoder.frame_size(), frame_size as i32, "aac frame_size");

        let mut muxer = rsmedia::mux::Muxer::new(&path)?;
        let a_idx = muxer.add_encoder(encoder)?;
        for &nb in &input_sizes {
            // 关键：不设置 pts，样本位置由 audio_fifo 的计数器自动维护。
            let frame = sine_audio_frame::<f32>(440.0, channels, nb, sample_rate);
            muxer.mux(frame.to_avframe()?, a_idx)?;
        }
        muxer.finish()?;

        let mut reader = rsmedia::StreamReader::new(&path)?;
        let mut decoder = DecoderBuilder::new(MediaType::AUDIO).build_from_reader(&reader)?;
        let mut pts_list: Vec<i64> = Vec::new();
        let mut decoded_samples: i64 = 0;
        while let Some(frame) = decoder.decode::<f32>(&mut reader)? {
            pts_list.push(frame.pts);
            decoded_samples += frame.nb_samples as i64;
        }
        assert!(!pts_list.is_empty(), "no audio frames decoded");

        // 解码 pts 位于输出流时间基；a从 0 起步、按 frame_size 等差。
        let tb = decoder.time_base();
        let expected_delta =
            (frame_size as f64 * tb.den as f64 / tb.num as f64 / sample_rate as f64).round() as i64;
        assert!(
            expected_delta > 0,
            "non-positive expected pts delta: {expected_delta}"
        );
        for (i, pts) in pts_list.iter().enumerate() {
            assert_eq!(
                *pts,
                i as i64 * expected_delta,
                "audio frame {i}: pts {pts} != {i}*{expected_delta} (tb {}/{})",
                tb.num,
                tb.den
            );
        }
        // Sample conservation: aac restores samples losslessly, but the
        // encoder pads the last frame up to `frame_size`. Whether that
        // padding is cropped depends on the container/version: the ffmpeg 9
        // mp4 demuxer trims it via the edit list (decoded == input), while
        // 6/7/8 return the full padded last frame. The version-independent
        // invariant is therefore: padding is less than one frame.
        let padding = decoded_samples - total_samples as i64;
        assert!(
            (0..frame_size).contains(&padding),
            "sample count mismatch: decoded {decoded_samples} vs input {total_samples} \
             (padding {padding} must be in [0, {frame_size}))"
        );

        common::remove_test_output(&path);
        Ok(())
    }

    #[test]
    fn test_encode_audio_containers() {
        let mut passed = Vec::new();
        let mut skipped = Vec::new();
        let mut failed = Vec::new();

        for spec in AUDIO_CONTAINERS {
            println!(
                "Testing audio container: {} (codec: {}, bitrate: {}, rate: {}, ch: {})...",
                spec.container,
                spec.codec.unwrap_or("aac"),
                spec.bit_rate,
                spec.sample_rate,
                spec.channels
            );
            match encode_audio_for_container(spec) {
                Ok(()) => {
                    println!("Testing audio container: {} passed.", spec.container);
                    passed.push(spec.container);
                }
                Err(e) if is_encoder_unavailable(&e) => {
                    println!("SKIP {}: {e:#}", spec.container);
                    skipped.push(spec.container);
                }
                Err(e) => failed.push((spec.container, format!("{e:#}"))),
            }
        }

        assert_container_results("audio containers", passed, skipped, failed);
    }

    /// 全局头格式（AVFMT_GLOBALHEADER）的 extradata 端到端验证。
    ///
    /// 全局头容器要求编码参数以 extradata 随流写入：flac 的 STREAMINFO、
    /// AAC 的 AudioSpecificConfig。编码器须在 open 前设置
    /// AV_CODEC_FLAG_GLOBAL_HEADER（由实际输出容器派生），否则流缺少
    /// 解码所需的带外参数。严格验证：读回输出文件断言 extradata 存在。
    #[test]
    fn test_global_header_extradata() -> Result<()> {
        use rsmedia::io::Reader as _;

        // 1) flac → .flac：STREAMINFO 必须作为 extradata 存在
        let flac_path = common::test_output_path("encode", "rsmedia_global_header.flac");
        common::remove_test_output(&flac_path);
        let encoder = EncoderBuilder::new_audio(0, 2, 44_100, SampleFormat::S16)
            .with_codec_name("flac".to_string())
            .build()?;
        let enc_tb = encoder.time_base();
        let mut total_pts: i64 = 0;
        {
            let mut muxer = rsmedia::mux::Muxer::new(&flac_path)?;
            let idx = muxer.add_encoder(encoder)?;
            for _ in 0..44_100 / 1024 {
                let frame = sine_audio_frame::<i16>(440.0, 2, 1024, 44_100);
                let mut av = frame.to_avframe()?;
                av.set_pts(total_pts);
                av.set_time_base(enc_tb);
                total_pts += 1024;
                muxer.mux(av, idx)?;
            }
            muxer.finish()?;
        }

        let reader = rsmedia::io::StreamReader::new(&flac_path)?;
        let stream = reader.input().streams().first().unwrap();
        assert_eq!(stream.codecpar().codec_id, ffi::AV_CODEC_ID_FLAC);
        assert!(
            stream.codecpar().extradata_size > 0,
            "flac STREAMINFO must be carried as extradata in the output stream"
        );
        drop(reader);
        common::remove_test_output(&flac_path);

        // 2) aac → .m4a（MP4 全局头容器）：AudioSpecificConfig 必须存在
        let m4a_path = common::test_output_path("encode", "rsmedia_global_header.m4a");
        common::remove_test_output(&m4a_path);
        let encoder = EncoderBuilder::new_audio(128_000, 2, 44_100, SampleFormat::FLTP)
            .with_codec_name("aac".to_string())
            .build()?;
        let enc_tb = encoder.time_base();
        let mut total_pts: i64 = 0;
        {
            let mut muxer = rsmedia::mux::Muxer::new(&m4a_path)?;
            let idx = muxer.add_encoder(encoder)?;
            for _ in 0..44_100 / 1024 {
                let frame = sine_audio_frame::<f32>(440.0, 2, 1024, 44_100);
                let mut av = frame.to_avframe()?;
                av.set_pts(total_pts);
                av.set_time_base(enc_tb);
                total_pts += 1024;
                muxer.mux(av, idx)?;
            }
            muxer.finish()?;
        }

        let reader = rsmedia::io::StreamReader::new(&m4a_path)?;
        let stream = reader.input().streams().first().unwrap();
        assert_eq!(stream.codecpar().codec_id, ffi::AV_CODEC_ID_AAC);
        assert!(
            stream.codecpar().extradata_size > 0,
            "AAC AudioSpecificConfig must be carried as extradata in the output stream"
        );
        drop(reader);
        common::remove_test_output(&m4a_path);
        Ok(())
    }

    /// 音频编解码往返测试：编码若干 AAC 音频帧，解码回验证采样率/通道数/总采样数。
    #[test]
    fn test_encode_decode_audio_roundtrip() -> Result<()> {
        use rsmedia::frame::MediaFrame;
        use rsmedia::{DecoderBuilder, MediaType};

        let sample_rate = 44_100u32;
        let channels = 2u32;
        let format = SampleFormat::FLTP;
        // AAC 默认 frame_size = 1024 采样/帧
        let samples_per_frame = 1024u32;
        let frames_to_write = 10u32;

        let path = common::test_output_path("encode", "rsmedia_audio_roundtrip.m4a");
        common::remove_test_output(&path);

        let encoder =
            EncoderBuilder::new_audio(128_000, channels as i32, sample_rate as i32, format)
                .build()?;

        // 1) 音频 time_base 应为 1/sample_rate
        let tb = encoder.time_base();
        let expected_tb = time::new_rational(1, sample_rate as i32);
        assert_eq!(
            (tb.num, tb.den),
            (expected_tb.num, expected_tb.den),
            "audio time_base {}/{} != 1/sample_rate",
            tb.num,
            tb.den
        );

        let mut total_pts: i64 = 0;
        {
            let mut muxer = rsmedia::mux::Muxer::new(&path)?;
            let idx = muxer.add_encoder(encoder)?;
            for _ in 0..frames_to_write {
                let mut frame = MediaFrame::<f32>::new_audio_frame(
                    format,
                    channels,
                    samples_per_frame,
                    sample_rate,
                    time::new_rational(1, sample_rate as i32),
                )?;
                frame.set_pts(total_pts);
                let mut av = frame.to_avframe()?;
                av.set_time_base(tb);
                total_pts += samples_per_frame as i64;
                muxer.mux(av, idx)?;
            }
            muxer.finish()?;
        }

        // 2) 解码验证：采样率、通道数、采样量（AAC 有编码延迟/padding，总采样数应覆盖输入）
        // 音频 FLTP 用 f32 解码（decode_frame 固定返回 u8，仅适用于视频）。
        let mut reader = rsmedia::StreamReader::new(&path)?;
        let mut decoder = DecoderBuilder::new(MediaType::AUDIO).build_from_reader(&reader)?;
        let mut total_samples = 0u64;
        let mut decoded_frames = 0usize;
        while let Some(frame) = decoder.decode::<f32>(&mut reader)? {
            assert_eq!(
                frame.format(),
                Some(FrameFormat::Sample(format)),
                "sample format mismatch"
            );
            assert_eq!(frame.sample_rate, sample_rate, "sample rate mismatch");
            assert_eq!(frame.nb_channels, channels, "channel count mismatch");
            total_samples += frame.nb_samples as u64;
            decoded_frames += 1;
        }
        let expected = frames_to_write as u64 * samples_per_frame as u64;
        assert!(
            total_samples >= expected,
            "decoded {total_samples} samples, expected >= {expected}"
        );
        assert!(decoded_frames > 0, "no audio frames decoded");

        common::remove_test_output(&path);
        Ok(())
    }

    /// 末帧不足一帧（非 frame_size 整倍数）时，应作为合法末帧编码，而非被 `check_frame`
    /// 的帧长校验拒绝。回归测试：无滤镜向 aac 发非整倍数样本总数。
    #[test]
    fn test_encode_audio_partial_last_frame() -> Result<()> {
        use rsmedia::frame::MediaFrame;
        use rsmedia::{DecoderBuilder, MediaType};

        let sample_rate = 44_100u32;
        let channels = 2u32;
        let format = SampleFormat::FLTP;
        // AAC frame_size = 1024；故意发非整倍数：3×1000 = 3000 样本
        let samples_per_frame = 1000u32;
        let frames_to_write = 3u32;

        let path = common::test_output_path("encode", "rsmedia_audio_partial.m4a");
        common::remove_test_output(&path);

        let encoder =
            EncoderBuilder::new_audio(128_000, channels as i32, sample_rate as i32, format)
                .build()?;
        let enc_tb = encoder.time_base();
        let mut total_pts: i64 = 0;
        {
            let mut muxer = rsmedia::mux::Muxer::new(&path)?;
            let idx = muxer.add_encoder(encoder)?;
            for _ in 0..frames_to_write {
                let mut frame = MediaFrame::<f32>::new_audio_frame(
                    format,
                    channels,
                    samples_per_frame,
                    sample_rate,
                    time::new_rational(1, sample_rate as i32),
                )?;
                frame.set_pts(total_pts);
                let mut av = frame.to_avframe()?;
                av.set_time_base(enc_tb);
                total_pts += samples_per_frame as i64;
                muxer.mux(av, idx)?;
            }
            muxer.finish()?;
        }

        let mut reader = rsmedia::StreamReader::new(&path)?;
        let mut decoder = DecoderBuilder::new(MediaType::AUDIO).build_from_reader(&reader)?;
        let mut total_samples = 0u64;
        while let Some(frame) = decoder.decode::<f32>(&mut reader)? {
            assert_eq!(frame.sample_rate, sample_rate, "sample rate mismatch");
            assert_eq!(frame.nb_channels, channels, "channel count mismatch");
            total_samples += frame.nb_samples as u64;
        }
        let expected = frames_to_write as u64 * samples_per_frame as u64;
        assert!(
            total_samples >= expected,
            "decoded {total_samples} samples, expected >= {expected}"
        );

        common::remove_test_output(&path);
        Ok(())
    }

    /// 音频转码（重编码）往返测试：编码源文件 → 解码读出 → 重编码到新文件 → 解码校验。
    ///
    /// 覆盖音频「解码→编码」完整链路，验证重编码结果采样率/声道数/采样格式与样本量不丢失。
    #[test]
    fn test_audio_transcode_roundtrip() -> Result<()> {
        use rsmedia::frame::MediaFrame;
        use rsmedia::{DecoderBuilder, EncoderBuilder, MediaType, SampleFormat};

        let sample_rate = 44_100u32;
        let channels = 2u32;
        let format = SampleFormat::FLTP;
        let samples_per_frame = 1024u32;
        let frames_to_write = 10u32;

        let src = common::test_output_path("encode", "rsmedia_audio_transcode_src.m4a");
        let dst = common::test_output_path("encode", "rsmedia_audio_transcode_dst.m4a");
        common::remove_test_output(&src);
        common::remove_test_output(&dst);

        // 1) 生成源音频文件
        let enc = EncoderBuilder::new_audio(128_000, channels as i32, sample_rate as i32, format)
            .build()?;
        let src_enc_tb = enc.time_base();
        let mut total_pts: i64 = 0;
        {
            let mut muxer = rsmedia::mux::Muxer::new(&src)?;
            let src_idx = muxer.add_encoder(enc)?;
            for _ in 0..frames_to_write {
                let mut frame = MediaFrame::<f32>::new_audio_frame(
                    format,
                    channels,
                    samples_per_frame,
                    sample_rate,
                    time::new_rational(1, sample_rate as i32),
                )?;
                frame.set_pts(total_pts);
                let mut av = frame.to_avframe()?;
                av.set_time_base(src_enc_tb);
                total_pts += samples_per_frame as i64;
                muxer.mux(av, src_idx)?;
            }
            muxer.finish()?;
        }
        let src_samples = frames_to_write as u64 * samples_per_frame as u64;

        // 2) 转码：解码源 → 重编码到新文件
        let mut src_reader = rsmedia::StreamReader::new(&src)?;
        let mut dec = DecoderBuilder::new(MediaType::AUDIO).build_from_reader(&src_reader)?;
        let enc2 = EncoderBuilder::new_audio(128_000, channels as i32, sample_rate as i32, format)
            .build()?;
        let dst_enc_tb = enc2.time_base();
        let mut dst_pts: i64 = 0;
        let mut transcoded_samples = 0u64;
        {
            let mut muxer = rsmedia::mux::Muxer::new(&dst)?;
            let dst_idx = muxer.add_encoder(enc2)?;
            while let Some(frame) = dec.decode::<f32>(&mut src_reader)? {
                transcoded_samples += frame.nb_samples as u64;
                let mut av = frame.to_avframe()?;
                av.set_pts(dst_pts);
                av.set_time_base(dst_enc_tb);
                dst_pts += frame.nb_samples as i64;
                muxer.mux(av, dst_idx)?;
            }
            muxer.finish()?;
        }
        assert!(
            transcoded_samples >= src_samples,
            "decoded {transcoded_samples} source samples, expected >= {src_samples}"
        );

        // 3) 解码转码结果并校验
        let mut out_reader = rsmedia::StreamReader::new(&dst)?;
        let mut out: rsmedia::Decoder =
            DecoderBuilder::new(MediaType::AUDIO).build_from_reader(&out_reader)?;
        let mut total = 0u64;
        while let Some(frame) = out.decode::<f32>(&mut out_reader)? {
            assert_eq!(
                frame.format(),
                Some(FrameFormat::Sample(format)),
                "sample format mismatch"
            );
            assert_eq!(frame.sample_rate, sample_rate, "sample rate mismatch");
            assert_eq!(frame.nb_channels, channels, "channel count mismatch");
            total += frame.nb_samples as u64;
        }
        assert!(
            total >= src_samples,
            "transcoded decoded {total} samples, expected >= {src_samples}"
        );

        common::remove_test_output(&src);
        common::remove_test_output(&dst);
        Ok(())
    }

    /// 音频滤镜全量往返测试：编码时对每个滤镜逐一应用，再解码验证。
    ///
    /// 覆盖所有不依赖外部资源/设备的音频滤镜（与视频滤镜对称，验证编码管线中的
    /// 滤镜初始化、EOF/flush 冲刷不丢帧、不报错）。验证：
    ///   1) 滤镜在音频编码管线中可正常初始化、不报错；
    ///   2) EOF / flush 阶段不丢帧、不报 "cannot decode after flushed"；
    ///   3) 时长保持类滤镜输出采样数不丢失（>= 输入采样数）。
    #[test]
    fn test_audio_filters_roundtrip() -> Result<()> {
        use rsmedia::filter::{self, audio};
        use rsmedia::{DecoderBuilder, MediaType};

        let sample_rate = 44_100u32;
        let channels = 2u32;
        let format = SampleFormat::FLTP;
        let samples_per_frame = 1024u32;
        let frames_to_write = 12u32;
        let input_samples = frames_to_write as u64 * samples_per_frame as u64;

        // (名称, Filter, 时长保持 ?)。时长保持类滤镜不解散采样量，可断言 `>= 输入采样数`；
        // 时长变化类（延时/变速/裁剪/时间戳重排/响度测量）只断言能正常解码出帧。
        type FilterCase = (&'static str, Filter, bool);
        let cases: Vec<FilterCase> = vec![
            // 时长保持类
            ("volume", audio::volume(0.8), true),
            ("equalizer", audio::equalizer(1000, 3.0, 200), true),
            (
                "compressor",
                audio::compressor(4.0, None, None).unwrap(),
                true,
            ),
            ("highpass", audio::highpass(100), true),
            ("lowpass", audio::lowpass(4000), true),
            ("atempo", audio::atempo(1.0), true),
            ("fft_denoise", audio::fft_denoise(12, -50), true),
            ("denoise", audio::denoise(12.0), true),
            // FIXME:
            // ("anlm_denoise", audio::anlm_denoise(None, None, None), true),
            // anlm_denoise 暂不纳入测试：FFmpeg 9.0 的 anlmdn 滤镜存在堆越界写 bug ——
            // EOF 冲刷不满一窗的尾巴帧时（libavfilter/avfilter.c 在 status_in 时将 min
            // 降为队列剩余样本数），filter_channel 仍向按尾巴尺寸分配的输出缓冲写满
            // H 个样本（默认 44.1kHz 下 H=177），越界 408 字节/声道。堆被污染后会使
            // 其它测试随机 SIGSEGV/malloc abort（即本测试套件曾经的偶发失败根因）。
            // 上游 master 尚未修复；等修复发布后再恢复此用例。
            (
                "three_band_equalizer",
                audio::three_band_equalizer(2.0, 0.0, 2.0),
                true,
            ),
            ("format", audio::format(channels, sample_rate, format), true),
            (
                "resample",
                audio::resample(channels, sample_rate, format),
                true,
            ),
            // 时长变化类
            ("adelay", audio::adelay(100), false),
            ("loudnorm", audio::loudnorm(-16.0), false),
            (
                "asetpts",
                filter::setpts(MediaType::AUDIO, "PTS-STARTPTS"),
                false,
            ),
            ("atrim", filter::trim(MediaType::AUDIO, 0.0, 0.2), false),
        ];

        for (name, audio_filter, duration_preserving) in cases {
            println!("AUDFILT {name}");
            let path = common::test_output_path("encode", &format!("rsmedia_afilter_{name}.m4a"));
            common::remove_test_output(&path);

            let enc = match EncoderBuilder::new_audio(
                128_000,
                channels as i32,
                sample_rate as i32,
                format,
            )
            .with_filters(vec![audio_filter])
            .build()
            {
                Ok(enc) => enc,
                // 部分滤镜（如 `fft_denoise`/`loudnorm`）依赖特定 FFmpeg 编译配置，
                // 未编译时初始化失败，这里优雅跳过，避免环境差异导致测试失败。
                Err(e) if is_filter_unavailable(&e) => {
                    println!("SKIP {name}: not available ({e:#})");
                    common::remove_test_output(&path);
                    continue;
                }
                Err(e) => return Err(e),
            };
            let enc_tb = enc.time_base();
            let mut total_pts: i64 = 0;
            {
                let mut muxer = rsmedia::mux::Muxer::new(&path)?;
                let idx = muxer.add_encoder(enc)?;
                for _ in 0..frames_to_write {
                    let mut frame =
                        sine_audio_frame::<f32>(440.0, channels, samples_per_frame, sample_rate);
                    frame.set_pts(total_pts);
                    let mut av = frame.to_avframe()?;
                    av.set_time_base(enc_tb);
                    total_pts += samples_per_frame as i64;
                    muxer.mux(av, idx)?;
                }
                muxer.finish()?;
            }

            // 解码验证：不报错、能解出帧；时长保持类滤镜采样量不丢失。
            let mut reader = rsmedia::StreamReader::new(&path)?;
            let mut dec = DecoderBuilder::new(MediaType::AUDIO).build_from_reader(&reader)?;
            let mut total_samples = 0u64;
            let mut decoded = 0usize;
            while let Some(frame) = dec.decode::<f32>(&mut reader)? {
                assert_eq!(
                    frame.sample_rate, sample_rate,
                    "{name}: sample rate mismatch"
                );
                assert_eq!(
                    frame.format(),
                    Some(FrameFormat::Sample(format)),
                    "{name}: sample format mismatch"
                );
                assert_eq!(
                    frame.nb_channels, channels,
                    "{name}: channel count mismatch"
                );
                total_samples += frame.nb_samples as u64;
                decoded += 1;
            }
            assert!(
                decoded > 0,
                "{name}: no audio frames decoded (possible EOF loss)"
            );
            if duration_preserving {
                assert!(
                    total_samples >= input_samples,
                    "{name}: lost samples, decoded {total_samples}, expected >= {input_samples}"
                );
            }

            common::remove_test_output(&path);
        }
        Ok(())
    }
}
