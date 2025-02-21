use crate::flags::{AvCodecFlags, AvFormatFlags};
#[cfg(feature = "ndarray")]
use crate::frame::{self, FrameArray};
use crate::hwaccel::{HWContext, HWDeviceType};
use crate::io::private::Write;
use crate::io::{Writer, WriterBuilder};
use crate::location::Location;
use crate::options::Options;
use crate::packet::Packet;
use crate::pixel::PixelFormat;
use crate::time::Time;
use crate::{Rational, RawFrame};

use rsmpeg::avcodec::{AVCodec, AVCodecContext, AVCodecRef};
use rsmpeg::avutil::AVPixelFormat;
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi;

use anyhow::{Context, Error, Result};
use libc::c_uint;
use std::ffi::CString;

/// Builds an [`Encoder`].
pub struct EncoderBuilder<'a> {
    destination: Location,
    settings: Settings,
    options: Option<&'a Options>,
    format: Option<&'a str>,
    interleaved: bool,
    hw_device_type: Option<HWDeviceType>,
}

impl<'a> EncoderBuilder<'a> {
    /// Create an encoder with the specified destination and settings.
    ///
    /// * `destination` - Where to encode to.
    /// * `settings` - Encoding settings.
    pub fn new(destination: impl Into<Location>, settings: Settings) -> Self {
        Self {
            destination: destination.into(),
            settings,
            options: None,
            format: None,
            interleaved: false,
            hw_device_type: None,
        }
    }

    /// Set the output options for the encoder.
    ///
    /// # Arguments
    ///
    /// * `options` - The output options.
    pub fn with_options(mut self, options: &'a Options) -> Self {
        self.options = Some(options);
        self
    }

    /// Set the container format for the encoder.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use. eg. `"mp4"`, `"mkv"`, `"mov"`, `"avi"`, `"flv"`.
    ///
    /// reference: https://trac.ffmpeg.org/wiki/HWAccelIntro
    ///
    /// | Format                          | Filename Extension | H.264/AVC | H.265/HEVC | AV1   |
    /// |---------------------------------|--------------------|-----------|------------|-------|
    /// | Matroska                        | .mkv               | Y         | Y          | Y     |
    /// | MPEG-4 Part 14 (MP4)            | .mp4               | Y         | Y          | Y     |
    /// | Audio Video Interleave (AVI)    | .avi               | Y         | N          | Y     |
    /// | Material Exchange Format (MXF)  | .mxf               | Y         | n/a        | n/a   |
    /// | MPEG transport stream (TS)      | .ts                | Y         | Y          | N     |
    /// | 3GPP (3GP)                      | .3gp               | Y         | n/a        | n/a   |
    /// | Flash Video (FLV)               | .flv               | Y         | n/a        | n/a   |
    /// | WebM                            | .webm              | n/a       | n/a        | Y     |
    /// | Advanced Systems Format (ASF)   | .asf, .wmv         | Y         | Y          | Y     |
    /// | QuickTime File Format (QTFF)    | .mov               | Y         | Y          | n/a   |
    pub fn with_format(mut self, format: &'a str) -> Self {
        self.format = Some(format);
        self
    }

    /// Set interleaved. This will cause the encoder to use interleaved write instead of normal
    /// write.
    pub fn interleaved(mut self) -> Self {
        self.interleaved = true;
        self
    }

    /// Enable hardware acceleration with the specified device type.
    ///
    /// * `device_type` - Device to use for hardware acceleration.
    pub fn with_hardware_device(mut self, device_type: HWDeviceType) -> Self {
        self.hw_device_type = Some(device_type);
        self
    }

    /// Build an [`Encoder`].
    pub fn build(self) -> Result<Encoder> {
        let mut writer_builder = WriterBuilder::new(self.destination);
        if let Some(options) = self.options {
            writer_builder = writer_builder.with_options(options);
        }
        if let Some(format) = self.format {
            writer_builder = writer_builder.with_format(format);
        }
        Encoder::from_writer(
            writer_builder.build()?,
            self.interleaved,
            self.settings,
            self.hw_device_type,
        )
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
    writer: Writer,
    writer_stream_index: usize,
    encoder: AVCodecContext,
    hw_context: Option<HWContext>,
    keyframe_interval: u64,
    interleaved: bool,
    frame_count: u64,
    have_written_header: bool,
    have_written_trailer: bool,
}

impl Encoder {
    /// Create an encoder with the specified destination and settings.
    ///
    /// * `destination` - Where to encode to.
    /// * `settings` - Encoding settings.
    #[inline]
    pub fn new(destination: impl Into<Location>, settings: Settings) -> Result<Self> {
        EncoderBuilder::new(destination, settings).build()
    }

    /// Create an encoder from a `FileWriter` instance.
    ///
    /// # Arguments
    ///
    /// * `writer` - [`Writer`] to create encoder from.
    /// * `interleaved` - Whether or not to use interleaved write.
    /// * `settings` - Encoder settings to use.
    fn from_writer(
        mut writer: Writer,
        interleaved: bool,
        settings: Settings,
        hw_device_type: Option<HWDeviceType>,
    ) -> Result<Self> {
        let global_header =
            AvFormatFlags::from_bits_truncate(writer.output.oformat().flags as c_uint)
                .contains(AvFormatFlags::GLOBAL_HEADER);

        let codec = match settings.codec() {
            None => return Err(Error::msg("Invalid codec parameters.")),
            Some(c) => c,
        };
        let mut encode_ctx = AVCodecContext::new(&codec);

        // Some formats require this flag to be set or the output will
        // not be playable by dumb players.
        if global_header {
            encode_ctx.set_flags(AvCodecFlags::GLOBAL_HEADER.bits() as i32);
        }

        settings.apply_to(&mut encode_ctx);
        let (width, height) = (encode_ctx.width, encode_ctx.height);

        let hw_context = match hw_device_type {
            Some(device_type) => {
                let mut hw_ctx = HWContext::new(device_type.auto_best_device()?)
                    .context("Hardware acceleration context initialization failed.")?;
                hw_ctx
                    .setup_hw_frames(&mut encode_ctx, width, height)
                    .unwrap();
                Some(hw_ctx)
            }
            None => None,
        };

        encode_ctx
            .open(Some(settings.options().to_dict().av_dict()))
            .context("Could not open encode context")?;

        let writer_stream_index = {
            let mut out_stream = writer.output.new_stream();
            out_stream.set_codecpar(encode_ctx.extract_codecpar());
            out_stream.set_time_base(encode_ctx.time_base);
            out_stream.index as usize
        };

        Ok(Self {
            writer,
            writer_stream_index,
            encoder: encode_ctx,
            hw_context,
            keyframe_interval: settings.keyframe_interval,
            interleaved,
            frame_count: 0,
            have_written_header: false,
            have_written_trailer: false,
        })
    }

    /// Encode a single `ndarray` frame.
    ///
    /// # Arguments
    ///
    /// * `frame` - Frame to encode in `HWC` format and standard layout.
    /// * `source_timestamp` - Frame timestamp of original source. This is necessary to make sure
    ///   the output will be timed correctly.
    #[cfg(feature = "ndarray")]
    pub fn encode(&mut self, frame: &FrameArray, source_timestamp: Time) -> Result<()> {
        let (height, width, channels) = frame.dim();
        if height != self.encoder.height as usize
            || width != self.encoder.width as usize
            || channels != 3
        {
            return Err(Error::msg("Invalid frame format."));
        }

        let mut frame = frame::ndarray_yuv_to_avframe(frame).unwrap();

        frame.set_pts(
            source_timestamp
                .aligned_with_rational(self.time_base())
                .into_value()
                .unwrap(),
        );

        self.encode_raw(&frame)
    }

    /// Encode a single raw frame.
    ///
    /// # Arguments
    ///
    /// * `frame` - Frame to encode.
    pub fn encode_raw(&mut self, raw_frame: &RawFrame) -> Result<()> {
        if raw_frame.width != self.encoder.width || raw_frame.height != self.encoder.height {
            return Err(Error::msg("Invalid frame format."));
        }

        // Write file header if we hadn't done that yet.
        if !self.have_written_header {
            self.writer.write_header()?;
            self.have_written_header = true;
        }

        // 根据编码器类型选择目标像素格式
        let target_format = if self.hw_context.is_some() {
            // 使用硬件编码器支持的输入格式
            self.encoder
                .hw_frames_ctx_mut()
                .map(|mut ctx| PixelFormat::from_raw(ctx.data().sw_format).unwrap())
                .unwrap_or(PixelFormat::YUV420P)
        } else {
            PixelFormat::YUV420P
        };

        // Reformat frame to target pixel format if need
        let mut frame = if raw_frame.format != target_format.into_raw() {
            frame::convert_avframe(raw_frame, raw_frame.width, raw_frame.height, target_format)
                .unwrap()
        } else {
            raw_frame.clone()
        };

        // Producer key frame every once in a while
        if self.frame_count % self.keyframe_interval == 0 {
            frame.set_pict_type(ffi::AV_PICTURE_TYPE_I);
        }

        // 发送帧到编码器
        match self.hw_context.as_ref() {
            Some(hw_ctx) => {
                // 上传到硬件内存并获取硬件帧
                let hw_frame = {
                    if hw_ctx.is_hw_frame(&frame) {
                        frame
                    } else {
                        hw_ctx
                            .upload_frame(&mut self.encoder, &frame)
                            .map_err(|e| Error::msg(format!("Failed to upload frame: {}", e)))?
                    }
                };

                // 发送硬件帧到编码器
                self.encoder
                    .send_frame(Some(&hw_frame))
                    .map_err(|e| Error::msg(format!("Failed to send hardware frame: {}", e)))?;
            }
            None => {
                // 软件编码
                self.encoder
                    .send_frame(Some(&frame))
                    .map_err(|e| Error::msg(format!("Failed to send frame: {}", e)))?;
            }
        }

        // Increment frame count regardless of whether or not frame is written,
        // see https://github.com/oddity-ai/video-rs/issues/46.
        self.frame_count += 1;

        if let Some(packet) = self.encoder_receive_packet().unwrap() {
            self.write(packet)?;
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
            self.flush().unwrap();
            self.writer.write_trailer()?;
        }

        Ok(())
    }

    /// Get encoder time base.
    #[inline]
    pub fn time_base(&self) -> Rational {
        self.encoder.time_base.into()
    }

    #[inline]
    pub fn width(&self) -> i32 {
        self.encoder.width
    }

    #[inline]
    pub fn height(&self) -> i32 {
        self.encoder.height
    }

    #[inline]
    pub fn pix_fmt(&self) -> AVPixelFormat {
        self.encoder.pix_fmt
    }

    /// Pull an encoded packet from the decoder. This function also handles the possible `EAGAIN`
    /// result, in which case we just need to go again.
    fn encoder_receive_packet(&mut self) -> Result<Option<Packet>> {
        let packet = match self.encoder.receive_packet() {
            Ok(p) => Packet::new_with_avpacket(p),
            Err(RsmpegError::EncoderDrainError) | Err(RsmpegError::EncoderFlushedError) => {
                return Ok(None);
            }
            Err(err) => return Err(Error::new(err)),
        };
        Ok(Some(packet))
    }

    /// Acquire the time base of the output stream.
    fn stream_time_base(&mut self) -> Rational {
        self.writer
            .output
            .streams()
            .get(self.writer_stream_index)
            .unwrap()
            .time_base
            .into()
    }

    /// Write encoded packet to output stream.
    ///
    /// # Arguments
    ///
    /// * `packet` - Encoded packet.
    fn write(&mut self, mut packet: Packet) -> Result<()> {
        packet.set_stream_index(self.writer_stream_index);
        packet.set_position(-1);
        packet.rescale_ts(self.time_base(), self.stream_time_base());
        if self.interleaved {
            self.writer.write_interleaved(&mut packet).unwrap();
        } else {
            self.writer.write_frame(&mut packet).unwrap();
        };

        Ok(())
    }

    /// Flush the encoder, drain any packets that still need processing.
    fn flush(&mut self) -> Result<()> {
        // Maximum number of invocations to `encoder_receive_packet`
        // to drain the items still on the queue before giving up.
        const MAX_DRAIN_ITERATIONS: u32 = 100;

        // Notify the encoder that the last frame has been sent.
        self.send_eof()?;

        // We need to drain the items still in the encoders queue.
        for _ in 0..MAX_DRAIN_ITERATIONS {
            match self.encoder_receive_packet() {
                Ok(Some(packet)) => self.write(packet).unwrap(),
                Ok(None) => continue,
                Err(_) => break,
            }
        }

        Ok(())
    }

    /// 发送一个空帧来刷新编码器 EOF
    fn send_eof(&mut self) -> Result<()> {
        Ok(self.encoder.send_frame(None)?)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

unsafe impl Send for Encoder {}
unsafe impl Sync for Encoder {}

/// Holds a logical combination of encoder settings.
#[derive(Debug, Clone)]
pub struct Settings {
    width: i32,
    height: i32,
    bit_rate: i64,
    gop_size: i32,
    frame_rate: Rational,
    time_base: Rational,
    max_b_frames: i32,
    keyframe_interval: u64,
    thread_count: i32,
    options: Options,
    codec_name: Option<String>,
    pixel_format: PixelFormat,
}

impl Settings {
    /// Default keyframe interval.
    const KEY_FRAME_INTERVAL: u64 = 12;

    /// This is the assumed FPS for the encoder to use. Note that this does not need to be correct
    /// exactly.
    const FRAME_RATE: i32 = 30;

    /// Default bit rate.
    /// 分辨率(width, height) + 推荐比特率（单位：bps）
    /// * 标清 Sd_480p:          (640, 480)   => 1_000_000,   // 1 Mbps
    /// * 高清 Hd_720p:          (1280, 720)  => 2_500_000,   // 2.5 Mbps
    /// * 全高清 FullHd(1080p):  (1920, 1080) => 5_000_000,   // 5 Mbps
    /// * 超高清 FullHd_2k:      (2560, 1440) => 8_000_000,   // 8 Mbps
    /// * 超高清 UltraHd_4K:     (3840, 2160) => 20_000_000,  // 20 Mbps
    /// * 超高清 FullUltraHd_8K: (7680, 4320) => 60_000_000,  // 60 Mbps
    const BIT_RATE: i64 = 1_000_000;

    /// default codec
    const CODEC_NAME: &'static str = "libx264";

    /// Create encoder settings for an H264 stream with YUV420p pixel format. This will encode to
    /// arguably the most widely compatible video file since H264 is a common codec and YUV420p is
    /// the most commonly used pixel format.
    pub fn preset_h264_yuv420p(width: i32, height: i32, realtime: bool) -> Settings {
        let options = if realtime {
            Options::preset_h264_realtime()
        } else {
            Options::preset_h264()
        };

        Self {
            width,
            height,
            gop_size: 10,
            max_b_frames: 1,
            thread_count: 0,
            codec_name: None,
            bit_rate: Self::BIT_RATE,
            frame_rate: Rational::new(1, Self::FRAME_RATE),
            time_base: Rational::new(1, Self::FRAME_RATE * 1000),
            keyframe_interval: Self::KEY_FRAME_INTERVAL,
            pixel_format: PixelFormat::YUV420P,
            options,
        }
    }

    /// Create encoder settings for an H264 stream with a custom pixel format and options.
    /// This allows for greater flexibility in encoding settings, enabling specific requirements
    /// or optimizations to be set depending on the use case.
    ///
    /// # Arguments
    ///
    /// * `width` - The width of the video stream.
    /// * `height` - The height of the video stream.
    /// * `pixel_format` - The desired pixel format for the video stream.
    /// * `options` - Custom H264 encoding options.
    ///
    /// # Return value
    ///
    /// A `Settings` instance with the specified configuration.+
    pub fn preset_h264_custom(
        width: i32,
        height: i32,
        pixel_format: PixelFormat,
        options: Options,
    ) -> Settings {
        Self {
            width,
            height,
            gop_size: 10,
            max_b_frames: 1,
            thread_count: 0,
            codec_name: None,
            bit_rate: Self::BIT_RATE,
            frame_rate: Rational::new(1, Self::FRAME_RATE),
            time_base: Rational::new(1, Self::FRAME_RATE * 1000),
            pixel_format,
            keyframe_interval: Self::KEY_FRAME_INTERVAL,
            options,
        }
    }

    /// Set the codec name.
    pub fn with_codec_name(mut self, codec_name: String) -> Self {
        self.codec_name = Some(codec_name);
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
    pub fn with_frame_rate(mut self, frame_rate: Rational) -> Self {
        self.frame_rate = frame_rate;
        self.time_base = Rational::new(1, frame_rate.numerator() * 1000);
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

    /// Set the options.
    pub fn with_options(mut self, options: Options) -> Self {
        self.options = options;
        self
    }

    /// Get encoder options.
    pub fn options(&self) -> &Options {
        &self.options
    }

    /// Get codec.
    pub fn codec(&self) -> Option<AVCodecRef> {
        // Try to use the default libx264 encoder
        match &self.codec_name {
            Some(codec) => AVCodec::find_encoder_by_name(&CString::new(codec.to_string()).unwrap()),
            None => AVCodec::find_encoder_by_name(&CString::new(Self::CODEC_NAME).unwrap()),
        }
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
    fn apply_to(&self, encoder: &mut AVCodecContext) {
        encoder.set_width(self.width);
        encoder.set_height(self.height);
        encoder.set_bit_rate(self.bit_rate);
        encoder.set_gop_size(self.gop_size);
        encoder.set_max_b_frames(self.max_b_frames);
        encoder.set_framerate(self.frame_rate.into());
        encoder.set_time_base(self.time_base.into());
        encoder.set_pix_fmt(self.pixel_format.into_raw());
        unsafe {
            (*encoder.as_mut_ptr()).thread_count = self.thread_count;
        }
    }
}
