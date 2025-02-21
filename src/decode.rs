#[cfg(feature = "ndarray")]
use crate::frame::{self, FrameArray};
use crate::hwaccel::{HWContext, HWDeviceType};
use crate::io::{Reader, ReaderBuilder};
use crate::location::Location;
use crate::options::Options;
use crate::packet::Packet;
use crate::resize::Resize;
use crate::time::Time;
use crate::{Rational, RawFrame};

use anyhow::{Context, Error, Result};
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi;

/// Builds a [`Decoder`].
pub struct DecoderBuilder<'a> {
    source: Location,
    options: Option<&'a Options>,
    resize: Option<Resize>,
    hw_device_type: Option<HWDeviceType>,
}

impl<'a> DecoderBuilder<'a> {
    /// Create a decoder with the specified source.
    ///
    /// * `source` - Source to decode.
    pub fn new(source: impl Into<Location>) -> Self {
        Self {
            source: source.into(),
            options: None,
            resize: None,
            hw_device_type: None,
        }
    }

    /// Set custom options. Options are applied to the input.
    ///
    /// * `options` - Custom options.
    pub fn with_options(mut self, options: &'a Options) -> Self {
        self.options = Some(options);
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

    /// Build [`Decoder`].
    pub fn build(self) -> Result<Decoder> {
        let mut reader_builder = ReaderBuilder::new(self.source);
        if let Some(options) = self.options {
            reader_builder = reader_builder.with_options(options);
        }
        let reader = reader_builder.build().unwrap();
        let reader_stream_index = reader.best_video_stream_index().unwrap();
        let stream_info = reader.stream_info(reader_stream_index).unwrap();
        tracing::info!(
            "decoder stream index: {} stream_info: {}",
            reader_stream_index,
            stream_info
        );

        Ok(Decoder {
            decoder: DecoderSplit::new(
                &reader,
                reader_stream_index,
                self.resize,
                self.hw_device_type,
            )?,
            reader,
            reader_stream_index,
            draining: false,
        })
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
    decoder: DecoderSplit,
    reader: Reader,
    reader_stream_index: usize,
    draining: bool,
}

impl Decoder {
    /// Create a decoder to decode the specified source.
    ///
    /// # Arguments
    ///
    /// * `source` - Source to decode.
    #[inline]
    pub fn new(source: impl Into<Location>) -> Result<Self> {
        DecoderBuilder::new(source).build()
    }

    /// Get decoder time base.
    #[inline]
    pub fn time_base(&self) -> Rational {
        self.decoder.time_base()
    }

    /// Duration of the decoder stream.
    #[inline]
    pub fn duration(&self) -> Result<Time> {
        let reader_stream = self
            .reader
            .input
            .streams()
            .get(self.reader_stream_index)
            .ok_or(RsmpegError::FindStreamInfoError(
                ffi::AVERROR_STREAM_NOT_FOUND,
            ))?;
        Ok(Time::new(
            Some(reader_stream.duration),
            reader_stream.time_base.into(),
        ))
    }

    /// Number of frames in the decoder stream.
    #[inline]
    pub fn frames(&self) -> Result<u64> {
        Ok(self
            .reader
            .input
            .streams()
            .get(self.reader_stream_index)
            .ok_or(RsmpegError::FindStreamInfoError(
                ffi::AVERROR_STREAM_NOT_FOUND,
            ))?
            .nb_frames
            .max(0) as u64)
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
                match self.reader.read(self.reader_stream_index) {
                    Ok(packet) => match self.decoder.decode(packet) {
                        Ok(Some(frame)) => break frame,
                        Ok(None) => {}
                        Err(err) => return Err(err),
                    },
                    Err(err) => return Err(err),
                }
                // FIXME: ReadExhausted
                // if matches!(packet_result, Err(MediaError::ReadExhausted)) {
                //     self.draining = true;
                //     continue;
                // }
            } else {
                match self.decoder.drain() {
                    Ok(Some(frame)) => break frame,
                    // FIXME: ReadExhausted
                    // Ok(None) | Err(MediaError::ReadExhausted) => {
                    //     self.decoder.reset();
                    //     self.draining = false;
                    //     return Err(MediaError::DecodeExhausted);
                    // }
                    Ok(None) => {
                        self.decoder.reset();
                        self.draining = false;
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
                match self.reader.read(self.reader_stream_index) {
                    Ok(packet) => match self.decoder.decode_raw(packet) {
                        Ok(Some(frame)) => break frame,
                        Ok(None) => {}
                        Err(err) => return Err(err),
                    },
                    Err(err) => return Err(err),
                }
                // FIXME: ReadExhausted
                // if matches!(packet_result, Err(MediaError::ReadExhausted)) {
                //     self.draining = true;
                //     continue;
                // }
            } else {
                match self.decoder.drain_raw() {
                    Ok(Some(frame)) => break frame,
                    // FIXME: ReadExhausted
                    // Ok(None) | Err(MediaError::ReadExhausted) => {
                    //     self.decoder.reset();
                    //     self.draining = false;
                    //     return Err(MediaError::DecodeExhausted);
                    // }
                    Ok(None) => {
                        self.decoder.reset();
                        self.draining = false;
                    }
                    Err(err) => return Err(err),
                }
            }
        })
    }

    /// Seek in reader.
    ///
    /// See [`Reader::seek`](crate::io::Reader::seek) for more information.
    #[inline]
    pub fn seek(&mut self, timestamp_milliseconds: i64) -> Result<()> {
        self.reader
            .seek(timestamp_milliseconds)
            .inspect(|_| self.decoder.flush())
    }

    /// Seek to specific frame in reader.
    ///
    /// See [`Reader::seek_to_frame`](crate::io::Reader::seek_to_frame) for more information.
    #[inline]
    pub fn seek_to_frame(&mut self, frame_number: i64) -> Result<()> {
        self.reader
            .seek_to_frame(frame_number)
            .inspect(|_| self.decoder.flush())
    }

    /// Seek to start of reader.
    ///
    /// See [`Reader::seek_to_start`](crate::io::Reader::seek_to_start) for more information.
    #[inline]
    pub fn seek_to_start(&mut self) -> Result<()> {
        self.reader
            .seek_to_start()
            .inspect(|_| self.decoder.flush())
    }

    /// Split the decoder into a decoder (of type [`DecoderSplit`]) and a [`Reader`].
    ///
    /// This allows the caller to detach stream reading from decoding, which is useful for advanced
    /// use cases.
    ///
    /// # Return value
    ///
    /// Tuple of the [`DecoderSplit`], [`Reader`] and the reader stream index.
    #[inline]
    pub fn into_parts(self) -> (DecoderSplit, Reader, usize) {
        (self.decoder, self.reader, self.reader_stream_index)
    }

    /// Get the decoders input size (resolution dimensions): width and height.
    #[inline(always)]
    pub fn size(&self) -> (u32, u32) {
        self.decoder.size
    }

    /// Get the decoders output size after resizing is applied (resolution dimensions): width and
    /// height.
    #[inline(always)]
    pub fn size_out(&self) -> (u32, u32) {
        self.decoder.size_out
    }

    /// Get the decoders input frame rate as floating-point value.
    pub fn frame_rate(&self) -> f32 {
        let frame_rate = self
            .reader
            .input
            .streams()
            .get(self.reader_stream_index)
            .map(|stream| Rational::from(stream.r_frame_rate));

        if let Some(frame_rate) = frame_rate {
            if frame_rate.denominator() > 0 {
                (frame_rate.numerator() as f32) / (frame_rate.denominator() as f32)
            } else {
                0.0
            }
        } else {
            0.0
        }
    }
}

/// Decoder part of a split [`Decoder`] and [`Reader`].
///
/// Important note: Do not forget to drain the decoder after the reader is exhausted. It may still
/// contain frames. Run `drain_raw()` or `drain()` in a loop until no more frames are produced.
pub struct DecoderSplit {
    decoder: AVCodecContext,
    decoder_time_base: Rational,
    hw_context: Option<HWContext>,
    size: (u32, u32),
    size_out: (u32, u32),
    draining: bool,
}

impl DecoderSplit {
    /// Create a new [`DecoderSplit`].
    ///
    /// # Arguments
    ///
    /// * `reader` - [`Reader`] to initialize decoder from.
    /// * `resize` - Optional resize strategy to apply to frames.
    pub fn new(
        reader: &Reader,
        reader_stream_index: usize,
        resize: Option<Resize>,
        hw_device_type: Option<HWDeviceType>,
    ) -> Result<Self> {
        let reader_stream = reader.input.streams().get(reader_stream_index).ok_or(
            RsmpegError::FindStreamInfoError(ffi::AVERROR_STREAM_NOT_FOUND),
        )?;

        let decoder = AVCodec::find_decoder(reader_stream.codecpar().codec_id)
            .context("Failed to find decoder for stream")?;

        let mut decode_ctx = AVCodecContext::new(&decoder);
        decode_ctx.set_time_base(reader_stream.time_base);
        decode_ctx.apply_codecpar(&reader_stream.codecpar())?;
        let (width, height) = (decode_ctx.width, decode_ctx.height);

        let hw_context = match hw_device_type {
            Some(device_type) => {
                let hw_ctx = HWContext::new(device_type.auto_best_device().unwrap())?;
                hw_ctx.setup_hw_frames(&mut decode_ctx, width, height)?;
                Some(hw_ctx)
            }
            None => None,
        };

        decode_ctx
            .open(None)
            .context("Failed to open decoder for stream")?;

        let (resize_width, resize_height) = match resize {
            Some(resize) => resize
                .compute_for((width as u32, height as u32))
                .ok_or(Error::msg("Invalid resize parameters"))?,
            None => (width as u32, height as u32),
        };

        let size = (width as u32, height as u32);
        let size_out = (resize_width, resize_height);

        Ok(Self {
            decoder: decode_ctx,
            decoder_time_base: reader_stream.time_base.into(),
            hw_context,
            size,
            size_out,
            draining: false,
        })
    }

    /// Get decoder time base.
    #[inline]
    pub fn time_base(&self) -> Rational {
        self.decoder_time_base
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
    pub fn decode(&mut self, packet: Packet) -> Result<Option<(Time, FrameArray)>> {
        match self.decode_raw(packet)? {
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
    pub fn decode_raw(&mut self, packet: Packet) -> Result<Option<RawFrame>> {
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
        Ok(self.decoder.send_packet(None)?)
    }

    /// Reset the decoder to be used again after draining.
    pub fn reset(&mut self) {
        self.flush();
        self.draining = false;
    }

    pub fn flush(&mut self) {
        unsafe {
            ffi::avcodec_flush_buffers(self.decoder.as_mut_ptr());
        }
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

    /// Send packet to decoder. Includes rescaling timestamps accordingly.
    fn send_packet_to_decoder(&mut self, packet: Packet) -> Result<()> {
        let (mut packet, packet_time_base) = packet.into_inner_parts();
        packet.rescale_ts(packet_time_base.into(), self.decoder_time_base.into());

        self.decoder.send_packet(Some(&packet))?;

        Ok(())
    }

    /// Receive packet from decoder. Will handle hwaccel conversions and scaling as well.
    fn receive_frame_from_decoder(&mut self) -> Result<Option<RawFrame>> {
        match self.decoder_receive_frame().unwrap() {
            Some(frame) => match self.hw_context.as_ref() {
                Some(hw_ctx) => {
                    if self.decoder.is_hwaccel() && hw_ctx.is_hw_frame(&frame) {
                        let f = match hw_ctx.download_frame(&mut self.decoder, &frame) {
                            Ok(f) => Some(f),
                            Err(e) => {
                                tracing::error!("Failed to download frame from hw_device: {}", e);
                                None
                            }
                        };
                        Ok(f)
                    } else {
                        tracing::debug!("Hardware decoding not available or not applicable");
                        Ok(Some(frame))
                    }
                }
                _ => Ok(Some(frame)),
            },
            None => Ok(None),
        }
    }

    /// Pull a decoded frame from the decoder. This function also implements retry mechanism in case
    /// the decoder signals `EAGAIN`.
    fn decoder_receive_frame(&mut self) -> Result<Option<RawFrame>> {
        let decode_result = self.decoder.receive_frame();
        match decode_result {
            Ok(frame) => Ok(Some(frame)),
            Err(RsmpegError::DecoderDrainError) | Err(RsmpegError::DecoderFlushedError) => Ok(None),
            Err(e) => Err(Error::new(e).context("Failed to receive frame from decoder")),
        }
    }

    #[cfg(feature = "ndarray")]
    fn raw_frame_to_time_and_frame(&self, frame: &mut RawFrame) -> Result<(Time, FrameArray)> {
        // We use the packet DTS here (which is `frame->pkt_dts`) because that is what the
        // encoder will use when encoding for the `PTS` field.
        let timestamp = Time::new(Some(frame.pkt_dts), self.decoder_time_base);
        // AVFrame default pixel is YUV420P, So here keeping the format that YUV420P the same
        // after I convert it, If you want RGB24, always remember to convert it yourself!
        let frame = frame::avframe_yuv_to_ndarray(frame).unwrap();

        Ok((timestamp, frame))
    }
}

impl Drop for DecoderSplit {
    fn drop(&mut self) {
        // Maximum number of invocations to `decoder_receive_frame` to drain the items still on the
        // queue before giving up.
        const MAX_DRAIN_ITERATIONS: u32 = 100;

        // We need to drain the items still in the decoders queue.
        if let Ok(()) = self.send_eof() {
            for _ in 0..MAX_DRAIN_ITERATIONS {
                if self.decoder_receive_frame().is_err() {
                    break;
                }
            }
        }
    }
}

unsafe impl Send for DecoderSplit {}
unsafe impl Sync for DecoderSplit {}
