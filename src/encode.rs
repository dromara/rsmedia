use crate::codec::{CodecConfig, CodecContextState};
use crate::error::{Context, Result, RsmediaError};
use crate::filter::{AudioParams, Filter, FilterGraph, FilterParams, VideoParams};
use crate::fmt::{AVFormatFlag, FrameFormat};
#[cfg(feature = "ndarray")]
use crate::frame::{MediaFrame, MediaFrameType};
use crate::hwaccel::{HWContext, HWDeviceConfig};
use crate::io::Writer;
use crate::options::{CRF_CAPABLE_CODECS, Options, Quality, VideoProfile};
use crate::pixel::PixelFormat;
use crate::resample;
use crate::scale::{ScaleAlgorithm, ScaleQuality, Scaler};
use crate::strutils;
use crate::subtitle::SubtitleSegment;
use crate::time::{self, Rescale};
use crate::{MediaType, SampleFormat};

use rsmpeg::avcodec::{AVCodec, AVCodecContext, AVCodecParameters, AVPacket, AVSubtitle};
use rsmpeg::avutil::{self, AVAudioFifo, AVChannelLayout, AVChannelLayoutRef, AVFrame};
use rsmpeg::ffi;

use std::collections::VecDeque;
use std::sync::Arc;

/// Builds an [`Encoder`].
#[derive(Debug)]
pub struct EncoderBuilder {
    /// Video
    fps: f32,
    width: usize,
    height: usize,
    /// `None` = 未显式指定，`build()` 时按编码器支持列表自动协商。
    pixel_format: Option<PixelFormat>,
    /// Audio
    nb_channels: i32,
    sample_rate: i32,
    /// `None` = 未显式指定，`build()` 时按编码器支持列表自动协商。
    sample_format: Option<SampleFormat>,
    /// Common
    bit_rate: i64,
    gop_size: i32,
    max_b_frames: i32,
    time_base: ffi::AVRational,
    pkt_time_base: ffi::AVRational,
    frame_rate: ffi::AVRational,
    /// config
    ofmt_flag: u32,
    thread_count: usize,
    media_type: MediaType,
    codec_name: Option<String>,
    codec_opts: Option<Options>,
    level: Option<String>,
    quality: Option<Quality>,
    profile: Option<VideoProfile>,
    /// Subtitle ASS script header (`[Script Info]` + `[V4+ Styles]` + `[Events]`
    /// format line). Required for subtitle encoders; in a transcode pipeline
    /// forward it from the decoded subtitle stream instead of hand-crafting one.
    subtitle_header: Option<String>,
    filters: Option<Vec<Filter>>,
    hw_device_config: Option<HWDeviceConfig>,
    /// 缩放核选择（互斥，只取一个算法位）
    scale_algorithm: ScaleAlgorithm,
    /// 缩放质量位（可多位，见 [`ScaleQuality`]）；构建 `Scaler` 时由 [`ScaleQuality::mask`] 合成为掩码
    scale_quality: Vec<ScaleQuality>,
}

impl EncoderBuilder {
    /// This is the assumed FPS for the encoder to use.
    /// Note that this does not need to be correct exactly.
    const FRAME_RATE: i32 = 30;

    /// Max numerator/denominator when converting a float fps via `av_d2q`.
    const FPS_MAX: i32 = 100_000;

    /// Default bit rate.
    /// 分辨率(width, height) + 推荐比特率（单位：bps）
    /// * 标清 Sd_480p:          (640, 480)   => 1_000_000,   // 1 Mbps
    /// * 高清 Hd_720p:          (1280, 720)  => 2_500_000,   // 2.5 Mbps
    /// * 全高清 FullHd(1080p):  (1920, 1080) => 5_000_000,   // 5 Mbps
    /// * 超高清 FullHd_2k:      (2560, 1440) => 8_000_000,   // 8 Mbps
    /// * 超高清 UltraHd_4K:     (3840, 2160) => 20_000_000,  // 20 Mbps
    /// * 超高清 FullUltraHd_8K: (7680, 4320) => 60_000_000,  // 60 Mbps
    const VIDEO_BIT_RATE: i64 = 1_000_000;

    /// default video codec
    const VIDEO_CODEC_NAME: &'static str = "libx264";
    /// default audio codec
    const AUDIO_CODEC_NAME: &'static str = "aac";
    /// 字幕默认编码器：subrip（通用文本格式）。MP4 容器请用 指定 [`mov_text`]
    const SUBTITLE_CODEC_NAME: &'static str = "subrip";

    /// 字幕编码器时间基分母：1/1000 秒（毫秒精度），与 ffmpeg CLI 行为一致。
    const SUBTITLE_TIME_BASE_DEN: i32 = 1000;

    /// 单条字幕编码缓冲区大小。mov_text 载荷 = 2 字节大端长度 + 文本，
    /// subrip = 纯文本；按文本长度的 2 倍 + 256 分配已足够宽裕。
    const SUBTITLE_BUFFER_SIZE: usize = 8192;

    /// Create a video encoder with the specified destination
    ///
    /// The default codec is `libx264`, with default frame rate and bit rate.
    ///
    /// # Arguments
    ///
    /// * `width` - The width of the video stream.
    /// * `height` - The height of the video stream.
    pub fn new_video(width: usize, height: usize) -> Self {
        Self::default().with_width(width).with_height(height)
    }

    /// Create an audio encoder with the specified parameters.
    ///
    /// The default codec is `aac`, with default bit rate of 128k.
    ///
    /// # Arguments
    ///
    /// * `bit_rate` - The bit rate of the audio stream.
    /// * `nb_channels` - The number of channels in the audio stream.
    /// * `sample_rate` - The sample rate of the audio stream.
    /// * `sample_format` - The sample format of the audio stream.
    pub fn new_audio(
        bit_rate: i64,
        nb_channels: i32,
        sample_rate: i32,
        sample_format: SampleFormat,
    ) -> Self {
        Self::default()
            .with_bit_rate(bit_rate)
            .with_nb_channels(nb_channels)
            .with_sample_rate(sample_rate)
            .with_sample_format(sample_format)
            .with_media_type(MediaType::AUDIO)
    }

    /// Create a subtitle encoder.
    ///
    /// The default codec is `subrip`; for MP4 output select `mov_text` via
    /// [`Self::with_codec_name`]. The encoder time base defaults to 1/1000
    /// (millisecond precision), matching the ffmpeg CLI.
    ///
    /// A subtitle encoder requires an ASS script header before opening:
    /// provide it via [`Self::with_subtitle_header`] — in a transcode pipeline
    /// forward the header from the decoded subtitle stream, otherwise pass a
    /// complete `[Script Info]` / `[V4+ Styles]` / `[Events]` header (see the
    /// `subtitle` example).
    pub fn new_subtitle() -> Self {
        Self::default().with_media_type(MediaType::SUBTITLE)
    }

    /// Set the ASS script header for subtitle encoders.
    ///
    /// Subtitle encoders (subrip, mov_text, ...) fail to open with
    /// `AVERROR_INVALIDDATA` unless an ASS header is set. An empty or partial
    /// header opens but silently mis-parses dialogues, so always provide a
    /// complete header. In a transcode pipeline copy the header from the
    /// decoder side (it is populated by subtitle decoders on open).
    pub fn with_subtitle_header(mut self, header: impl Into<String>) -> Self {
        self.subtitle_header = Some(header.into());
        self
    }

    /// Set the width of the video stream.
    pub fn with_width(mut self, width: usize) -> Self {
        self.width = width;
        self
    }

    /// Set the height of the video stream.
    pub fn with_height(mut self, height: usize) -> Self {
        self.height = height;
        self
    }

    /// Set the codec name.
    /// video codec default is `libx264`
    /// audio codec default is `aac`
    pub fn with_codec_name(mut self, codec_name: impl Into<Option<String>>) -> Self {
        self.codec_name = codec_name.into();
        self
    }

    /// Set the thread count.
    pub fn with_thread_count(mut self, thread_count: usize) -> Self {
        self.thread_count = thread_count;
        self
    }

    /// Set the bit rate.
    pub fn with_bit_rate(mut self, bit_rate: i64) -> Self {
        self.bit_rate = bit_rate;
        self
    }

    /// Set the rate control strategy (video encoders only; ignored for audio).
    ///
    /// * [`Quality::Crf`] — quality-targeted encoding. Applied via the codec's
    ///   `crf` private option for the encoders in [`CRF_CAPABLE_CODECS`]; other
    ///   codecs fall back to [`Self::with_bit_rate`] with a warning and the
    ///   stream bit rate is left untouched.
    /// * [`Quality::Bitrate`] — explicit target bit rate, overriding
    ///   [`Self::with_bit_rate`].
    pub fn with_quality(mut self, quality: Quality) -> Self {
        self.quality = Some(quality);
        self
    }

    /// Set the H.264-style profile (`baseline`, `main`, `high`, `high10`,
    /// `high422`, `high444`) via the codec's `profile` private option.
    ///
    /// Works out of the box for `libx264`; other encoders map what they
    /// support and silently ignore unknown profile names (check the encoder
    /// documentation, or pass a codec-specific option via
    /// [`Self::with_options`]). Video only.
    pub fn with_profile(mut self, profile: VideoProfile) -> Self {
        self.profile = Some(profile);
        self
    }

    /// Set the codec level, e.g. `"4.1"` for H.264 ( Annex A level) — passed
    /// through to the codec's `level` private option when available
    /// (`libx264` accepts Annex A strings like `"4.1"`). Video only.
    pub fn with_level(mut self, level: impl ToString) -> Self {
        self.level = Some(level.to_string());
        self
    }

    // /// Set the frame rate.
    // pub fn with_frame_rate_ra(mut self, frame_rare: ffi::AVRational) -> Self {
    //     self.frame_rate = frame_rare;
    //     self
    // }
    //
    // pub fn with_frame_rate(mut self, num: i32, den: i32) -> Self {
    //     self.frame_rate = time::new_rational(num, den);
    //     self
    // }

    /// Set the video frame rate from a floating-point number of frames per second.
    ///
    /// The value is converted to a reduced rational via FFmpeg's `av_d2q` and used
    /// as the encoder frame rate.
    pub fn with_fps(mut self, fps: f32) -> Self {
        if fps > 0.0 && fps.is_finite() {
            self.fps = fps;
            self.frame_rate = avutil::av_d2q(fps as f64, Self::FPS_MAX);
        }
        self
    }

    // /// Set the time base.
    // pub fn with_time_base_ra(mut self, time_base: ffi::AVRational) -> Self {
    //     self.time_base = time_base;
    //     self
    // }
    //
    // pub fn with_time_base(mut self, num: i32, den: i32) -> Self {
    //     self.time_base = time::new_rational(num, den);
    //     self
    // }
    //
    // /// Set the packet time base.
    // pub fn with_pkt_time_base_ra(mut self, pkt_time_base: ffi::AVRational) -> Self {
    //     self.pkt_time_base = pkt_time_base;
    //     self
    // }
    //
    // pub fn with_pkt_time_base(mut self, num: i32, den: i32) -> Self {
    //     self.pkt_time_base = time::new_rational(num, den);
    //     self
    // }

    /// Set the GOP size.
    pub fn with_gop_size(mut self, gop_size: i32) -> Self {
        self.gop_size = gop_size;
        self
    }

    /// Set the maximum number of B-frames.
    pub fn with_max_b_frames(mut self, max_b_frames: i32) -> Self {
        self.max_b_frames = max_b_frames;
        self
    }

    /// Set the pixel format.
    ///
    /// When not set, [`Self::build`] negotiates one from the encoder's
    /// supported list (preferring [`PixelFormat::YUV420P`]).
    pub fn with_pixel_format(mut self, pixel_format: PixelFormat) -> Self {
        self.pixel_format = Some(pixel_format);
        self
    }

    /// codec options used for encoder
    pub fn with_options(mut self, options: impl Into<Option<Options>>) -> Self {
        self.codec_opts = options.into();
        self
    }

    /// filters used for encoder
    pub fn with_filters(mut self, filters: impl Into<Option<Vec<Filter>>>) -> Self {
        self.filters = filters.into();
        self
    }

    /// Enable hardware acceleration with the specified device type.
    ///
    /// * `device_config` - Device to use for hardware acceleration.
    pub fn with_hardware_device(mut self, device_config: Option<HWDeviceConfig>) -> Self {
        self.hw_device_config = device_config;
        self
    }

    /// Set the scaling algorithm used when converting input frames to the
    /// encoder's target pixel format (e.g. RGB24 -> YUV420P).
    ///
    /// The algorithm picks the scaling kernel and is **mutually exclusive** —
    /// FFmpeg's header states *"Scaler selection options. Only one may be active
    /// at a time."* Defaults to [`ScaleAlgorithm::BICUBIC`]; the quality/behaviour
    /// bits are set separately with [`Self::with_scale_quality`].
    pub fn with_scale_algorithm(mut self, algorithm: ScaleAlgorithm) -> Self {
        self.scale_algorithm = algorithm;
        self
    }

    /// Set the scaling quality/behaviour bits used when converting input frames
    /// to the encoder's target pixel format.
    ///
    /// Unlike the algorithm (exactly one bit), the quality flags are a set: pass the
    /// bits themselves as a list — `[ScaleQuality::BITEXACT]`,
    /// `[ScaleQuality::FULL_CHR_H_INT, ScaleQuality::ACCURATE_RND]`, … — and they are
    /// combined into the mask handed to FFmpeg. Taking a list of [`ScaleQuality`]
    /// values rather than a raw `u32` means an invalid flag cannot be passed.
    /// Defaults to [`ScaleQuality::default_mask`].
    pub fn with_scale_quality(mut self, quality: impl AsRef<[ScaleQuality]>) -> Self {
        self.scale_quality = quality.as_ref().to_vec();
        self
    }

    /// explicit media type, default is `MediaType::VIDEO`
    pub fn with_media_type(mut self, media_type: MediaType) -> Self {
        self.media_type = media_type;
        self
    }

    pub fn with_nb_channels(mut self, nb_channels: i32) -> Self {
        self.nb_channels = nb_channels;
        self
    }

    pub fn with_sample_rate(mut self, sample_rate: i32) -> Self {
        self.sample_rate = sample_rate;
        self
    }

    /// Set the sample format.
    ///
    /// When not set, [`Self::build`] negotiates one from the encoder's
    /// supported list (preferring [`SampleFormat::FLTP`]).
    pub fn with_sample_format(mut self, sample_format: SampleFormat) -> Self {
        self.sample_format = Some(sample_format);
        self
    }

    /// Some formats want stream headers to be separate.
    ///
    /// 显式设置编码器的 `AV_CODEC_FLAG_GLOBAL_HEADER` 相关 flag。注意：使用
    /// [`Muxer`](crate::mux::Muxer) 写容器时，输出容器的 `AVFMT_GLOBALHEADER`
    /// flag 由 muxer 派生，这里显式设置可能无法覆盖。
    pub fn with_oformat_flags(mut self, flag: AVFormatFlag) -> Self {
        self.ofmt_flag = flag.as_raw();
        self
    }

    /// 编码器使用的 time_base。
    ///
    /// 视频由用户 fps 推导为 `1/fps`（libx264 等编码器在这种 time_base 下才
    /// 能正确输出 packet duration，避免 MP4 muxer 丢弃末帧）；音频按
    /// `1/sample_rate` 推导，对所有音频编码器一致。
    fn effective_time_base(&self) -> ffi::AVRational {
        match self.media_type {
            MediaType::VIDEO => avutil::av_inv_q(self.frame_rate),
            MediaType::AUDIO => time::new_rational(1, self.sample_rate),
            // 字幕：1/1000（毫秒精度），与 ffmpeg CLI 一致
            MediaType::SUBTITLE => time::new_rational(1, Self::SUBTITLE_TIME_BASE_DEN),
            _ => self.time_base,
        }
    }

    /// The bit rate actually applied to the codec context: an explicit
    /// [`Quality::Bitrate`] overrides [`Self::bit_rate`].
    fn effective_bit_rate(&self) -> i64 {
        match self.quality {
            Some(Quality::Bitrate(bit_rate)) if bit_rate > 0 => bit_rate,
            _ => self.bit_rate,
        }
    }

    /// Apply the settings to an encoder.
    ///
    /// # Arguments
    ///
    /// * `encoder` - Encoder to apply settings to.
    /// * `use_crf` - Whether CRF rate control is active (skip bit rate).
    ///
    /// # Return value
    ///
    /// New encoder with settings applied.
    fn setup_codec_context(
        &self,
        encoder: &mut AVCodecContext,
        use_crf: bool,
        pixel_format: PixelFormat,
        sample_format: SampleFormat,
        config: &CodecConfig,
    ) -> Result<()> {
        let media_type = self.media_type;
        if media_type as ffi::AVMediaType != encoder.codec_type {
            return Err(RsmediaError::custom(format!(
                "Encoder codec type not supported: {:?} vs. {:?}",
                media_type, encoder.codec_type
            )));
        }

        if media_type == MediaType::VIDEO {
            encoder.set_width(self.width as i32);
            encoder.set_height(self.height as i32);
            // CRF 模式下不设置 bit_rate（CRF 以质量为目标，码率由编码器自行
            // 决定；写默认 1Mbps 会让 muxer 元数据与实际输出不符）。
            if !use_crf {
                encoder.set_bit_rate(self.effective_bit_rate());
            }
            encoder.set_gop_size(self.gop_size);
            encoder.set_max_b_frames(self.max_b_frames);
            encoder.set_framerate(self.frame_rate);
            encoder.set_time_base(self.effective_time_base());
            encoder.set_pkt_timebase(self.pkt_time_base);
            encoder.set_pix_fmt(pixel_format.into());
            encoder.set_sample_aspect_ratio(time::new_rational(1, 1));
        } else if media_type == MediaType::AUDIO {
            if !config.is_support_channel_count(self.nb_channels) {
                return Err(RsmediaError::InvalidConfig(format!(
                    "encoder '{}' does not support nb_channels {}",
                    config.name().to_string_lossy(),
                    self.nb_channels
                )));
            }
            if !config.is_support_sample_rate(self.sample_rate) {
                return Err(RsmediaError::InvalidConfig(format!(
                    "encoder '{}' does not support sample rate {}",
                    config.name().to_string_lossy(),
                    self.sample_rate
                )));
            }
            encoder.set_ch_layout(AVChannelLayout::from_nb_channels(self.nb_channels).into_inner());
            encoder.set_bit_rate(self.effective_bit_rate());
            encoder.set_sample_rate(self.sample_rate);
            encoder.set_sample_fmt(sample_format as _);
            encoder.set_time_base(self.effective_time_base());
        } else if media_type == MediaType::SUBTITLE {
            // 字幕编码器只需 time_base（毫秒精度），无像素/采样格式、码率等概念
            encoder.set_time_base(self.effective_time_base());
        } else {
            return Err(RsmediaError::custom(format!(
                "Unsupported media type: {media_type:?}"
            )));
        }

        // Some formats want stream headers to be separate.
        if self.ofmt_flag & AVFormatFlag::GLOBAL_HEADER.as_raw() != 0 {
            encoder.set_flags(encoder.flags | ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32);
        }
        unsafe {
            (*encoder.as_mut_ptr()).thread_count = self.thread_count as i32;
        }

        Ok(())
    }

    /// 解析编码目标像素格式（P0-2 自动格式协商）。
    ///
    /// * 显式指定（[`Self::with_pixel_format`]]）：软件路径立即校验编码器
    ///   是否支持，不支持时 `build()` 报错（fail fast）；硬件路径跳过校验
    ///   （`setup_encoder_frames` 会按 HW 要求重设 pix_fmt，HW 私有格式不在
    ///   软件支持列表内）。
    /// * 未指定：优先 [`PixelFormat::YUV420P`]（兼容性最好）；编码器不支持
    ///   时（如 mjpeg 仅接受 YUVJ 系）取支持列表首个格式；列表为 `None`
    ///   （FFmpeg 未限制）或查询失败时仍回退 YUV420P。
    fn resolve_pixel_format(&self, config: &CodecConfig, codec_name: &str) -> Result<PixelFormat> {
        match self.pixel_format {
            Some(fmt) => {
                if self.hw_device_config.is_none() && !config.is_support_pixel_format(fmt as i32) {
                    return Err(RsmediaError::InvalidConfig(format!(
                        "encoder '{codec_name}' does not support pixel format {fmt:?}"
                    )));
                }
                Ok(fmt)
            }
            None => {
                let negotiated = match config.supported_pixel_formats() {
                    Ok(Some(list)) if !list.is_empty() => PixelFormat::from(list[0]),
                    _ => PixelFormat::YUV420P,
                };
                log::debug!("negotiated pixel format {negotiated:?} for encoder '{codec_name}'");
                Ok(negotiated)
            }
        }
    }

    /// 解析编码目标采样格式（P0-2 自动格式协商），策略同
    /// [`Self::resolve_pixel_format`]：显式指定则校验（fail fast），
    /// 未指定优先 [`SampleFormat::FLTP`]（aac/mp3 等原生平面浮点格式），
    /// 不支持时取支持列表首个（如 `pcm_s16le` 为 S16）。
    fn resolve_sample_format(
        &self,
        config: &CodecConfig,
        codec_name: &str,
    ) -> Result<SampleFormat> {
        match self.sample_format {
            Some(fmt) => {
                if !config.is_support_sample_format(fmt as i32) {
                    return Err(RsmediaError::InvalidConfig(format!(
                        "encoder '{codec_name}' does not support sample format {fmt:?}"
                    )));
                }
                Ok(fmt)
            }
            None => {
                let negotiated = match config.supported_sample_formats() {
                    Ok(Some(list)) if !list.is_empty() => {
                        if list.contains(&(SampleFormat::FLTP as i32)) {
                            SampleFormat::FLTP
                        } else {
                            SampleFormat::from(list[0])
                        }
                    }
                    _ => SampleFormat::FLTP,
                };
                log::debug!("negotiated sample format {negotiated:?} for encoder '{codec_name}'");
                Ok(negotiated)
            }
        }
    }

    /// Build an [`Encoder`].
    ///
    /// Create an encoder from a [`StreamWriter`].
    ///
    /// # Arguments
    ///
    /// * `writer` - [`StreamWriter`] to create encoder from.
    /// * `interleaved` - Whether to use interleaved write.
    /// * `settings` - Encoder settings to use.
    pub fn build(self) -> Result<Encoder> {
        let media_type = self.media_type;
        let codec_name: String = match &self.codec_name {
            Some(codec_name) => codec_name.clone(),
            None => match media_type {
                MediaType::VIDEO => Self::VIDEO_CODEC_NAME.to_string(),
                MediaType::AUDIO => Self::AUDIO_CODEC_NAME.to_string(),
                MediaType::SUBTITLE => Self::SUBTITLE_CODEC_NAME.to_string(),
                _ => {
                    return Err(RsmediaError::custom(format!(
                        "Unsupported media type:{media_type:?}",
                    )));
                }
            },
        };
        let codec = AVCodec::find_encoder_by_name(&strutils::str_to_cstring(&codec_name))
            .context(format!("Failed to find encoder by name: '{codec_name}'"))?;

        // CRF 速率控制：仅对支持 crf 私有选项的视频编码器生效，其余编码器
        // 回退到 bit_rate 控制（与 ffmpeg CLI 行为一致，只是多一个警告）。
        let use_crf = media_type == MediaType::VIDEO
            && match self.quality {
                Some(Quality::Crf(_)) => {
                    let capable = CRF_CAPABLE_CODECS.contains(&codec_name.as_str());
                    if !capable {
                        log::warn!(
                            "codec '{codec_name}' has no CRF support, falling back to bit rate control"
                        );
                    }
                    capable
                }
                _ => false,
            };

        let mut encode_ctx = AVCodecContext::new(&codec);
        let config = CodecConfig::from_codec(codec);
        // P0-2 自动格式协商：显式指定的格式立即校验（fail fast），未指定的
        // 从编码器支持列表中挑选，避免把帧转进一个编码器不支持的格式后才
        // 在写入阶段报错。字幕编码器无像素/采样格式概念，跳过协商。
        let (pixel_format, sample_format) = if media_type == MediaType::SUBTITLE {
            (PixelFormat::YUV420P, SampleFormat::FLTP)
        } else {
            (
                self.resolve_pixel_format(&config, &codec_name)?,
                self.resolve_sample_format(&config, &codec_name)?,
            )
        };

        self.setup_codec_context(
            &mut encode_ctx,
            use_crf,
            pixel_format,
            sample_format,
            &config,
        )?;

        // 编码器输入时间基：与滤镜图 buffer 源（下方 FilterParams）和"滤镜未改写
        // 帧率时的编码器 time_base"同源。必须在 self 被部分 move 之前求值。
        let input_time_base = self.effective_time_base();

        // 在 hw_device_config / codec_opts 被 move 之前构造 filter graph：
        // 此位置 self 尚未被部分 move，可直接借用 self 计算 time_base。
        // 滤镜链可声明要求的输入格式（如 GIF 调色板链要求 RGB 输入、输出
        // pal8；音频链可声明输入采样格式）；未声明时输入格式=编码器协商格式
        // （src/sink 同格式，零行为变化）。
        let filter_input_format = self
            .filters
            .as_ref()
            .and_then(|filters| filters.iter().find_map(|f| f.input_format()));
        let mut filter_graph = if let Some(filters) = self.filters.as_ref() {
            let filter_params = match media_type {
                MediaType::VIDEO => {
                    FilterParams::Video(VideoParams {
                        width: self.width as i32,
                        height: self.height as i32,
                        src_format: filter_input_format
                            .and_then(FrameFormat::into_pixel)
                            .unwrap_or(pixel_format),
                        format: pixel_format,
                        time_base: input_time_base,
                        frame_rate: self.frame_rate,
                        pixel_aspect: encode_ctx.sample_aspect_ratio, // sample aspect ratio (0 if unknown)
                    })
                }
                MediaType::AUDIO => {
                    FilterParams::Audio(AudioParams {
                        nb_channels: self.nb_channels,
                        sample_rate: self.sample_rate,
                        format: sample_format,
                        src_format: filter_input_format
                            .and_then(FrameFormat::into_sample)
                            .unwrap_or(sample_format),
                        time_base: input_time_base, // time_base = 1 / sample_rate
                    })
                }
                _ => {
                    return Err(RsmediaError::custom(format!(
                        "Unsupported filter for media type: {media_type:?}"
                    )));
                }
            };
            let mut graph = FilterGraph::new();
            // check Filter media type
            if !filters.iter().all(|f| f.media_type() == media_type) {
                return Err(RsmediaError::custom(format!(
                    "Filter media type mismatch for encoder type {media_type:?}"
                )));
            }
            graph
                .init(&filter_params, filters.as_slice())
                .context("Failed to initialize filter graph")?;

            Some(graph)
        } else {
            None
        };

        // 滤镜可能改变输出帧率/时间基（如 `framerate`、`fps`、`setpts`）以及输出尺寸
        // （如 `scale`、`crop`、`pad`、`rotate`、`transpose`）。此时编码器必须采用滤镜
        // 输出帧率/时间基/尺寸，否则按输入参数推导的 time_base 会与滤镜输出 pts 不匹配
        // （B 帧重排 dts 乱序、mux 报错），或 codec context 尺寸与滤镜输出帧尺寸不符
        // 导致 send_frame 报错。
        // 注：滤镜输出时间基无需在此缓存——`send_frame_post_filter` 会在发送前按需
        // 从滤镜图实时查询，用于把 pts 换算到编码器时间基。
        let (filter_frame_rate, filter_size) = match filter_graph.as_mut() {
            Some(graph) => (graph.output_frame_rate(), graph.output_size()),
            None => (None, None),
        };
        if media_type == MediaType::VIDEO {
            if let Some(out_fr) = filter_frame_rate {
                let changed =
                    out_fr.num != self.frame_rate.num || out_fr.den != self.frame_rate.den;
                if out_fr.num > 0 && out_fr.den > 0 && changed {
                    log::info!(
                        "Filter changes frame rate: {}/{} -> {}/{}",
                        self.frame_rate.num,
                        self.frame_rate.den,
                        out_fr.num,
                        out_fr.den
                    );
                    encode_ctx.set_framerate(out_fr);
                    encode_ctx.set_time_base(avutil::av_inv_q(out_fr));
                }
            }
            if let Some((fw, fh)) = filter_size
                && fw > 0
                && fh > 0
                && (fw != encode_ctx.width || fh != encode_ctx.height)
            {
                log::info!(
                    "Filter changes size: {}x{} -> {}x{}",
                    encode_ctx.width,
                    encode_ctx.height,
                    fw,
                    fh
                );
                encode_ctx.set_width(fw);
                encode_ctx.set_height(fh);
            }
        }

        let hw_context = self
            .hw_device_config
            .filter(|_cfg| {
                // hardware acceleration enabled for video
                media_type == MediaType::VIDEO
            })
            .map(|cfg| {
                // codec support or not for hardware acceleration
                log::info!(
                    "Video Encoder with HW acceleration codec: {:?}, config: {:#?}",
                    self.codec_name,
                    cfg
                );

                // create hardware context
                let (width, height) = (encode_ctx.width, encode_ctx.height);
                HWContext::new(cfg)
                    .and_then(|ctx| {
                        // *注意*: setup_encoder_frames 会根据 HW 能力修改 encode_ctx.pix_fmt
                        ctx.setup_encoder_frames(&mut encode_ctx, width, height)?;
                        Ok(ctx)
                    })
                    .context("Hardware acceleration context initialization failed")
            })
            .transpose()?;

        // 打开编码器前的私有选项：quality/profile/level 先写入，用户
        // codec_opts 后合并覆盖（显式指定的用户选项优先）。
        let mut opts = Options::new();
        if use_crf && let Some(Quality::Crf(crf)) = self.quality {
            opts.insert("crf", crf.to_string());
        }
        if media_type == MediaType::VIDEO {
            if let Some(profile) = self.profile {
                opts.insert("profile", profile.as_option_str());
            }
            if let Some(level) = &self.level {
                opts.insert("level", level);
            }
        }
        if let Some(user_opts) = self.codec_opts {
            // 用户显式选项覆盖 quality 默认值
            opts.merge(user_opts);
        }

        // 字幕编码器（mov_text/subrip 等）init 时会执行
        // `ff_ass_split(avctx->subtitle_header)`，未设置时返回 NULL →
        // AVERROR_INVALIDDATA。header 必须由调用方提供：转码时从解码器侧
        // 传递（解码器 open 时填充），authoring 场景用
        // [`EncoderBuilder::with_subtitle_header`] 显式给出。
        if media_type == MediaType::SUBTITLE {
            let Some(header) = &self.subtitle_header else {
                return Err(RsmediaError::custom(
                    "subtitle encoder requires an ASS script header: provide it via \
                     EncoderBuilder::with_subtitle_header, or forward it from the decoded \
                     subtitle stream in a transcode pipeline",
                ));
            };
            let header_c = std::ffi::CString::new(header.as_str())
                .map_err(|e| RsmediaError::custom(format!("Invalid subtitle header: {e}")))?;
            encode_ctx
                .set_subtitle_header(header_c.as_c_str())
                .context("Failed to set subtitle header")?;
        }

        encode_ctx
            .open(opts.into_dict())
            .context("Failed to open encode context")?;

        Ok(Encoder {
            config,
            hw_context,
            media_type,
            filter_input_format,
            filter_graph,
            context: encode_ctx,
            state: CodecContextState::Normal,
            scaler: Scaler::new_with_options(self.scale_algorithm, self.scale_quality),
            pending_packets: VecDeque::new(),
            audio_fifo: None,
            next_pts: 0,
            input_time_base,
        })
    }
}

impl Default for EncoderBuilder {
    fn default() -> Self {
        Self {
            // video
            width: 0,
            height: 0,
            pixel_format: None,
            time_base: time::TIME_BASE,
            pkt_time_base: time::TIME_BASE,
            bit_rate: Self::VIDEO_BIT_RATE,
            frame_rate: time::new_rational(Self::FRAME_RATE, 1),
            fps: Self::FRAME_RATE as f32,
            gop_size: 0,
            max_b_frames: 0,
            ofmt_flag: AVFormatFlag::GLOBAL_HEADER.as_raw(),
            // audio
            nb_channels: 2,
            sample_rate: 44100,
            sample_format: None,
            // common
            media_type: MediaType::VIDEO,
            thread_count: num_cpus::get(),
            codec_name: None,
            codec_opts: None,
            quality: None,
            profile: None,
            level: None,
            filters: None,
            subtitle_header: None,
            hw_device_config: None,
            scale_algorithm: ScaleAlgorithm::default(),
            scale_quality: ScaleQuality::default_quality().to_vec(),
        }
    }
}

/// Encodes frames into a video stream.
///
/// # Example
///
/// ```ignore
/// let decoder = Decoder::new("video_out.mkv").unwrap();
/// decoder
///     .decode_iter()
///     .take_while(Result::is_ok)
///     .map(|frame| encoder
///         .encode(frame.unwrap())
///         .expect("Failed to encode frame."),
///     );
/// ```
pub struct Encoder {
    config: CodecConfig,
    context: AVCodecContext,
    filter_graph: Option<FilterGraph>,
    /// 滤镜图输入格式声明（来自 [`Filter::input_format`]）。仅当滤镜图
    /// 存在时可能为 `Some`；输入帧进图前需转换到该格式（视频=像素格式，
    /// 音频=采样格式；默认=编码器协商格式）。
    filter_input_format: Option<FrameFormat>,
    hw_context: Option<Arc<HWContext>>,
    media_type: MediaType,
    state: CodecContextState,
    scaler: Scaler,
    /// 编码器缓冲满（send_frame 返回 EAGAIN）时，先行排空的已就绪包暂存于此， 由 `receive_packet` 优先取出，
    /// 避免丢包。按 FIFO 出队（`pop_front`）， 保证与编码器输出顺序一致（否则 dts 会乱序、mux 报错）。
    pending_packets: VecDeque<AVPacket>,
    /// 音频样本缓冲：固定帧长编码器（如 aac，frame_size=1024）要求每次 `send_frame`
    /// 恰好给出 `frame_size` 个样本，而待编码音频帧大小可能可变（滤镜输出、或用户
    /// 输入不足一帧），需先累积补齐到帧长再送编码器。
    audio_fifo: Option<AVAudioFifo>,
    /// 下一个自动分配的 pts（`input_time_base` 下的 tick 计数）。
    ///
    /// 用户未设置 pts（`AV_NOPTS_VALUE`）的帧由此计数器自动编号：视频每帧 +1
    /// （输入时间基 = `1/fps`，每帧恰一 tick），音频按样本数递增（输入时间基 =
    /// `1/sample_rate`，样本位置即时间轴）。用户设置了 pts 的帧照常使用其值，
    /// 但计数器仍跳到其后，保证后续未设置的帧能接续正确的时间轴。
    ///
    /// 固定帧长音频（aac 等）由 `audio_fifo` 重新切帧：输出帧的 pts 无法取自
    /// 任何单个输入帧，只能用"已输出累计样本数"。因此首次缓冲时以首帧 pts
    /// （若设置）播种本计数器，此后每切出一帧按 `frame_size` 递增、flush 末帧
    /// 按 `remaining` 递增——与视频/直发路径共用同一个计数器。
    next_pts: i64,
    /// 编码器**输入**时间基：滤镜存在时为滤镜图 buffer 源的时间基（= 建图时的
    /// `effective_time_base()`），否则等于编码器 time_base。滤镜改写输出帧率时
    /// 编码器 time_base 会被改为 `1/滤镜输出fps`，与输入时间基不再相等，因此
    /// 必须显式保存，供 pts 换算与自动编号使用。
    input_time_base: ffi::AVRational,
}

impl Encoder {
    /// Create a video encoder with the specified destination
    ///
    /// # Arguments
    ///
    /// * `width` - The width of the video stream.
    /// * `height` - The height of the video stream.
    ///
    /// note: default video codec is `libx264`
    #[inline]
    pub fn new_video(width: usize, height: usize) -> Result<Encoder> {
        EncoderBuilder::new_video(width, height).build()
    }

    /// Create a audio encoder with the specified parameters.
    ///
    /// * `bit_rate` - Bit rate in bits per second. default is 128k.
    /// * `nb_channels` - Number of channels.
    /// * `sample_rate` - Sample rate in Hz.
    /// * `sample_format` - Sample format.
    ///
    /// note: default audio codec is `aac`
    #[inline]
    pub fn new_audio(
        nb_channels: i32,
        sample_rate: i32,
        sample_format: SampleFormat,
    ) -> Result<Encoder> {
        EncoderBuilder::new_audio(128_000, nb_channels, sample_rate, sample_format).build()
    }

    /// Returns `true` if the encoder is in the "drained" state.
    ///
    /// This means all input has been processed, but not fully flushed.
    pub fn is_drained(&self) -> bool {
        self.state == CodecContextState::Drained
    }

    /// Returns `true` if the encoder is fully flushed and finished.
    pub fn is_flushed(&self) -> bool {
        self.state == CodecContextState::Flushed
    }

    /// Encode a high-level frame (a single frame ndarray-based)
    ///
    /// # Arguments
    ///
    /// * `frame` - Frame to encode in `HWC` format and standard layout.
    #[cfg(feature = "ndarray")]
    pub fn encode<T>(&mut self, frame: MediaFrame<T>) -> Result<Vec<AVPacket>>
    where
        T: MediaFrameType,
    {
        let raw_frame = frame.to_avframe()?;
        self.encode_raw(raw_frame)
    }

    /// Encode a single raw frame.
    ///
    /// # Arguments
    ///
    /// * `frame` - Frame to encode.
    ///
    /// # Returns
    ///
    /// 所有已就绪的编码包。一次输入帧可能（在编码器缓冲满、滤镜升帧率等场景下）
    /// 产出 0 或多包，因此返回集合而非单个包。
    pub fn encode_raw(&mut self, frame: AVFrame) -> Result<Vec<AVPacket>> {
        // send frame
        self.send_frame_to_encoder(Some(frame))?;

        // receive packet: 排空所有已就绪的包（含 EAGAIN 时暂存的 pending_packets）
        let mut packets = Vec::new();
        while let Some(pkt) = self.receive_packet()? {
            packets.push(pkt);
        }
        Ok(packets)
    }

    /// ASS 时间格式 `H:MM:SS.cc`（厘秒精度）。
    /// 编码一条字幕段落（仅字幕编码器）。
    ///
    /// 字幕编码走 rsmpeg 的 [`AVCodecContext::encode_subtitle`]（同步 API，无
    /// send_frame/receive_packet 队列），与音视频的 [`Self::encode_raw`] 不同：
    /// 每条段落恰好产出 0 或 1 包，无需 flush。pts/duration 已按编码器
    /// time_base（1/1000 毫秒精度）设置。
    pub fn encode_subtitle_segment(&mut self, segment: &SubtitleSegment) -> Result<Vec<AVPacket>> {
        if self.media_type != MediaType::SUBTITLE {
            return Err(RsmediaError::custom(format!(
                "encode_subtitle_segment requires a subtitle encoder, got media type: {:?}",
                self.media_type
            )));
        }
        if segment.text.is_empty() {
            return Ok(Vec::new());
        }

        // Build a single ASS text rect. `AVCodecContext::encode_subtitle`
        // hands `rect.ass` to the codec's `ff_ass_split_dialog`, whose
        // hardcoded field list is `ReadOrder, Layer, Style, Name, MarginL,
        // MarginR, MarginV, Effect, Text` — **no start/end timestamps**: the
        // timings are carried by the packet pts/duration set below. A full
        // `Dialogue:` line would shift every field by two (the timestamps get
        // eaten by Layer/Style) and surface as a stray comma prepended to the
        // decoded text plus a spurious zero-style record.
        let mut subtitle = AVSubtitle::new();
        let dialogue = format!("0,0,Default,,0,0,0,,{}", segment.text);
        let dialogue_c = std::ffi::CString::new(dialogue)
            .map_err(|e| RsmediaError::custom(format!("Subtitle text contains NUL byte: {e}")))?;
        subtitle
            .push_ass_rect(dialogue_c.as_c_str())
            .context("Failed to build subtitle rect")?;

        let mut buf = vec![0u8; EncoderBuilder::SUBTITLE_BUFFER_SIZE];
        let len = self
            .context
            .encode_subtitle(&subtitle, &mut buf)
            .context("Subtitle encoding failed")?;
        if len == 0 {
            return Ok(Vec::new());
        }

        let mut packet = AVPacket::new();
        let ret = unsafe { ffi::av_new_packet(packet.as_mut_ptr(), len as i32) };
        if ret < 0 {
            return Err(RsmediaError::from(rsmpeg::error::RsmpegError::from(ret)));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(buf.as_ptr(), (*packet.as_mut_ptr()).data, len);
        }

        // 编码器 time_base 为 1/1000，pts/duration 直接使用毫秒值；
        // dts = pts（字幕无 B 帧重排）。
        let pts = segment.start_ms;
        packet.set_pts(pts);
        packet.set_dts(pts);
        packet.set_duration(segment.duration_ms().max(1));
        packet.set_pos(-1);

        Ok(vec![packet])
    }

    fn send_frame_to_encoder(&mut self, frame_opt: Option<AVFrame>) -> Result<()> {
        if let Some(mut frame) = frame_opt {
            // pts 处理必须在进滤镜/格式转换之前：滤镜（`fps`/`framerate` 等）需要
            // 有效 pts 才能正确工作，时间基换算也要以滤镜图输入时间基为基准。
            self.assign_pts(&mut frame);
            // 正常编码帧：经过 filter（如有）
            // 滤镜 buffer/abuffer 源按"滤镜图输入格式"配置（声明优先，见
            // `Filter::with_input_format`；默认=编码器协商格式）。输入帧格式
            // 与图输入格式不一致时需先转换再进图：
            // - 视频：像素格式转换（swscale）。直接送入不匹配的 buffer 源会
            //   触发 FFmpeg 自动格式转换路径的越界读写（SIGSEGV）。此转换与
            //   无滤镜时 `send_frame_post_filter` 里的 `rescale` 行为一致。
            // - 音频：采样格式转换（swresample，速率/声道不变）。abuffer 拒绝
            //   属性不符的帧（"Changing frame properties on the fly"），图内
            //   格式变化由滤镜链（如 aformat）完成，滤镜后 `rescale` 兜底。
            let graph_input_format = self.filter_input_format.unwrap_or_else(|| {
                // 无声明时图输入=编码器协商格式
                match self.media_type {
                    MediaType::VIDEO => FrameFormat::Pixel(self.pix_fmt()),
                    _ => FrameFormat::Sample(self.sample_fmt()),
                }
            });
            // 先把帧转换到滤镜图输入格式（此转换需要 `&mut self` 以复用可变的
            // `scaler`/重采样上下文），转换完成后再借用 `filter_graph` 处理。
            let converted = match graph_input_format {
                FrameFormat::Pixel(dst) if frame.format != dst as i32 => {
                    self.rescale_frame(&frame, dst)?
                }
                FrameFormat::Sample(dst) if frame.format != dst as i32 => {
                    resample::convert_frame(&frame, frame.ch_layout, dst as _, frame.sample_rate)?
                }
                _ => frame,
            };
            if let Some(graph) = self.filter_graph.as_mut() {
                match graph.process_frame(Some(converted))? {
                    Some(filtered) => self.send_frame_post_filter(filtered)?,
                    None => {
                        // filter 暂未输出（内部缓冲中），等待后续帧驱动
                        log::debug!("Filter graph drained, waiting for more input.");
                    }
                }
            } else {
                self.send_frame_post_filter(converted)?;
            }
            Ok(())
        } else {
            // EOF：向编码器发送 EOS。filter 的缓冲帧已由 `flush()` 单独冲刷送走，
            // 这里不应再调用 `process_frame(None)`，否则对已 flushed 的 graph 会报错。
            // 编码器缓冲可能仍满（EAGAIN），需先排空已就绪包再重试发送 EOS。
            loop {
                match self.context.send_frame(None) {
                    Ok(()) => break,
                    Err(rsmpeg::error::RsmpegError::SendFrameAgainError) => {
                        self.drain_encoder_packets()?;
                    }
                    Err(e) => return Err(RsmediaError::from(e)),
                }
            }
            Ok(())
        }
    }

    /// pts 的自动处理与时间基归一化（在帧进滤镜/编码器之前调用）。
    ///
    /// FFmpeg 在编码器输入侧**忽略** `AVFrame.time_base`，pts 一律按
    /// `AVCodecContext.time_base` 解释。因此两件事必须在这里完成：
    ///
    /// 1. **时间基换算**：帧携带了有效且不同的 `time_base` 时（解码侧容器流时间基，
    ///    如 mp4 的 `1/15360`），把 pts 换算到编码器输入时间基，否则时间轴被静默
    ///    误读（时长/帧率全错）。
    /// 2. **自动编号**：pts 为 `AV_NOPTS_VALUE`（用户未设置）时，用运行计数器
    ///    [`next_pts`](Self::next_pts) 编号——视频每帧 +1 tick（输入时间基 =
    ///    `1/fps`），音频按样本位置递增。固定帧长音频（aac 等）除外：其帧切分由
    ///    `audio_fifo` 完成，本方法不为输入帧编号（NOPTS 原样通过），而由
    ///    `buffer_audio_frame` 首次缓冲时用首帧 pts 播种 `next_pts`，此后每个
    ///    输出帧按已输出样本数递增。
    ///
    /// 计数器在用户设置了 pts 的帧上同样前进（跳到该 pts 之后），使后续未设置
    /// pts 的帧能接续正确的时间轴。
    fn assign_pts(&mut self, frame: &mut AVFrame) {
        let input_tb = self.input_time_base;
        if frame.pts != ffi::AV_NOPTS_VALUE
            && frame.time_base.num > 0
            && frame.time_base.den > 0
            && (frame.time_base.num != input_tb.num || frame.time_base.den != input_tb.den)
        {
            frame.set_pts(frame.pts.rescale(frame.time_base, input_tb));
        }
        frame.set_time_base(input_tb);

        // 固定帧长音频走 audio_fifo，输出帧 pts 由 `next_pts` 按已输出样本数
        // 递增（见 buffer_audio_frame / drain_audio_fifo），这里不编号。
        if self.media_type == MediaType::AUDIO && self.frame_size() > 0 {
            return;
        }
        if frame.pts == ffi::AV_NOPTS_VALUE {
            frame.set_pts(self.next_pts);
        }
        let step = if self.media_type == MediaType::AUDIO {
            // 输入时间基 = 1/sample_rate，样本位置即时间轴。
            frame.nb_samples.max(1) as i64
        } else {
            // 输入时间基 = 1/fps，每帧恰一 tick。
            1
        };
        self.next_pts = frame.pts + step;
    }

    /// 将已通过 filter（或无 filter）的帧做 rescale/hw 上传后发送给编码器。
    ///
    /// 注意：`flush()` 阶段 filter 已进入 Flushed 状态，不能再把缓冲帧送回
    /// `process_frame`（会因 EAGAIN 被丢弃），因此缓冲帧必须直接走本方法。
    fn send_frame_post_filter(&mut self, frame: AVFrame) -> Result<()> {
        // 滤镜输出帧的 pts 位于滤镜输出时间基（如 `framerate` 输出 1/120），而编码器
        // 时间基已按滤镜输出帧率对齐（如 1/30）。发送前需把 pts 换算到编码器时间基，
        // 否则 B 帧重排得到的 dts 会乱序、mux 报 AVERROR(-22)。
        // 时间基按需从滤镜图实时查询，避免缓存冗余状态。
        let mut frame = frame;
        if let Some(filter_tb) = self
            .filter_graph
            .as_mut()
            .and_then(|g| g.output_time_base())
        {
            let enc_tb = self.context.time_base;
            if frame.pts != ffi::AV_NOPTS_VALUE {
                frame.set_pts(frame.pts.rescale(filter_tb, enc_tb));
                frame.set_time_base(enc_tb);
            }
        }

        // 确保帧的格式匹配编码器要求
        let scaled_frame = self.rescale(frame)?;

        // 转换硬件帧
        let hw_frame = match self.hw_context.as_ref() {
            Some(hw_ctx) if hw_ctx.is_sw_frame(&scaled_frame) => {
                // sw_frame -> hw_frame
                hw_ctx
                    .hw_upload(&mut self.context, &scaled_frame)
                    .context("Failed to upload frame to HW")?
            }
            _ => scaled_frame, // 不需要上传或已经是 HW frame
        };

        // 固定帧长音频编码器（如 aac，frame_size=1024）要求每次 `send_frame` 恰好给出
        // frame_size 个样本，而待编码帧大小可能可变（滤镜输出、或用户输入不足一帧），
        // 需先进 `audio_fifo` 累积补齐后再送编码器；无固定帧长（frame_size=0）的
        // 编码器（如部分无损格式）直接发送。
        if self.media_type == MediaType::AUDIO && self.frame_size() > 0 {
            self.buffer_audio_frame(hw_frame)
        } else {
            self.check_frame(Some(&hw_frame))?;

            log::debug!(
                "Send frame to encoder: {:?}, time_base: {:?}, media_type: {:?}",
                hw_frame,
                self.time_base(),
                self.media_type()
            );

            self.send_ready_frame(hw_frame)
        }
    }

    /// 将一帧已 rescale 的音频帧写入 `audio_fifo`，凑满 `frame_size` 后送出。
    ///
    /// 固定帧长编码器必须在每次 `send_frame` 时恰好给出 `frame_size` 个样本，
    /// 因此先把待编码帧写入 `audio_fifo` 累积，凑满 `frame_size` 再送编码器；
    /// 不足 `frame_size` 的剩余样本，由 `flush` 阶段作为末帧截取。
    fn buffer_audio_frame(&mut self, frame: AVFrame) -> Result<()> {
        let frame_size = self.frame_size();
        if self.audio_fifo.is_none() {
            let channels = self.ch_layout().nb_channels;
            let sample_fmt = self.sample_fmt() as _;
            // 首次缓冲时以首帧 pts（编码器时间基下的样本位置，`assign_pts` 已
            // 完成换算/自动编号的对齐）播种样本计数器；未设置则保持 0 起步。
            if frame.pts != ffi::AV_NOPTS_VALUE {
                self.next_pts = frame.pts;
            }
            self.audio_fifo = Some(AVAudioFifo::new(sample_fmt, channels, frame_size));
        }
        unsafe {
            self.audio_fifo
                .as_mut()
                .unwrap()
                .write(frame.data.as_ptr(), frame.nb_samples)?;
        }
        self.drain_audio_fifo(frame_size)
    }

    /// 从 `audio_fifo` 中取出满帧长样本，拼成帧送编码器，直至剩余不足一帧。
    fn drain_audio_fifo(&mut self, frame_size: i32) -> Result<()> {
        while self.audio_fifo.as_ref().unwrap().size() >= frame_size {
            let mut frame = AVFrame::new();
            frame.set_nb_samples(frame_size);
            frame.set_ch_layout(self.ch_layout().clone().into_inner());
            frame.set_format(self.sample_fmt() as _);
            frame.set_sample_rate(self.sample_rate());
            frame.set_time_base(self.time_base());
            unsafe {
                frame
                    .alloc_buffer()
                    .context("Failed to allocate audio frame buffer")?;
                self.audio_fifo
                    .as_mut()
                    .unwrap()
                    .read(frame.data.as_ptr(), frame_size)?;
            }
            frame.set_pts(self.next_pts);
            self.next_pts += frame_size as i64;
            self.check_frame(Some(&frame))?;
            self.send_ready_frame(frame)?;
        }
        Ok(())
    }

    /// 冲刷音频缓冲中不足一帧的剩余样本，作为末帧送编码器。
    fn flush_audio_fifo(&mut self) -> Result<()> {
        let sample_fmt = self.sample_fmt() as _;
        let ch_layout = self.ch_layout().clone().into_inner();
        let sample_rate = self.sample_rate();
        let time_base = self.time_base();
        let Some(fifo) = self.audio_fifo.as_mut() else {
            return Ok(());
        };
        let remaining = fifo.size();
        if remaining <= 0 {
            return Ok(());
        }
        let mut frame = AVFrame::new();
        frame.set_nb_samples(remaining);
        frame.set_ch_layout(ch_layout);
        frame.set_format(sample_fmt);
        frame.set_sample_rate(sample_rate);
        frame.set_time_base(time_base);
        unsafe {
            frame
                .alloc_buffer()
                .context("Failed to allocate audio frame buffer")?;
            fifo.read(frame.data.as_ptr(), remaining)?;
        }
        frame.set_pts(self.next_pts);
        self.next_pts += remaining as i64;
        self.check_frame(Some(&frame))?;
        self.send_ready_frame(frame)
    }

    /// 向编码器发送一帧已就绪（rescale/校验完成）的帧；若缓冲已满（EAGAIN），
    /// 先排空已就绪包，再重试发送。
    fn send_ready_frame(&mut self, frame: AVFrame) -> Result<()> {
        loop {
            match self.context.send_frame(Some(&frame)) {
                Ok(()) => break,
                Err(rsmpeg::error::RsmpegError::SendFrameAgainError) => {
                    self.drain_encoder_packets()?;
                }
                Err(e) => return Err(RsmediaError::from(e)),
            }
        }
        Ok(())
    }

    /// 编码器缓冲已满（send_frame 返回 EAGAIN）时，先排空已就绪包到 `pending_packets`，
    /// 供 `receive_packet` 优先返回，避免丢包，随后由调用方重试发送。
    fn drain_encoder_packets(&mut self) -> Result<()> {
        loop {
            match self.context.receive_packet() {
                Ok(pkt) => self.pending_packets.push_back(pkt),
                Err(rsmpeg::error::RsmpegError::EncoderDrainError) => break,
                Err(rsmpeg::error::RsmpegError::EncoderFlushedError) => break,
                Err(e) => return Err(RsmediaError::from(e)),
            }
        }
        Ok(())
    }

    fn rescale(&mut self, frame: AVFrame) -> Result<AVFrame> {
        let scaled_frame = match self.media_type {
            MediaType::VIDEO => {
                let target_sw_pix_fmt = if let Some(hw_ctx) = self.hw_context.as_ref() {
                    hw_ctx.get_format(false).into()
                } else {
                    self.pix_fmt()
                };
                if frame.format != i32::from(target_sw_pix_fmt) {
                    self.rescale_frame(&frame, target_sw_pix_fmt)?
                } else {
                    frame
                }
            }
            MediaType::AUDIO => {
                let ch_layout = self.ch_layout();
                if frame.sample_rate != self.sample_rate()
                    || frame.format != self.sample_fmt() as i32
                    || frame.ch_layout.nb_channels != ch_layout.nb_channels
                {
                    resample::convert_frame(
                        &frame,
                        ch_layout.clone().into_inner(),
                        self.sample_fmt() as _,
                        self.sample_rate(),
                    )?
                } else {
                    frame
                }
            }
            _ => {
                // do nothing
                return Err(RsmediaError::custom(format!(
                    "Unsupported encode frame media type: {:?}",
                    self.media_type
                )));
            }
        };
        Ok(scaled_frame)
    }

    /// Scale a video frame to the encoder's target pixel format, through the
    /// encoder's persistent [`Scaler`].
    ///
    /// The destination keeps the source geometry (size changes are the filter
    /// graph's job, see [`Filter`]); the scaler rebuilds its context by itself
    /// when the geometry or format changes mid-stream.
    fn rescale_frame(&mut self, frame: &AVFrame, dst_fmt: PixelFormat) -> Result<AVFrame> {
        self.scaler
            .scale_frame(frame, frame.width, frame.height, dst_fmt)
    }

    /// Check if the frame is valid for encoding.
    fn check_frame(&self, frame: Option<&AVFrame>) -> Result<()> {
        let Some(frame) = frame else {
            return Ok(());
        };
        match self.media_type {
            MediaType::VIDEO => {
                // 硬件帧的像素格式（如 NV12/HW 私有格式）不在软件编码器的
                // `supported_pixel_formats()` 列表中，跳过该检查以免误报。
                if !frame.hw_frames_ctx.is_null() {
                    return Ok(());
                }
                if !self.config.is_support_pixel_format(frame.format) {
                    return Err(RsmediaError::custom(format!(
                        "Unsupported video encoder frame pixel format: {:?}",
                        frame.format
                    )));
                }
            }

            MediaType::AUDIO => {
                if !self.config.is_support_sample_format(frame.format) {
                    return Err(RsmediaError::custom(format!(
                        "Unsupported encode audio frame sample format: {:?}",
                        frame.format
                    )));
                }

                if !self.config.is_support_frame_rates(self.context.framerate) {
                    return Err(RsmediaError::custom(format!(
                        "Unsupported encode audio frame rate: {:?}",
                        self.context.framerate
                    )));
                }

                if !self.config.is_support_sample_rate(frame.sample_rate) {
                    return Err(RsmediaError::custom(format!(
                        "Unsupported encode audio frame sample rate: {:?}",
                        frame.sample_rate
                    )));
                }

                // 注意：不在此校验 `nb_samples == frame_size`。固定帧长音频编码器
                // 已由 `audio_fifo` 缓冲切帧（切出帧恒为 frame_size，flushed 末帧
                // 允许不足一帧），此处切出的帧长短不由待编码帧决定；且末帧不足一帧
                // 是编码器合法接受的，故帧长正确性由缓冲路径保证，不在此拦截。
            }
            _ => {}
        }
        Ok(())
    }

    /// Get encoder time base.
    #[inline]
    pub fn time_base(&self) -> ffi::AVRational {
        self.context.time_base
    }

    #[inline]
    pub fn frame_rate(&self) -> ffi::AVRational {
        self.context.framerate
    }

    #[inline]
    pub fn width(&self) -> i32 {
        self.context.width
    }

    #[inline]
    pub fn height(&self) -> i32 {
        self.context.height
    }

    #[inline]
    pub fn pix_fmt(&self) -> PixelFormat {
        self.context.pix_fmt.into()
    }

    /// Each submitted frame except the last must contain exactly frame_size samples per channel.
    /// May be 0 when the codec has AV_CODEC_CAP_VARIABLE_FRAME_SIZE set, then the frame size is not restricted.
    #[inline]
    pub fn frame_size(&self) -> i32 {
        self.context.frame_size
    }

    /// audio samples per second
    #[inline]
    pub fn sample_rate(&self) -> i32 {
        self.context.sample_rate
    }

    /// audio sample format
    #[inline]
    pub fn sample_fmt(&self) -> SampleFormat {
        SampleFormat::from(self.context.sample_fmt)
    }

    #[inline]
    pub fn ch_layout(&self) -> AVChannelLayoutRef<'_> {
        self.context.ch_layout()
    }

    #[inline]
    pub fn media_type(&self) -> MediaType {
        self.media_type
    }

    #[inline]
    pub fn codecpar(&self) -> AVCodecParameters {
        self.context.extract_codecpar()
    }

    /// 单帧时长（编码器 time_base 单位），用于补全缺失的 packet duration。
    ///
    /// 先求单帧时长（秒），再换算到编码器 time_base 的整数 tick：
    /// `ticks = av_rescale_q(1, frame_dur_sec, time_base)`。
    pub(crate) fn packet_duration(&self) -> i64 {
        let tb = self.time_base();
        let frame_dur_sec = match self.media_type {
            // 视频：1 / frame_rate
            MediaType::VIDEO => avutil::av_inv_q(self.frame_rate()),
            // 音频：frame_size / sample_rate
            MediaType::AUDIO => {
                let fs = self.frame_size();
                if fs <= 0 {
                    return 0;
                }
                time::new_rational(fs, self.sample_rate())
            }
            _ => return 0,
        };
        avutil::av_rescale_q(1, frame_dur_sec, tb).max(1)
    }

    /// Internal: Pull an encoded packet from the decoder.
    ///
    /// Handles `EAGAIN`, drained, and flushed states.
    ///
    /// # Returns
    ///
    /// `Some(packet)` if a packet is returned, `None` if waiting or end.
    fn receive_packet(&mut self) -> Result<Option<AVPacket>> {
        // 优先返回暂存区（send_frame EAGAIN 排空时存入）的包。
        // 按 FIFO 顺序出队（`pop_front`），保证与编码器输出顺序一致，
        // 避免 dts 乱序、mux 报错。
        if let Some(pkt) = self.pending_packets.pop_front() {
            return Ok(Some(pkt));
        }
        match self.context.receive_packet() {
            Ok(pkt) => Ok(Some(pkt)),
            Err(rsmpeg::error::RsmpegError::EncoderDrainError) => {
                log::debug!("Encoder drained, try send new frame again.");
                self.state = CodecContextState::Drained;
                Ok(None)
            }
            Err(rsmpeg::error::RsmpegError::EncoderFlushedError) => {
                log::debug!("Encoder flushed, EOF reached.");
                self.state = CodecContextState::Flushed;
                Ok(None)
            }
            Err(err) => Err(RsmediaError::from(err)),
        }
    }

    /// Flush the encoder and write any remaining packets.
    ///
    /// This function sends an end-of-stream signal to the encoder, and continues
    /// to pull packets until the encoder is fully flushed.
    ///
    /// # Arguments
    ///
    /// * `writer` - Writer to write encoded packets.
    /// * `interleaved` - Whether to write packets in interleaved mode (typical for most formats).
    /// * `index` - Stream index for the output stream.
    /// * `out_stream_time_base` - Time base of the output stream.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if flushing completes successfully.
    /// May return an error if writing fails or encoder returns an error.
    pub fn flush<W: Writer>(
        &mut self,
        writer: &mut W,
        interleaved: bool,
        index: usize,
        out_stream_time_base: ffi::AVRational,
    ) -> Result<()> {
        // 字幕编码器走同步 API（avcodec_encode_subtitle），无内部缓冲，
        // 不支持 send/receive flush（send_frame(None) 会崩溃），直接返回。
        if self.media_type == MediaType::SUBTITLE {
            self.state = CodecContextState::Flushed;
            return Ok(());
        }

        if let Some(filter) = self.filter_graph.as_mut() {
            let frames = filter.flush()?;
            for frame in frames {
                // filter 已 Flushed，缓冲帧直接走 post-filter 路径，不可再进 process_frame
                self.send_frame_post_filter(frame)?;
            }
        }

        // 冲刷音频缓冲中不足一帧的剩余样本（作为末帧送编码器）
        self.flush_audio_fifo()?;

        // EOF: Notify the encoder that the last frame has been sent.
        self.send_frame_to_encoder(None)?;

        // drain the items still on the queue before giving up.
        // EOF 已发送，理论上编码器最终会返回 EOF；但为防御个别编码器在 EOS 后
        // 持续返回 EAGAIN（Drained）而不返回 EOF，增加迭代上限，避免死循环。
        let mut drained_iterations = 0u32;
        loop {
            match self.receive_packet() {
                Ok(Some(mut packet)) => {
                    drained_iterations = 0;
                    packet.set_pos(-1);
                    packet.set_stream_index(index as i32);
                    // 编码器输出的 packet 常不带 duration（libx264 等），若缺失则按
                    // 帧率/采样率补上，否则 MP4 等容器无法推导**最后一帧**的时长，
                    // 导致末帧被 muxer 丢弃。
                    if packet.duration <= 0 {
                        packet.set_duration(self.packet_duration());
                    }
                    // 将编码器输出的数据包时间戳，从编码器时间基转换到输出流时间基
                    // encode_ctx_timebase => out_stream_time_base
                    packet.rescale_ts(self.time_base(), out_stream_time_base);
                    if interleaved {
                        writer.write_interleaved(&mut packet)?;
                    } else {
                        writer.write_frame(&mut packet)?;
                    }
                }
                Ok(None) => {
                    if self.is_drained() {
                        log::debug!("Encoder drained, try send new frame again.");
                        drained_iterations += 1;
                        if drained_iterations > 1_000 {
                            log::error!(
                                "Encoder keeps returning EAGAIN after EOF, aborting flush."
                            );
                            break;
                        }
                        continue;
                    } else {
                        log::debug!("Encoder flushed, EOF reached.");
                        break;
                    }
                }
                Err(e) => {
                    log::debug!("Encode packet error: {e}");
                    break;
                }
            }
        }

        Ok(())
    }
}

impl Drop for Encoder {
    /// Automatically called when the `Encoder` is dropped.
    ///
    /// **Warning**: This does NOT automatically flush the encoder.
    /// The user is responsible for calling [`Encoder::flush`] manually
    /// before dropping the encoder to ensure all frames are written.
    fn drop(&mut self) {
        //! let _ = self.flush();
        if !self.is_flushed() {
            log::error!("Encoder dropped without flushing, data may be lost.");
        }
    }
}

/// SAFETY:
/// - Encoder contains `AVCodecContext`, which is not inherently thread-safe.
///   Sharing `&Encoder` across threads (`Sync`) cannot be guaranteed, so only
///   `Send` is implemented: moving an Encoder to another thread for exclusive
///   use is safe, as all resources move with the object.
unsafe impl Send for Encoder {}

#[cfg(test)]
mod tests {
    use super::*;

    // ====================================================================
    // 核心方法单元测试
    //
    // 只覆盖编码器自身的决策逻辑 —— 格式协商、时间基/码率推导、pts 自动编号、
    // 帧校验、builder 选项落点 —— 不落盘、不做编解码往返，因此不需要 ndarray，
    // 也不依赖任何测试媒体文件。
    //
    // 需要真实文件的端到端功能测试（容器矩阵 / 编解码往返 / 滤镜 / 转码 /
    // 音频切帧）见 `tests/encode_pipeline.rs`。
    // ====================================================================

    /// 未显式指定像素格式时按编码器能力协商；显式指定且编码器不支持时
    /// 立即报错（fail fast），而不是把帧转进去后在写入阶段才失败。
    #[test]
    fn test_resolve_pixel_format_negotiation() -> Result<()> {
        let codec = strutils::str_to_cstring("libx264");
        let config = CodecConfig::new_with_name(&codec)?;

        // 未指定 → 协商为 YUV420P（兼容性最好的默认值）。
        let builder = EncoderBuilder::new_video(64, 64);
        assert_eq!(
            builder.resolve_pixel_format(&config, "libx264")?,
            PixelFormat::YUV420P
        );

        // 显式且受支持 → 原样采用。
        let builder = EncoderBuilder::new_video(64, 64).with_pixel_format(PixelFormat::YUV420P);
        assert_eq!(
            builder.resolve_pixel_format(&config, "libx264")?,
            PixelFormat::YUV420P
        );

        // 显式但不受支持 → InvalidConfig。
        let builder = EncoderBuilder::new_video(64, 64).with_pixel_format(PixelFormat::RGB24);
        let err = builder
            .resolve_pixel_format(&config, "libx264")
            .expect_err("libx264 does not accept RGB24");
        assert!(
            matches!(err, RsmediaError::InvalidConfig(_)),
            "expected InvalidConfig, got {err:?}"
        );
        Ok(())
    }

    /// 采样格式协商：未指定时优先 FLTP，编码器不支持 FLTP 时取支持列表首个；
    /// 显式指定且不受支持时立即报错。
    #[test]
    fn test_resolve_sample_format_negotiation() -> Result<()> {
        // pcm_s16le 只接受 S16，而默认优先级是 FLTP → 落到列表首个 S16。
        let codec = strutils::str_to_cstring("pcm_s16le");
        let config = CodecConfig::new_with_name(&codec)?;
        let builder = EncoderBuilder::default()
            .with_media_type(MediaType::AUDIO)
            .with_codec_name("pcm_s16le".to_string())
            .with_nb_channels(2)
            .with_sample_rate(44_100);
        assert_eq!(
            builder.resolve_sample_format(&config, "pcm_s16le")?,
            SampleFormat::S16
        );

        // 显式指定不支持的格式 → InvalidConfig。
        let builder = builder.with_sample_format(SampleFormat::FLTP);
        let err = builder
            .resolve_sample_format(&config, "pcm_s16le")
            .expect_err("pcm_s16le does not accept FLTP");
        assert!(
            matches!(err, RsmediaError::InvalidConfig(_)),
            "expected InvalidConfig, got {err:?}"
        );

        // aac 原生平面浮点 → 未指定时协商为 FLTP。
        let codec = strutils::str_to_cstring("aac");
        let config = CodecConfig::new_with_name(&codec)?;
        let builder = EncoderBuilder::default()
            .with_media_type(MediaType::AUDIO)
            .with_codec_name("aac".to_string())
            .with_nb_channels(2)
            .with_sample_rate(44_100);
        assert_eq!(
            builder.resolve_sample_format(&config, "aac")?,
            SampleFormat::FLTP
        );
        Ok(())
    }

    /// 编码器输入时间基的推导：视频 `1/fps`、音频 `1/sample_rate`、
    /// 字幕 `1/1000`（毫秒精度，与 ffmpeg CLI 一致）。
    #[test]
    fn test_effective_time_base_per_media_type() {
        let video = EncoderBuilder::new_video(64, 64).with_fps(25.0);
        let tb = video.effective_time_base();
        assert_eq!((tb.num, tb.den), (1, 25), "video input time base = 1/fps");

        let audio = EncoderBuilder::new_audio(128_000, 2, 44_100, SampleFormat::FLTP);
        let tb = audio.effective_time_base();
        assert_eq!(
            (tb.num, tb.den),
            (1, 44_100),
            "audio input time base = 1/sample_rate"
        );

        let subtitle = EncoderBuilder::new_subtitle();
        let tb = subtitle.effective_time_base();
        assert_eq!(
            (tb.num, tb.den),
            (1, EncoderBuilder::SUBTITLE_TIME_BASE_DEN),
            "subtitle input time base = 1/1000"
        );
    }

    /// `Quality::Bitrate` 覆盖 `with_bit_rate`，其他 `Quality` 不参与码率推导。
    #[test]
    fn test_effective_bit_rate_precedence() {
        fn bitrate_builder() -> EncoderBuilder {
            EncoderBuilder::new_video(64, 64).with_bit_rate(500_000)
        }

        assert_eq!(bitrate_builder().effective_bit_rate(), 500_000);
        assert_eq!(
            bitrate_builder()
                .with_quality(Quality::Bitrate(2_000_000))
                .effective_bit_rate(),
            2_000_000,
            "Quality::Bitrate overrides with_bit_rate"
        );
        assert_eq!(
            bitrate_builder()
                .with_quality(Quality::Crf(20))
                .effective_bit_rate(),
            500_000,
            "Crf is a video quality knob and must not touch the bit rate"
        );
        assert_eq!(
            bitrate_builder()
                .with_quality(Quality::Bitrate(0))
                .effective_bit_rate(),
            500_000,
            "a non-positive Quality::Bitrate counts as unset"
        );
        assert_eq!(
            EncoderBuilder::new_video(64, 64).effective_bit_rate(),
            EncoderBuilder::VIDEO_BIT_RATE,
            "default bit rate"
        );
    }

    /// `check_frame` 按媒体类型校验帧格式：`None`（flush 空帧）直接放行，
    /// 编码器不支持的像素/采样格式被拒。
    #[test]
    fn test_check_frame_validates_frame_formats() -> Result<()> {
        let video = EncoderBuilder::new_video(64, 64).build()?;
        video.check_frame(None)?;

        let mut frame = AVFrame::new();
        frame.set_width(64);
        frame.set_height(64);
        frame.set_format(PixelFormat::YUV420P as i32);
        video.check_frame(Some(&frame))?;

        frame.set_format(PixelFormat::RGB24 as i32);
        assert!(
            video.check_frame(Some(&frame)).is_err(),
            "libx264 does not accept RGB24 frames"
        );

        let audio = EncoderBuilder::new_audio(128_000, 2, 44_100, SampleFormat::FLTP).build()?;
        audio.check_frame(None)?;

        let mut frame = AVFrame::new();
        frame.set_format(SampleFormat::FLTP as i32);
        frame.set_sample_rate(44_100);
        audio.check_frame(Some(&frame))?;

        frame.set_format(SampleFormat::U8 as i32);
        assert!(
            audio.check_frame(Some(&frame)).is_err(),
            "aac does not accept U8 frames"
        );
        Ok(())
    }

    /// 视频自动 pts：未设置 pts 的帧按每帧 1 tick 编号，用户设置的 pts 原样
    /// 使用并把计数器跳到其后，后续未设置的帧接续编号。
    #[test]
    fn test_assign_pts_auto_numbering_video() -> Result<()> {
        let mut encoder = EncoderBuilder::new_video(64, 64).with_fps(25.0).build()?;
        assert_eq!(encoder.next_pts, 0);

        let mut first = AVFrame::new();
        encoder.assign_pts(&mut first);
        assert_eq!(first.pts, 0, "first auto pts");
        assert_eq!(encoder.next_pts, 1);

        let mut second = AVFrame::new();
        encoder.assign_pts(&mut second);
        assert_eq!(second.pts, 1, "second auto pts");
        assert_eq!(encoder.next_pts, 2);

        // 输入时间基被统一为编码器的 1/fps。
        assert_eq!((second.time_base.num, second.time_base.den), (1, 25));

        // 用户显式 pts：原样使用，计数器跳到该 pts 之后。
        let mut explicit = AVFrame::new();
        explicit.set_pts(100);
        encoder.assign_pts(&mut explicit);
        assert_eq!(explicit.pts, 100);
        assert_eq!(encoder.next_pts, 101);

        let mut resumed = AVFrame::new();
        encoder.assign_pts(&mut resumed);
        assert_eq!(
            resumed.pts, 101,
            "auto numbering resumes after the explicit pts"
        );
        Ok(())
    }

    /// 固定帧长音频（aac，frame_size = 1024）的输出 pts 由 `audio_fifo` 切帧时
    /// 按已输出样本数维护，`assign_pts` 不参与编号。
    #[test]
    fn test_assign_pts_defers_to_audio_fifo_for_fixed_frame_size() -> Result<()> {
        let mut encoder =
            EncoderBuilder::new_audio(128_000, 2, 44_100, SampleFormat::FLTP).build()?;
        assert!(encoder.frame_size() > 0, "aac has a fixed frame size");

        let mut frame = AVFrame::new();
        encoder.assign_pts(&mut frame);
        assert_eq!(
            frame.pts,
            ffi::AV_NOPTS_VALUE,
            "fixed-frame-size audio is numbered by the fifo, not here"
        );
        Ok(())
    }

    /// 只读访问器反映 builder 的配置。
    #[test]
    fn test_encoder_accessors_reflect_builder() -> Result<()> {
        let encoder = EncoderBuilder::new_video(320, 240)
            .with_fps(30.0)
            .with_pixel_format(PixelFormat::YUV420P)
            .build()?;

        assert_eq!(encoder.width(), 320);
        assert_eq!(encoder.height(), 240);
        assert_eq!(encoder.pix_fmt(), PixelFormat::YUV420P);
        assert_eq!(encoder.media_type(), MediaType::VIDEO);
        assert_eq!(encoder.frame_size(), 0, "video codecs have no frame size");
        let tb = encoder.time_base();
        assert_eq!((tb.num, tb.den), (1, 30), "time base = 1/fps");
        let fr = encoder.frame_rate();
        assert_eq!((fr.num, fr.den), (30, 1), "frame rate = fps");
        assert!(!encoder.is_drained(), "a fresh encoder is not drained");
        assert!(!encoder.is_flushed(), "a fresh encoder is not flushed");
        Ok(())
    }

    /// 字幕编码器在打开时必须已有 ASS 脚本 header，否则 `build()` 报错
    /// （`ff_ass_split(NULL)` 会让 init 返回 `AVERROR_INVALIDDATA`）。
    #[test]
    fn test_subtitle_builder_requires_ass_header() {
        const ASS_HEADER: &str = "[Script Info]\n\
             ScriptType: v4.00+\n\
             \n\
             [V4+ Styles]\n\
             Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, \
             OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, \
             ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, \
             Alignment, MarginL, MarginR, MarginV, Encoding\n\
             Style: Default,Arial,16,&Hffffff,&Hffffff,&H0,&H0,0,0,0,0,100,100,0,0,1,1,0,2,10,10,10,1\n\
             \n\
             [Events]\n\
             Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";

        // 缺 header：必须是 header 校验失败（编码器缺失时跳过，环境差异不算失败）。
        match EncoderBuilder::new_subtitle()
            .with_codec_name(Some("mov_text".to_string()))
            .build()
        {
            Err(e) if e.to_string().contains("ASS script header") => {}
            Err(e) => {
                println!("SKIP: mov_text encoder unavailable in this build ({e})");
            }
            Ok(_) => panic!("subtitle encoder must not open without an ASS header"),
        }

        // 提供 header 后可正常构建。
        if let Err(e) = EncoderBuilder::new_subtitle()
            .with_codec_name(Some("mov_text".to_string()))
            .with_subtitle_header(ASS_HEADER)
            .build()
        {
            panic!("subtitle encoder must open once an ASS header is provided: {e}");
        }
    }

    // ====================================================================
    // 既有单元测试（原 `video_tests` 模块中不落盘的几条，其余已移到
    // `tests/encode_pipeline.rs`）
    // ====================================================================

    /// Quality::Bitrate 覆盖 with_bit_rate，并反映到 codecpar。
    #[test]
    fn test_quality_bitrate_applied() -> Result<()> {
        let encoder = EncoderBuilder::new_video(64, 64)
            .with_bit_rate(500_000)
            .with_quality(Quality::Bitrate(2_000_000))
            .build()?;
        assert_eq!(encoder.codecpar().bit_rate, 2_000_000);
        Ok(())
    }

    /// Crf 请求在不支持的编码器上回退为 bit_rate 控制（默认 1Mbps）。
    #[test]
    fn test_crf_fallback_bitrate() -> Result<()> {
        let encoder = EncoderBuilder::new_video(64, 64)
            .with_fps(25.0)
            .with_codec_name("mpeg4".to_string())
            .with_quality(Quality::Crf(20))
            .build()?;
        assert_eq!(encoder.codecpar().bit_rate, EncoderBuilder::VIDEO_BIT_RATE);
        Ok(())
    }

    /// 音频编码器忽略 Crf（视频概念），正常按 bit_rate 打开。
    #[test]
    fn test_audio_ignores_crf() -> Result<()> {
        let encoder = EncoderBuilder::new_audio(128_000, 2, 44100, SampleFormat::FLTP)
            .with_quality(Quality::Crf(20))
            .build()?;
        assert_eq!(encoder.codecpar().bit_rate, 128_000);
        Ok(())
    }

    /// builder 的缩放选项进入编码器持有的 [`Scaler`]：算法位一个、质量位可多个。
    #[test]
    fn test_builder_scale_options_reach_the_scaler() -> Result<()> {
        use crate::scale::{ScaleAlgorithm, ScaleQuality};

        // 多质量位（掩码）+ 非默认算法。
        let encoder = EncoderBuilder::new_video(320, 240)
            .with_scale_algorithm(ScaleAlgorithm::LANCZOS)
            .with_scale_quality([
                ScaleQuality::FULL_CHR_H_INT,
                ScaleQuality::ACCURATE_RND,
                ScaleQuality::BITEXACT,
            ])
            .build()?;
        assert_eq!(encoder.scaler.algorithm(), ScaleAlgorithm::LANCZOS);
        assert_eq!(encoder.scaler.quality(), ScaleQuality::default_mask());
        assert_eq!(
            encoder.scaler.flags(),
            ScaleAlgorithm::LANCZOS.as_raw() | ScaleQuality::default_mask()
        );

        // 默认策略：BICUBIC + 默认质量掩码。
        let encoder = EncoderBuilder::new_video(320, 240).build()?;
        assert_eq!(encoder.scaler.algorithm(), ScaleAlgorithm::default());
        assert_eq!(encoder.scaler.quality(), ScaleQuality::default_mask());

        // 单个质量位（`Into<u32>`）。
        let encoder = EncoderBuilder::new_video(320, 240)
            .with_scale_quality([ScaleQuality::BITEXACT])
            .build()?;
        assert_eq!(encoder.scaler.quality(), ScaleQuality::BITEXACT.as_raw());
        Ok(())
    }
}
