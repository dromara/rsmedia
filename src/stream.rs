use crate::hwaccel::HWDeviceType;
use crate::io::{Reader, Writer};
use crate::{utils, MediaType, Options, PixelFormat, SampleFormat};

use rsmpeg::avcodec::{AVCodec, AVCodecParametersRef, AVPacket};
use rsmpeg::avformat::{AVInputFormatRef, AVStream};
use rsmpeg::avutil::AVDictionaryRef;
use rsmpeg::ffi;

use anyhow::{Error, Result};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::Deref;
use std::ptr::NonNull;

/// Holds transferable stream information. This can be used to duplicate stream settings for the
/// purpose of transmuxing or transcoding.
#[derive(Clone)]
pub struct StreamInfo {
    /// id
    pub id: i32,
    /// Stream index
    pub index: usize,
    /// Media type video/audio/subtitle
    pub media_type: MediaType,
    /// Stream codec `ffi::AVCodecID`
    pub codec_id: u32,
    /// Codec Additional Info
    pub codec_tag: u32,
    /// Video: [`PixelFormat`], Audio: [`SampleFormat`]
    pub format: i32,
    /// Number of bits per sample or zero if unknown for the given codec.
    pub bits_per_sample: i32,
    /// Only return non-zero if the bits per sample is exactly correct, not an approximation.
    pub exact_bits_per_sample: i32,
    /// the number of bits actually used for storing the pixel information,
    /// that is padding bits are not counted.
    pub bits_per_pixel: i32,
    /// the number of bits per pixel for the pixel format
    /// including any padding or unused bits.
    pub padded_bits_per_pixel: i32,

    /// time_base of stream
    pub time_base: ffi::AVRational,
    /// Stream Duration
    pub duration: i64,
    /// Start time
    pub start_time: i64,
    /// Number of frames
    pub nb_frames: i64,
    /// Bit rate
    pub bit_rate: i64,
    /// combination of AV_DISPOSITION_*
    pub disposition: i32,
    /// discard of AVDISCARD_*
    pub discard: i32,
    /// codec profile
    pub profile: i32,
    /// codec level, eg. 3.1, 4.1 etc.
    pub level: i32,

    // Video parameters
    /// Video width
    pub width: i32,
    /// Video height
    pub height: i32,
    /// Video frame rate FPS
    pub frame_rate: ffi::AVRational,
    pub avg_frame_rate: ffi::AVRational,
    pub real_frame_rate: ffi::AVRational,
    /// Number of bits in timestamps. Used for wrapping control.
    pub pts_wrap_bits: i32,
    /// Flags indicating events happening on the stream, a combination of AVSTREAM_EVENT_FLAG_*.
    pub event_flags: i32,
    /// video_delay
    pub video_delay: i32,
    /// Video sample aspect ratio
    pub sample_aspect_ratio: ffi::AVRational,
    /// Display aspect ratio
    pub display_aspect_ratio: ffi::AVRational,
    /// Video color space, eg: ffi::AVCOL_SPC_*
    pub color_space: usize,
    /// Video color range, eg: ffi::AVCOL_RANGE_*
    pub color_range: usize,
    /// Video color primaries, eg: ffi::AVCOL_PRI_*
    pub color_primaries: usize,
    /// Video color transfer, eg: ffi::AVCOL_TRC_*
    pub color_transfer: usize,
    /// Location of chroma samples, eg: ffi::AVCHROMA_LOC_*
    pub chroma_location: usize,
    /// Video field order
    pub field_order: usize,
    /// Video rotation
    pub rotation: f64,

    // Audio parameters
    /// Audio sample rate
    pub sample_rate: i32,
    /// Audio Channel layout
    pub channel_layout: ffi::AVChannelLayout,
    /// Audio frame size
    pub frame_size: i32,
    /// Audio block align
    pub block_align: i32,
    /// Initial padding
    pub initial_padding: i32,
    /// Trailing padding
    pub trailing_padding: i32,
    /// Seek preroll
    pub seek_preroll: i32,
    /// The number of bits per code sample
    pub bits_per_coded_sample: i32,
    /// Raw Sample Bit Depth
    pub bits_per_raw_sample: i32,
    /// number of bytes per sample
    pub bytes_per_sample: Option<usize>,

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
    pub fn from_reader<R: Reader>(reader: &R, stream_index: usize) -> Result<Self> {
        let stream = reader
            .input()
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
        let codec_type = codecpar.codec_type();
        let metadata = stream
            .metadata()
            .map_or(HashMap::new(), |d| Options::new(d.to_owned()).into());
        let bytes_per_sample = if codec_type.is_audio() {
            SampleFormat::from(codecpar.format).get_bytes_per_sample()
        } else {
            None
        };

        let (bits_per_sample, exact_bits_per_sample, bits_per_pixel, padded_bits_per_pixel) = unsafe {
            let bits_sample = ffi::av_get_bits_per_sample(codecpar.codec_id);
            let exact_bits_sample = ffi::av_get_exact_bits_per_sample(codecpar.codec_id);
            let (bits_pixel, padded_bits_pixel) = if codec_type.is_video() {
                let pix_fmt_desc = PixelFormat::from(codecpar.format).descriptor();
                (
                    ffi::av_get_bits_per_pixel(pix_fmt_desc.deref()),
                    ffi::av_get_padded_bits_per_pixel(pix_fmt_desc.deref()),
                )
            } else {
                (0, 0)
            };
            (
                bits_sample,
                exact_bits_sample,
                bits_pixel,
                padded_bits_pixel,
            )
        };

        Ok(Self {
            id: stream.id,
            index: stream.index as usize,
            media_type: MediaType::from(codecpar.codec_type),
            #[allow(clippy::unnecessary_cast)]
            codec_id: codecpar.codec_id as u32,
            codec_tag: codecpar.codec_tag,
            format: codecpar.format,
            bits_per_sample,
            exact_bits_per_sample,
            bits_per_pixel,
            padded_bits_per_pixel,
            time_base: stream.time_base,
            duration: stream.duration,
            start_time: stream.start_time,
            nb_frames: stream.nb_frames,
            disposition: stream.disposition,
            discard: stream.discard,
            profile: codecpar.profile,
            level: codecpar.level,
            // Video
            width: codecpar.width,
            height: codecpar.height,
            bit_rate: codecpar.bit_rate,
            frame_rate: codecpar.framerate,
            avg_frame_rate: stream.avg_frame_rate,
            real_frame_rate: stream.r_frame_rate,
            pts_wrap_bits: stream.pts_wrap_bits,
            event_flags: stream.event_flags,
            video_delay: codecpar.video_delay,
            sample_aspect_ratio: codecpar.sample_aspect_ratio,
            display_aspect_ratio: stream.sample_aspect_ratio,
            color_space: codecpar.color_space as usize,
            color_range: codecpar.color_range as usize,
            color_transfer: codecpar.color_trc as usize,
            color_primaries: codecpar.color_primaries as usize,
            chroma_location: codecpar.chroma_location as usize,
            field_order: codecpar.field_order as usize,
            rotation: Self::get_stream_display_rotation(stream, &metadata),
            // Audio
            sample_rate: codecpar.sample_rate,
            channel_layout: codecpar.ch_layout,
            frame_size: codecpar.frame_size,
            block_align: codecpar.block_align,
            initial_padding: codecpar.initial_padding,
            trailing_padding: codecpar.trailing_padding,
            seek_preroll: codecpar.seek_preroll,
            bits_per_coded_sample: codecpar.bits_per_coded_sample,
            bits_per_raw_sample: codecpar.bits_per_raw_sample,
            bytes_per_sample,
            // extra
            metadata,
            extra_data: Self::get_extra_data(stream),
            codec_parameters: NonNull::new(stream.codecpar).unwrap(),
        })
    }

    fn get_stream_display_rotation(stream: &AVStream, map: &HashMap<String, String>) -> f64 {
        fn get_rotation_from_metadata(map: &HashMap<String, String>) -> f64 {
            if let Some(value) = map.get("rotate") {
                value.parse::<f64>().unwrap_or(0.0)
            } else {
                0.0
            }
        }

        /// av_stream_new_side_data
        /// av_stream_add_side_data
        /// av_stream_get_side_data
        unsafe fn get_rotation_from_side_data(stream: &AVStream) -> f64 {
            for i in 0..stream.nb_side_data {
                let side_data = stream.side_data.offset(i as isize);
                if (*side_data).type_ == ffi::AV_PKT_DATA_DISPLAYMATRIX {
                    let matrix = ffi::av_stream_get_side_data(
                        stream.as_ptr(),
                        (*side_data).type_,
                        std::ptr::null_mut(),
                    ) as *const i32;
                    if !matrix.is_null() {
                        return ffi::av_display_rotation_get(matrix);
                    }
                }
            }
            0.0
        }

        let rotation = unsafe { get_rotation_from_side_data(stream) };
        if rotation != 0.0 {
            return rotation;
        }
        get_rotation_from_metadata(map)
    }

    #[allow(dead_code)]
    fn set_stream_display_rotation(stream: &mut AVStream, angle: f64) {
        // Display matrix in libavcodec is [i32; 9] where each point is 16.16
        // fixed point, but all this is opaque here.
        const MATRIX_LEN: usize = 9 * size_of::<i32>();
        let mut matrix = [0u8; MATRIX_LEN];

        unsafe {
            ffi::av_display_rotation_set(matrix.as_mut_ptr() as *mut i32, angle);

            let mut data_size: usize = 0;
            let mut side_data = ffi::av_stream_get_side_data(
                stream.as_ptr(),
                ffi::AV_PKT_DATA_DISPLAYMATRIX,
                &mut data_size,
            );
            if side_data.is_null() || data_size != MATRIX_LEN {
                side_data = ffi::av_stream_new_side_data(
                    stream.as_mut_ptr(),
                    ffi::AV_PKT_DATA_DISPLAYMATRIX,
                    MATRIX_LEN,
                );
            }
            side_data.copy_from(matrix.as_ptr(), MATRIX_LEN);
        }
    }

    fn get_extra_data(stream: &AVStream) -> Option<Vec<u8>> {
        let codecpar = stream.codecpar();
        if codecpar.extradata_size > 0 && !codecpar.extradata.is_null() {
            let extra_data = unsafe {
                std::slice::from_raw_parts(
                    codecpar.extradata as *const _,
                    codecpar.extradata_size as usize,
                )
            };
            Some(extra_data.to_vec())
        } else {
            None
        }
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
    pub fn into_parts(self) -> (usize, NonNull<ffi::AVCodecParameters>, ffi::AVRational) {
        (self.index, self.codec_parameters, self.time_base)
    }

    /// find codec name, if have hw_device_type, will use hw accelerated codec name
    /// if not, will use current stream codec name
    pub fn find_decoder_name(&self, hw_device_type: Option<HWDeviceType>) -> Option<String> {
        let codec_id = self.codec_id as ffi::AVCodecID;
        let codec_name = utils::to_string(AVCodec::find_decoder(codec_id)?.name()).unwrap();

        let hw_codec_name = if let Some(hw_type) = hw_device_type {
            match hw_type {
                HWDeviceType::CUDA => match codec_id {
                    ffi::AV_CODEC_ID_H264 => Some("h264_cuvid".to_string()),
                    ffi::AV_CODEC_ID_HEVC => Some("hevc_cuvid".to_string()),
                    ffi::AV_CODEC_ID_MPEG1VIDEO => Some("mpeg1_cuvid".to_string()),
                    ffi::AV_CODEC_ID_MPEG2VIDEO => Some("mpeg2_cuvid".to_string()),
                    ffi::AV_CODEC_ID_MPEG4 => Some("mpeg4_cuvid".to_string()),
                    ffi::AV_CODEC_ID_VC1 => Some("vc1_cuvid".to_string()),
                    ffi::AV_CODEC_ID_VP8 => Some("vp8_cuvid".to_string()),
                    ffi::AV_CODEC_ID_VP9 => Some("vp9_cuvid".to_string()),
                    ffi::AV_CODEC_ID_AV1 => Some("av1_cuvid".to_string()),
                    ffi::AV_CODEC_ID_MJPEG => Some("mjpeg_cuvid".to_string()),
                    _ => None,
                },
                HWDeviceType::QSV => match codec_id {
                    ffi::AV_CODEC_ID_H264 => Some("h264_qsv".to_string()),
                    ffi::AV_CODEC_ID_HEVC => Some("hevc_qsv".to_string()),
                    ffi::AV_CODEC_ID_MPEG2VIDEO => Some("mpeg2_qsv".to_string()),
                    ffi::AV_CODEC_ID_VC1 => Some("vc1_qsv".to_string()),
                    ffi::AV_CODEC_ID_VP8 => Some("vp8_qsv".to_string()),
                    ffi::AV_CODEC_ID_VP9 => Some("vp9_qsv".to_string()),
                    ffi::AV_CODEC_ID_AV1 => Some("av1_qsv".to_string()),
                    ffi::AV_CODEC_ID_MJPEG => Some("mjpeg_qsv".to_string()),
                    _ => None,
                },
                HWDeviceType::VAAPI => {
                    // VAAPI使用通用解码器，但需要特定配置
                    match codec_id {
                        ffi::AV_CODEC_ID_H264 => Some("h264_vaapi".to_string()),
                        ffi::AV_CODEC_ID_HEVC => Some("hevc_vaapi".to_string()),
                        ffi::AV_CODEC_ID_MPEG2VIDEO => Some("mpeg2_vaapi".to_string()),
                        ffi::AV_CODEC_ID_VP8 => Some("vp8_vaapi".to_string()),
                        ffi::AV_CODEC_ID_VP9 => Some("vp9_vaapi".to_string()),
                        ffi::AV_CODEC_ID_AV1 => Some("av1_vaapi".to_string()),
                        ffi::AV_CODEC_ID_MJPEG => Some("mjpeg_vaapi".to_string()),
                        ffi::AV_CODEC_ID_VC1 => Some("vc1_vaapi".to_string()),
                        _ => None,
                    }
                }
                HWDeviceType::VULKAN => match codec_id {
                    ffi::AV_CODEC_ID_H264 => Some("h264_vulkan".to_string()),
                    ffi::AV_CODEC_ID_HEVC => Some("hevc_vulkan".to_string()),
                    ffi::AV_CODEC_ID_AV1 => Some("av1_vulkan".to_string()),
                    _ => None,
                },
                _ => None,
            }
        } else {
            None
        };

        if hw_codec_name.is_some() {
            hw_codec_name
        } else {
            Some(codec_name)
        }
    }

    /// find encoder name, if we have hw_device_type, will use hw accelerated codec name
    /// if not, will use current stream codec name
    pub fn find_encoder_name(
        stream_info: &StreamInfo,
        hw_device_type: Option<HWDeviceType>,
    ) -> Option<String> {
        let codec_id = stream_info.codec_id as ffi::AVCodecID;
        let codec_name = utils::to_string(AVCodec::find_encoder(codec_id)?.name()).unwrap();

        let hw_codec_name = if let Some(hw_type) = hw_device_type {
            match hw_type {
                HWDeviceType::CUDA => match codec_id {
                    ffi::AV_CODEC_ID_H264 => Some("h264_nvenc".to_string()),
                    ffi::AV_CODEC_ID_HEVC => Some("hevc_nvenc".to_string()),
                    ffi::AV_CODEC_ID_AV1 => Some("av1_nvenc".to_string()),
                    _ => None,
                },
                HWDeviceType::QSV => match codec_id {
                    ffi::AV_CODEC_ID_H264 => Some("h264_qsv".to_string()),
                    ffi::AV_CODEC_ID_HEVC => Some("hevc_qsv".to_string()),
                    ffi::AV_CODEC_ID_MPEG2VIDEO => Some("mpeg2_qsv".to_string()),
                    ffi::AV_CODEC_ID_VP9 => Some("vp9_qsv".to_string()),
                    ffi::AV_CODEC_ID_AV1 => Some("av1_qsv".to_string()),
                    ffi::AV_CODEC_ID_MJPEG => Some("mjpeg_qsv".to_string()),
                    _ => None,
                },
                HWDeviceType::VAAPI => match codec_id {
                    ffi::AV_CODEC_ID_H264 => Some("h264_vaapi".to_string()),
                    ffi::AV_CODEC_ID_HEVC => Some("hevc_vaapi".to_string()),
                    ffi::AV_CODEC_ID_MPEG2VIDEO => Some("mpeg2_vaapi".to_string()),
                    ffi::AV_CODEC_ID_VP8 => Some("vp8_vaapi".to_string()),
                    ffi::AV_CODEC_ID_VP9 => Some("vp9_vaapi".to_string()),
                    ffi::AV_CODEC_ID_AV1 => Some("av1_vaapi".to_string()),
                    ffi::AV_CODEC_ID_MJPEG => Some("mjpeg_vaapi".to_string()),
                    _ => None,
                },
                HWDeviceType::VIDEOTOOLBOX => match codec_id {
                    ffi::AV_CODEC_ID_H264 => Some("h264_videotoolbox".to_string()),
                    ffi::AV_CODEC_ID_HEVC => Some("hevc_videotoolbox".to_string()),
                    ffi::AV_CODEC_ID_PRORES => Some("prores_videotoolbox".to_string()),
                    _ => None,
                },
                HWDeviceType::VULKAN => match codec_id {
                    ffi::AV_CODEC_ID_H264 => Some("h264_vulkan".to_string()),
                    ffi::AV_CODEC_ID_HEVC => Some("hevc_vulkan".to_string()),
                    _ => None,
                },
                _ => None,
            }
        } else {
            None
        };

        if hw_codec_name.is_some() {
            hw_codec_name
        } else {
            Some(codec_name)
        }
    }
}

impl std::fmt::Debug for StreamInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let codec_name = unsafe {
            let codec_id = self.codec_id as ffi::AVCodecID;
            utils::from_c_char(ffi::avcodec_get_name(codec_id))
        };
        let format = {
            if self.media_type == MediaType::VIDEO {
                PixelFormat::from(self.format).get_pix_fmt_name()
            } else if self.media_type == MediaType::AUDIO {
                SampleFormat::from(self.format).get_sample_fmt_name()
            } else {
                format!("Unknown:{}", self.format).to_string()
            }
        };
        let stream_type = self.media_type.get_media_type_string();
        write!(
            f,
            "{} #{}: codec={}, format={}, size={}x{}, fps={:?}, bit_rate={}, sample_rate={}, nb_channels={}, video_delay={}",
            stream_type,
            self.index,
            codec_name,
            format,
            self.width,
            self.height,
            self.avg_frame_rate,
            self.bit_rate,
            self.sample_rate,
            self.channel_layout.nb_channels,
            self.video_delay,
        )
    }
}

impl std::fmt::Display for StreamInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

unsafe impl Send for StreamInfo {}
unsafe impl Sync for StreamInfo {}

//////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////

pub struct Stream<'a> {
    av_stream: &'a AVStream,
    iformat: AVInputFormatRef<'a>,
    metadata: Option<AVDictionaryRef<'a>>,
}

impl<'a> Stream<'a> {
    pub fn wrap(
        av_stream: &'a AVStream,
        iformat: AVInputFormatRef<'a>,
        metadata: Option<AVDictionaryRef<'a>>,
    ) -> Stream<'a> {
        Stream {
            av_stream,
            iformat,
            metadata,
        }
    }

    pub fn iformat(&self) -> &AVInputFormatRef<'a> {
        &self.iformat
    }

    pub fn ctx_metadata(&self) -> &Option<AVDictionaryRef<'a>> {
        &self.metadata
    }
}

impl Stream<'_> {
    pub fn id(&self) -> i32 {
        self.av_stream.id
    }

    pub fn index(&self) -> usize {
        self.av_stream.index as usize
    }

    pub fn time_base(&self) -> ffi::AVRational {
        self.av_stream.time_base
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

    pub fn side_data(&self) -> SideDataIter<'_> {
        SideDataIter::new(self)
    }

    pub fn r_frame_rate(&self) -> ffi::AVRational {
        self.av_stream.r_frame_rate
    }

    pub fn avg_frame_rate(&self) -> ffi::AVRational {
        self.av_stream.avg_frame_rate
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
        self.av_stream.id == other.av_stream.id
    }
}

impl Eq for Stream<'_> {}

/////////////////////////////////////////////////////////////////////
/////////////////////////////////////////////////////////////////////

pub struct PacketSideData<'a> {
    ptr: *mut ffi::AVPacketSideData,
    _marker: PhantomData<&'a AVPacket>,
}

impl PacketSideData<'_> {
    pub fn wrap(ptr: *mut ffi::AVPacketSideData) -> Self {
        PacketSideData {
            ptr,
            _marker: PhantomData,
        }
    }

    pub fn as_ptr(&self) -> *const ffi::AVPacketSideData {
        self.ptr as *const _
    }
}

impl PacketSideData<'_> {
    pub fn kind(&self) -> ffi::AVPacketSideDataType {
        unsafe { ffi::AVPacketSideDataType::from((*self.as_ptr()).type_) }
    }

    pub fn size(&self) -> usize {
        unsafe { (*self.as_ptr()).size }
    }

    pub fn data(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts((*self.as_ptr()).data, (*self.as_ptr()).size) }
    }
}

/////////////////////////////////////////////////////////////////////
/////////////////////////////////////////////////////////////////////

pub struct SideDataIter<'a> {
    stream: &'a Stream<'a>,
    index: usize,
    len: usize,
}

impl SideDataIter<'_> {
    pub fn new<'sd, 's: 'sd>(stream: &'s Stream) -> SideDataIter<'sd> {
        let len = stream.av_stream.nb_side_data as usize;
        SideDataIter {
            stream,
            index: 0,
            len,
        }
    }
}

impl<'a> Iterator for SideDataIter<'a> {
    type Item = PacketSideData<'a>;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        unsafe {
            if self.index >= self.len {
                return None;
            }

            self.index += 1;

            Some(PacketSideData::wrap(
                self.stream.av_stream.side_data.add(self.index - 1),
            ))
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let hint = self.len - self.index;
        (hint, Some(hint))
    }
}

impl ExactSizeIterator for SideDataIter<'_> {}
