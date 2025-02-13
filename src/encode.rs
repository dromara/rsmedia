use crate::error::MediaError;
use crate::flags::{AvCodecFlags, AvFormatFlags};
#[cfg(feature = "ndarray")]
use crate::frame::{self, FrameArray};
use crate::io::{Writer, WriterBuilder};
use crate::location::Location;
use crate::options::Options;
use crate::packet::Packet;
use crate::time::Time;
use crate::{PixelFormat, Rational, RawFrame};
use rsmpeg::avcodec::{AVCodec, AVCodecContext, AVCodecRef};
use rsmpeg::avutil::AVPixelFormat;
use rsmpeg::error::RsmpegError;

use crate::io::private::Write;
use libc::c_uint;
use rsmpeg::ffi;
use std::ffi::CString;

type Result<T> = std::result::Result<T, MediaError>;

/// Builds an [`Encoder`].
pub struct EncoderBuilder<'a> {
    destination: Location,
    settings: Settings,
    options: Option<&'a Options>,
    format: Option<&'a str>,
    interleaved: bool,
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

    /// Build an [`Encoder`].
    pub fn build(self) -> Result<Encoder> {
        let mut writer_builder = WriterBuilder::new(self.destination);
        if let Some(options) = self.options {
            writer_builder = writer_builder.with_options(options);
        }
        if let Some(format) = self.format {
            writer_builder = writer_builder.with_format(format);
        }
        Encoder::from_writer(writer_builder.build()?, self.interleaved, self.settings)
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
            return Err(MediaError::InvalidFrameFormat);
        }

        let mut frame = frame::ndarray_to_avframe_yuv(frame).unwrap();

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
            return Err(MediaError::InvalidFrameFormat);
        }

        // Write file header if we hadn't done that yet.
        if !self.have_written_header {
            self.writer.write_header()?;
            self.have_written_header = true;
        }

        // Reformat frame to target pixel format
        let mut frame = if raw_frame.format != frame::PIXEL_FORMAT_YUV420P {
            frame::convert_avframe(
                raw_frame,
                raw_frame.width,
                raw_frame.height,
                frame::PIXEL_FORMAT_YUV420P,
            )
            .unwrap()
        } else {
            raw_frame.clone()
        };

        // Producer key frame every once in a while
        if self.frame_count % self.keyframe_interval == 0 {
            frame.set_pict_type(ffi::AV_PICTURE_TYPE_I);
        }

        self.encoder.send_frame(Some(&frame)).unwrap();

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

    /// Create an encoder from a `FileWriter` instance.
    ///
    /// # Arguments
    ///
    /// * `writer` - [`Writer`] to create encoder from.
    /// * `interleaved` - Whether or not to use interleaved write.
    /// * `settings` - Encoder settings to use.
    fn from_writer(mut writer: Writer, interleaved: bool, settings: Settings) -> Result<Self> {
        let global_header =
            AvFormatFlags::from_bits_truncate(writer.output.oformat().flags as c_uint)
                .contains(AvFormatFlags::GLOBAL_HEADER);

        let mut encode_context = AVCodecContext::new(&settings.codec().unwrap());
        // Some formats require this flag to be set or the output will
        // not be playable by dumb players.
        if global_header {
            encode_context.set_flags(AvCodecFlags::GLOBAL_HEADER.bits() as i32);
        }

        settings.apply_to(&mut encode_context);

        encode_context
            .open(Some(settings.options().to_dict().av_dict()))
            .expect("Could not open encode context");

        let writer_stream_index = {
            let mut out_stream = writer.output.new_stream();
            out_stream.set_codecpar(encode_context.extract_codecpar());
            out_stream.set_time_base(encode_context.time_base);
            out_stream.index as usize
        };

        Ok(Self {
            writer,
            writer_stream_index,
            encoder: encode_context,
            keyframe_interval: settings.keyframe_interval,
            interleaved,
            frame_count: 0,
            have_written_header: false,
            have_written_trailer: false,
        })
    }

    /// Pull an encoded packet from the decoder. This function also handles the possible `EAGAIN`
    /// result, in which case we just need to go again.
    fn encoder_receive_packet(&mut self) -> Result<Option<Packet>> {
        let packet = match self.encoder.receive_packet() {
            Ok(p) => Packet::new_with_avpacket(p),
            Err(RsmpegError::EncoderDrainError) | Err(RsmpegError::EncoderFlushedError) => {
                return Ok(None);
            }
            Err(err) => return Err(MediaError::BackendError(err)),
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
            self.writer.write_interleaved(&mut packet)?;
        } else {
            self.writer.write_frame(&mut packet)?;
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
                Ok(Some(packet)) => self.write(packet)?,
                Ok(None) => continue,
                Err(_) => break,
            }
        }

        Ok(())
    }

    fn send_eof(&mut self) -> Result<()> {
        Ok(self.encoder.send_frame(None)?)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

/// Holds a logical combination of encoder settings.
#[derive(Debug, Clone)]
pub struct Settings {
    width: i32,
    height: i32,
    pixel_format: AVPixelFormat,
    keyframe_interval: u64,
    options: Options,
}

impl Settings {
    /// Default keyframe interval.
    const KEY_FRAME_INTERVAL: u64 = 12;

    /// This is the assumed FPS for the encoder to use. Note that this does not need to be correct
    /// exactly.
    const FRAME_RATE: i32 = 30;

    /// Default bit rate.
    /// 分辨率(width, height) + 推荐比特率（单位：bps）
    /// 标清 480p:     (640, 480) =>  1_000_000,    // 1 Mbps
    /// 高清 720p:     (1280, 720) => 2_500_000,    // 2.5 Mbps
    /// 全高清 1080p:  (1920, 1080) => 5_000_000,   // 5 Mbps
    /// 超高清 2K:     (2560, 1440) => 8_000_000,   // 8 Mbps
    /// 超高清 4K:     (3840, 2160) => 20_000_000,  // 20 Mbps
    const BIT_RATE: i64 = 1_000_000;

    /// Create encoder settings for an H264 stream with YUV420p pixel format. This will encode to
    /// arguably the most widely compatible video file since H264 is a common codec and YUV420p is
    /// the most commonly used pixel format.
    pub fn preset_h264_yuv420p(width: usize, height: usize, realtime: bool) -> Settings {
        let options = if realtime {
            Options::preset_h264_realtime()
        } else {
            Options::preset_h264()
        };

        Self {
            width: width as i32,
            height: height as i32,
            pixel_format: ffi::AV_PIX_FMT_YUV420P,
            keyframe_interval: Self::KEY_FRAME_INTERVAL,
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
        width: usize,
        height: usize,
        pixel_format: PixelFormat,
        options: Options,
    ) -> Settings {
        Self {
            width: width as i32,
            height: height as i32,
            pixel_format,
            keyframe_interval: Self::KEY_FRAME_INTERVAL,
            options,
        }
    }

    /// Set the keyframe interval.
    pub fn set_keyframe_interval(&mut self, keyframe_interval: u64) {
        self.keyframe_interval = keyframe_interval;
    }

    /// Set the keyframe interval.
    pub fn with_keyframe_interval(mut self, keyframe_interval: u64) -> Self {
        self.set_keyframe_interval(keyframe_interval);
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
    fn apply_to(&self, encoder: &mut AVCodecContext) {
        encoder.set_width(self.width);
        encoder.set_height(self.height);
        encoder.set_pix_fmt(self.pixel_format);
        encoder.set_bit_rate(Self::BIT_RATE);
        // 30
        encoder.set_framerate(Rational::new(Self::FRAME_RATE, 1).into());
        // 30 * 1000
        encoder.set_time_base(Rational::new(1, Self::FRAME_RATE * 1000).into());
    }

    /// Get codec.
    fn codec(&self) -> Option<AVCodecRef> {
        // Try to use the libx264 decoder. If it is not available, then use use whatever default
        // h264 decoder we have.
        let codec = AVCodec::find_encoder_by_name(&CString::new("libx264").unwrap());
        if codec.is_none() {
            AVCodec::find_encoder(ffi::AV_CODEC_ID_H264)
        } else {
            codec
        }
    }

    /// Get encoder options.
    fn options(&self) -> &Options {
        &self.options
    }
}

unsafe impl Send for Encoder {}
unsafe impl Sync for Encoder {}
