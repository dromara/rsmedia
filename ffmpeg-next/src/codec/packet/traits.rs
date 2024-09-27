use rsmpeg::ffi;

pub trait Ref {
    fn as_ptr(&self) -> *const ffi::AVPacket;
}

pub trait Mut {
    fn as_mut_ptr(&mut self) -> *mut ffi::AVPacket;
}
