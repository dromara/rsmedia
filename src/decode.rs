use crate::filter::FilterContext;
use crate::flags::AvCodecFlags;
#[cfg(feature = "ndarray")]
use crate::frame::{MediaFrame, MediaFrameType};
use crate::hwaccel::{HWContext, HWDeviceConfig};
use crate::io::Reader;
use crate::options::Options;
use crate::resize::Resize;
use crate::stream::StreamInfo;
use crate::{swctx, utils, MediaType, PixelFormat, RawFrame, SampleFormat, Time};

use rsmpeg::avcodec::{AVCodec, AVCodecContext, AVPacket};
use rsmpeg::avformat::AVStream;
use rsmpeg::avutil::{self, AVChannelLayout};
use rsmpeg::ffi;

use anyhow::{Context, Error, Result};
use std::sync::Arc;

/// Builds a [`Decoder`].
#[derive(Debug)]
pub struct DecoderBuilder {
    flags: AvCodecFlags,
    thread_count: usize,
    media_type: MediaType,
    resize: Option<Resize>,
    codec_name: Option<String>,
    codec_opts: Option<Options>,
    filter: Option<FilterContext>,
    hw_device_config: Option<HWDeviceConfig>,
}

impl DecoderBuilder {
    /// create a new decoder builder with specified media type.
    ///
    /// # Arguments
    ///
    /// `media_type` - The media type of the decoder.
    pub fn new(media_type: MediaType) -> Self {
        Self {
            media_type,
            filter: None,
            resize: None,
            codec_name: None,
            codec_opts: None,
            hw_device_config: None,
            thread_count: num_cpus::get(),
            flags: AvCodecFlags::LOW_DELAY,
        }
    }

    /// Create a video decoder
    pub fn new_video() -> Self {
        Self::new(MediaType::VIDEO)
    }

    /// create an audio decoder
    pub fn new_audio() -> Self {
        Self::new(MediaType::AUDIO)
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
        assert_eq!(
            self.media_type,
            MediaType::VIDEO,
            "Resizing is only supported for video"
        );
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

    /// set the thread count.
    pub fn with_thread_count(mut self, thread_count: usize) -> Self {
        self.thread_count = thread_count;
        self
    }

    pub fn with_filter(mut self, filter: Option<FilterContext>) -> Self {
        self.filter = filter;
        self
    }

    /// Enable hardware acceleration with the specified device type.
    ///
    /// * `device_config` - Device to use for hardware acceleration.
    pub fn with_hardware_device(mut self, device_config: Option<HWDeviceConfig>) -> Self {
        self.hw_device_config = device_config;
        self
    }

    fn setup_codec_context(&self, decoder: &mut AVCodecContext, input: &AVStream) -> Result<()> {
        let media_type = self.media_type;
        if media_type as ffi::AVMediaType != decoder.codec_type {
            return Err(Error::msg(format!(
                "Decoder codec type not supported: {:?} vs. {:?}",
                media_type, decoder.codec_type
            )));
        }

        decoder.apply_codecpar(&input.codecpar())?;
        decoder.set_flags(self.flags as i32);
        decoder.set_time_base(input.time_base);
        if let Some(framerate) = input.guess_framerate() {
            decoder.set_framerate(framerate);
        }

        unsafe {
            (*decoder.as_mut_ptr()).thread_count = self.thread_count as i32;
        }

        Ok(())
    }

    /// Build [`Decoder`].
    pub fn build<R: Reader>(self, reader: &R) -> Result<Decoder> {
        self.build_from_reader(reader)
    }

    pub fn build_from_reader<R: Reader>(self, reader: &R) -> Result<Decoder> {
        let media_type = self.media_type;
        let (stream_index, codec_name) = reader.find_best_stream(media_type)?;
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

        let duration = Time::new(Some(input_stream.duration), input_stream.time_base);
        let nb_frames = input_stream.nb_frames;
        let frame_rate = (
            avutil::av_q2d(input_stream.r_frame_rate) as f32,
            avutil::av_q2d(input_stream.avg_frame_rate) as f32,
        );

        let mut decode_ctx = AVCodecContext::new(&codec);
        self.setup_codec_context(&mut decode_ctx, input_stream)?;

        let (width, height) = (decode_ctx.width, decode_ctx.height);
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
                            "Decoder with HW acceleration is not supported for codec: {codec_name}"
                        ))
                    })?;

                log::info!(
                    "Video decoder with HW acceleration codec: {:?}, hw_pixel: {:?}, config: {:#?}",
                    codec.name(),
                    PixelFormat::from(hw_pixel),
                    cfg
                );

                // create hardware context
                HWContext::new(cfg)
                    .and_then(|ctx| {
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

        let rescale = if let Some(resize) = self.resize {
            let (resize_width, resize_height) = resize
                .compute_for((width as u32, height as u32))
                .ok_or(Error::msg("Invalid resize parameters"))?;
            Some((
                resize_width as usize,
                resize_height as usize,
                PixelFormat::from(decode_ctx.pix_fmt),
            ))
        } else {
            None
        };

        // audio resampling is not supported external settings available for now.
        let resample = if media_type == MediaType::AUDIO {
            Some((
                decode_ctx.ch_layout.nb_channels as usize,
                decode_ctx.sample_rate as usize,
                SampleFormat::from(decode_ctx.sample_fmt),
            ))
        } else {
            None
        };

        Ok(Decoder {
            rescale,
            resample,
            media_type,
            stream_index,
            duration,
            nb_frames,
            frame_rate,
            hw_context,
            context: decode_ctx,
            filter: self.filter,
            state: DecoderState::Normal,
        })
    }
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum DecoderState {
    Normal,
    Drained,
    Flushed,
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
    context: AVCodecContext,
    filter: Option<FilterContext>,
    hw_context: Option<Arc<HWContext>>,
    // video rescaling: (width, height, pixel_format)
    rescale: Option<(usize, usize, PixelFormat)>,
    // audio resampling: (nb_channels, sample_rate, sample_format)
    resample: Option<(usize, usize, SampleFormat)>,
    // (r_frame_rate, avg_frame_rate)
    frame_rate: (f32, f32),
    nb_frames: i64,
    duration: Time,
    stream_index: usize,
    media_type: MediaType,
    state: DecoderState,
}

impl Decoder {
    /// Create a decoder to decode the specified source.
    ///
    /// # Arguments
    ///
    /// * `reader` - A [`Reader`] to read the source from.
    #[inline]
    pub fn new_video<R: Reader>(reader: &R) -> Result<Decoder> {
        DecoderBuilder::new_video().build(reader)
    }

    /// Create a decoder to decode the audio stream of the specified source.
    ///
    /// # Arguments
    ///
    /// * `reader` - A [`Reader`] to read the source from.
    #[inline]
    pub fn new_audio<R: Reader>(reader: &R) -> Result<Decoder> {
        DecoderBuilder::new_audio().build(reader)
    }

    /// Get the decoders input size width
    #[inline(always)]
    pub fn width(&self) -> usize {
        self.context.width as usize
    }

    /// Get the decoders input size height
    #[inline(always)]
    pub fn height(&self) -> usize {
        self.context.height as usize
    }

    /// Get the decoders input duration
    #[inline(always)]
    pub fn duration(&self) -> Time {
        self.duration
    }

    /// Get decoder time base.
    #[inline(always)]
    pub fn time_base(&self) -> ffi::AVRational {
        self.duration.time_base
    }

    /// Get the decoders input stream number of frames
    #[inline(always)]
    pub fn frames(&self) -> i64 {
        self.nb_frames
    }

    /// Get the decoders input frame rate
    ///
    /// # Return
    /// A tuple of the frame rate of float values
    ///
    /// `0`: r_frame_rate
    /// `1`: avg_frame_rate
    ///
    #[inline(always)]
    pub fn frame_rate(&self) -> (f32, f32) {
        self.frame_rate
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
    pub fn is_drained(&self) -> bool {
        self.state == DecoderState::Drained
    }

    pub fn is_flushed(&self) -> bool {
        self.state == DecoderState::Flushed
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
    pub fn decode<T>(&mut self, reader: &mut impl Reader) -> Result<Option<MediaFrame<T>>>
    where
        T: MediaFrameType,
    {
        if self.is_flushed() {
            return Err(Error::msg(
                "Decoder cannot decode after flushed. Call reset().",
            ));
        }

        let mut read_exhausted = false;
        Ok(loop {
            if !read_exhausted {
                match reader.read_packet() {
                    Ok(Some((stream, packet))) => {
                        if stream.index() != self.stream_index() {
                            // skip other streams
                            log::debug!("skip stream index: {}, {:?}", stream.index(), packet);
                            continue;
                        }
                        if let Some(frame) = self.decode_packet(&packet)? {
                            break Some(frame);
                        }
                    }
                    Ok(None) => {
                        log::debug!("No more packets, Reader exhausted.");
                        read_exhausted = true;
                        continue;
                    }
                    Err(e) => {
                        log::error!("Error reading packet: {}", e);
                        return Err(e);
                    }
                }
            } else {
                match self.drain() {
                    Ok(Some(frame)) => {
                        break Some(frame);
                    }
                    Ok(None) => {
                        log::debug!("Decoder flushed. EOF reached.");
                        // self.reset();
                        // read_exhausted = false;
                        break None;
                    }
                    Err(e) => {
                        log::error!("Error to drain decoder: {}", e);
                        return Err(e);
                    }
                }
            }
        })
    }

    /// Decode a single frame and return the raw ffmpeg `AvFrame`.
    ///
    /// # Arguments
    ///
    /// * `reader` - A [`Reader`] to read the source from.
    ///
    /// # Return value
    ///
    /// The decoded raw frame as [`RawFrame`].
    pub fn decode_raw<R>(&mut self, reader: &mut R) -> Result<Option<RawFrame>>
    where
        R: Reader,
    {
        if self.is_flushed() {
            return Err(Error::msg(
                "Decoder cannot decode after flushed. Call reset().",
            ));
        }

        let mut read_exhausted = false;
        Ok(loop {
            if !read_exhausted {
                match reader.read_packet() {
                    Ok(Some((stream, packet))) => {
                        if stream.index() != self.stream_index() {
                            // skip other streams
                            log::debug!("skip stream index: {}, {:?}", stream.index(), packet);
                            continue;
                        }
                        if let Some(frame) = self.decode_raw_packet(&packet)? {
                            break Some(frame);
                        }
                    }
                    Ok(None) => {
                        log::debug!("No more packets, Reader exhausted.");
                        read_exhausted = true;
                        continue;
                    }
                    Err(e) => {
                        log::error!("Error reading packet: {}", e);
                        return Err(e);
                    }
                }
            } else {
                match self.drain_raw() {
                    Ok(Some(frame)) => {
                        break Some(frame);
                    }
                    Ok(None) => {
                        log::debug!("Decoder flushed. EOF reached.");
                        // self.reset();
                        // read_exhausted = false;
                        break None;
                    }
                    Err(e) => {
                        log::error!("Error to drain decoder: {}", e);
                        return Err(e);
                    }
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
    pub fn decode_packet<T>(&mut self, packet: &AVPacket) -> Result<Option<MediaFrame<T>>>
    where
        T: MediaFrameType,
    {
        match self.decode_raw_packet(packet) {
            Ok(Some(raw_frame)) => Ok(Some(self.raw_frame_to_media_frame(&raw_frame)?)),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
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
    pub fn decode_raw_packet(&mut self, packet: &AVPacket) -> Result<Option<RawFrame>> {
        self.send_packet_to_decoder(Some(packet))?;
        self.receive_frame_from_decoder()
    }

    /// Drain one frame from the decoder.
    ///
    /// After calling drain once the decoder is in draining mode and the caller may not use normal
    /// decode anymore, or it will panic.
    ///
    /// # Return value
    ///
    /// A tuple of the [`Frame`] and timestamp (relative to the stream) and the frame itself if the
    /// decoder has a frame available, [`None`] if not.
    #[cfg(feature = "ndarray")]
    pub fn drain<T>(&mut self) -> Result<Option<MediaFrame<T>>>
    where
        T: MediaFrameType,
    {
        match self.drain_raw() {
            Ok(Some(raw_frame)) => Ok(Some(self.raw_frame_to_media_frame(&raw_frame)?)),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Drain one frame from the decoder.
    ///
    /// After calling drain once the decoder is in draining mode and the caller may not use normal
    /// decode anymore, or it will panic.
    ///
    /// # Return value
    ///
    /// The decoded raw frame as [`RawFrame`] if the decoder has a frame available, [`None`] if not.
    pub fn drain_raw(&mut self) -> Result<Option<RawFrame>> {
        if !self.is_drained() {
            self.send_packet_to_decoder(None)?;
        }
        self.receive_frame_from_decoder()
    }

    /// Reset the decoder to be used again after draining.
    pub fn reset(&mut self) {
        self.flush();
        self.state = DecoderState::Normal;
    }

    fn flush(&mut self) {
        unsafe {
            ffi::avcodec_flush_buffers(self.context.as_mut_ptr());
        }
    }

    /// Send packet to decoder.
    /// Ensure rescaling timestamps accordingly before sending to decoder.
    fn send_packet_to_decoder(&mut self, packet: Option<&AVPacket>) -> Result<()> {
        self.context
            .send_packet(packet)
            .context("Failed to send packet to decoder")?;
        Ok(())
    }

    /// Receive packet from decoder. Will handle hwaccel conversions and scaling as well.
    fn receive_frame_from_decoder(&mut self) -> Result<Option<RawFrame>> {
        let frame = match self.decoder_receive_frame() {
            Ok(Some(f)) => f,
            Ok(None) => return Ok(None),
            Err(e) => return Err(e),
        };

        let res = match self.media_type {
            MediaType::VIDEO => self.process_video_frame(frame),
            MediaType::AUDIO => self.process_audio_frame(frame),
            _ => Ok(frame),
        };

        match res {
            Ok(raw_frame) => {
                if let Some(filter) = self.filter.as_mut() {
                    match filter.process_frame(Some(raw_frame.clone()))? {
                        Some(f) => Ok(Some(f)),
                        None => {
                            log::warn!("Filter returned None, keeping original frame.");
                            Ok(Some(raw_frame))
                        }
                    }
                } else {
                    Ok(Some(raw_frame))
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Pull a decoded frame from the decoder. This function also implements retry mechanism in case
    /// the decoder signals `EAGAIN` and `EOF`
    fn decoder_receive_frame(&mut self) -> Result<Option<RawFrame>> {
        match self.context.receive_frame() {
            Ok(frame) => Ok(Some(frame)),
            Err(rsmpeg::error::RsmpegError::DecoderDrainError) => {
                log::debug!("Decoder drained. try send new packet again.");
                self.state = DecoderState::Drained;
                Ok(None)
            }
            Err(rsmpeg::error::RsmpegError::DecoderFlushedError) => {
                log::debug!("Decoder flushed. EOF reached.");
                self.state = DecoderState::Flushed;
                Ok(None)
            }
            Err(e) => {
                log::warn!("Failed to receive frame from decoder: {}", e);
                Err(Error::new(e))
            }
        }
    }

    /// Rescale frame if needed.
    fn process_video_frame(&mut self, hw_frame: RawFrame) -> Result<RawFrame> {
        // hardware acceleration decoding
        let sw_frame = self
            .hw_context
            .as_ref()
            .and_then(|hw_ctx| {
                if hw_ctx.is_hw_frame(&hw_frame) {
                    Some(hw_ctx.hw_download(&mut self.context, &hw_frame))
                } else {
                    log::warn!("Hardware acceleration decoding not available!");
                    None
                }
            })
            .map_or(Ok(hw_frame), |result| {
                result.map_err(|e| {
                    log::error!("Failed to download frame from hw_device: {}", e);
                    Error::msg(format!("HW frame download failed: {}", e))
                })
            })?;

        // rescale
        if let Some((dst_w, dst_h, pix_fmt)) = self.rescale {
            let is_scale_needed = !(sw_frame.format == pix_fmt.into()
                && sw_frame.width == dst_w as i32
                && sw_frame.height == dst_h as i32);
            if is_scale_needed {
                swctx::scale(&sw_frame, dst_w as i32, dst_h as i32, pix_fmt)
            } else {
                Ok(sw_frame)
            }
        } else {
            Ok(sw_frame)
        }
    }

    fn process_audio_frame(&mut self, frame: RawFrame) -> Result<RawFrame> {
        if let Some((nb_channels, sample_rate, sample_fmt)) = self.resample {
            let src_sample_rate = frame.sample_rate;
            let src_nb_channels = frame.ch_layout.nb_channels;

            let is_resample_needed = !(src_nb_channels == nb_channels as i32
                && src_sample_rate == sample_rate as i32
                && frame.format == sample_fmt as i32);

            let out_frame = if is_resample_needed {
                swctx::convert_frame(
                    &frame,
                    AVChannelLayout::from_nb_channels(nb_channels as i32).into_inner(),
                    sample_rate as i32,
                    sample_fmt as i32,
                )?
            } else {
                frame
            };

            // ensure timebase are correct
            // let dst_time_base = self.decode_ctx.time_base;
            // if src_pts != ffi::AV_NOPTS_VALUE {
            //     let new_pts = avutil::av_rescale_q(src_pts, src_time_base, dst_time_base);
            //     out_frame.set_pts(new_pts);
            //     out_frame.set_time_base(dst_time_base);
            // }

            Ok(out_frame)
        } else {
            Ok(frame)
        }
    }

    #[cfg(feature = "ndarray")]
    fn raw_frame_to_media_frame<T>(&self, frame: &RawFrame) -> Result<MediaFrame<T>>
    where
        T: MediaFrameType,
    {
        // AVFrame default pixel is YUV420P, So here keeping the format that YUV420P the same
        // after I convert it, If you want RGB24, always remember to convert it yourself!
        let frame = MediaFrame::<T>::from_avframe(frame)?;

        Ok(frame)
    }
}

/// Important note: Do not forget to drain the decoder after the reader is exhausted. It may still
/// contain frames. Run `drain_raw()` or `drain()` in a loop until no more frames are produced.
impl Drop for Decoder {
    fn drop(&mut self) {
        // flush filter before drop
        if let Some(filter) = self.filter.as_mut() {
            let _frames = filter.flush().context("Failed to flush filter").unwrap();
        }

        // We need to drain the items still in the decoders queue.
        match self.send_packet_to_decoder(None) {
            Ok(_) => {
                loop {
                    match self.decoder_receive_frame() {
                        Ok(Some(_frame)) => {
                            // If receive a frame, we continue to drain the queue.
                            log::debug!("continue draining decoder queue.");
                        }
                        Ok(None) => {
                            if self.is_drained() {
                                // If we need more, we continue to drain the queue.
                                log::debug!("Decoder drained. try send new packet again.");
                                continue;
                            } else {
                                log::debug!("Decoder flushed. EOF reached.");
                                break;
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to drain decoder: {}", e);
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to send flush packet to decoder: {}", e)
            }
        }

        unsafe {
            // explicitly drop the hw_context to release the hardware resources
            // 1. malloc(): unsorted double linked list corrupted
            // 2. malloc(): mismatching next->prev_size (unsorted)
            // 3. free(): invalid pointer
            // 4. double free or corruption (!prev)
            // 5. corrupted double-linked list Aborted (core dumped)
            let codec_ctx_ptr = self.context.as_mut_ptr();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::{self, AudioParams, FilterConfig, FilterParams, VideoParams};
    use crate::io::StreamReader;

    #[test]
    #[ignore = "need a video file"]
    fn test_decode_video() -> Result<()> {
        let path = std::path::Path::new("/tmp/bear.mp4");

        let video_filter_params = FilterParams::Video(VideoParams {
            width: 640,
            height: 360,
            format: PixelFormat::YUV420P,
            time_base: ffi::AVRational { num: 1, den: 25 },
            frame_rate: ffi::AVRational { num: 25, den: 1 },
            pixel_aspect: ffi::AVRational { num: 1, den: 1 },
        });

        let filters = vec![
            filter::video::scale(1280, 720, PixelFormat::RGB24),
            filter::video::drawtext("Hello", 10, 10, 24, "white"),
        ];

        let video_filter_config = FilterConfig {
            params: video_filter_params,
            filters,
        };

        let mut stream_reader = StreamReader::new(path)?;
        let mut decoder = DecoderBuilder::new_video()
            .with_filter(Some(FilterContext::new(video_filter_config)?))
            .build(&stream_reader)
            .unwrap();
        loop {
            match decoder.decode_raw(&mut stream_reader) {
                Ok(Some(frame)) => {
                    println!("video frame: {:?}, timebase:{:?}", frame, frame.time_base);
                }
                Ok(None) => {
                    println!("No more frames, decoder flushed");
                    break;
                }
                Err(e) => {
                    log::error!("Error decoding frame: {}", e);
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

        let audio_filter_params = FilterParams::Audio(AudioParams {
            nb_channels: 2,
            sample_rate: 44100,
            format: SampleFormat::FLTP,
            time_base: ffi::AVRational { num: 1, den: 44100 },
        });

        let filters = vec![
            filter::audio::resample(2, 48000, SampleFormat::FLTP),
            filter::audio::volume(1.5),
            filter::audio::loudnorm(-16.0),
        ];

        let audio_filter_config = FilterConfig {
            params: audio_filter_params,
            filters,
        };

        let mut stream_reader = StreamReader::new(path)?;
        let mut decoder = DecoderBuilder::new_audio()
            .with_filter(Some(FilterContext::new(audio_filter_config)?))
            .build(&stream_reader)
            .unwrap();
        loop {
            match decoder.decode_raw(&mut stream_reader) {
                Ok(Some(frame)) => {
                    println!("audio frame: {:?}, timebase:{:?}", frame, frame.time_base);
                }
                Ok(None) => {
                    println!("No more frames, decoder flushed");
                    break;
                }
                Err(e) => {
                    log::error!("Error decoding frame: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(())
    }
}
