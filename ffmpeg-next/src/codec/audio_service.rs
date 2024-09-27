use rsmpeg::ffi;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum AudioService {
    Main,
    Effects,
    VisuallyImpaired,
    HearingImpaired,
    Dialogue,
    Commentary,
    Emergency,
    VoiceOver,
    Karaoke,
}

impl From<ffi::AVAudioServiceType> for AudioService {
    fn from(value: ffi::AVAudioServiceType) -> Self {
        match value {
            ffi::AV_AUDIO_SERVICE_TYPE_MAIN => AudioService::Main,
            ffi::AV_AUDIO_SERVICE_TYPE_EFFECTS => AudioService::Effects,
            ffi::AV_AUDIO_SERVICE_TYPE_VISUALLY_IMPAIRED => AudioService::VisuallyImpaired,
            ffi::AV_AUDIO_SERVICE_TYPE_HEARING_IMPAIRED => AudioService::HearingImpaired,
            ffi::AV_AUDIO_SERVICE_TYPE_DIALOGUE => AudioService::Dialogue,
            ffi::AV_AUDIO_SERVICE_TYPE_COMMENTARY => AudioService::Commentary,
            ffi::AV_AUDIO_SERVICE_TYPE_EMERGENCY => AudioService::Emergency,
            ffi::AV_AUDIO_SERVICE_TYPE_VOICE_OVER => AudioService::VoiceOver,
            ffi::AV_AUDIO_SERVICE_TYPE_KARAOKE => AudioService::Karaoke,
            ffi::AV_AUDIO_SERVICE_TYPE_NB => AudioService::Main,
            // non-exhaustive patterns: `10_u32..=u32::MAX` not covered
            10_u32..=u32::MAX => todo!(),
        }
    }
}

impl From<AudioService> for ffi::AVAudioServiceType {
    fn from(value: AudioService) -> ffi::AVAudioServiceType {
        match value {
            AudioService::Main => ffi::AV_AUDIO_SERVICE_TYPE_MAIN,
            AudioService::Effects => ffi::AV_AUDIO_SERVICE_TYPE_EFFECTS,
            AudioService::VisuallyImpaired => ffi::AV_AUDIO_SERVICE_TYPE_VISUALLY_IMPAIRED,
            AudioService::HearingImpaired => ffi::AV_AUDIO_SERVICE_TYPE_HEARING_IMPAIRED,
            AudioService::Dialogue => ffi::AV_AUDIO_SERVICE_TYPE_DIALOGUE,
            AudioService::Commentary => ffi::AV_AUDIO_SERVICE_TYPE_COMMENTARY,
            AudioService::Emergency => ffi::AV_AUDIO_SERVICE_TYPE_EMERGENCY,
            AudioService::VoiceOver => ffi::AV_AUDIO_SERVICE_TYPE_VOICE_OVER,
            AudioService::Karaoke => ffi::AV_AUDIO_SERVICE_TYPE_KARAOKE,
        }
    }
}
