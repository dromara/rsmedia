use super::Context;
use rsmpeg::ffi;

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub struct Delay {
    pub seconds: i64,
    pub milliseconds: i64,
    pub input: i64,
    pub output: i64,
}

impl Delay {
    pub fn from(context: &Context) -> Self {
        unsafe {
            Delay {
                seconds: ffi::swr_get_delay(context.as_ptr() as *mut _, 1),
                milliseconds: ffi::swr_get_delay(context.as_ptr() as *mut _, 1000),
                input: ffi::swr_get_delay(context.as_ptr() as *mut _, i64::from(context.input().rate)),
                output: ffi::swr_get_delay(context.as_ptr() as *mut _, i64::from(context.output().rate)),
            }
        }
    }
}
