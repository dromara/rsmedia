#[cfg(feature = "ndarray")]
use crate::frame::{self, FrameArray};
use crate::hwaccel::{HWContext, HWDeviceType};
use crate::io::{Reader, StreamReader, StreamReaderBuilder};
use crate::location::Location;
use crate::options::Options;
use crate::packet::Packet;
use crate::resize::Resize;
use crate::stream::StreamInfo;
use crate::time::Time;
use crate::{utils, MediaType, PixelFormat, Rational, RawFrame};

use anyhow::{Context, Error, Result};
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avformat::AVStreamRef;
use rsmpeg::error::RsmpegError;
use rsmpeg::{avutil, ffi};

/// Builds a [`Decoder`].
pub struct DecoderBuilder<'a> {
    source: Location,
    resize: Option<Resize>,
    media_type: MediaType,
    // container format
    format: Option<&'a str>,
    format_opts: Option<&'a Options>,
    codec_name: Option<String>,
    codec_opts: Option<&'a Options>,
    hw_device_type: Option<HWDeviceType>,
}

impl<'a> DecoderBuilder<'a> {
    /// Create a decoder with the specified source.
    ///
    /// * `source` - Source to decode.
    pub fn new(source: impl Into<Location>) -> Self {
        Self {
            source: source.into(),
            resize: None,
            media_type: MediaType::VIDEO,
            format: None,
            format_opts: None,
            codec_name: None,
            codec_opts: None,
            hw_device_type: None,
        }
    }

    pub fn new_with_location(source: Location) -> Self {
        Self::new(source)
    }

    /// Set the codec name to use for decoding.
    /// If not set, the decoder will try to guess the codec based on the input.
    pub fn with_codec_name(mut self, codec_name: String) -> Self {
        self.codec_name = Some(codec_name);
        self
    }

    /// Set the container format.
    pub fn with_format(mut self, format: &'a str) -> Self {
        self.format = Some(format);
        self
    }

    /// Set custom options. Options are applied to the input.
    ///
    /// * `options` - Custom options.
    pub fn with_format_options(mut self, options: &'a Options) -> Self {
        self.format_opts = Some(options);
        self
    }

    pub fn with_codec_options(mut self, options: &'a Options) -> Self {
        self.codec_opts = Some(options);
        self
    }

    /// Set resizing to apply to frames.
    ///
    /// * `resize` - Resizing to apply.
    pub fn with_resize(mut self, resize: Resize) -> Self {
        self.resize = Some(resize);
        self
    }

    /// Enable hardware acceleration with the specified device type.
    ///
    /// * `device_type` - Device to use for hardware acceleration.
    pub fn with_hardware_device(mut self, device_type: HWDeviceType) -> Self {
        self.hw_device_type = Some(device_type);
        self
    }

    pub fn with_media_type(mut self, media_type: MediaType) -> Self {
        self.media_type = media_type;
        self
    }

    /// Build [`Decoder`].
    pub fn build<R: Reader>(self, reader: R) -> Result<Decoder<R>> {
        self.build_from_reader(reader)
    }

    pub fn build_from_reader<R: Reader>(self, reader: R) -> Result<Decoder<R>> {
        let (stream_index, codec_name) = reader.find_best_stream(self.media_type)?;
        let stream = reader
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

        let time_base = stream.time_base;
        let mut decode_ctx = AVCodecContext::new(&codec);
        decode_ctx.set_time_base(time_base);
        decode_ctx.set_pkt_timebase(time_base);
        decode_ctx.apply_codecpar(&stream.codecpar())?;

        let (width, height) = (decode_ctx.width, decode_ctx.height);
        let hw_context = if self.media_type == MediaType::VIDEO && self.hw_device_type.is_some() {
            let device_type = self.hw_device_type.unwrap();
            if device_type
                .find_hw_pixel_format_with_codec(&codec)
                .is_none()
            {
                return Err(Error::msg(format!(
                    "HW acceleration decoder not supported for codec: {}",
                    utils::to_string(codec.name())
                )));
            }
            let mut hw_ctx = HWContext::new(device_type.auto_best_device().unwrap())?;
            hw_ctx.setup_hw_frames(true, &mut decode_ctx, width, height)?;
            Some(hw_ctx)
        } else {
            None
        };

        let dict = self.codec_opts.map(|options| options.to_dict());
        decode_ctx
            .open(dict)
            .context("Failed to open decoder for stream")?;

        let stream_info = StreamInfo::from_stream(stream)?;
        log::info!("{}", stream_info);

        let (resize_width, resize_height) = match self.resize {
            Some(resize) => resize
                .compute_for((width as u32, height as u32))
                .ok_or(Error::msg("Invalid resize parameters"))?,
            None => (width as u32, height as u32),
        };

        Ok(Decoder {
            reader,
            decode_ctx,
            hw_context,
            time_base: time_base.into(),
            media_type: self.media_type,
            size: (width as u32, height as u32),
            size_out: (resize_width, resize_height),
            stream_index,
            draining: false,
        })
    }

    pub fn build_with_stream_reader(self) -> Result<Decoder<StreamReader>> {
        let source = self.source.clone();
        let mut reader_builder = StreamReaderBuilder::new(source);
        if let Some(format) = self.format {
            reader_builder = reader_builder.with_format(format);
        }
        if let Some(opts) = self.format_opts {
            reader_builder = reader_builder.with_options(opts);
        }
        self.build_from_reader(reader_builder.build().unwrap())
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
pub struct Decoder<R: Reader> {
    reader: R,
    decode_ctx: AVCodecContext,
    hw_context: Option<HWContext>,
    time_base: Rational,
    media_type: MediaType,
    stream_index: usize,
    size: (u32, u32),
    size_out: (u32, u32),
    draining: bool,
}

impl<R: Reader> Decoder<R> {
    /// Create a decoder to decode the specified source.
    ///
    /// # Arguments
    ///
    /// * `source` - Source to decode.
    #[inline]
    pub fn new(source: impl Into<Location>) -> Result<Decoder<StreamReader>> {
        DecoderBuilder::new(source).build_with_stream_reader()
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
    pub fn time_base(&self) -> Rational {
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

    pub fn current_stream(&self) -> Result<&AVStreamRef> {
        self.reader
            .input()
            .streams()
            .get(self.stream_index)
            .ok_or(Error::msg(format!(
                "stream: {} not found!",
                self.stream_index
            )))
    }

    /// Duration of the decoder stream.
    #[inline]
    pub fn duration(&self) -> Result<Time> {
        let stream = self.current_stream()?;
        Ok(Time::new(Some(stream.duration), stream.time_base.into()))
    }

    /// Number of frames in the decoder stream.
    #[inline]
    pub fn frames(&self) -> Result<u64> {
        Ok(self.current_stream()?.nb_frames.max(0) as u64)
    }

    /// Decode frames through iterator interface. This is similar to `decode` but it returns frames
    /// through an infinite iterator.
    ///
    /// # Example
    ///
    /// ```ignore
    /// decoder
    ///     .decode_iter()
    ///     .take_while(Result::is_ok)
    ///     .map(Result::unwrap)
    ///     .for_each(|(ts, frame)| {
    ///         // Do something with frame...
    ///     });
    /// ```
    #[cfg(feature = "ndarray")]
    pub fn decode_iter(&mut self) -> impl Iterator<Item = Result<(Time, FrameArray)>> + '_ {
        std::iter::from_fn(move || Some(self.decode()))
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
    pub fn decode(&mut self) -> Result<(Time, FrameArray)> {
        Ok(loop {
            if !self.draining {
                match self.read(self.stream_index) {
                    Ok(packet) => match self._decode(packet) {
                        Ok(Some(frame)) => break frame,
                        Ok(None) => {
                            log::debug!("[decode]: no frame decoded.");
                        }
                        Err(err) => return Err(err),
                    },
                    Err(err) => return Err(err),
                }
            } else {
                match self.drain() {
                    Ok(Some(frame)) => break frame,
                    Ok(None) => {
                        self.reset();
                    }
                    Err(err) => return Err(err),
                }
            }
        })
    }

    /// Decode frames through iterator interface. This is similar to `decode_raw` but it returns
    /// frames through an infinite iterator.
    pub fn decode_raw_iter(&mut self) -> impl Iterator<Item = Result<RawFrame>> + '_ {
        std::iter::from_fn(move || Some(self.decode_raw()))
    }

    /// Decode a single frame and return the raw ffmpeg `AvFrame`.
    ///
    /// # Return value
    ///
    /// The decoded raw frame as [`RawFrame`].
    pub fn decode_raw(&mut self) -> Result<RawFrame> {
        Ok(loop {
            if !self.draining {
                match self.read(self.stream_index) {
                    Ok(packet) => match self._decode_raw(packet) {
                        Ok(Some(frame)) => break frame,
                        Ok(None) => {
                            log::debug!("[decode_raw]: no frame decoded.");
                        }
                        Err(err) => return Err(err),
                    },
                    Err(err) => return Err(err),
                }
            } else {
                match self.drain_raw() {
                    Ok(Some(frame)) => break frame,
                    Ok(None) => {
                        self.reset();
                    }
                    Err(err) => return Err(err),
                }
            }
        })
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

    /// Get the decoders input frame rate as floating-point value.
    pub fn frame_rate(&self) -> f32 {
        avutil::av_q2d(
            self.current_stream()
                .map_or(avutil::ra(0, 1), |stream| stream.r_frame_rate),
        ) as f32
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
    fn _decode(&mut self, packet: Packet) -> Result<Option<(Time, FrameArray)>> {
        match self._decode_raw(packet)? {
            Some(mut frame) => Ok(Some(self.raw_frame_to_time_and_frame(&mut frame)?)),
            None => Ok(None),
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
    fn _decode_raw(&mut self, packet: Packet) -> Result<Option<RawFrame>> {
        assert!(!self.draining);
        self.send_packet_to_decoder(packet)?;
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
    pub fn drain(&mut self) -> Result<Option<(Time, FrameArray)>> {
        match self.drain_raw()? {
            Some(mut frame) => Ok(Some(self.raw_frame_to_time_and_frame(&mut frame)?)),
            None => Ok(None),
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
    pub fn drain_raw(&mut self) -> Result<Option<RawFrame>> {
        if !self.draining {
            self.send_eof()?;
            self.draining = true;
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
        self.draining = false;
    }

    pub fn flush(&mut self) {
        unsafe {
            ffi::avcodec_flush_buffers(self.decode_ctx.as_mut_ptr());
        }
    }

    /// Read a single packet from the source video file.
    ///
    /// # Arguments
    ///
    /// * `stream_index` - Index of stream to read from.
    ///
    /// # Example
    ///
    /// Read a single packet:
    ///
    /// ```ignore
    /// let mut reader = StreamReader::new(Path::new("my_video.mp4")).unwrap();
    /// let stream = reader.best_video_stream_index().unwrap();
    /// let mut packet = reader.read(stream).unwrap();
    /// ```
    pub fn read(&mut self, stream_index: usize) -> Result<Packet> {
        loop {
            match self.reader.read_packet() {
                Ok(Some((stream, packet))) => {
                    if stream.index() == stream_index {
                        return Ok(Packet::new(packet, stream.time_base()));
                    }
                    log::debug!("Skipping packet from stream: {}", stream.index());
                }
                Ok(None) => return Err(Error::msg("No more packets")),
                Err(e) => {
                    log::error!("Error reading packet: {}", e);
                    return Err(e);
                }
            }
        }
    }

    pub fn read_any(&mut self) -> Result<Packet> {
        match self.reader.read_packet() {
            Ok(Some((stream, packet))) => Ok(Packet::new(packet, stream.time_base())),
            Ok(None) => Err(Error::msg("No more packets")),
            Err(e) => {
                log::error!("Error reading packet: {}", e);
                Err(e)
            }
        }
    }

    /// Send packet to decoder. Includes rescaling timestamps accordingly.
    fn send_packet_to_decoder(&mut self, packet: Packet) -> Result<()> {
        let (mut packet, packet_time_base) = packet.into_inner_parts();
        packet.rescale_ts(packet_time_base.into(), self.time_base().into());

        self.decode_ctx.send_packet(Some(&packet))?;

        Ok(())
    }

    /// Receive packet from decoder. Will handle hwaccel conversions and scaling as well.
    fn receive_frame_from_decoder(&mut self) -> Result<Option<RawFrame>> {
        let frame_result = self.decoder_receive_frame()?;
        let Some(frame) = frame_result else {
            return Ok(None);
        };

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
            })?;

        // handle scale frame only for video
        if self.media_type != MediaType::VIDEO {
            return Ok(Some(sw_frame));
        }

        // handle scaling frame if needed (if not, size_out is the same as size)
        Ok(Some(self.rescale_frame(sw_frame)?))
    }

    /// Pull a decoded frame from the decoder. This function also implements retry mechanism in case
    /// the decoder signals `EAGAIN`.
    fn decoder_receive_frame(&mut self) -> Result<Option<RawFrame>> {
        let decode_result = self.decode_ctx.receive_frame();
        match decode_result {
            Ok(frame) => Ok(Some(frame)),
            Err(RsmpegError::DecoderDrainError) | Err(RsmpegError::DecoderFlushedError) => Ok(None),
            Err(e) => Err(Error::new(e).context("Failed to receive frame from decoder")),
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
            return frame::scale_frame(
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
impl<R: Reader> Drop for Decoder<R> {
    fn drop(&mut self) {
        // Maximum number of invocations to `decoder_receive_frame` to drain the items still on the
        // queue before giving up.
        const MAX_DRAIN_ITERATIONS: u32 = 100;

        // We need to drain the items still in the decoders queue.
        if let Ok(()) = self.send_eof() {
            for i in 0..MAX_DRAIN_ITERATIONS {
                match self.decoder_receive_frame() {
                    Ok(Some(_)) => {
                        // If receive a frame, we continue to drain the queue.
                        log::debug!("continue draining decoder, try:{}", i);
                        continue;
                    }
                    Ok(None) => {
                        log::debug!("Drained decoder.");
                        break;
                    }
                    Err(err) => {
                        log::error!("Failed to drain decoder: {}", err);
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

unsafe impl<R: Reader> Send for Decoder<R> {}
unsafe impl<R: Reader> Sync for Decoder<R> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_video() -> Result<()> {
        let path = std::path::Path::new("/tmp/bear.mp4");
        let mut decoder = Decoder::<StreamReader>::new(path)?;
        for res in decoder.decode_raw_iter() {
            match res {
                Ok(frame) => {
                    println!("{:?}", frame);
                }
                Err(e) => {
                    println!("Error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_decode_audio() -> Result<()> {
        let path = std::path::Path::new("/tmp/bear.mp4");
        let mut decoder = DecoderBuilder::new(path)
            .with_media_type(MediaType::AUDIO)
            .build_with_stream_reader()?;

        for res in decoder.decode_raw_iter() {
            match res {
                Ok(frame) => {
                    println!("{:?}", frame);
                }
                Err(e) => {
                    println!("Error: {}", e);
                    break;
                }
            }
        }
        Ok(())
    }
}
