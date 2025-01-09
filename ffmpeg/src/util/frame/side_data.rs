use std::ffi::CStr;
use std::marker::PhantomData;
use std::slice;
use std::str::from_utf8_unchecked;

use super::Frame;
use crate::DictionaryRef;
use rsmpeg::ffi;

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum Type {
    PanScan,
    A53CC,
    Stereo3D,
    MatrixEncoding,
    DownMixInfo,
    ReplayGain,
    DisplayMatrix,
    AFD,
    MotionVectors,
    SkipSamples,
    AudioServiceType,
    MasteringDisplayMetadata,
    GOPTimecode,
    Spherical,

    ContentLightLevel,
    IccProfile,

    // #[cfg(all(feature = "ffmpeg_4_0", not(feature = "ffmpeg_5_0")))]
    QPTableProperties,
    // #[cfg(all(feature = "ffmpeg_4_0", not(feature = "ffmpeg_5_0")))]
    QPTableData,

    // #[cfg(feature = "ffmpeg_4_1")]
    S12M_TIMECODE,

    // #[cfg(feature = "ffmpeg_4_2")]
    DYNAMIC_HDR_PLUS,
    // #[cfg(feature = "ffmpeg_4_2")]
    REGIONS_OF_INTEREST,

    // #[cfg(feature = "ffmpeg_4_3")]
    VIDEO_ENC_PARAMS,

    // #[cfg(feature = "ffmpeg_4_4")]
    SEI_UNREGISTERED,
    // #[cfg(feature = "ffmpeg_4_4")]
    FILM_GRAIN_PARAMS,

    // #[cfg(feature = "ffmpeg_5_0")]
    DETECTION_BBOXES,
    // #[cfg(feature = "ffmpeg_5_0")]
    DOVI_RPU_BUFFER,
    // #[cfg(feature = "ffmpeg_5_0")]
    DOVI_METADATA,

    // #[cfg(feature = "ffmpeg_5_1")]
    DYNAMIC_HDR_VIVID,

    #[cfg(feature = "ffmpeg6")]
    AMBIENT_VIEWING_ENVIRONMENT,
    #[cfg(feature = "ffmpeg6")]
    VIDEO_HINT,

    #[cfg(feature = "ffmpeg7")]
    LCEVC,
    #[cfg(feature = "ffmpeg7")]
    VIEW_ID,
}

impl Type {
    #[inline]
    pub fn name(&self) -> &'static str {
        unsafe {
            from_utf8_unchecked(
                CStr::from_ptr(ffi::av_frame_side_data_name((*self).into())).to_bytes(),
            )
        }
    }
}

impl From<ffi::AVFrameSideDataType> for Type {
    #[inline(always)]
    fn from(value: ffi::AVFrameSideDataType) -> Self {
        match value {
            ffi::AV_FRAME_DATA_PANSCAN => Type::PanScan,
            ffi::AV_FRAME_DATA_A53_CC => Type::A53CC,
            ffi::AV_FRAME_DATA_STEREO3D => Type::Stereo3D,
            ffi::AV_FRAME_DATA_MATRIXENCODING => Type::MatrixEncoding,
            ffi::AV_FRAME_DATA_DOWNMIX_INFO => Type::DownMixInfo,
            ffi::AV_FRAME_DATA_REPLAYGAIN => Type::ReplayGain,
            ffi::AV_FRAME_DATA_DISPLAYMATRIX => Type::DisplayMatrix,
            ffi::AV_FRAME_DATA_AFD => Type::AFD,
            ffi::AV_FRAME_DATA_MOTION_VECTORS => Type::MotionVectors,
            ffi::AV_FRAME_DATA_SKIP_SAMPLES => Type::SkipSamples,
            ffi::AV_FRAME_DATA_AUDIO_SERVICE_TYPE => Type::AudioServiceType,
            ffi::AV_FRAME_DATA_MASTERING_DISPLAY_METADATA => Type::MasteringDisplayMetadata,
            ffi::AV_FRAME_DATA_GOP_TIMECODE => Type::GOPTimecode,
            ffi::AV_FRAME_DATA_SPHERICAL => Type::Spherical,

            ffi::AV_FRAME_DATA_CONTENT_LIGHT_LEVEL => Type::ContentLightLevel,
            ffi::AV_FRAME_DATA_ICC_PROFILE => Type::IccProfile,

            // #[cfg(all(feature = "ffmpeg_4_0", not(feature = "ffmpeg_5_0")))]
            // ffi::AV_FRAME_DATA_QP_TABLE_PROPERTIES => Type::QPTableProperties,
            // #[cfg(all(feature = "ffmpeg_4_0", not(feature = "ffmpeg_5_0")))]
            // ffi::AV_FRAME_DATA_QP_TABLE_DATA => Type::QPTableData,
            // #[cfg(feature = "ffmpeg_4_1")]
            ffi::AV_FRAME_DATA_S12M_TIMECODE => Type::S12M_TIMECODE,

            // #[cfg(feature = "ffmpeg_4_2")]
            ffi::AV_FRAME_DATA_DYNAMIC_HDR_PLUS => Type::DYNAMIC_HDR_PLUS,
            // #[cfg(feature = "ffmpeg_4_2")]
            ffi::AV_FRAME_DATA_REGIONS_OF_INTEREST => Type::REGIONS_OF_INTEREST,

            // #[cfg(feature = "ffmpeg_4_3")]
            ffi::AV_FRAME_DATA_VIDEO_ENC_PARAMS => Type::VIDEO_ENC_PARAMS,

            // #[cfg(feature = "ffmpeg_4_4")]
            ffi::AV_FRAME_DATA_SEI_UNREGISTERED => Type::SEI_UNREGISTERED,
            // #[cfg(feature = "ffmpeg_4_4")]
            ffi::AV_FRAME_DATA_FILM_GRAIN_PARAMS => Type::FILM_GRAIN_PARAMS,

            // #[cfg(feature = "ffmpeg_5_0")]
            ffi::AV_FRAME_DATA_DETECTION_BBOXES => Type::DETECTION_BBOXES,
            // #[cfg(feature = "ffmpeg_5_0")]
            ffi::AV_FRAME_DATA_DOVI_RPU_BUFFER => Type::DOVI_RPU_BUFFER,
            // #[cfg(feature = "ffmpeg_5_0")]
            ffi::AV_FRAME_DATA_DOVI_METADATA => Type::DOVI_METADATA,

            // #[cfg(feature = "ffmpeg_5_1")]
            ffi::AV_FRAME_DATA_DYNAMIC_HDR_VIVID => Type::DYNAMIC_HDR_VIVID,

            #[cfg(feature = "ffmpeg6")]
            ffi::AV_FRAME_DATA_AMBIENT_VIEWING_ENVIRONMENT => Type::AMBIENT_VIEWING_ENVIRONMENT,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_FRAME_DATA_VIDEO_HINT => Type::VIDEO_HINT,

            #[cfg(feature = "ffmpeg7")]
            ffi::AV_FRAME_DATA_LCEVC => Type::LCEVC,
            #[cfg(feature = "ffmpeg7")]
            ffi::AV_FRAME_DATA_VIEW_ID => Type::VIEW_ID,

            //  non-exhaustive patterns: `16_u32..=u32::MAX` not covered
            16_u32..=u32::MAX => todo!(),
        }
    }
}

impl From<Type> for ffi::AVFrameSideDataType {
    #[inline(always)]
    fn from(value: Type) -> ffi::AVFrameSideDataType {
        match value {
            Type::PanScan => ffi::AV_FRAME_DATA_PANSCAN,
            Type::A53CC => ffi::AV_FRAME_DATA_A53_CC,
            Type::Stereo3D => ffi::AV_FRAME_DATA_STEREO3D,
            Type::MatrixEncoding => ffi::AV_FRAME_DATA_MATRIXENCODING,
            Type::DownMixInfo => ffi::AV_FRAME_DATA_DOWNMIX_INFO,
            Type::ReplayGain => ffi::AV_FRAME_DATA_REPLAYGAIN,
            Type::DisplayMatrix => ffi::AV_FRAME_DATA_DISPLAYMATRIX,
            Type::AFD => ffi::AV_FRAME_DATA_AFD,
            Type::MotionVectors => ffi::AV_FRAME_DATA_MOTION_VECTORS,
            Type::SkipSamples => ffi::AV_FRAME_DATA_SKIP_SAMPLES,
            Type::AudioServiceType => ffi::AV_FRAME_DATA_AUDIO_SERVICE_TYPE,
            Type::MasteringDisplayMetadata => ffi::AV_FRAME_DATA_MASTERING_DISPLAY_METADATA,
            Type::GOPTimecode => ffi::AV_FRAME_DATA_GOP_TIMECODE,
            Type::Spherical => ffi::AV_FRAME_DATA_SPHERICAL,

            Type::ContentLightLevel => ffi::AV_FRAME_DATA_CONTENT_LIGHT_LEVEL,
            Type::IccProfile => ffi::AV_FRAME_DATA_ICC_PROFILE,

            // #[cfg(all(feature = "ffmpeg_4_0", not(feature = "ffmpeg_5_0")))]
            Type::QPTableProperties => panic!("not implemented"), // ffi::AV_FRAME_DATA_QP_TABLE_PROPERTIES,
            // #[cfg(all(feature = "ffmpeg_4_0", not(feature = "ffmpeg_5_0")))]
            Type::QPTableData => panic!("not implemented"), // ffi::AV_FRAME_DATA_QP_TABLE_DATA,
            // #[cfg(feature = "ffmpeg_4_1")]
            Type::S12M_TIMECODE => ffi::AV_FRAME_DATA_S12M_TIMECODE,

            // #[cfg(feature = "ffmpeg_4_2")]
            Type::DYNAMIC_HDR_PLUS => ffi::AV_FRAME_DATA_DYNAMIC_HDR_PLUS,
            // #[cfg(feature = "ffmpeg_4_2")]
            Type::REGIONS_OF_INTEREST => ffi::AV_FRAME_DATA_REGIONS_OF_INTEREST,

            // #[cfg(feature = "ffmpeg_4_3")]
            Type::VIDEO_ENC_PARAMS => ffi::AV_FRAME_DATA_VIDEO_ENC_PARAMS,

            // #[cfg(feature = "ffmpeg_4_4")]
            Type::SEI_UNREGISTERED => ffi::AV_FRAME_DATA_SEI_UNREGISTERED,
            // #[cfg(feature = "ffmpeg_4_4")]
            Type::FILM_GRAIN_PARAMS => ffi::AV_FRAME_DATA_FILM_GRAIN_PARAMS,

            // #[cfg(feature = "ffmpeg_5_0")]
            Type::DETECTION_BBOXES => ffi::AV_FRAME_DATA_DETECTION_BBOXES,
            // #[cfg(feature = "ffmpeg_5_0")]
            Type::DOVI_RPU_BUFFER => ffi::AV_FRAME_DATA_DOVI_RPU_BUFFER,
            // #[cfg(feature = "ffmpeg_5_0")]
            Type::DOVI_METADATA => ffi::AV_FRAME_DATA_DOVI_METADATA,

            // #[cfg(feature = "ffmpeg_5_1")]
            Type::DYNAMIC_HDR_VIVID => ffi::AV_FRAME_DATA_DYNAMIC_HDR_VIVID,

            #[cfg(feature = "ffmpeg6")]
            Type::AMBIENT_VIEWING_ENVIRONMENT => ffi::AV_FRAME_DATA_AMBIENT_VIEWING_ENVIRONMENT,
            #[cfg(feature = "ffmpeg6")]
            Type::VIDEO_HINT => ffi::AV_FRAME_DATA_VIDEO_HINT,

            #[cfg(feature = "ffmpeg7")]
            Type::LCEVC => ffi::AV_FRAME_DATA_LCEVC,
            #[cfg(feature = "ffmpeg7")]
            Type::VIEW_ID => ffi::AV_FRAME_DATA_VIEW_ID,
        }
    }
}

pub struct SideData<'a> {
    ptr: *mut ffi::AVFrameSideData,

    _marker: PhantomData<&'a Frame>,
}

impl<'a> SideData<'a> {
    #[inline(always)]
    pub unsafe fn wrap(ptr: *mut ffi::AVFrameSideData) -> Self {
        SideData {
            ptr,
            _marker: PhantomData,
        }
    }

    #[inline(always)]
    pub unsafe fn as_ptr(&self) -> *const ffi::AVFrameSideData {
        self.ptr as *const _
    }

    #[inline(always)]
    pub unsafe fn as_mut_ptr(&mut self) -> *mut ffi::AVFrameSideData {
        self.ptr
    }
}

impl<'a> SideData<'a> {
    #[inline]
    pub fn kind(&self) -> Type {
        unsafe { Type::from((*self.as_ptr()).type_) }
    }

    #[inline]
    pub fn data(&self) -> &[u8] {
        #[allow(clippy::unnecessary_cast)]
        unsafe {
            slice::from_raw_parts((*self.as_ptr()).data, (*self.as_ptr()).size as usize)
        }
    }

    #[inline]
    pub fn metadata(&self) -> DictionaryRef {
        unsafe { DictionaryRef::wrap((*self.as_ptr()).metadata) }
    }
}
