use ffi::{AVDiscard, AVPacketSideDataType};
use rsmpeg::avcodec::AVCodecParameters;
use rsmpeg::avformat::AVFormatContextInput;
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi;

use libc::{c_int, c_uint};
use std::marker::PhantomData;
use std::ops::Deref;

use crate::error::MediaError;
use crate::flags::AvDispositionFlags;
use crate::io::Reader;
use crate::options::{Dictionary, DictionaryRef};
use crate::packet::Packet;
use crate::Rational;

type Result<T> = std::result::Result<T, MediaError>;

/// Holds transferable stream information. This can be used to duplicate stream settings for the
/// purpose of transmuxing or transcoding.
#[derive(Clone)]
pub struct StreamInfo {
    pub index: usize,
    codec_parameters: AVCodecParameters,
    time_base: Rational,
}

impl StreamInfo {
    /// Fetch stream information from a reader by stream index.
    ///
    /// # Arguments
    ///
    /// * `reader` - Reader to find stream information from.
    /// * `stream_index` - Index of stream in reader.
    pub(crate) fn from_reader(reader: &Reader, stream_index: usize) -> Result<Self> {
        let stream =
            reader
                .input
                .streams()
                .get(stream_index)
                .ok_or(RsmpegError::FindStreamInfoError(
                    ffi::AVERROR_STREAM_NOT_FOUND,
                ))?;

        Self::from_params(
            stream.codecpar().to_owned(),
            stream.time_base.into(),
            stream_index,
        )
    }

    pub fn from_params(
        codecpar: AVCodecParameters,
        timebase: Rational,
        stream_index: usize,
    ) -> Result<Self> {
        Ok(Self {
            index: stream_index,
            codec_parameters: codecpar,
            time_base: timebase,
        })
    }

    /// Turn information back into parts for usage.
    ///
    /// Note: Consumes stream information object.
    ///
    /// # Return value
    ///
    /// A tuple consisting of:
    /// * The stream index.
    /// * Codec parameters.
    /// * Original stream time base.
    pub(crate) fn into_parts(self) -> (usize, AVCodecParameters, Rational) {
        (self.index, self.codec_parameters, self.time_base)
    }
}

unsafe impl Send for StreamInfo {}
unsafe impl Sync for StreamInfo {}

/////////////////////////////////////////
pub struct SideData<'a> {
    ptr: *mut ffi::AVPacketSideData,
    _marker: PhantomData<&'a Packet>,
}

impl<'a> SideData<'a> {
    /// # Safety
    pub unsafe fn wrap(ptr: *mut ffi::AVPacketSideData) -> Self {
        SideData {
            ptr,
            _marker: PhantomData,
        }
    }

    /// # Safety
    pub unsafe fn as_ptr(&self) -> *const ffi::AVPacketSideData {
        self.ptr as *const _
    }
}

impl<'a> SideData<'a> {
    pub fn kind(&self) -> AVPacketSideDataType {
        unsafe { AVPacketSideDataType::from((*self.as_ptr()).type_) }
    }

    pub fn data(&self) -> &[u8] {
        #[allow(clippy::unnecessary_cast)]
        unsafe {
            std::slice::from_raw_parts((*self.as_ptr()).data, (*self.as_ptr()).size as usize)
        }
    }
}

////////////////////////////////////////
// #[derive(Debug)]
pub struct Stream<'a> {
    context: &'a AVFormatContextInput,
    index: usize,
}

impl<'a> Stream<'a> {
    pub fn wrap(context: &AVFormatContextInput, index: usize) -> Stream {
        Stream { context, index }
    }

    pub fn as_ptr(&self) -> *const ffi::AVStream {
        unsafe { *(*self.context.as_ptr()).streams.add(self.index) }
    }
}

impl<'a> Stream<'a> {
    pub fn id(&self) -> i32 {
        unsafe { (*self.as_ptr()).id }
    }

    // pub fn parameters(&self) -> codec::Parameters {
    //     unsafe {
    //         codec::Parameters::wrap((*self.as_ptr()).codecpar, Some(self.context.destructor()))
    //     }
    // }

    pub fn index(&self) -> usize {
        unsafe { (*self.as_ptr()).index as usize }
    }

    pub fn time_base(&self) -> Rational {
        unsafe { Rational::from((*self.as_ptr()).time_base) }
    }

    pub fn start_time(&self) -> i64 {
        unsafe { (*self.as_ptr()).start_time }
    }

    pub fn duration(&self) -> i64 {
        unsafe { (*self.as_ptr()).duration }
    }

    pub fn frames(&self) -> i64 {
        unsafe { (*self.as_ptr()).nb_frames }
    }

    pub fn disposition(&self) -> AvDispositionFlags {
        unsafe { AvDispositionFlags::from_bits_truncate((*self.as_ptr()).disposition as c_uint) }
    }

    pub fn discard(&self) -> AVDiscard {
        unsafe { ffi::AVDiscard::from((*self.as_ptr()).discard) }
    }

    pub fn side_data(&self) -> SideDataIter {
        SideDataIter::new(self)
    }

    pub fn rate(&self) -> Rational {
        unsafe { Rational::from((*self.as_ptr()).r_frame_rate) }
    }

    pub fn avg_frame_rate(&self) -> Rational {
        unsafe { Rational::from((*self.as_ptr()).avg_frame_rate) }
    }

    pub fn metadata(&self) -> DictionaryRef {
        unsafe { DictionaryRef::wrap((*self.as_ptr()).metadata) }
    }
}

impl<'a> PartialEq for Stream<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.as_ptr() == other.as_ptr()
    }
}

impl<'a> Eq for Stream<'a> {}

pub struct SideDataIter<'a> {
    stream: &'a Stream<'a>,
    current: c_int,
}

impl<'a> SideDataIter<'a> {
    pub fn new<'sd, 's: 'sd>(stream: &'s Stream) -> SideDataIter<'sd> {
        SideDataIter { stream, current: 0 }
    }
}

impl<'a> Iterator for SideDataIter<'a> {
    type Item = SideData<'a>;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        unsafe {
            if self.current >= (*self.stream.as_ptr()).nb_side_data {
                return None;
            }

            self.current += 1;

            Some(SideData::wrap(
                (*self.stream.as_ptr())
                    .side_data
                    .offset((self.current - 1) as isize),
            ))
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        unsafe {
            let length = (*self.stream.as_ptr()).nb_side_data as usize;

            (
                length - self.current as usize,
                Some(length - self.current as usize),
            )
        }
    }
}

impl<'a> ExactSizeIterator for SideDataIter<'a> {}

/////////////////////////////////////////
pub struct StreamMut<'a> {
    context: &'a mut AVFormatContextInput,
    index: usize,
    immutable: Stream<'a>,
}

impl<'a> StreamMut<'a> {
    /// Wraps a mutable reference to `AVFormatContextInput` and an index into a `StreamMut`.
    ///
    /// # Safety
    pub unsafe fn wrap(context: &mut AVFormatContextInput, index: usize) -> StreamMut {
        unsafe {
            StreamMut {
                context: std::mem::transmute_copy(&context),
                index,
                immutable: Stream::wrap(std::mem::transmute_copy(&context), index),
            }
        }
    }

    pub fn as_mut_ptr(&mut self) -> *mut ffi::AVStream {
        unsafe { *(*self.context.as_mut_ptr()).streams.add(self.index) }
    }
}

impl<'a> StreamMut<'a> {
    pub fn set_time_base<R: Into<Rational>>(&mut self, value: R) {
        unsafe {
            (*self.as_mut_ptr()).time_base = value.into().into();
        }
    }

    pub fn set_rate<R: Into<Rational>>(&mut self, value: R) {
        unsafe {
            (*self.as_mut_ptr()).r_frame_rate = value.into().into();
        }
    }

    pub fn set_avg_frame_rate<R: Into<Rational>>(&mut self, value: R) {
        unsafe {
            (*self.as_mut_ptr()).avg_frame_rate = value.into().into();
        }
    }

    // pub fn set_parameters<P: Into<codec::Parameters>>(&mut self, parameters: P) {
    //     let parameters = parameters.into();
    //
    //     unsafe {
    //         avcodec_parameters_copy((*self.as_mut_ptr()).codecpar, parameters.as_ptr());
    //     }
    // }

    pub fn set_metadata(&mut self, metadata: Dictionary) {
        unsafe {
            let metadata = metadata.disown();
            (*self.as_mut_ptr()).metadata = metadata;
        }
    }
}

impl<'a> Deref for StreamMut<'a> {
    type Target = Stream<'a>;

    fn deref(&self) -> &Self::Target {
        &self.immutable
    }
}
