#[macro_use]
pub mod dictionary;
pub mod chroma;
pub mod color;
pub mod error;
pub mod format;
pub mod frame;
pub mod interrupt;
pub mod log;
pub mod mathematics;
pub mod media;
pub mod option;
pub mod picture;
pub mod range;
pub mod rational;
pub mod time;

#[cfg_attr(not(feature = "ffmpeg7"), path = "legacy_channel_layout.rs")]
pub mod channel_layout;

use std::{
    ffi::{CStr, CString, OsStr},
    str::from_utf8_unchecked,
};

use libc::c_int;
use sys::ffi::*;

#[inline(always)]
pub fn version() -> u32 {
    unsafe { avutil_version() }
}

#[inline(always)]
pub fn configuration() -> &'static str {
    unsafe { from_utf8_unchecked(CStr::from_ptr(avutil_configuration()).to_bytes()) }
}

#[inline(always)]
pub fn license() -> &'static str {
    unsafe { from_utf8_unchecked(CStr::from_ptr(avutil_license()).to_bytes()) }
}

#[cfg(unix)]
pub fn from_os_str(path_or_url: impl AsRef<OsStr>) -> CString {
    use std::os::unix::ffi::OsStrExt;
    CString::new(path_or_url.as_ref().as_bytes()).unwrap()
}

#[cfg(not(unix))]
pub fn from_os_str(path_or_url: impl AsRef<OsStr>) -> CString {
    CString::new(path_or_url.as_ref().to_str().unwrap()).unwrap()
}

///////////////////////////////////////////////////////////
//////////////////// Not Change ///////////////////////////
///////////////////////////////////////////////////////////
// Here until https://github.com/rust-lang/rust-bindgen/issues/2192 /
// https://github.com/rust-lang/rust-bindgen/issues/258 is fixed.
//
// The constants here should be kept up to date with libavutil/channel_layout.h.
// #include <libavutil/channel_layout.h>
//
// Audio channel masks
// FFmpeg 5.0 及以上版本 (包含 libavutil 57.28+)
// #if (LIBAVUTIL_VERSION_MAJOR >= 57 && LIBAVUTIL_VERSION_MINOR >= 28) || LIBAVUTIL_VERSION_MAJOR >= 58
pub const AV_CH_FRONT_LEFT: u64 = 1u64 << AV_CHAN_FRONT_LEFT;
pub const AV_CH_FRONT_RIGHT: u64 = 1u64 << AV_CHAN_FRONT_RIGHT;
pub const AV_CH_FRONT_CENTER: u64 = 1u64 << AV_CHAN_FRONT_CENTER;
pub const AV_CH_LOW_FREQUENCY: u64 = 1u64 << AV_CHAN_LOW_FREQUENCY;
pub const AV_CH_BACK_LEFT: u64 = 1u64 << AV_CHAN_BACK_LEFT;
pub const AV_CH_BACK_RIGHT: u64 = 1u64 << AV_CHAN_BACK_RIGHT;
pub const AV_CH_FRONT_LEFT_OF_CENTER: u64 = 1u64 << AV_CHAN_FRONT_LEFT_OF_CENTER;
pub const AV_CH_FRONT_RIGHT_OF_CENTER: u64 = 1u64 << AV_CHAN_FRONT_RIGHT_OF_CENTER;
pub const AV_CH_BACK_CENTER: u64 = 1u64 << AV_CHAN_BACK_CENTER;
pub const AV_CH_SIDE_LEFT: u64 = 1u64 << AV_CHAN_SIDE_LEFT;
pub const AV_CH_SIDE_RIGHT: u64 = 1u64 << AV_CHAN_SIDE_RIGHT;
pub const AV_CH_TOP_CENTER: u64 = 1u64 << AV_CHAN_TOP_CENTER;
pub const AV_CH_TOP_FRONT_LEFT: u64 = 1u64 << AV_CHAN_TOP_FRONT_LEFT;
pub const AV_CH_TOP_FRONT_CENTER: u64 = 1u64 << AV_CHAN_TOP_FRONT_CENTER;
pub const AV_CH_TOP_FRONT_RIGHT: u64 = 1u64 << AV_CHAN_TOP_FRONT_RIGHT;
pub const AV_CH_TOP_BACK_LEFT: u64 = 1u64 << AV_CHAN_TOP_BACK_LEFT;
pub const AV_CH_TOP_BACK_CENTER: u64 = 1u64 << AV_CHAN_TOP_BACK_CENTER;
pub const AV_CH_TOP_BACK_RIGHT: u64 = 1u64 << AV_CHAN_TOP_BACK_RIGHT;
pub const AV_CH_STEREO_LEFT: u64 = 1u64 << AV_CHAN_STEREO_LEFT;
pub const AV_CH_STEREO_RIGHT: u64 = 1u64 << AV_CHAN_STEREO_RIGHT;
pub const AV_CH_WIDE_LEFT: u64 = 1u64 << AV_CHAN_WIDE_LEFT;
pub const AV_CH_WIDE_RIGHT: u64 = 1u64 << AV_CHAN_WIDE_RIGHT;
pub const AV_CH_SURROUND_DIRECT_LEFT: u64 = 1u64 << AV_CHAN_SURROUND_DIRECT_LEFT;
pub const AV_CH_SURROUND_DIRECT_RIGHT: u64 = 1u64 << AV_CHAN_SURROUND_DIRECT_RIGHT;
pub const AV_CH_LOW_FREQUENCY_2: u64 = 1u64 << AV_CHAN_LOW_FREQUENCY_2;
pub const AV_CH_TOP_SIDE_LEFT: u64 = 1u64 << AV_CHAN_TOP_SIDE_LEFT;
pub const AV_CH_TOP_SIDE_RIGHT: u64 = 1u64 << AV_CHAN_TOP_SIDE_RIGHT;
pub const AV_CH_BOTTOM_FRONT_CENTER: u64 = 1u64 << AV_CHAN_BOTTOM_FRONT_CENTER;
pub const AV_CH_BOTTOM_FRONT_LEFT: u64 = 1u64 << AV_CHAN_BOTTOM_FRONT_LEFT;
pub const AV_CH_BOTTOM_FRONT_RIGHT: u64 = 1u64 << AV_CHAN_BOTTOM_FRONT_RIGHT;
#[cfg(feature = "ffmpeg7")]
pub const AV_CH_SIDE_SURROUND_LEFT: u64 = 1u64 << AV_CHAN_SIDE_SURROUND_LEFT;
#[cfg(feature = "ffmpeg7")]
pub const AV_CH_SIDE_SURROUND_RIGHT: u64 = 1u64 << AV_CHAN_SIDE_SURROUND_RIGHT;
#[cfg(feature = "ffmpeg7")]
pub const AV_CH_TOP_SURROUND_LEFT: u64 = 1u64 << AV_CHAN_TOP_SURROUND_LEFT;
#[cfg(feature = "ffmpeg7")]
pub const AV_CH_TOP_SURROUND_RIGHT: u64 = 1u64 << AV_CHAN_TOP_SURROUND_RIGHT;
////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////
// Audio channel layouts
pub const AV_CH_LAYOUT_MONO: u64 = AV_CH_FRONT_CENTER;
pub const AV_CH_LAYOUT_STEREO: u64 = AV_CH_FRONT_LEFT | AV_CH_FRONT_RIGHT;
pub const AV_CH_LAYOUT_2POINT1: u64 = AV_CH_LAYOUT_STEREO | AV_CH_LOW_FREQUENCY;
pub const AV_CH_LAYOUT_2_1: u64 = AV_CH_LAYOUT_STEREO | AV_CH_BACK_CENTER;
pub const AV_CH_LAYOUT_SURROUND: u64 = AV_CH_LAYOUT_STEREO | AV_CH_FRONT_CENTER;
pub const AV_CH_LAYOUT_3POINT1: u64 = AV_CH_LAYOUT_SURROUND | AV_CH_LOW_FREQUENCY;
pub const AV_CH_LAYOUT_4POINT0: u64 = AV_CH_LAYOUT_SURROUND | AV_CH_BACK_CENTER;
pub const AV_CH_LAYOUT_4POINT1: u64 = AV_CH_LAYOUT_4POINT0 | AV_CH_LOW_FREQUENCY;
pub const AV_CH_LAYOUT_2_2: u64 = AV_CH_LAYOUT_STEREO | AV_CH_SIDE_LEFT | AV_CH_SIDE_RIGHT;
pub const AV_CH_LAYOUT_QUAD: u64 = AV_CH_LAYOUT_STEREO | AV_CH_BACK_LEFT | AV_CH_BACK_RIGHT;
pub const AV_CH_LAYOUT_5POINT0: u64 = AV_CH_LAYOUT_SURROUND | AV_CH_SIDE_LEFT | AV_CH_SIDE_RIGHT;
pub const AV_CH_LAYOUT_5POINT1: u64 = AV_CH_LAYOUT_5POINT0 | AV_CH_LOW_FREQUENCY;
pub const AV_CH_LAYOUT_5POINT0_BACK: u64 =
    AV_CH_LAYOUT_SURROUND | AV_CH_BACK_LEFT | AV_CH_BACK_RIGHT;
pub const AV_CH_LAYOUT_5POINT1_BACK: u64 = AV_CH_LAYOUT_5POINT0_BACK | AV_CH_LOW_FREQUENCY;
pub const AV_CH_LAYOUT_6POINT0: u64 = AV_CH_LAYOUT_5POINT0 | AV_CH_BACK_CENTER;
pub const AV_CH_LAYOUT_6POINT0_FRONT: u64 =
    AV_CH_LAYOUT_2_2 | AV_CH_FRONT_LEFT_OF_CENTER | AV_CH_FRONT_RIGHT_OF_CENTER;
pub const AV_CH_LAYOUT_HEXAGONAL: u64 = AV_CH_LAYOUT_5POINT0_BACK | AV_CH_BACK_CENTER;
pub const AV_CH_LAYOUT_3POINT1POINT2: u64 =
    AV_CH_LAYOUT_3POINT1 | AV_CH_TOP_FRONT_LEFT | AV_CH_TOP_FRONT_RIGHT;
pub const AV_CH_LAYOUT_6POINT1: u64 = AV_CH_LAYOUT_5POINT1 | AV_CH_BACK_CENTER;
pub const AV_CH_LAYOUT_6POINT1_BACK: u64 = AV_CH_LAYOUT_5POINT1_BACK | AV_CH_BACK_CENTER;
pub const AV_CH_LAYOUT_6POINT1_FRONT: u64 = AV_CH_LAYOUT_6POINT0_FRONT | AV_CH_LOW_FREQUENCY;
pub const AV_CH_LAYOUT_7POINT0: u64 = AV_CH_LAYOUT_5POINT0 | AV_CH_BACK_LEFT | AV_CH_BACK_RIGHT;
pub const AV_CH_LAYOUT_7POINT0_FRONT: u64 =
    AV_CH_LAYOUT_5POINT0 | AV_CH_FRONT_LEFT_OF_CENTER | AV_CH_FRONT_RIGHT_OF_CENTER;
pub const AV_CH_LAYOUT_7POINT1: u64 = AV_CH_LAYOUT_5POINT1 | AV_CH_BACK_LEFT | AV_CH_BACK_RIGHT;
pub const AV_CH_LAYOUT_7POINT1_WIDE: u64 =
    AV_CH_LAYOUT_5POINT1 | AV_CH_FRONT_LEFT_OF_CENTER | AV_CH_FRONT_RIGHT_OF_CENTER;
pub const AV_CH_LAYOUT_7POINT1_WIDE_BACK: u64 =
    AV_CH_LAYOUT_5POINT1_BACK | AV_CH_FRONT_LEFT_OF_CENTER | AV_CH_FRONT_RIGHT_OF_CENTER;
pub const AV_CH_LAYOUT_5POINT1POINT2_BACK: u64 =
    AV_CH_LAYOUT_5POINT1_BACK | AV_CH_TOP_FRONT_LEFT | AV_CH_TOP_FRONT_RIGHT;
pub const AV_CH_LAYOUT_OCTAGONAL: u64 =
    AV_CH_LAYOUT_5POINT0 | AV_CH_BACK_LEFT | AV_CH_BACK_CENTER | AV_CH_BACK_RIGHT;
pub const AV_CH_LAYOUT_CUBE: u64 = AV_CH_LAYOUT_QUAD
    | AV_CH_TOP_FRONT_LEFT
    | AV_CH_TOP_FRONT_RIGHT
    | AV_CH_TOP_BACK_LEFT
    | AV_CH_TOP_BACK_RIGHT;
pub const AV_CH_LAYOUT_5POINT1POINT4_BACK: u64 =
    AV_CH_LAYOUT_5POINT1POINT2_BACK | AV_CH_TOP_BACK_LEFT | AV_CH_TOP_BACK_RIGHT;
pub const AV_CH_LAYOUT_7POINT1POINT2: u64 =
    AV_CH_LAYOUT_7POINT1 | AV_CH_TOP_FRONT_LEFT | AV_CH_TOP_FRONT_RIGHT;
pub const AV_CH_LAYOUT_7POINT1POINT4_BACK: u64 =
    AV_CH_LAYOUT_7POINT1POINT2 | AV_CH_TOP_BACK_LEFT | AV_CH_TOP_BACK_RIGHT;
#[cfg(feature = "ffmpeg7")]
pub const AV_CH_LAYOUT_7POINT2POINT3: u64 =
    AV_CH_LAYOUT_7POINT1POINT2 | AV_CH_TOP_BACK_CENTER | AV_CH_LOW_FREQUENCY_2;
#[cfg(feature = "ffmpeg7")]
pub const AV_CH_LAYOUT_9POINT1POINT4_BACK: u64 =
    AV_CH_LAYOUT_7POINT1POINT4_BACK | AV_CH_FRONT_LEFT_OF_CENTER | AV_CH_FRONT_RIGHT_OF_CENTER;
pub const AV_CH_LAYOUT_HEXADECAGONAL: u64 = AV_CH_LAYOUT_OCTAGONAL
    | AV_CH_WIDE_LEFT
    | AV_CH_WIDE_RIGHT
    | AV_CH_TOP_BACK_LEFT
    | AV_CH_TOP_BACK_RIGHT
    | AV_CH_TOP_BACK_CENTER
    | AV_CH_TOP_FRONT_CENTER
    | AV_CH_TOP_FRONT_LEFT
    | AV_CH_TOP_FRONT_RIGHT;
pub const AV_CH_LAYOUT_STEREO_DOWNMIX: u64 = AV_CH_STEREO_LEFT | AV_CH_STEREO_RIGHT;
pub const AV_CH_LAYOUT_22POINT2: u64 = AV_CH_LAYOUT_5POINT1_BACK
    | AV_CH_FRONT_LEFT_OF_CENTER
    | AV_CH_FRONT_RIGHT_OF_CENTER
    | AV_CH_BACK_CENTER
    | AV_CH_LOW_FREQUENCY_2
    | AV_CH_SIDE_LEFT
    | AV_CH_SIDE_RIGHT
    | AV_CH_TOP_FRONT_LEFT
    | AV_CH_TOP_FRONT_RIGHT
    | AV_CH_TOP_FRONT_CENTER
    | AV_CH_TOP_CENTER
    | AV_CH_TOP_BACK_LEFT
    | AV_CH_TOP_BACK_RIGHT
    | AV_CH_TOP_SIDE_LEFT
    | AV_CH_TOP_SIDE_RIGHT
    | AV_CH_TOP_BACK_CENTER
    | AV_CH_BOTTOM_FRONT_CENTER
    | AV_CH_BOTTOM_FRONT_LEFT
    | AV_CH_BOTTOM_FRONT_RIGHT;

pub const AV_CH_LAYOUT_7POINT1_TOP_BACK: u64 = AV_CH_LAYOUT_5POINT1POINT2_BACK;

// Audio channel layouts as AVChannelLayout
pub const fn av_channel_layout_mask(nb_channels: c_int, channel_mask: u64) -> AVChannelLayout {
    AVChannelLayout {
        order: AV_CHANNEL_ORDER_NATIVE,
        nb_channels,
        u: AVChannelLayout__bindgen_ty_1 { mask: channel_mask },
        opaque: std::ptr::null_mut(),
    }
}

pub const AV_CHANNEL_LAYOUT_MONO: AVChannelLayout = av_channel_layout_mask(1, AV_CH_LAYOUT_MONO);
pub const AV_CHANNEL_LAYOUT_STEREO: AVChannelLayout =
    av_channel_layout_mask(2, AV_CH_LAYOUT_STEREO);
pub const AV_CHANNEL_LAYOUT_2POINT1: AVChannelLayout =
    av_channel_layout_mask(3, AV_CH_LAYOUT_2POINT1);
pub const AV_CHANNEL_LAYOUT_2_1: AVChannelLayout = av_channel_layout_mask(3, AV_CH_LAYOUT_2_1);
pub const AV_CHANNEL_LAYOUT_SURROUND: AVChannelLayout =
    av_channel_layout_mask(3, AV_CH_LAYOUT_SURROUND);
pub const AV_CHANNEL_LAYOUT_3POINT1: AVChannelLayout =
    av_channel_layout_mask(4, AV_CH_LAYOUT_3POINT1);
pub const AV_CHANNEL_LAYOUT_4POINT0: AVChannelLayout =
    av_channel_layout_mask(4, AV_CH_LAYOUT_4POINT0);
pub const AV_CHANNEL_LAYOUT_4POINT1: AVChannelLayout =
    av_channel_layout_mask(5, AV_CH_LAYOUT_4POINT1);
pub const AV_CHANNEL_LAYOUT_2_2: AVChannelLayout = av_channel_layout_mask(4, AV_CH_LAYOUT_2_2);
pub const AV_CHANNEL_LAYOUT_QUAD: AVChannelLayout = av_channel_layout_mask(4, AV_CH_LAYOUT_QUAD);
pub const AV_CHANNEL_LAYOUT_5POINT0: AVChannelLayout =
    av_channel_layout_mask(5, AV_CH_LAYOUT_5POINT0);
pub const AV_CHANNEL_LAYOUT_5POINT1: AVChannelLayout =
    av_channel_layout_mask(6, AV_CH_LAYOUT_5POINT1);
pub const AV_CHANNEL_LAYOUT_5POINT0_BACK: AVChannelLayout =
    av_channel_layout_mask(5, AV_CH_LAYOUT_5POINT0_BACK);
pub const AV_CHANNEL_LAYOUT_5POINT1_BACK: AVChannelLayout =
    av_channel_layout_mask(6, AV_CH_LAYOUT_5POINT1_BACK);
pub const AV_CHANNEL_LAYOUT_6POINT0: AVChannelLayout =
    av_channel_layout_mask(6, AV_CH_LAYOUT_6POINT0);
pub const AV_CHANNEL_LAYOUT_6POINT0_FRONT: AVChannelLayout =
    av_channel_layout_mask(6, AV_CH_LAYOUT_6POINT0_FRONT);
pub const AV_CHANNEL_LAYOUT_3POINT1POINT2: AVChannelLayout =
    av_channel_layout_mask(6, AV_CH_LAYOUT_3POINT1POINT2);
pub const AV_CHANNEL_LAYOUT_HEXAGONAL: AVChannelLayout =
    av_channel_layout_mask(6, AV_CH_LAYOUT_HEXAGONAL);
pub const AV_CHANNEL_LAYOUT_6POINT1: AVChannelLayout =
    av_channel_layout_mask(7, AV_CH_LAYOUT_6POINT1);
pub const AV_CHANNEL_LAYOUT_6POINT1_BACK: AVChannelLayout =
    av_channel_layout_mask(7, AV_CH_LAYOUT_6POINT1_BACK);
pub const AV_CHANNEL_LAYOUT_6POINT1_FRONT: AVChannelLayout =
    av_channel_layout_mask(7, AV_CH_LAYOUT_6POINT1_FRONT);
pub const AV_CHANNEL_LAYOUT_7POINT0: AVChannelLayout =
    av_channel_layout_mask(7, AV_CH_LAYOUT_7POINT0);
pub const AV_CHANNEL_LAYOUT_7POINT0_FRONT: AVChannelLayout =
    av_channel_layout_mask(7, AV_CH_LAYOUT_7POINT0_FRONT);
pub const AV_CHANNEL_LAYOUT_7POINT1: AVChannelLayout =
    av_channel_layout_mask(8, AV_CH_LAYOUT_7POINT1);
pub const AV_CHANNEL_LAYOUT_7POINT1_WIDE: AVChannelLayout =
    av_channel_layout_mask(8, AV_CH_LAYOUT_7POINT1_WIDE);
pub const AV_CHANNEL_LAYOUT_7POINT1_WIDE_BACK: AVChannelLayout =
    av_channel_layout_mask(8, AV_CH_LAYOUT_7POINT1_WIDE_BACK);
pub const AV_CHANNEL_LAYOUT_5POINT1POINT2_BACK: AVChannelLayout =
    av_channel_layout_mask(8, AV_CH_LAYOUT_5POINT1POINT2_BACK);
pub const AV_CHANNEL_LAYOUT_OCTAGONAL: AVChannelLayout =
    av_channel_layout_mask(8, AV_CH_LAYOUT_OCTAGONAL);
pub const AV_CHANNEL_LAYOUT_CUBE: AVChannelLayout = av_channel_layout_mask(8, AV_CH_LAYOUT_CUBE);
pub const AV_CHANNEL_LAYOUT_5POINT1POINT4_BACK: AVChannelLayout =
    av_channel_layout_mask(10, AV_CH_LAYOUT_5POINT1POINT4_BACK);
pub const AV_CHANNEL_LAYOUT_7POINT1POINT2: AVChannelLayout =
    av_channel_layout_mask(10, AV_CH_LAYOUT_7POINT1POINT2);
pub const AV_CHANNEL_LAYOUT_7POINT1POINT4_BACK: AVChannelLayout =
    av_channel_layout_mask(12, AV_CH_LAYOUT_7POINT1POINT4_BACK);
#[cfg(feature = "ffmpeg7")]
pub const AV_CHANNEL_LAYOUT_7POINT2POINT3: AVChannelLayout =
    av_channel_layout_mask(12, AV_CH_LAYOUT_7POINT2POINT3);
#[cfg(feature = "ffmpeg7")]
pub const AV_CHANNEL_LAYOUT_9POINT1POINT4_BACK: AVChannelLayout =
    av_channel_layout_mask(14, AV_CH_LAYOUT_9POINT1POINT4_BACK);
pub const AV_CHANNEL_LAYOUT_HEXADECAGONAL: AVChannelLayout =
    av_channel_layout_mask(16, AV_CH_LAYOUT_HEXADECAGONAL);
pub const AV_CHANNEL_LAYOUT_STEREO_DOWNMIX: AVChannelLayout =
    av_channel_layout_mask(2, AV_CH_LAYOUT_STEREO_DOWNMIX);
pub const AV_CHANNEL_LAYOUT_22POINT2: AVChannelLayout =
    av_channel_layout_mask(24, AV_CH_LAYOUT_22POINT2);

pub const AV_CHANNEL_LAYOUT_7POINT1_TOP_BACK: AVChannelLayout =
    AV_CHANNEL_LAYOUT_5POINT1POINT2_BACK;
