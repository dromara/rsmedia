use rsmpeg::ffi;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Rounding {
    Zero,
    Infinity,
    Down,
    Up,
    NearInfinity,
    PassMinMax,
}

impl From<ffi::AVRounding> for Rounding {
    #[inline(always)]
    fn from(value: ffi::AVRounding) -> Self {
        match value {
            ffi::AV_ROUND_ZERO => Rounding::Zero,
            ffi::AV_ROUND_INF => Rounding::Infinity,
            ffi::AV_ROUND_DOWN => Rounding::Down,
            ffi::AV_ROUND_UP => Rounding::Up,
            ffi::AV_ROUND_NEAR_INF => Rounding::NearInfinity,
            ffi::AV_ROUND_PASS_MINMAX => Rounding::PassMinMax,

            // non-exhaustive patterns: `4_u32`, `6_u32..=8191_u32` and `8193_u32..=u32::MAX` not covered
            4_u32 | 6_u32..=8191_u32 | 8193_u32..=u32::MAX => todo!(),
        }
    }
}

impl From<Rounding> for ffi::AVRounding {
    #[inline(always)]
    fn from(value: Rounding) -> ffi::AVRounding {
        match value {
            Rounding::Zero => ffi::AV_ROUND_ZERO,
            Rounding::Infinity => ffi::AV_ROUND_INF,
            Rounding::Down => ffi::AV_ROUND_DOWN,
            Rounding::Up => ffi::AV_ROUND_UP,
            Rounding::NearInfinity => ffi::AV_ROUND_NEAR_INF,
            Rounding::PassMinMax => ffi::AV_ROUND_PASS_MINMAX,
        }
    }
}
