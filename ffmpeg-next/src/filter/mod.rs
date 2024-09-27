pub mod flag;
pub use self::flag::Flags;

pub mod pad;
pub use self::pad::Pad;

pub mod filter;
pub use self::filter::Filter;

pub mod context;
pub use self::context::{Context, Sink, Source};

pub mod graph;
pub use self::graph::Graph;

use std::ffi::{CStr, CString};
use std::str::from_utf8_unchecked;

use rsmpeg::ffi;
use crate::Error;

pub fn register_all() {
    // unsafe {
    //     // TODO:
    //     // ffi::avfilter_register_all();
    // }
}

pub fn register(_filter: &Filter) -> Result<(), Error> {
    // unsafe {
    //     // TODO:
    //     // match ffi::avfilter_register(filter.as_ptr() as *mut _) {
    //     //     0 => Ok(()),
    //     //     _ => Err(Error::InvalidData),
    //     // }
    //     todo!("Not implemented")
    // }
    Ok(())
}

pub fn version() -> u32 {
    unsafe { ffi::avfilter_version() }
}

pub fn configuration() -> &'static str {
    unsafe { from_utf8_unchecked(CStr::from_ptr(ffi::avfilter_configuration()).to_bytes()) }
}

pub fn license() -> &'static str {
    unsafe { from_utf8_unchecked(CStr::from_ptr(ffi::avfilter_license()).to_bytes()) }
}

pub fn find(name: &str) -> Option<Filter> {
    unsafe {
        let name = CString::new(name).unwrap();
        let ptr = ffi::avfilter_get_by_name(name.as_ptr());

        if ptr.is_null() {
            None
        } else {
            Some(Filter::wrap(ptr as *mut _))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paditer() {
        register_all();
        assert_eq!(
            find("overlay")
                .unwrap()
                .inputs()
                .unwrap()
                .map(|input| input.name().unwrap().to_string())
                .collect::<Vec<_>>(),
            vec!("main", "overlay")
        );
    }
}
