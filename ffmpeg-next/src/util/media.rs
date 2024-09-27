use rsmpeg::ffi;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Type {
    Unknown,
    Video,
    Audio,
    Data,
    Subtitle,
    Attachment,
}

impl From<ffi::AVMediaType> for Type {
    #[inline(always)]
    fn from(value: ffi::AVMediaType) -> Self {
        match value {
            ffi::AVMEDIA_TYPE_UNKNOWN => Type::Unknown,
            ffi::AVMEDIA_TYPE_VIDEO => Type::Video,
            ffi::AVMEDIA_TYPE_AUDIO => Type::Audio,
            ffi::AVMEDIA_TYPE_DATA => Type::Data,
            ffi::AVMEDIA_TYPE_SUBTITLE => Type::Subtitle,
            ffi::AVMEDIA_TYPE_ATTACHMENT => Type::Attachment,
            ffi::AVMEDIA_TYPE_NB => Type::Unknown,
            //  non-exhaustive patterns: `i32::MIN..=-2_i32` and `6_i32..=i32::MAX` not covered
            i32::MIN..=-2_i32 | 6_i32..=i32::MAX => todo!(),
        }
    }
}

impl From<Type> for ffi::AVMediaType {
    #[inline(always)]
    fn from(value: Type) -> ffi::AVMediaType {
        match value {
            Type::Unknown => ffi::AVMEDIA_TYPE_UNKNOWN,
            Type::Video => ffi::AVMEDIA_TYPE_VIDEO,
            Type::Audio => ffi::AVMEDIA_TYPE_AUDIO,
            Type::Data => ffi::AVMEDIA_TYPE_DATA,
            Type::Subtitle => ffi::AVMEDIA_TYPE_SUBTITLE,
            Type::Attachment => ffi::AVMEDIA_TYPE_ATTACHMENT,
        }
    }
}
