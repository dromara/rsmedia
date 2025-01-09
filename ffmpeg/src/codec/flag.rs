use libc::c_uint;
use rsmpeg::ffi;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Flags: c_uint {
        const UNALIGNED       = ffi::AV_CODEC_FLAG_UNALIGNED;
        const QSCALE          = ffi::AV_CODEC_FLAG_QSCALE;
        const _4MV            = ffi::AV_CODEC_FLAG_4MV;
        const OUTPUT_CORRUPT  = ffi::AV_CODEC_FLAG_OUTPUT_CORRUPT;
        const QPEL            = ffi::AV_CODEC_FLAG_QPEL;
        const PASS1           = ffi::AV_CODEC_FLAG_PASS1;
        const PASS2           = ffi::AV_CODEC_FLAG_PASS2;
        const LOOP_FILTER     = ffi::AV_CODEC_FLAG_LOOP_FILTER;
        const GRAY            = ffi::AV_CODEC_FLAG_GRAY;
        const PSNR            = ffi::AV_CODEC_FLAG_PSNR;
        // #[cfg(not(feature = "ffmpeg6"))]
        // const TRUNCATED       = ffi::AV_CODEC_FLAG_TRUNCATED;
        const INTERLACED_DCT  = ffi::AV_CODEC_FLAG_INTERLACED_DCT;
        const LOW_DELAY       = ffi::AV_CODEC_FLAG_LOW_DELAY;
        const GLOBAL_HEADER   = ffi::AV_CODEC_FLAG_GLOBAL_HEADER;
        const BITEXACT        = ffi::AV_CODEC_FLAG_BITEXACT;
        const AC_PRED         = ffi::AV_CODEC_FLAG_AC_PRED;
        const INTERLACED_ME   = ffi::AV_CODEC_FLAG_INTERLACED_ME;
        const CLOSED_GOP      = ffi::AV_CODEC_FLAG_CLOSED_GOP;
    }
}
