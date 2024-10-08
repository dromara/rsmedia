use rsmpeg::ffi;
use libc::c_int;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Disposition: c_int {
        const DEFAULT          = ffi::AV_DISPOSITION_DEFAULT as i32;
        const DUB              = ffi::AV_DISPOSITION_DUB as i32;
        const ORIGINAL         = ffi::AV_DISPOSITION_ORIGINAL as i32;
        const COMMENT          = ffi::AV_DISPOSITION_COMMENT as i32;
        const LYRICS           = ffi::AV_DISPOSITION_LYRICS as i32;
        const KARAOKE          = ffi::AV_DISPOSITION_KARAOKE as i32;
        const FORCED           = ffi::AV_DISPOSITION_FORCED as i32;
        const HEARING_IMPAIRED = ffi::AV_DISPOSITION_HEARING_IMPAIRED as i32;
        const VISUAL_IMPAIRED  = ffi::AV_DISPOSITION_VISUAL_IMPAIRED as i32;
        const CLEAN_EFFECTS    = ffi::AV_DISPOSITION_CLEAN_EFFECTS as i32;
        const ATTACHED_PIC     = ffi::AV_DISPOSITION_ATTACHED_PIC as i32;
        const CAPTIONS         = ffi::AV_DISPOSITION_CAPTIONS as i32;
        const DESCRIPTIONS     = ffi::AV_DISPOSITION_DESCRIPTIONS as i32;
        const METADATA         = ffi::AV_DISPOSITION_METADATA as i32;
        #[cfg(feature = "ffmpeg7")]
        const MULTILAYER       = ffi::AV_DISPOSITION_MULTILAYER as i32;
    }
}
