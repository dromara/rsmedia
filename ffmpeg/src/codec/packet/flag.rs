use libc::c_int;
use sys::ffi;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Flags: c_int {
        const KEY     = ffi::AV_PKT_FLAG_KEY as i32;
        const CORRUPT = ffi::AV_PKT_FLAG_CORRUPT as i32;
    }
}
