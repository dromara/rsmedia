use rsmpeg::ffi;

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum Filter {
    Cubic,
    BlackmanNuttall,
    Kaiser,
}

impl From<ffi::SwrFilterType> for Filter {
    fn from(value: ffi::SwrFilterType) -> Filter {
        match value {
            ffi::SWR_FILTER_TYPE_CUBIC => Filter::Cubic,
            ffi::SWR_FILTER_TYPE_BLACKMAN_NUTTALL => Filter::BlackmanNuttall,
            ffi::SWR_FILTER_TYPE_KAISER => Filter::Kaiser,
        }
    }
}

impl From<Filter> for ffi::SwrFilterType {
    fn from(value: Filter) -> ffi::SwrFilterType {
        match value {
            Filter::Cubic => ffi::SWR_FILTER_TYPE_CUBIC,
            Filter::BlackmanNuttall => ffi::SWR_FILTER_TYPE_BLACKMAN_NUTTALL,
            Filter::Kaiser => ffi::SWR_FILTER_TYPE_KAISER,
        }
    }
}
