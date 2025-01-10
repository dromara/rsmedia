use libc::c_int;
use sys::ffi;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum ColorSpace {
    Default,

    ITU709,
    FCC,
    ITU601,
    ITU624,
    SMPTE170M,
    SMPTE240M,
}

impl From<c_int> for ColorSpace {
    fn from(value: c_int) -> ColorSpace {
        match value as u32 {
            ffi::SWS_CS_ITU709 => ColorSpace::ITU709,
            ffi::SWS_CS_FCC => ColorSpace::FCC,
            ffi::SWS_CS_DEFAULT => ColorSpace::Default,
            ffi::SWS_CS_SMPTE240M => ColorSpace::SMPTE240M,

            _ => ColorSpace::Default,
        }
    }
}

impl From<ColorSpace> for c_int {
    fn from(value: ColorSpace) -> c_int {
        match value {
            ColorSpace::Default => ffi::SWS_CS_DEFAULT as i32,
            ColorSpace::ITU709 => ffi::SWS_CS_ITU709 as i32,
            ColorSpace::FCC => ffi::SWS_CS_FCC as i32,
            ColorSpace::ITU601 => ffi::SWS_CS_ITU601 as i32,
            ColorSpace::ITU624 => ffi::SWS_CS_ITU624 as i32,
            ColorSpace::SMPTE170M => ffi::SWS_CS_SMPTE170M as i32,
            ColorSpace::SMPTE240M => ffi::SWS_CS_SMPTE240M as i32,
        }
    }
}
