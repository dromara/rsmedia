use std::ffi::CStr;
use std::marker::PhantomData;
use std::slice;
use std::str::from_utf8_unchecked;

use super::Frame;
use rsmpeg::ffi;
use crate::DictionaryRef;

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

    #[cfg(feature = "ffmpeg6")]
    AMBIENT_VIEWING_ENVIRONMENT,

    #[cfg(feature = "ffmpeg6")]
    VIDEO_HINT,
}

impl Type {
    #[inline]
    pub fn name(&self) -> &'static str {
        unsafe {
            from_utf8_unchecked(CStr::from_ptr(ffi::av_frame_side_data_name((*self).into())).to_bytes())
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
            //  non-exhaustive patterns: `16_u32..=u32::MAX` not covered
            16_u32..=u32::MAX => todo!(),

            #[cfg(feature = "ffmpeg6")]
            ffi::AV_FRAME_DATA_AMBIENT_VIEWING_ENVIRONMENT => Type::AMBIENT_VIEWING_ENVIRONMENT,

            #[cfg(feature = "ffmpeg6")]
            ffi::AV_FRAME_DATA_VIDEO_HINT => Type::VIDEO_HINT,
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

            #[cfg(feature = "ffmpeg6")]
            Type::AMBIENT_VIEWING_ENVIRONMENT => ffi::AV_FRAME_DATA_AMBIENT_VIEWING_ENVIRONMENT,

            #[cfg(feature = "ffmpeg6")]
            Type::VIDEO_HINT => ffi::AV_FRAME_DATA_VIDEO_HINT,
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
