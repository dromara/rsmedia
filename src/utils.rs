use std::ffi::{CStr, CString, OsStr};
use std::path::Path;

/// 从任意实现了 AsRef<Path> 的类型转换为 CString
pub fn from_path<P: AsRef<Path> + ?Sized>(path: &P) -> CString {
    CString::new(path.as_ref().as_os_str().to_str().unwrap()).unwrap()
}

/// 从任意实现了 AsRef<str> 的类型转换为 CString
pub fn from_str<S: AsRef<str> + ?Sized>(s: &S) -> CString {
    CString::new(s.as_ref()).unwrap()
}

/// 从任意实现了 AsRef<Path> 的类型转换为 CString，返回 Option
pub fn path_opt<P: AsRef<Path> + ?Sized>(path: Option<&P>) -> Option<CString> {
    path.map(from_path)
}

/// 从任意实现了 AsRef<str> 的类型转换为 CString，返回 Option
pub fn str_opt<S: AsRef<str> + ?Sized>(s: Option<&S>) -> Option<CString> {
    s.map(from_str)
}

/// 从 CStr 转换为 String，提供默认值
pub fn to_string(s: &CStr) -> String {
    s.to_str().map(String::from).unwrap_or_default()
}

#[cfg(unix)]
pub fn from_os_str(path_or_url: impl AsRef<OsStr>) -> CString {
    use std::os::unix::ffi::OsStrExt;
    CString::new(path_or_url.as_ref().as_bytes()).unwrap()
}

#[cfg(not(unix))]
pub fn from_os_str(path_or_url: impl AsRef<OsStr>) -> CString {
    CString::new(path_or_url.as_ref().to_string_lossy().as_bytes()).unwrap()
}

/// `ptr` must be non-null and valid.
/// Ensure that the returned lifetime is correctly bounded.
/// # Safety
#[inline]
pub unsafe fn str_from_c_ptr<'s>(ptr: *const libc::c_char) -> &'s str {
    unsafe { std::str::from_utf8_unchecked(CStr::from_ptr(ptr).to_bytes()) }
}

/// `ptr` must be null or valid.
/// Ensure that the returned lifetime is correctly bounded.
/// # Safety
#[inline]
pub unsafe fn str_from_c_ptr_opt<'s>(ptr: *const libc::c_char) -> Option<&'s str> {
    if ptr.is_null() { None } else { Some(str_from_c_ptr(ptr)) }
}

// 使用示例
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn test_path_conversion() {
        // 从 &str 路径
        let path_str = "/usr/local/bin";
        let cstring = from_path(Path::new(path_str));
        assert_eq!(cstring.to_str().unwrap(), path_str);

        // 从 PathBuf
        let path_buf = PathBuf::from("/usr/local/bin");
        let cstring = from_path(&path_buf);
        assert_eq!(cstring.to_str().unwrap(), path_str);
    }

    #[test]
    fn test_str_conversion() {
        // 从 &str
        let s = "hello world";
        let cstring = from_str(s);
        assert_eq!(cstring.to_str().unwrap(), s);

        // 从 String
        let string = String::from("hello world");
        let cstring = from_str(&string);
        assert_eq!(cstring.to_str().unwrap(), string);
    }

    #[test]
    fn test_optional_conversion() {
        // Optional Path
        let path: Option<&Path> = Some(Path::new("/usr/local"));
        let cstring = path_opt(path);
        assert!(cstring.is_some());
        assert_eq!(cstring.unwrap().to_str().unwrap(), "/usr/local");

        // Optional str
        let s: Option<&str> = Some("hello");
        let cstring = str_opt(s);
        assert!(cstring.is_some());
        assert_eq!(cstring.unwrap().to_str().unwrap(), "hello");

        // None cases
        let none_path: Option<&Path> = None;
        assert!(path_opt(none_path).is_none());

        let none_str: Option<&str> = None;
        assert!(str_opt(none_str).is_none());
    }
}
