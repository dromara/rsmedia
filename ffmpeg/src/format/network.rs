use sys::ffi;

pub fn init() {
    unsafe {
        ffi::avformat_network_init();
    }
}

pub fn deinit() {
    unsafe {
        ffi::avformat_network_deinit();
    }
}
