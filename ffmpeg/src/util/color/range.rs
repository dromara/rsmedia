use std::ffi::CStr;
use std::str::from_utf8_unchecked;

use rsmpeg::ffi;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Range {
    Unspecified,
    MPEG,
    JPEG,
}

impl Range {
    pub fn name(&self) -> Option<&'static str> {
        if *self == Range::Unspecified {
            return None;
        }
        unsafe {
            let ptr = ffi::av_color_range_name((*self).into());
            ptr.as_ref()
                .map(|ptr| from_utf8_unchecked(CStr::from_ptr(ptr).to_bytes()))
        }
    }
}

impl From<ffi::AVColorRange> for Range {
    fn from(value: ffi::AVColorRange) -> Self {
        match value {
            ffi::AVCOL_RANGE_UNSPECIFIED => Range::Unspecified,
            ffi::AVCOL_RANGE_MPEG => Range::MPEG,
            ffi::AVCOL_RANGE_JPEG => Range::JPEG,
            ffi::AVCOL_RANGE_NB => Range::Unspecified,
        }
    }
}

impl From<Range> for ffi::AVColorRange {
    fn from(value: Range) -> ffi::AVColorRange {
        match value {
            Range::Unspecified => ffi::AVCOL_RANGE_UNSPECIFIED,
            Range::MPEG => ffi::AVCOL_RANGE_MPEG,
            Range::JPEG => ffi::AVCOL_RANGE_JPEG,
        }
    }
}
