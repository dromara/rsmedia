use sys::ffi;

#[derive(Copy, Clone, Debug)]
pub enum Mode {
    Input,
    Output,
}

pub struct Destructor {
    ptr: *mut ffi::AVFormatContext,
    mode: Mode,
}

impl Destructor {
    pub unsafe fn new(ptr: *mut ffi::AVFormatContext, mode: Mode) -> Self {
        Destructor { ptr, mode }
    }
}

impl Drop for Destructor {
    fn drop(&mut self) {
        unsafe {
            match self.mode {
                Mode::Input => ffi::avformat_close_input(&mut self.ptr),

                Mode::Output => {
                    ffi::avio_close((*self.ptr).pb);
                    ffi::avformat_free_context(self.ptr);
                }
            }
        }
    }
}
