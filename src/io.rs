use crate::error::Error as MediaError;
use crate::location::Location;
use crate::options::{Dictionary, Options};
use crate::packet::Packet as AvPacket;
use crate::packet::PacketIter;
use crate::stream::{Stream, StreamInfo};
use anyhow::{Context, Error, Result};
use rsmpeg::avformat::AVFormatContextInput as AvInput;
use rsmpeg::avformat::AVFormatContextOutput as AvOutput;
use rsmpeg::error::RsmpegError;

use crate::Packet;
use libc::c_int;
use std::ffi::{CStr, CString};
use std::fmt::Display;
use std::ops::Bound;
use std::path::Path;
use std::ptr;

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
                        Err(Error::new(MediaError::BackendError(RsmpegError::from(e))))
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
                        return Ok(AvPacket::new(packet, stream.time_base()));
                    }
                }
                None => {
                    error_count += 1;
                    if error_count > 3 {
                        return Err(Error::new(MediaError::ReadExhausted));
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

        self._seek(timestamp, range)
            .context("Failed to seek in reader")
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
                e => Err(Error::new(MediaError::BackendError(RsmpegError::from(e)))),
            }
        }
    }

    /// Seek to start of reader. This function performs best effort seeking to the start of the
    /// file.
    pub fn seek_to_start(&mut self) -> Result<()> {
        self._seek(i64::MIN, ..)
            .context("Failed to seek to start of reader")
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

/// Video writer that can write to files.
pub struct Writer {
    pub destination: Location,
    pub output: AvOutput,
}

impl Writer {
    pub(crate) fn write_header(&self) -> Result<()> {
        todo!()
    }

    pub(crate) fn write_trailer(&self) -> Result<()> {
        todo!()
    }

    pub(crate) fn write_interleaved(&self, p0: &mut Packet) -> Result<()> {
        todo!()
    }

    pub(crate) fn write_frame(&self, p0: &mut Packet) -> Result<()> {
        todo!()
    }
}
