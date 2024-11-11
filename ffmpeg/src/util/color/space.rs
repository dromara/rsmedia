use std::ffi::CStr;
use std::str::from_utf8_unchecked;

use rsmpeg::ffi;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Space {
    RGB,
    BT709,
    Unspecified,
    Reserved,
    FCC,
    BT470BG,
    SMPTE170M,
    SMPTE240M,
    YCGCO,
    BT2020NCL,
    BT2020CL,
    SMPTE2085,

    ChromaDerivedNCL,
    ChromaDerivedCL,
    ICTCP,

    #[cfg(feature = "ffmpeg7")]
    IPT_C2,
    #[cfg(feature = "ffmpeg7")]
    YCGCO_RE,
    #[cfg(feature = "ffmpeg7")]
    YCGCO_RO,
}

impl Space {
    pub const YCOCG: Space = Space::YCGCO;

    pub fn name(&self) -> Option<&'static str> {
        if *self == Space::Unspecified {
            return None;
        }
        unsafe {
            let ptr = ffi::av_color_space_name((*self).into());
            ptr.as_ref()
                .map(|ptr| from_utf8_unchecked(CStr::from_ptr(ptr).to_bytes()))
        }
    }
}

impl From<ffi::AVColorSpace> for Space {
    fn from(value: ffi::AVColorSpace) -> Self {
        match value {
            ffi::AVCOL_SPC_RGB => Space::RGB,
            ffi::AVCOL_SPC_BT709 => Space::BT709,
            ffi::AVCOL_SPC_UNSPECIFIED => Space::Unspecified,
            ffi::AVCOL_SPC_RESERVED => Space::Reserved,
            ffi::AVCOL_SPC_FCC => Space::FCC,
            ffi::AVCOL_SPC_BT470BG => Space::BT470BG,
            ffi::AVCOL_SPC_SMPTE170M => Space::SMPTE170M,
            ffi::AVCOL_SPC_SMPTE240M => Space::SMPTE240M,
            ffi::AVCOL_SPC_YCGCO => Space::YCGCO,
            ffi::AVCOL_SPC_BT2020_NCL => Space::BT2020NCL,
            ffi::AVCOL_SPC_BT2020_CL => Space::BT2020CL,
            ffi::AVCOL_SPC_SMPTE2085 => Space::SMPTE2085,
            ffi::AVCOL_SPC_NB => Space::Unspecified,

            ffi::AVCOL_SPC_CHROMA_DERIVED_NCL => Space::ChromaDerivedNCL,
            ffi::AVCOL_SPC_CHROMA_DERIVED_CL => Space::ChromaDerivedCL,
            ffi::AVCOL_SPC_ICTCP => Space::ICTCP,

            #[cfg(feature = "ffmpeg7")]
            ffi::AVCOL_SPC_IPT_C2 => Space::IPT_C2,
            #[cfg(feature = "ffmpeg7")]
            ffi::AVCOL_SPC_YCGCO_RE => Space::YCGCO_RE,
            #[cfg(feature = "ffmpeg7")]
            ffi::AVCOL_SPC_YCGCO_RO => Space::YCGCO_RO,

            16_u32..=u32::MAX => todo!(),
        }
    }
}

impl From<Space> for ffi::AVColorSpace {
    fn from(value: Space) -> ffi::AVColorSpace {
        match value {
            Space::RGB => ffi::AVCOL_SPC_RGB,
            Space::BT709 => ffi::AVCOL_SPC_BT709,
            Space::Unspecified => ffi::AVCOL_SPC_UNSPECIFIED,
            Space::Reserved => ffi::AVCOL_SPC_RESERVED,
            Space::FCC => ffi::AVCOL_SPC_FCC,
            Space::BT470BG => ffi::AVCOL_SPC_BT470BG,
            Space::SMPTE170M => ffi::AVCOL_SPC_SMPTE170M,
            Space::SMPTE240M => ffi::AVCOL_SPC_SMPTE240M,
            Space::YCGCO => ffi::AVCOL_SPC_YCGCO,
            Space::BT2020NCL => ffi::AVCOL_SPC_BT2020_NCL,
            Space::BT2020CL => ffi::AVCOL_SPC_BT2020_CL,
            Space::SMPTE2085 => ffi::AVCOL_SPC_SMPTE2085,

            Space::ChromaDerivedNCL => ffi::AVCOL_SPC_CHROMA_DERIVED_NCL,
            Space::ChromaDerivedCL => ffi::AVCOL_SPC_CHROMA_DERIVED_CL,
            Space::ICTCP => ffi::AVCOL_SPC_ICTCP,

            #[cfg(feature = "ffmpeg7")]
            Space::IPT_C2 => ffi::AVCOL_SPC_IPT_C2,
            #[cfg(feature = "ffmpeg7")]
            Space::YCGCO_RE => ffi::AVCOL_SPC_YCGCO_RE,
            #[cfg(feature = "ffmpeg7")]
            Space::YCGCO_RO => ffi::AVCOL_SPC_YCGCO_RO,
        }
    }
}
