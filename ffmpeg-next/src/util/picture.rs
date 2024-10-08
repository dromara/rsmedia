use rsmpeg::ffi;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Type {
    None,
    I,
    P,
    B,
    S,
    SI,
    SP,
    BI,
}

impl From<ffi::AVPictureType> for Type {
    #[inline(always)]
    fn from(value: ffi::AVPictureType) -> Type {
        match value {
            ffi::AV_PICTURE_TYPE_NONE => Type::None,
            ffi::AV_PICTURE_TYPE_I => Type::I,
            ffi::AV_PICTURE_TYPE_P => Type::P,
            ffi::AV_PICTURE_TYPE_B => Type::B,
            ffi::AV_PICTURE_TYPE_S => Type::S,
            ffi::AV_PICTURE_TYPE_SI => Type::SI,
            ffi::AV_PICTURE_TYPE_SP => Type::SP,
            ffi::AV_PICTURE_TYPE_BI => Type::BI,

            // non-exhaustive patterns: `8_u32..=u32::MAX` not covered
            8_u32..=u32::MAX => todo!(),
        }
    }
}

impl From<Type> for ffi::AVPictureType {
    #[inline(always)]
    fn from(value: Type) -> ffi::AVPictureType {
        match value {
            Type::None => ffi::AV_PICTURE_TYPE_NONE,
            Type::I => ffi::AV_PICTURE_TYPE_I,
            Type::P => ffi::AV_PICTURE_TYPE_P,
            Type::B => ffi::AV_PICTURE_TYPE_B,
            Type::S => ffi::AV_PICTURE_TYPE_S,
            Type::SI => ffi::AV_PICTURE_TYPE_SI,
            Type::SP => ffi::AV_PICTURE_TYPE_SP,
            Type::BI => ffi::AV_PICTURE_TYPE_BI,
        }
    }
}
