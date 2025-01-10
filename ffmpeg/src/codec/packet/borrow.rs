use std::mem;
use std::ptr;

use super::Ref;
use libc::c_int;
use sys::ffi;

pub struct Borrow<'a> {
    packet: ffi::AVPacket,
    data: &'a [u8],
}

impl<'a> Borrow<'a> {
    pub fn new(data: &[u8]) -> Borrow {
        unsafe {
            let mut packet: ffi::AVPacket = mem::zeroed();

            packet.data = data.as_ptr() as *mut _;
            packet.size = data.len() as c_int;

            Borrow { packet, data }
        }
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.packet.size as usize
    }

    #[inline]
    pub fn data(&self) -> Option<&[u8]> {
        Some(self.data)
    }
}

impl<'a> Ref for Borrow<'a> {
    fn as_ptr(&self) -> *const ffi::AVPacket {
        &self.packet
    }
}

impl<'a> Drop for Borrow<'a> {
    fn drop(&mut self) {
        unsafe {
            self.packet.data = ptr::null_mut();
            self.packet.size = 0;

            ffi::av_packet_unref(&mut self.packet);
        }
    }
}
