use std::marker::PhantomData;
use std::slice;

use super::Packet;
use rsmpeg::ffi;

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum Type {
    Palette,
    NewExtraData,
    ParamChange,
    H263MbInfo,
    ReplayGain,
    DisplayMatrix,
    Stereo3d,
    AudioServiceType,
    QualityStats,
    FallbackTrack,
    CBPProperties,
    SkipSamples,
    JpDualMono,
    StringsMetadata,
    SubtitlePosition,
    MatroskaBlockAdditional,
    WebVTTIdentifier,
    WebVTTSettings,
    MetadataUpdate,
    MPEGTSStreamID,
    MasteringDisplayMetadata,
    DataSpherical,
    DataNb,

    ContentLightLevel,
    A53CC,


    #[cfg(feature = "ffmpeg7")]
    IAMF_MIX_GAIN_PARAM,
    #[cfg(feature = "ffmpeg7")]
    IAMF_DEMIXING_INFO_PARAM,
    #[cfg(feature = "ffmpeg7")]
    IAMF_RECON_GAIN_INFO_PARAM,
    #[cfg(feature = "ffmpeg7")]
    AMBIENT_VIEWING_ENVIRONMENT,
}

impl From<ffi::AVPacketSideDataType> for Type {
    fn from(value: ffi::AVPacketSideDataType) -> Self {
        match value {
            ffi::AV_PKT_DATA_PALETTE => Type::Palette,
            ffi::AV_PKT_DATA_NEW_EXTRADATA => Type::NewExtraData,
            ffi::AV_PKT_DATA_PARAM_CHANGE => Type::ParamChange,
            ffi::AV_PKT_DATA_H263_MB_INFO => Type::H263MbInfo,
            ffi::AV_PKT_DATA_REPLAYGAIN => Type::ReplayGain,
            ffi::AV_PKT_DATA_DISPLAYMATRIX => Type::DisplayMatrix,
            ffi::AV_PKT_DATA_STEREO3D => Type::Stereo3d,
            ffi::AV_PKT_DATA_AUDIO_SERVICE_TYPE => Type::AudioServiceType,
            ffi::AV_PKT_DATA_QUALITY_STATS => Type::QualityStats,
            ffi::AV_PKT_DATA_FALLBACK_TRACK => Type::FallbackTrack,
            ffi::AV_PKT_DATA_CPB_PROPERTIES => Type::CBPProperties,
            ffi::AV_PKT_DATA_SKIP_SAMPLES => Type::SkipSamples,
            ffi::AV_PKT_DATA_JP_DUALMONO => Type::JpDualMono,
            ffi::AV_PKT_DATA_STRINGS_METADATA => Type::StringsMetadata,
            ffi::AV_PKT_DATA_SUBTITLE_POSITION => Type::SubtitlePosition,
            ffi::AV_PKT_DATA_MATROSKA_BLOCKADDITIONAL => Type::MatroskaBlockAdditional,
            ffi::AV_PKT_DATA_WEBVTT_IDENTIFIER => Type::WebVTTIdentifier,
            ffi::AV_PKT_DATA_WEBVTT_SETTINGS => Type::WebVTTSettings,
            ffi::AV_PKT_DATA_METADATA_UPDATE => Type::MetadataUpdate,
            ffi::AV_PKT_DATA_MPEGTS_STREAM_ID => Type::MPEGTSStreamID,
            ffi::AV_PKT_DATA_MASTERING_DISPLAY_METADATA => Type::MasteringDisplayMetadata,
            ffi::AV_PKT_DATA_SPHERICAL => Type::DataSpherical,
            ffi::AV_PKT_DATA_NB => Type::DataNb,

            ffi::AV_PKT_DATA_CONTENT_LIGHT_LEVEL => Type::ContentLightLevel,
            ffi::AV_PKT_DATA_A53_CC => Type::A53CC,


            #[cfg(feature = "ffmpeg7")]
            ffi::AV_PKT_DATA_IAMF_MIX_GAIN_PARAM => Type::IAMF_MIX_GAIN_PARAM,
            #[cfg(feature = "ffmpeg7")]
            ffi::AV_PKT_DATA_IAMF_DEMIXING_INFO_PARAM => Type::IAMF_DEMIXING_INFO_PARAM,
            #[cfg(feature = "ffmpeg7")]
            ffi::AV_PKT_DATA_IAMF_RECON_GAIN_INFO_PARAM => Type::IAMF_RECON_GAIN_INFO_PARAM,
            #[cfg(feature = "ffmpeg7")]
            ffi::AV_PKT_DATA_AMBIENT_VIEWING_ENVIRONMENT => Type::AMBIENT_VIEWING_ENVIRONMENT,
            // non-exhaustive patterns: `24_u32..=31_u32` and `37_u32..=u32::MAX` not covered
            24_u32..=31_u32 | 37_u32..=u32::MAX => todo!(),
        }
    }
}

impl From<Type> for ffi::AVPacketSideDataType {
    fn from(value: Type) -> ffi::AVPacketSideDataType {
        match value {
            Type::Palette => ffi::AV_PKT_DATA_PALETTE,
            Type::NewExtraData => ffi::AV_PKT_DATA_NEW_EXTRADATA,
            Type::ParamChange => ffi::AV_PKT_DATA_PARAM_CHANGE,
            Type::H263MbInfo => ffi::AV_PKT_DATA_H263_MB_INFO,
            Type::ReplayGain => ffi::AV_PKT_DATA_REPLAYGAIN,
            Type::DisplayMatrix => ffi::AV_PKT_DATA_DISPLAYMATRIX,
            Type::Stereo3d => ffi::AV_PKT_DATA_STEREO3D,
            Type::AudioServiceType => ffi::AV_PKT_DATA_AUDIO_SERVICE_TYPE,
            Type::QualityStats => ffi::AV_PKT_DATA_QUALITY_STATS,
            Type::FallbackTrack => ffi::AV_PKT_DATA_FALLBACK_TRACK,
            Type::CBPProperties => ffi::AV_PKT_DATA_CPB_PROPERTIES,
            Type::SkipSamples => ffi::AV_PKT_DATA_SKIP_SAMPLES,
            Type::JpDualMono => ffi::AV_PKT_DATA_JP_DUALMONO,
            Type::StringsMetadata => ffi::AV_PKT_DATA_STRINGS_METADATA,
            Type::SubtitlePosition => ffi::AV_PKT_DATA_SUBTITLE_POSITION,
            Type::MatroskaBlockAdditional => ffi::AV_PKT_DATA_MATROSKA_BLOCKADDITIONAL,
            Type::WebVTTIdentifier => ffi::AV_PKT_DATA_WEBVTT_IDENTIFIER,
            Type::WebVTTSettings => ffi::AV_PKT_DATA_WEBVTT_SETTINGS,
            Type::MetadataUpdate => ffi::AV_PKT_DATA_METADATA_UPDATE,
            Type::MPEGTSStreamID => ffi::AV_PKT_DATA_MPEGTS_STREAM_ID,
            Type::MasteringDisplayMetadata => ffi::AV_PKT_DATA_MASTERING_DISPLAY_METADATA,
            Type::DataSpherical => ffi::AV_PKT_DATA_SPHERICAL,
            Type::DataNb => ffi::AV_PKT_DATA_NB,

            Type::ContentLightLevel => ffi::AV_PKT_DATA_CONTENT_LIGHT_LEVEL,
            Type::A53CC => ffi::AV_PKT_DATA_A53_CC,

            #[cfg(feature = "ffmpeg7")]
            Type::IAMF_MIX_GAIN_PARAM => ffi::AV_PKT_DATA_IAMF_MIX_GAIN_PARAM,
            #[cfg(feature = "ffmpeg7")]
            Type::IAMF_DEMIXING_INFO_PARAM => ffi::AV_PKT_DATA_IAMF_DEMIXING_INFO_PARAM,
            #[cfg(feature = "ffmpeg7")]
            Type::IAMF_RECON_GAIN_INFO_PARAM => ffi::AV_PKT_DATA_IAMF_RECON_GAIN_INFO_PARAM,
            #[cfg(feature = "ffmpeg7")]
            Type::AMBIENT_VIEWING_ENVIRONMENT => ffi::AV_PKT_DATA_AMBIENT_VIEWING_ENVIRONMENT,
        }
    }
}

pub struct SideData<'a> {
    ptr: *mut ffi::AVPacketSideData,

    _marker: PhantomData<&'a Packet>,
}

impl<'a> SideData<'a> {
    pub unsafe fn wrap(ptr: *mut ffi::AVPacketSideData) -> Self {
        SideData {
            ptr,
            _marker: PhantomData,
        }
    }

    pub unsafe fn as_ptr(&self) -> *const ffi::AVPacketSideData {
        self.ptr as *const _
    }
}

impl<'a> SideData<'a> {
    pub fn kind(&self) -> Type {
        unsafe { Type::from((*self.as_ptr()).type_) }
    }

    pub fn data(&self) -> &[u8] {
        #[allow(clippy::unnecessary_cast)]
        unsafe {
            slice::from_raw_parts((*self.as_ptr()).data, (*self.as_ptr()).size as usize)
        }
    }
}
