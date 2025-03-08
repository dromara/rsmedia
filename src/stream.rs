use crate::io::{Reader, Writer};
use crate::packet::Packet;
use crate::{utils, Options, Rational};

use rsmpeg::avcodec::AVCodecParametersRef;
use rsmpeg::avformat::AVStream;
use rsmpeg::avutil::{AVDictionaryRef, AVMediaType};
use rsmpeg::ffi;

use anyhow::{Error, Result};
use libc::c_int;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::ptr::NonNull;

/// Holds transferable stream information. This can be used to duplicate stream settings for the
/// purpose of transmuxing or transcoding.
// #[derive(Debug, Clone)]
pub struct StreamInfo {
    /// Stream index
    pub index: usize,
    /// Media type video/audio/subtitle
    pub media_type: AVMediaType,
    /// Stream codec
    pub codec: isize,
    /// Pixel format / Sample format
    pub format: i32,
    /// time_base
    pub time_base: Rational,
    /// Stream Duration
    pub duration: i64,
    /// Start time
    pub start_time: i64,
    /// Number of frames
    pub nb_frames: i64,
    // Video parameters
    /// Video width
    pub width: i32,
    /// Video height
    pub height: i32,
    /// Video bit_rate
    pub bit_rate: i64,
    /// Video frame rate FPS
    pub frame_rate: f32,
    pub avg_frame_rate: f32,
    pub real_frame_rate: f32,
    /// video_delay
    pub video_delay: i32,
    /// Video GOP size
    pub gop_size: i32,
    /// Video has B frames
    pub has_b_frames: i32,
    /// Video sample aspect ratio
    pub sample_aspect_ratio: Rational,
    /// Display aspect ratio
    pub display_aspect_ratio: Rational,
    /// Video color space, eg: ffi::AVCOL_SPC_*
    pub color_space: usize,
    /// Video color range, eg: ffi::AVCOL_RANGE_*
    pub color_range: usize,
    /// Video color primaries, eg: ffi::AVCOL_PRI_*
    pub color_primaries: usize,
    /// Video color transfer, eg: ffi::AVCOL_TRC_*
    pub color_transfer: usize,
    /// Video field order
    pub field_order: usize,
    /// Video profile
    pub profile: i32,
    /// Video level, eg. 3.1, 4.1 etc.
    pub level: i32,
    /// Video rotation
    pub rotation: f64,
    // Audio parameters
    /// Audio sample rate
    pub sample_rate: i32,
    /// Audio number of channels
    pub channels: i32,
    /// Audio channel layout
    pub channel_layout: usize,
    /// Audio frame size
    pub frame_size: i32,
    /// Audio block align
    pub block_align: i32,
    // extra
    pub extra_data: Option<Vec<u8>>,
    pub metadata: HashMap<String, String>,
    pub codec_parameters: NonNull<ffi::AVCodecParameters>,
}

impl StreamInfo {
    /// Fetch stream information from a reader by stream index.
    ///
    /// # Arguments
    ///
    /// * `reader` - Reader to find stream information from.
    /// * `stream_index` - Index of stream in reader.
    pub fn from_reader(reader: &Reader, stream_index: usize) -> Result<Self> {
        let stream = reader
            .input
            .streams()
            .get(stream_index)
            .ok_or(Error::msg(format!(
                "reader stream: {} not found!",
                stream_index
            )))?;

        Self::from_stream(stream)
    }

    pub fn from_writer<W: Writer>(writer: &W, stream_index: usize) -> Result<Self> {
        let stream = writer
            .output()
            .streams()
            .get(stream_index)
            .ok_or(Error::msg(format!(
                "writer stream: {} not found!",
                stream_index
            )))?;

        Self::from_stream(stream)
    }

    pub fn from_stream(stream: &AVStream) -> Result<Self> {
        let codecpar = stream.codecpar();
        Ok(Self {
            index: stream.index as usize,
            media_type: codecpar.codec_type(),
            codec: codecpar.codec_id as isize,
            format: codecpar.format,
            time_base: stream.time_base.into(),
            duration: stream.duration,
            start_time: stream.start_time,
            nb_frames: stream.nb_frames,
            // Video
            width: codecpar.width,
            height: codecpar.height,
            bit_rate: codecpar.bit_rate,
            frame_rate: ffi::av_q2d(codecpar.framerate) as f32,
            avg_frame_rate: ffi::av_q2d(stream.avg_frame_rate) as f32,
            real_frame_rate: ffi::av_q2d(stream.r_frame_rate) as f32,
            video_delay: codecpar.video_delay,
            gop_size: 0,
            has_b_frames: 0,
            sample_aspect_ratio: codecpar.sample_aspect_ratio.into(),
            display_aspect_ratio: stream.sample_aspect_ratio.into(),
            color_space: codecpar.color_space as usize,
            color_range: codecpar.color_range as usize,
            color_primaries: codecpar.color_primaries as usize,
            color_transfer: codecpar.color_trc as usize,
            field_order: codecpar.field_order as usize,
            profile: codecpar.profile,
            level: codecpar.level,
            rotation: Self::display_rotation(stream),
            // Audio
            sample_rate: codecpar.sample_rate,
            channels: codecpar.ch_layout.nb_channels,
            channel_layout: codecpar.ch_layout.order as usize,
            frame_size: codecpar.frame_size,
            block_align: codecpar.block_align,
            // extra
            extra_data: None,
            metadata: stream
                .metadata()
                .map_or(HashMap::new(), |d| Options::new(d.to_owned()).into()),
            codec_parameters: NonNull::new(stream.codecpar).unwrap(),
        })
    }

    fn display_rotation(_stream: &AVStream) -> f64 {
        // TODO: ffi::av_display_rotation_get
        0.0f64
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
    pub fn into_parts(self) -> (usize, NonNull<ffi::AVCodecParameters>, Rational) {
        (self.index, self.codec_parameters, self.time_base)
    }
}

impl std::fmt::Display for StreamInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let codec_name = unsafe {
            #[allow(clippy::missing_transmute_annotations)]
            utils::from_cstr(ffi::avcodec_get_name(std::mem::transmute(
                self.codec as i32,
            )))
        };
        let pix_fmt = unsafe {
            if self.media_type.is_video() {
                utils::from_cstr(ffi::av_get_pix_fmt_name(self.format as c_int))
            } else if self.media_type.is_audio() {
                utils::from_cstr(ffi::av_get_sample_fmt_name(self.format as c_int))
            } else {
                "unknown".to_string()
            }
        };
        let stream_type = if self.media_type.is_video() {
            "video"
        } else if self.media_type.is_audio() {
            "audio"
        } else if self.media_type.is_data() {
            "data"
        } else if self.media_type.is_subtitle() {
            "subtitle"
        } else {
            "unknown"
        };
        write!(
            f,
            "{} #{}: codec={}, pix_fmt={}, size={}x{}, bit_rate={}, fps={:.3}, frame_rate={:.3}, video_delay={}",
            stream_type,
            self.index,
            codec_name,
            pix_fmt,
            self.width,
            self.height,
            self.bit_rate,
            self.avg_frame_rate,
            self.frame_rate,
            self.video_delay
        )
    }
}

impl std::fmt::Debug for StreamInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

unsafe impl Send for StreamInfo {}
unsafe impl Sync for StreamInfo {}

//////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////

pub struct Stream<'a> {
    av_stream: &'a AVStream,
}

impl<'a> Stream<'a> {
    pub fn wrap(av_stream: &'a AVStream) -> Stream<'a> {
        Stream { av_stream }
    }
}

impl Stream<'_> {
    pub fn id(&self) -> i32 {
        self.av_stream.id
    }

    pub fn index(&self) -> usize {
        self.av_stream.index as usize
    }

    pub fn time_base(&self) -> Rational {
        Rational::from(self.av_stream.time_base)
    }

    pub fn start_time(&self) -> i64 {
        self.av_stream.start_time
    }

    pub fn duration(&self) -> i64 {
        self.av_stream.duration
    }

    pub fn nb_frames(&self) -> i64 {
        self.av_stream.nb_frames
    }

    pub fn disposition(&self) -> i32 {
        self.av_stream.disposition
    }

    pub fn discard(&self) -> ffi::AVDiscard {
        self.av_stream.discard
    }

    pub fn side_data(&self) -> StreamSideDataIter {
        StreamSideDataIter::new(self)
    }

    pub fn r_frame_rate(&self) -> Rational {
        self.av_stream.r_frame_rate.into()
    }

    pub fn avg_frame_rate(&self) -> Rational {
        self.av_stream.avg_frame_rate.into()
    }

    pub fn parameters(&self) -> AVCodecParametersRef {
        self.av_stream.codecpar()
    }

    pub fn metadata(&self) -> Option<AVDictionaryRef> {
        self.av_stream.metadata()
    }
}

impl PartialEq for Stream<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.av_stream.as_ptr() == other.av_stream.as_ptr()
    }
}

impl Eq for Stream<'_> {}

/////////////////////////////////////////////////////////////////////
/////////////////////////////////////////////////////////////////////

pub struct StreamSideData<'a> {
    ptr: *mut ffi::AVPacketSideData,
    _marker: PhantomData<&'a Packet>,
}

impl StreamSideData<'_> {
    pub fn wrap(ptr: *mut ffi::AVPacketSideData) -> Self {
        StreamSideData {
            ptr,
            _marker: PhantomData,
        }
    }

    pub fn as_ptr(&self) -> *const ffi::AVPacketSideData {
        self.ptr as *const _
    }
}

impl StreamSideData<'_> {
    pub fn kind(&self) -> ffi::AVPacketSideDataType {
        unsafe { ffi::AVPacketSideDataType::from((*self.as_ptr()).type_) }
    }

    pub fn data(&self) -> &[u8] {
        #[allow(clippy::unnecessary_cast)]
        unsafe {
            std::slice::from_raw_parts((*self.as_ptr()).data, (*self.as_ptr()).size as usize)
        }
    }
}

/////////////////////////////////////////////////////////////////////
/////////////////////////////////////////////////////////////////////

pub struct StreamSideDataIter<'a> {
    stream: &'a Stream<'a>,
    current: c_int,
}

impl StreamSideDataIter<'_> {
    pub fn new<'sd, 's: 'sd>(stream: &'s Stream) -> StreamSideDataIter<'sd> {
        StreamSideDataIter { stream, current: 0 }
    }
}

impl<'a> Iterator for StreamSideDataIter<'a> {
    type Item = StreamSideData<'a>;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        unsafe {
            if self.current >= self.stream.av_stream.nb_side_data {
                return None;
            }

            self.current += 1;

            Some(StreamSideData::wrap(
                self.stream
                    .av_stream
                    .side_data
                    .offset((self.current - 1) as isize),
            ))
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let length = self.stream.av_stream.nb_side_data as usize;
        (
            length - self.current as usize,
            Some(length - self.current as usize),
        )
    }
}

impl ExactSizeIterator for StreamSideDataIter<'_> {}
