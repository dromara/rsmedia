use crate::codec::{AVCodecFlag, CodecContextState};
use crate::error::{Context, Result, RsmediaError};
use crate::filter::{AudioParams, Filter, FilterGraph, FilterParams, VideoParams};
#[cfg(feature = "ndarray")]
use crate::frame::{ElementType, MediaFrame};
use crate::hwaccel::{HWContext, HWDeviceConfig};
use crate::io::{Reader, Seekable};
use crate::options::Options;
use crate::resample;
use crate::resize::Resize;
use crate::scale::{ScaleAlgorithm, ScaleQuality, Scaler};
use crate::stream::StreamInfo;
use crate::strutils;
use crate::subtitle::SubtitleSegment;
use crate::{Location, MediaType, PixelFormat, SampleFormat, StreamReader, Time};

use rsmpeg::avcodec::{AVCodec, AVCodecContext, AVPacket, AVSubtitle};
use rsmpeg::avformat::AVStream;
use rsmpeg::avutil::{self, AVChannelLayoutRef, AVFrame};
use rsmpeg::ffi;

use std::sync::Arc;

/// Builds a [`Decoder`].
#[derive(Debug)]
pub struct DecoderBuilder {
    flags: AVCodecFlag,
    thread_count: usize,
    media_type: MediaType,
    codec_name: Option<String>,
    codec_opts: Option<Options>,
    filters: Option<Vec<Filter>>,
    hw_device_config: Option<HWDeviceConfig>,
    /// 缩放核选择（互斥，只取一个算法位）
    scale_algorithm: ScaleAlgorithm,
    /// 缩放质量位（可多位，见 [`ScaleQuality`]）；构建 `Scaler` 时由 [`ScaleQuality::mask`] 合成为掩码
    scale_quality: Vec<ScaleQuality>,
    /// 是否用 `AVBufferPool` 池化缩放输出的帧缓冲（默认关闭）。
    scale_pool: bool,
    resize: Option<Resize>,
    /// 解码输出目标像素格式（仅视频），默认 [`PixelFormat::YUV420P`]。
    pix_fmt: Option<PixelFormat>,
    /// 解码输出目标采样格式（仅音频）。`None` 表示保留编解码器原生格式。
    sample_fmt: Option<SampleFormat>,
}

impl DecoderBuilder {
    /// create a new decoder builder with specified media type.
    ///
    /// # Arguments
    ///
    /// `media_type` - The media type of the decoder.
    pub fn new(media_type: MediaType) -> Self {
        Self {
            media_type,
            filters: None,
            codec_name: None,
            codec_opts: None,
            hw_device_config: None,
            thread_count: num_cpus::get(),
            flags: AVCodecFlag::LOW_DELAY,
            scale_algorithm: ScaleAlgorithm::default(),
            scale_quality: ScaleQuality::default_quality().to_vec(),
            scale_pool: false,
            resize: None,
            pix_fmt: None,
            sample_fmt: None,
        }
    }

    /// Set decoding flags.
    pub fn with_flags(mut self, flags: AVCodecFlag) -> Self {
        self.flags = flags;
        self
    }

    /// Set the codec name to use for decoding.
    /// If not set, the decoder will try to guess the codec based on the input.
    pub fn with_codec_name(mut self, codec_name: impl Into<Option<String>>) -> Self {
        self.codec_name = codec_name.into();
        self
    }

    /// codec options to use for decoding.
    pub fn with_options(mut self, options: impl Into<Option<Options>>) -> Self {
        self.codec_opts = options.into();
        self
    }

    /// set the thread count.
    pub fn with_thread_count(mut self, thread_count: usize) -> Self {
        self.thread_count = thread_count;
        self
    }

    /// set the filters to apply to decoded frames.
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

    /// Set the scaling algorithm used when converting decoded frames to the
    /// output pixel format (and, with [`Self::with_resize`], to the output size).
    ///
    /// The algorithm picks the scaling kernel and is **mutually exclusive** —
    /// FFmpeg's header states *"Scaler selection options. Only one may be active
    /// at a time."* Defaults to [`ScaleAlgorithm::BICUBIC`]; use
    /// [`ScaleAlgorithm::BILINEAR`] for output consistent with FFmpeg's command
    /// line default, or [`ScaleAlgorithm::AREA`] when downscaling. The
    /// quality/behaviour bits are set separately with [`Self::with_scale_quality`].
    pub fn with_scale_algorithm(mut self, algorithm: ScaleAlgorithm) -> Self {
        self.scale_algorithm = algorithm;
        self
    }

    /// Set the scaling quality/behaviour bits used when converting decoded frames.
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

    /// Enable (`true`) or disable (`false`) pooled allocation of the scaler's
    /// destination frames (see [`Scaler::with_buffer_pool`]).
    ///
    /// Off by default. With it on, frames this decoder scales are allocated from an
    /// internal `AVBufferPool` instead of being freshly allocated per frame, so a
    /// steady stream of same-geometry conversions stops allocating after a couple
    /// of frames; buffers are zero-filled before use, matching `alloc_buffer`.
    pub fn with_scale_pool(mut self, enabled: bool) -> Self {
        self.scale_pool = enabled;
        self
    }

    /// Set the resize strategy applied to decoded video frames.
    ///
    /// Controls the output dimensions: [`Resize::Exact`] forces an exact size,
    /// [`Resize::Fit`]/[`Resize::FitEven`] keep the aspect ratio while fitting
    /// within the given bounds. When `None`, frames keep their source size.
    pub fn with_resize(mut self, resize: Resize) -> Self {
        self.resize = Some(resize);
        self
    }

    /// Set the output pixel format of decoded video frames.
    ///
    /// 默认 [`PixelFormat::YUV420P`]。可指定任何能表示为数据平面的像素格式
    /// （见 [`PixelFormat::data_layout`]）：
    /// - packed 8bit：GRAY8 / YUYV422 / UYVY422 / RGB24 / BGR24 / RGBA 族
    ///   —— 交错为单个数组；
    /// - planar / 半平面 / 9..16bit：YUV420P / YUV422P / YUV444P / NV12 /
    ///   GBRP / YUV420P10LE 等 —— 每平面一个数组。
    ///
    /// 解码输出 [`MediaFrame::data`](crate::frame::MediaFrame::data) 的变体与
    /// 形状随目标格式而变。位流 / 调色板 / 硬件格式无法用数据平面表达，
    /// 构建时即报错。
    ///
    /// 源格式与目标不一致时由 swscale 自动转换（如 NV12 → RGBA）。
    ///
    /// 仅对视频解码器有效；其他媒体类型构建时返回错误（fail-fast）。
    ///
    /// 注意：解码输出帧的元素类型 `T` 必须与格式的每样本字节数一致 ——
    /// 8bit 格式用 `u8`，9..16bit 格式用 `u16`（见 [`PixelFormat::bytes_per_component`]）。
    pub fn with_pix_fmt(mut self, pix_fmt: PixelFormat) -> Self {
        self.pix_fmt = Some(pix_fmt);
        self
    }

    /// Set the output sample format of decoded audio frames.
    ///
    /// 默认保留编解码器**原生**采样格式（AAC/AC-3 为 `FLTP`、MP2 为 `S16P`、
    /// PCM 为各自的 `S16LE`/`S32LE`…）。指定本项后，解码输出统一重采样到目标
    /// 格式，于是 `decode::<T>` 的元素类型不再需要随源文件而变 ——
    /// 例如统一到 [`SampleFormat::FLTP`] 后，任何音频文件都能用 `decode::<f32>()`
    /// 读取，不必先查 `codecpar().format`。
    ///
    /// 采样率与声道布局**不变**，只换采样格式（同一率下等长转换）。源格式已与
    /// 目标相同时不做任何转换，因此该选项在无需转换时零开销。
    ///
    /// 解码输出 `MediaFrame::data` 的交错/平面变体随之变化（见
    /// [`SampleFormat::data_layout`]）：平面格式（`FLTP` 等）每声道一个
    /// `(1, nb_samples)` 平面，交错格式（`FLT` 等）为单个
    /// `(1, nb_samples, nb_channels)` 数组。
    ///
    /// 仅对音频解码器有效；其他媒体类型构建时返回错误（fail-fast）。
    ///
    /// 注意：元素类型 `T` 的字节宽度必须与目标格式的每样本字节数一致
    /// （见 [`SampleFormat::get_bytes_per_sample`]）——`FLTP` 用 `f32`、
    /// `S16P` 用 `i16`、`S32P` 用 `i32`。
    pub fn with_sample_fmt(mut self, sample_fmt: SampleFormat) -> Self {
        self.sample_fmt = Some(sample_fmt);
        self
    }

    /// 校验像素格式能否以数据平面承载（非位流/调色板/硬件格式）。
    fn ensure_pix_fmt_storable(fmt: PixelFormat) -> Result<()> {
        if !fmt.is_plane_storable() {
            return Err(RsmediaError::msg(format!(
                "Unsupported output pixel format: {fmt:?}; it cannot be stored as sample planes \
                 (bitstream, paletted and hardware formats are not supported)"
            )));
        }
        Ok(())
    }

    /// 某个仅对视频解码器生效的配置项被用于其它媒体类型时构造的错误。
    fn option_only_for(opt: &'static str, value: String, media_type: MediaType) -> RsmediaError {
        RsmediaError::msg(format!(
            "{opt}({value}) is only valid for {} decoders, got media type: {media_type:?}",
            media_type.get_media_name()
        ))
    }

    fn setup_codec_context(&self, decoder: &mut AVCodecContext, input: &AVStream) -> Result<()> {
        let media_type = self.media_type;
        if media_type as ffi::AVMediaType != decoder.codec_type {
            return Err(RsmediaError::msg(format!(
                "Decoder codec type not supported: {:?} vs. {:?}",
                media_type, decoder.codec_type
            )));
        }

        decoder.apply_codecpar(&input.codecpar())?;
        decoder.set_flags(self.flags as i32);
        decoder.set_time_base(input.time_base);
        decoder.set_pkt_timebase(input.time_base);
        if let Some(framerate) = input.guess_framerate() {
            decoder.set_framerate(framerate);
        }

        unsafe {
            (*decoder.as_mut_ptr()).thread_count = self.thread_count as i32;
        }

        Ok(())
    }

    /// 构建一个**裸** [`Decoder`]（不持有 reader）。
    ///
    /// 适合需要精细控制 reader 生命周期的高级场景：构建时会在内部创建并持有
    /// 一个 reader，解码时需每帧传入该 reader（`decoder.decode(&mut reader)`），
    /// 且 seek 需要显式操作 reader。
    pub fn build(self, source: impl Into<Location>) -> Result<Decoder> {
        let reader = StreamReader::new(source)?;
        self.build_from_reader(&reader)
    }

    /// 用给定的 reader 构建**裸** [`Decoder`]（不持有 reader）。
    ///
    /// 高级/内部场景使用（如 mux 多流共享同一 reader）。解码时需每帧传入
    /// reader，且不支持 seek。
    pub fn build_from_reader<R: Reader>(self, reader: &R) -> Result<Decoder> {
        let media_type = self.media_type;
        let (stream_index, codec_name) = reader.find_best_stream(media_type)?;
        let input_stream = reader
            .input()
            .streams()
            .get(stream_index)
            .ok_or(RsmediaError::msg(format!(
                "stream: {stream_index} not found!"
            )))?;

        // 优先用调用方指定的解码器名，否则用流自带的名字（`find_best_stream`
        // 的返回值）。这两个名字来自不同来源，不要写在同名绑定里——那样两个分支
        // 看起来一模一样，实际解析到不同的变量。
        let codec_name = self.codec_name.as_deref().unwrap_or(&codec_name);
        let codec = AVCodec::find_decoder_by_name(&strutils::str_to_cstring(codec_name))
            .context(format!("Failed to find decoder by name: '{codec_name}'"))?;

        let duration = Time::new(Some(input_stream.duration), input_stream.time_base);
        let nb_frames = input_stream.nb_frames;
        let frame_rate = (
            avutil::av_q2d(input_stream.r_frame_rate) as f32,
            avutil::av_q2d(input_stream.avg_frame_rate) as f32,
        );

        let mut decode_ctx = AVCodecContext::new(&codec);
        self.setup_codec_context(&mut decode_ctx, input_stream)?;

        // video
        let init_width = decode_ctx.width;
        let init_height = decode_ctx.height;

        let hw_context = self
            .hw_device_config
            .filter(|_cfg| {
                // hardware acceleration enabled for video
                media_type == MediaType::VIDEO
            })
            .map(|cfg| {
                // codec support or not for hardware acceleration
                let hw_pixel = cfg
                    .device_type
                    .find_hw_pixel_format_with_codec(&codec)
                    .ok_or_else(|| {
                        let codec_name = strutils::cstr_to_string(codec.name()).unwrap();
                        RsmediaError::msg(format!(
                            "Decoder with HW acceleration is not supported for codec: {codec_name}"
                        ))
                    })?;

                log::info!(
                    "Video decoder with HW acceleration codec: {:?}, hw_pixel: {:?}, config: {:#?}",
                    codec.name(),
                    PixelFormat::from(hw_pixel),
                    cfg
                );

                // create hardware context
                HWContext::new(cfg)
                    .and_then(|ctx| {
                        // 注意：setup_decoder_frames 可能会改变 decode_ctx.pix_fmt
                        ctx.setup_decoder_frames(&mut decode_ctx, init_width, init_height)?;
                        Ok(ctx)
                    })
                    .context("Hardware acceleration context initialization failed")
            })
            .transpose()?;

        let dict = self.codec_opts.and_then(|opts| opts.into_dict());
        decode_ctx
            .open(dict)
            .context("Failed to open decoder for stream")?;

        let stream_info = StreamInfo::from_stream(input_stream)?;
        log::info!("{stream_info}");

        // 输出像素格式：仅视频有效。任何能表示为数据平面的格式都接受
        // （布局由描述符推导，见 `PixelFormat::data_layout`）；位流 / 调色板 /
        // 硬件格式在构建期快速失败，而不是拖到运行时。解码输出经 swscale
        // 统一转换到目标格式。
        // 非视频类型配置了 pix_fmt 视为调用方错误，快速失败而非静默忽略。
        let output_pix_fmt = match (media_type, self.pix_fmt) {
            (MediaType::VIDEO, Some(fmt)) => {
                Self::ensure_pix_fmt_storable(fmt)?;
                fmt
            }
            (_, None) => PixelFormat::YUV420P,
            (media_type, Some(fmt)) => {
                return Err(Self::option_only_for(
                    "with_pix_fmt",
                    format!("{fmt:?}"),
                    media_type,
                ));
            }
        };

        // 输出采样格式：仅音频有效。`None` = 保留编解码器原生格式（默认），
        // 代价为零；指定后解码帧在进滤镜图之前统一转换到目标格式。
        // 非音频类型配置了 sample_fmt 视为调用方错误，快速失败而非静默忽略。
        let output_sample_fmt = match (media_type, self.sample_fmt) {
            (MediaType::AUDIO, None) => None,
            (MediaType::AUDIO, Some(fmt)) if fmt != SampleFormat::NONE => Some(fmt),
            (MediaType::AUDIO, Some(fmt)) => {
                return Err(RsmediaError::msg(format!(
                    "Unsupported output sample format: {fmt:?}"
                )));
            }
            (media_type, Some(fmt)) => {
                return Err(Self::option_only_for(
                    "with_sample_fmt",
                    format!("{fmt:?}"),
                    media_type,
                ));
            }
            (_, None) => None,
        };

        let filter_graph = if let Some(filters) = self.filters {
            let filter_params = match media_type {
                MediaType::VIDEO => FilterParams::Video(VideoParams {
                    width: init_width,
                    height: init_height,
                    src_format: output_pix_fmt,
                    format: output_pix_fmt,
                    time_base: decode_ctx.time_base,
                    frame_rate: decode_ctx.framerate,
                    pixel_aspect: decode_ctx.sample_aspect_ratio,
                }),
                MediaType::AUDIO => FilterParams::Audio(AudioParams {
                    nb_channels: decode_ctx.ch_layout.nb_channels,
                    sample_rate: decode_ctx.sample_rate,
                    // 滤镜图的输入格式须与送进去的帧一致：指定了输出采样格式时，
                    // 帧在进图之前已转换（见 `receive_frame_from_decoder`），因此
                    // 这里用目标格式而非编解码器原生格式。
                    format: output_sample_fmt.unwrap_or(SampleFormat::from(decode_ctx.sample_fmt)),
                    src_format: output_sample_fmt
                        .unwrap_or(SampleFormat::from(decode_ctx.sample_fmt)),
                    time_base: decode_ctx.time_base,
                }),
                _ => {
                    return Err(RsmediaError::msg(format!(
                        "Unsupported filter for media type: {media_type:?}"
                    )));
                }
            };

            let mut graph = FilterGraph::new();
            // 验证 Filter 链的媒体类型是否与当前流匹配
            if !filters.iter().all(|f| f.media_type() == media_type) {
                return Err(RsmediaError::msg(format!(
                    "Filter media type mismatch for stream type {media_type:?}"
                )));
            }
            graph
                .init(&filter_params, filters.as_slice())
                .context("Failed to initialize filter graph")?;

            // 参数随图一起留下：重启流水线时必须重建一张新图。
            Some(DecodeFilterChain {
                graph,
                params: filter_params,
                filters,
            })
        } else {
            None
        };

        Ok(Decoder {
            media_type,
            stream_index,
            duration,
            nb_frames,
            frame_rate,
            hw_context,
            filter_graph,
            context: decode_ctx,
            state: CodecContextState::Normal,
            scaler: Scaler::new_with_options(self.scale_algorithm, self.scale_quality)
                .with_buffer_pool(self.scale_pool),
            resize: self.resize,
            output_pix_fmt,
            output_sample_fmt,
        })
    }
}

/// The decode pipeline's filter graph **together with what it was built from**.
///
/// The graph and its inputs are one unit on purpose: the pipeline can only be
/// restarted by rebuilding the graph (see `FilterGraph::rebuild`), which needs
/// those exact parameters, so keeping them apart would let them drift. The same
/// reasoning as `CodecContextState`: one fact, one place.
struct DecodeFilterChain {
    graph: FilterGraph,
    params: FilterParams,
    filters: Vec<Filter>,
}

/// Decode video files and streams.
///
/// # Example
///
/// ```ignore
/// let mut reader = StreamReader::new("video.mp4").unwrap();
/// let mut decoder = Decoder::new_video("video.mp4").unwrap();
/// while let Some(frame) = decoder.decode(&mut reader).unwrap() {
///     println!("Got frame!");
/// }
/// ```
pub struct Decoder {
    context: AVCodecContext,
    filter_graph: Option<DecodeFilterChain>,
    hw_context: Option<Arc<HWContext>>,
    /// (r_frame_rate, avg_frame_rate)
    frame_rate: (f32, f32),
    nb_frames: i64,
    duration: Time,
    stream_index: usize,
    media_type: MediaType,
    state: CodecContextState,
    scaler: Scaler,
    resize: Option<Resize>,
    /// 解码输出目标像素格式（仅视频）
    output_pix_fmt: PixelFormat,
    /// 解码输出目标采样格式（仅音频）；`None` = 保留编解码器原生格式
    output_sample_fmt: Option<SampleFormat>,
}

impl Decoder {
    /// Create a decoder to decode the specified source.
    ///
    /// # Arguments
    ///
    /// * `reader` - A [`Reader`] to read the source from.
    #[inline]
    pub fn new_video(source: impl Into<Location>) -> Result<Decoder> {
        DecoderBuilder::new(MediaType::VIDEO).build(source)
    }

    /// Create a decoder to decode the audio stream of the specified source.
    ///
    /// # Arguments
    ///
    /// * `reader` - A [`Reader`] to read the source from.
    #[inline]
    pub fn new_audio(source: impl Into<Location>) -> Result<Decoder> {
        DecoderBuilder::new(MediaType::AUDIO).build(source)
    }

    /// Create a decoder to decode the subtitle stream of the specified source.
    ///
    /// 字幕解码走独立的 [`decode_subtitle_segment`](Self::decode_subtitle_segment)
    /// 通道（输出 [`SubtitleSegment`]，与音视频的 AVFrame 通道并行）。
    ///
    /// # Arguments
    ///
    /// * `reader` - A [`Reader`] to read the source from.
    #[inline]
    pub fn new_subtitle(source: impl Into<Location>) -> Result<Decoder> {
        DecoderBuilder::new(MediaType::SUBTITLE).build(source)
    }

    /// Get the decoders input size width
    #[inline(always)]
    pub fn width(&self) -> i32 {
        self.context.width
    }

    /// Get the decoders input size height
    #[inline(always)]
    pub fn height(&self) -> i32 {
        self.context.height
    }

    /// The pixel format of the frames this decoder **yields**.
    ///
    /// With [`DecoderBuilder::with_pix_fmt`] that is the configured output format
    /// (decoding converts to it); without it, video still comes out as
    /// [`PixelFormat::YUV420P`]. The codec's own internal format is not what this
    /// reports — read it from the container's stream parameters if needed.
    #[inline]
    pub fn pix_fmt(&self) -> PixelFormat {
        self.output_pix_fmt
    }

    #[inline]
    pub fn sample_rate(&self) -> i32 {
        self.context.sample_rate
    }

    /// The sample format of the frames this decoder **yields**, which is the
    /// element type `T` [`decode`](Self::decode) must be called with.
    ///
    /// Audio keeps the codec's native format unless
    /// [`DecoderBuilder::with_sample_fmt`] unifies the output, in which case this
    /// reports that target format — so the value always matches the frames that
    /// actually arrive.
    #[inline]
    pub fn sample_fmt(&self) -> SampleFormat {
        self.output_sample_fmt
            .unwrap_or_else(|| SampleFormat::from(self.context.sample_fmt))
    }

    #[inline]
    pub fn ch_layout(&self) -> AVChannelLayoutRef<'_> {
        self.context.ch_layout()
    }

    /// Get the decoders input duration
    #[inline(always)]
    pub fn duration(&self) -> Time {
        self.duration
    }

    /// Get decoder time base.
    #[inline(always)]
    pub fn time_base(&self) -> ffi::AVRational {
        self.duration.time_base
    }

    /// Number of frames in the input stream (`AVStream.nb_frames`).
    ///
    /// `0` when the container does not state a count.
    #[inline(always)]
    pub fn nb_frames(&self) -> i64 {
        self.nb_frames
    }

    /// The input stream's frame rates, as `(r_frame_rate, avg_frame_rate)`.
    ///
    /// Two rates, not one: `r_frame_rate` is the lowest rate that can represent
    /// all timestamps exactly, `avg_frame_rate` the average over the stream.
    #[inline(always)]
    pub fn frame_rates(&self) -> (f32, f32) {
        self.frame_rate
    }

    #[inline(always)]
    pub fn media_type(&self) -> MediaType {
        self.media_type
    }

    #[inline]
    pub fn stream_index(&self) -> usize {
        self.stream_index
    }

    /// Check if decoder is in draining mode.
    pub fn is_drained(&self) -> bool {
        self.state == CodecContextState::Drained
    }

    /// Whether the decoder itself has reached EOF.
    ///
    /// This is **not** the "may I stop?" predicate: with a filter graph attached
    /// the graph may still hold buffered frames after the decoder is done (a
    /// delayed filter such as `framerate`), so stopping here would drop them. Use
    /// [`is_finished`](Self::is_finished) for that.
    pub fn is_flushed(&self) -> bool {
        self.state == CodecContextState::Flushed
    }

    /// Whether the decode pipeline is fully done: the decoder reached EOF **and**
    /// the filter graph (when present) has flushed its buffered frames.
    ///
    /// This is the predicate a caller loop should stop on. Stopping at
    /// [`is_flushed`](Self::is_flushed) instead would cut off frames a delayed
    /// filter still has to emit; calling `decode`/`decode_raw` after *this* is
    /// true is what returns "cannot decode after flushed".
    pub fn is_finished(&self) -> bool {
        self.is_flushed()
            && match &self.filter_graph {
                Some(chain) => chain.graph.is_flushed(),
                None => true,
            }
    }

    /// Decode a single frame.
    ///
    /// `T` must match the width of the samples being decoded: the byte size of
    /// the frame's format. For video that is the output pixel format's
    /// [`bytes_per_component`](PixelFormat::bytes_per_component) — 8-bit formats
    /// take `u8`, 9..16-bit ones `u16`. For audio it is the decoded sample
    /// format: the codec's native one unless
    /// [`with_sample_fmt`](DecoderBuilder::with_sample_fmt) unifies the output
    /// (e.g. to `FLTP`, which every audio codec can be read as with `decode::<f32>()`).
    /// A mismatch is rejected with the format and both widths in the error.
    ///
    /// # Return value
    ///
    /// A tuple of the frame timestamp (relative to the stream) and the frame itself.
    ///
    /// # Example
    ///
    /// ```ignore
    /// loop {
    ///     let (ts, frame) = decoder.decode::<u8>()?;
    ///     // Do something with frame...
    /// }
    /// ```
    #[cfg(feature = "ndarray")]
    pub fn decode<T>(&mut self, reader: &mut impl Reader) -> Result<Option<MediaFrame<T>>>
    where
        T: ElementType,
    {
        decode_stream(
            self,
            reader,
            Decoder::decode_packet::<T>,
            Decoder::drain::<T>,
        )
    }

    /// Decode a single frame as a `MediaFrame<u8>`.
    ///
    /// Convenience for `decode::<u8>()` which is the common video path
    /// (8-bit formats such as YUV420P/RGB24). For audio the element type must
    /// match the decoded sample format size: with the codec's native format by
    /// default, or with the format [`with_sample_fmt`](DecoderBuilder::with_sample_fmt)
    /// unifies the output to — e.g. `decode::<f32>()` for `FLTP`/`FLT`. Use
    /// [`decode_raw`](Self::decode_raw) to avoid the typed conversion entirely.
    ///
    /// # Return value
    ///
    /// The decoded frame, or [`None`] at end of stream.
    #[cfg(feature = "ndarray")]
    pub fn decode_frame(&mut self, reader: &mut impl Reader) -> Result<Option<MediaFrame<u8>>> {
        self.decode::<u8>(reader)
    }

    /// Decode a single frame and return the raw ffmpeg `AvFrame`.
    ///
    /// # Arguments
    ///
    /// * `reader` - A [`Reader`] to read the source from.
    ///
    /// # Return value
    ///
    /// The decoded raw frame that after decoding, HW download, and filtering as [`AVFrame`].
    pub fn decode_raw<R>(&mut self, reader: &mut R) -> Result<Option<AVFrame>>
    where
        R: Reader,
    {
        decode_stream(self, reader, Decoder::decode_raw_packet, Decoder::drain_raw)
    }

    /// 解码单个 packet 为字幕（仅字幕解码器，低层手动解码 API）。
    ///
    /// 字幕解码走 rsmpeg 的 [`AVCodecContext::decode_subtitle`]（同步 API，
    /// 与音视频的 send_packet/receive_packet 帧通道并行）：每个 packet 直接
    /// 产出 0 或 1 条字幕，无需排空帧队列。传 [`None`] 表示 flush（排空带
    /// `AV_CODEC_CAP_DELAY` 的解码器；无缓冲的解码器直接返回 [`None`]）。
    ///
    /// 返回的 [`AVSubtitle`] 可通过
    /// [`SubtitleSegment::from_avsubtitle`](crate::subtitle::SubtitleSegment::from_avsubtitle)
    /// 转换为纯文本段落，或经 `rect_iter` 自行处理。
    pub fn decode_subtitle_packet(
        &mut self,
        packet: Option<&mut AVPacket>,
    ) -> Result<Option<AVSubtitle>> {
        self.context
            .decode_subtitle(packet)
            .context("Failed to decode subtitle packet")
    }

    /// 解码下一条字幕段落（仅字幕解码器，推荐入口）。
    ///
    /// 驱动「读 packet → 解码 → EOF 排空」状态机：跳过非目标流的 packet，
    /// 目标流 packet 经 [`Self::decode_subtitle_packet`] 解码；reader 耗尽后
    /// 以空包 flush 字幕解码器（处理 `AV_CODEC_CAP_DELAY` 的尾部缓冲）。
    /// 无文本 rect 的字幕（如位图字幕）会被跳过。
    ///
    /// # Return value
    ///
    /// The decoded [`SubtitleSegment`], or [`None`] at end of stream.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut decoder = Decoder::new_subtitle("video.mp4").unwrap();
    /// while let Some(segment) = decoder.decode_subtitle_segment(&mut reader)? {
    ///     println!("{}-{}ms: {}", segment.start_ms, segment.end_ms, segment.text);
    /// }
    /// ```
    pub fn decode_subtitle_segment<R>(&mut self, reader: &mut R) -> Result<Option<SubtitleSegment>>
    where
        R: Reader,
    {
        if self.media_type != MediaType::SUBTITLE {
            return Err(RsmediaError::msg(format!(
                "decode_subtitle_segment requires a subtitle decoder, got media type: {:?}",
                self.media_type
            )));
        }
        if self.is_flushed() {
            return Err(RsmediaError::invalid_config(
                "Decoder cannot decode after flushed. Call reset().",
            ));
        }

        let mut read_exhausted = false;
        loop {
            if !read_exhausted {
                match reader.read_packet()? {
                    Some((stream_index, mut packet)) => {
                        if stream_index != self.stream_index {
                            log::trace!("skip stream index: {stream_index}");
                            continue;
                        }
                        if let Some(subtitle) = self.decode_subtitle_packet(Some(&mut packet))?
                            && let Some(segment) = SubtitleSegment::from_avsubtitle(&subtitle)
                        {
                            return Ok(Some(segment));
                        }
                    }
                    None => {
                        log::debug!("No more packets, Reader exhausted.");
                        read_exhausted = true;
                    }
                }
            } else {
                // EOF：空包 flush 字幕解码器（CAP_DELAY 解码器可能仍有缓冲字幕）
                match self.decode_subtitle_packet(None)? {
                    Some(subtitle) => {
                        if let Some(segment) = SubtitleSegment::from_avsubtitle(&subtitle) {
                            return Ok(Some(segment));
                        }
                    }
                    None => {
                        self.state = CodecContextState::Flushed;
                        log::debug!("Subtitle decoder flushed. EOF reached.");
                        return Ok(None);
                    }
                }
            }
        }
    }

    /// Decode one [`AVPacket`].
    ///
    /// Feeds the packet to the decoder and returns a frame if there is one available. The caller
    /// should keep feeding packets until the decoder returns a frame.
    ///
    /// # Return value
    ///
    /// A tuple of the [`AVFrame`] and timestamp (relative to the stream) and the frame itself if the
    /// decoder has a frame available, [`None`] if not.
    #[cfg(feature = "ndarray")]
    pub fn decode_packet<T>(&mut self, packet: &AVPacket) -> Result<Option<MediaFrame<T>>>
    where
        T: ElementType,
    {
        match self.decode_raw_packet(packet) {
            Ok(Some(raw_frame)) => Ok(Some(MediaFrame::<T>::from_avframe(&raw_frame)?)),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Decode one [`AVPacket`].
    ///
    /// Feeds the packet to the decoder and returns a frame if there is one available. The caller
    /// should keep feeding packets until the decoder returns a frame.
    ///
    /// # Errors
    ///
    /// Returns an error once the decoder has been flushed (`reset` is required before
    /// decoding again) or when the decoder itself fails.
    ///
    /// # Return value
    ///
    /// The decoded raw frame as [`AVFrame`] if the decoder has a frame available, [`None`] if not.
    pub fn decode_raw_packet(&mut self, packet: &AVPacket) -> Result<Option<AVFrame>> {
        // 与 `decode`/`decode_raw` 同一阶段守卫：解码器 flush 之后再送包，FFmpeg
        // 只会回一句 "Decoder is already flushed"，调用方看不出该怎么办。
        if self.is_finished() {
            return Err(RsmediaError::invalid_config(
                "Decoder cannot decode after flushed. Call reset().",
            ));
        }
        self.send_packet_to_decoder(Some(packet))?;
        self.receive_normalized_frame()
    }

    /// Drain one frame from the decoder.
    ///
    /// The first call sends end-of-stream and puts the decoder in draining mode;
    /// afterwards the normal decode path returns an error until
    /// [`reset`](Self::reset) is called.
    ///
    /// # Return value
    ///
    /// A tuple of the [`AVFrame`] and timestamp (relative to the stream) and the frame itself if the
    /// decoder has a frame available, [`None`] if not.
    #[cfg(feature = "ndarray")]
    pub fn drain<T>(&mut self) -> Result<Option<MediaFrame<T>>>
    where
        T: ElementType,
    {
        match self.drain_raw() {
            Ok(Some(raw_frame)) => Ok(Some(MediaFrame::<T>::from_avframe(&raw_frame)?)),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Drain one frame from the decoder.
    ///
    /// The first call sends end-of-stream and puts the decoder in draining mode;
    /// afterwards the normal decode path returns an error until
    /// [`reset`](Self::reset) is called.
    ///
    /// # Return value
    ///
    /// The decoded raw frame as [`AVFrame`] if the decoder has a frame available, [`None`] if not.
    ///
    /// 作为低层手动解码 API 的一部分公开：配合 [`decode_raw_packet`](Self::decode_raw_packet)
    /// 使用，可逐 packet 送入解码器并排空缓冲帧。需要 [`MediaFrame`] 的高级调用请使用
    /// [`drain`](Self::drain)。
    pub fn drain_raw(&mut self) -> Result<Option<AVFrame>> {
        if self.state == CodecContextState::Normal {
            self.send_packet_to_decoder(None)?;
            // 已发送 EOS，进入 draining 模式。此后 EAGAIN 表示"仍在 drain"，
            // 而非 read 阶段缺包，因此在此处显式置位。
            self.state = CodecContextState::Drained;
        }
        self.receive_normalized_frame()
    }

    /// Restarts the whole decode pipeline so the decoder can be used again after
    /// draining (or after a seek).
    ///
    /// This is [`flush_buffers`](Self::flush_buffers) — codec buffers *and* the
    /// filter graph — plus the phase reset. Restoring only the codec would leave a
    /// filtered decoder in a state that reports "ready" while every frame is
    /// rejected by the graph.
    pub fn reset(&mut self) -> Result<()> {
        self.flush_buffers()?;
        self.state = CodecContextState::Normal;
        Ok(())
    }

    /// Discards the decoder's buffered frames and resets it to a clean state.
    ///
    /// This is `avcodec_flush_buffers`: after a seek the decoder still holds
    /// frames from the old position, and this drops them so decoding restarts at
    /// the new one. It is deliberately **not** named `flush` — on an
    /// [`Encoder`](crate::encode::Encoder), `flush` *drains* everything that is
    /// still buffered towards the writer, the opposite direction.
    ///
    /// The filter graph is part of the pipeline and holds frames of its own
    /// (a delay filter such as `fps` keeps one, and anything queued behind it
    /// stays inside the graph). `avcodec_flush_buffers` knows nothing about it, so
    /// this rebuilds the graph as well; without that, frames from **before** the
    /// seek would be emitted after it.
    pub fn flush_buffers(&mut self) -> Result<()> {
        unsafe {
            ffi::avcodec_flush_buffers(self.context.as_mut_ptr());
        }
        self.rebuild_filter_graph()
    }

    /// Rebuilds the filter graph (if any) so it forgets every buffered frame.
    ///
    /// See [`FilterGraph::rebuild`] for why a rebuild is the only way to do this.
    /// The graph's build parameters were kept at construction for exactly this
    /// call, so a rebuilt graph is identical to the original one.
    fn rebuild_filter_graph(&mut self) -> Result<()> {
        let Some(chain) = self.filter_graph.as_mut() else {
            return Ok(());
        };
        let DecodeFilterChain {
            graph,
            params,
            filters,
        } = chain;
        graph
            .rebuild(params, filters)
            .context("Failed to rebuild the filter graph")
    }

    /// Send packet to decoder.
    /// Ensure rescaling timestamps accordingly before sending to decoder.
    fn send_packet_to_decoder(&mut self, packet: Option<&AVPacket>) -> Result<()> {
        self.context
            .send_packet(packet)
            .context("Failed to send packet to decoder")?;
        Ok(())
    }

    /// Pulls one frame out of the decoder and returns it in a uniform shape.
    ///
    /// This is the single place where a decoded frame is normalised, in this
    /// order: download hardware frames to system memory, convert the video pixel
    /// format / resize through swscale, convert the audio sample format through
    /// swresample, and finally run the result through the filter graph (when one
    /// is configured). Callers above see only the resulting software frame.
    fn receive_normalized_frame(&mut self) -> Result<Option<AVFrame>> {
        // 1. 从解码器获取原始帧
        let decoded_frame = match self.decoder_receive_frame() {
            Ok(Some(f)) => f,
            Ok(None) => {
                // 解码器当前无帧可出。按状态区分是"仍需更多输入"还是"已到 EOF"：
                // - Normal / Drained：读阶段或 drain 阶段的 EAGAIN，需要继续喂包，
                //   此时绝不能刷新 filter（否则会给 buffersrc 发 EOF，后续真实帧
                //   提交会得到 AVERROR_EOF）。
                // - Flushed：解码器到达 EOF，此时驱动 filter graph 冲刷内部缓冲帧
                //   （如 fps/setpts 等带延迟滤镜）。逐帧调用 `process_frame(None)`，
                //   每帧返回一帧，直到 graph 进入 Flushed 状态。
                match self.state {
                    CodecContextState::Normal | CodecContextState::Drained => return Ok(None),
                    CodecContextState::Flushed => {
                        if let Some(chain) = self.filter_graph.as_mut()
                            && !chain.graph.is_flushed()
                        {
                            match chain.graph.process_frame(None)? {
                                Some(frame) => return Ok(Some(frame)),
                                None => {
                                    // 已无更多缓冲帧（graph 此时已 Flushed）
                                    debug_assert!(chain.graph.is_flushed());
                                }
                            }
                        }
                        return Ok(None);
                    }
                }
            }
            Err(e) => return Err(e),
        };

        // 2. 处理硬件加速帧下载,
        let sw_frame = match &self.hw_context {
            Some(hw_ctx) if hw_ctx.is_hw_frame(&decoded_frame) => {
                // hw_frame -> sw_frame
                hw_ctx
                    .hw_download(&decoded_frame)
                    .context("Failed HW frame download")?
            }
            _ => {
                // 已经是 CPU 帧或无 HWaccel
                decoded_frame
            }
        };

        // 3. 统一视频输出格式（如 YUV420P / RGB24，由 `with_pix_fmt` 配置）
        // 例如：
        // 无硬件加速，默认解码格式 YUV420P
        // 存在硬件加速帧，则转换 NV12 -> 目标格式
        // 注意：这里无论是否有 Filter，都会统一转成目标格式（因为
        // `MediaFrame` 视频目前主要支持 YUV420P/RGB24）。因此即使源是
        // yuv444p / nv12 / 10-bit，解码输出也会被转成目标格式，不会颜色错乱。
        let raw_frame = match self.media_type {
            MediaType::VIDEO => {
                let target_sw_pix_fmt = self.output_pix_fmt;
                // 计算目标尺寸：无 resize 时保持源尺寸，有 resize 时按策略计算
                let (out_w, out_h) = match self.resize {
                    Some(resize) => resize
                        .compute_for((sw_frame.width as u32, sw_frame.height as u32))
                        .ok_or_else(|| {
                            let (w, h) = (sw_frame.width, sw_frame.height);
                            RsmediaError::msg(format!(
                                "Cannot resize frame {w}x{h} into {resize:?}"
                            ))
                        })?,
                    None => (sw_frame.width as u32, sw_frame.height as u32),
                };
                self.scaler.scale_if_needed(
                    sw_frame,
                    out_w as i32,
                    out_h as i32,
                    target_sw_pix_fmt,
                )?
            }
            MediaType::AUDIO => match self.output_sample_fmt {
                // 统一音频输出格式（由 `with_sample_fmt` 配置）。与视频侧一样在
                // 进滤镜图之前完成，图内因此按目标格式声明输入（见 build_from_reader）。
                // 只在格式真的不同、且帧确实带样本时转换：默认（未指定目标）与
                // 「目标 == 原生」两种情况都零开销，空帧也无从转换。
                Some(target)
                    if target != SampleFormat::from(sw_frame.format) && sw_frame.nb_samples > 0 =>
                {
                    resample::convert_frame(
                        &sw_frame,
                        sw_frame.ch_layout,
                        target.into(),
                        sw_frame.sample_rate,
                    )
                    .context("Failed to convert decoded audio to the output sample format")?
                }
                _ => sw_frame,
            },
            _ => {
                // do nothing
                sw_frame
            }
        };

        // 4. 应用 Filter Graph
        if let Some(chain) = self.filter_graph.as_mut() {
            // filter process
            match chain.graph.process_frame(Some(raw_frame))? {
                Some(filtered_frame) => Ok(Some(filtered_frame)),
                // `process_frame` 只在把图置为 `Drained`（还要更多输入）或
                // `Flushed`（EOF）之后才返回 `None`，因此这里没有第三种情况：
                // 返回 `None` 让外层循环继续驱动解码器，或就此收尾。
                None => {
                    log::debug!(
                        "Filter graph produced no frame (drained: {}, flushed: {})",
                        chain.graph.is_drained(),
                        chain.graph.is_flushed()
                    );
                    Ok(None)
                }
            }
        } else {
            // 如果没有 Filter Graph，直接返回 CPU 帧
            Ok(Some(raw_frame))
        }
    }

    /// Pull a decoded frame from the decoder. This function also implements retry mechanism in case
    /// the decoder signals `EAGAIN` and `EOF`
    fn decoder_receive_frame(&mut self) -> Result<Option<AVFrame>> {
        match self.context.receive_frame() {
            Ok(frame) => Ok(Some(frame)),
            Err(rsmpeg::error::RsmpegError::DecoderDrainError) => {
                // EAGAIN：此刻无帧可出。
                // - read 阶段：表示"该包暂未解出帧，需继续喂包"，此时不应置 Drained，
                //   否则会使后续 drain_raw 误判已进入 draining 而跳过 EOS 发送（见 drain_raw）。
                // - drain 阶段：Drained 已在 drain_raw 中置位，这里保持即可。
                log::debug!("Decoder drained. try send new packet again.");
                // self.state = DecoderState::Drained;
                Ok(None)
            }
            Err(rsmpeg::error::RsmpegError::DecoderFlushedError) => {
                log::debug!("Decoder flushed. EOF reached.");
                self.state = CodecContextState::Flushed;
                Ok(None)
            }
            Err(e) => {
                log::warn!("Failed to receive frame from decoder: {e}");
                Err(RsmediaError::from(e))
            }
        }
    }
}

/// 驱动“读 packet → 解码 → EOF 排空”的通用状态机，供 [`Decoder::decode`] 与
/// [`Decoder::decode_raw`] 复用。`on_packet` 处理单个输入包，`on_drain` 处理 EOF
/// 阶段的排空；二者共用同一套“忽略非目标流 / 报错即返回 / 排空到底”的语义。
fn decode_stream<O, OP, OD>(
    decoder: &mut Decoder,
    reader: &mut impl Reader,
    mut on_packet: OP,
    mut on_drain: OD,
) -> Result<Option<O>>
where
    OP: FnMut(&mut Decoder, &AVPacket) -> Result<Option<O>>,
    OD: FnMut(&mut Decoder) -> Result<Option<O>>,
{
    if decoder.is_finished() {
        return Err(RsmediaError::invalid_config(
            "Decoder cannot decode after flushed. Call reset().",
        ));
    }

    let mut read_exhausted = false;
    let mut drained_iterations = 0usize;
    Ok(loop {
        if !read_exhausted {
            match reader.read_packet() {
                Ok(Some((stream_index, packet))) => {
                    if stream_index != decoder.stream_index() {
                        // 跳过其它流
                        log::trace!("skip stream index: {}, {:?}", stream_index, packet);
                        continue;
                    }
                    if let Some(out) = on_packet(decoder, &packet)? {
                        break Some(out);
                    }
                }
                Ok(None) => {
                    log::debug!("No more packets, Reader exhausted.");
                    read_exhausted = true;
                    continue;
                }
                Err(e) => {
                    log::error!("Error reading packet: {e}");
                    return Err(e);
                }
            }
        } else {
            match on_drain(decoder) {
                Ok(Some(out)) => break Some(out),
                Ok(None) => {
                    // None 可能来自 Drained（EAGAIN，解码器仍有缓冲帧待产出）或
                    // Flushed（EOF）。若是 Drained 需继续 drain，否则会丢失尾部帧
                    // （多见于含 B 帧的码流）。
                    if decoder.is_drained() {
                        // 有上限的继续排空：解码器若一直回 EAGAIN 而从不报 EOF，
                        // 这里必须收尾，否则公开的 decode() 会永远转下去。
                        if drained_iterations >= crate::MAX_DRAIN_ITERATIONS {
                            log::error!(
                                "Decoder keeps returning EAGAIN after EOF, giving up after \
                                 {} iterations",
                                crate::MAX_DRAIN_ITERATIONS
                            );
                            break None;
                        }
                        drained_iterations += 1;
                        log::debug!("Decoder drained, keep draining.");
                        continue;
                    }
                    log::debug!("Decoder flushed. EOF reached.");
                    // self.reset();
                    // read_exhausted = false;
                    break None;
                }
                Err(e) => {
                    log::error!("Error to drain decoder: {e}");
                    return Err(e);
                }
            }
        }
    })
}

/// Important note: Do not forget to drain the decoder after the reader is exhausted. It may still
/// contain frames. Run `drain_raw()` or `drain()` in a loop until no more frames are produced.
impl Drop for Decoder {
    fn drop(&mut self) {
        // 字幕解码走同步的 avcodec_decode_subtitle2，无 send/receive 帧队列
        // 需要排空（且字幕解码器不支持 send_packet flush）
        if self.media_type == MediaType::SUBTITLE {
            return;
        }

        // 1. Flush Filter Graph if exists.
        if let Some(chain) = self.filter_graph.as_mut() {
            match chain.graph.flush() {
                Ok(frames) => {
                    if !frames.is_empty() {
                        log::warn!(
                            "{} frames dropped during Decoder drop filter flush.",
                            frames.len()
                        );
                    }
                    log::debug!("Filter graph flushed during Decoder drop.");
                }
                Err(e) => log::error!("Failed to flush filter graph during Decoder drop: {e}"),
            }
        }

        // We need to drain the items still in the decoders queue.
        match self.send_packet_to_decoder(None) {
            Ok(_) => {
                // 兜底上限见 `MAX_DRAIN_ITERATIONS`。
                let mut iterations = 0usize;
                loop {
                    if iterations >= crate::MAX_DRAIN_ITERATIONS {
                        log::warn!(
                            "Decoder drain exceeded {} iterations, forcing EOF.",
                            crate::MAX_DRAIN_ITERATIONS
                        );
                        break;
                    }
                    iterations += 1;
                    match self.decoder_receive_frame() {
                        Ok(Some(_frame)) => {
                            // If receive a frame, we continue to drain the queue.
                            log::debug!("continue draining decoder queue.");
                        }
                        Ok(None) => {
                            if self.is_drained() {
                                // If we need more, we continue to drain the queue.
                                log::debug!("Decoder draining. continue...");
                                continue;
                            } else {
                                log::debug!("Decoder flushed. EOF reached.");
                                break;
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to drain decoder: {e}");
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to send flush packet to decoder: {e}")
            }
        }
    }
}

/// SAFETY:
/// - `Decoder` 内含 `AVCodecContext` / filter graph，FFmpeg 不保证其线程安全，
///   `&Decoder` 跨线程共享（`Sync`）无法成立，故不实现 `Sync`。
/// - `Send`（move 到另一线程独占使用）是安全的：所有资源随对象移动，
///   无线程局部句柄。
unsafe impl Send for Decoder {}

/// 一站式从输入获取一帧视频缩略图，返回 `image::DynamicImage`。
///
/// 内部流程：构建视频解码器（RGB24 输出 + [`Resize::Fit`] 保持纵横比缩放）
/// → seek 到目标时间 → 解码一帧原始 `AVFrame` → 转为
/// [`image::DynamicImage`](crate::imgutils::to_dynamic_image)。
/// 不依赖 `ndarray` feature，适合生成封面图 / 视频预览等场景。
///
/// # Arguments
///
/// * `source` - 输入（文件路径 / URL 等，见 [`Location`]）
/// * `timestamp_milliseconds` - 取帧时间点；`None` 时取**流中点**
///   （视频开头往往是黑帧/淡入，中点更容易取到有代表性的画面；
///   时长未知的流退化为取第一帧）
/// * `max_dims` - 缩略图最大 (宽, 高)；实际尺寸按纵横比缩放，
///   源小于该尺寸时不放大
///
/// # Example
///
/// ```rust,no_run
/// # use rsmedia::thumbnail;
/// # use std::path::Path;
/// let img = thumbnail(Path::new("assets/mp4.mp4"), None, (320, 240)).unwrap();
/// println!("thumbnail: {}x{}", img.width(), img.height());
/// img.save("thumbnail.png").unwrap();
/// ```
pub fn thumbnail(
    source: impl Into<Location>,
    timestamp_ms: Option<i64>,
    max_dims: (u32, u32),
) -> Result<image::DynamicImage> {
    let mut reader = StreamReader::new(source).context("Failed to open thumbnail source")?;
    let mut decoder = DecoderBuilder::new(MediaType::VIDEO)
        .with_pix_fmt(PixelFormat::RGB24)
        .with_resize(Resize::Fit(max_dims.0, max_dims.1))
        .build_from_reader(&reader)
        .context("Failed to build thumbnail decoder")?;

    // None → 流中点；时长未知（0）→ 第一帧
    let ts = match timestamp_ms {
        Some(ts) => ts,
        None => {
            let info = StreamInfo::from_reader(&reader, decoder.stream_index())?;
            let mid_secs = info.duration as f64 * avutil::av_q2d(info.time_base) / 2.0;
            (mid_secs * 1000.0).round().max(0.0) as i64
        }
    };

    let frame = {
        // 定位到目标时间之前最近的关键帧，并刷新解码器以丢弃旧缓冲。
        // seek 失败不视为错误：退化为从当前位置解码第一帧。
        if reader.seek_to_timestamp(ts).is_err() {
            log::debug!("seek to {ts}ms failed, decoding from the current position");
        } else {
            decoder.flush_buffers()?;
        }
        decoder.decode_raw(&mut reader)?
    }
    .ok_or_else(|| RsmediaError::msg("No video frame decoded for thumbnail"))?;

    crate::imgutils::to_dynamic_image(&frame).context("Failed to convert AVFrame to image")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter;
    use std::collections::HashSet;

    #[test]
    fn test_thumbnail() -> Result<()> {
        let video_path = std::path::Path::new("assets/mp4.mp4");
        // 默认取流中点，Fit 缩放保持纵横比
        let img = thumbnail(video_path, None, (320, 240))?;
        assert!(img.width() > 0 && img.height() > 0);
        assert!(
            img.width() <= 320 && img.height() <= 240,
            "thumbnail dims {}x{} exceed 320x240",
            img.width(),
            img.height()
        );
        assert_eq!(img.color().channel_count(), 3, "expected RGB output");

        // 指定时间点
        let img = thumbnail(video_path, Some(1000), (64, 64))?;
        assert!(img.width() > 0 && img.height() > 0);
        Ok(())
    }

    #[test]
    fn test_decode_video() -> Result<()> {
        let video_path = std::path::Path::new("assets/mp4.mp4");

        // drawtext 依赖 libfreetype 编译进 FFmpeg，部分构建未启用：前置探测
        // 滤镜是否存在，缺失时降级为仅 scale，而非匹配错误字符串。
        let scale = filter::video::scale(1280, 720, None);
        let mut reader = StreamReader::new(video_path)?;
        let filters = if filter::is_available("drawtext") {
            let drawtext = filter::video::DrawText::new("Hello", 10, 10, 24, "white").build();
            vec![scale, drawtext]
        } else {
            println!("SKIP drawtext (libfreetype not available)");
            vec![scale]
        };
        let build_decoder = |filters: Vec<Filter>| -> Result<Decoder> {
            DecoderBuilder::new(MediaType::VIDEO)
                .with_filters(filters)
                .build_from_reader(&reader)
        };
        let mut decoder = build_decoder(filters)?;

        loop {
            match decoder.decode_raw(&mut reader) {
                Ok(Some(frame)) => {
                    println!("video frame: {:?}, timebase:{:?}", frame, frame.time_base);
                }
                Ok(None) => {
                    println!("No more frames, decoder flushed");
                    break;
                }
                Err(e) => {
                    log::error!("Error decoding frame: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_decode_audio() -> Result<()> {
        let audio_path = std::path::Path::new("assets/wav.wav");

        let filters = vec![
            filter::audio::resample(2, 48000, SampleFormat::FLTP),
            filter::audio::volume(1.5),
        ];

        let mut reader = StreamReader::new(audio_path)?;
        let mut decoder = DecoderBuilder::new(MediaType::AUDIO)
            .with_filters(filters)
            .build_from_reader(&reader)?;

        loop {
            match decoder.decode_raw(&mut reader) {
                Ok(Some(frame)) => {
                    println!("audio frame: {:?}, timebase:{:?}", frame, frame.time_base);
                }
                Ok(None) => {
                    println!("No more frames, decoder flushed");
                    break;
                }
                Err(e) => {
                    log::error!("Error decoding frame: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// `with_sample_fmt` 统一输出：无论编解码器原生格式是什么，解码帧都转换到
    /// 目标格式，`decode::<T>` 的元素类型因此不必随源文件而变。
    /// `assets/wav.wav` 的帧还带 `AV_CHANNEL_ORDER_UNSPEC` 布局（WAV 无声道
    /// 掩码），顺带回归重采样器的 `AVERROR_INPUT/OUTPUT_CHANGED`。
    #[test]
    fn test_decode_audio_with_sample_fmt_unifies_output() -> Result<()> {
        let audio_path = std::path::Path::new("assets/wav.wav");

        // 统一到 FLTP：decode::<f32> 全程可用，且每帧的格式都是目标格式。
        let mut reader = StreamReader::new(audio_path)?;
        let mut decoder = DecoderBuilder::new(MediaType::AUDIO)
            .with_sample_fmt(SampleFormat::FLTP)
            .build_from_reader(&reader)?;
        let mut unified_samples = 0u64;
        while let Some(frame) = decoder.decode::<f32>(&mut reader)? {
            assert_eq!(
                frame.format().and_then(|f| f.into_sample()),
                Some(SampleFormat::FLTP),
                "decoded frame was not converted to the requested sample format"
            );
            unified_samples += frame.nb_samples as u64;
        }
        assert!(unified_samples > 0, "no audio decoded");

        // 原生格式（默认）：pcm_s16le 解出 S16，样本总量一致——只换格式不变样本数。
        let mut reader = StreamReader::new(audio_path)?;
        let native_format = reader
            .input()
            .streams()
            .iter()
            .find(|stream| stream.codecpar().codec_type().is_audio())
            .map(|stream| SampleFormat::from(stream.codecpar().format))
            .expect("audio stream");
        let mut decoder = DecoderBuilder::new(MediaType::AUDIO).build_from_reader(&reader)?;
        let mut native_samples = 0u64;
        while let Some(frame) = decoder.decode::<i16>(&mut reader)? {
            native_samples += frame.nb_samples as u64;
        }
        assert_eq!(native_format, SampleFormat::S16);
        assert_eq!(
            native_samples, unified_samples,
            "format conversion changed the decoded sample count"
        );

        Ok(())
    }

    /// `with_sample_fmt` 只对音频解码器有效，`NONE` 不是可用目标：构建时快速失败。
    #[test]
    fn test_decode_builder_sample_fmt_validation() -> Result<()> {
        let reader = StreamReader::new("assets/mp4.mp4")?;
        let video = DecoderBuilder::new(MediaType::VIDEO)
            .with_sample_fmt(SampleFormat::FLTP)
            .build_from_reader(&reader);
        assert!(video.is_err(), "with_sample_fmt must be rejected for video");

        let reader = StreamReader::new("assets/wav.wav")?;
        let none = DecoderBuilder::new(MediaType::AUDIO)
            .with_sample_fmt(SampleFormat::NONE)
            .build_from_reader(&reader);
        assert!(none.is_err(), "SampleFormat::NONE is not a valid target");

        Ok(())
    }

    #[test]
    fn test_decode_video_with_resize() -> Result<()> {
        use crate::Resize;

        let video_path = std::path::Path::new("assets/mp4.mp4");

        let mut reader = StreamReader::new(video_path)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO)
            .with_resize(Resize::Exact(320, 240))
            .build_from_reader(&reader)?;

        let mut frames = 0usize;
        while let Some(frame) = decoder.decode_raw(&mut reader)? {
            assert_eq!(frame.width, 320);
            assert_eq!(frame.height, 240);
            frames += 1;
        }
        assert!(frames > 0, "expected at least one decoded frame");

        Ok(())
    }

    /// 验证 `with_pix_fmt(RGB24)`：解码输出应为 RGB24；同时 MediaFrame 路径
    /// 可正常转换。
    #[test]
    fn test_decode_video_with_pix_fmt_rgb24() -> Result<()> {
        let video_path = std::path::Path::new("assets/mp4.mp4");

        let mut reader = StreamReader::new(video_path)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO)
            .with_pix_fmt(PixelFormat::RGB24)
            .build_from_reader(&reader)?;

        let mut frames = 0usize;
        while let Some(frame) = decoder.decode_raw(&mut reader)? {
            assert_eq!(frame.format, i32::from(PixelFormat::RGB24));
            frames += 1;
        }
        assert!(frames > 0, "expected at least one decoded frame");

        Ok(())
    }

    /// `with_pix_fmt` 只拒绝无法表示为数据平面的格式（位流 / 调色板 / 硬件），
    /// 且在构建时返回错误而非 panic。
    #[test]
    fn test_decode_video_with_pix_fmt_unsupported() {
        let video_path = std::path::Path::new("assets/mp4.mp4");
        for fmt in [
            PixelFormat::MONOWHITE, // 位流：分量不足一字节
            PixelFormat::PAL8,      // 调色板格式：样本指向独立调色板
            PixelFormat::VAAPI,     // 硬件格式：没有主机端样本
        ] {
            let reader = StreamReader::new(video_path).unwrap();
            let result = DecoderBuilder::new(MediaType::VIDEO)
                .with_pix_fmt(fmt)
                .build_from_reader(&reader);
            assert!(result.is_err(), "{fmt:?} should be rejected");
        }
    }

    /// 平面 / 半平面格式现在同样可以作为解码输出（每平面一个数组），
    /// 输出格式不再是「YUV420P + packed 8bit」白名单。
    #[test]
    fn test_decode_video_with_planar_pix_fmt_accepted() {
        let video_path = std::path::Path::new("assets/mp4.mp4");
        for fmt in [PixelFormat::NV12, PixelFormat::YUV422P, PixelFormat::GBRP] {
            let reader = StreamReader::new(video_path).unwrap();
            let result = DecoderBuilder::new(MediaType::VIDEO)
                .with_pix_fmt(fmt)
                .build_from_reader(&reader);
            assert!(result.is_ok(), "{fmt:?} should be accepted");
        }
    }

    /// `with_pix_fmt` 对音频解码器应快速失败，而非静默忽略。
    #[test]
    fn test_decode_audio_with_pix_fmt_fails() {
        let video_path = std::path::Path::new("assets/mp4.mp4");
        let reader = StreamReader::new(video_path).unwrap();
        let result = DecoderBuilder::new(MediaType::AUDIO)
            .with_pix_fmt(PixelFormat::YUV420P)
            .build_from_reader(&reader);
        assert!(result.is_err());
    }

    /// 验证 `with_resize` 与 `scale` filter 两种缩放方式结果一致，且同时使用时
    /// 按「先 resize 后 filter」的顺序叠加，不冲突。
    #[test]
    fn test_resize_vs_filter_scale() -> Result<()> {
        use crate::Resize;

        let video_path = std::path::Path::new("assets/mp4.mp4");

        // A) 仅 with_resize
        eprintln!("[A] with_resize only");
        let mut reader_a = StreamReader::new(video_path)?;
        let mut dec_a = DecoderBuilder::new(MediaType::VIDEO)
            .with_resize(Resize::Exact(320, 240))
            .build_from_reader(&reader_a)?;
        let mut a_dims = HashSet::new();
        while let Some(f) = dec_a.decode_raw(&mut reader_a)? {
            a_dims.insert((f.width, f.height));
        }

        // B) 仅 scale filter
        eprintln!("[B] filter only");
        let mut reader_b = StreamReader::new(video_path)?;
        let mut dec_b = DecoderBuilder::new(MediaType::VIDEO)
            .with_filters(vec![filter::video::scale(320, 240, None)])
            .build_from_reader(&reader_b)?;
        let mut b_dims = HashSet::new();
        while let Some(f) = dec_b.decode_raw(&mut reader_b)? {
            b_dims.insert((f.width, f.height));
        }

        // A 与 B 应得到完全相同的尺寸集合
        assert_eq!(
            a_dims, b_dims,
            "with_resize and filter scale produced different dimensions"
        );
        assert_eq!(a_dims.len(), 1, "expected a single uniform output size");

        // C) 同时使用：resize(320x240) -> filter scale(640x480)，输出应为 filter 尺寸
        eprintln!("[C] resize + filter");
        let mut reader_c = StreamReader::new(video_path)?;
        let mut dec_c = DecoderBuilder::new(MediaType::VIDEO)
            .with_resize(Resize::Exact(320, 240))
            .with_filters(vec![filter::video::scale(640, 480, None)])
            .build_from_reader(&reader_c)?;
        let mut c_dims = HashSet::new();
        while let Some(f) = dec_c.decode_raw(&mut reader_c)? {
            c_dims.insert((f.width, f.height));
        }
        assert_eq!(
            c_dims,
            HashSet::from([(640i32, 480i32)]),
            "resize+filter should compose to the filter size"
        );

        Ok(())
    }

    /// builder 的 `with_scale_pool` 必须进入解码器持有的 [`Scaler`]：
    /// 默认关闭，显式开启后为真（真实 build 路径）。
    #[test]
    fn test_builder_scale_pool_reaches_decoder_scaler() -> Result<()> {
        let video_path = std::path::Path::new("assets/mp4.mp4");

        let decoder = DecoderBuilder::new(MediaType::VIDEO)
            .build_from_reader(&StreamReader::new(video_path)?)?;
        assert!(!decoder.scaler.pool_enabled(), "池化默认关闭");

        let decoder = DecoderBuilder::new(MediaType::VIDEO)
            .with_scale_pool(true)
            .build_from_reader(&StreamReader::new(video_path)?)?;
        assert!(
            decoder.scaler.pool_enabled(),
            "with_scale_pool 应进入 Scaler"
        );
        Ok(())
    }

    /// 解码到尾：`is_flushed` 只说解码器自己到 EOF，`is_finished` 才是整条流水线
    /// （含滤镜图）结束。此后继续解码必须报错，而不是静默返回 `None`。
    #[test]
    fn test_decode_reaches_finished_state() -> Result<()> {
        let video_path = std::path::Path::new("assets/mp4.mp4");
        let mut reader = StreamReader::new(video_path)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;

        let mut decoded = 0usize;
        while decoder.decode::<u8>(&mut reader)?.is_some() {
            decoded += 1;
            assert!(
                !decoder.is_finished(),
                "reported finished after {decoded} frames, with more still arriving"
            );
        }

        assert!(decoded > 0, "decoded nothing from {}", video_path.display());
        assert!(decoder.is_flushed(), "EOF must leave the decoder flushed");
        assert!(
            decoder.is_finished(),
            "decoder and its (absent) filter graph must both be finished at EOF"
        );
        assert!(
            decoder.decode::<u8>(&mut reader).is_err(),
            "decoding after the end must error, not silently return None"
        );

        // `reset` 之后可以重新解码（用于 seek 后的复用）。
        decoder.reset()?;
        assert!(!decoder.is_flushed());
        assert!(!decoder.is_finished());
        Ok(())
    }

    /// 带滤镜图的解码器读到 EOF 之后，`reset()` 必须能把**整条流水线**（解码器
    /// + 滤镜图）一起复位。
    ///
    /// 只复位解码器是不够的：滤镜图没有"回退"，一旦见过 EOF 就永久停在 EOF，
    /// 之后每一帧提交都会得到 `AVERROR_EOF` —— 而 `is_finished()` 却会报告
    /// "可以继续"，正是"状态在说谎"。
    #[test]
    fn test_reset_restarts_the_filtered_pipeline() -> Result<()> {
        let path = std::path::Path::new("assets/mp4.mp4");

        let mut reader = StreamReader::new(path)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO)
            .with_filters(vec![crate::filter::video::fps(10.0)])
            .build_from_reader(&reader)?;

        let mut first_pass = 0usize;
        while decoder.decode::<u8>(&mut reader)?.is_some() {
            first_pass += 1;
        }
        assert!(first_pass > 0, "the filtered decode produced nothing");
        assert!(decoder.is_finished(), "EOF must finish the whole pipeline");

        decoder.reset()?;
        assert!(!decoder.is_flushed() && !decoder.is_finished());

        // 重新读同一个文件：必须能正常出帧，而不是 AVERROR_EOF。
        let mut reader = StreamReader::new(path)?;
        let mut second_pass = 0usize;
        while decoder.decode::<u8>(&mut reader)?.is_some() {
            second_pass += 1;
        }
        assert_eq!(
            second_pass, first_pass,
            "a restarted pipeline must decode the same stream again"
        );
        Ok(())
    }

    /// `flush_buffers()`（seek 后使用）必须把**滤镜图里**的缓冲帧一起丢掉。
    ///
    /// `avcodec_flush_buffers` 只管编解码器；带缓冲的滤镜（如 `fps`）会把 seek
    /// 之前的帧留在图里，下一次解码就会把它们当新帧吐出来 —— 于是"seek 到 3s"
    /// 之后拿到的是 0.5s 的画面。这里用一个 GOP 已知的自造文件验证。
    ///
    /// 断言的是**不变量**而不是精确落点：默认的 BACKWARD seek 落在**目标位置或其
    /// 之前的最后一个关键帧**上，所以落点在 `[目标 - 一个 GOP, 目标]` 之间，具体
    /// 取决于容器时间基的取整（实测 seek 2s 在 macOS 上落在 2.0s，在 Linux/ffmpeg 7.1
    /// 上落在 1.6s —— 两者都合法）。而泄漏帧来自 seek 之前读到的位置（约 0.5s），
    /// 与合法区间相隔一个 GOP 以上，所以下面用"目标减一个 GOP"当界限即可区分两者。
    #[test]
    fn test_flush_buffers_drops_frames_buffered_by_the_filter_graph() -> Result<()> {
        const FPS: f64 = 25.0;
        const FILTER_FPS: f64 = 10.0;
        const FRAMES: i64 = 100;
        const GOP_FRAMES: i32 = 10;
        // 4s 的素材、seek 到 3s：seek 前的读取位置（~0.2s）与目标相隔很远，
        // 泄漏帧与合法落点因此有很宽的间隔。
        const SEEK_MS: i64 = 3_000;
        let path = crate::test_support::test_output_path("decode", "test_flush_filter.mp4");

        {
            let mut muxer = crate::Muxer::new(&path)?;
            let encoder = crate::EncoderBuilder::new_video(64, 64)
                .with_fps(FPS as f32)
                .with_gop_size(GOP_FRAMES)
                .build()?;
            let index = muxer.add_encoder(encoder)?;
            for frame_index in 0..FRAMES {
                let mut frame = AVFrame::new();
                frame.set_width(64);
                frame.set_height(64);
                frame.set_format(i32::from(PixelFormat::YUV420P));
                frame
                    .alloc_buffer()
                    .context("Failed to allocate frame buffer")?;
                frame.set_pts(frame_index);
                muxer.mux(frame, index)?;
            }
            muxer.finish()?;
        }

        let mut reader = StreamReader::new(&path)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO)
            .with_filters(vec![crate::filter::video::fps(FILTER_FPS as f32)])
            .build_from_reader(&reader)?;

        // 从头解几帧，让滤镜图里真正开始有缓冲
        let mut decoded_before_seek = 0i64;
        for _ in 0..5 {
            if decoder.decode_raw(&mut reader)?.is_some() {
                decoded_before_seek += 1;
            }
        }
        assert!(
            decoded_before_seek > 0,
            "the filter graph must have produced frames before the seek for this test to mean anything"
        );

        reader.seek_to_timestamp(SEEK_MS)?;
        decoder.flush_buffers()?;

        let first = decoder
            .decode_raw(&mut reader)?
            .ok_or_else(|| RsmediaError::msg("no frame decoded after the seek"))?;

        // 滤镜图输出帧的 pts 以 `1/fps` 为时间基（解码器不填 AVFrame.time_base）。
        // 落点最多比目标早一个 GOP；再放宽一帧输出网格的取整。泄漏帧（seek 前的位置，
        // 约 0.5s → pts≈5）远在界限之下，所以这个界限仍能抓住回归。
        let seek_secs = SEEK_MS as f64 / 1000.0;
        let gop_secs = f64::from(GOP_FRAMES) / FPS;
        let earliest_pts = ((seek_secs - gop_secs) * FILTER_FPS) as i64 - 1;
        assert!(
            first.pts >= earliest_pts,
            "the first frame after seeking to {seek_secs}s is at graph pts {} \
             (a legitimate BACKWARD seek lands between {} and {}, one GOP early at worst): \
             the filter graph is still holding pre-seek frames",
            first.pts,
            earliest_pts,
            (seek_secs * FILTER_FPS) as i64
        );

        crate::test_support::remove_test_output(&path);
        Ok(())
    }

    /// flush 之后再走低层入口（`decode_raw_packet`）也要拿到同一个清晰的错误。
    #[test]
    fn test_decode_raw_packet_rejects_a_finished_decoder() -> Result<()> {
        let path = std::path::Path::new("assets/mp4.mp4");
        let mut reader = StreamReader::new(path)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
        while decoder.decode_raw(&mut reader)?.is_some() {}
        assert!(decoder.is_finished());

        let mut source = StreamReader::new(path)?;
        if let Some((_index, packet)) = source.read_packet()? {
            let err = match decoder.decode_raw_packet(&packet) {
                Ok(_) => panic!("a finished decoder must reject a packet"),
                Err(e) => e,
            };
            assert!(err.is_invalid_config(), "{err}");
            assert!(err.to_string().contains("reset()"), "{err}");
        }
        Ok(())
    }
}
