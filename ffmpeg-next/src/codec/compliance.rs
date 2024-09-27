// use rsmpeg::ffi;
// use libc::c_int;
//
// #[derive(Eq, PartialEq, Clone, Copy, Debug)]
// pub enum Compliance {
//     VeryStrict,
//     Strict,
//     Normal,
//     Unofficial,
//     Experimental,
// }
//
// impl From<c_int> for Compliance {
//     fn from(value: c_int) -> Self {
//         let val_u32: u32 = value as u32;
//         match val_u32 {
//             ffi::FF_COMPLIANCE_VERY_STRICT => Compliance::VeryStrict,
//             ffi::FF_COMPLIANCE_STRICT => Compliance::Strict,
//             ffi::FF_COMPLIANCE_NORMAL => Compliance::Normal,
//             ffi::FF_COMPLIANCE_UNOFFICIAL => Compliance::Unofficial,
//             ffi::FF_COMPLIANCE_EXPERIMENTAL => Compliance::Experimental,
//             _ => Compliance::Normal,
//         }
//     }
// }
//
// impl From<Compliance> for c_int {
//     fn from(value: Compliance) -> c_int {
//         match value {
//             Compliance::VeryStrict => ffi::FF_COMPLIANCE_VERY_STRICT as i32,
//             Compliance::Strict => ffi::FF_COMPLIANCE_STRICT as i32,
//             Compliance::Normal => ffi::FF_COMPLIANCE_NORMAL as i32,
//             Compliance::Unofficial => ffi::FF_COMPLIANCE_UNOFFICIAL,
//             Compliance::Experimental => ffi::FF_COMPLIANCE_EXPERIMENTAL,
//         }
//     }
// }
