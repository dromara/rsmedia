use crate::flags::MediaType;
use crate::location::Location;
use crate::options::Options;
use crate::stream::Stream;
use crate::utils;

use rsmpeg::avcodec::{AVCodecParameters, AVPacket};
use rsmpeg::avformat::{AVFormatContextInput, AVFormatContextOutput, AVInputFormat};
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi;

use anyhow::{Context, Error, Result};
use std::ops::{Bound, Deref};

pub trait Reader {
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    fn input(&self) -> &AVFormatContextInput;
    fn input_mut(&mut self) -> &mut AVFormatContextInput;

    fn read_packet(&mut self) -> Result<Option<(Stream<'_>, AVPacket)>> {
        match self.input_mut().read_packet() {
            Ok(Some(pkt)) => {
                let av_stream = self
                    .input()
                    .streams()
                    .get(pkt.stream_index as usize)
                    .unwrap();
                let iformat = self.input().iformat();
                let metadata = self.input().metadata();
                Ok(Some((Stream::wrap(av_stream, iformat, metadata), pkt)))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(Error::new(e)),
        }
    }

    /// Find the best stream
    ///
    /// # Arguments
    ///
    /// * `media_type` - MediaType maybe Video, Audio, etc.
    fn find_best_stream(&self, media_type: MediaType) -> Result<(usize, String)> {
        self.input()
            .find_best_stream(media_type as _)?
            .map(|(index, codec)| (index, utils::to_string(codec.name()).unwrap()))
            .ok_or(Error::msg(format!(
                "No stream found for MediaType:{media_type:?}"
            )))
    }
}

/// Builds a [`StreamReader`].
///
/// # Example
///
/// ```rust,ignore
/// let mut options = HashMap::new();
/// options.insert(
///     "rtsp_transport".to_string(),
///     "tcp".to_string(),
/// );
///
/// let mut reader = StreamReaderBuilder::new(Path::new("my_file.mp4"))
///    .with_options(&options.into())
///    .build()
///    .unwrap();
/// ```
pub struct StreamReaderBuilder<'a> {
    source: Location,
    format: Option<&'a str>,
    options: Option<Options>,
}

impl<'a> StreamReaderBuilder<'a> {
    /// Create a new reader with the specified locator.
    ///
    /// # Arguments
    ///
    /// * `source` - Source to read.
    pub fn new(source: impl Into<Location>) -> Self {
        Self {
            source: source.into(),
            format: None,
            options: None,
        }
    }

    /// Specify a custom format for the reader.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    pub fn with_format(mut self, format: &'a str) -> Self {
        self.format = Some(format);
        self
    }

    /// Specify options for the backend.
    ///
    /// # Arguments
    ///
    /// * `options` - Options to pass on to input.
    pub fn with_options(mut self, options: impl Into<Option<Options>>) -> Self {
        self.options = options.into();
        self
    }

    /// Build [`StreamReader`].
    pub fn build(self) -> Result<StreamReader> {
        let src_path = self.source.as_path().to_str().unwrap();
        let protocol =
            unsafe { ffi::avio_find_protocol_name(utils::to_c_char(src_path) as *const _) };
        if protocol.is_null() {
            return Err(Error::msg(format!(
                "Unsupported input source protocol: {src_path}"
            )));
        }
        log::debug!(
            "Using input protocol: [{}], source: {}",
            unsafe { utils::from_c_char(protocol) },
            src_path
        );

        let filename = utils::from_path(&self.source.as_path());
        let fmt_opt = self
            .format
            .and_then(|str| AVInputFormat::find(&utils::from_str(str)));
        let mut dict = self.options.map(|opts| opts.into_dict());
        let mut ctx_input = AVFormatContextInput::builder()
            .url(&filename)
            .maybe_format(fmt_opt.as_deref())
            .options(&mut dict)
            .open()
            .context("Create input format context failed.")?;
        ctx_input
            .dump(0, &filename)
            .context("Dump input format context failed.")?;
        Ok(StreamReader {
            source: self.source,
            input: ctx_input,
        })
    }
}

/// Video reader that can read from files.
pub struct StreamReader {
    pub source: Location,
    pub input: AVFormatContextInput,
}

impl StreamReader {
    /// Create a new video file reader on a given source (path, URL, etc.).
    ///
    /// # Arguments
    ///
    /// * `source` - Source to read from.
    #[inline]
    pub fn new(source: impl Into<Location>) -> Result<Self> {
        StreamReaderBuilder::new(source).build()
    }

    /// Seek in reader. This will change the reader head so that it points to a location within one
    /// second of the target timestamp or it will return an error.
    ///
    /// # Arguments
    ///
    /// * `timestamp_milliseconds` - Number of millisecond from start of video to seek to.
    pub fn seek_to_timestamp(&mut self, timestamp_milliseconds: i64) -> Result<()> {
        // Conversion factor from timestamp in milliseconds to `TIME_BASE` units.
        const CONVERSION_FACTOR: i64 = (ffi::AV_TIME_BASE_Q.den / 1000) as i64;
        // One second left and right leeway when seeking.
        const LEEWAY: i64 = ffi::AV_TIME_BASE_Q.den as i64;

        let timestamp = CONVERSION_FACTOR * timestamp_milliseconds;
        let range = timestamp - LEEWAY..timestamp + LEEWAY;

        self._seek(timestamp, range)
            .context("Failed to seek timestamp in reader")?;

        Ok(())
    }

    /// Seek to start of reader. This function performs best effort seeking to the start of the
    /// file.
    pub fn seek_to_start(&mut self) -> Result<()> {
        self._seek(i64::MIN, ..)
            .context("Failed to seek to start of reader")?;
        Ok(())
    }

    /// Seek to a specific frame in the video stream.
    ///
    /// # Arguments
    ///
    /// * `stream_index` - The index of the stream to seek to.
    /// * `frame_ts` - The timestamp of the target frame. This is typically derived from the frame's presentation timestamp (PTS).
    /// * `flags` - Flags to use when seeking. Possible values include:
    ///   - `AVSEEK_FLAG_BACKWARD` (1) <- Seek backward.
    ///   - `AVSEEK_FLAG_BYTE` (2) <- Seek based on position in bytes.
    ///   - `AVSEEK_FLAG_ANY` (4) <- Seek to any frame, even non-key frames.
    ///   - `AVSEEK_FLAG_FRAME` (8) <- Seek based on frame number.
    ///
    pub fn seek_to_frame(&mut self, stream_index: usize, frame_ts: i64, flags: i32) -> Result<()> {
        unsafe {
            let res = ffi::av_seek_frame(
                self.input.as_mut_ptr(),
                stream_index as i32,
                frame_ts,
                flags,
            );
            if res < 0 {
                return Err(Error::msg(format!("Seek to frame failed: {res}")));
            }
            Ok(())
        }
    }

    fn _seek<R: std::ops::RangeBounds<i64>>(&mut self, ts: i64, range: R) -> Result<()> {
        let start = match range.start_bound().cloned() {
            Bound::Included(i) => i,
            Bound::Excluded(i) => i.saturating_add(1),
            Bound::Unbounded => i64::MIN,
        };

        let end = match range.end_bound().cloned() {
            Bound::Included(i) => i,
            Bound::Excluded(i) => i.saturating_sub(1),
            Bound::Unbounded => i64::MAX,
        };

        unsafe {
            let res = ffi::avformat_seek_file(self.input.as_mut_ptr(), -1, start, ts, end, 0);
            if res < 0 {
                // >=0 on success, error code otherwise
                return Err(Error::msg(format!("Seek file failed: {res}")));
            }
            Ok(())
        }
    }
}

impl Reader for StreamReader {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn input(&self) -> &AVFormatContextInput {
        &self.input
    }

    fn input_mut(&mut self) -> &mut AVFormatContextInput {
        &mut self.input
    }
}

unsafe impl Send for StreamReader {}
unsafe impl Sync for StreamReader {}

/// Any type that implements this can write video packets.
pub trait Writer: private::Write + private::Output {}

/// Build a [`StreamWriter`].
pub struct StreamWriterBuilder<'a> {
    destination: Location,
    format: Option<&'a str>,
    options: Option<Options>,
}

impl<'a> StreamWriterBuilder<'a> {
    /// Create a new writer with the specified destination.
    ///
    /// # Arguments
    ///
    /// * `destination` - Destination to write to.
    pub fn new(destination: impl Into<Location>) -> Self {
        Self {
            destination: destination.into(),
            format: None,
            options: None,
        }
    }

    /// Specify a container format for the writer.
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

    /// Specify options for the backend.
    ///
    /// # Arguments
    ///
    /// * `options` - Options to pass on to output.
    pub fn with_options(mut self, options: impl Into<Option<Options>>) -> Self {
        self.options = options.into();
        self
    }

    /// Build [`StreamWriter`].
    pub fn build(self) -> Result<StreamWriter> {
        let filename = utils::from_path(&self.destination.as_path());
        let format = self.format.map(utils::from_str);
        let mut dict = self.options.map(|opts| opts.into_dict());
        let output_ctx = AVFormatContextOutput::builder()
            .filename(&filename)
            .maybe_format_name(format.as_deref())
            .options(&mut dict)
            .build()
            .context("Create output format context failed.")?;
        Ok(StreamWriter {
            destination: self.destination,
            output: output_ctx,
        })
    }
}

/// File writer for video files.
///
/// # Example
///
/// Create a video writer that produces fragmented MP4:
///
/// ```ignore
/// let mut options = HashMap::new();
/// options.insert(
///     "movflags".to_string(),
///     "frag_keyframe+empty_moov".to_string(),
/// );
///
/// let mut writer = WriterBuilder::new(Path::new("my_file.mp4"))
///     .with_options(&options.into())
///     .build()
///     .unwrap();
/// ```
pub struct StreamWriter {
    pub destination: Location,
    pub output: AVFormatContextOutput,
}

impl StreamWriter {
    /// Create a new file writer for video files.
    ///
    /// # Arguments
    ///
    /// * `dest` - Where to write to.
    #[inline]
    pub fn new(destination: impl Into<Location>) -> Result<Self> {
        StreamWriterBuilder::new(destination).build()
    }
}

impl Writer for StreamWriter {}

unsafe impl Send for StreamWriter {}
unsafe impl Sync for StreamWriter {}

/// Type alias for a byte buffer.
pub type Buf = Vec<u8>;

/// Type alias for multiple buffers.
pub type Bufs = Vec<Buf>;

/// Build a [`BufferWriter`].
pub struct BufferWriterBuilder<'a> {
    format: &'a str,
    options: Option<Options>,
}

impl<'a> BufferWriterBuilder<'a> {
    /// Create a new writer that writes to a buffer.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    pub fn new(format: &'a str) -> Self {
        Self {
            format,
            options: None,
        }
    }

    /// Specify options for the backend.
    ///
    /// # Arguments
    ///
    /// * `options` - Options to pass on to output.
    pub fn with_options(mut self, options: impl Into<Option<Options>>) -> Self {
        self.options = options.into();
        self
    }

    /// Build [`BufferWriter`].
    pub fn build(self) -> Result<BufferWriter> {
        let _dict = self.options.map(|opts| opts.into_dict());
        Ok(BufferWriter {
            output: output_raw(self.format)?,
        })
    }
}

/// Video writer that writes to a buffer.
///
/// # Example
///
/// ```ignore
/// let mut writer = BufferWriter::new("mp4").unwrap();
/// let bytes = writer.write_header()?;
/// ```
pub struct BufferWriter {
    pub(crate) output: AVFormatContextOutput,
}

impl BufferWriter {
    /// Create a video writer that writes to a buffer and returns the resulting bytes.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    #[inline]
    pub fn new(format: &str) -> Result<Self> {
        BufferWriterBuilder::new(format).build()
    }

    fn begin_write(&mut self) {
        output_raw_buf_start(&mut self.output);
    }

    fn end_write(&mut self) -> Vec<u8> {
        output_raw_buf_end(&mut self.output)
    }
}

impl Writer for BufferWriter {}

impl Drop for BufferWriter {
    fn drop(&mut self) {
        // Make sure to close the buffer properly before dropping the object or `avio_close` will
        // get confused and double free. We can simply ignore the resulting buffer.
        let _ = output_raw_buf_end(&mut self.output);
    }
}

unsafe impl Send for BufferWriter {}
unsafe impl Sync for BufferWriter {}

/// Build a [`PacketizedBufWriter`].
pub struct PacketizedBufWriterBuilder<'a> {
    format: &'a str,
    options: Option<Options>,
}

impl<'a> PacketizedBufWriterBuilder<'a> {
    /// Create a new writer that writes to a packetized buffer.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    pub fn new(format: &'a str) -> Self {
        Self {
            format,
            options: None,
        }
    }

    /// Specify options for the backend.
    ///
    /// # Arguments
    ///
    /// * `options` - Options to pass on to output.
    pub fn with_options(mut self, options: impl Into<Option<Options>>) -> Self {
        self.options = options.into();
        self
    }

    /// Build [`PacketizedBufWriter`].
    pub fn build(self) -> Result<PacketizedBufWriter> {
        let _dict = self.options.map(|opts| opts.into_dict());
        Ok(PacketizedBufWriter {
            output: output_raw(self.format)?,
            buffers: Vec::new(),
        })
    }
}

/// Video writer that writes multiple packets to a buffer and returns the resulting
/// bytes for each packet.
///
/// # Example
///
/// ```ignore
/// let mut writer = BufPacketizedWriter::new("rtp").unwrap();
/// let bytes = writer.write_header()?;
/// ```
pub struct PacketizedBufWriter {
    pub(crate) output: AVFormatContextOutput,
    buffers: Bufs,
}

impl PacketizedBufWriter {
    /// Actual packet size. Value should be below MTU.
    const PACKET_SIZE: usize = 1024;

    /// Create a video writer that writes multiple packets to a buffer and returns the resulting
    /// bytes for each packet.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    #[inline]
    pub fn new(format: &str) -> Result<Self> {
        PacketizedBufWriterBuilder::new(format).build()
    }

    fn begin_write(&mut self) {
        output_raw_packetized_buf_start(
            &mut self.output,
            // Note: `ffi::output_raw_packetized_bug_start` requires that this value lives until
            // `ffi::output_raw_packetized_buf_end`. This is guaranteed by the fact that
            // `begin_write` is always followed by an invocation of `end_write` in the same function
            // (see the implementation) of `Write` for `PacketizedBufWriter`.
            &mut self.buffers,
            Self::PACKET_SIZE,
        );
    }

    fn end_write(&mut self) {
        output_raw_packetized_buf_end(&mut self.output);
    }

    #[inline]
    fn take_buffers(&mut self) -> Bufs {
        // We take the buffers here and replace them with an empty `Vec`.
        std::mem::take(&mut self.buffers)
    }
}

impl Writer for PacketizedBufWriter {}

unsafe impl Send for PacketizedBufWriter {}
unsafe impl Sync for PacketizedBufWriter {}

pub(crate) mod private {
    use super::*;

    pub trait Write {
        type Out;

        /// Write the container header.
        fn write_header(&mut self) -> Result<Self::Out>;

        /// Write a packet into the container.
        ///
        /// # Arguments
        ///
        /// * `packet` - AVPacket to write.
        fn write_frame(&mut self, packet: &mut AVPacket) -> Result<Self::Out>;

        /// Write a packet into the container and take care of interleaving.
        ///
        /// # Arguments
        ///
        /// * `packet` - AVPacket to write.
        fn write_interleaved(&mut self, packet: &mut AVPacket) -> Result<Self::Out>;

        /// Write the container trailer.
        fn write_trailer(&mut self) -> Result<Self::Out>;
    }

    impl Write for StreamWriter {
        type Out = ();

        fn write_header(&mut self) -> Result<()> {
            self.output
                .write_header(&mut None)
                .context("Failed to write header")?;
            Ok(())
        }

        fn write_frame(&mut self, packet: &mut AVPacket) -> Result<()> {
            self.output.write_frame(packet)?;
            Ok(())
        }

        fn write_interleaved(&mut self, packet: &mut AVPacket) -> Result<()> {
            self.output.interleaved_write_frame(packet)?;
            Ok(())
        }

        fn write_trailer(&mut self) -> Result<()> {
            self.output
                .write_trailer()
                .context("Failed to write trailer")?;
            Ok(())
        }
    }

    impl Write for BufferWriter {
        type Out = Buf;

        fn write_header(&mut self) -> Result<Buf> {
            self.begin_write();
            self.output.write_header(&mut None)?;
            Ok(self.end_write())
        }

        fn write_frame(&mut self, packet: &mut AVPacket) -> Result<Buf> {
            self.begin_write();
            self.output.write_frame(packet)?;
            flush_output(&mut self.output)?;
            Ok(self.end_write())
        }

        fn write_interleaved(&mut self, packet: &mut AVPacket) -> Result<Buf> {
            self.begin_write();
            self.output.interleaved_write_frame(packet)?;
            flush_output(&mut self.output)?;
            Ok(self.end_write())
        }

        fn write_trailer(&mut self) -> Result<Buf> {
            self.begin_write();
            self.output.write_trailer()?;
            Ok(self.end_write())
        }
    }

    impl Write for PacketizedBufWriter {
        type Out = Bufs;

        fn write_header(&mut self) -> Result<Bufs> {
            self.begin_write();
            self.output.write_header(&mut None)?;
            self.end_write();
            Ok(self.take_buffers())
        }

        fn write_frame(&mut self, packet: &mut AVPacket) -> Result<Bufs> {
            self.begin_write();
            self.output.write_frame(packet)?;
            flush_output(&mut self.output)?;
            self.end_write();
            Ok(self.take_buffers())
        }

        fn write_interleaved(&mut self, packet: &mut AVPacket) -> Result<Bufs> {
            self.begin_write();
            self.output.interleaved_write_frame(packet)?;
            flush_output(&mut self.output)?;
            self.end_write();
            Ok(self.take_buffers())
        }

        fn write_trailer(&mut self) -> Result<Bufs> {
            self.begin_write();
            self.output.write_trailer()?;
            self.end_write();
            Ok(self.take_buffers())
        }
    }

    pub trait Output {
        /// Obtain reference to output context.
        fn output(&self) -> &AVFormatContextOutput;

        /// Obtain mutable reference to output context.
        fn output_mut(&mut self) -> &mut AVFormatContextOutput;

        /// new stream
        fn add_stream(&mut self, codecpar: AVCodecParameters, timebase: ffi::AVRational) -> usize {
            let mut av_stream = self.output_mut().new_stream();
            av_stream.set_codecpar(codecpar);
            av_stream.set_time_base(timebase);
            av_stream.index as usize
        }
    }

    impl Output for StreamWriter {
        fn output(&self) -> &AVFormatContextOutput {
            &self.output
        }

        fn output_mut(&mut self) -> &mut AVFormatContextOutput {
            &mut self.output
        }
    }

    impl Output for BufferWriter {
        fn output(&self) -> &AVFormatContextOutput {
            &self.output
        }

        fn output_mut(&mut self) -> &mut AVFormatContextOutput {
            &mut self.output
        }
    }

    impl Output for PacketizedBufWriter {
        fn output(&self) -> &AVFormatContextOutput {
            &self.output
        }

        fn output_mut(&mut self) -> &mut AVFormatContextOutput {
            &mut self.output
        }
    }
}

///////////////////////////////////
///////////////////////////////////

/// This function is similar to the existing bindings in ffmpeg-next like `output` and `output_as`,
/// but does not assume that it is opening a file-like context. Instead, it opens a raw output,
/// without a file attached.
///
/// Combined with the `output_raw_buf_start` and `output_raw_buf_end` functions, this can be used to
/// write to a buffer instead of a file.
///
/// # Arguments
///
/// * `format` - String to indicate the container format, like "mp4".
///
/// # Example
///
/// ```ignore
/// let output = ffi::output_raw("mp4");
///
/// output_raw_buf_start(&mut output);
/// output.write_header()?;
/// let buf output_raw_buf_end(&mut output);
/// println!("{}", buf.len());
/// ```
pub(crate) fn output_raw(format: &str) -> Result<AVFormatContextOutput> {
    unsafe {
        let mut output_ptr = std::ptr::null_mut();
        let format = std::ffi::CString::new(format)?;
        match ffi::avformat_alloc_output_context2(
            &mut output_ptr,
            std::ptr::null_mut(),
            format.as_ptr(),
            std::ptr::null(),
        ) {
            0 => Ok(AVFormatContextOutput::from_raw(
                std::ptr::NonNull::new(output_ptr).unwrap(),
            )),
            e => Err(Error::new(RsmpegError::from(e))),
        }
    }
}

/// This function initializes a dynamic buffer and inserts it into an output context to allow a
/// write to happen. Afterwards, the callee can use `output_raw_buf_end` to retrieve what was
/// written.
///
/// # Arguments
///
/// * `output` - Output context to start write on.
pub(crate) fn output_raw_buf_start(output: &mut AVFormatContextOutput) {
    unsafe {
        // Here we initialize a raw pointer (mutable) as nullptr initially. We then call the
        // `avio_open_dyn_buf` which expects a ptr ptr, and place the result in p. In case of
        // success, we override the `pb` pointer inside the output context to point to the dyn buf.
        let mut p: *mut ffi::AVIOContext = std::ptr::null_mut();
        match ffi::avio_open_dyn_buf((&mut p) as *mut *mut ffi::AVIOContext) {
            0 => {
                (*output.as_mut_ptr()).pb = p;
            }
            _ => {
                panic!("Failed to open dynamic buffer for output context.");
            }
        }
    }
}

/// This function cleans up the dynamic buffer used for the write and returns the buffer as a vector
/// of bytes.
///
/// # Arguments
///
/// * `output` - Output context to end write on.
pub(crate) fn output_raw_buf_end(output: &mut AVFormatContextOutput) -> Vec<u8> {
    unsafe {
        // First, we acquire a raw pointer to the AVIOContext in the `pb` field of the output
        // context. We stored the dyn buf there when we called `output_raw_buf_start`. Secondly, the
        // `close_dyn_buf` function will place a pointer to the starting address of the buffer in
        // `buffer_raw` through a ptr ptr. It also returns the size of that buffer.
        let output_pb = (*output.as_mut_ptr()).pb;
        let mut buffer_raw: *mut u8 = std::ptr::null_mut();
        let buffer_size = ffi::avio_close_dyn_buf(output_pb, &mut buffer_raw);

        // Reset the `pb` field or `avformat_close` will try to free it!
        (*output.as_mut_ptr()).pb = std::ptr::null_mut::<ffi::AVIOContext>();

        // Create a Rust `Vec` from the buffer (copying).
        let buffer = if buffer_size > 0 {
            std::slice::from_raw_parts(buffer_raw, buffer_size as usize).to_vec()
        } else {
            Vec::new()
        };

        // Now deallocate the original backing buffer.
        ffi::av_free(buffer_raw as *mut std::ffi::c_void);

        buffer
    }
}

/// This function initializes an IO context for the `Output` that packetizes individual writes. Each
/// write is pushed onto a packet buffer (a collection of buffers, each being a packet).
///
/// The callee must invoke `output_raw_packetized_buf_end` soon after calling this function. The
/// `Vec` pointed to by `packet_buffer` must live between invocation of this function and
/// `output_raw_packetized_buf_end`!
///
/// Not calling `output_raw_packetized_buf_end` after calling this function will result in memory
/// leaking.
///
/// # Arguments
///
/// * `output` - Output context to start write on.
/// * `packet_buffer` - Packet buffer to push buffers onto. Must live until
///   `output_raw_packetized_buf`.
/// * `max_packet_size` - Maximum size per packet.
pub fn output_raw_packetized_buf_start(
    output: &mut AVFormatContextOutput,
    packet_buffer: &mut Vec<Vec<u8>>,
    max_packet_size: usize,
) {
    unsafe {
        let buffer = ffi::av_malloc(max_packet_size) as *mut u8;

        // Create a custom IO context around our buffer.
        let io: *mut ffi::AVIOContext = ffi::avio_alloc_context(
            buffer,
            max_packet_size as std::os::raw::c_int,
            // Set stream to WRITE.
            1,
            // Pass on a pointer *UNSAFE* to the packet buffer, assuming the packet buffer will live
            // long enough.
            packet_buffer as *mut Vec<Vec<u8>> as *mut std::ffi::c_void,
            // No `read_packet`.
            None,
            // Passthrough for `write_packet`.
            // XXX: Doing a manual transmute here to match the expected callback function
            // signature. Since it changed since ffmpeg 7 and we don't know during compile time
            // what version we're dealing with, this trick will convert to the either the signature
            // where the buffer argument is `*const u8` or `*mut u8`.
            #[allow(clippy::missing_transmute_annotations)]
            Some(std::mem::transmute::<*const (), _>(
                output_raw_buf_start_callback as _,
            )),
            // No `seek`.
            None,
        );

        // Setting `max_packet_size` will let the underlying IO stream know that this buffer must be
        // treated as packetized.
        (*io).max_packet_size = max_packet_size.try_into().unwrap();

        // Assign IO to output context.
        (*output.as_mut_ptr()).pb = io;
    }
}

/// This function cleans up the IO context used for packetized writing created by
/// `output_raw_packetized_buf_start`.
///
/// # Arguments
///
/// * `output` - Output context to end write on.
pub fn output_raw_packetized_buf_end(output: &mut AVFormatContextOutput) {
    unsafe {
        let output_pb = (*output.as_mut_ptr()).pb;

        // One last flush (might incur write, most likely won't).
        ffi::avio_flush(output_pb);

        // Note: No need for handling `opaque` as it is managed by Rust code anyway and will be
        // freed by it.

        // We do need to free the buffer itself though (we allocatd it manually earlier).
        ffi::av_free((*output_pb).buffer as *mut std::ffi::c_void);
        // And deallocate the entire IO context.
        ffi::av_free(output_pb as *mut std::ffi::c_void);

        // Reset the `pb` field or `avformat_close` will try to free it!
        (*output.as_mut_ptr()).pb = std::ptr::null_mut::<ffi::AVIOContext>();
    }
}

/// Passthrough function that is passed to `libavformat` in `avio_alloc_context` and pushes buffers
/// from a packetized stream onto the packet buffer held in `opaque`.
extern "C" fn output_raw_buf_start_callback(
    opaque: *mut std::ffi::c_void,
    buffer: *const u8,
    buffer_size: i32,
) -> i32 {
    unsafe {
        // Acquire a reference to the packet buffer transmuted from the `opaque` gotten through
        // `libavformat`.
        let packet_buffer: &mut Vec<Vec<u8>> = &mut *(opaque as *mut Vec<Vec<u8>>);
        // Push the current packet onto the packet buffer.
        packet_buffer.push(std::slice::from_raw_parts(buffer, buffer_size as usize).to_vec());
    }

    // Number of bytes written.
    buffer_size
}

/// Flush the output. This can be useful in some circumstances.options
///
/// For example: It is used to flush fragments when outputting fragmented mp4 packets in combination
/// with the `frag_custom` option.
///
/// # Arguments
///
/// * `output` - Output context to flush.
pub(crate) fn flush_output(output: &mut AVFormatContextOutput) -> Result<()> {
    unsafe {
        match ffi::av_write_frame(output.as_mut_ptr(), std::ptr::null_mut()) {
            0 | 1 => Ok(()),
            e => Err(Error::new(RsmpegError::from(e))),
        }
    }
}

/// Initialize the logging handler. This will redirect all ffmpeg logging to the Rust `tracing`
/// crate and any subscribers to it.
pub fn init_logging() {
    unsafe {
        ffi::av_log_set_callback(Some(log_callback));
    }
}

/// Internal function with C-style callback behavior that receives all log messages from ffmpeg and
/// handles them with the `log` crate, the Rust way.
///
/// # Arguments
///
/// * `avcl` - Internal struct with log message data.
/// * `level_no` - Log message level integer.
/// * `fmt` - Log message format string.
/// * `vl` - Variable list with format string items.
unsafe extern "C" fn log_callback(
    avcl: *mut std::ffi::c_void,
    level_no: std::ffi::c_int,
    fmt: *const std::ffi::c_char,
    #[cfg(all(target_arch = "x86_64", target_family = "unix"))] vl: *mut ffi::__va_list_tag,
    #[cfg(not(all(target_arch = "x86_64", target_family = "unix")))] vl: ffi::va_list,
) {
    // Check whether or not the message would be printed at all.
    let event_would_log = match level_no as u32 {
        // These are all error states.
        ffi::AV_LOG_PANIC | ffi::AV_LOG_FATAL | ffi::AV_LOG_ERROR => {
            tracing::enabled!(tracing::Level::ERROR)
        }
        ffi::AV_LOG_WARNING => tracing::enabled!(tracing::Level::WARN),
        ffi::AV_LOG_INFO => tracing::enabled!(tracing::Level::INFO),
        // There is no "verbose" in `log`, so we just put it in the "debug" category.
        ffi::AV_LOG_VERBOSE | ffi::AV_LOG_DEBUG => tracing::enabled!(tracing::Level::DEBUG),
        ffi::AV_LOG_TRACE => tracing::enabled!(tracing::Level::TRACE),
        _ => {
            return;
        }
    };

    if event_would_log {
        // Allocate some memory for the log line (might be truncated). 1024 bytes is the number used
        // by ffmpeg itself, so it should be mostly fine.
        let mut line = [0; 1024];
        let mut print_prefix: std::ffi::c_int = 1;
        // Use the ffmpeg default formatting.
        let ret = ffi::av_log_format_line2(
            avcl,
            level_no,
            fmt,
            vl,
            line.as_mut_ptr(),
            (line.len()) as std::ffi::c_int,
            (&mut print_prefix) as *mut std::ffi::c_int,
        );
        // Simply discard the log message if formatting fails.
        if ret > 0 {
            if let Ok(line) = std::ffi::CStr::from_ptr(line.as_mut_ptr()).to_str() {
                let line = line.trim();
                if log_filter_hacks(line) {
                    match level_no as u32 {
                        // These are all error states.
                        ffi::AV_LOG_PANIC | ffi::AV_LOG_FATAL | ffi::AV_LOG_ERROR => {
                            tracing::error!(target: "rsmedia", "{}", line)
                        }
                        ffi::AV_LOG_WARNING => tracing::warn!(target: "rsmedia", "{}", line),
                        ffi::AV_LOG_INFO => tracing::info!(target: "rsmedia", "{}", line),
                        // There is no "verbose" in `log`, so we just put it in the "debug" category.
                        ffi::AV_LOG_VERBOSE | ffi::AV_LOG_DEBUG => {
                            tracing::debug!(target: "rsmedia", "{}", line)
                        }
                        ffi::AV_LOG_TRACE => tracing::trace!(target: "rsmedia", "{}", line),
                        _ => {}
                    };
                }
            }
        }
    }
}

/// Helper function to filter out any lines that we don't want to log because they contaminate.
/// Currently, it includes the following log line hacks:
///
/// * **Pelco H264 encoding issue**. Pelco cameras and encoders have a problem with their SEI NALs
///   that causes ffmpeg to complain but does not hurt the stream. It does cause continuous error
///   messages though which we filter out here.
fn log_filter_hacks(line: &str) -> bool {
    /* Hack 1 */
    const HACK_1_PELCO_NEEDLE_1: &str = "SEI type 5 size";
    const HACK_1_PELCO_NEEDLE_2: &str = "truncated at";
    if line.contains(HACK_1_PELCO_NEEDLE_1) && line.contains(HACK_1_PELCO_NEEDLE_2) {
        return false;
    }

    true
}

/// Create SDP file contents for the given output. Useful for RTP muxers.
///
/// A media entry will be created for each stream in the output. This function will take care of all
/// details, such as setting the correct media attributes needed by any SDP consumers.
///
/// # Arguments
///
/// * `output` - Output to generate SDP file for.
///
/// # Return value
///
/// A string with the SDP file contents.
pub fn sdp(output_fmt_ctx: &AVFormatContextOutput) -> Result<String> {
    const BUF_SIZE: i32 = 4096;
    unsafe {
        let mut buf: [std::ffi::c_char; BUF_SIZE as usize] = [0; BUF_SIZE as usize];
        let buf_ptr = &mut buf as *mut std::ffi::c_char;
        let mut output_fmt_ctx_ptr = output_fmt_ctx.as_ptr();
        let output_fmt_ctx_ptr = &mut output_fmt_ctx_ptr as *mut *const ffi::AVFormatContext;
        // WARNING! Casting from const ptr to mutable ptr here!
        let output_fmt_ctx_ptr = output_fmt_ctx_ptr as *mut *mut ffi::AVFormatContext;
        let ret = ffi::av_sdp_create(output_fmt_ctx_ptr, 1, buf_ptr, BUF_SIZE);
        if ret == 0 {
            Ok(utils::from_c_char(buf_ptr))
        } else {
            Err(Error::new(RsmpegError::from(ret)))
        }
    }
}

/// Whether or not the output format context is configured to use H.264 packetization mode 0.
///
/// # Arguments
///
/// * `output` - Output format context.
pub fn rtp_h264_mode_0(output: &AVFormatContextOutput) -> bool {
    unsafe {
        ffi::av_opt_flag_is_set(
            output.deref().priv_data,
            "rtpflags".as_ptr() as *const std::ffi::c_char,
            "h264_mode0".as_ptr() as *const std::ffi::c_char,
        ) != 0
    }
}

/// Get the current sequence number and timestamp of the RTP muxer.
///
/// Note: This method is only safe to use on RTP output formats.
pub fn rtp_seq_and_timestamp(output: &AVFormatContextOutput) -> (u16, u32) {
    unsafe {
        let rtp_mux_context = &*(output.deref().priv_data as *const RTPMuxContext);
        (rtp_mux_context.seq, rtp_mux_context.timestamp)
    }
}

/// Rust version of the `RTPMuxContext` struct in `libavformat`.
#[repr(C)]
struct RTPMuxContext {
    _av_class: *const ffi::AVClass,
    _ic: *mut ffi::AVFormatContext,
    _st: *mut ffi::AVStream,
    pub payload_type: std::ffi::c_int,
    pub ssrc: u32,
    pub cname: *const std::ffi::c_char,
    pub seq: u16,
    pub timestamp: u32,
    pub base_timestamp: u32,
    pub cur_timestamp: u32,
    pub max_payload_size: std::ffi::c_int,
}
