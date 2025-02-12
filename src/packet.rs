use crate::error::MediaError;
use crate::flags::AvPacketFlags;
use crate::stream::Stream;
use crate::time::Time;
use crate::time::TIME_BASE;
use crate::Rational;

use libc::{c_int, c_uint};
use rsmpeg::avcodec::AVPacket;
use rsmpeg::avformat::{AVFormatContextInput, AVFormatContextOutput};
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi;

/// Represents a stream packet.
#[derive(Debug)]
pub struct Packet {
    inner: AVPacket,
    time_base: Rational,
}

impl Packet {
    /// Get packet PTS (presentation timestamp).
    #[inline]
    pub fn pts(&self) -> Time {
        Time::new(Some(self.inner.pts), self.time_base)
    }

    /// Get packet DTS (decoder timestamp).
    #[inline]
    pub fn dts(&self) -> Time {
        Time::new(Some(self.inner.dts), self.time_base)
    }

    /// Get packet duration.
    #[inline]
    pub fn duration(&self) -> Time {
        Time::new(Some(self.inner.duration), self.time_base)
    }

    /// Set packet PTS (presentation timestamp).
    #[inline]
    pub fn set_pts(&mut self, timestamp: Time) {
        self.inner.set_pts(
            timestamp
                .aligned_with_rational(self.time_base)
                .into_value()
                .unwrap(),
        );
    }

    /// Set packet DTS (decoder timestamp).
    #[inline]
    pub fn set_dts(&mut self, timestamp: Time) {
        self.inner.set_dts(
            timestamp
                .aligned_with_rational(self.time_base)
                .into_value()
                .unwrap(),
        );
    }

    /// Set duration.
    #[inline]
    pub fn set_duration(&mut self, timestamp: Time) {
        if let Some(duration) = timestamp.aligned_with_rational(self.time_base).into_value() {
            self.inner.set_duration(duration);
        }
    }

    #[inline(always)]
    pub unsafe fn is_empty(&self) -> bool {
        self.inner.size == 0
    }

    #[inline]
    pub fn flags(&self) -> AvPacketFlags {
        AvPacketFlags::from_bits_truncate(self.inner.flags as c_uint)
    }

    // Check whether packet is key.
    #[inline]
    pub fn is_key(&self) -> bool {
        self.flags().contains(AvPacketFlags::KEY)
    }

    #[inline]
    pub fn is_corrupt(&self) -> bool {
        self.flags().contains(AvPacketFlags::CORRUPT)
    }

    #[inline]
    pub fn stream_index(&self) -> usize {
        self.inner.stream_index as usize
    }

    #[inline]
    pub fn set_stream_index(&mut self, index: usize) {
        self.inner.set_stream_index(index as c_int);
    }

    #[inline]
    pub fn set_position(&mut self, value: isize) {
        self.inner.set_pos(value as i64)
    }

    #[inline]
    pub fn rescale_ts<S, D>(&mut self, source: S, destination: D)
    where
        S: Into<Rational>,
        D: Into<Rational>,
    {
        unsafe {
            ffi::av_packet_rescale_ts(
                self.inner.as_mut_ptr(),
                source.into().into(),
                destination.into().into(),
            );
        }
    }

    #[inline]
    pub fn data(&self) -> Option<&[u8]> {
        unsafe {
            if self.inner.data.is_null() {
                None
            } else {
                Some(std::slice::from_raw_parts(
                    self.inner.data,
                    self.inner.size as usize,
                ))
            }
        }
    }

    #[inline]
    pub fn data_mut(&mut self) -> Option<&mut [u8]> {
        unsafe {
            if self.inner.data.is_null() {
                None
            } else {
                Some(std::slice::from_raw_parts_mut(
                    self.inner.data,
                    self.inner.size as usize,
                ))
            }
        }
    }

    #[inline]
    pub fn read(&mut self, format: *mut ffi::AVFormatContext) -> Result<(), RsmpegError> {
        unsafe {
            match ffi::av_read_frame(format, self.inner.as_mut_ptr()) {
                0 => Ok(()),
                e => Err(RsmpegError::from(e)),
            }
        }
    }

    #[inline]
    pub fn write(&self, format: &mut AVFormatContextOutput) -> Result<bool, RsmpegError> {
        unsafe {
            if self.is_empty() {
                return Err(RsmpegError::AVError(ffi::AVERROR_INVALIDDATA));
            }

            match ffi::av_write_frame(format.as_mut_ptr(), self.inner.as_ptr() as *mut _) {
                1 => Ok(true),
                0 => Ok(false),
                e => Err(RsmpegError::from(e)),
            }
        }
    }

    #[inline]
    pub fn write_interleaved(&self, format: &mut AVFormatContextOutput) -> Result<(), RsmpegError> {
        unsafe {
            if self.is_empty() {
                return Err(RsmpegError::AVError(ffi::AVERROR_INVALIDDATA));
            }

            match ffi::av_interleaved_write_frame(
                format.as_mut_ptr(),
                self.inner.as_ptr() as *mut _,
            ) {
                0 => Ok(()),
                e => Err(RsmpegError::from(e)),
            }
        }
    }

    /// Create a new packet.
    ///
    /// # Arguments
    ///
    /// * `inner` - Inner `AvPacket`.
    /// * `time_base` - Source time base.
    pub fn new(inner: Packet, time_base: Rational) -> Self {
        Self {
            inner: inner.into_inner(),
            time_base,
        }
    }

    /// Downcast to native inner type.
    pub(crate) fn into_inner(self) -> AVPacket {
        self.inner
    }

    /// Downcast to native inner type and time base.
    pub(crate) fn into_inner_parts(self) -> (AVPacket, Rational) {
        (self.inner, self.time_base)
    }

    /////////////////////

    #[inline]
    pub fn copy(data: &[u8]) -> Self {
        use std::io::Write;

        let mut packet = Packet::new_with_size(data.len());
        packet.data_mut().unwrap().write_all(data).unwrap();

        packet
    }

    #[inline]
    pub fn empty() -> Self {
        unsafe {
            Packet {
                inner: AVPacket::new(),
                time_base: TIME_BASE,
            }
        }
    }

    #[inline]
    pub fn copy_props(&mut self, packet: AVPacket) -> Result<(), RsmpegError> {
        unsafe {
            let _res = ffi::av_packet_copy_props(self.inner.as_mut_ptr(), packet.as_ptr());
            if _res < 0 {
                Err(RsmpegError::from(_res))
            } else {
                Ok(())
            }
        }
    }

    #[inline]
    pub fn new_with_size(size: usize) -> Self {
        unsafe {
            let mut pkt = std::mem::zeroed::<AVPacket>();

            ffi::av_init_packet(pkt.as_mut_ptr());
            ffi::av_new_packet(pkt.as_mut_ptr(), size as c_int);

            Packet {
                inner: pkt,
                time_base: TIME_BASE,
            }
        }
    }
}

unsafe impl Send for Packet {}
unsafe impl Sync for Packet {}

//////////////////////////////

pub struct PacketIter<'a> {
    context: &'a mut AVFormatContextInput,
}

impl<'a> PacketIter<'a> {
    pub fn new(context: &mut AVFormatContextInput) -> PacketIter {
        PacketIter { context }
    }
}

impl<'a> Iterator for PacketIter<'a> {
    type Item = Result<(Stream<'a>, Packet), RsmpegError>;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        let mut packet = Packet::empty();

        match packet.read(self.context.as_mut_ptr()) {
            Ok(..) => unsafe {
                Some(Ok((
                    Stream::wrap(std::mem::transmute_copy(&self.context), packet.stream_index()),
                    packet,
                )))
            },
            Err(RsmpegError::BufferSinkEofError) => None,
            Err(e) => Some(Err(e)),
        }
    }
}
