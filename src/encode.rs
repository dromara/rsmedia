use crate::flags::AvFormatFlags;
#[cfg(feature = "ndarray")]
use crate::frame::{self, FrameArray};
use crate::hwaccel::{HWContext, HWDeviceConfig};
use crate::options::Options;
use crate::pixel::PixelFormat;
use crate::swctx;
use crate::time;
use crate::{utils, MediaType, RawFrame, SampleFormat, Writer};

use rsmpeg::avcodec::{AVCodec, AVCodecContext, AVCodecParameters, AVPacket};
use rsmpeg::avutil::{self, AVChannelLayout, AVChannelLayoutRef};
use rsmpeg::ffi;

use anyhow::{Context, Error, Result};
use std::sync::Arc;

/// Builds an [`Encoder`].
#[derive(Clone, Debug)]
pub struct EncoderBuilder {
    /// Video
    width: i32,
    height: i32,
    pixel_format: PixelFormat,
    gop_size: i32,
    time_base: ffi::AVRational,
    pkt_time_base: ffi::AVRational,
    frame_rate: ffi::AVRational,
    max_b_frames: i32,
    keyframe_interval: u64,
    oformat_flags: i32,
    /// Audio
    nb_channels: i32,
    sample_rate: i32,
    sample_format: SampleFormat,
    /// Common
    bit_rate: i64,
    thread_count: i32,
    media_type: MediaType,
    codec_name: Option<String>,
    codec_opts: Option<Options>,
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
    /// # Arguments
    ///
    /// * `width` - The width of the video stream.
    /// * `height` - The height of the video stream.
    /// * `pixel_format` - The desired pixel format for the video stream.
    ///
    /// note: default video codec is `libx264`
    pub fn new_video(width: u32, height: u32) -> Self {
        Self::default().with_width(width).with_height(height)
    }

    /// Create an audio encoder with the specified parameters.
    ///
    /// # Arguments
    ///
    /// * `bit_rate` - The bit rate of the audio stream.
    /// * `nb_channels` - The number of channels in the audio stream.
    /// * `sample_rate` - The sample rate of the audio stream.
    /// * `sample_format` - The sample format of the audio stream.
    ///
    /// note: default audio codec is `aac`
    pub fn new_audio(
        bit_rate: i64,
        nb_channels: u32,
        sample_rate: u32,
        sample_format: SampleFormat,
    ) -> Self {
        Self::default()
            .with_bit_rate(bit_rate)
            .with_nb_channels(nb_channels)
            .with_sample_rate(sample_rate)
            .with_time_base(1, sample_rate as i32)
            .with_sample_format(sample_format)
            .with_media_type(MediaType::AUDIO)
    }

    /// Set the width of the video stream.
    pub fn with_width(mut self, width: u32) -> Self {
        self.width = width as i32;
        self
    }

    /// Set the height of the video stream.
    pub fn with_height(mut self, height: u32) -> Self {
        self.height = height as i32;
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
    pub fn with_thread_count(mut self, thread_count: i32) -> Self {
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
        self.frame_rate = avutil::ra(num, den);
        self
    }

    /// Set the time base.
    pub fn with_time_base_ra(mut self, time_base: ffi::AVRational) -> Self {
        self.time_base = time_base;
        self
    }

    pub fn with_time_base(mut self, num: i32, den: i32) -> Self {
        self.time_base = avutil::ra(num, den);
        self
    }

    /// Set the packet time base.
    pub fn with_pkt_time_base_ra(mut self, pkt_time_base: ffi::AVRational) -> Self {
        self.pkt_time_base = pkt_time_base;
        self
    }

    pub fn with_pkt_time_base(mut self, num: i32, den: i32) -> Self {
        self.pkt_time_base = avutil::ra(num, den);
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

    pub fn with_nb_channels(mut self, nb_channels: u32) -> Self {
        self.nb_channels = nb_channels as i32;
        self
    }

    pub fn with_sample_rate(mut self, sample_rate: u32) -> Self {
        self.sample_rate = sample_rate as i32;
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
    fn apply_to(&self, encoder: &mut AVCodecContext, media_type: MediaType) {
        if media_type == MediaType::VIDEO {
            encoder.set_width(self.width);
            encoder.set_height(self.height);
            encoder.set_bit_rate(self.bit_rate);
            encoder.set_gop_size(self.gop_size);
            encoder.set_max_b_frames(self.max_b_frames);
            encoder.set_framerate(self.frame_rate);
            encoder.set_time_base(self.time_base);
            encoder.set_pkt_timebase(self.pkt_time_base);
            encoder.set_pix_fmt(self.pixel_format.into());
            encoder.set_sample_aspect_ratio(avutil::ra(1, 1));
        } else if media_type == MediaType::AUDIO {
            encoder.set_ch_layout(AVChannelLayout::from_nb_channels(self.nb_channels).into_inner());
            encoder.set_bit_rate(self.bit_rate);
            encoder.set_sample_rate(self.sample_rate);
            encoder.set_time_base(avutil::ra(1, self.sample_rate));
            encoder.set_sample_fmt(self.sample_format as _);
        } else {
            panic!("{}", format!("Unsupported media type:{:?}", media_type))
        }
        unsafe {
            (*encoder.as_mut_ptr()).thread_count = self.thread_count;
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
        let codec = {
            let codec_name = if let Some(codec_name) = &self.codec_name {
                codec_name.as_ref()
            } else {
                match self.media_type {
                    MediaType::VIDEO => Self::VIDEO_CODEC_NAME,
                    MediaType::AUDIO => Self::AUDIO_CODEC_NAME,
                    _ => panic!("Unsupported media type, please specify codec name."),
                }
            };
            AVCodec::find_encoder_by_name(&utils::from_str(codec_name))
                .context(format!(
                    "Failed to find encoder for codec: '{}'",
                    codec_name
                ))
                .unwrap()
        };

        let mut encode_ctx = AVCodecContext::new(&codec);

        // Some formats want stream headers to be separate.
        if self.oformat_flags & ffi::AVFMT_GLOBALHEADER as i32 != 0 {
            encode_ctx.set_flags(encode_ctx.flags | ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32);
        }

        self.apply_to(&mut encode_ctx, self.media_type);

        let hw_context = self
            .hw_device_config
            .filter(|_cfg| {
                // hardware acceleration enabled for video
                self.media_type == MediaType::VIDEO
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
                        ctx.setup_hw_frames(false, &mut encode_ctx, width, height)?;
                        Ok(ctx)
                    })
                    .context("Hardware acceleration context initialization failed")
            })
            .transpose()?;

        let dict = self.codec_opts.map(|opts| opts.into_dict());
        encode_ctx
            .open(dict)
            .context("Failed to open encode context")?;

        Ok(Encoder {
            encode_ctx,
            hw_context,
            frame_count: 0,
            media_type: self.media_type,
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
            frame_rate: avutil::ra(Self::FRAME_RATE, 1),
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
            thread_count: 0,
            codec_name: None,
            codec_opts: None,
            hw_device_config: None,
        }
    }
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
    encode_ctx: AVCodecContext,
    hw_context: Option<Arc<HWContext>>,
    media_type: MediaType,
    frame_count: u64,
    keyframe_interval: u64,
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
    pub fn new_video(width: u32, height: u32) -> Result<Encoder> {
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
        nb_channels: u32,
        sample_rate: u32,
        sample_format: SampleFormat,
    ) -> Result<Encoder> {
        EncoderBuilder::new_audio(128_000, nb_channels, sample_rate, sample_format).build()
    }

    /// Encode a single `ndarray` frame.
    ///
    /// # Arguments
    ///
    /// * `frame` - Frame to encode in `HWC` format and standard layout.
    /// * `source_timestamp` - Frame timestamp of original source. This is necessary to make sure
    ///   the output will be timed correctly.
    #[cfg(feature = "ndarray")]
    pub fn encode(&mut self, frame: &FrameArray, source_timestamp: crate::Time) -> EncodeResult {
        let (height, width, channels) = frame.dim();
        if height != self.encode_ctx.height as usize
            || width != self.encode_ctx.width as usize
            || channels != 3
        {
            return EncodeResult::Error(Error::msg("Invalid frame format."));
        }

        let mut frame = frame::ndarray_yuv_to_avframe(frame).unwrap();

        frame.set_pts(
            source_timestamp
                .aligned_with_rational(self.time_base())
                .into_value()
                .unwrap(),
        );

        match self.encode_raw(&frame) {
            EncodeRawResult::Packet(pkt) => EncodeResult::Packet(pkt),
            EncodeRawResult::Drain => EncodeResult::Drain,
            EncodeRawResult::Flushed => EncodeResult::Flushed,
            EncodeRawResult::Error(e) => EncodeResult::Error(e),
        }
    }

    /// Encode a single raw frame.
    ///
    /// # Arguments
    ///
    /// * `frame` - Frame to encode.
    pub fn encode_raw(&mut self, raw_frame: &RawFrame) -> EncodeRawResult {
        log::info!(
            "raw_frame: {:?}, time_base: {:?}",
            raw_frame,
            raw_frame.time_base
        );
        if raw_frame.width != self.encode_ctx.width || raw_frame.height != self.encode_ctx.height {
            return EncodeRawResult::Error(Error::msg(format!(
                "Invalid frame pixel format: {:?}, or dimensions: expected {}x{}, got {}x{}",
                PixelFormat::from(raw_frame.format),
                self.encode_ctx.width,
                self.encode_ctx.height,
                raw_frame.width,
                raw_frame.height
            )));
        }

        let mut av_frame = if self.media_type == MediaType::VIDEO {
            // get target pixel format for codec context,
            // if hardware acceleration is enabled, use the format of the hardware context
            // otherwise, YUV420P is used as the default format
            let target_format = self
                .hw_context
                .as_ref()
                .and_then(|_| self.encode_ctx.hw_frames_ctx_mut())
                .map(|mut ctx| PixelFormat::from(ctx.data().sw_format))
                .unwrap_or(PixelFormat::YUV420P);

            // Reformat frame to target pixel format if we need
            let sw_frame = if raw_frame.format != target_format.into() {
                swctx::scale_frame(raw_frame, raw_frame.width, raw_frame.height, target_format)
                    .unwrap()
            } else {
                raw_frame.clone()
            };

            match &self.hw_context {
                Some(hw_ctx) if hw_ctx.is_sw_frame(&sw_frame) => {
                    // sw_frame -> hw_frame
                    hw_ctx
                        .hw_upload(&mut self.encode_ctx, &sw_frame)
                        .map_err(|e| Error::msg(format!("Failed to upload frame: {}", e)))
                        .unwrap()
                }
                _ => sw_frame,
            }
        } else if self.media_type == MediaType::AUDIO {
            raw_frame.clone()
        } else {
            panic!(
                "{}",
                format!("Unsupported mediaType :{:?}", self.media_type)
            )
        };

        // Producer key frame every once in a while
        if self.frame_count % self.keyframe_interval == 0 {
            // not set for now
            // av_frame.set_pict_type(ffi::AV_PICTURE_TYPE_I);
        }
        av_frame.set_pict_type(ffi::AV_PICTURE_TYPE_NONE);

        log::debug!(
            "send encoder {:?}, time_base: {:?}",
            av_frame,
            av_frame.time_base
        );

        // send frame to encoder
        self.encode_ctx
            .send_frame(Some(&av_frame))
            .map_err(|e| Error::msg(format!("Failed to send frame: {}", e)))
            .unwrap();

        // Increment frame count regardless of whether frame is written,
        // see https://github.com/oddity-ai/video-rs/issues/46.
        self.frame_count += 1;

        self.receive_packet()
    }

    /// Get encoder time base.
    #[inline]
    pub fn time_base(&self) -> ffi::AVRational {
        self.encode_ctx.time_base
    }

    #[inline]
    pub fn frame_rate(&self) -> ffi::AVRational {
        self.encode_ctx.framerate
    }

    #[inline]
    pub fn sample_rate(&self) -> i32 {
        self.encode_ctx.sample_rate
    }

    #[inline]
    pub fn width(&self) -> i32 {
        self.encode_ctx.width
    }

    #[inline]
    pub fn height(&self) -> i32 {
        self.encode_ctx.height
    }

    #[inline]
    pub fn pix_fmt(&self) -> PixelFormat {
        self.encode_ctx.pix_fmt.into()
    }

    #[inline]
    pub fn media_type(&self) -> MediaType {
        self.media_type
    }

    #[inline]
    pub fn codecpar(&self) -> AVCodecParameters {
        self.encode_ctx.extract_codecpar()
    }

    #[inline]
    pub fn ch_layout(&self) -> AVChannelLayoutRef {
        self.encode_ctx.ch_layout()
    }

    /// Pull an encoded packet from the decoder. This function also handles the possible `EAGAIN`
    /// result, in which case we just need to go again.
    fn receive_packet(&mut self) -> EncodeRawResult {
        match self.encode_ctx.receive_packet() {
            Ok(pkt) => EncodeRawResult::Packet(pkt),
            Err(rsmpeg::error::RsmpegError::EncoderDrainError) => {
                log::debug!("Encoder drained, try send new frame again.");
                EncodeRawResult::Drain
            }
            Err(rsmpeg::error::RsmpegError::EncoderFlushedError) => {
                log::debug!("Encoder flushed, EOF reached.");
                EncodeRawResult::Flushed
            }
            Err(err) => EncodeRawResult::Error(Error::new(err)),
        }
    }

    /// Flush the encoder, drain any packets that still need processing.
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
        if self.encode_ctx.codec().capabilities & ffi::AV_CODEC_CAP_DELAY as i32 == 0 {
            return Ok(());
        }

        // Notify the encoder that the last frame has been sent.
        self.send_eof().context("Send EOF frame failed.")?;

        // drain the items still on the queue before giving up.
        loop {
            match self.receive_packet() {
                EncodeRawResult::Packet(mut packet) => {
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
                EncodeRawResult::Drain => {
                    log::debug!("Encoder drained, try send new frame again.");
                    continue;
                }
                EncodeRawResult::Flushed => {
                    log::debug!("Encoder flushed, EOF reached.");
                    break;
                }
                EncodeRawResult::Error(e) => {
                    log::debug!("Encode error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// encode context send EOF for flush encoder
    fn send_eof(&mut self) -> Result<()> {
        Ok(self.encode_ctx.send_frame(None)?)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // let _ = self.flush();
    }
}

unsafe impl Send for Encoder {}
unsafe impl Sync for Encoder {}

/// encode_raw result
#[derive(Debug)]
pub enum EncodeRawResult {
    /// encoder packet
    Packet(AVPacket),
    /// encoder drained
    Drain,
    /// encoder
    Flushed,
    /// encoder error
    Error(Error),
}

/// encode result
#[cfg(feature = "ndarray")]
pub type EncodeResult = EncodeRawResult;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::private::{Output, Write};
    use crate::stream::StreamInfo;
    use crate::StreamWriterBuilder;
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
        use crate::FrameArray;

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
        let codec = AVCodec::find_encoder_by_name(&codec_name).expect("Failed to find encoder");

        let supported_frame_rates = codec.supported_framerates().map(|rates| rates.to_vec());
        let supported_pix_fmts = codec
            .pix_fmts()
            .unwrap_or(&[])
            .iter()
            .filter(|&&fmt| fmt != ffi::AV_PIX_FMT_NONE)
            .cloned()
            .collect();

        let config = VideoFormatParams {
            time_base,
            codec_name: codec.name().to_str().unwrap().to_string(),
            supported_frame_rates,
            supported_pix_fmts,
            codec_options,
            format_options,
        };

        // 创建编码器
        let mut encoder = EncoderBuilder::new_video(1280, 720)
            .with_time_base(time_base.0, time_base.1)
            .with_codec_name(Some(config.codec_name))
            .with_options(config.codec_options.map(|opts| opts.into()))
            .build()?;

        // 确定输出路径和扩展名
        let output_file = format!("/tmp/test_encode_video.{}", container_type);
        let output_path = Path::new(output_file.as_str());

        // 创建流写入器
        let mut stream_writer = StreamWriterBuilder::new(output_path)
            .with_options(config.format_options.map(|opts| opts.into()))
            .build()?;
        let video_index = stream_writer.add_stream(encoder.codecpar(), encoder.time_base());
        let stream_info = StreamInfo::from_writer(&stream_writer, video_index)?;

        // 输出实际使用的时间基（可能与请求的不同）
        println!(
            "Requested timebase: {}/{}, Actual encoder timebase: {}/{}, Stream timebase: {}/{}",
            time_base.0,
            time_base.1,
            encoder.time_base().num,
            encoder.time_base().den,
            stream_info.time_base.num,
            stream_info.time_base.den
        );

        // write header
        stream_writer.write_header()?;

        fn rainbow_frame(p: f32) -> FrameArray {
            use crate::colors;
            let rgb = colors::hsv_to_rgb(p * 360.0, 100.0, 100.0);
            FrameArray::from_shape_fn((720, 1280, 3), |(_y, _x, c)| rgb[c])
        }

        let actual_timebase = encoder.time_base();
        let frame_duration_seconds = 1.0 / fps;

        // 将秒转换为对应时间基单位
        let duration_units = (frame_duration_seconds * actual_timebase.den as f64
            / actual_timebase.num as f64)
            .round() as i64;

        let duration = Time::new(Some(duration_units), actual_timebase);

        // 初始化position时使用正确的时间基
        let mut position = Time::new(Some(0), avutil::ra(time_base.0, time_base.1));

        println!(
            "Encoding {} with actual timebase: {}/{}, duration units: {}, fps: {}",
            container_type, actual_timebase.num, actual_timebase.den, duration_units, fps
        );

        // 帧编码并写入文件
        for i in 0..10 {
            let frame = rainbow_frame(i as f32 / 10.0);

            match encoder.encode(&frame, position) {
                EncodeResult::Packet(mut packet) => {
                    packet.set_pos(-1);
                    packet.set_stream_index(video_index as i32);

                    // 将编码器输出的数据包时间戳，从编码器时间基转换到输出流时间基
                    // encode_ctx_timebase => out_stream_time_base
                    let orig_pts = packet.pts;
                    packet.rescale_ts(encoder.time_base(), stream_info.time_base);
                    let new_pts = packet.pts;
                    // 只打印少量帧以避免日志过多
                    if i < 5 || i % 30 == 0 {
                        println!("Frame {}: orig_pts={}, new_pts={}", i, orig_pts, new_pts);
                    }

                    stream_writer.write_frame(&mut packet)?;
                }
                EncodeResult::Drain => {
                    println!("Encoder drained, try send new frame again.");
                    continue;
                }
                EncodeResult::Flushed => {
                    println!("Encoder flushed, EOF reached.");
                    break;
                }
                EncodeResult::Error(e) => {
                    println!("Encode error: {}", e);
                    break;
                }
            }

            // 使用aligned_with确保时间基一致进行加法操作
            position = position.aligned_with(duration).add();
        }

        // flush encoder
        encoder.flush(
            &mut stream_writer,
            false,
            video_index,
            stream_info.time_base,
        )?;

        // write trailer
        stream_writer.write_trailer()?;

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
