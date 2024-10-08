pub mod flag;
pub use self::flag::Flags;

mod rect;
pub use self::rect::{Ass, Bitmap, Rect, Text};

mod rect_mut;
pub use self::rect_mut::{AssMut, BitmapMut, RectMut, TextMut};

use std::marker::PhantomData;
use std::mem;

use rsmpeg::ffi;
use libc::{c_uint, size_t};

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Type {
    None,
    Bitmap,
    Text,
    Ass,
}

impl From<ffi::AVSubtitleType> for Type {
    fn from(value: ffi::AVSubtitleType) -> Type {
        match value {
            ffi::SUBTITLE_NONE => Type::None,
            ffi::SUBTITLE_BITMAP => Type::Bitmap,
            ffi::SUBTITLE_TEXT => Type::Text,
            ffi::SUBTITLE_ASS => Type::Ass,

            // non-exhaustive patterns: `4_u32..=u32::MAX` not covered
            4_u32..=u32::MAX => todo!(),
        }
    }
}

impl From<Type> for ffi::AVSubtitleType {
    fn from(value: Type) -> ffi::AVSubtitleType {
        match value {
            Type::None => ffi::SUBTITLE_NONE,
            Type::Bitmap => ffi::SUBTITLE_BITMAP,
            Type::Text => ffi::SUBTITLE_TEXT,
            Type::Ass => ffi::SUBTITLE_ASS,
        }
    }
}

pub struct Subtitle(ffi::AVSubtitle);

impl Subtitle {
    pub unsafe fn as_ptr(&self) -> *const ffi::AVSubtitle {
        &self.0
    }

    pub unsafe fn as_mut_ptr(&mut self) -> *mut ffi::AVSubtitle {
        &mut self.0
    }
}

impl Subtitle {
    pub fn new() -> Self {
        unsafe { Subtitle(mem::zeroed()) }
    }

    pub fn pts(&self) -> Option<i64> {
        match self.0.pts {
            ffi::AV_NOPTS_VALUE => None,
            pts => Some(pts),
        }
    }

    pub fn set_pts(&mut self, value: Option<i64>) {
        self.0.pts = value.unwrap_or(ffi::AV_NOPTS_VALUE);
    }

    pub fn start(&self) -> u32 {
        self.0.start_display_time
    }

    pub fn set_start(&mut self, value: u32) {
        self.0.start_display_time = value;
    }

    pub fn end(&self) -> u32 {
        self.0.end_display_time
    }

    pub fn set_end(&mut self, value: u32) {
        self.0.end_display_time = value;
    }

    pub fn rects(&self) -> RectIter {
        RectIter::new(&self.0)
    }

    pub fn rects_mut(&mut self) -> RectMutIter {
        RectMutIter::new(&mut self.0)
    }

    pub fn add_rect(&mut self, kind: Type) -> RectMut {
        unsafe {
            self.0.num_rects += 1;
            self.0.rects = ffi::av_realloc(
                self.0.rects as *mut _,
                (mem::size_of::<*const ffi::AVSubtitleRect>() * self.0.num_rects as usize) as size_t,
            ) as *mut _;

            let rect =
                ffi::av_mallocz(mem::size_of::<ffi::AVSubtitleRect>() as size_t) as *mut ffi::AVSubtitleRect;
            (*rect).type_ = kind.into();

            *self.0.rects.offset((self.0.num_rects - 1) as isize) = rect;

            RectMut::wrap(rect)
        }
    }
}

impl Default for Subtitle {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RectIter<'a> {
    ptr: *const ffi::AVSubtitle,
    cur: c_uint,

    _marker: PhantomData<&'a Subtitle>,
}

impl<'a> RectIter<'a> {
    pub fn new(ptr: *const ffi::AVSubtitle) -> Self {
        RectIter {
            ptr,
            cur: 0,
            _marker: PhantomData,
        }
    }
}

impl<'a> Iterator for RectIter<'a> {
    type Item = Rect<'a>;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        unsafe {
            if self.cur >= (*self.ptr).num_rects {
                None
            } else {
                self.cur += 1;
                Some(Rect::wrap(
                    *(*self.ptr).rects.offset((self.cur - 1) as isize),
                ))
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        unsafe {
            let length = (*self.ptr).num_rects as usize;

            (length - self.cur as usize, Some(length - self.cur as usize))
        }
    }
}

impl<'a> ExactSizeIterator for RectIter<'a> {}

pub struct RectMutIter<'a> {
    ptr: *mut ffi::AVSubtitle,
    cur: c_uint,

    _marker: PhantomData<&'a Subtitle>,
}

impl<'a> RectMutIter<'a> {
    pub fn new(ptr: *mut ffi::AVSubtitle) -> Self {
        RectMutIter {
            ptr,
            cur: 0,
            _marker: PhantomData,
        }
    }
}

impl<'a> Iterator for RectMutIter<'a> {
    type Item = RectMut<'a>;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        unsafe {
            if self.cur >= (*self.ptr).num_rects {
                None
            } else {
                self.cur += 1;
                Some(RectMut::wrap(
                    *(*self.ptr).rects.offset((self.cur - 1) as isize),
                ))
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        unsafe {
            let length = (*self.ptr).num_rects as usize;

            (length - self.cur as usize, Some(length - self.cur as usize))
        }
    }
}

impl<'a> ExactSizeIterator for RectMutIter<'a> {}
