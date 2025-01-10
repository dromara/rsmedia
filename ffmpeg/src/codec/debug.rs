use libc::c_int;
use sys::ffi;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Debug: c_int {
        const PICT_INFO   = ffi::FF_DEBUG_PICT_INFO as i32;
        const RC          = ffi::FF_DEBUG_RC as i32;
        const BITSTREAM   = ffi::FF_DEBUG_BITSTREAM as i32;
        const MB_TYPE     = ffi::FF_DEBUG_MB_TYPE as i32;
        const QP          = ffi::FF_DEBUG_QP as i32;
        // #[cfg(not(feature = "ffmpeg_4_0"))]
        // const MV          = ffi::FF_DEBUG_MV;
        const DCT_COEFF   = ffi::FF_DEBUG_DCT_COEFF as i32;
        const SKIP        = ffi::FF_DEBUG_SKIP as i32;
        const STARTCODE   = ffi::FF_DEBUG_STARTCODE as i32;
        // #[cfg(not(feature = "ffmpeg_4_0"))]
        // const PTS         = ffi::FF_DEBUG_PTS;
        const ER          = ffi::FF_DEBUG_ER as i32;
        const MMCO        = ffi::FF_DEBUG_MMCO as i32;
        const BUGS        = ffi::FF_DEBUG_BUGS as i32;
        // #[cfg(not(feature = "ffmpeg_4_0"))]
        // const VIS_QP      = ffi::FF_DEBUG_VIS_QP;
        // #[cfg(not(feature = "ffmpeg_4_0"))]
        // const VIS_MB_TYPE = ffi::FF_DEBUG_VIS_MB_TYPE;
        const BUFFERS     = ffi::FF_DEBUG_BUFFERS as i32;
        const THREADS     = ffi::FF_DEBUG_THREADS as i32;
        const GREEN_MD     = ffi::FF_DEBUG_GREEN_MD as i32;
        const NOMC        = ffi::FF_DEBUG_NOMC as i32;
    }
}
