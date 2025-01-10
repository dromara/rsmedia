use libc::c_int;
use sys::ffi;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Flags: c_int {
        const CORRUPT = ffi::AV_FRAME_FLAG_CORRUPT as i32;
    }
}
