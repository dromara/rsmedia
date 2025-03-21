use crate::flags::AvCodecFlags;
#[cfg(feature = "ndarray")]
use crate::frame::{self, FrameArray};
use crate::hwaccel::{HWContext, HWDeviceConfig};
use crate::io::Reader;
use crate::options::Options;
use crate::resize::Resize;
use crate::stream::StreamInfo;
#[cfg(feature = "ndarray")]
use crate::time::Time;
use crate::{swctx, utils, MediaType, PixelFormat, RawFrame};

use anyhow::{Context, Error, Result};
use rsmpeg::avcodec::{AVCodec, AVCodecContext, AVPacket};
use rsmpeg::ffi;

/// Builds a [`Decoder`].
#[derive(Debug, Clone)]
pub struct DecoderBuilder {
    flags: AvCodecFlags,
    media_type: MediaType,
    resize: Option<Resize>,
    codec_name: Option<String>,
    codec_opts: Option<Options>,
    hw_device_config: Option<HWDeviceConfig>,
}

impl DecoderBuilder {
    /// Create a decoder with the specified source.
    ///
    /// * `source` - Source to decode.
    pub fn new() -> Self {
        Self {
            flags: AvCodecFlags::LOW_DELAY,
            resize: None,
            media_type: MediaType::VIDEO,
            codec_name: None,
            codec_opts: None,
            hw_device_config: None,
        }
    }

    /// Set decoding flags.
    pub fn with_flags(mut self, flags: AvCodecFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Set resizing to apply to frames.
    ///
    /// * `resize` - Resizing to apply.
    pub fn with_resize(mut self, resize: Option<Resize>) -> Self {
        self.resize = resize;
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

    /// Enable hardware acceleration with the specified device type.
    ///
    /// * `device_config` - Device to use for hardware acceleration.
    pub fn with_hardware_device(mut self, device_config: Option<HWDeviceConfig>) -> Self {
        self.hw_device_config = device_config;
        self
    }

    pub fn with_media_type(mut self, media_type: MediaType) -> Self {
        self.media_type = media_type;
        self
    }

    /// Build [`Decoder`].
    pub fn build<R: Reader>(self, reader: &R) -> Result<Decoder> {
        self.build_from_reader(reader)
    }

    pub fn build_from_reader<R: Reader>(self, reader: &R) -> Result<Decoder> {
        let (stream_index, codec_name) = reader.find_best_stream(self.media_type)?;
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

        let time_base = input_stream.time_base;
        let mut decode_ctx = AVCodecContext::new(&codec);
        decode_ctx.set_time_base(time_base);
        decode_ctx.set_pkt_timebase(time_base);
        decode_ctx.set_flags(self.flags as i32);
        decode_ctx.apply_codecpar(&input_stream.codecpar())?;
        if let Some(framerate) = input_stream.guess_framerate() {
            decode_ctx.set_framerate(framerate);
        }

        let (width, height) = (decode_ctx.width, decode_ctx.height);
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
                HWContext::new(cfg)
                    .and_then(|mut ctx| {
                        ctx.setup_hw_frames(true, &mut decode_ctx, width, height)?;
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

        let (resize_width, resize_height) = match self.resize {
            Some(resize) => resize
                .compute_for((width as u32, height as u32))
                .ok_or(Error::msg("Invalid resize parameters"))?,
            None => (width as u32, height as u32),
        };

        Ok(Decoder {
            decode_ctx,
            hw_context,
            time_base,
            media_type: self.media_type,
            size: (width as u32, height as u32),
            size_out: (resize_width, resize_height),
            stream_index,
            draining: false,
        })
    }
}

impl Default for DecoderBuilder {
    fn default() -> Self {
        Self::new()
    }
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
    decode_ctx: AVCodecContext,
    hw_context: Option<HWContext>,
    time_base: ffi::AVRational,
    media_type: MediaType,
    stream_index: usize,
    size: (u32, u32),
    size_out: (u32, u32),
    draining: bool,
}

impl Decoder {
    /// Create a decoder to decode the specified source.
    ///
    /// # Arguments
    ///
    /// * `source` - Source to decode.
    #[inline]
    pub fn new<R: Reader>(reader: &R) -> Result<Decoder> {
        DecoderBuilder::new().build(reader)
    }

    /// Get the decoders input size (resolution dimensions): width * height.
    #[inline(always)]
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Get the decoders output size after resizing is applied (resolution dimensions): width * height.
    #[inline(always)]
    pub fn size_out(&self) -> (u32, u32) {
        self.size_out
    }

    /// Get decoder time base.
    #[inline(always)]
    pub fn time_base(&self) -> ffi::AVRational {
        self.time_base
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
    pub fn draining(&self) -> bool {
        self.draining
    }

    /// Set draining mode.
    pub fn set_draining(&mut self, draining: bool) {
        self.draining = draining;
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
    pub fn decode(&mut self, packet: &AVPacket) -> DecodeResult {
        if !self.draining() {
            self._decode(packet)
        } else {
            self.drain()
        }
    }

    /// Decode a single frame and return the raw ffmpeg `AvFrame`.
    ///
    /// # Return value
    ///
    /// The decoded raw frame as [`RawFrame`].
    pub fn decode_raw(&mut self, packet: &AVPacket) -> DecodeRawResult {
        if !self.draining() {
            self._decode_raw(packet)
        } else {
            self.drain_raw()
        }
    }

    // /// Seek in reader.
    // ///
    // /// See [`StreamReader::seek`](crate::io::StreamReader::seek) for more information.
    // #[inline]
    // pub fn seek(&mut self, timestamp_milliseconds: i64) -> Result<()> {
    //     self.reader
    //         .seek(timestamp_milliseconds)
    //         .inspect(|_| self.flush())
    // }

    // /// Seek to specific frame in reader.
    // ///
    // /// See [`StreamReader::seek_to_frame`](crate::io::StreamReader::seek_to_frame) for more information.
    // #[inline]
    // pub fn seek_to_frame(&mut self, frame_number: i64) -> Result<()> {
    //     self.reader
    //         .seek_to_frame(frame_number)
    //         .inspect(|_| self.flush())
    // }

    // /// Seek to start of reader.
    // ///
    // /// See [`StreamReader::seek_to_start`](crate::io::StreamReader::seek_to_start) for more information.
    // #[inline]
    // pub fn seek_to_start(&mut self) -> Result<()> {
    //     self.reader.seek_to_start().inspect(|_| self.flush())
    // }

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
    fn _decode(&mut self, packet: &AVPacket) -> DecodeResult {
        let decode_result = self._decode_raw(packet);
        match decode_result {
            DecodeRawResult::Frame(mut frame) => match self.raw_frame_to_time_and_frame(&mut frame)
            {
                Ok(frame_arr) => DecodeResult::Frame(frame_arr),
                Err(e) => DecodeResult::Error(e),
            },
            DecodeRawResult::Drain => DecodeResult::Drain,
            DecodeRawResult::Flushed => DecodeResult::Flushed,
            DecodeRawResult::Error(e) => DecodeResult::Error(e),
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
    /// The decoded raw frame as [`RawFrame`] if the decoder has a frame available, [`None`] if not.
    fn _decode_raw(&mut self, packet: &AVPacket) -> DecodeRawResult {
        assert!(!self.draining());
        self.send_packet_to_decoder(packet).unwrap();
        self.receive_frame_from_decoder()
    }

    /// Drain one frame from the decoder.
    ///
    /// After calling drain once the decoder is in draining mode and the caller may not use normal
    /// decode anymore or it will panic.
    ///
    /// # Return value
    ///
    /// A tuple of the [`Frame`] and timestamp (relative to the stream) and the frame itself if the
    /// decoder has a frame available, [`None`] if not.
    #[cfg(feature = "ndarray")]
    pub fn drain(&mut self) -> DecodeResult {
        let decode_result = self.drain_raw();
        match decode_result {
            DecodeRawResult::Frame(mut frame) => match self.raw_frame_to_time_and_frame(&mut frame)
            {
                Ok(frame_arr) => DecodeResult::Frame(frame_arr),
                Err(e) => DecodeResult::Error(e),
            },
            DecodeRawResult::Drain => DecodeResult::Drain,
            DecodeRawResult::Flushed => DecodeResult::Flushed,
            DecodeRawResult::Error(e) => DecodeResult::Error(e),
        }
    }

    /// Drain one frame from the decoder.
    ///
    /// After calling drain once the decoder is in draining mode and the caller may not use normal
    /// decode anymore or it will panic.
    ///
    /// # Return value
    ///
    /// The decoded raw frame as [`RawFrame`] if the decoder has a frame available, [`None`] if not.
    pub fn drain_raw(&mut self) -> DecodeRawResult {
        if !self.draining() {
            self.send_eof().unwrap();
            self.set_draining(true);
        }
        self.receive_frame_from_decoder()
    }

    /// Sends a NULL packet to the decoder to signal end of stream and enter
    /// draining mode.
    fn send_eof(&mut self) -> Result<()> {
        self.decode_ctx.send_packet(None)?;
        Ok(())
    }

    /// Reset the decoder to be used again after draining.
    pub fn reset(&mut self) {
        self.flush();
        self.set_draining(false);
    }

    pub fn flush(&mut self) {
        unsafe {
            ffi::avcodec_flush_buffers(self.decode_ctx.as_mut_ptr());
        }
    }

    /// Send packet to decoder.
    /// Ensure rescaling timestamps accordingly before sending to decoder.
    fn send_packet_to_decoder(&mut self, packet: &AVPacket) -> Result<()> {
        self.decode_ctx
            .send_packet(Some(packet))
            .context("Failed to send packet to decoder")?;
        Ok(())
    }

    /// Receive packet from decoder. Will handle hwaccel conversions and scaling as well.
    fn receive_frame_from_decoder(&mut self) -> DecodeRawResult {
        let decode_result = self.decoder_receive_frame();
        let frame = match decode_result {
            DecodeRawResult::Frame(frame) => frame,
            _ => return decode_result,
        };

        // handle hwaccel decoding and rescale frame only for video
        if self.media_type != MediaType::VIDEO {
            return DecodeRawResult::Frame(frame);
        }

        let sw_frame = self
            .hw_context
            .as_ref()
            .and_then(|hw_ctx| {
                if hw_ctx.is_hw_frame(&frame) {
                    Some(hw_ctx.hw_download(&mut self.decode_ctx, &frame))
                } else {
                    log::warn!("Hardware acceleration decoding not available!");
                    None
                }
            })
            .map_or(Ok(frame), |result| {
                result.map_err(|e| {
                    log::error!("Failed to download frame from hw_device: {}", e);
                    Error::msg(format!("HW frame download failed: {}", e))
                })
            })
            .unwrap();

        // handle scaling frame if needed (if not, size_out is the same as size)
        match self.rescale_frame(sw_frame) {
            Ok(scaled_frame) => DecodeRawResult::Frame(scaled_frame),
            Err(e) => DecodeRawResult::Error(e),
        }
    }

    /// Pull a decoded frame from the decoder. This function also implements retry mechanism in case
    /// the decoder signals `EAGAIN` and `EOF`
    fn decoder_receive_frame(&mut self) -> DecodeRawResult {
        match self.decode_ctx.receive_frame() {
            Ok(frame) => DecodeRawResult::Frame(frame),
            Err(rsmpeg::error::RsmpegError::DecoderDrainError) => {
                log::debug!("Decoder drained. try send new packet again.");
                DecodeRawResult::Drain
            }
            Err(rsmpeg::error::RsmpegError::DecoderFlushedError) => {
                log::debug!("Decoder flushed. EOF reached.");
                DecodeRawResult::Flushed
            }
            Err(e) => {
                log::warn!("Failed to receive frame from decoder: {}", e);
                DecodeRawResult::Error(Error::new(e))
            }
        }
    }

    /// Rescale frame if needed.
    fn rescale_frame(&self, frame: RawFrame) -> Result<RawFrame> {
        let input_format = self
            .hw_context
            .as_ref()
            .map_or(frame.format, |ctx| ctx.get_format(false));

        let (resize_width, resize_height) = self.size_out();
        let is_scale_needed = !(input_format == PixelFormat::YUV420P.into()
            && frame.width as u32 == resize_width
            && frame.height as u32 == resize_height);

        if is_scale_needed {
            return swctx::scale_frame(
                &frame,
                resize_width as i32,
                resize_height as i32,
                PixelFormat::YUV420P,
            );
        }

        Ok(frame)
    }

    #[cfg(feature = "ndarray")]
    fn raw_frame_to_time_and_frame(&self, frame: &mut RawFrame) -> Result<(Time, FrameArray)> {
        // We use the packet DTS here (which is `frame->pkt_dts`) because that is what the
        // encoder will use when encoding for the `PTS` field.
        let timestamp = Time::new(Some(frame.pkt_dts), self.time_base());
        // AVFrame default pixel is YUV420P, So here keeping the format that YUV420P the same
        // after I convert it, If you want RGB24, always remember to convert it yourself!
        let frame = frame::avframe_yuv_to_ndarray(frame).unwrap();

        Ok((timestamp, frame))
    }
}

/// Important note: Do not forget to drain the decoder after the reader is exhausted. It may still
/// contain frames. Run `drain_raw()` or `drain()` in a loop until no more frames are produced.
impl Drop for Decoder {
    fn drop(&mut self) {
        // We need to drain the items still in the decoders queue.
        if let Ok(()) = self.send_eof() {
            loop {
                match self.decoder_receive_frame() {
                    DecodeRawResult::Frame(_) => {
                        // If receive a frame, we continue to drain the queue.
                        log::debug!("continue draining decoder queue.");
                        continue;
                    }
                    DecodeRawResult::Drain => {
                        // If need more, we continue to drain the queue.
                        log::debug!("Decoder drained. try send new packet again.");
                        continue;
                    }
                    DecodeRawResult::Flushed => {
                        log::debug!("Decoder flushed. EOF reached.");
                        break;
                    }
                    DecodeRawResult::Error(e) => {
                        log::error!("Failed to drain decoder: {}", e);
                        break;
                    }
                }
            }
        }

        unsafe {
            // explicitly drop the hw_context to release the hardware resources
            // 1. malloc(): unsorted double linked list corrupted
            // 2. malloc(): mismatching next->prev_size (unsorted)
            // 3. free(): invalid pointer
            // 4. double free or corruption (!prev)
            // 5. corrupted double-linked list Aborted (core dumped)
            let codec_ctx_ptr = self.decode_ctx.as_mut_ptr();
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

/// decode result
#[cfg(feature = "ndarray")]
#[derive(Debug)]
pub enum DecodeResult {
    /// decoded frame as [`FrameArray`]
    Frame((Time, FrameArray)),
    /// decoder is drained
    Drain,
    /// decoder is flushed reached
    Flushed,
    /// decoder error
    Error(Error),
}

/// decode_raw result
#[derive(Debug)]
pub enum DecodeRawResult {
    /// decoded frame
    Frame(RawFrame),
    /// decoder is drained
    Drain,
    /// decoder is flushed reached EOF
    Flushed,
    /// decoder error
    Error(Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::StreamReader;

    #[test]
    #[ignore = "need a video file"]
    fn test_decode_video() -> Result<()> {
        let path = std::path::Path::new("/tmp/bear.mp4");

        let mut stream_reader = StreamReader::new(path)?;
        let mut decoder = DecoderBuilder::new()
            .with_media_type(MediaType::VIDEO)
            .build(&stream_reader)?;

        loop {
            match stream_reader.read_packet() {
                Ok(Some((in_stream, mut packet))) => {
                    // println!("packet: {:?}", packet);
                    // 这里需要注意，reader 读取到的包是没有解码的所有通道的数据包
                    // 如果是视频流，需要先判断是否是视频流，然后再decode
                    if decoder.stream_index() == in_stream.index() {
                        // 解码前处理输入数据包, 将输入容器的时间基转换为解码器的时间基
                        // in_stream->time_base  =>  dec_ctx->time_base
                        packet.rescale_ts(in_stream.time_base(), decoder.time_base());
                        match decoder.decode_raw(&packet) {
                            DecodeRawResult::Frame(frame) => {
                                println!(
                                    "video frame: {:?}, timebase:{:?}",
                                    frame, frame.time_base
                                );
                            }
                            DecodeRawResult::Drain => {
                                println!("decoder is drained.");
                                continue;
                            }
                            DecodeRawResult::Flushed => {
                                println!("decoder is flushed.");
                                break;
                            }
                            DecodeRawResult::Error(e) => {
                                log::error!("Error on decoding frame: {}", e);
                                return Err(e);
                            }
                        }
                    } else {
                        println!("skip packet for stream index: {}", in_stream.index())
                    }
                }
                Ok(None) => {
                    println!("No more packets, Reader exhausted.");
                    break;
                }
                Err(e) => {
                    log::error!("Error reading packet: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    #[test]
    #[ignore = "need a audio file"]
    fn test_decode_audio() -> Result<()> {
        let path = std::path::Path::new("/tmp/bear.mp4");

        let mut stream_reader = StreamReader::new(path)?;
        let mut decoder = DecoderBuilder::new()
            .with_media_type(MediaType::AUDIO)
            .build(&stream_reader)?;

        loop {
            match stream_reader.read_packet() {
                Ok(Some((in_stream, mut packet))) => {
                    // println!("packet: {:?}", packet);
                    // 这里需要注意，reader 读取到的包是没有解码的所有通道的数据包
                    // 如果是视频流，需要先判断是否是视频流，然后再decode
                    if decoder.stream_index() == in_stream.index() {
                        // 解码前处理输入数据包, 将输入容器的时间基转换为解码器的时间基
                        // in_stream->time_base  =>  dec_ctx->time_base
                        packet.rescale_ts(in_stream.time_base(), decoder.time_base());
                        match decoder.decode_raw(&packet) {
                            DecodeRawResult::Frame(frame) => {
                                println!(
                                    "audio frame: {:?}, timebase:{:?}",
                                    frame, frame.time_base
                                );
                            }
                            DecodeRawResult::Drain => {
                                println!("decoder is drained.");
                                continue;
                            }
                            DecodeRawResult::Flushed => {
                                println!("decoder is flushed.");
                                break;
                            }
                            DecodeRawResult::Error(e) => {
                                log::error!("Error on decoding frame: {}", e);
                                return Err(e);
                            }
                        }
                    } else {
                        println!("skip packet for stream index: {}", in_stream.index())
                    }
                }
                Ok(None) => {
                    println!("No more packets, Reader exhausted.");
                    break;
                }
                Err(e) => {
                    log::error!("Error reading packet: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(())
    }
}
