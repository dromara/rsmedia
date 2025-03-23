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

    /// Create an encoder with the specified destination and settings.
    ///
    /// * `destination` - Where to encode to.
    /// * `width` - The width of the video stream.
    /// * `height` - The height of the video stream.
    /// * `pixel_format` - The desired pixel format for the video stream.
    /// * `options` - Custom H264 encoding options.
    pub fn new() -> Self {
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

    pub fn with_video_size(mut self, width: u32, height: u32) -> Self {
        self.width = width as i32;
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
            .filter(|cfg| {
                // hardware acceleration enabled for video
                let is_video = self.media_type == MediaType::VIDEO;
                // codec support or not for hardware acceleration
                let hw_pixel = cfg
                    .device_type
                    .find_hw_pixel_format_with_codec(&codec)
                    .ok_or_else(|| {
                        let codec_name = utils::to_string(codec.name()).unwrap();
                        Error::msg(format!(
                            "HW acceleration encoder not supported for codec: {codec_name}"
                        ))
                    });
                is_video && hw_pixel.is_ok()
            })
            .map(|cfg| {
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
        Self::new()
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
    /// Create an encoder with the specified destination and settings.
    ///
    /// * `destination` - Where to encode to.
    /// * `settings` - Encoding settings.
    #[inline]
    pub fn new() -> Result<Encoder> {
        EncoderBuilder::new().build()
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
    use crate::io::StreamWriter;
    use crate::stream::StreamInfo;
    use std::path::Path;

    #[test]
    #[cfg(feature = "ndarray")]
    #[ignore = "ignore video output file"]
    fn test_encode_video() -> Result<()> {
        use crate::time::Time;
        use crate::FrameArray;

        let output_path = Path::new("/tmp/h264_encode_video.mp4");

        let mut encoder = EncoderBuilder::new()
            .with_video_size(1280, 720)
            .with_media_type(MediaType::VIDEO)
            .build()?;

        // build writer
        let mut stream_writer = StreamWriter::new(output_path)?;
        let video_index = stream_writer.add_stream(encoder.codecpar(), encoder.time_base());
        let stream_info = StreamInfo::from_writer(&stream_writer, video_index).unwrap();

        // write header
        stream_writer.write_header().unwrap();

        let duration: Time = Time::from_nth_of_a_second(24);
        let mut position = Time::zero();

        fn rainbow_frame(p: f32) -> FrameArray {
            use crate::colors;
            let rgb = colors::hsv_to_rgb(p * 360.0, 100.0, 100.0);
            FrameArray::from_shape_fn((720, 1280, 3), |(_y, _x, c)| rgb[c])
        }

        // frame encode and write to file
        for i in 0..256 {
            let frame = rainbow_frame(i as f32 / 256.0);

            match encoder.encode(&frame, position) {
                EncodeResult::Packet(mut packet) => {
                    packet.set_pos(-1);
                    packet.set_stream_index(video_index as i32);
                    // 将编码器输出的数据包时间戳，从编码器时间基转换到输出流时间基
                    // encode_ctx_timebase => out_stream_time_base
                    packet.rescale_ts(encoder.time_base(), stream_info.time_base);
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

            println!("Encoded frame {} at position {:?}", i, position);

            position = position.aligned_with(duration).add();
        }

        // flush encoder and write trailer
        encoder
            .flush(
                &mut stream_writer,
                false,
                video_index,
                stream_info.time_base,
            )
            .unwrap();

        // write trailer
        stream_writer.write_trailer().unwrap();

        Ok(())
    }

    /// 音频采样率
    const DEFAULT_SAMPLE_RATE: u32 = 44_100;
    /// 比特率
    const DEFAULT_BIT_RATE: i64 = 128_000;
    /// 正弦波振幅
    const SAFE_AMPLITUDE: f32 = 0.7;
    /// 时长(秒)
    const FADE_DURATION_MS: u32 = 10;
    /// 生成带淡入淡出效果的浮点型正弦波音频帧
    ///
    /// # 参数说明
    /// - `frequency`: 正弦波基础频率，单位赫兹(Hz)，有效范围 (0, sample_rate/2]
    /// - `channels`: 音频声道数量，支持范围 [1, 8] 个声道
    /// - `nb_samples`: 单个音频帧包含的样本数量，单位：采样点/帧
    /// - `sample_rate`: 音频采样率，单位Hz，常见值：44100(CD)、48000(专业音频)
    ///
    /// # 返回值
    /// 返回包装好的原始音频帧(RawFrame)，数据格式为平面浮点型(FLTP)
    ///
    /// # 数据格式说明
    /// FLTP格式特点：
    /// - 每个声道数据存储在独立的内存平面
    /// - 采样值范围：[-1.0, 1.0]，超出会导致削波失真
    /// - 内存布局示例（立体声）：
    ///   data[0]: [L0, L1, L2,...] 左声道数据
    ///   data[1]: [R0, R1, R2,...] 右声道数据
    fn generate_sine_wave_frame(
        frequency: f32,
        channels: usize,
        nb_samples: i32,
        sample_rate: u32,
    ) -> Result<RawFrame> {
        // 奈奎斯特频率限制：频率不能超过采样率的一半
        anyhow::ensure!(
            frequency > 0.0 && frequency <= (sample_rate as f32 / 2.0),
            "无效频率：{}Hz (采样率 {}Hz 下最高支持 {}Hz)",
            frequency,
            sample_rate,
            sample_rate as f32 / 2.0
        );

        // 声道数限制：支持1到8声道
        anyhow::ensure!(
            channels > 0 && channels <= 8,
            "不支持的声道数量：{} (支持1-8声道)",
            channels
        );

        let mut frame = RawFrame::new();

        // 设置帧参数
        frame.set_nb_samples(nb_samples); // 每帧样本数
        frame.set_format(SampleFormat::FLTP as i32); // 强制指定为平面浮点格式
        frame.set_sample_rate(sample_rate as i32); // 设置采样率
        frame.set_ch_layout(AVChannelLayout::from_nb_channels(channels as i32).into_inner());

        // 分配音频数据缓冲区
        frame
            .alloc_buffer()
            .context("Failed alloc audio frame buffer.")?;

        // 计算音频生成参数
        let sample_interval = 1.0 / sample_rate as f32; // 单个采样时间间隔（秒）
        let two_pi_f = 2.0 * std::f32::consts::PI * frequency; // 角频率计算 2πf

        // 淡入淡出参数
        let fade_samples = (sample_rate as f32 * FADE_DURATION_MS as f32 / 1000.0).round() as usize;
        let total_samples = nb_samples as usize; // 总采样点数

        // 分声道生成数据
        for ch in 0..channels {
            // 获取当前声道的平面数据指针
            // SAFETY: 帧缓冲区在frame生命周期内保持有效
            let data_ptr = unsafe {
                std::slice::from_raw_parts_mut(
                    (*frame.as_mut_ptr()).data[ch] as *mut f32,
                    total_samples,
                )
            };

            // 生成每个采样点的数据
            for (i, sample) in data_ptr.iter_mut().enumerate() {
                let t = i as f32 * sample_interval; // 当前采样时间点
                let value = (two_pi_f * t).sin(); // 计算正弦波值

                // 应用淡入淡出窗口函数
                let window = match i {
                    // 前 fade_samples 个采样：线性淡入
                    i if i < fade_samples => i as f32 / fade_samples as f32,
                    // 最后 fade_samples 个采样：线性淡出
                    i if i >= total_samples - fade_samples => {
                        (total_samples - i) as f32 / fade_samples as f32
                    }
                    // 中间部分：全振幅
                    _ => 1.0,
                };

                // 设置采样值并限制振幅
                *sample = value * SAFE_AMPLITUDE * window;
            }
        }

        Ok(frame)
    }

    #[test]
    #[ignore = "ignore audio output file"]
    fn test_encode_audio() -> Result<()> {
        let output_path = Path::new("/tmp/aac_encode_audio.aac");

        let mut encoder = EncoderBuilder::new()
            .with_media_type(MediaType::AUDIO) // 指定音频编码
            .with_nb_channels(2) // 立体声
            .with_sample_rate(DEFAULT_SAMPLE_RATE) // 采样率
            .with_bit_rate(DEFAULT_BIT_RATE) // 128kbps 比特率
            .with_sample_format(SampleFormat::FLTP) // 平面浮点格式
            .with_codec_name(Some("aac".to_string())) // 指定AAC编码
            .build()
            .unwrap();

        let mut stream_writer = StreamWriter::new(output_path)?;
        let audio_index = stream_writer.add_stream(encoder.codecpar(), encoder.time_base());
        let stream_info = StreamInfo::from_writer(&stream_writer, audio_index)?;
        // 写入文件头
        stream_writer.write_header().unwrap();

        // 音频生成参数
        let duration_secs = 5; // 总时长5秒
        let samples_per_second = DEFAULT_SAMPLE_RATE as i64; // 每秒采样数
        let total_samples = duration_secs * samples_per_second; // 总采样数
        let samples_per_frame = 1024; // 每帧采样数（AAC标准帧长）
        let frequency = 440.0; // 正弦波基础频率

        for frame_idx in 0..(total_samples / samples_per_frame as i64) {
            let mut sine_frame = generate_sine_wave_frame(
                frequency, // A4标准音高（国际标准音）
                2,         // 立体声
                samples_per_frame,
                DEFAULT_SAMPLE_RATE,
            )
            .context("音频帧生成失败")?;

            // 设置精确时间戳（单位：采样点数）
            // 每个时间戳增量对应一帧的持续时间
            // 例如：1024 samples/frame => 每帧时间戳增量为1024
            sine_frame.set_pts(frame_idx * samples_per_frame as i64);

            match encoder.encode_raw(&sine_frame) {
                EncodeRawResult::Packet(mut packet) => {
                    packet.set_pos(-1);
                    packet.set_stream_index(audio_index as i32);
                    // 将编码器输出的数据包时间戳，从编码器时间基转换到输出流时间基
                    // encode_ctx_timebase => out_stream_time_base
                    packet.rescale_ts(encoder.time_base(), stream_info.time_base);
                    stream_writer.write_frame(&mut packet)?;
                }
                EncodeRawResult::Drain => {
                    println!("Encoder drained, try send new frame again.");
                    continue;
                }
                EncodeRawResult::Flushed => {
                    println!("Encoder flushed, EOF reached.");
                    break;
                }
                EncodeRawResult::Error(e) => {
                    println!("Encode error: {}", e);
                    break;
                }
            }
        }

        // 处理剩余不足一帧的样本
        let remaining = (total_samples % samples_per_frame as i64) as i32;
        if remaining > 0 {
            let mut last_frame = generate_sine_wave_frame(
                frequency,
                2,
                remaining, // 最后剩余的样本数
                DEFAULT_SAMPLE_RATE,
            )?;

            // 设置最后帧的时间戳（总样本数 - 剩余样本数）
            last_frame.set_pts(total_samples - remaining as i64);

            // write last frame
            if let EncodeRawResult::Packet(mut packet) = encoder.encode_raw(&last_frame) {
                packet.set_pos(-1);
                packet.set_stream_index(audio_index as i32);
                // 将编码器输出的数据包时间戳，从编码器时间基转换到输出流时间基
                // encode_ctx_timebase => out_stream_time_base
                packet.rescale_ts(encoder.time_base(), stream_info.time_base);
                stream_writer.write_frame(&mut packet)?;
            }
        }

        // flush encoder and write trailer
        encoder
            .flush(
                &mut stream_writer,
                false,
                audio_index,
                stream_info.time_base,
            )
            .unwrap();

        // write trailer
        stream_writer.write_trailer().unwrap();

        Ok(())
    }
}
