use libc::c_int;
use rsmpeg::ffi;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Flags: c_int {
        const FORCE = ffi::SWR_FLAG_RESAMPLE as i32;
    }
}
