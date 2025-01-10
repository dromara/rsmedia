pub mod extensions;
pub mod input;
pub mod output;

use std::ffi::CStr;
use std::marker::PhantomData;
use std::str::from_utf8_unchecked;

use sys::ffi;

pub struct Info<'a> {
    ptr: *mut ffi::AVDeviceInfo,

    _marker: PhantomData<&'a ()>,
}

impl<'a> Info<'a> {
    pub unsafe fn wrap(ptr: *mut ffi::AVDeviceInfo) -> Self {
        Info {
            ptr,
            _marker: PhantomData,
        }
    }

    pub unsafe fn as_ptr(&self) -> *const ffi::AVDeviceInfo {
        self.ptr as *const _
    }

    pub unsafe fn as_mut_ptr(&mut self) -> *mut ffi::AVDeviceInfo {
        self.ptr
    }
}

impl<'a> Info<'a> {
    pub fn name(&self) -> &str {
        unsafe { from_utf8_unchecked(CStr::from_ptr((*self.as_ptr()).device_name).to_bytes()) }
    }

    pub fn description(&self) -> &str {
        unsafe {
            from_utf8_unchecked(CStr::from_ptr((*self.as_ptr()).device_description).to_bytes())
        }
    }
}

pub fn register_all() {
    unsafe {
        ffi::avdevice_register_all();
    }
}

pub fn version() -> u32 {
    unsafe { ffi::avdevice_version() }
}

pub fn configuration() -> &'static str {
    unsafe { from_utf8_unchecked(CStr::from_ptr(ffi::avdevice_configuration()).to_bytes()) }
}

pub fn license() -> &'static str {
    unsafe { from_utf8_unchecked(CStr::from_ptr(ffi::avdevice_license()).to_bytes()) }
}
