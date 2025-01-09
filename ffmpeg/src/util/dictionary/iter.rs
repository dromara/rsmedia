use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::ptr;
use std::str::from_utf8_unchecked;

use rsmpeg::ffi;

pub struct Iter<'a> {
    ptr: *const ffi::AVDictionary,
    cur: *mut ffi::AVDictionaryEntry,

    _marker: PhantomData<&'a ()>,
}

impl<'a> Iter<'a> {
    pub fn new(dictionary: *const ffi::AVDictionary) -> Self {
        Iter {
            ptr: dictionary,
            cur: ptr::null_mut(),

            _marker: PhantomData,
        }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        unsafe {
            let empty = CString::new("").unwrap();
            let entry = ffi::av_dict_get(
                self.ptr,
                empty.as_ptr(),
                self.cur,
                ffi::AV_DICT_IGNORE_SUFFIX as i32,
            );

            if !entry.is_null() {
                let key = from_utf8_unchecked(CStr::from_ptr((*entry).key).to_bytes());
                let val = from_utf8_unchecked(CStr::from_ptr((*entry).value).to_bytes());

                self.cur = entry;

                Some((key, val))
            } else {
                None
            }
        }
    }
}
