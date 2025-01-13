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

use std::ffi::CStr;
use std::str::from_utf8_unchecked;

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

///////////////////////////////////////////////////////////
//////////////////// Not Change ///////////////////////////
///////////////////////////////////////////////////////////
// #include <libavutil/channel_layout.h>
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
/////////////////////////////////////////////////////////////////////
/////////////////////////////////////////////////////////////////////

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
pub const AV_CH_LAYOUT_OCTAGONAL: u64 =
    AV_CH_LAYOUT_5POINT0 | AV_CH_BACK_LEFT | AV_CH_BACK_CENTER | AV_CH_BACK_RIGHT;
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
pub const AV_CH_LAYOUT_CUBE: u64 = AV_CH_LAYOUT_QUAD
    | AV_CH_TOP_FRONT_LEFT
    | AV_CH_TOP_FRONT_RIGHT
    | AV_CH_TOP_BACK_LEFT
    | AV_CH_TOP_BACK_RIGHT;
/////////////////////////////////////////////////////////////////////
/////////////////////////////////////////////////////////////////////

/////////////////////////////////////////////////////////////////////
// FFmpeg 6.0 及以上版本 (包含 libavutil 58.x)
// #if (LIBAVUTIL_VERSION_MAJOR >= 58 && LIBAVUTIL_VERSION_MINOR >= 29) || LIBAVUTIL_VERSION_MAJOR >= 59
pub const AV_CH_LAYOUT_3POINT1POINT2: u64 =
    AV_CH_LAYOUT_3POINT1 | AV_CH_TOP_FRONT_LEFT | AV_CH_TOP_FRONT_RIGHT;
pub const AV_CH_LAYOUT_5POINT1POINT2_BACK: u64 =
    AV_CH_LAYOUT_5POINT1_BACK | AV_CH_TOP_FRONT_LEFT | AV_CH_TOP_FRONT_RIGHT;
pub const AV_CH_LAYOUT_5POINT1POINT4_BACK: u64 =
    AV_CH_LAYOUT_5POINT1POINT2_BACK | AV_CH_TOP_BACK_LEFT | AV_CH_TOP_BACK_RIGHT;
pub const AV_CH_LAYOUT_7POINT1POINT2: u64 =
    AV_CH_LAYOUT_7POINT1 | AV_CH_TOP_FRONT_LEFT | AV_CH_TOP_FRONT_RIGHT;
pub const AV_CH_LAYOUT_7POINT1POINT4_BACK: u64 =
    AV_CH_LAYOUT_7POINT1POINT2 | AV_CH_TOP_BACK_LEFT | AV_CH_TOP_BACK_RIGHT;
/////////////////////////////////////////////////////////////////////
/////////////////////////////////////////////////////////////////////

/////////////////////////////////////////////////////////////////////
// FFmpeg 7.0 及以上版本 (包含 libavutil 59.x)
// #if (LIBAVUTIL_VERSION_MAJOR >= 59)
pub const AV_CH_LAYOUT_7POINT2POINT3: u64 =
    AV_CH_LAYOUT_7POINT1POINT2 | AV_CH_TOP_BACK_CENTER | AV_CH_LOW_FREQUENCY_2;
pub const AV_CH_LAYOUT_9POINT1POINT4_BACK: u64 =
    AV_CH_LAYOUT_7POINT1POINT4_BACK | AV_CH_FRONT_LEFT_OF_CENTER | AV_CH_FRONT_RIGHT_OF_CENTER;
/////////////////////////////////////////////////////////////////////
/////////////////////////////////////////////////////////////////////
