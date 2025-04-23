use crate::filter::{AudioParams, Filter, FilterGraph, FilterParams, VideoParams};
use crate::flags::AvCodecFlags;
#[cfg(feature = "ndarray")]
use crate::frame::{MediaFrame, MediaFrameType};
use crate::hwaccel::{HWContext, HWDeviceConfig};
use crate::io::Reader;
use crate::options::Options;
use crate::stream::StreamInfo;
use crate::{utils, Location, MediaType, PixelFormat, SampleFormat, StreamReader, Time};

use rsmpeg::avcodec::{AVCodec, AVCodecContext, AVPacket};
use rsmpeg::avformat::AVStream;
use rsmpeg::avutil::{self, AVFrame};
use rsmpeg::ffi;

use anyhow::{Context, Error, Result};
use std::sync::Arc;

/// Builds a [`Decoder`].
#[derive(Debug)]
pub struct DecoderBuilder {
    flags: AvCodecFlags,
    thread_count: usize,
    media_type: MediaType,
    codec_name: Option<String>,
    codec_opts: Option<Options>,
    filters: Option<Vec<Filter>>,
    hw_device_config: Option<HWDeviceConfig>,
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
            flags: AvCodecFlags::LOW_DELAY,
        }
    }

    /// Set decoding flags.
    pub fn with_flags(mut self, flags: AvCodecFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Set the codec name to use for decoding.
    /// If not set, the decoder will try to guess the codec based on the input.
    pub fn with_codec_name(mut self, codec_name: Option<String>) -> Self {
        self.codec_name = codec_name;
        self
    }

    /// codec options to use for decoding.
    pub fn with_options(mut self, options: Option<Options>) -> Self {
        self.codec_opts = options;
        self
    }

    /// set the thread count.
    pub fn with_thread_count(mut self, thread_count: usize) -> Self {
        self.thread_count = thread_count;
        self
    }

    /// set the filters to apply to decoded frames.
    pub fn with_filters(mut self, filters: Option<Vec<Filter>>) -> Self {
        self.filters = filters;
        self
    }

    /// Enable hardware acceleration with the specified device type.
    ///
    /// * `device_config` - Device to use for hardware acceleration.
    pub fn with_hardware_device(mut self, device_config: Option<HWDeviceConfig>) -> Self {
        self.hw_device_config = device_config;
        self
    }

    fn setup_codec_context(&self, decoder: &mut AVCodecContext, input: &AVStream) -> Result<()> {
        let media_type = self.media_type;
        if media_type as ffi::AVMediaType != decoder.codec_type {
            return Err(Error::msg(format!(
                "Decoder codec type not supported: {:?} vs. {:?}",
                media_type, decoder.codec_type
            )));
        }

        decoder.apply_codecpar(&input.codecpar())?;
        decoder.set_flags(self.flags as i32);
        decoder.set_time_base(input.time_base);
        if let Some(framerate) = input.guess_framerate() {
            decoder.set_framerate(framerate);
        }

        unsafe {
            (*decoder.as_mut_ptr()).thread_count = self.thread_count as i32;
        }

        Ok(())
    }

    /// Build [`Decoder`].
    pub fn build(self, source: impl Into<Location>) -> Result<Decoder> {
        let reader = StreamReader::new(source)?;
        self.build_from_reader(&reader)
    }

    /// 创建一个包装的解码器
    pub fn build_wrapped(
        self,
        source: impl Into<Location>,
    ) -> Result<DecoderWrapper<StreamReader>> {
        let reader = StreamReader::new(source)?;
        self.build_wrapped_with_reader(reader)
    }

    /// a reader be required to get input stream, and build a decoder.
    pub fn build_wrapped_with_reader<R: Reader>(self, reader: R) -> Result<DecoderWrapper<R>> {
        let decoder = self.build_from_reader(&reader)?;
        Ok(DecoderWrapper::new(decoder, reader))
    }

    /// a reader be required to get input stream, and build a decoder.
    pub fn build_from_reader<R: Reader>(self, reader: &R) -> Result<Decoder> {
        let media_type = self.media_type;
        let (stream_index, codec_name) = reader.find_best_stream(media_type)?;
        let input_stream = reader
            .input()
            .streams()
            .get(stream_index)
            .ok_or(Error::msg(format!("stream: {} not found!", stream_index)))?;

        let codec = {
            let codec_name = if let Some(ref codec_name) = self.codec_name {
                codec_name.as_str()
            } else {
                codec_name.as_str()
            };
            AVCodec::find_decoder_by_name(&utils::from_str(codec_name)).context(format!(
                "Failed to find decoder by codec name: '{}'",
                codec_name
            ))?
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
        let mut init_pix_fmt = PixelFormat::from(decode_ctx.pix_fmt);
        // audio
        let init_sample_rate = decode_ctx.sample_rate;
        let init_time_base = decode_ctx.time_base;
        let init_ch_layout = decode_ctx.ch_layout;
        let init_sample_fmt = SampleFormat::from(decode_ctx.sample_fmt);

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
                        let codec_name = utils::to_string(codec.name()).unwrap();
                        Error::msg(format!(
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
                        // *注意*：setup_hw_frames 可能会改变 decode_ctx.pix_fmt
                        ctx.setup_hw_frames(true, &mut decode_ctx, init_width, init_height)?;
                        // *重要*: 更新 filter 输入参数中的 pix_fmt (因为 HW 下载后格式会变)
                        init_pix_fmt = ctx.config.sw_pixel_format;
                        Ok(ctx)
                    })
                    .context("Hardware acceleration context initialization failed")
            })
            .transpose()?;

        let dict = self.codec_opts.map(|opts| opts.into_dict());
        decode_ctx
            .open(dict)
            .context("Failed to open decoder for stream")?;

        let stream_info = StreamInfo::from_stream(input_stream)?;
        log::info!("{}", stream_info);

        let filter_graph = if let Some(filters) = self.filters {
            let filter_params = match media_type {
                MediaType::VIDEO => {
                    FilterParams::Video(VideoParams {
                        width: init_width,
                        height: init_height,
                        format: init_pix_fmt, // 使用解码器（或HW下载后）的格式
                        time_base: init_time_base,
                        frame_rate: decode_ctx.framerate, // 使用解码器帧率
                        pixel_aspect: decode_ctx.sample_aspect_ratio,
                    })
                }
                MediaType::AUDIO => FilterParams::Audio(AudioParams {
                    nb_channels: init_ch_layout.nb_channels,
                    sample_rate: init_sample_rate,
                    format: init_sample_fmt,
                    time_base: init_time_base,
                }),
                _ => panic!("Unsupported filter for media type: {:?}", media_type),
            };

            let mut graph = FilterGraph::new();
            // 验证 Filter 链的媒体类型是否与当前流匹配
            if !filters.iter().all(|f| f.media_type() == media_type) {
                return Err(Error::msg(format!(
                    "Filter media type mismatch for stream type {:?}",
                    media_type
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
            state: DecoderState::Normal,
        })
    }
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum DecoderState {
    Normal,
    Drained,
    Flushed,
}

/// Decode video files and streams.
///
/// # Example
///
/// ```ignore
/// let decoder = Decoder::new(Path::new("video.mp4")).unwrap();
/// decoder
///     .decode_iter()
///     .take_while(Result::is_ok)
///     .for_each(|frame| println!("Got frame!"),
/// );
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
    state: DecoderState,
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

    /// Get the decoders input size width
    #[inline(always)]
    pub fn width(&self) -> usize {
        self.context.width as usize
    }

    /// Get the decoders input size height
    #[inline(always)]
    pub fn height(&self) -> usize {
        self.context.height as usize
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
        self.state == DecoderState::Drained
    }

    pub fn is_flushed(&self) -> bool {
        self.state == DecoderState::Flushed
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
    ///     let (ts, frame) = decoder.decode()?;
    ///     // Do something with frame...
    /// }
    /// ```
    #[cfg(feature = "ndarray")]
    pub fn decode<T>(&mut self, reader: &mut impl Reader) -> Result<Option<MediaFrame<T>>>
    where
        T: MediaFrameType,
    {
        if self.is_flushed() {
            return Err(Error::msg(
                "Decoder cannot decode after flushed. Call reset().",
            ));
        }

        let mut read_exhausted = false;
        Ok(loop {
            if !read_exhausted {
                match reader.read_packet() {
                    Ok(Some((stream, packet))) => {
                        if stream.index() != self.stream_index() {
                            // skip other streams
                            log::debug!("skip stream index: {}, {:?}", stream.index(), packet);
                            continue;
                        }
                        if let Some(frame) = self.decode_packet(&packet)? {
                            break Some(frame);
                        }
                    }
                    Ok(None) => {
                        log::debug!("No more packets, Reader exhausted.");
                        read_exhausted = true;
                        continue;
                    }
                    Err(e) => {
                        log::error!("Error reading packet: {}", e);
                        return Err(e);
                    }
                }
            } else {
                match self.drain() {
                    Ok(Some(frame)) => {
                        break Some(frame);
                    }
                    Ok(None) => {
                        log::debug!("Decoder flushed. EOF reached.");
                        // self.reset();
                        // read_exhausted = false;
                        break None;
                    }
                    Err(e) => {
                        log::error!("Error to drain decoder: {}", e);
                        return Err(e);
                    }
                }
            }
        })
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
        if self.is_flushed() {
            return Err(Error::msg(
                "Decoder cannot decode after flushed. Call reset().",
            ));
        }

        let mut read_exhausted = false;
        Ok(loop {
            if !read_exhausted {
                match reader.read_packet() {
                    Ok(Some((stream, packet))) => {
                        if stream.index() != self.stream_index() {
                            // skip other streams
                            log::debug!("skip stream index: {}, {:?}", stream.index(), packet);
                            continue;
                        }
                        if let Some(frame) = self.decode_raw_packet(&packet)? {
                            break Some(frame);
                        }
                    }
                    Ok(None) => {
                        log::debug!("No more packets, Reader exhausted.");
                        read_exhausted = true;
                        continue;
                    }
                    Err(e) => {
                        log::error!("Error reading packet: {}", e);
                        return Err(e);
                    }
                }
            } else {
                match self.drain_raw() {
                    Ok(Some(frame)) => {
                        break Some(frame);
                    }
                    Ok(None) => {
                        log::debug!("Decoder flushed. EOF reached.");
                        // self.reset();
                        // read_exhausted = false;
                        break None;
                    }
                    Err(e) => {
                        log::error!("Error to drain decoder: {}", e);
                        return Err(e);
                    }
                }
            }
        })
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
    /// A tuple of the [`Frame`] and timestamp (relative to the stream) and the frame itself if the
    /// decoder has a frame available, [`None`] if not.
    #[cfg(feature = "ndarray")]
    pub fn decode_packet<T>(&mut self, packet: &AVPacket) -> Result<Option<MediaFrame<T>>>
    where
        T: MediaFrameType,
    {
        match self.decode_raw_packet(packet) {
            Ok(Some(raw_frame)) => Ok(Some(self.raw_frame_to_media_frame(&raw_frame)?)),
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
            Ok(Some(raw_frame)) => Ok(Some(self.raw_frame_to_media_frame(&raw_frame)?)),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    #[cfg(feature = "ndarray")]
    fn raw_frame_to_media_frame<T>(&self, frame: &AVFrame) -> Result<MediaFrame<T>>
    where
        T: MediaFrameType,
    {
        // AVFrame default pixel is YUV420P, So here keeping the format that YUV420P the same
        // after I convert it, If you want RGB24, always remember to convert it yourself!
        MediaFrame::<T>::from_avframe(frame)
    }

    /// Drain one frame from the decoder.
    ///
    /// After calling drain once the decoder is in draining mode and the caller may not use normal
    /// decode anymore, or it will panic.
    ///
    /// # Return value
    ///
    /// The decoded raw frame as [`AVFrame`] if the decoder has a frame available, [`None`] if not.
    pub fn drain_raw(&mut self) -> Result<Option<AVFrame>> {
        if !self.is_drained() {
            self.send_packet_to_decoder(None)?;
        }
        self.receive_frame_from_decoder()
    }

    /// Reset the decoder to be used again after draining.
    pub fn reset(&mut self) {
        self.flush();
        self.state = DecoderState::Normal;
    }

    fn flush(&mut self) {
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
            Ok(None) => return Ok(None), // Decoder drained or flushed
            Err(e) => return Err(e),
        };

        // 2. 处理硬件加速帧下载 (如果需要)
        let sw_frame = match &self.hw_context {
            Some(hw_ctx) if hw_ctx.is_hw_frame(&decoded_frame) => hw_ctx
                .hw_download(&mut self.context, &decoded_frame)
                .context("Failed HW frame download")?,
            _ => decoded_frame, // 已经是 CPU 帧或无 HW 加速
        };

        // 3. 应用 Filter Graph (如果存在)
        if let Some(graph) = self.filter_graph.as_mut() {
            // filter process
            match graph.process_frame(Some(sw_frame))? {
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
            // 4. 如果没有 Filter Graph，直接返回 CPU 帧
            Ok(Some(sw_frame))
        }
    }

    /// Pull a decoded frame from the decoder. This function also implements retry mechanism in case
    /// the decoder signals `EAGAIN` and `EOF`
    fn decoder_receive_frame(&mut self) -> Result<Option<AVFrame>> {
        match self.context.receive_frame() {
            Ok(frame) => Ok(Some(frame)),
            Err(rsmpeg::error::RsmpegError::DecoderDrainError) => {
                log::debug!("Decoder drained. try send new packet again.");
                self.state = DecoderState::Drained;
                Ok(None)
            }
            Err(rsmpeg::error::RsmpegError::DecoderFlushedError) => {
                log::debug!("Decoder flushed. EOF reached.");
                self.state = DecoderState::Flushed;
                Ok(None)
            }
            Err(e) => {
                log::warn!("Failed to receive frame from decoder: {}", e);
                Err(Error::new(e))
            }
        }
    }
}

/// Important note: Do not forget to drain the decoder after the reader is exhausted. It may still
/// contain frames. Run `drain_raw()` or `drain()` in a loop until no more frames are produced.
impl Drop for Decoder {
    fn drop(&mut self) {
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
                Err(e) => log::error!("Failed to flush filter graph during Decoder drop: {}", e),
            }
        }

        // We need to drain the items still in the decoders queue.
        match self.send_packet_to_decoder(None) {
            Ok(_) => {
                loop {
                    match self.decoder_receive_frame() {
                        Ok(Some(_frame)) => {
                            // If receive a frame, we continue to drain the queue.
                            log::debug!("continue draining decoder queue.");
                        }
                        Ok(None) => {
                            if self.is_drained() {
                                // If we need more, we continue to drain the queue.
                                log::debug!("Decoder drained. try send new packet again.");
                                continue;
                            } else {
                                log::debug!("Decoder flushed. EOF reached.");
                                break;
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to drain decoder: {}", e);
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to send flush packet to decoder: {}", e)
            }
        }

        unsafe {
            // explicitly drop the hw_context to release the hardware resources
            // 1. malloc(): unsorted double linked list corrupted
            // 2. malloc(): mismatching next->prev_size (unsorted)
            // 3. free(): invalid pointer
            // 4. double free or corruption (!prev)
            // 5. corrupted double-linked list Aborted (core dumped)
            let codec_ctx_ptr = self.context.as_mut_ptr();
            if !codec_ctx_ptr.is_null() {
                if !(*codec_ctx_ptr).hw_frames_ctx.is_null() {
                    let _hw_frames = (*codec_ctx_ptr).hw_frames_ctx;
                    (*codec_ctx_ptr).hw_frames_ctx = std::ptr::null_mut();
                }

                if !(*codec_ctx_ptr).hw_device_ctx.is_null() {
                    let _hw_device = (*codec_ctx_ptr).hw_device_ctx;
                    (*codec_ctx_ptr).hw_device_ctx = std::ptr::null_mut();
                }
            }
        }
    }
}

unsafe impl Send for Decoder {}
unsafe impl Sync for Decoder {}

/// 解码器包装器，持有 Decoder 和 Reader
pub struct DecoderWrapper<R: Reader> {
    reader: R,
    decoder: Decoder,
}

impl<R: Reader> DecoderWrapper<R> {
    /// 创建一个新的解码器包装器
    pub fn new(decoder: Decoder, reader: R) -> Self {
        Self { decoder, reader }
    }

    /// 解码下一帧（媒体帧）
    #[cfg(feature = "ndarray")]
    pub fn decode<T: MediaFrameType>(&mut self) -> Result<Option<MediaFrame<T>>> {
        self.decoder.decode(&mut self.reader)
    }

    /// 解码下一帧（原始帧）
    pub fn decode_raw(&mut self) -> Result<Option<AVFrame>> {
        self.decoder.decode_raw(&mut self.reader)
    }

    /// 获取内部解码器的可变引用
    pub fn decoder_mut(&mut self) -> &mut Decoder {
        &mut self.decoder
    }

    /// 获取内部读取器的可变引用
    pub fn reader_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    /// 解构并返回内部组件
    pub fn into_parts(self) -> (Decoder, R) {
        (self.decoder, self.reader)
    }

    /// Seek in reader.
    ///
    /// See [`StreamReader::seek_to_time`](crate::io::StreamReader::seek_to_timestamp) for more information.
    #[inline]
    pub fn seek_to_timestamp(&mut self, timestamp_milliseconds: i64) -> Result<()> {
        if let Some(stream_reader) = self.reader.as_any_mut().downcast_mut::<StreamReader>() {
            stream_reader
                .seek_to_timestamp(timestamp_milliseconds)
                .inspect(|_| self.decoder.flush())
        } else {
            Err(Error::msg("Seek is only supported for StreamReader"))
        }
    }

    /// Seek to specific frame in reader.
    ///
    /// See [`StreamReader::seek_to_frame`](crate::io::StreamReader::seek_to_frame) for more information.
    #[inline]
    pub fn seek_to_frame(&mut self, frame_number: i64) -> Result<()> {
        if let Some(stream_reader) = self.reader.as_any_mut().downcast_mut::<StreamReader>() {
            stream_reader
                .seek_to_frame(
                    self.decoder.stream_index(),
                    frame_number,
                    ffi::AVSEEK_FLAG_ANY as i32,
                )
                .inspect(|_| self.decoder.flush())
        } else {
            Err(Error::msg(
                "Seek to frame is only supported for StreamReader",
            ))
        }
    }

    /// Seek to start of reader.
    ///
    /// See [`StreamReader::seek_to_start`](crate::io::StreamReader::seek_to_start) for more information.
    #[inline]
    pub fn seek_to_start(&mut self) -> Result<()> {
        if let Some(stream_reader) = self.reader.as_any_mut().downcast_mut::<StreamReader>() {
            stream_reader
                .seek_to_start()
                .inspect(|_| self.decoder.flush())
        } else {
            Err(Error::msg(
                "Seek to start is only supported for StreamReader",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter;

    #[test]
    #[ignore = "need a video file"]
    fn test_decode_video() -> Result<()> {
        let video_path = std::path::Path::new("/tmp/bear.mp4");

        let filters = vec![
            filter::video::scale(1280, 720, None),
            filter::video::drawtext("Hello", 10, 10, "", 24, "white"),
        ];

        let mut decoder = DecoderBuilder::new(MediaType::VIDEO)
            .with_filters(Some(filters))
            .build_wrapped(video_path)?;

        loop {
            match decoder.decode_raw() {
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
    #[ignore = "need a audio file"]
    fn test_decode_audio() -> Result<()> {
        let audio_path = std::path::Path::new("/tmp/bear.mp4");

        let filters = vec![
            filter::audio::resample(2, 48000, SampleFormat::FLTP),
            filter::audio::volume(1.5),
        ];

        let mut decoder = DecoderBuilder::new(MediaType::AUDIO)
            .with_filters(Some(filters))
            .build_wrapped(audio_path)?;

        loop {
            match decoder.decode_raw() {
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
}
