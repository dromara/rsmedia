use libc::c_uint;
use rsmpeg::ffi;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Capabilities: c_uint {
        const DRAW_HORIZ_BAND               = ffi::AV_CODEC_CAP_DRAW_HORIZ_BAND;
        const DR1                           = ffi::AV_CODEC_CAP_DR1;
        const DELAY                         = ffi::AV_CODEC_CAP_DELAY;
        const SMALL_LAST_FRAME              = ffi::AV_CODEC_CAP_SMALL_LAST_FRAME;
        const SUBFRAMES                     = ffi::AV_CODEC_CAP_SUBFRAMES;
        const EXPERIMENTAL                  = ffi::AV_CODEC_CAP_EXPERIMENTAL;
        const CHANNEL_CONF                  = ffi::AV_CODEC_CAP_CHANNEL_CONF;
        const FRAME_THREADS                 = ffi::AV_CODEC_CAP_FRAME_THREADS;
        const SLICE_THREADS                 = ffi::AV_CODEC_CAP_SLICE_THREADS;
        const PARAM_CHANGE                  = ffi::AV_CODEC_CAP_PARAM_CHANGE;
        const OTHER_THREADS                 = ffi::AV_CODEC_CAP_OTHER_THREADS;
        const VARIABLE_FRAME_SIZE           = ffi::AV_CODEC_CAP_VARIABLE_FRAME_SIZE;
        const AVOID_PROBING                 = ffi::AV_CODEC_CAP_AVOID_PROBING;
        const HARDWARE                      = ffi::AV_CODEC_CAP_HARDWARE;
        const HYBRID                        = ffi::AV_CODEC_CAP_HYBRID;
        const ENCODER_REORDERED_OPAQUE      = ffi::AV_CODEC_CAP_ENCODER_REORDERED_OPAQUE;
        const ENCODER_FLUSH                 = ffi::AV_CODEC_CAP_ENCODER_FLUSH;
        const ENCODER_RECON_FRAME           = ffi::AV_CODEC_CAP_ENCODER_RECON_FRAME;
    }
}
