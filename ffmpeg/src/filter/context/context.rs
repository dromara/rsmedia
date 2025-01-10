use super::{Sink, Source};

use libc::c_void;
use sys::ffi;

use crate::{option, ChannelLayout};

pub struct Context {
    ptr: *mut ffi::AVFilterContext,
}

impl Context {
    pub unsafe fn wrap(ptr: *mut ffi::AVFilterContext) -> Self {
        Context { ptr }
    }

    pub unsafe fn as_ptr(&self) -> *const ffi::AVFilterContext {
        self.ptr as *const _
    }

    pub unsafe fn as_mut_ptr(&mut self) -> *mut ffi::AVFilterContext {
        self.ptr
    }
}

impl Context {
    pub fn source(&mut self) -> Source {
        unsafe { Source::wrap(self) }
    }

    pub fn sink(&mut self) -> Sink {
        unsafe { Sink::wrap(self) }
    }

    pub fn set_pixel_format(&mut self, value: crate::format::Pixel) {
        let _ = option::Settable::set::<ffi::AVPixelFormat>(self, "pix_fmts", &value.into());
    }

    pub fn set_sample_format(&mut self, value: crate::format::Sample) {
        let _ = option::Settable::set::<ffi::AVSampleFormat>(self, "sample_fmts", &value.into());
    }

    pub fn set_sample_rate(&mut self, value: u32) {
        let _ = option::Settable::set(self, "sample_rates", &i64::from(value));
    }

    pub fn set_channel_layout(&mut self, value: ChannelLayout) {
        #[cfg(not(feature = "ffmpeg7"))]
        {
            let _ = option::Settable::set(self, "channel_layouts", &value.bits());
        }
        #[cfg(feature = "ffmpeg7")]
        {
            let _ = option::Settable::set_channel_layout(self, "channel_layouts", value);
        }
    }

    pub fn link(&mut self, srcpad: u32, dst: &mut Self, dstpad: u32) {
        unsafe { ffi::avfilter_link(self.as_mut_ptr(), srcpad, dst.as_mut_ptr(), dstpad) };
    }
}

unsafe impl option::Target for Context {
    fn as_ptr(&self) -> *const c_void {
        self.ptr as *const _
    }

    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.ptr as *mut _
    }
}

impl option::Settable for Context {}
