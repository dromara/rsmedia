use std::ffi::CStr;
use std::str::from_utf8_unchecked;

use rsmpeg::ffi;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum TransferCharacteristic {
    Reserved0,
    BT709,
    Unspecified,
    Reserved,
    GAMMA22,
    GAMMA28,
    SMPTE170M,
    SMPTE240M,
    Linear,
    Log,
    LogSqrt,
    IEC61966_2_4,
    BT1361_ECG,
    IEC61966_2_1,
    BT2020_10,
    BT2020_12,
    SMPTE2084,
    SMPTE428,
    ARIB_STD_B67,
}

impl TransferCharacteristic {
    pub fn name(&self) -> Option<&'static str> {
        if *self == TransferCharacteristic::Unspecified {
            return None;
        }
        unsafe {
            let ptr = ffi::av_color_transfer_name((*self).into());
            ptr.as_ref()
                .map(|ptr| from_utf8_unchecked(CStr::from_ptr(ptr).to_bytes()))
        }
    }
}

impl From<ffi::AVColorTransferCharacteristic> for TransferCharacteristic {
    fn from(value: ffi::AVColorTransferCharacteristic) -> TransferCharacteristic {
        match value {
            ffi::AVCOL_TRC_RESERVED0 => TransferCharacteristic::Reserved0,
            ffi::AVCOL_TRC_BT709 => TransferCharacteristic::BT709,
            ffi::AVCOL_TRC_UNSPECIFIED => TransferCharacteristic::Unspecified,
            ffi::AVCOL_TRC_RESERVED => TransferCharacteristic::Reserved,
            ffi::AVCOL_TRC_GAMMA22 => TransferCharacteristic::GAMMA22,
            ffi::AVCOL_TRC_GAMMA28 => TransferCharacteristic::GAMMA28,
            ffi::AVCOL_TRC_SMPTE170M => TransferCharacteristic::SMPTE170M,
            ffi::AVCOL_TRC_SMPTE240M => TransferCharacteristic::SMPTE240M,
            ffi::AVCOL_TRC_LINEAR => TransferCharacteristic::Linear,
            ffi::AVCOL_TRC_LOG => TransferCharacteristic::Log,
            ffi::AVCOL_TRC_LOG_SQRT => TransferCharacteristic::LogSqrt,
            ffi::AVCOL_TRC_IEC61966_2_4 => TransferCharacteristic::IEC61966_2_4,
            ffi::AVCOL_TRC_BT1361_ECG => TransferCharacteristic::BT1361_ECG,
            ffi::AVCOL_TRC_IEC61966_2_1 => TransferCharacteristic::IEC61966_2_1,
            ffi::AVCOL_TRC_BT2020_10 => TransferCharacteristic::BT2020_10,
            ffi::AVCOL_TRC_BT2020_12 => TransferCharacteristic::BT2020_12,
            ffi::AVCOL_TRC_NB => TransferCharacteristic::Reserved0,
            ffi::AVCOL_TRC_SMPTE2084 => TransferCharacteristic::SMPTE2084,
            ffi::AVCOL_TRC_SMPTE428 => TransferCharacteristic::SMPTE428,
            ffi::AVCOL_TRC_ARIB_STD_B67 => TransferCharacteristic::ARIB_STD_B67,
            20_u32..=u32::MAX => todo!(),
        }
    }
}

impl From<TransferCharacteristic> for ffi::AVColorTransferCharacteristic {
    fn from(value: TransferCharacteristic) -> ffi::AVColorTransferCharacteristic {
        match value {
            TransferCharacteristic::Reserved0 => ffi::AVCOL_TRC_RESERVED0,
            TransferCharacteristic::BT709 => ffi::AVCOL_TRC_BT709,
            TransferCharacteristic::Unspecified => ffi::AVCOL_TRC_UNSPECIFIED,
            TransferCharacteristic::Reserved => ffi::AVCOL_TRC_RESERVED,
            TransferCharacteristic::GAMMA22 => ffi::AVCOL_TRC_GAMMA22,
            TransferCharacteristic::GAMMA28 => ffi::AVCOL_TRC_GAMMA28,
            TransferCharacteristic::SMPTE170M => ffi::AVCOL_TRC_SMPTE170M,
            TransferCharacteristic::SMPTE240M => ffi::AVCOL_TRC_SMPTE240M,
            TransferCharacteristic::Linear => ffi::AVCOL_TRC_LINEAR,
            TransferCharacteristic::Log => ffi::AVCOL_TRC_LOG,
            TransferCharacteristic::LogSqrt => ffi::AVCOL_TRC_LOG_SQRT,
            TransferCharacteristic::IEC61966_2_4 => ffi::AVCOL_TRC_IEC61966_2_4,
            TransferCharacteristic::BT1361_ECG => ffi::AVCOL_TRC_BT1361_ECG,
            TransferCharacteristic::IEC61966_2_1 => ffi::AVCOL_TRC_IEC61966_2_1,
            TransferCharacteristic::BT2020_10 => ffi::AVCOL_TRC_BT2020_10,
            TransferCharacteristic::BT2020_12 => ffi::AVCOL_TRC_BT2020_12,
            TransferCharacteristic::SMPTE2084 => ffi::AVCOL_TRC_SMPTE2084,
            TransferCharacteristic::SMPTE428 => ffi::AVCOL_TRC_SMPTE428,
            TransferCharacteristic::ARIB_STD_B67 => ffi::AVCOL_TRC_ARIB_STD_B67,
        }
    }
}
