use rsmpeg::ffi;
use libc::c_int;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Compliance {
    VeryStrict,
    Strict,
    Normal,
    Unofficial,
    Experimental,
}

impl From<c_int> for Compliance {
    fn from(value: c_int) -> Self {
        match value {
            x if x == ffi::FF_COMPLIANCE_VERY_STRICT as i32 => Compliance::VeryStrict,
            x if x == ffi::FF_COMPLIANCE_STRICT as i32 => Compliance::Strict,
            x if x == ffi::FF_COMPLIANCE_NORMAL as i32 => Compliance::Normal,
            x if x == ffi::FF_COMPLIANCE_UNOFFICIAL => Compliance::Unofficial,
            x if x == ffi::FF_COMPLIANCE_EXPERIMENTAL => Compliance::Experimental,
            _ => Compliance::Normal,
        }
    }
}

impl From<Compliance> for c_int {
    fn from(value: Compliance) -> c_int {
        match value {
            Compliance::VeryStrict => ffi::FF_COMPLIANCE_VERY_STRICT as i32,
            Compliance::Strict => ffi::FF_COMPLIANCE_STRICT as i32,
            Compliance::Normal => ffi::FF_COMPLIANCE_NORMAL as i32,
            Compliance::Unofficial => ffi::FF_COMPLIANCE_UNOFFICIAL,
            Compliance::Experimental => ffi::FF_COMPLIANCE_EXPERIMENTAL,
        }
    }
}
