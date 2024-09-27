use rsmpeg::ffi;
use libc::c_ulonglong;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct ChannelLayout: c_ulonglong {
        const FRONT_LEFT            = ffi::AV_CH_FRONT_LEFT;
        const FRONT_RIGHT           = ffi::AV_CH_FRONT_RIGHT;
        const FRONT_CENTER          = ffi::AV_CH_FRONT_CENTER;
        const LOW_FREQUENCY         = ffi::AV_CH_LOW_FREQUENCY;
        const BACK_LEFT             = ffi::AV_CH_BACK_LEFT;
        const BACK_RIGHT            = ffi::AV_CH_BACK_RIGHT;
        const FRONT_LEFT_OF_CENTER  = ffi::AV_CH_FRONT_LEFT_OF_CENTER;
        const FRONT_RIGHT_OF_CENTER = ffi::AV_CH_FRONT_RIGHT_OF_CENTER;
        const BACK_CENTER           = ffi::AV_CH_BACK_CENTER;
        const SIDE_LEFT             = ffi::AV_CH_SIDE_LEFT;
        const SIDE_RIGHT            = ffi::AV_CH_SIDE_RIGHT;
        const TOP_CENTER            = ffi::AV_CH_TOP_CENTER;
        const TOP_FRONT_LEFT        = ffi::AV_CH_TOP_FRONT_LEFT;
        const TOP_FRONT_CENTER      = ffi::AV_CH_TOP_FRONT_CENTER;
        const TOP_FRONT_RIGHT       = ffi::AV_CH_TOP_FRONT_RIGHT;
        const TOP_BACK_LEFT         = ffi::AV_CH_TOP_BACK_LEFT;
        const TOP_BACK_CENTER       = ffi::AV_CH_TOP_BACK_CENTER;
        const TOP_BACK_RIGHT        = ffi::AV_CH_TOP_BACK_RIGHT;
        const STEREO_LEFT           = ffi::AV_CH_STEREO_LEFT;
        const STEREO_RIGHT          = ffi::AV_CH_STEREO_RIGHT;
        const WIDE_LEFT             = ffi::AV_CH_WIDE_LEFT;
        const WIDE_RIGHT            = ffi::AV_CH_WIDE_RIGHT;
        const SURROUND_DIRECT_LEFT  = ffi::AV_CH_SURROUND_DIRECT_LEFT;
        const SURROUND_DIRECT_RIGHT = ffi::AV_CH_SURROUND_DIRECT_RIGHT;
        const LOW_FREQUENCY_2       = ffi::AV_CH_LOW_FREQUENCY_2;

        const MONO               = ffi::AV_CH_LAYOUT_MONO;
        const STEREO             = ffi::AV_CH_LAYOUT_STEREO;
        const _2POINT1           = ffi::AV_CH_LAYOUT_2POINT1;
        const _2_1               = ffi::AV_CH_LAYOUT_2_1;
        const SURROUND           = ffi::AV_CH_LAYOUT_SURROUND;
        const _3POINT1           = ffi::AV_CH_LAYOUT_3POINT1;
        const _4POINT0           = ffi::AV_CH_LAYOUT_4POINT0;
        const _4POINT1           = ffi::AV_CH_LAYOUT_4POINT1;
        const _2_2               = ffi::AV_CH_LAYOUT_2_2;
        const QUAD               = ffi::AV_CH_LAYOUT_QUAD;
        const _5POINT0           = ffi::AV_CH_LAYOUT_5POINT0;
        const _5POINT1           = ffi::AV_CH_LAYOUT_5POINT1;
        const _5POINT0_BACK      = ffi::AV_CH_LAYOUT_5POINT0_BACK;
        const _5POINT1_BACK      = ffi::AV_CH_LAYOUT_5POINT1_BACK;
        const _6POINT0           = ffi::AV_CH_LAYOUT_6POINT0;
        const _6POINT0_FRONT     = ffi::AV_CH_LAYOUT_6POINT0_FRONT;
        const HEXAGONAL          = ffi::AV_CH_LAYOUT_HEXAGONAL;
        const _6POINT1           = ffi::AV_CH_LAYOUT_6POINT1;
        const _6POINT1_BACK      = ffi::AV_CH_LAYOUT_6POINT1_BACK;
        const _6POINT1_FRONT     = ffi::AV_CH_LAYOUT_6POINT1_FRONT;
        const _7POINT0           = ffi::AV_CH_LAYOUT_7POINT0;
        const _7POINT0_FRONT     = ffi::AV_CH_LAYOUT_7POINT0_FRONT;
        const _7POINT1           = ffi::AV_CH_LAYOUT_7POINT1;
        const _7POINT1_WIDE      = ffi::AV_CH_LAYOUT_7POINT1_WIDE;
        const _7POINT1_WIDE_BACK = ffi::AV_CH_LAYOUT_7POINT1_WIDE_BACK;
        const OCTAGONAL          = ffi::AV_CH_LAYOUT_OCTAGONAL;
        const HEXADECAGONAL      = ffi::AV_CH_LAYOUT_HEXADECAGONAL;
        const STEREO_DOWNMIX     = ffi::AV_CH_LAYOUT_STEREO_DOWNMIX;

        #[cfg(feature = "ffmpeg6")]
        const _3POINT1POINT2      = AV_CH_LAYOUT_3POINT1POINT2;
        #[cfg(feature = "ffmpeg6")]
        const _5POINT1POINT2_BACK = AV_CH_LAYOUT_5POINT1POINT2_BACK;
        #[cfg(feature = "ffmpeg6")]
        const _5POINT1POINT4_BACK = AV_CH_LAYOUT_5POINT1POINT4_BACK;
        #[cfg(feature = "ffmpeg6")]
        const _7POINT1POINT2      = AV_CH_LAYOUT_7POINT1POINT2;
        #[cfg(feature = "ffmpeg6")]
        const _7POINT1POINT4_BACK = AV_CH_LAYOUT_7POINT1POINT4_BACK;
        #[cfg(feature = "ffmpeg6")]
        const CUBE = AV_CH_LAYOUT_CUBE;
    }
}

impl ChannelLayout {
    #[inline]
    pub fn channels(&self) -> i32 {
        unsafe { ffi::av_get_channel_layout_nb_channels(self.bits()) }
    }

    pub fn default(number: i32) -> ChannelLayout {
        unsafe {
            ChannelLayout::from_bits_truncate(ffi::av_get_default_channel_layout(number) as c_ulonglong)
        }
    }
}
