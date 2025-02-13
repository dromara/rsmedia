use crate::error::MediaError;
use crate::location::Location;
use crate::options::{Dictionary, Options};
use crate::packet::PacketIter;
use crate::stream::StreamInfo;
use crate::Packet;

use rsmpeg::avformat::AVFormatContextInput;
use rsmpeg::avformat::AVFormatContextOutput;
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi;

use libc::c_int;
use std::ffi::CString;
use std::ops::Bound;
use std::path::Path;
use std::ptr;

type Result<T> = std::result::Result<T, MediaError>;

/// use to_cstring when stable
pub(crate) fn from_path<P: AsRef<Path> + ?Sized>(path: &P) -> CString {
    CString::new(path.as_ref().as_os_str().to_str().unwrap()).unwrap()
}

/// Builds a [`Reader`].
///
/// # Example
///
/// ```ignore
/// let mut options = HashMap::new();
/// options.insert(
///     "rtsp_transport".to_string(),
///     "tcp".to_string(),
/// );
///
/// let mut reader = ReaderBuilder::new(Path::new("my_file.mp4"))
/// .with_options(&options.into())
/// .unwrap();
/// ```
pub struct ReaderBuilder<'a> {
    source: Location,
    options: Option<&'a Options>,
}

impl<'a> ReaderBuilder<'a> {
    /// Create a new reader with the specified locator.
    ///
    /// # Arguments
    ///
    /// * `source` - Source to read.
    pub fn new(source: impl Into<Location>) -> Self {
        Self {
            source: source.into(),
            options: None,
        }
    }

    /// Specify options for the backend.
    ///
    /// # Arguments
    ///
    /// * `options` - Options to pass on to input.
    pub fn with_options(mut self, options: &'a Options) -> Self {
        self.options = Some(options);
        self
    }

    /// Build [`Reader`].
    pub fn build(self) -> Result<Reader> {
        match self.options {
            None => Ok(Reader {
                input: Self::input(&self.source.as_path())?,
                source: self.source,
            }),
            Some(options) => Ok(Reader {
                input: Self::input_with_dictionary(&self.source.as_path(), options.to_dict())?,
                source: self.source,
            }),
        }
    }

    pub fn input<P: AsRef<Path> + ?Sized>(path: &P) -> Result<AVFormatContextInput> {
        unsafe {
            let mut ps = ptr::null_mut();
            let path = from_path(path);

            match ffi::avformat_open_input(&mut ps, path.as_ptr(), ptr::null_mut(), ptr::null_mut())
            {
                0 => match ffi::avformat_find_stream_info(ps, ptr::null_mut()) {
                    r if r >= 0 => Ok(AVFormatContextInput::from_raw(
                        ptr::NonNull::new(ps).unwrap(),
                    )),
                    e => {
                        ffi::avformat_close_input(&mut ps);
                        Err(MediaError::BackendError(RsmpegError::from(e)))
                    }
                },

                e => Err(MediaError::from(RsmpegError::from(e))),
            }
        }
    }

    pub fn input_with_dictionary<P: AsRef<Path> + ?Sized>(
        path: &P,
        options: Dictionary,
    ) -> Result<AVFormatContextInput> {
        unsafe {
            let mut ps = ptr::null_mut();
            let path = from_path(path);
            let opts = options.disown();
            let res =
                ffi::avformat_open_input(&mut ps, path.as_ptr(), ptr::null_mut(), opts as *mut _);

            Dictionary::own(opts);

            match res {
                0 => match ffi::avformat_find_stream_info(ps, ptr::null_mut()) {
                    r if r >= 0 => Ok(AVFormatContextInput::from_raw(
                        ptr::NonNull::new(ps).unwrap(),
                    )),
                    e => {
                        ffi::avformat_close_input(&mut ps);
                        Err(MediaError::from(RsmpegError::from(e)))
                    }
                },

                e => Err(MediaError::from(RsmpegError::from(e))),
            }
        }
    }
}

/// Video reader that can read from files.
pub struct Reader {
    pub source: Location,
    pub input: AVFormatContextInput,
}

impl Reader {
    /// Create a new video file reader on a given source (path, URL, etc.).
    ///
    /// # Arguments
    ///
    /// * `source` - Source to read from.
    #[inline]
    pub fn new(source: impl Into<Location>) -> Result<Self> {
        ReaderBuilder::new(source).build()
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
    /// let mut reader = Reader::new(Path::new("my_video.mp4")).unwrap();
    /// let stream = reader.best_video_stream_index().unwrap();
    /// let mut packet = reader.read(stream).unwrap();
    /// ```
    pub fn read(&mut self, stream_index: usize) -> Result<Packet> {
        let mut error_count = 0;
        loop {
            match self.packets().next().unwrap() {
                Ok((stream, packet)) => {
                    if stream.index() == stream_index {
                        return Ok(Packet::new(packet, stream.time_base()));
                    }
                }
                Err(_) => {
                    error_count += 1;
                    if error_count > 3 {
                        return Err(MediaError::ReadExhausted);
                    }
                }
            }
        }
    }

    pub fn packets(&mut self) -> PacketIter {
        PacketIter::new(&mut self.input)
    }

    /// Retrieve stream information for a stream. Stream information can be used to set up a
    /// corresponding stream for transmuxing or transcoding.
    ///
    /// # Arguments
    ///
    /// * `stream_index` - Index of stream to produce information for.
    pub fn stream_info(&self, stream_index: usize) -> Result<StreamInfo> {
        StreamInfo::from_reader(self, stream_index)
    }

    /// Seek in reader. This will change the reader head so that it points to a location within one
    /// second of the target timestamp or it will return an error.
    ///
    /// # Arguments
    ///
    /// * `timestamp_milliseconds` - Number of millisecond from start of video to seek to.
    pub fn seek(&mut self, timestamp_milliseconds: i64) -> Result<()> {
        // Conversion factor from timestamp in milliseconds to `TIME_BASE` units.
        const CONVERSION_FACTOR: i64 = (ffi::AV_TIME_BASE_Q.den / 1000) as i64;
        // One second left and right leeway when seeking.
        const LEEWAY: i64 = ffi::AV_TIME_BASE_Q.den as i64;

        let timestamp = CONVERSION_FACTOR * timestamp_milliseconds;
        let range = timestamp - LEEWAY..timestamp + LEEWAY;

        self._seek(timestamp, range)
            .expect("Failed to seek in reader");
        Ok(())
    }

    /// Seek to a specific frame in the video stream.
    ///
    /// # Arguments
    ///
    /// * `frame_number` - The frame number to seek to.
    pub fn seek_to_frame(&mut self, frame_number: i64) -> Result<()> {
        unsafe {
            match ffi::av_seek_frame(self.input.as_mut_ptr(), -1, frame_number, 0) {
                0 => Ok(()),
                e => Err(MediaError::BackendError(RsmpegError::from(e))),
            }
        }
    }

    /// Seek to start of reader. This function performs best effort seeking to the start of the
    /// file.
    pub fn seek_to_start(&mut self) -> Result<()> {
        self._seek(i64::MIN, ..)
            .expect("Failed to seek to start of reader");
        Ok(())
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
            match ffi::avformat_seek_file(self.input.as_mut_ptr(), -1, start, ts, end, 0) {
                s if s >= 0 => Ok(()),
                e => Err(MediaError::from(RsmpegError::from(e))),
            }
        }
    }

    /// Find the best video stream and return the index.
    pub fn best_video_stream_index(&self) -> Result<usize> {
        Ok(self
            .input
            .find_best_stream(ffi::AVMEDIA_TYPE_VIDEO)?
            .ok_or(RsmpegError::FindStreamInfoError(
                ffi::AVERROR_STREAM_NOT_FOUND,
            ))?
            .0)
    }
}

unsafe impl Send for Reader {}
unsafe impl Sync for Reader {}

/// Any type that implements this can write video packets.
pub trait Write: private::Write + private::Output {}

/// Build a [`Writer`].
pub struct WriterBuilder<'a> {
    destination: Location,
    format: Option<&'a str>,
    options: Option<&'a Options>,
}

impl<'a> WriterBuilder<'a> {
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

    /// Specify a custom format for the writer.
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
    /// * `options` - Options to pass on to output.
    pub fn with_options(mut self, options: &'a Options) -> Self {
        self.options = Some(options);
        self
    }

    /// Build [`Writer`].
    pub fn build(self) -> Result<Writer> {
        match (self.format, self.options) {
            (None, None) => Ok(Writer {
                output: Self::output(&self.destination.as_path())?,
                destination: self.destination,
            }),
            (Some(format), None) => Ok(Writer {
                output: Self::output_as(&self.destination.as_path(), format)?,
                destination: self.destination,
            }),
            (None, Some(options)) => Ok(Writer {
                output: Self::output_with(&self.destination.as_path(), options.to_dict())?,
                destination: self.destination,
            }),
            (Some(format), Some(options)) => Ok(Writer {
                output: Self::output_as_with(
                    &self.destination.as_path(),
                    format,
                    options.to_dict(),
                )?,
                destination: self.destination,
            }),
        }
    }

    pub fn output<P: AsRef<Path> + ?Sized>(path: &P) -> Result<AVFormatContextOutput> {
        //Ok(AVFormatContextOutput::create(&Self::from_path(path), None)?)
        unsafe {
            let mut ps = ptr::null_mut();
            let path = from_path(path);
            match ffi::avformat_alloc_output_context2(
                &mut ps,
                ptr::null_mut(),
                ptr::null(),
                path.as_ptr(),
            ) {
                0 => match ffi::avio_open(
                    &mut (*ps).pb,
                    path.as_ptr(),
                    ffi::AVIO_FLAG_WRITE as c_int,
                ) {
                    0 => Ok(AVFormatContextOutput::from_raw(
                        std::ptr::NonNull::new(ps).unwrap(),
                    )),
                    e => Err(MediaError::BackendError(RsmpegError::from(e))),
                },
                e => Err(MediaError::BackendError(RsmpegError::from(e))),
            }
        }
    }

    pub fn output_with<P: AsRef<Path> + ?Sized>(
        path: &P,
        options: Dictionary,
    ) -> Result<AVFormatContextOutput> {
        unsafe {
            let mut ps = ptr::null_mut();
            let path = from_path(path);
            let opts = options.disown();

            match ffi::avformat_alloc_output_context2(
                &mut ps,
                ptr::null_mut(),
                ptr::null(),
                path.as_ptr(),
            ) {
                0 => {
                    let res = ffi::avio_open2(
                        &mut (*ps).pb,
                        path.as_ptr(),
                        ffi::AVIO_FLAG_WRITE as c_int,
                        ptr::null(),
                        opts as *mut _,
                    );

                    Dictionary::own(opts);

                    match res {
                        0 => Ok(AVFormatContextOutput::from_raw(
                            ptr::NonNull::new(ps).unwrap(),
                        )),
                        e => Err(MediaError::from(RsmpegError::from(e))),
                    }
                }

                e => Err(MediaError::from(RsmpegError::from(e))),
            }
        }
    }

    pub fn output_as<P: AsRef<Path> + ?Sized>(
        path: &P,
        format: &str,
    ) -> Result<AVFormatContextOutput> {
        unsafe {
            let mut ps = ptr::null_mut();
            let path = from_path(path);
            let format = CString::new(format).unwrap();

            match ffi::avformat_alloc_output_context2(
                &mut ps,
                ptr::null_mut(),
                format.as_ptr(),
                path.as_ptr(),
            ) {
                0 => match ffi::avio_open(
                    &mut (*ps).pb,
                    path.as_ptr(),
                    ffi::AVIO_FLAG_WRITE as c_int,
                ) {
                    0 => Ok(AVFormatContextOutput::from_raw(
                        ptr::NonNull::new(ps).unwrap(),
                    )),
                    e => Err(MediaError::from(RsmpegError::from(e))),
                },

                e => Err(MediaError::from(RsmpegError::from(e))),
            }
        }
    }

    pub fn output_as_with<P: AsRef<Path> + ?Sized>(
        path: &P,
        format: &str,
        options: Dictionary,
    ) -> Result<AVFormatContextOutput> {
        unsafe {
            let mut ps = ptr::null_mut();
            let path = from_path(path);
            let format = CString::new(format).unwrap();
            let opts = options.disown();

            match ffi::avformat_alloc_output_context2(
                &mut ps,
                ptr::null_mut(),
                format.as_ptr(),
                path.as_ptr(),
            ) {
                0 => {
                    let res = ffi::avio_open2(
                        &mut (*ps).pb,
                        path.as_ptr(),
                        ffi::AVIO_FLAG_WRITE as c_int,
                        ptr::null(),
                        opts as *mut _,
                    );

                    Dictionary::own(opts);

                    match res {
                        0 => Ok(AVFormatContextOutput::from_raw(
                            ptr::NonNull::new(ps).unwrap(),
                        )),
                        e => Err(MediaError::from(RsmpegError::from(e))),
                    }
                }

                e => Err(MediaError::from(RsmpegError::from(e))),
            }
        }
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
/// .with_options(&options.into())
/// .unwrap();
/// ```
pub struct Writer {
    pub destination: Location,
    pub output: AVFormatContextOutput,
}

impl Writer {
    /// Create a new file writer for video files.
    ///
    /// # Arguments
    ///
    /// * `dest` - Where to write to.
    #[inline]
    pub fn new(destination: impl Into<Location>) -> Result<Self> {
        WriterBuilder::new(destination).build()
    }
}

impl Write for Writer {}

unsafe impl Send for Writer {}
unsafe impl Sync for Writer {}

/// Type alias for a byte buffer.
pub type Buf = Vec<u8>;

/// Type alias for multiple buffers.
pub type Bufs = Vec<Buf>;

/// Build a [`BufWriter`].
pub struct BufWriterBuilder<'a> {
    format: &'a str,
    options: Option<&'a Options>,
}

impl<'a> BufWriterBuilder<'a> {
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
    pub fn with_options(mut self, options: &'a Options) -> Self {
        self.options = Some(options);
        self
    }

    /// Build [`BufWriter`].
    pub fn build(self) -> Result<BufWriter> {
        Ok(BufWriter {
            output: output_raw(self.format)?,
            options: self.options.cloned().unwrap_or_default(),
        })
    }
}

/// Video writer that writes to a buffer.
///
/// # Example
///
/// ```ignore
/// let mut writer = BufWriter::new("mp4").unwrap();
/// let bytes = writer.write_header()?;
/// ```
pub struct BufWriter {
    pub(crate) output: AVFormatContextOutput,
    options: Options,
}

impl BufWriter {
    /// Create a video writer that writes to a buffer and returns the resulting bytes.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    #[inline]
    pub fn new(format: &str) -> Result<Self> {
        BufWriterBuilder::new(format).build()
    }

    fn begin_write(&mut self) {
        output_raw_buf_start(&mut self.output);
    }

    fn end_write(&mut self) -> Vec<u8> {
        output_raw_buf_end(&mut self.output)
    }
}

impl Write for BufWriter {}

impl Drop for BufWriter {
    fn drop(&mut self) {
        // Make sure to close the buffer properly before dropping the object or `avio_close` will
        // get confused and double free. We can simply ignore the resulting buffer.
        let _ = output_raw_buf_end(&mut self.output);
    }
}

unsafe impl Send for BufWriter {}
unsafe impl Sync for BufWriter {}

/// Build a [`PacketizedBufWriter`].
pub struct PacketizedBufWriterBuilder<'a> {
    format: &'a str,
    options: Option<&'a Options>,
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
    pub fn with_options(mut self, options: &'a Options) -> Self {
        self.options = Some(options);
        self
    }

    /// Build [`PacketizedBufWriter`].
    pub fn build(self) -> Result<PacketizedBufWriter> {
        Ok(PacketizedBufWriter {
            output: output_raw(self.format)?,
            options: self.options.cloned().unwrap_or_default(),
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
    options: Options,
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

impl Write for PacketizedBufWriter {}

unsafe impl Send for PacketizedBufWriter {}
unsafe impl Sync for PacketizedBufWriter {}

pub(crate) mod private {
    use super::*;

    type Result<T> = std::result::Result<T, RsmpegError>;

    pub trait Write {
        type Out;

        /// Write the container header.
        fn write_header(&mut self) -> Result<Self::Out>;

        /// Write a packet into the container.
        ///
        /// # Arguments
        ///
        /// * `packet` - Packet to write.
        fn write_frame(&mut self, packet: &mut Packet) -> Result<Self::Out>;

        /// Write a packet into the container and take care of interleaving.
        ///
        /// # Arguments
        ///
        /// * `packet` - Packet to write.
        fn write_interleaved(&mut self, packet: &mut Packet) -> Result<Self::Out>;

        /// Write the container trailer.
        fn write_trailer(&mut self) -> Result<Self::Out>;
    }

    impl Write for Writer {
        type Out = ();

        fn write_header(&mut self) -> Result<()> {
            self.output.write_header(&mut None)?;
            Ok(())
        }

        fn write_frame(&mut self, packet: &mut Packet) -> Result<()> {
            // packet.write(&mut self.output)?;
            self.output.write_frame(packet.as_inner())?;
            Ok(())
        }

        fn write_interleaved(&mut self, packet: &mut Packet) -> Result<()> {
            // packet.write_interleaved(&mut self.output)?;
            self.output.interleaved_write_frame(packet.as_inner())?;
            Ok(())
        }

        fn write_trailer(&mut self) -> Result<()> {
            self.output.write_trailer()?;
            Ok(())
        }
    }

    impl Write for BufWriter {
        type Out = Buf;

        fn write_header(&mut self) -> Result<Buf> {
            self.begin_write();
            self.output.write_header(&mut None)?;
            Ok(self.end_write())
        }

        fn write_frame(&mut self, packet: &mut Packet) -> Result<Buf> {
            self.begin_write();
            packet.write(&mut self.output)?;
            flush_output(&mut self.output).unwrap();
            Ok(self.end_write())
        }

        fn write_interleaved(&mut self, packet: &mut Packet) -> Result<Buf> {
            self.begin_write();
            packet.write_interleaved(&mut self.output)?;
            flush_output(&mut self.output).unwrap();
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

        fn write_frame(&mut self, packet: &mut Packet) -> Result<Bufs> {
            self.begin_write();
            packet.write(&mut self.output)?;
            flush_output(&mut self.output).unwrap();
            self.end_write();
            Ok(self.take_buffers())
        }

        fn write_interleaved(&mut self, packet: &mut Packet) -> Result<Bufs> {
            self.begin_write();
            packet.write_interleaved(&mut self.output)?;
            flush_output(&mut self.output).unwrap();
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
    }

    impl Output for Writer {
        fn output(&self) -> &AVFormatContextOutput {
            &self.output
        }

        fn output_mut(&mut self) -> &mut AVFormatContextOutput {
            &mut self.output
        }
    }

    impl Output for BufWriter {
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
        let format = std::ffi::CString::new(format).unwrap();
        match ffi::avformat_alloc_output_context2(
            &mut output_ptr,
            std::ptr::null_mut(),
            format.as_ptr(),
            std::ptr::null(),
        ) {
            0 => Ok(AVFormatContextOutput::from_raw(
                std::ptr::NonNull::new(output_ptr).unwrap(),
            )),
            e => Err(MediaError::BackendError(RsmpegError::from(e))),
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
        let buffer_size =
            ffi::avio_close_dyn_buf(output_pb, (&mut buffer_raw) as *mut *mut u8) as usize;

        // Reset the `pb` field or `avformat_close` will try to free it!
        (*output.as_mut_ptr()).pb = std::ptr::null_mut::<ffi::AVIOContext>();

        // Create a Rust `Vec` from the buffer (copying).
        let buffer = std::slice::from_raw_parts(buffer_raw, buffer_size).to_vec();

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
            max_packet_size.try_into().unwrap(),
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
            // what verion we're dealing with, this trick will convert to the either the signature
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
            0 => Ok(()),
            1 => Ok(()),
            e => Err(MediaError::BackendError(RsmpegError::from(e))),
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
                            tracing::error!(target: "video", "{}", line)
                        }
                        ffi::AV_LOG_WARNING => tracing::warn!(target: "video", "{}", line),
                        ffi::AV_LOG_INFO => tracing::info!(target: "video", "{}", line),
                        // There is no "verbose" in `log`, so we just put it in the "debug"
                        // category.
                        ffi::AV_LOG_VERBOSE | ffi::AV_LOG_DEBUG => {
                            tracing::debug!(target: "video", "{}", line)
                        }
                        ffi::AV_LOG_TRACE => tracing::trace!(target: "video", "{}", line),
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
