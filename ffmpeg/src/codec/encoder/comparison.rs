use rsmpeg::ffi;
use libc::c_int;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Comparison {
    SAD,
    SSE,
    SATD,
    DCT,
    PSNR,
    BIT,
    RD,
    ZERO,
    VSAD,
    VSSE,
    NSSE,
    W53,
    W97,
    DCTMAX,
    DCT264,
    CHROMA,
}

impl From<c_int> for Comparison {
    fn from(value: c_int) -> Comparison {
        match value as u32 {
            ffi::FF_CMP_SAD => Comparison::SAD,
            ffi::FF_CMP_SSE => Comparison::SSE,
            ffi::FF_CMP_SATD => Comparison::SATD,
            ffi::FF_CMP_DCT => Comparison::DCT,
            ffi::FF_CMP_PSNR => Comparison::PSNR,
            ffi::FF_CMP_BIT => Comparison::BIT,
            ffi::FF_CMP_RD => Comparison::RD,
            ffi::FF_CMP_ZERO => Comparison::ZERO,
            ffi::FF_CMP_VSAD => Comparison::VSAD,
            ffi::FF_CMP_VSSE => Comparison::VSSE,
            ffi::FF_CMP_NSSE => Comparison::NSSE,
            ffi::FF_CMP_W53 => Comparison::W53,
            ffi::FF_CMP_W97 => Comparison::W97,
            ffi::FF_CMP_DCTMAX => Comparison::DCTMAX,
            ffi::FF_CMP_DCT264 => Comparison::DCT264,
            ffi::FF_CMP_CHROMA => Comparison::CHROMA,

            _ => Comparison::ZERO,
        }
    }
}

impl From<Comparison> for c_int {
    fn from(value: Comparison) -> c_int {
        match value {
            Comparison::SAD => ffi::FF_CMP_SAD as i32,
            Comparison::SSE => ffi::FF_CMP_SSE as i32,
            Comparison::SATD => ffi::FF_CMP_SATD as i32,
            Comparison::DCT => ffi::FF_CMP_DCT as i32,
            Comparison::PSNR => ffi::FF_CMP_PSNR as i32,
            Comparison::BIT => ffi::FF_CMP_BIT as i32,
            Comparison::RD => ffi::FF_CMP_RD as i32,
            Comparison::ZERO => ffi::FF_CMP_ZERO as i32,
            Comparison::VSAD => ffi::FF_CMP_VSAD as i32,
            Comparison::VSSE => ffi::FF_CMP_VSSE as i32,
            Comparison::NSSE => ffi::FF_CMP_NSSE as i32,
            Comparison::W53 => ffi::FF_CMP_W53 as i32,
            Comparison::W97 => ffi::FF_CMP_W97 as i32,
            Comparison::DCTMAX => ffi::FF_CMP_DCTMAX as i32,
            Comparison::DCT264 => ffi::FF_CMP_DCT264 as i32,
            Comparison::CHROMA => ffi::FF_CMP_CHROMA as i32,
        }
    }
}
