use libc::c_int;
use rsmpeg::ffi;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Flags: c_int {
        const SKIP_REPEATED = ffi::AV_LOG_SKIP_REPEATED as i32;
        const PRINT_LEVEL = ffi::AV_LOG_PRINT_LEVEL as i32;
    }
}
