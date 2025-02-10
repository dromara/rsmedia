use rsmpeg::avcodec::{AVCodec, AVCodecContext, AVCodecParameters};
use rsmpeg::avcodec::AVCodecContext as AvEncoder;


use rsmpeg::swscale::SwsContext as AvScaler;
use rsmpeg::error::RsmpegError;
use rsmpeg::avutil::{AVDictionary, AVPixelFormat};
use crate::time::TIME_BASE;
use crate::packet::Packet as AvPacket;
use crate::Rational as AvRational;
use crate::flags::{AvCodecFlags, AvFormatFlags, AvScalerFlags};

use crate::error::Error;
use crate::ffi;
#[cfg(feature = "ndarray")]
use crate::frame::Frame;
use crate::frame::{self, PixelFormat, RawFrame};
use crate::io::private::Write;
use crate::io::{Writer, WriterBuilder};
use crate::location::Location;
use crate::options::Options;
#[cfg(feature = "ndarray")]
use crate::time::Time;

use std::ffi::CString;
use libc::c_uint;

type Result<T> = std::result::Result<T, Error>;

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
    encoder: AvEncoder,
    encoder_time_base: AvRational,
    keyframe_interval: u64,
    interleaved: bool,
    scaler: AvScaler,
    scaler_width: u32,
    scaler_height: u32,
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
    pub fn encode(&mut self, frame: &Frame, source_timestamp: Time) -> Result<()> {
        let (height, width, channels) = frame.dim();
        if height != self.scaler_height as usize
            || width != self.scaler_width as usize
            || channels != 3
        {
            return Err(Error::InvalidFrameFormat);
        }

        let mut frame = ffi::convert_ndarray_to_frame_rgb24(frame).map_err(Error::BackendError)?;

        frame.set_pts(
            source_timestamp
                .aligned_with_rational(self.encoder_time_base.into())
                .into_value().unwrap(),
        );

        self.encode_raw(frame)
    }

    /// Encode a single raw frame.
    ///
    /// # Arguments
    ///
    /// * `frame` - Frame to encode.
    pub fn encode_raw(&mut self, frame: RawFrame) -> Result<()> {
        if frame.width as u32 != self.scaler_width
            || frame.height as u32 != self.scaler_height
            || frame.format != frame::FRAME_PIXEL_FORMAT
        {
            return Err(Error::InvalidFrameFormat);
        }

        // Write file header if we hadn't done that yet.
        if !self.have_written_header {
            self.writer.write_header()?;
            self.have_written_header = true;
        }

        // Reformat frame to target pixel format.
        let mut frame = self.scale(frame)?;
        // Producer key frame every once in a while
        if self.frame_count % self.keyframe_interval == 0 {
            frame.set_pict_type(rsmpeg::ffi::AV_PICTURE_TYPE_I);
        }

        self.encoder
            .send_frame(Some(&frame))
            .map_err(Error::BackendError).unwrap();
        // Increment frame count regardless of whether or not frame is written, see
        // https://github.com/oddity-ai/video-rs/issues/46.
        self.frame_count += 1;

        if let Some(packet) = self.encoder_receive_packet()? {
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
            self.flush()?;
            self.writer.write_trailer()?;
        }

        Ok(())
    }

    /// Get encoder time base.
    #[inline]
    pub fn time_base(&self) -> AvRational {
        self.encoder_time_base
    }

    /// Create an encoder from a `FileWriter` instance.
    ///
    /// # Arguments
    ///
    /// * `writer` - [`Writer`] to create encoder from.
    /// * `interleaved` - Whether or not to use interleaved write.
    /// * `settings` - Encoder settings to use.
    fn from_writer(mut writer: Writer, interleaved: bool, settings: Settings) -> Result<Self> {
        let global_header = unsafe {
            AvFormatFlags::from_bits_truncate(writer.output.oformat().flags as c_uint)
                .contains(AvFormatFlags::GLOBAL_HEADER)
        };

        let writer_stream_index = {
            let mut writer_stream = writer.output.new_stream();
            let mut params = AVCodecParameters::new();
            params.from_context(&mut AVCodecContext::new(&settings.codec().unwrap()));
            writer_stream.set_codecpar(params);
            writer_stream.index as usize
        };

        let mut encoder_context = ffi::codec_context_as(settings.codec())?;

        // Some formats require this flag to be set or the output will
        // not be playable by dumb players.
        if global_header {
            encoder_context.set_flags(AvCodecFlags::GLOBAL_HEADER.bits() as i32);
        }

        settings.apply_to(&mut encoder_context);

        // TODO:
        // let opts = unsafe { settings.options().to_dict().disown() };
        encoder_context.open(None).map_err(Error::BackendError).unwrap();

        let encoder_time_base = encoder_context.time_base.into();

        let scaler_width = encoder_context.width;
        let scaler_height = encoder_context.height;
        let scaler = AvScaler::get_context(
            scaler_width,
            scaler_height,
            frame::FRAME_PIXEL_FORMAT,
            scaler_width,
            scaler_height,
            encoder_context.pix_fmt,
            AvScalerFlags::empty().bits(),
            None,
            None,
            None,
        ).unwrap();

        Ok(Self {
            writer,
            writer_stream_index,
            encoder: encoder_context,
            encoder_time_base,
            keyframe_interval: settings.keyframe_interval,
            interleaved,
            scaler,
            scaler_width: scaler_width as u32,
            scaler_height: scaler_height as u32,
            frame_count: 0,
            have_written_header: false,
            have_written_trailer: false,
        })
    }

    /// Apply scaling (or pixel reformatting in this case) on the frame with the scaler we
    /// initialized earlier.
    ///
    /// # Arguments
    ///
    /// * `frame` - Frame to rescale.
    fn scale(&mut self, frame: RawFrame) -> Result<RawFrame> {
        let mut frame_scaled = RawFrame::new();
        self.scaler
            .scale_frame(&frame, frame.width, frame.height,&mut frame_scaled)
            .map_err(Error::BackendError)?;
        // Copy over PTS from old frame.
        frame_scaled.set_pts(frame.pts);

        Ok(frame_scaled)
    }

    /// Pull an encoded packet from the decoder. This function also handles the possible `EAGAIN`
    /// result, in which case we just need to go again.
    fn encoder_receive_packet(&mut self) -> Result<Option<AvPacket>> {
        let mut packet = AvPacket::empty();
        let encode_result = self.encoder.receive_packet();
        match encode_result {
            Ok(p) => {
                packet.copy_props(p)?;
                Ok(Some(packet))
            }
            Err(RsmpegError::SendFrameAgainError) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Acquire the time base of the output stream.
    fn stream_time_base(&mut self) -> AvRational {
        self.writer
            .output
            .streams().get(self.writer_stream_index)
            .unwrap()
            .time_base.into()
    }

    /// Write encoded packet to output stream.
    ///
    /// # Arguments
    ///
    /// * `packet` - Encoded packet.
    fn write(&mut self, mut packet: AvPacket) -> Result<()> {
        packet.set_stream_index(self.writer_stream_index);
        packet.set_position(-1);
        packet.rescale_ts(self.encoder_time_base, self.stream_time_base());
        if self.interleaved {
            self.writer.write_interleaved(&mut packet)?;
        } else {
            self.writer.write(&mut packet)?;
        };

        Ok(())
    }

    /// Flush the encoder, drain any packets that still need processing.
    fn flush(&mut self) -> Result<()> {
        // Maximum number of invocations to `encoder_receive_packet`
        // to drain the items still on the queue before giving up.
        const MAX_DRAIN_ITERATIONS: u32 = 100;

        // send eof
        // Notify the encoder that the last frame has been sent.
        self.encoder.send_frame(None)?;

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
            pixel_format: rsmpeg::ffi::AV_PIX_FMT_YUV420P,
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
    fn apply_to(&self, encoder: &mut AvEncoder) {
        encoder.set_width(self.width);
        encoder.set_height(self.height);
        encoder.set_pix_fmt(self.pixel_format);
        encoder.set_bit_rate(Self::BIT_RATE);
        encoder.set_framerate(AvRational::new(Self::FRAME_RATE, 1).into());
        // Just use the ffmpeg global time base which is precise enough
        // that we should never get in trouble.
        encoder.set_time_base(TIME_BASE.into());
    }

    /// Get codec.
    fn codec(&self) -> Option<AVCodec> {
        // Try to use the libx264 decoder. If it is not available, then use use whatever default
        // h264 decoder we have.
        unsafe {
            let codec = AVCodec::find_encoder_by_name(&CString::new("libx264").unwrap());
            if codec.is_none() {
                let encoder = AVCodec::find_encoder(rsmpeg::ffi::AV_CODEC_ID_H264)?;
                Some(AVCodec::from_raw(std::ptr::NonNull::new(encoder.as_ptr() as *mut _)?))
            } else {
                Some(AVCodec::from_raw(std::ptr::NonNull::new(codec.unwrap().as_ptr() as *mut _)?))
            }
        }
    }

    /// Get encoder options.
    fn options(&self) -> &Options {
        &self.options
    }
}

unsafe impl Send for Encoder {}
unsafe impl Sync for Encoder {}
