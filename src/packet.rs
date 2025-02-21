use crate::flags::AvPacketFlags;
use crate::stream::Stream;
use crate::time::Time;
use crate::Rational;

use rsmpeg::avcodec::AVPacket;
use rsmpeg::avformat::{AVFormatContextInput, AVFormatContextOutput};
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi;

use std::marker::PhantomData;

use libc::{c_int, c_uint};

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

    /// Get packet DTS (decoder timestamp).
    #[inline]
    pub fn dts(&self) -> Time {
        Time::new(Some(self.inner.dts), self.time_base)
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

    /// Get packet duration.
    #[inline]
    pub fn duration(&self) -> Time {
        Time::new(Some(self.inner.duration), self.time_base)
    }

    /// Set duration.
    #[inline]
    pub fn set_duration(&mut self, timestamp: Time) {
        if let Some(duration) = timestamp.aligned_with_rational(self.time_base).into_value() {
            self.inner.set_duration(duration);
        }
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
    pub fn pos(&self) -> isize {
        self.inner.pos as isize
    }

    #[inline]
    pub fn set_pos(&mut self, value: i64) {
        self.inner.set_pos(value)
    }

    #[inline]
    pub fn flags(&self) -> AvPacketFlags {
        AvPacketFlags::from_bits_truncate(self.inner.flags as c_uint)
    }

    #[inline]
    pub fn set_flags(&mut self, flag: i32) {
        self.inner.set_flags(flag);
    }

    #[inline]
    pub fn set_shrink(&mut self, size: usize) {
        unsafe {
            ffi::av_shrink_packet(self.inner.as_mut_ptr(), size as c_int);
        }
    }

    #[inline]
    pub fn set_grow(&mut self, size: usize) {
        unsafe {
            ffi::av_grow_packet(self.inner.as_mut_ptr(), size as c_int);
        }
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.inner.size as usize
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.inner.size == 0
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
    pub fn side_data(&self) -> PacketSideDataIter {
        PacketSideDataIter::new(&self.inner)
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
    pub fn read(&mut self, format: &mut AVFormatContextInput) -> Result<(), RsmpegError> {
        unsafe {
            match ffi::av_read_frame(format.as_mut_ptr(), self.inner.as_mut_ptr()) {
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

    // 获取可变引用
    pub fn as_inner(&mut self) -> &mut AVPacket {
        &mut self.inner
    }

    // 获取不可变引用
    pub fn as_inner_ref(&self) -> &AVPacket {
        &self.inner
    }

    /// Downcast to native inner type.
    pub(crate) fn into_inner(self) -> AVPacket {
        self.inner
    }

    /// Downcast to native inner type and time base.
    pub(crate) fn into_inner_parts(self) -> (AVPacket, Rational) {
        (self.inner, self.time_base)
    }

    pub fn time_base(&self) -> Rational {
        self.time_base
    }

    /////////////////////

    #[inline]
    pub fn copy(data: &[u8]) -> Self {
        use std::io::Write;

        let mut packet = Packet::new_with_size(data.len());
        packet.data_mut().unwrap().write_all(data).unwrap();

        packet
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

    #[inline]
    pub fn empty() -> Self {
        Packet::new_with_avpacket(AVPacket::new())
    }

    #[inline]
    pub fn new_with_avpacket(pkt: AVPacket) -> Self {
        Packet {
            inner: pkt,
            time_base: Rational::new(1, 30 * 1000),
        }
    }

    #[inline]
    pub fn new_with_size(size: usize) -> Self {
        unsafe {
            let mut pkt = std::mem::MaybeUninit::<ffi::AVPacket>::uninit();

            ffi::av_init_packet(pkt.as_mut_ptr());
            ffi::av_new_packet(pkt.as_mut_ptr(), size as c_int);

            Packet::new_with_avpacket(AVPacket::from_raw(
                std::ptr::NonNull::new(pkt.as_mut_ptr()).unwrap(),
            ))
        }
    }
}

unsafe impl Send for Packet {}
unsafe impl Sync for Packet {}

impl Clone for Packet {
    #[inline]
    fn clone(&self) -> Self {
        let mut pkt = Packet::empty();
        pkt.clone_from(self);

        pkt
    }

    #[inline]
    fn clone_from(&mut self, source: &Self) {
        unsafe {
            let pkt = ffi::av_packet_clone(source.inner.as_ptr());
            self.inner.set_ptr(std::ptr::NonNull::new(pkt).unwrap())
        }
    }
}

//////////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////////

pub struct PacketSideDataIter<'a> {
    ptr: *const AVPacket,
    cur: c_int,
    _marker: PhantomData<&'a Packet>,
}

impl PacketSideDataIter<'_> {
    pub fn new(ptr: *const AVPacket) -> Self {
        PacketSideDataIter {
            ptr,
            cur: 0,
            _marker: PhantomData,
        }
    }
}

impl<'a> Iterator for PacketSideDataIter<'a> {
    type Item = PacketSideData<'a>;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        unsafe {
            if self.cur >= (*self.ptr).side_data_elems {
                None
            } else {
                self.cur += 1;
                Some(PacketSideData::wrap(
                    (*self.ptr).side_data.offset((self.cur - 1) as isize),
                ))
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        unsafe {
            let length = (*self.ptr).side_data_elems as usize;

            (length - self.cur as usize, Some(length - self.cur as usize))
        }
    }
}

impl ExactSizeIterator for PacketSideDataIter<'_> {}

pub struct PacketSideData<'a> {
    ptr: *mut ffi::AVPacketSideData,
    _marker: PhantomData<&'a Packet>,
}

impl PacketSideData<'_> {
    pub fn wrap(ptr: *mut ffi::AVPacketSideData) -> Self {
        PacketSideData {
            ptr,
            _marker: PhantomData,
        }
    }

    pub fn as_ptr(&self) -> *const ffi::AVPacketSideData {
        self.ptr as *const _
    }
}

impl PacketSideData<'_> {
    pub fn kind(&self) -> ffi::AVPacketSideDataType {
        unsafe { (*self.as_ptr()).type_ }
    }

    pub fn size(&self) -> usize {
        unsafe { (*self.as_ptr()).size }
    }

    pub fn data(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts((*self.as_ptr()).data, (*self.as_ptr()).size) }
    }
}

//////////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////////

pub struct PacketIter<'a> {
    context: &'a mut AVFormatContextInput,
}

impl PacketIter<'_> {
    pub fn new(context: &mut AVFormatContextInput) -> PacketIter {
        PacketIter { context }
    }
}

impl<'a> Iterator for PacketIter<'a> {
    type Item = Result<(Stream<'a>, Packet), RsmpegError>;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        let mut packet = Packet::empty();

        match packet.read(self.context) {
            Ok(..) => unsafe {
                Some(Ok((
                    Stream::wrap(
                        std::mem::transmute_copy(&self.context),
                        packet.stream_index(),
                    ),
                    packet,
                )))
            },
            Err(RsmpegError::BufferSinkEofError) => None,
            Err(e) => Some(Err(e)),
        }
    }
}
