use std::fmt;
use std::iter::FromIterator;
use std::ops::{Deref, DerefMut};
use std::ptr;

use super::mutable;
use sys::ffi;

pub struct Owned<'a> {
    inner: mutable::Ref<'a>,
}

impl Default for Owned<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl Owned<'_> {
    pub unsafe fn own(ptr: *mut ffi::AVDictionary) -> Self {
        Owned {
            inner: mutable::Ref::wrap(ptr),
        }
    }

    pub unsafe fn disown(mut self) -> *mut ffi::AVDictionary {
        let result = self.inner.as_mut_ptr();
        self.inner = mutable::Ref::wrap(ptr::null_mut());

        result
    }
}

impl Owned<'_> {
    pub fn new() -> Self {
        unsafe {
            Owned {
                inner: mutable::Ref::wrap(ptr::null_mut()),
            }
        }
    }
}

impl<'b> FromIterator<(&'b str, &'b str)> for Owned<'_> {
    fn from_iter<T: IntoIterator<Item = (&'b str, &'b str)>>(iterator: T) -> Self {
        let mut result = Owned::new();

        for (key, value) in iterator {
            result.set(key, value);
        }

        result
    }
}

impl<'b> FromIterator<&'b (&'b str, &'b str)> for Owned<'_> {
    fn from_iter<T: IntoIterator<Item = &'b (&'b str, &'b str)>>(iterator: T) -> Self {
        let mut result = Owned::new();

        for &(key, value) in iterator {
            result.set(key, value);
        }

        result
    }
}

impl FromIterator<(String, String)> for Owned<'_> {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(iterator: T) -> Self {
        let mut result = Owned::new();

        for (key, value) in iterator {
            result.set(&key, &value);
        }

        result
    }
}

impl<'b> FromIterator<&'b (String, String)> for Owned<'_> {
    fn from_iter<T: IntoIterator<Item = &'b (String, String)>>(iterator: T) -> Self {
        let mut result = Owned::new();

        for (key, value) in iterator {
            result.set(key, value);
        }

        result
    }
}

impl<'a> Deref for Owned<'a> {
    type Target = mutable::Ref<'a>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Owned<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Clone for Owned<'_> {
    fn clone(&self) -> Self {
        let mut dictionary = Owned::new();
        dictionary.clone_from(self);

        dictionary
    }

    fn clone_from(&mut self, source: &Self) {
        unsafe {
            let mut ptr = self.as_mut_ptr();
            ffi::av_dict_copy(&mut ptr, source.as_ptr(), 0);
            self.inner = mutable::Ref::wrap(ptr);
        }
    }
}

impl Drop for Owned<'_> {
    fn drop(&mut self) {
        unsafe {
            ffi::av_dict_free(&mut self.inner.as_mut_ptr());
        }
    }
}

impl fmt::Debug for Owned<'_> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.inner.fmt(fmt)
    }
}
