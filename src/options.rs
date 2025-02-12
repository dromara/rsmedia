use rsmpeg::avutil::AVDictionary;
use rsmpeg::ffi;
use std::collections::HashMap;
use std::ffi::{c_int, CStr, CString};
use std::fmt;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::str::from_utf8_unchecked;

/// Dictionary micro
#[macro_export]
macro_rules! dict {
	( $($key:expr => $value:expr),* $(,)*) => ({
			let mut dict = ::Dictionary::new();

			$(
				dict.set($key, $value);
			)*

			dict
		}
	);
}

///////////////////////////////////////////////////////////////////////////////
///////////////////////////////////////////////////////////////////////////////
/// Iter
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
                ffi::AV_DICT_IGNORE_SUFFIX as c_int,
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

///////////////////////////////////////////////////////////////////////////////
///////////////////////////////////////////////////////////////////////////////
/// immutable
pub struct ImmutableRef<'a> {
    ptr: *const ffi::AVDictionary,
    _marker: PhantomData<&'a ()>,
}

impl<'a> ImmutableRef<'a> {
    pub unsafe fn wrap(ptr: *const ffi::AVDictionary) -> Self {
        ImmutableRef {
            ptr,
            _marker: PhantomData,
        }
    }

    pub unsafe fn as_ptr(&self) -> *const ffi::AVDictionary {
        self.ptr
    }
}

impl<'a> ImmutableRef<'a> {
    pub fn get(&'a self, key: &str) -> Option<&'a str> {
        unsafe {
            let key = CString::new(key).unwrap();
            let entry = ffi::av_dict_get(self.as_ptr(), key.as_ptr(), ptr::null_mut(), 0);

            if entry.is_null() {
                None
            } else {
                Some(from_utf8_unchecked(
                    CStr::from_ptr((*entry).value).to_bytes(),
                ))
            }
        }
    }

    pub fn iter(&self) -> Iter {
        unsafe { Iter::new(self.as_ptr()) }
    }

    pub fn to_owned<'b>(&self) -> Owned<'b> {
        self.iter().collect()
    }
}

impl<'a> IntoIterator for &'a ImmutableRef<'a> {
    type Item = (&'a str, &'a str);
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> fmt::Debug for ImmutableRef<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_map().entries(self.iter()).finish()
    }
}

///////////////////////////////////////////////////////////////////////////////
///////////////////////////////////////////////////////////////////////////////
/// mutable
pub struct MutableRef<'a> {
    ptr: *mut ffi::AVDictionary,
    imm: ImmutableRef<'a>,
    _marker: PhantomData<&'a ()>,
}

impl<'a> MutableRef<'a> {
    pub unsafe fn wrap(ptr: *mut ffi::AVDictionary) -> Self {
        MutableRef {
            ptr,
            imm: ImmutableRef::wrap(ptr),
            _marker: PhantomData,
        }
    }

    pub unsafe fn as_mut_ptr(&self) -> *mut ffi::AVDictionary {
        self.ptr
    }
}

impl<'a> MutableRef<'a> {
    pub fn set(&mut self, key: &str, value: &str) {
        unsafe {
            let key = CString::new(key).unwrap();
            let value = CString::new(value).unwrap();
            let mut ptr = self.as_mut_ptr();

            if ffi::av_dict_set(&mut ptr, key.as_ptr(), value.as_ptr(), 0) < 0 {
                panic!("out of memory");
            }

            self.ptr = ptr;
            self.imm = ImmutableRef::wrap(ptr);
        }
    }
}

impl<'a> Deref for MutableRef<'a> {
    type Target = ImmutableRef<'a>;

    fn deref(&self) -> &Self::Target {
        &self.imm
    }
}

impl<'a> fmt::Debug for MutableRef<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.imm.fmt(fmt)
    }
}

///////////////////////////////////////////////////////////////////////////////
///////////////////////////////////////////////////////////////////////////////
/// owned
pub struct Owned<'a> {
    inner: MutableRef<'a>,
}

impl<'a> Default for Owned<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Owned<'a> {
    pub unsafe fn own(ptr: *mut ffi::AVDictionary) -> Self {
        Owned {
            inner: MutableRef::wrap(ptr),
        }
    }

    pub unsafe fn disown(mut self) -> *mut ffi::AVDictionary {
        let result = self.inner.as_mut_ptr();
        self.inner = MutableRef::wrap(ptr::null_mut());

        result
    }
}

impl<'a> Owned<'a> {
    pub fn new() -> Self {
        unsafe {
            Owned {
                inner: MutableRef::wrap(ptr::null_mut()),
            }
        }
    }
}

impl<'a, 'b> FromIterator<(&'b str, &'b str)> for Owned<'a> {
    fn from_iter<T: IntoIterator<Item = (&'b str, &'b str)>>(iterator: T) -> Self {
        let mut result = Owned::new();

        for (key, value) in iterator {
            result.set(key, value);
        }

        result
    }
}

impl<'a, 'b> FromIterator<&'b (&'b str, &'b str)> for Owned<'a> {
    fn from_iter<T: IntoIterator<Item = &'b (&'b str, &'b str)>>(iterator: T) -> Self {
        let mut result = Owned::new();

        for &(key, value) in iterator {
            result.set(key, value);
        }

        result
    }
}

impl<'a> FromIterator<(String, String)> for Owned<'a> {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(iterator: T) -> Self {
        let mut result = Owned::new();

        for (key, value) in iterator {
            result.set(&key, &value);
        }

        result
    }
}

impl<'a, 'b> FromIterator<&'b (String, String)> for Owned<'a> {
    fn from_iter<T: IntoIterator<Item = &'b (String, String)>>(iterator: T) -> Self {
        let mut result = Owned::new();

        for (key, value) in iterator {
            result.set(key, value);
        }

        result
    }
}

impl<'a> Deref for Owned<'a> {
    type Target = MutableRef<'a>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> DerefMut for Owned<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a> Clone for Owned<'a> {
    fn clone(&self) -> Self {
        let mut dictionary = Owned::new();
        dictionary.clone_from(self);

        dictionary
    }

    fn clone_from(&mut self, source: &Self) {
        unsafe {
            let mut ptr = self.as_mut_ptr();
            ffi::av_dict_copy(&mut ptr, source.as_ptr(), 0);
            self.inner = MutableRef::wrap(ptr);
        }
    }
}

impl<'a> Drop for Owned<'a> {
    fn drop(&mut self) {
        unsafe {
            ffi::av_dict_free(&mut self.inner.as_mut_ptr());
        }
    }
}

impl<'a> fmt::Debug for Owned<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.inner.fmt(fmt)
    }
}

///////////////////////////////////////////////////////////////////////////////
///////////////////////////////////////////////////////////////////////////////

pub use ImmutableRef as DictionaryRef;
pub use MutableRef as DictionaryMut;
pub use Owned as Dictionary;

/// A wrapper type for ffmpeg options.
#[derive(Debug, Clone)]
pub struct Options(Dictionary<'static>);

impl Options {
    /// Creates options such that ffmpeg will prefer TCP transport when reading RTSP stream (over
    /// the default UDP format).
    ///
    /// This sets the `rtsp_transport` to `tcp` in ffmpeg options.
    pub fn preset_rtsp_transport_tcp() -> Self {
        let mut opts = Dictionary::new();
        opts.set("rtsp_transport", "tcp");

        Self(opts)
    }

    /// Creates options such that ffmpeg will prefer TCP transport when reading RTSP stream (over
    /// the default UDP format). It also adds some options to reduce the socket and I/O timeouts to
    /// 4 seconds.
    ///
    /// This sets the `rtsp_transport` to `tcp` in ffmpeg options, it also sets `rw_timeout` to
    /// lower (more sane) values.
    pub fn preset_rtsp_transport_tcp_and_sane_timeouts() -> Self {
        let mut opts = Dictionary::new();
        opts.set("rtsp_transport", "tcp");
        // These can't be too low because ffmpeg takes its sweet time when connecting to RTSP
        // sources sometimes.
        opts.set("rw_timeout", "16000000");
        opts.set("stimeout", "16000000");

        Self(opts)
    }

    /// Creates options such that ffmpeg is instructed to fragment output and mux to fragmented mp4
    /// container format.
    ///
    /// This modifies the `movflags` key to supported fragmented output. The muxer output will not
    /// have a header and each packet contains enough metadata to be streamed without the header.
    /// Muxer output should be compatiable with MSE.
    pub fn preset_fragmented_mov() -> Self {
        let mut opts = Dictionary::new();
        opts.set(
            "movflags",
            "faststart+frag_keyframe+frag_custom+empty_moov+omit_tfhd_offset",
        );

        Self(opts)
    }

    /// Default options for a H264 encoder.
    pub fn preset_h264() -> Self {
        let mut opts = Dictionary::new();
        // Set H264 encoder to the medium preset.
        // preset: 预设编码配置,控制编码速度和质量的平衡
        // - ultrafast,superfast,veryfast,faster,fast
        // - medium (默认)
        // - slow,slower,veryslow
        opts.set("preset", "medium");

        Self(opts)
    }

    /// Options for a H264 encoder that are tuned for low-latency encoding such as for real-time
    /// streaming.
    pub fn preset_h264_realtime() -> Self {
        let mut opts = Dictionary::new();
        // Set H264 encoder to the medium preset.
        // preset: 预设编码配置,控制编码速度和质量的平衡
        // - slow: 更高质量,但编码速度较慢
        // - medium: 默认设置,平衡质量和速度
        // - fast: 更快编码速度,但质量可能略降
        opts.set("preset", "medium");

        // quality: NVENC特定的质量控制模式
        // - high: 质量优先模式,产生最佳画质但占用更多GPU资源
        // - speed: 性能优先模式,平衡质量和编码速度
        // - fast: 低延迟优先模式,最快编码速度但质量较低
        opts.set("quality", "fast");

        // Tune for low latency
        opts.set("tune", "zerolatency");

        Self(opts)
    }

    /// Convert back to ffmpeg native dictionary, which can be used with `ffmpeg` functions.
    pub(super) fn to_dict(&self) -> Dictionary {
        self.0.clone()
    }
}

impl Default for Options {
    fn default() -> Self {
        Self(Dictionary::new())
    }
}

impl From<HashMap<String, String>> for Options {
    /// Converts from `HashMap` to `Options`.
    ///
    /// # Arguments
    ///
    /// * `item` - Item to convert from.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let my_opts = HashMap::new();
    /// options.insert(
    ///     "my_option".to_string(),
    ///     "my_value".to_string(),
    /// );
    ///
    /// let opts: Options = my_opts.into();
    /// ```
    fn from(item: HashMap<String, String>) -> Self {
        let mut opts = Dictionary::new();
        for (k, v) in item {
            opts.set(&k.clone(), &v.clone());
        }

        Self(opts)
    }
}

impl From<Options> for HashMap<String, String> {
    /// Converts from `Options` to `HashMap`.
    ///
    /// # Arguments
    ///
    /// * `item` - Item to convert from.
    fn from(item: Options) -> Self {
        item.0
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
}

unsafe impl Send for Options {}
unsafe impl Sync for Options {}
