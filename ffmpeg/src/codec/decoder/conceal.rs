use libc::c_int;
use rsmpeg::ffi;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Conceal: c_int {
        const GUESS_MVS   = ffi::FF_EC_GUESS_MVS as i32;
        const DEBLOCK     = ffi::FF_EC_DEBLOCK as i32;
        const FAVOR_INTER = ffi::FF_EC_FAVOR_INTER as i32;
    }
}
