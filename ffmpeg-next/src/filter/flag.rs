use rsmpeg::ffi;
use libc::c_int;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Flags: c_int {
        const DYNAMIC_INPUTS            = ffi::AVFILTER_FLAG_DYNAMIC_INPUTS as i32;
        const DYNAMIC_OUTPUTS           = ffi::AVFILTER_FLAG_DYNAMIC_OUTPUTS as i32;
        const SLICE_THREADS             = ffi::AVFILTER_FLAG_SLICE_THREADS as i32;
        const SUPPORT_TIMELINE_GENERIC  = ffi::AVFILTER_FLAG_SUPPORT_TIMELINE_GENERIC as i32;
        const SUPPORT_TIMELINE_INTERNAL = ffi::AVFILTER_FLAG_SUPPORT_TIMELINE_INTERNAL as i32;
        const SUPPORT_TIMELINE          = ffi::AVFILTER_FLAG_SUPPORT_TIMELINE as i32;
    }
}
