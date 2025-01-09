use rsmpeg::ffi;

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum Dither {
    None,
    Rectangular,
    Triangular,
    TriangularHighPass,

    NoiseShapingLipshitz,
    NoiseShapingFWeighted,
    NoiseShapingModifiedEWeighted,
    NoiseShapingImprovedEWeighted,
    NoiseShapingShibata,
    NoiseShapingLowShibata,
    NoiseShapingHighShibata,
}

impl From<ffi::SwrDitherType> for Dither {
    fn from(value: ffi::SwrDitherType) -> Dither {
        match value {
            ffi::SWR_DITHER_NONE => Dither::None,
            ffi::SWR_DITHER_RECTANGULAR => Dither::Rectangular,
            ffi::SWR_DITHER_TRIANGULAR => Dither::Triangular,
            ffi::SWR_DITHER_TRIANGULAR_HIGHPASS => Dither::TriangularHighPass,

            ffi::SWR_DITHER_NS => Dither::None,
            ffi::SWR_DITHER_NS_LIPSHITZ => Dither::NoiseShapingLipshitz,
            ffi::SWR_DITHER_NS_F_WEIGHTED => Dither::NoiseShapingFWeighted,
            ffi::SWR_DITHER_NS_MODIFIED_E_WEIGHTED => Dither::NoiseShapingModifiedEWeighted,
            ffi::SWR_DITHER_NS_IMPROVED_E_WEIGHTED => Dither::NoiseShapingImprovedEWeighted,
            ffi::SWR_DITHER_NS_SHIBATA => Dither::NoiseShapingShibata,
            ffi::SWR_DITHER_NS_LOW_SHIBATA => Dither::NoiseShapingLowShibata,
            ffi::SWR_DITHER_NS_HIGH_SHIBATA => Dither::NoiseShapingHighShibata,
            ffi::SWR_DITHER_NB => Dither::None,
        }
    }
}

impl From<Dither> for ffi::SwrDitherType {
    fn from(value: Dither) -> ffi::SwrDitherType {
        match value {
            Dither::None => ffi::SWR_DITHER_NONE,
            Dither::Rectangular => ffi::SWR_DITHER_RECTANGULAR,
            Dither::Triangular => ffi::SWR_DITHER_TRIANGULAR,
            Dither::TriangularHighPass => ffi::SWR_DITHER_TRIANGULAR_HIGHPASS,

            Dither::NoiseShapingLipshitz => ffi::SWR_DITHER_NS_LIPSHITZ,
            Dither::NoiseShapingFWeighted => ffi::SWR_DITHER_NS_F_WEIGHTED,
            Dither::NoiseShapingModifiedEWeighted => ffi::SWR_DITHER_NS_MODIFIED_E_WEIGHTED,
            Dither::NoiseShapingImprovedEWeighted => ffi::SWR_DITHER_NS_IMPROVED_E_WEIGHTED,
            Dither::NoiseShapingShibata => ffi::SWR_DITHER_NS_SHIBATA,
            Dither::NoiseShapingLowShibata => ffi::SWR_DITHER_NS_LOW_SHIBATA,
            Dither::NoiseShapingHighShibata => ffi::SWR_DITHER_NS_HIGH_SHIBATA,
        }
    }
}
