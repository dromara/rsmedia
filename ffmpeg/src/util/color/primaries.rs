use rsmpeg::ffi;
use std::ffi::CStr;
use std::str::from_utf8_unchecked;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Primaries {
    Reserved0,
    BT709,
    Unspecified,
    Reserved,
    BT470M,

    BT470BG,
    SMPTE170M,
    SMPTE240M,
    Film,
    BT2020,

    SMPTE428,
    SMPTE431,
    SMPTE432,
    // #[cfg(not(feature = "ffmpeg_4_3"))]
    JEDEC_P22,
    // #[cfg(feature = "ffmpeg_4_3")]
    EBU3213,
}

impl Primaries {
    // #[cfg(feature = "ffmpeg_4_3")]
    pub const JEDEC_P22: Primaries = Primaries::EBU3213;

    pub fn name(&self) -> Option<&'static str> {
        if *self == Primaries::Unspecified {
            return None;
        }
        unsafe {
            let ptr = ffi::av_color_primaries_name((*self).into());
            ptr.as_ref()
                .map(|ptr| from_utf8_unchecked(CStr::from_ptr(ptr).to_bytes()))
        }
    }
}

impl From<ffi::AVColorPrimaries> for Primaries {
    fn from(value: ffi::AVColorPrimaries) -> Primaries {
        match value {
            ffi::AVCOL_PRI_RESERVED0 => Primaries::Reserved0,
            ffi::AVCOL_PRI_BT709 => Primaries::BT709,
            ffi::AVCOL_PRI_UNSPECIFIED => Primaries::Unspecified,
            ffi::AVCOL_PRI_RESERVED => Primaries::Reserved,
            ffi::AVCOL_PRI_BT470M => Primaries::BT470M,

            ffi::AVCOL_PRI_BT470BG => Primaries::BT470BG,
            ffi::AVCOL_PRI_SMPTE170M => Primaries::SMPTE170M,
            ffi::AVCOL_PRI_SMPTE240M => Primaries::SMPTE240M,
            ffi::AVCOL_PRI_FILM => Primaries::Film,
            ffi::AVCOL_PRI_BT2020 => Primaries::BT2020,
            ffi::AVCOL_PRI_NB => Primaries::Reserved0,

            ffi::AVCOL_PRI_SMPTE428 => Primaries::SMPTE428,
            ffi::AVCOL_PRI_SMPTE431 => Primaries::SMPTE431,
            ffi::AVCOL_PRI_SMPTE432 => Primaries::SMPTE432,
            // #[cfg(not(feature = "ffmpeg_4_3"))]
            ffi::AVCOL_PRI_JEDEC_P22 => Primaries::JEDEC_P22,
            // #[cfg(feature = "ffmpeg_4_3")]
            // ffi::AVCOL_PRI_EBU3213 => Primaries::EBU3213,
            _ => panic!("Unknown primaries"),
        }
    }
}

impl From<Primaries> for ffi::AVColorPrimaries {
    fn from(value: Primaries) -> ffi::AVColorPrimaries {
        match value {
            Primaries::Reserved0 => ffi::AVCOL_PRI_RESERVED0,
            Primaries::BT709 => ffi::AVCOL_PRI_BT709,
            Primaries::Unspecified => ffi::AVCOL_PRI_UNSPECIFIED,
            Primaries::Reserved => ffi::AVCOL_PRI_RESERVED,
            Primaries::BT470M => ffi::AVCOL_PRI_BT470M,

            Primaries::BT470BG => ffi::AVCOL_PRI_BT470BG,
            Primaries::SMPTE170M => ffi::AVCOL_PRI_SMPTE170M,
            Primaries::SMPTE240M => ffi::AVCOL_PRI_SMPTE240M,
            Primaries::Film => ffi::AVCOL_PRI_FILM,
            Primaries::BT2020 => ffi::AVCOL_PRI_BT2020,

            Primaries::SMPTE428 => ffi::AVCOL_PRI_SMPTE428,
            Primaries::SMPTE431 => ffi::AVCOL_PRI_SMPTE431,
            Primaries::SMPTE432 => ffi::AVCOL_PRI_SMPTE432,
            // #[cfg(not(feature = "ffmpeg_4_3"))]
            Primaries::JEDEC_P22 => ffi::AVCOL_PRI_JEDEC_P22,
            // #[cfg(feature = "ffmpeg_4_3")]
            Primaries::EBU3213 => ffi::AVCOL_PRI_EBU3213,
        }
    }
}
