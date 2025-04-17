use crate::codec::CodecConfig;
use crate::filter::{AudioParams, Filter, FilterGraph, FilterParams, VideoParams};
use crate::flags::AvFormatFlags;
#[cfg(feature = "ndarray")]
use crate::frame::{MediaFrame, MediaFrameType};
use crate::hwaccel::{HWContext, HWDeviceConfig};
use crate::io::{private::Output, Writer};
use crate::options::Options;
use crate::pixel::PixelFormat;
use crate::{swctx, time, utils, Location, MediaType, SampleFormat, StreamWriter};

use rsmpeg::avcodec::{AVCodec, AVCodecContext, AVCodecParameters, AVPacket};
use rsmpeg::avutil::{AVChannelLayout, AVChannelLayoutRef, AVFrame};
use rsmpeg::ffi;

use anyhow::{Context, Error, Result};
use std::sync::Arc;

/// Builds an [`Encoder`].
#[derive(Debug)]
pub struct EncoderBuilder {
    /// Video
    width: usize,
    height: usize,
    pixel_format: PixelFormat,
    /// Audio
    nb_channels: i32,
    sample_rate: i32,
    sample_format: SampleFormat,
    /// Common
    bit_rate: i64,
    gop_size: i32,
    max_b_frames: i32,
    time_base: ffi::AVRational,
    pkt_time_base: ffi::AVRational,
    frame_rate: ffi::AVRational,
    /// config
    oformat_flags: i32,
    thread_count: usize,
    keyframe_interval: u64,
    media_type: MediaType,
    codec_name: Option<String>,
    codec_opts: Option<Options>,
    filters: Option<Vec<Filter>>,
    hw_device_config: Option<HWDeviceConfig>,
}

impl EncoderBuilder {
    /// Default keyframe interval.
    const KEY_FRAME_INTERVAL: u64 = 12;

    /// This is the assumed FPS for the encoder to use.
    /// Note that this does not need to be correct exactly.
    const FRAME_RATE: i32 = 24;

    /// Default bit rate.
    /// 分辨率(width, height) + 推荐比特率（单位：bps）
    /// * 标清 Sd_480p:          (640, 480)   => 1_000_000,   // 1 Mbps
    /// * 高清 Hd_720p:          (1280, 720)  => 2_500_000,   // 2.5 Mbps
    /// * 全高清 FullHd(1080p):  (1920, 1080) => 5_000_000,   // 5 Mbps
    /// * 超高清 FullHd_2k:      (2560, 1440) => 8_000_000,   // 8 Mbps
    /// * 超高清 UltraHd_4K:     (3840, 2160) => 20_000_000,  // 20 Mbps
    /// * 超高清 FullUltraHd_8K: (7680, 4320) => 60_000_000,  // 60 Mbps
    const VIDEO_BIT_RATE: i64 = 1_000_000;

    /// default codec
    const VIDEO_CODEC_NAME: &'static str = "libx264";
    const AUDIO_CODEC_NAME: &'static str = "aac";

    /// Create a video encoder with the specified destination
    ///
    /// The default codec is `libx264`, with default frame rate and bit rate.
    ///
    /// # Arguments
    ///
    /// * `width` - The width of the video stream.
    /// * `height` - The height of the video stream.
    /// * `pixel_format` - The desired pixel format for the video stream.
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
            .with_time_base(1, sample_rate)
            .with_sample_format(sample_format)
            .with_media_type(MediaType::AUDIO)
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
    pub fn with_codec_name(mut self, codec_name: Option<String>) -> Self {
        self.codec_name = codec_name;
        self
    }

    /// Set the keyframe interval.
    pub fn with_keyframe_interval(mut self, keyframe_interval: u64) -> Self {
        self.keyframe_interval = keyframe_interval;
        self
    }

    /// set the thread count.
    pub fn with_thread_count(mut self, thread_count: usize) -> Self {
        self.thread_count = thread_count;
        self
    }

    /// Set the bit rate.
    pub fn with_bit_rate(mut self, bit_rate: i64) -> Self {
        self.bit_rate = bit_rate;
        self
    }

    /// Set the frame rate.
    pub fn with_frame_rate_ra(mut self, frame_rare: ffi::AVRational) -> Self {
        self.frame_rate = frame_rare;
        self
    }

    pub fn with_frame_rate(mut self, num: i32, den: i32) -> Self {
        self.frame_rate = time::new_rational(num, den);
        self
    }

    /// Set the time base.
    pub fn with_time_base_ra(mut self, time_base: ffi::AVRational) -> Self {
        self.time_base = time_base;
        self
    }

    pub fn with_time_base(mut self, num: i32, den: i32) -> Self {
        self.time_base = time::new_rational(num, den);
        self
    }

    /// Set the packet time base.
    pub fn with_pkt_time_base_ra(mut self, pkt_time_base: ffi::AVRational) -> Self {
        self.pkt_time_base = pkt_time_base;
        self
    }

    pub fn with_pkt_time_base(mut self, num: i32, den: i32) -> Self {
        self.pkt_time_base = time::new_rational(num, den);
        self
    }

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
    pub fn with_pixel_format(mut self, pixel_format: PixelFormat) -> Self {
        self.pixel_format = pixel_format;
        self
    }

    /// codec options used for encoder
    pub fn with_options(mut self, options: Option<Options>) -> Self {
        self.codec_opts = options;
        self
    }

    /// filters used for encoder
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

    /// explicit media type, default is `MediaType::VIDEO`
    pub fn with_media_type(mut self, media_type: MediaType) -> Self {
        self.media_type = media_type;
        self
    }

    pub fn with_nb_channels(mut self, nb_channels: i32) -> Self {
        assert!(
            nb_channels > 0 && nb_channels < 9,
            "nb_channels should be in range [1, 8]"
        );
        self.nb_channels = nb_channels;
        self
    }

    pub fn with_sample_rate(mut self, sample_rate: i32) -> Self {
        self.sample_rate = sample_rate;
        self
    }

    pub fn with_sample_format(mut self, sample_format: SampleFormat) -> Self {
        self.sample_format = sample_format;
        self
    }

    /// Some formats want stream headers to be separate.
    pub fn with_oformat_flags(mut self, flags: AvFormatFlags) -> Self {
        self.oformat_flags = flags as i32;
        self
    }

    /// Apply the settings to an encoder.
    ///
    /// # Arguments
    ///
    /// * `encoder` - Encoder to apply settings to.
    ///
    /// # Return value
    ///
    /// New encoder with settings applied.
    fn setup_codec_context(&self, encoder: &mut AVCodecContext) -> Result<()> {
        let media_type = self.media_type;
        if media_type as ffi::AVMediaType != encoder.codec_type {
            return Err(Error::msg(format!(
                "Encoder codec type not supported: {:?} vs. {:?}",
                media_type, encoder.codec_type
            )));
        }

        if media_type == MediaType::VIDEO {
            encoder.set_width(self.width as i32);
            encoder.set_height(self.height as i32);
            encoder.set_bit_rate(self.bit_rate);
            encoder.set_gop_size(self.gop_size);
            encoder.set_max_b_frames(self.max_b_frames);
            encoder.set_framerate(self.frame_rate);
            encoder.set_time_base(self.time_base);
            encoder.set_pkt_timebase(self.pkt_time_base);
            encoder.set_pix_fmt(self.pixel_format.into());
            encoder.set_sample_aspect_ratio(time::new_rational(1, 1));
        } else if media_type == MediaType::AUDIO {
            encoder.set_ch_layout(AVChannelLayout::from_nb_channels(self.nb_channels).into_inner());
            encoder.set_bit_rate(self.bit_rate);
            encoder.set_sample_rate(self.sample_rate);
            encoder.set_time_base(time::new_rational(1, self.sample_rate));
            encoder.set_sample_fmt(self.sample_format as _);
        } else {
            return Err(Error::msg(format!(
                "Unsupported media type: {:?}",
                media_type
            )));
        }

        // Some formats want stream headers to be separate.
        if self.oformat_flags & ffi::AVFMT_GLOBALHEADER as i32 != 0 {
            encoder.set_flags(encoder.flags | ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32);
        }
        unsafe {
            (*encoder.as_mut_ptr()).thread_count = self.thread_count as i32;
        }

        Ok(())
    }

    pub fn build_wrapped(
        self,
        destination: impl Into<Location>,
    ) -> Result<EncoderWrapper<StreamWriter>> {
        let encoder = self.build()?;
        let mut writer = StreamWriter::new(destination)?;
        let index = writer.add_stream(encoder.codecpar(), encoder.time_base());
        Ok(EncoderWrapper::new(encoder, writer, index, true))
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
        let codec = {
            let codec_name = if let Some(codec_name) = &self.codec_name {
                codec_name.as_ref()
            } else {
                match media_type {
                    MediaType::VIDEO => Self::VIDEO_CODEC_NAME,
                    MediaType::AUDIO => Self::AUDIO_CODEC_NAME,
                    _ => {
                        return Err(Error::msg(format!(
                            "Unsupported media type:{:?}",
                            media_type
                        )))
                    }
                }
            };
            AVCodec::find_encoder_by_name(&utils::from_str(codec_name)).context(format!(
                "Failed to find encoder for codec: '{}'",
                codec_name
            ))?
        };

        let mut encode_ctx = AVCodecContext::new(&codec);
        self.setup_codec_context(&mut encode_ctx)?;
        let config = CodecConfig::new_with_ctx(&encode_ctx);

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
                            "Encoder with HW acceleration is not supported for codec: {codec_name}"
                        ))
                    })?;

                log::info!(
                    "Video Encoder with HW acceleration codec: {:?}, hw_pixel: {:?}, config: {:#?}",
                    codec.name(),
                    PixelFormat::from(hw_pixel),
                    cfg
                );

                // create hardware context
                let (width, height) = (encode_ctx.width, encode_ctx.height);
                HWContext::new(cfg)
                    .and_then(|ctx| {
                        // *注意*: setup_hw_frames 会根据 HW 能力修改 encode_ctx.pix_fmt
                        ctx.setup_hw_frames(false, &mut encode_ctx, width, height)?;
                        // 更新 Builder 中记录的目标格式，以反映 HW 的要求
                        // self.pixel_format = PixelFormat::from(encode_ctx.pix_fmt);
                        Ok(ctx)
                    })
                    .context("Hardware acceleration context initialization failed")
            })
            .transpose()?;

        let dict = self.codec_opts.map(|opts| opts.into_dict());
        encode_ctx
            .open(dict)
            .context("Failed to open encode context")?;

        let filter_graph = if let Some(filters) = self.filters {
            let filter_params = match media_type {
                MediaType::VIDEO => {
                    FilterParams::Video(VideoParams {
                        width: self.width as i32, // 使用 Builder 的 width/height/format
                        height: self.height as i32,
                        format: self.pixel_format,
                        time_base: self.time_base, // 使用 Builder 的 time_base
                        frame_rate: self.frame_rate,
                        pixel_aspect: time::new_rational(1, 1), // 默认
                    })
                }
                MediaType::AUDIO => {
                    FilterParams::Audio(AudioParams {
                        nb_channels: self.nb_channels,
                        sample_rate: self.sample_rate,
                        format: self.sample_format, // 使用 Builder 的 format/rate/channels
                        time_base: time::new_rational(1, self.sample_rate), // 基于 Builder 的 rate
                    })
                }
                _ => {
                    panic!("Unsupported filter for media type: {:?}", media_type);
                }
            };
            let mut graph = FilterGraph::new();
            // 验证 Filter 链的媒体类型
            if !filters.iter().all(|f| f.media_type() == media_type) {
                return Err(Error::msg(format!(
                    "Filter media type mismatch for encoder type {:?}",
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

        Ok(Encoder {
            config,
            hw_context,
            media_type,
            filter_graph,
            context: encode_ctx,
            state: EncoderState::Normal,
            keyframe_interval: self.keyframe_interval,
        })
    }
}

impl Default for EncoderBuilder {
    fn default() -> Self {
        Self {
            // video
            width: 0,
            height: 0,
            pixel_format: PixelFormat::YUV420P,
            time_base: time::TIME_BASE,
            pkt_time_base: time::TIME_BASE,
            bit_rate: Self::VIDEO_BIT_RATE,
            frame_rate: time::new_rational(Self::FRAME_RATE, 1),
            keyframe_interval: Self::KEY_FRAME_INTERVAL,
            gop_size: 0,
            max_b_frames: 0,
            oformat_flags: AvFormatFlags::GLOBAL_HEADER as i32,
            // audio
            nb_channels: 2,
            sample_rate: 44100,
            sample_format: SampleFormat::FLTP,
            // common
            media_type: MediaType::VIDEO,
            thread_count: num_cpus::get(),
            codec_name: None,
            codec_opts: None,
            filters: None,
            hw_device_config: None,
        }
    }
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum EncoderState {
    Normal,
    Drained,
    Flushed,
}

/// Encodes frames into a video stream.
///
/// # Example
///
/// ```ignore
/// let encoder = Encoder::new(
///     Path::new("video_in.mp4"),
///     Settings::for_h264_yuv420p(800, 600, 30.0)
/// )
/// .unwrap();
///
/// let decoder = Decoder::new(Path::new("video_out.mkv")).unwrap();
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
    hw_context: Option<Arc<HWContext>>,
    media_type: MediaType,
    keyframe_interval: u64,
    state: EncoderState,
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
        self.state == EncoderState::Drained
    }

    /// Returns `true` if the encoder is fully flushed and finished.
    pub fn is_flushed(&self) -> bool {
        self.state == EncoderState::Flushed
    }

    /// Encode a high-level frame (a single frame ndarray-based)
    ///
    /// # Arguments
    ///
    /// * `frame` - Frame to encode in `HWC` format and standard layout.
    #[cfg(feature = "ndarray")]
    pub fn encode<T>(&mut self, frame: MediaFrame<T>) -> Result<Option<AVPacket>>
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
    pub fn encode_raw(&mut self, frame: AVFrame) -> Result<Option<AVPacket>> {
        log::info!("{:?}, time_base: {:?}", frame, frame.time_base);

        // reformat
        let raw_frame = match self.media_type {
            MediaType::VIDEO => {
                if frame.width != self.width()
                    || frame.height != self.height()
                    || frame.format != self.pix_fmt().into()
                {
                    swctx::scale(&frame, self.width(), self.height(), self.pix_fmt())?
                } else {
                    frame
                }
            }
            MediaType::AUDIO => {
                let ch_layout = self.context.ch_layout;
                if frame.sample_rate != self.sample_rate()
                    || frame.format != self.sample_fmt() as i32
                    || frame.ch_layout.nb_channels != ch_layout.nb_channels
                {
                    swctx::convert_frame(
                        &frame,
                        ch_layout,
                        self.sample_fmt() as _,
                        self.sample_rate(),
                    )?
                } else {
                    frame
                }
            }
            _ => {
                // do nothing
                return Err(Error::msg(format!(
                    "Unsupported encode frame media type: {:?}",
                    self.media_type
                )));
            }
        };

        // send frame
        self.send_frame_to_encoder(Some(raw_frame))?;

        // receive packet
        self.receive_packet()
    }

    fn send_frame_to_encoder(&mut self, frame_opt: Option<AVFrame>) -> Result<()> {
        // 1. 应用 Filter Graph (如果存在)
        let filtered_frame = if let Some(graph) = self.filter_graph.as_mut() {
            // 即便输入是 None (EOF flush), 也要调用 process_frame(None) 来驱动 Filter flush
            match graph.process_frame(frame_opt)? {
                Some(filtered) => Some(filtered),
                None => {
                    if graph.is_drained() {
                        log::debug!("Filter graph drained, try send new frame again.");
                        return Ok(());
                    } else if graph.is_flushed() {
                        log::warn!("Filter graph EOF reached.");
                    } else {
                        log::error!("Filter graph returned None, should not happen.");
                    }
                    None
                }
            }
        } else {
            // 没有Filter，直接发送到 Encoder 或者是 EOF
            frame_opt
        };

        let final_frame = if let Some(mut frame) = filtered_frame {
            // 2. 处理需要发送给编码器的帧 (可能是过滤后的，也可能是原始的，或者是 None)
            // 确保关键帧标记正确, *注意*：frame_num 在 send_frame 后才更新
            if (self.context.frame_num + 1) % self.keyframe_interval as i64 == 0 {
                frame.set_pict_type(ffi::AV_PICTURE_TYPE_I);
            }

            // HW 上传 (如果需要)
            let hw_frame = match self.hw_context.as_ref() {
                Some(hw_ctx) if hw_ctx.is_sw_frame(&frame) => {
                    // sw_frame -> hw_frame
                    hw_ctx
                        .hw_upload(&mut self.context, &frame)
                        .context("Failed to upload frame to HW")?
                }
                _ => frame, // 不需要上传或已经是 HW frame
            };

            Some(hw_frame)
        } else {
            // EOF
            None
        };

        log::debug!(
            "Send frame to encoder: {:?}, time_base: {:?}",
            final_frame,
            self.time_base()
        );

        // c. 发送最终帧给编码器上下文
        self.context.send_frame(final_frame.as_ref())?;

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
    pub fn sample_rate(&self) -> i32 {
        self.context.sample_rate
    }

    #[inline]
    pub fn sample_fmt(&self) -> SampleFormat {
        SampleFormat::from(self.context.sample_fmt)
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

    #[inline]
    pub fn media_type(&self) -> MediaType {
        self.media_type
    }

    #[inline]
    pub fn codecpar(&self) -> AVCodecParameters {
        self.context.extract_codecpar()
    }

    #[inline]
    pub fn ch_layout(&self) -> AVChannelLayoutRef {
        self.context.ch_layout()
    }

    /// Internal: Pull an encoded packet from the decoder.
    ///
    /// Handles `EAGAIN`, drained, and flushed states.
    ///
    /// # Returns
    ///
    /// `Some(packet)` if a packet is returned, `None` if waiting or end.
    fn receive_packet(&mut self) -> Result<Option<AVPacket>> {
        match self.context.receive_packet() {
            Ok(pkt) => Ok(Some(pkt)),
            Err(rsmpeg::error::RsmpegError::EncoderDrainError) => {
                log::debug!("Encoder drained, try send new frame again.");
                self.state = EncoderState::Drained;
                Ok(None)
            }
            Err(rsmpeg::error::RsmpegError::EncoderFlushedError) => {
                log::debug!("Encoder flushed, EOF reached.");
                self.state = EncoderState::Flushed;
                Ok(None)
            }
            Err(err) => Err(Error::new(err)),
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
        // 确定编码器是否支持延迟（delay）
        // 如果编码器不支持延迟，那么就没有必要进行 flush 操作，因为在这种情况下，编码器不会保留任何未处理的数据。
        // 如果编码器支持延迟（delay），则在结束编码之前发送 EOS 包是有必要的，
        // 因为编码器可能还在缓冲一些数据，直到接收到 EOS 信号才会处理完这些数据并输出剩余的包。
        if !self.config.support_delayed_frame() {
            return Ok(());
        }

        if let Some(filter) = self.filter_graph.as_mut() {
            let frames = filter.flush()?;
            for frame in frames {
                self.send_frame_to_encoder(Some(frame))?;
            }
        }

        // EOF: Notify the encoder that the last frame has been sent.
        self.send_frame_to_encoder(None)?;

        // drain the items still on the queue before giving up.
        loop {
            match self.receive_packet() {
                Ok(Some(mut packet)) => {
                    packet.set_pos(-1);
                    packet.set_stream_index(index as i32);
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
                        continue;
                    } else {
                        log::debug!("Encoder flushed, EOF reached.");
                        break;
                    }
                }
                Err(e) => {
                    log::debug!("Encode packet error: {}", e);
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
/// - We implement `Send`/`Sync` only because `Encoder` is guaranteed to be used
///   in a single-threaded context or externally synchronized by the caller.
/// - If used across threads, caller must ensure no concurrent access.
unsafe impl Send for Encoder {}
unsafe impl Sync for Encoder {}

/// 编码器包装器，持有编码器和写入器
pub struct EncoderWrapper<W: Writer> {
    writer: W,
    encoder: Encoder,
    interleaved: bool,
    stream_index: usize,
    have_written_header: bool,
    have_written_trailer: bool,
}

impl<W: Writer> EncoderWrapper<W> {
    /// 创建一个新的编码器包装器
    pub fn new(encoder: Encoder, writer: W, stream_index: usize, interleaved: bool) -> Self {
        Self {
            writer,
            encoder,
            interleaved,
            stream_index,
            have_written_header: false,
            have_written_trailer: false,
        }
    }

    #[cfg(feature = "ndarray")]
    pub fn encode<T: MediaFrameType>(&mut self, frame: MediaFrame<T>) -> Result<()> {
        let raw_frame = frame.to_avframe()?;
        self.encode_raw(raw_frame)
    }

    pub fn encode_raw(&mut self, frame: AVFrame) -> Result<()> {
        // Write file header if we hadn't done that yet.
        if !self.have_written_header {
            self.writer.write_header()?;
            self.have_written_header = true;
        }

        if let Some(mut pkt) = self.encoder.encode_raw(frame)? {
            if self.interleaved {
                self.writer.write_interleaved(&mut pkt)?;
            } else {
                self.writer.write_frame(&mut pkt)?;
            }
        }

        Ok(())
    }

    /// Signal to the encoder that writing has finished. This will cause any packets in the encoder
    /// to be flushed and a trailer to be written if the container format has one.
    ///
    /// Note: If you don't call this function before dropping the encoder, it will be called
    /// automatically. This will block the caller thread. Any errors cannot be propagated in this
    /// case.
    pub fn finish(&mut self) -> Result<()> {
        if self.have_written_header && !self.have_written_trailer {
            self.have_written_trailer = true;
            self.flush()?;
            self.writer.write_trailer()?;
        }

        Ok(())
    }

    /// 刷新编码器并写入剩余数据
    fn flush(&mut self) -> Result<()> {
        self.encoder.flush(
            &mut self.writer,
            self.interleaved,
            self.stream_index,
            self.encoder.time_base(),
        )
    }

    pub fn time_base(&self) -> ffi::AVRational {
        self.encoder.time_base()
    }

    /// 获取内部编码器的可变引用
    pub fn encoder_mut(&mut self) -> &mut Encoder {
        &mut self.encoder
    }

    /// 获取内部写入器的可变引用
    pub fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    /// 解构并返回内部组件
    pub fn into_parts(self) -> (Encoder, W) {
        (self.encoder, self.writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter;

    use std::collections::HashMap;
    use std::path::Path;

    /// 定义视频格式参数结构体
    #[allow(dead_code)]
    struct VideoFormatParams {
        time_base: (i32, i32),
        codec_name: String,
        /// 支持的帧率列表（VIDEO）
        supported_frame_rates: Option<Vec<ffi::AVRational>>,
        /// 支持的像素格式列表（VIDEO）
        supported_pix_fmts: Vec<ffi::AVPixelFormat>,
        /// 特定编码器选项
        codec_options: Option<HashMap<String, String>>,
        /// 特定格式选项
        format_options: Option<HashMap<String, String>>,
    }

    /// 动态码率/帧率调整、关键帧间隔控制
    /// 完善主流编码格式支持（H.264/265, VP9/AV1, AAC/Opus）
    ///
    /// | 容器格式 | 标准时间基         | 说明                      |
    /// | :------- | :-------------- | :------------------------|
    /// | MP4      | 1/90_000        | 90kHz，源自MPEG-2标准      |
    /// | MOV      | 1/10_000_000    | 10MHz，苹果QuickTime格式   |
    /// | MKV      | 1/1_000_000_000 | 纳秒级精度                 |
    /// | FLV      | 1/1_000         | 毫秒级，Flash视频标准       |
    /// | TS       | 1/90_000        | 90kHz，MPEG传输流          |
    /// | AVI      | 1/{帧率}         | 基于帧计数，或1/1000        |
    /// | WebM     | 1/1_000_000_000 | 纳秒级，基于MKV            |
    /// | 3GP      | 1/90_000        | 移动设备视频标准            |
    /// | ASF/WMV  | 1/10_000_000    | 100纳秒单位，Windows Media |
    /// | OGG/OGV  | 1/1_000_000     | 微秒级，开源标准            |
    /// | MPEG     | 1/90_000        | 90kHz，MPEG标准           |
    #[cfg(feature = "ndarray")]
    fn test_encode_video_for_container(container_type: &str, fps: f64) -> Result<()> {
        use crate::time::Time;

        // 使用单一match获取基本参数
        let (codec_name, time_base, codec_options, format_options) = match container_type {
            // 常见流媒体/通用格式
            "mp4" => (
                None,
                (1, 90_000), // 90kHz
                None,
                None,
            ),
            "mov" => {
                let mut format_opts = HashMap::new();
                format_opts.insert(
                    "movflags".to_string(),
                    "frag_keyframe+empty_moov".to_string(),
                );

                (
                    None,
                    (1, 90_000), // 90kHz
                    None,
                    Some(format_opts),
                )
            }
            "mkv" => {
                let mut format_opts = HashMap::new();
                format_opts.insert("strict".to_string(), "experimental".to_string());

                (
                    None,
                    (1, 1_000_000_000), // 纳秒级
                    None,
                    Some(format_opts),
                )
            }
            "webm" => (
                Some("libvpx-vp9".to_string()),
                (1, 1_000_000_000), // 纳秒级
                None,
                None,
            ),
            "flv" => {
                let mut format_opts = HashMap::new();
                format_opts.insert("flvflags".to_string(), "no_duration_filesize".to_string());

                (
                    None,
                    (1, 1_000), // 毫秒级
                    None,
                    Some(format_opts),
                )
            }
            "ts" | "mts" | "m2ts" => (
                None,
                (1, 90_000), // 90kHz
                None,
                None,
            ),
            "avi" => {
                // AVI通常使用帧率作为时间基
                let fps_rounded = fps.round() as i32;

                let mut codec_opts = HashMap::new();
                codec_opts.insert("profile".to_string(), "baseline".to_string());
                codec_opts.insert("level".to_string(), "3.0".to_string());

                let mut format_opts = HashMap::new();
                format_opts.insert("strict".to_string(), "normal".to_string());

                (None, (1, fps_rounded), Some(codec_opts), Some(format_opts))
            }
            "3gp" => (
                None,
                (1, 90_000), // 通常为90kHz
                None,
                None,
            ),
            "wmv" | "asf" => {
                let mut format_opts = HashMap::new();
                format_opts.insert("strict".to_string(), "normal".to_string());

                (
                    None,
                    (1, 10_000_000), // 100纳秒单位
                    None,
                    Some(format_opts),
                )
            }
            "ogg" | "ogv" => (
                Some("libtheora".to_string()),
                (1, 1_000_000), // 微秒级
                None,
                None,
            ),
            "mpg" | "mpeg" => (
                None,
                (1, 90_000), // 90kHz
                None,
                None,
            ),

            // 专业广电格式
            "mxf" => {
                let mut format_opts = HashMap::new();
                format_opts.insert("strict".to_string(), "experimental".to_string());
                format_opts.insert("mxf_operational_pattern".to_string(), "1a".to_string());

                let mut codec_opts = HashMap::new();
                codec_opts.insert("profile".to_string(), "main".to_string());
                codec_opts.insert("r".to_string(), "25".to_string());
                codec_opts.insert("g".to_string(), "15".to_string());
                codec_opts.insert("b".to_string(), "5M".to_string());

                (
                    Some("mpeg2video".to_string()),
                    (1, 25),
                    Some(codec_opts),
                    Some(format_opts),
                )
            }
            "gxf" | "ps" => (Some("mpeg2video".to_string()), (1, 90_000), None, None),
            "xavc" => (None, (1, 90_000), None, None),

            // 硬件设备格式
            "vob" => (Some("mpeg2video".to_string()), (1, 90_000), None, None),
            "rmvb" | "divx" => (None, (1, 90_000), None, None),

            // 特殊格式
            "heif" => {
                let mut codec_opts = HashMap::new();
                codec_opts.insert("x265-params".to_string(), "lossless=1".to_string());

                let mut format_opts = HashMap::new();
                format_opts.insert("brand".to_string(), "heic".to_string());
                format_opts.insert("hvc1_flag".to_string(), "1".to_string());

                (
                    Some("libx265".to_string()),
                    (1, 90_000),
                    Some(codec_opts),
                    Some(format_opts),
                )
            }
            "f4v" | "dav" | "evo" | "h264" => (None, (1, 90_000), None, None),
            "h265" => (Some("libx265".to_string()), (1, 90_000), None, None),
            "cmaf" => {
                let mut format_opts = HashMap::new();
                format_opts.insert(
                    "movflags".to_string(),
                    "cmaf+dash+frag_keyframe+negative_cts_offsets".to_string(),
                );
                format_opts.insert("use_template".to_string(), "1".to_string());
                format_opts.insert("use_timeline".to_string(), "1".to_string());

                (None, (1, 90_000), None, Some(format_opts))
            }

            // 默认值（用于未明确定义的格式）
            _ => (
                None,
                (1, 90_000), // 90kHz为最安全的默认值
                None,
                None,
            ),
        };

        let codec_name = utils::from_str(&codec_name.unwrap_or_else(|| "libx264".to_string()));
        let codec_config = CodecConfig::new_with_name(&codec_name)?;
        assert!(
            codec_config.is_encoder(),
            "Codec:'{:?}' is not an encoder.",
            codec_name
        );

        let config = VideoFormatParams {
            time_base,
            codec_name: codec_name.to_str()?.to_string(),
            supported_frame_rates: codec_config
                .supported_frame_rates()?
                .map(|fps| fps.to_vec()),
            supported_pix_fmts: codec_config.supported_pixel_formats()?.unwrap().to_vec(),
            codec_options,
            format_options,
        };

        let filters = vec![
            filter::video::scale(1920, 1080, None),
            filter::video::drawtext("Watermark", 50, 50, "", 24, "white@0.5"),
            filter::video::crop(0, 0, 640, 360),
        ];

        // 视频编码参数
        let width = 1280;
        let height = 720;
        // 确定输出路径和扩展名
        let output_file = format!("/tmp/test_encode_video.{}", container_type);
        let output_path = Path::new(output_file.as_str());

        // 创建编码器
        let mut encoder = EncoderBuilder::new_video(width as usize, height as usize)
            .with_time_base(time_base.0, time_base.1)
            .with_codec_name(Some(config.codec_name))
            .with_options(config.codec_options.map(|opts| opts.into()))
            .with_filters(Some(filters))
            .build_wrapped(output_path)?;

        fn rainbow_frame(w: usize, h: usize, p: f32) -> MediaFrame<u8> {
            use crate::colors;
            let rgb = colors::hsv_to_rgb(p * 360.0, 100.0, 100.0);
            let mut frame = MediaFrame::<u8>::new_video_frame(
                w,
                h,
                PixelFormat::RGB24,
                time::new_rational(1, 24),
            )
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

        let actual_timebase = encoder.time_base();
        let frame_duration_seconds = 1.0 / fps;

        // 将秒转换为对应时间基单位
        let duration_units = (frame_duration_seconds * actual_timebase.den as f64
            / actual_timebase.num as f64)
            .round() as i64;

        let duration = Time::new(Some(duration_units), actual_timebase);

        // 初始化position时使用正确的时间基
        let mut position = Time::new(Some(0), time::new_rational(time_base.0, time_base.1));

        println!(
            "Encoding {} with actual timebase: {}/{}, duration units: {}, fps: {}",
            container_type, actual_timebase.num, actual_timebase.den, duration_units, fps
        );

        // 帧编码并写入文件
        for i in 0..10 {
            let mut frame = rainbow_frame(width as usize, height as usize, i as f32 / 10.0);
            frame.set_pts(
                position
                    .aligned_with_rational(encoder.time_base())
                    .into_value()
                    .unwrap(),
            );

            encoder.encode(frame)?;

            // 使用aligned_with确保时间基一致进行加法操作
            position = position.aligned_with(duration).add();
        }

        // flush encoder
        encoder.finish().unwrap();

        Ok(())
    }

    #[test]
    #[rustfmt::skip]
    #[ignore = "ignore video output file"]
    fn test_encode_video() -> Result<()> {

        let video_formats = [
            // 通用/主流视频容器
            "mp4",   // MPEG-4 Part 14，最通用的视频格式
            "mkv",   // Matroska，开源高灵活性容器
            "webm",  // Web优化的Matroska子集
            "avi",   // Audio Video Interleave，传统通用格式
            "mov",   // QuickTime格式，苹果生态常用
            "wmv",   // Windows Media Video
            "flv",   // Flash Video，流媒体
            "mpg",   // MPEG-1/2 Program Stream
            "mpeg",  // 同上
            "asf",   // Advanced Systems Format

            // 广播/专业视频容器
            "mxf",   // Material eXchange Format，广电行业标准
            "gxf",   // General eXchange Format，磁带元数据
            "ts",    // MPEG Transport Stream，广播流
            "m2ts",  // Blu-ray MPEG-2 Transport Stream
            "mts",   // 同上
            "xavc",  // Sony's 4K/8K专业格式
            "ps",    // MPEG-2 Program Stream (专业用途)

            // 流媒体/网络视频容器
            "f4v",   // Adobe Flash MP4衍生格式
            "cmaf",  // Common Media Application Format (DASH/HLS)
            "ismv",  // Microsoft Smooth Streaming

            // 移动设备/特殊视频容器
            "3gp",   // 3GPP移动多媒体格式
            "3g2",   // 3GPP2多媒体格式
            "ogv",   // Ogg Video
            "rmvb",  // RealMedia Variable Bitrate
            "rm",    // RealMedia
            "vob",   // DVD Video Object
            "divx",  // DivX Media Format
            "amv",   // Anime Music Video

            // 图像序列/原始视频格式
            "heif",  // High Efficiency Image File Format
            "heic",  // HEIF的iOS实现
            "dav",   // 大华监控专用
            "h264",  // 裸H.264比特流
            "h265",  // 裸H.265/HEVC比特流
            "y4m",   // YUV4MPEG2原始格式

            // 旧版/特殊格式
            "swf",   // Shockwave Flash
            "evo",   // HD-DVD格式
            "ifo",   // DVD信息文件
            "m4v",   // MPEG-4视频文件
        ];

        let mut err_encoder = Vec::new();
        for format in video_formats {
            println!("Testing format: {}...", format);
            match test_encode_video_for_container(format, 24.0) {
                Ok(_) => println!("Testing format: {} passed.", format),
                Err(e) => {
                    println!("Testing format: {} failed: {}", format, e);
                    err_encoder.push(format.to_string());
                }
            }
        }

        if !err_encoder.is_empty() {
            eprintln!("Failed encoders: {:#?}", err_encoder)
        }

        Ok(())
    }
}
