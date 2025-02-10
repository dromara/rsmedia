use rsmpeg::avformat::AVFormatContextInput as AvInput;
use rsmpeg::avformat::AVFormatContextOutput as AvOutput;
use rsmpeg::error::RsmpegError;

use crate::error::Error;
use crate::{ffi, Packet};
use crate::location::Location;
use crate::options::{Dictionary, Options};
use crate::packet::Packet as AvPacket;
use crate::packet::PacketIter;
use crate::stream::{Stream, StreamInfo};

use libc::c_int;
use std::ffi::{CStr, CString};
use std::fmt::Display;
use std::path::Path;
use std::ptr;
use std::ops::Bound;

type Result<T> = std::result::Result<T, Error>;

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

    // XXX: use to_cstring when stable
    fn from_path<P: AsRef<Path> + ?Sized>(path: &P) -> CString {
        CString::new(path.as_ref().as_os_str().to_str().unwrap()).unwrap()
    }

    pub fn input<P: AsRef<Path> + ?Sized>(path: &P) -> Result<AvInput> {
        unsafe {
            let mut ps = ptr::null_mut();
            let path = Self::from_path(path);

            match rsmpeg::ffi::avformat_open_input(
                &mut ps,
                path.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
            ) {
                0 => match rsmpeg::ffi::avformat_find_stream_info(ps, ptr::null_mut()) {
                    r if r >= 0 => Ok(AvInput::from_raw(ptr::NonNull::new(ps).unwrap())),
                    e => {
                        rsmpeg::ffi::avformat_close_input(&mut ps);
                        Err(Error::BackendError(RsmpegError::from(e)))
                    }
                },

                e => Err(Error::from(RsmpegError::from(e))),
            }
        }
    }

    pub fn input_with_dictionary<P: AsRef<Path> + ?Sized>(
        path: &P,
        options: Dictionary,
    ) -> Result<AvInput> {
        unsafe {
            let mut ps = ptr::null_mut();
            let path = Self::from_path(path);
            let opts = options.disown();
            let res = rsmpeg::ffi::avformat_open_input(
                &mut ps,
                path.as_ptr(),
                ptr::null_mut(),
                opts as *mut _,
            );

            Dictionary::own(opts);

            match res {
                0 => match rsmpeg::ffi::avformat_find_stream_info(ps, ptr::null_mut()) {
                    r if r >= 0 => Ok(AvInput::from_raw(ptr::NonNull::new(ps).unwrap())),
                    e => {
                        rsmpeg::ffi::avformat_close_input(&mut ps);
                        Err(Error::from(RsmpegError::from(e)))
                    }
                },

                e => Err(Error::from(RsmpegError::from(e))),
            }
        }
    }
}

/// Video reader that can read from files.
pub struct Reader {
    pub source: Location,
    pub input: AvInput,
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
    pub fn read(&mut self, stream_index: usize) -> Result<AvPacket> {
        let mut error_count = 0;
        loop {
            match self.packets().next() {
                Some((stream, packet)) => {
                    if stream.index() == stream_index {
                        return Ok(Packet::new(packet, stream.time_base()));
                    }
                }
                None => {
                    error_count += 1;
                    if error_count > 3 {
                        return Err(Error::ReadExhausted);
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
        const CONVERSION_FACTOR: i64 = (rsmpeg::ffi::AV_TIME_BASE_Q.den / 1000) as i64;
        // One second left and right leeway when seeking.
        const LEEWAY: i64 = rsmpeg::ffi::AV_TIME_BASE_Q.den as i64;

        let timestamp = CONVERSION_FACTOR * timestamp_milliseconds;
        let range = timestamp - LEEWAY..timestamp + LEEWAY;

        self._seek(timestamp, range).map_err(|_| Error::BackendError(RsmpegError::Unknown))
    }

    /// Seek to a specific frame in the video stream.
    ///
    /// # Arguments
    ///
    /// * `frame_number` - The frame number to seek to.
    pub fn seek_to_frame(&mut self, frame_number: i64) -> Result<()> {
        unsafe {
            match rsmpeg::ffi::av_seek_frame(self.input.as_mut_ptr(), -1, frame_number, 0) {
                0 => Ok(()),
                e => Err(Error::BackendError(RsmpegError::from(e))),
            }
        }
    }

    /// Seek to start of reader. This function performs best effort seeking to the start of the
    /// file.
    pub fn seek_to_start(&mut self) -> Result<()> {
        self._seek(i64::MIN, ..).map_err(|_| Error::BackendError(RsmpegError::Unknown))
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
            match rsmpeg::ffi::avformat_seek_file(self.input.as_mut_ptr(), -1, start, ts, end, 0) {
                s if s >= 0 => Ok(()),
                e => Err(Error::from(RsmpegError::from(e))),
            }
        }
    }

    /// Find the best video stream and return the index.
    pub fn best_video_stream_index(&self) -> Result<usize> {
        Ok(self
            .input
            .find_best_stream(rsmpeg::ffi::AVMEDIA_TYPE_VIDEO)?
            .ok_or(RsmpegError::FindStreamInfoError(
                rsmpeg::ffi::AVERROR_STREAM_NOT_FOUND,
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

    // XXX: use to_cstring when stable
    fn from_path<P: AsRef<Path> + ?Sized>(path: &P) -> CString {
        CString::new(path.as_ref().as_os_str().to_str().unwrap()).unwrap()
    }

    pub fn output<P: AsRef<Path> + ?Sized>(path: &P) -> Result<AvOutput> {
        Ok(AvOutput::create(&Self::from_path(path), None)?)
    }

    pub fn output_with<P: AsRef<Path> + ?Sized>(path: &P, options: Dictionary) -> Result<AvOutput> {
        unsafe {
            let mut ps = ptr::null_mut();
            let path = Self::from_path(path);
            let opts = options.disown();

            match rsmpeg::ffi::avformat_alloc_output_context2(
                &mut ps,
                ptr::null_mut(),
                ptr::null(),
                path.as_ptr(),
            ) {
                0 => {
                    let res = rsmpeg::ffi::avio_open2(
                        &mut (*ps).pb,
                        path.as_ptr(),
                        rsmpeg::ffi::AVIO_FLAG_WRITE as c_int,
                        ptr::null(),
                        opts as *mut _,
                    );

                    Dictionary::own(opts);

                    match res {
                        0 => Ok(AvOutput::from_raw(ptr::NonNull::new(ps).unwrap())),
                        e => Err(Error::from(RsmpegError::from(e))),
                    }
                }

                e => Err(Error::from(RsmpegError::from(e))),
            }
        }
    }

    pub fn output_as<P: AsRef<Path> + ?Sized>(path: &P, format: &str) -> Result<AvOutput> {
        unsafe {
            let mut ps = ptr::null_mut();
            let path = Self::from_path(path);
            let format = CString::new(format).unwrap();

            match rsmpeg::ffi::avformat_alloc_output_context2(
                &mut ps,
                ptr::null_mut(),
                format.as_ptr(),
                path.as_ptr(),
            ) {
                0 => match rsmpeg::ffi::avio_open(
                    &mut (*ps).pb,
                    path.as_ptr(),
                    rsmpeg::ffi::AVIO_FLAG_WRITE as c_int,
                ) {
                    0 => Ok(AvOutput::from_raw(ptr::NonNull::new(ps).unwrap())),
                    e => Err(Error::from(RsmpegError::from(e))),
                },

                e => Err(Error::from(RsmpegError::from(e))),
            }
        }
    }

    pub fn output_as_with<P: AsRef<Path> + ?Sized>(
        path: &P,
        format: &str,
        options: Dictionary,
    ) -> Result<AvOutput> {
        unsafe {
            let mut ps = ptr::null_mut();
            let path = Self::from_path(path);
            let format = CString::new(format).unwrap();
            let opts = options.disown();

            match rsmpeg::ffi::avformat_alloc_output_context2(
                &mut ps,
                ptr::null_mut(),
                format.as_ptr(),
                path.as_ptr(),
            ) {
                0 => {
                    let res = rsmpeg::ffi::avio_open2(
                        &mut (*ps).pb,
                        path.as_ptr(),
                        rsmpeg::ffi::AVIO_FLAG_WRITE as c_int,
                        ptr::null(),
                        opts as *mut _,
                    );

                    Dictionary::own(opts);

                    match res {
                        0 => Ok(AvOutput::from_raw(ptr::NonNull::new(ps).unwrap())),
                        e => Err(Error::from(RsmpegError::from(e))),
                    }
                }

                e => Err(Error::from(RsmpegError::from(e))),
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
    pub(crate) output: AvOutput,
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
            output: ffi::output_raw(self.format)?,
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
    pub(crate) output: AvOutput,
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
        ffi::output_raw_buf_start(&mut self.output);
    }

    fn end_write(&mut self) -> Buf {
        ffi::output_raw_buf_end(&mut self.output)
    }
}

impl Write for BufWriter {}

impl Drop for BufWriter {
    fn drop(&mut self) {
        // Make sure to close the buffer properly before dropping the object or `avio_close` will
        // get confused and double free. We can simply ignore the resulting buffer.
        let _ = ffi::output_raw_buf_end(&mut self.output);
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
            output: ffi::output_raw(self.format)?,
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
    pub(crate) output: AvOutput,
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
        ffi::output_raw_packetized_buf_start(
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
        ffi::output_raw_packetized_buf_end(&mut self.output);
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
    use rsmpeg::avformat::AVOutputFormat;
    use super::*;

    type Result<T> = std::result::Result<T, Error>;

    pub trait Write {
        type Out;

        /// Write the container header.
        fn write_header(&mut self) -> Result<Self::Out>;

        /// Write a packet into the container.
        ///
        /// # Arguments
        ///
        /// * `packet` - Packet to write.
        fn write(&mut self, packet: &mut AvPacket) -> Result<Self::Out>;

        /// Write a packet into the container and take care of interleaving.
        ///
        /// # Arguments
        ///
        /// * `packet` - Packet to write.
        fn write_interleaved(&mut self, packet: &mut AvPacket) -> Result<Self::Out>;

        /// Write the container trailer.
        fn write_trailer(&mut self) -> Result<Self::Out>;
    }

    impl Write for Writer {
        type Out = ();

        fn write_header(&mut self) -> Result<()> {
            self.output.write_header(&mut None).unwrap();
            Ok(())
        }

        fn write(&mut self, packet: &mut AvPacket) -> Result<()> {
            packet.write(&mut self.output)?;
            Ok(())
        }

        fn write_interleaved(&mut self, packet: &mut AvPacket) -> Result<()> {
            packet.write_interleaved(&mut self.output)?;
            Ok(())
        }

        fn write_trailer(&mut self) -> Result<()> {
            Ok(self.output.write_trailer().unwrap())
        }
    }

    impl Write for BufWriter {
        type Out = Buf;

        fn write_header(&mut self) -> Result<Buf> {
            self.begin_write();
            self.output.write_header(&mut None)?;
            Ok(self.end_write())
        }

        fn write(&mut self, packet: &mut AvPacket) -> Result<Buf> {
            self.begin_write();
            packet.write(&mut self.output)?;
            ffi::flush_output(&mut self.output)?;
            Ok(self.end_write())
        }

        fn write_interleaved(&mut self, packet: &mut AvPacket) -> Result<Buf> {
            self.begin_write();
            packet.write_interleaved(&mut self.output)?;
            ffi::flush_output(&mut self.output)?;
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

        fn write(&mut self, packet: &mut AvPacket) -> Result<Bufs> {
            self.begin_write();
            packet.write(&mut self.output)?;
            ffi::flush_output(&mut self.output)?;
            self.end_write();
            Ok(self.take_buffers())
        }

        fn write_interleaved(&mut self, packet: &mut AvPacket) -> Result<Bufs> {
            self.begin_write();
            packet.write_interleaved(&mut self.output)?;
            ffi::flush_output(&mut self.output)?;
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
        fn output(&self) -> &AvOutput;

        /// Obtain mutable reference to output context.
        fn output_mut(&mut self) -> &mut AvOutput;
    }

    impl Output for Writer {
        fn output(&self) -> &AvOutput {
            &self.output
        }

        fn output_mut(&mut self) -> &mut AvOutput {
            &mut self.output
        }
    }

    impl Output for BufWriter {
        fn output(&self) -> &AvOutput {
            &self.output
        }

        fn output_mut(&mut self) -> &mut AvOutput {
            &mut self.output
        }
    }

    impl Output for PacketizedBufWriter {
        fn output(&self) -> &AvOutput {
            &self.output
        }

        fn output_mut(&mut self) -> &mut AvOutput {
            &mut self.output
        }
    }
}
