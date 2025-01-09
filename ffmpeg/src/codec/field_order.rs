use rsmpeg::ffi;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum FieldOrder {
    Unknown,
    Progressive,
    TT,
    BB,
    TB,
    BT,
}

impl From<ffi::AVFieldOrder> for FieldOrder {
    fn from(value: ffi::AVFieldOrder) -> Self {
        match value {
            ffi::AV_FIELD_UNKNOWN => FieldOrder::Unknown,
            ffi::AV_FIELD_PROGRESSIVE => FieldOrder::Progressive,
            ffi::AV_FIELD_TT => FieldOrder::TT,
            ffi::AV_FIELD_BB => FieldOrder::BB,
            ffi::AV_FIELD_TB => FieldOrder::TB,
            ffi::AV_FIELD_BT => FieldOrder::BT,
        }
    }
}

impl From<FieldOrder> for ffi::AVFieldOrder {
    fn from(value: FieldOrder) -> ffi::AVFieldOrder {
        match value {
            FieldOrder::Unknown => ffi::AV_FIELD_UNKNOWN,
            FieldOrder::Progressive => ffi::AV_FIELD_PROGRESSIVE,
            FieldOrder::TT => ffi::AV_FIELD_TT,
            FieldOrder::BB => ffi::AV_FIELD_BB,
            FieldOrder::TB => ffi::AV_FIELD_TB,
            FieldOrder::BT => ffi::AV_FIELD_BT,
        }
    }
}
