use crate::Error;
use rsmpeg::ffi;

#[inline(always)]
pub fn current() -> i64 {
    unsafe { ffi::av_gettime() }
}

#[inline(always)]
pub fn relative() -> i64 {
    unsafe { ffi::av_gettime_relative() }
}

#[inline(always)]
pub fn is_monotonic() -> bool {
    unsafe { ffi::av_gettime_relative_is_monotonic() != 0 }
}

#[inline(always)]
pub fn sleep(usec: u32) -> Result<(), Error> {
    unsafe {
        match ffi::av_usleep(usec) {
            0 => Ok(()),
            e => Err(Error::from(e)),
        }
    }
}
