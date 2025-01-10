use libc::c_int;
use sys::ffi;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Flags: c_int {
        const FAST_BILINEAR        = ffi::SWS_FAST_BILINEAR as i32;
        const BILINEAR             = ffi::SWS_BILINEAR as i32;
        const BICUBIC              = ffi::SWS_BICUBIC as i32;
        const X                    = ffi::SWS_X as i32;
        const POINT                = ffi::SWS_POINT as i32;
        const AREA                 = ffi::SWS_AREA as i32;
        const BICUBLIN             = ffi::SWS_BICUBLIN as i32;
        const GAUSS                = ffi::SWS_GAUSS as i32;
        const SINC                 = ffi::SWS_SINC as i32;
        const LANCZOS              = ffi::SWS_LANCZOS as i32;
        const SPLINE               = ffi::SWS_SPLINE as i32;
        const SRC_V_CHR_DROP_MASK  = ffi::SWS_SRC_V_CHR_DROP_MASK as i32;
        const SRC_V_CHR_DROP_SHIFT = ffi::SWS_SRC_V_CHR_DROP_SHIFT as i32;
        const PARAM_DEFAULT        = ffi::SWS_PARAM_DEFAULT as i32;
        const PRINT_INFO           = ffi::SWS_PRINT_INFO as i32;
        const FULL_CHR_H_INT       = ffi::SWS_FULL_CHR_H_INT as i32;
        const FULL_CHR_H_INP       = ffi::SWS_FULL_CHR_H_INP as i32;
        const DIRECT_BGR           = ffi::SWS_DIRECT_BGR as i32;
        const ACCURATE_RND         = ffi::SWS_ACCURATE_RND as i32;
        const BITEXACT             = ffi::SWS_BITEXACT as i32;
        const ERROR_DIFFUSION      = ffi::SWS_ERROR_DIFFUSION as i32;
    }
}
