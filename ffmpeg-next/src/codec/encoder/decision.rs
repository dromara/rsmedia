use rsmpeg::ffi;
use libc::c_int;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Decision {
    Simple,
    Bits,
    RateDistortion,
}

impl From<c_int> for Decision {
    fn from(value: c_int) -> Decision {
        match value as u32 {
            ffi::FF_MB_DECISION_SIMPLE => Decision::Simple,
            ffi::FF_MB_DECISION_BITS => Decision::Bits,
            ffi::FF_MB_DECISION_RD => Decision::RateDistortion,

            _ => Decision::Simple,
        }
    }
}

impl From<Decision> for c_int {
    fn from(value: Decision) -> c_int {
        match value {
            Decision::Simple => ffi::FF_MB_DECISION_SIMPLE as i32,
            Decision::Bits => ffi::FF_MB_DECISION_BITS as i32,
            Decision::RateDistortion => ffi::FF_MB_DECISION_RD as i32,
        }
    }
}
