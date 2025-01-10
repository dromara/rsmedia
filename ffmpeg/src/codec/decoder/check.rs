use libc::c_int;
use sys::ffi;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Check: c_int {
        const CRC      = ffi::AV_EF_CRCCHECK as i32;
        const BISTREAM = ffi::AV_EF_BITSTREAM as i32;
        const BUFFER   = ffi::AV_EF_BUFFER as i32;
        const EXPLODE  = ffi::AV_EF_EXPLODE as i32;

        const IGNORE_ERROR = ffi::AV_EF_IGNORE_ERR as i32;
        const CAREFUL      = ffi::AV_EF_CAREFUL as i32;
        const COMPLIANT    = ffi::AV_EF_COMPLIANT as i32;
        const AGGRESSIVE   = ffi::AV_EF_AGGRESSIVE as i32;
    }
}
