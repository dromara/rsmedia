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
use crate::{Rational, RawFrame, utils};

use rsmpeg::avcodec::{AVCodec, AVCodecContext, AVCodecRef};
use rsmpeg::avutil::{self, AVPixelFormat};
use rsmpeg::error::RsmpegError;
use rsmpeg::{UnsafeDerefMut, ffi};

use anyhow::{Context, Error, Result};
use libc::{c_int, c_uint};

/// Builds an [`Encoder`].
pub struct EncoderBuilder<'a> {
    width: i32,
    height: i32,
    bit_rate: i64,
    gop_size: i32,
    interleaved: bool,
    time_base: Rational,
    frame_rate: Rational,
    max_b_frames: i32,
    thread_count: i32,
    keyframe_interval: u64,
    pixel_format: PixelFormat,
    destination: Location,
    /// container format
    format: Option<&'a str>,
    codec_name: Option<String>,
    options: Option<&'a Options>,
    hw_device_type: Option<HWDeviceType>,
}

impl<'a> EncoderBuilder<'a> {
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
    const BIT_RATE: i64 = 1_000_000;

    /// default codec
    const CODEC_NAME: &'static str = "libx264";

    /// Create an encoder with the specified destination and settings.
    ///
    /// * `destination` - Where to encode to.
    /// * `width` - The width of the video stream.
    /// * `height` - The height of the video stream.
    /// * `pixel_format` - The desired pixel format for the video stream.
    /// * `options` - Custom H264 encoding options.
    pub fn new(destination: impl Into<Location>, width: u32, height: u32) -> Self {
        Self {
            width: width as i32,
            height: height as i32,
            destination: destination.into(),
            pixel_format: PixelFormat::YUV420P,
            format: None,
            options: None,
            codec_name: None,
            hw_device_type: None,
            max_b_frames: 0,
            thread_count: 0,
            interleaved: false,
            bit_rate: Self::BIT_RATE,
            gop_size: Self::FRAME_RATE * 2,
            keyframe_interval: Self::KEY_FRAME_INTERVAL,
            time_base: Rational::new(1, Self::FRAME_RATE),
            frame_rate: Rational::new(Self::FRAME_RATE, 1),
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
    pub fn with_frame_rate(mut self, frame_rate: i32) -> Self {
        self.time_base = Rational::new(1, frame_rate);
        self.frame_rate = Rational::new(frame_rate, 1);
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
    pub fn with_interleaved(mut self) -> Self {
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

    /// Create an encoder from a `FileWriter` instance.
    ///
    /// # Arguments
    ///
    /// * `writer` - [`Writer`] to create encoder from.
    /// * `interleaved` - Whether or not to use interleaved write.
    /// * `settings` - Encoder settings to use.
    pub fn build_from_writer(self, mut writer: Writer) -> Result<Encoder> {
        let global_header = AvFormatFlags::from_bits_truncate(writer.output.oformat().flags as c_uint)
            .contains(AvFormatFlags::GLOBAL_HEADER);

        let codec = self.codec();
        let mut encode_ctx = AVCodecContext::new(&codec);

        // Some formats require this flag to be set or the output will
        // not be playable by dumb players.
        if global_header {
            encode_ctx.set_flags(AvCodecFlags::GLOBAL_HEADER.bits() as i32);
        }

        self.apply_to(&mut encode_ctx);
        let (width, height) = (encode_ctx.width, encode_ctx.height);

        let hw_context = match self.hw_device_type {
            Some(device_type) => {
                if device_type.find_hw_pixel_format_with_codec(&codec).is_none() {
                    return Err(Error::msg(format!(
                        "HW acceleration encoder not supported for codec: {}",
                        utils::to_string(codec.name())
                    )));
                }
                let mut hw_ctx = HWContext::new(device_type.auto_best_device()?)
                    .context("Hardware acceleration context initialization failed.")?;
                hw_ctx.setup_hw_frames(false, &mut encode_ctx, width, height)?;
                Some(hw_ctx)
            }
            None => None,
        };

        let dict = self.options.map(|options| options.to_dict());
        encode_ctx.open(dict).context("Failed to open encode context")?;

        let writer_stream_index = {
            let mut out_stream = writer.output.new_stream();
            out_stream.set_codecpar(encode_ctx.extract_codecpar());
            out_stream.set_time_base(encode_ctx.time_base);
            out_stream.index as usize
        };

        let stream_info = writer.stream_info(writer_stream_index)?;
        log::info!("{}", stream_info);

        Ok(Encoder {
            hw_context,
            encode_ctx,
            writer,
            writer_stream_index,
            frame_count: 0,
            interleaved: self.interleaved,
            keyframe_interval: self.keyframe_interval,
            have_written_header: false,
            have_written_trailer: false,
        })
    }

    /// Get codec, or Try to use the default codec libx264 if none specified.
    pub fn codec(&self) -> AVCodecRef {
        let codec_name = if let Some(codec_name) = &self.codec_name {
            codec_name.as_ref()
        } else {
            Self::CODEC_NAME
        };
        AVCodec::find_encoder_by_name(&utils::from_str(codec_name))
            .context(format!("Failed to find encoder for codec: '{}'", codec_name))
            .unwrap()
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
        encoder.set_pkt_timebase(self.time_base.into());
        encoder.set_pix_fmt(self.pixel_format.into());
        encoder.set_sample_aspect_ratio(avutil::ra(1, 1));
        unsafe {
            encoder.deref_mut().thread_count = self.thread_count;
            encoder.deref_mut().flags2 = ffi::AV_CODEC_FLAG2_FAST as c_int;
        }
    }

    /// Build an [`Encoder`].
    pub fn build(self) -> Result<Encoder> {
        let mut writer_builder = WriterBuilder::new(self.destination.clone());
        if let Some(options) = self.options {
            writer_builder = writer_builder.with_options(options);
        }
        if let Some(format) = self.format {
            writer_builder = writer_builder.with_format(format);
        }
        self.build_from_writer(writer_builder.build()?)
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
    hw_context: Option<HWContext>,
    encode_ctx: AVCodecContext,
    writer: Writer,
    writer_stream_index: usize,
    interleaved: bool,
    frame_count: u64,
    keyframe_interval: u64,
    have_written_header: bool,
    have_written_trailer: bool,
}

impl Encoder {
    /// Create an encoder with the specified destination and settings.
    ///
    /// * `destination` - Where to encode to.
    /// * `settings` - Encoding settings.
    #[inline]
    pub fn new(destination: impl Into<Location>, width: u32, height: u32) -> Result<Self> {
        EncoderBuilder::new(destination, width, height).build()
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
        if height != self.encode_ctx.height as usize || width != self.encode_ctx.width as usize || channels != 3 {
            return Err(Error::msg("Invalid frame format."));
        }

        let mut frame = frame::ndarray_yuv_to_avframe(frame)?;

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
        log::info!("encode_raw raw_frame: {:?}", raw_frame);
        if raw_frame.width != self.encode_ctx.width || raw_frame.height != self.encode_ctx.height {
            return Err(anyhow::anyhow!(
                "Invalid frame pixel format: {:?}, or dimensions: expected {}x{}, got {}x{}",
                PixelFormat::from(raw_frame.format),
                self.encode_ctx.width,
                self.encode_ctx.height,
                raw_frame.width,
                raw_frame.height
            ));
        }

        // Write file header if we hadn't done that yet.
        if !self.have_written_header {
            self.writer.write_header()?;
            self.have_written_header = true;
        }

        // 根据编码器类型选择目标像素格式
        let target_format = if self.hw_context.is_some() {
            self.encode_ctx
                .hw_frames_ctx_mut()
                .map(|mut ctx| PixelFormat::from(ctx.data().sw_format))
                .unwrap_or(PixelFormat::YUV420P)
        } else {
            PixelFormat::YUV420P
        };

        // Reformat frame to target pixel format if need
        let mut frame = if raw_frame.format != target_format.into() {
            frame::convert_avframe(raw_frame, raw_frame.width, raw_frame.height, target_format)?
        } else {
            raw_frame.clone()
        };

        // 计算关键帧
        self.calc_key_frame_pts(&mut frame);

        // 发送帧到编码器
        match self.hw_context.as_ref() {
            Some(hw_ctx) => {
                // 上传到硬件内存并获取硬件帧
                let hw_frame = {
                    if hw_ctx.is_sw_frame(frame.clone()) {
                        hw_ctx
                            .upload_frame(&mut self.encode_ctx, &frame)
                            .map_err(|e| Error::msg(format!("Failed to upload frame: {}", e)))?
                    } else {
                        frame
                    }
                };

                // 发送硬件帧到编码器
                self.encode_ctx
                    .send_frame(Some(&hw_frame))
                    .map_err(|e| Error::msg(format!("Failed to send hardware frame: {}", e)))?;
            }
            None => {
                // 软件编码
                self.encode_ctx
                    .send_frame(Some(&frame))
                    .map_err(|e| Error::msg(format!("Failed to send frame: {}", e)))?;
            }
        }

        match self.encoder_receive_packet() {
            Ok(Some(packet)) => {
                self.write(packet)?;
            }
            Ok(None) => {
                log::debug!("No packet received from encoder.")
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to receive packet from encoder: {}", e));
            }
        }

        Ok(())
    }

    /// Get encoder time base.
    #[inline]
    pub fn time_base(&self) -> Rational {
        self.encode_ctx.time_base.into()
    }

    #[inline]
    pub fn frame_rate(&self) -> Rational {
        self.encode_ctx.framerate.into()
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
    pub fn pix_fmt(&self) -> AVPixelFormat {
        self.encode_ctx.pix_fmt
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

    /// calculate key frame and pts
    fn calc_key_frame_pts(&mut self, frame: &mut RawFrame) {
        // Producer key frame every once in a while
        if self.frame_count % self.keyframe_interval == 0 {
            frame.set_pict_type(ffi::AV_PICTURE_TYPE_I);
        }

        let pts_increment = self.time_base().denominator() as i64 / self.frame_rate().numerator() as i64;
        let pts = self.frame_count as i64 * pts_increment;

        // Update frame pts
        frame.set_time_base(self.time_base().into());
        frame.set_pts(pts);

        // Increment frame count regardless of whether or not frame is written,
        // see https://github.com/oddity-ai/video-rs/issues/46.
        self.frame_count += 1;

        log::debug!(
            "send frame to encoder time_base:{:?}, frame: {:?}",
            frame.time_base,
            frame
        );
    }

    /// Pull an encoded packet from the decoder. This function also handles the possible `EAGAIN`
    /// result, in which case we just need to go again.
    fn encoder_receive_packet(&mut self) -> Result<Option<Packet>> {
        let packet = match self.encode_ctx.receive_packet() {
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
        packet.set_pos(-1);
        packet.set_pts(packet.dts());
        packet.set_stream_index(self.writer_stream_index);
        packet.rescale_ts(self.time_base(), self.stream_time_base());
        if self.interleaved {
            self.writer.write_interleaved(&mut packet)?;
        } else {
            self.writer.write_frame(&mut packet)?;
        };

        Ok(())
    }

    /// Flush the encoder, drain any packets that still need processing.
    fn flush(&mut self) -> Result<()> {
        // 确定编码器是否支持延迟（delay）
        // 如果编码器不支持延迟，那么就没有必要进行 flush 操作，因为在这种情况下，编码器不会保留任何未处理的数据。
        // 如果编码器支持延迟（delay），则在结束编码之前发送 EOS 包是有必要的，
        // 因为编码器可能还在缓冲一些数据，直到接收到 EOS 信号才会处理完这些数据并输出剩余的包。
        if self.encode_ctx.codec().capabilities & ffi::AV_CODEC_CAP_DELAY as i32 == 0 {
            return Ok(());
        }

        // Maximum number of invocations to `encoder_receive_packet`
        // to drain the items still on the queue before giving up.
        const MAX_DRAIN_ITERATIONS: u32 = 100;

        // Notify the encoder that the last frame has been sent.
        self.send_eof().context("Send EOF frame failed.")?;

        for i in 0..MAX_DRAIN_ITERATIONS {
            match self.encoder_receive_packet() {
                Ok(Some(packet)) => self.write(packet)?,
                Ok(None) => {
                    log::debug!("No more packet received, try: {}", i);
                }
                Err(e) => return Err(e).context("Receive packet failed.")?,
            }
        }

        Ok(())
    }

    /// 发送一个空帧来刷新编码器 EOF
    fn send_eof(&mut self) -> Result<()> {
        Ok(self.encode_ctx.send_frame(None)?)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

unsafe impl Send for Encoder {}
unsafe impl Sync for Encoder {}
