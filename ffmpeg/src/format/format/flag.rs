use libc::c_int;
use rsmpeg::ffi;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Flags: c_int {
        const NO_FILE       = ffi::AVFMT_NOFILE as i32;
        const NEED_NUMBER   = ffi::AVFMT_NEEDNUMBER as i32;
        const SHOW_IDS      = ffi::AVFMT_SHOW_IDS as i32;
        // #[cfg(not(feature = "ffmpeg_4_0"))]
        // const RAW_PICTURE   = ffi::AVFMT_RAWPICTURE;
        const GLOBAL_HEADER = ffi::AVFMT_GLOBALHEADER as i32;
        const NO_TIMESTAMPS = ffi::AVFMT_NOTIMESTAMPS as i32;
        const GENERIC_INDEX = ffi::AVFMT_GENERIC_INDEX as i32;
        const TS_DISCONT    = ffi::AVFMT_TS_DISCONT as i32;
        const VARIABLE_FPS  = ffi::AVFMT_VARIABLE_FPS as i32;
        const NO_DIMENSIONS = ffi::AVFMT_NODIMENSIONS as i32;
        const NO_STREAMS    = ffi::AVFMT_NOSTREAMS as i32;
        const NO_BINSEARCH  = ffi::AVFMT_NOBINSEARCH as i32;
        const NO_GENSEARCH  = ffi::AVFMT_NOGENSEARCH as i32;
        const NO_BYTE_SEEK  = ffi::AVFMT_NO_BYTE_SEEK as i32;
        const ALLOW_FLUSH   = ffi::AVFMT_ALLOW_FLUSH as i32;
        const TS_NONSTRICT  = ffi::AVFMT_TS_NONSTRICT as i32;
        const TS_NEGATIVE   = ffi::AVFMT_TS_NEGATIVE as i32;
        const SEEK_TO_PTS   = ffi::AVFMT_SEEK_TO_PTS as i32;
    }
}
