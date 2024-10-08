use libc::c_uint;
use rsmpeg::ffi;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Capabilities: c_uint {
        const DRAW_HORIZ_BAND               = ffi::AV_CODEC_CAP_DRAW_HORIZ_BAND;
        const DR1                           = ffi::AV_CODEC_CAP_DR1;
        // #[cfg(not(feature = "ffmpeg_6_0"))]
        // const TRUNCATED                     = ffi::AV_CODEC_CAP_TRUNCATED;
        const DELAY                         = ffi::AV_CODEC_CAP_DELAY;
        const SMALL_LAST_FRAME              = ffi::AV_CODEC_CAP_SMALL_LAST_FRAME;
        // #[cfg(not(feature = "ffmpeg_4_0"))]
        // const HWACCEL_VDPAU                 = ffi::AV_CODEC_CAP_HWACCEL_VDPAU;
        const SUBFRAMES                     = ffi::AV_CODEC_CAP_SUBFRAMES;
        const EXPERIMENTAL                  = ffi::AV_CODEC_CAP_EXPERIMENTAL;
        const CHANNEL_CONF                  = ffi::AV_CODEC_CAP_CHANNEL_CONF;
        const FRAME_THREADS                 = ffi::AV_CODEC_CAP_FRAME_THREADS;
        const SLICE_THREADS                 = ffi::AV_CODEC_CAP_SLICE_THREADS;
        const PARAM_CHANGE                  = ffi::AV_CODEC_CAP_PARAM_CHANGE;
        // #[cfg(not(feature = "ffmpeg_6_0"))]
        // const AUTO_THREADS                  = ffi::AV_CODEC_CAP_AUTO_THREADS;
        // #[cfg(feature = "ffmpeg_6_0")]
        const OTHER_THREADS                 = ffi::AV_CODEC_CAP_OTHER_THREADS;
        const VARIABLE_FRAME_SIZE           = ffi::AV_CODEC_CAP_VARIABLE_FRAME_SIZE;
        // #[cfg(not(feature = "ffmpeg_6_0"))]
        // const INTRA_ONLY                    = ffi::AV_CODEC_CAP_INTRA_ONLY;
        // #[cfg(not(feature = "ffmpeg_6_0"))]
        // const LOSSLESS                      = ffi::AV_CODEC_CAP_LOSSLESS;
        const AVOID_PROBING                 = ffi::AV_CODEC_CAP_AVOID_PROBING;
        const HARDWARE                      = ffi::AV_CODEC_CAP_HARDWARE;
        const HYBRID                        = ffi::AV_CODEC_CAP_HYBRID;
        const ENCODER_REORDERED_OPAQUE      = ffi::AV_CODEC_CAP_ENCODER_REORDERED_OPAQUE;
        const ENCODER_FLUSH                 = ffi::AV_CODEC_CAP_ENCODER_FLUSH;
        const ENCODER_RECON_FRAME           = ffi::AV_CODEC_CAP_ENCODER_RECON_FRAME;
    }
}
