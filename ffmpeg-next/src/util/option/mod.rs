mod traits;
pub use self::traits::{Gettable, Iterable, Settable, Target};

use rsmpeg::ffi;

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum Type {
    Flags,
    Int,
    Int64,
    Double,
    Float,
    String,
    Rational,
    Binary,
    Dictionary,
    Constant,

    ImageSize,
    PixelFormat,
    SampleFormat,
    VideoRate,
    Duration,
    Color,
    ChannelLayout,

    #[cfg(feature = "ffmpeg7")]
    FlagArray,
    c_ulong,
    bool,

    #[cfg(feature = "ffmpeg7")]
    UInt,
}

impl From<ffi::AVOptionType> for Type {
    fn from(value: ffi::AVOptionType) -> Self {
        match value {
            ffi::AV_OPT_TYPE_FLAGS => Type::Flags,
            ffi::AV_OPT_TYPE_INT => Type::Int,
            ffi::AV_OPT_TYPE_INT64 => Type::Int64,
            ffi::AV_OPT_TYPE_DOUBLE => Type::Double,
            ffi::AV_OPT_TYPE_FLOAT => Type::Float,
            ffi::AV_OPT_TYPE_STRING => Type::String,
            ffi::AV_OPT_TYPE_RATIONAL => Type::Rational,
            ffi::AV_OPT_TYPE_BINARY => Type::Binary,
            ffi::AV_OPT_TYPE_DICT => Type::Dictionary,
            ffi::AV_OPT_TYPE_CONST => Type::Constant,
            ffi::AV_OPT_TYPE_UINT64 => Type::c_ulong,
            ffi::AV_OPT_TYPE_BOOL => Type::bool,

            ffi::AV_OPT_TYPE_IMAGE_SIZE => Type::ImageSize,
            ffi::AV_OPT_TYPE_PIXEL_FMT => Type::PixelFormat,
            ffi::AV_OPT_TYPE_SAMPLE_FMT => Type::SampleFormat,
            ffi::AV_OPT_TYPE_VIDEO_RATE => Type::VideoRate,
            ffi::AV_OPT_TYPE_DURATION => Type::Duration,
            ffi::AV_OPT_TYPE_COLOR => Type::Color,

            #[cfg(not(feature = "ffmpeg7"))]
            ffi::AV_OPT_TYPE_CHANNEL_LAYOUT => Type::ChannelLayout,

            #[cfg(feature = "ffmpeg7")]
            ffi::AV_OPT_TYPE_CHLAYOUT => Type::ChannelLayout,
            #[cfg(feature = "ffmpeg7")]
            ffi::AV_OPT_TYPE_FLAG_ARRAY => Type::FlagArray,
            #[cfg(feature = "ffmpeg7")]
            ffi::AV_OPT_TYPE_UINT => Type::UInt,

            // non-exhaustive patterns: `0_u32`, `19_u32..=65535_u32` and `65537_u32..=u32::MAX` not covered
            0_u32 | 19_u32..=65535_u32 | 65537_u32..=u32::MAX => todo!(),
        }
    }
}

impl From<Type> for ffi::AVOptionType {
    fn from(value: Type) -> ffi::AVOptionType {
        match value {
            Type::Flags => ffi::AV_OPT_TYPE_FLAGS,
            Type::Int => ffi::AV_OPT_TYPE_INT,
            Type::Int64 => ffi::AV_OPT_TYPE_INT64,
            Type::Double => ffi::AV_OPT_TYPE_DOUBLE,
            Type::Float => ffi::AV_OPT_TYPE_FLOAT,
            Type::String => ffi::AV_OPT_TYPE_STRING,
            Type::Rational => ffi::AV_OPT_TYPE_RATIONAL,
            Type::Binary => ffi::AV_OPT_TYPE_BINARY,
            Type::Dictionary => ffi::AV_OPT_TYPE_DICT,
            Type::Constant => ffi::AV_OPT_TYPE_CONST,
            Type::c_ulong => ffi::AV_OPT_TYPE_UINT64,
            Type::bool => ffi::AV_OPT_TYPE_BOOL,

            Type::ImageSize => ffi::AV_OPT_TYPE_IMAGE_SIZE,
            Type::PixelFormat => ffi::AV_OPT_TYPE_PIXEL_FMT,
            Type::SampleFormat => ffi::AV_OPT_TYPE_SAMPLE_FMT,
            Type::VideoRate => ffi::AV_OPT_TYPE_VIDEO_RATE,
            Type::Duration => ffi::AV_OPT_TYPE_DURATION,
            Type::Color => ffi::AV_OPT_TYPE_COLOR,

            #[cfg(not(feature = "ffmpeg7"))]
            Type::ChannelLayout => ffi::AV_OPT_TYPE_CHANNEL_LAYOUT,

            #[cfg(feature = "ffmpeg7")]
            Type::ChannelLayout => ffi::AV_OPT_TYPE_CHLAYOUT,
            #[cfg(feature = "ffmpeg7")]
            Type::FlagArray => ffi::AV_OPT_TYPE_FLAG_ARRAY,
            #[cfg(feature = "ffmpeg7")]
            Type::UInt => ffi::AV_OPT_TYPE_UINT,
        }
    }
}
