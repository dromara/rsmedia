use libc::c_int;
use rsmpeg::ffi;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Flags: c_int {
        const FORCED = ffi::AV_SUBTITLE_FLAG_FORCED as i32;
    }
}
