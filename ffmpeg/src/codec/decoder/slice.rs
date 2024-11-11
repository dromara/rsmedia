use rsmpeg::ffi;
use libc::c_int;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Flags: c_int {
        const CODED_ORDER = ffi::SLICE_FLAG_CODED_ORDER as i32;
        const ALLOW_FIELD = ffi::SLICE_FLAG_ALLOW_FIELD as i32;
        const ALLOW_PLANE = ffi::SLICE_FLAG_ALLOW_PLANE as i32;
    }
}
