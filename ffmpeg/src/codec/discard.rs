use sys::ffi;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Discard {
    None,
    Default,
    NonReference,
    Bidirectional,
    NonIntra,
    NonKey,
    All,
}

impl From<ffi::AVDiscard> for Discard {
    fn from(value: ffi::AVDiscard) -> Self {
        match value {
            ffi::AVDISCARD_NONE => Discard::None,
            ffi::AVDISCARD_DEFAULT => Discard::Default,
            ffi::AVDISCARD_NONREF => Discard::NonReference,
            ffi::AVDISCARD_BIDIR => Discard::Bidirectional,
            ffi::AVDISCARD_NONINTRA => Discard::NonIntra,
            ffi::AVDISCARD_NONKEY => Discard::NonKey,
            ffi::AVDISCARD_ALL => Discard::All,
            _ => {
                eprintln!("Unknown Discard variant: {}", value);
                Discard::None
            }
        }
    }
}

impl From<Discard> for ffi::AVDiscard {
    fn from(value: Discard) -> ffi::AVDiscard {
        match value {
            Discard::None => ffi::AVDISCARD_NONE,
            Discard::Default => ffi::AVDISCARD_DEFAULT,
            Discard::NonReference => ffi::AVDISCARD_NONREF,
            Discard::Bidirectional => ffi::AVDISCARD_BIDIR,
            Discard::NonIntra => ffi::AVDISCARD_NONINTRA,
            Discard::NonKey => ffi::AVDISCARD_NONKEY,
            Discard::All => ffi::AVDISCARD_ALL,
        }
    }
}
