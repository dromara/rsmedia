use rsmpeg::ffi;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Location {
    Unspecified,
    Left,
    Center,
    TopLeft,
    Top,
    BottomLeft,
    Bottom,
}

impl From<ffi::AVChromaLocation> for Location {
    fn from(value: ffi::AVChromaLocation) -> Self {
        match value {
            ffi::AVCHROMA_LOC_UNSPECIFIED => Location::Unspecified,
            ffi::AVCHROMA_LOC_LEFT => Location::Left,
            ffi::AVCHROMA_LOC_CENTER => Location::Center,
            ffi::AVCHROMA_LOC_TOPLEFT => Location::TopLeft,
            ffi::AVCHROMA_LOC_TOP => Location::Top,
            ffi::AVCHROMA_LOC_BOTTOMLEFT => Location::BottomLeft,
            ffi::AVCHROMA_LOC_BOTTOM => Location::Bottom,
            ffi::AVCHROMA_LOC_NB => Location::Unspecified,
        }
    }
}

impl From<Location> for ffi::AVChromaLocation {
    fn from(value: Location) -> ffi::AVChromaLocation {
        match value {
            Location::Unspecified => ffi::AVCHROMA_LOC_UNSPECIFIED,
            Location::Left => ffi::AVCHROMA_LOC_LEFT,
            Location::Center => ffi::AVCHROMA_LOC_CENTER,
            Location::TopLeft => ffi::AVCHROMA_LOC_TOPLEFT,
            Location::Top => ffi::AVCHROMA_LOC_TOP,
            Location::BottomLeft => ffi::AVCHROMA_LOC_BOTTOMLEFT,
            Location::Bottom => ffi::AVCHROMA_LOC_BOTTOM,
        }
    }
}
