use rsmpeg::ffi;

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum Engine {
    Software,
    SoundExchange,
}

impl From<ffi::SwrEngine> for Engine {
    fn from(value: ffi::SwrEngine) -> Engine {
        match value {
            ffi::SWR_ENGINE_SWR => Engine::Software,
            ffi::SWR_ENGINE_SOXR => Engine::SoundExchange,
            ffi::SWR_ENGINE_NB => Engine::Software,
        }
    }
}

impl From<Engine> for ffi::SwrEngine {
    fn from(value: Engine) -> ffi::SwrEngine {
        match value {
            Engine::Software => ffi::SWR_ENGINE_SWR,
            Engine::SoundExchange => ffi::SWR_ENGINE_SOXR,
        }
    }
}
