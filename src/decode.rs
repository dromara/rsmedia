use crate::codec::{AVCodecFlag, CodecContextState};
use crate::error::{Context, Result, RsmediaError};
use crate::filter::{AudioParams, Filter, FilterGraph, FilterParams, VideoParams};
#[cfg(feature = "ndarray")]
use crate::frame::{MediaFrame, MediaFrameType};
use crate::hwaccel::{HWContext, HWDeviceConfig};
use crate::io::{Reader, Seekable};
use crate::options::Options;
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
    resize: Option<Resize>,
    /// 解码输出目标像素格式（仅视频），默认 [`PixelFormat::YUV420P`]。
    pix_fmt: Option<PixelFormat>,
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
            resize: None,
            pix_fmt: None,
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
    /// 默认 [`PixelFormat::YUV420P`]。支持：
    /// - [`PixelFormat::YUV420P`]（专用分支，U/V 以 2x2 块代表值存入 `[H, W, 3]`）
    /// - 全部 packed 8bit 格式（见 [`PixelFormat::packed_channels`]）：
    ///   GRAY8 `[H,W,1]` / YUYV422、UYVY422 `[H,W,2]` / RGB24、BGR24 `[H,W,3]` /
    ///   RGBA、BGRA、ARGB、ABGR `[H,W,4]`（无损往返）
    ///
    /// 源格式与目标不一致时由 swscale 自动转换（如 NV12 → RGBA）。
    /// 其他格式请用 [`decode_raw`](Decoder::decode_raw) 获取原始 `AVFrame`，
    /// 或通过滤镜 `format` 转换。
    ///
    /// 仅对视频解码器有效；其他媒体类型构建时返回错误（fail-fast）。
    ///
    /// 注意：[`PixelFormat::YUV420P`] 要求输出宽高为偶数（色度平面下采样），
    /// 建议搭配 [`Resize::FitEven`] 保证尺寸约束。
    pub fn with_pix_fmt(mut self, pix_fmt: PixelFormat) -> Self {
        self.pix_fmt = Some(pix_fmt);
        self
    }

    fn setup_codec_context(&self, decoder: &mut AVCodecContext, input: &AVStream) -> Result<()> {
        let media_type = self.media_type;
        if media_type as ffi::AVMediaType != decoder.codec_type {
            return Err(RsmediaError::custom(format!(
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
        let input_stream =
            reader
                .input()
                .streams()
                .get(stream_index)
                .ok_or(RsmediaError::custom(format!(
                    "stream: {stream_index} not found!"
                )))?;

        let codec = {
            let codec_name = if let Some(ref codec_name) = self.codec_name {
                codec_name.as_str()
            } else {
                codec_name.as_str()
            };
            AVCodec::find_decoder_by_name(&strutils::str_to_cstring(codec_name))
                .context(format!("Failed to find decoder by name: '{codec_name}'"))?
        };

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
                        RsmediaError::custom(format!(
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

        // 输出像素格式：仅视频有效。支持 YUV420P 专用分支 + 全部 packed 8bit
        // 格式（GRAY8/RGB24/BGR24/RGBA/BGRA/ARGB/ABGR，见
        // `PixelFormat::packed_channels`）；解码输出经 swscale 统一转换到目标格式。
        // 非视频类型配置了 pix_fmt 视为调用方错误，快速失败而非静默忽略。
        let output_pix_fmt = match (media_type, self.pix_fmt) {
            (MediaType::VIDEO, Some(fmt)) => {
                if fmt != PixelFormat::YUV420P && fmt.packed_channels().is_none() {
                    return Err(RsmediaError::custom(format!(
                        "Unsupported output pixel format: {fmt:?}, only YUV420P and packed 8-bit \
                         formats (GRAY8/YUYV422/UYVY422/RGB24/BGR24/RGBA/BGRA/ARGB/ABGR) are \
                         supported"
                    )));
                }
                fmt
            }
            (MediaType::VIDEO, None) => PixelFormat::YUV420P,
            (media_type, Some(fmt)) => {
                return Err(RsmediaError::custom(format!(
                    "with_pix_fmt({fmt:?}) is only valid for video decoders, got media type: {media_type:?}"
                )));
            }
            (_, None) => PixelFormat::YUV420P,
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
                    format: SampleFormat::from(decode_ctx.sample_fmt),
                    src_format: SampleFormat::from(decode_ctx.sample_fmt),
                    time_base: decode_ctx.time_base,
                }),
                _ => {
                    return Err(RsmediaError::custom(format!(
                        "Unsupported filter for media type: {media_type:?}"
                    )));
                }
            };

            let mut graph = FilterGraph::new();
            // 验证 Filter 链的媒体类型是否与当前流匹配
            if !filters.iter().all(|f| f.media_type() == media_type) {
                return Err(RsmediaError::custom(format!(
                    "Filter media type mismatch for stream type {media_type:?}"
                )));
            }
            graph
                .init(&filter_params, filters.as_slice())
                .context("Failed to initialize filter graph")?;

            Some(graph)
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
            scaler: Scaler::new_with_options(self.scale_algorithm, self.scale_quality),
            resize: self.resize,
            output_pix_fmt,
        })
    }
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
    filter_graph: Option<FilterGraph>,
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

    #[inline]
    pub fn pix_fmt(&self) -> PixelFormat {
        self.context.pix_fmt.into()
    }

    #[inline]
    pub fn sample_rate(&self) -> i32 {
        self.context.sample_rate
    }

    #[inline]
    pub fn sample_fmt(&self) -> SampleFormat {
        SampleFormat::from(self.context.sample_fmt)
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

    /// Get the decoders input stream number of frames
    #[inline(always)]
    pub fn frames(&self) -> i64 {
        self.nb_frames
    }

    /// Get the decoders input frame rate
    ///
    /// # Return
    /// A tuple of the frame rate of float values
    ///
    /// `0`: r_frame_rate
    /// `1`: avg_frame_rate
    ///
    #[inline(always)]
    pub fn frame_rate(&self) -> (f32, f32) {
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

    pub fn is_flushed(&self) -> bool {
        self.state == CodecContextState::Flushed
    }

    /// 解码器是否已完全结束：解码器到达 EOF，且 filter（如有）内部缓冲帧也已全部
    /// 冲刷完毕。仅当二者都满足时，才禁止继续调用 `decode`/`decode_raw`。否则
    /// （解码器已 Flushed 但 filter 仍有多余缓冲帧待冲刷，如延迟滤镜 `framerate`），
    /// 仍需允许继续调用以取回剩余帧，否则会丢帧或报"cannot decode after flushed"。
    fn is_complete(&self) -> bool {
        self.is_flushed()
            && match &self.filter_graph {
                Some(graph) => graph.is_flushed(),
                None => true,
            }
    }

    /// Decode a single frame.
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
        T: MediaFrameType,
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
    /// (8-bit formats such as YUV420P/RGB24). For audio the sample type must
    /// match the codec's native sample format size — use `decode::<f32>()`
    /// for FLTP/FLT output or [`decode_raw`](Self::decode_raw) to avoid the
    /// typed conversion entirely.
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
            return Err(RsmediaError::custom(format!(
                "decode_subtitle_segment requires a subtitle decoder, got media type: {:?}",
                self.media_type
            )));
        }
        if self.is_flushed() {
            return Err(RsmediaError::custom(
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

    /// Decode a [`Packet`].
    ///
    /// Feeds the packet to the decoder and returns a frame if there is one available. The caller
    /// should keep feeding packets until the decoder returns a frame.
    ///
    /// # Return value
    ///
    /// A tuple of the [`Frame`] and timestamp (relative to the stream) and the frame itself if the
    /// decoder has a frame available, [`None`] if not.
    #[cfg(feature = "ndarray")]
    pub fn decode_packet<T>(&mut self, packet: &AVPacket) -> Result<Option<MediaFrame<T>>>
    where
        T: MediaFrameType,
    {
        match self.decode_raw_packet(packet) {
            Ok(Some(raw_frame)) => Ok(Some(self.raw_frame_to_media_frame(raw_frame)?)),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Decode a [`Packet`].
    ///
    /// Feeds the packet to the decoder and returns a frame if there is one available. The caller
    /// should keep feeding packets until the decoder returns a frame.
    ///
    /// # Panics
    ///
    /// Panics if in draining mode.
    ///
    /// # Return value
    ///
    /// The decoded raw frame as [`AVFrame`] if the decoder has a frame available, [`None`] if not.
    pub fn decode_raw_packet(&mut self, packet: &AVPacket) -> Result<Option<AVFrame>> {
        self.send_packet_to_decoder(Some(packet))?;
        self.receive_frame_from_decoder()
    }

    /// Drain one frame from the decoder.
    ///
    /// After calling drain once the decoder is in draining mode and the caller may not use normal
    /// decode anymore, or it will panic.
    ///
    /// # Return value
    ///
    /// A tuple of the [`Frame`] and timestamp (relative to the stream) and the frame itself if the
    /// decoder has a frame available, [`None`] if not.
    #[cfg(feature = "ndarray")]
    pub fn drain<T>(&mut self) -> Result<Option<MediaFrame<T>>>
    where
        T: MediaFrameType,
    {
        match self.drain_raw() {
            Ok(Some(raw_frame)) => Ok(Some(self.raw_frame_to_media_frame(raw_frame)?)),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    #[cfg(feature = "ndarray")]
    fn raw_frame_to_media_frame<T>(&self, frame: AVFrame) -> Result<MediaFrame<T>>
    where
        T: MediaFrameType,
    {
        // Video Frame: YUV420P 专用分支 + packed 8bit 格式（GRAY8/YUYV422/UYVY422/RGB24/BGR24/RGBA/BGRA/ARGB/ABGR）
        MediaFrame::<T>::from_avframe(&frame)
    }

    /// Drain one frame from the decoder.
    ///
    /// After calling drain once the decoder is in draining mode and the caller may not use normal
    /// decode anymore, or it will panic.
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
        self.receive_frame_from_decoder()
    }

    /// Reset the decoder to be used again after draining.
    pub fn reset(&mut self) {
        self.flush();
        self.state = CodecContextState::Normal;
    }

    /// Flush the decoder's internal decoding buffers.
    ///
    /// Called after a seek so the decoder discards stale buffered frames and
    /// starts cleanly from the newly positioned point.
    pub fn flush(&mut self) {
        unsafe {
            ffi::avcodec_flush_buffers(self.context.as_mut_ptr());
        }
    }

    /// Send packet to decoder.
    /// Ensure rescaling timestamps accordingly before sending to decoder.
    fn send_packet_to_decoder(&mut self, packet: Option<&AVPacket>) -> Result<()> {
        self.context
            .send_packet(packet)
            .context("Failed to send packet to decoder")?;
        Ok(())
    }

    /// Receive packet from decoder. Will handle hwaccel conversions and scaling as well.
    fn receive_frame_from_decoder(&mut self) -> Result<Option<AVFrame>> {
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
                        if let Some(graph) = self.filter_graph.as_mut()
                            && !graph.is_flushed()
                        {
                            match graph.process_frame(None)? {
                                Some(frame) => return Ok(Some(frame)),
                                None => {
                                    // 已无更多缓冲帧（graph 此时已 Flushed）
                                    debug_assert!(graph.is_flushed());
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
                            RsmediaError::custom(format!(
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
            _ => {
                // do nothing
                sw_frame
            }
        };

        // 4. 应用 Filter Graph
        if let Some(graph) = self.filter_graph.as_mut() {
            // filter process
            match graph.process_frame(Some(raw_frame))? {
                Some(filtered_frame) => Ok(Some(filtered_frame)),
                None => {
                    if graph.is_drained() {
                        // Filter graph 当前输入帧未能产生输出帧，需要继续尝试拉取
                        log::debug!("Filter graph drained, trying again.");
                        // 在这种情况下，我们应该返回 Ok(None)，让外层循环继续驱动解码器 或 filter graph
                    } else if graph.is_flushed() {
                        // Filter graph 当前输入帧未能产生输出帧，已经到达 EOF
                        log::error!("Filter graph flushed. EOF reached, should not happened.");
                    } else {
                        log::warn!("Filter graph did not output a frame.");
                    }
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
    if decoder.is_complete() {
        return Err(RsmediaError::custom(
            "Decoder cannot decode after flushed. Call reset().",
        ));
    }

    let mut read_exhausted = false;
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
        if let Some(graph) = self.filter_graph.as_mut() {
            match graph.flush() {
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
                // 兜底上限：个别解码器可能持续返回 EAGAIN 而迟迟不结束，
                // 与 encode.rs 的 1_000 保护一致，防止 Drop 排空无限循环。
                const MAX_DRAIN_ITERATIONS: usize = 1_000;
                let mut iterations = 0usize;
                loop {
                    if iterations >= MAX_DRAIN_ITERATIONS {
                        log::warn!(
                            "Decoder drain exceeded {MAX_DRAIN_ITERATIONS} iterations, forcing EOF."
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
/// [`image::DynamicImage`](imgutils::to_dynamic_image)。
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
            decoder.flush();
        }
        decoder.decode_raw(&mut reader)?
    }
    .ok_or_else(|| RsmediaError::custom("No video frame decoded for thumbnail"))?;

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

        // drawtext 依赖 libfreetype 编译进 FFmpeg，部分构建未启用，失败时降级为仅 scale。
        let scale = filter::video::scale(1280, 720, None);
        let drawtext = filter::video::DrawText::new("Hello", 10, 10, 24, "white").build();
        let mut reader = StreamReader::new(video_path)?;
        let build_decoder = |filters: Vec<Filter>| -> Result<Decoder> {
            DecoderBuilder::new(MediaType::VIDEO)
                .with_filters(filters)
                .build_from_reader(&reader)
        };
        let mut decoder = match build_decoder(vec![scale, drawtext]) {
            Ok(d) => d,
            Err(e)
                if format!("{e:#}").to_lowercase().contains("no such filter")
                    || format!("{e:#}").to_lowercase().contains("not found") =>
            {
                println!("SKIP drawtext (libfreetype not available): {e:#}");
                build_decoder(vec![filter::video::scale(1280, 720, None)])?
            }
            Err(e) => return Err(e),
        };

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

    /// `with_pix_fmt` 不支持的格式应在构建时返回错误而非 panic。
    #[test]
    fn test_decode_video_with_pix_fmt_unsupported() {
        let video_path = std::path::Path::new("assets/mp4.mp4");
        let reader = StreamReader::new(video_path).unwrap();
        let result = DecoderBuilder::new(MediaType::VIDEO)
            .with_pix_fmt(PixelFormat::NV12)
            .build_from_reader(&reader);
        assert!(result.is_err());
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
}
