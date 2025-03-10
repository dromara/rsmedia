use std::ffi::{CStr, CString, OsStr, OsString};
use std::os::raw::c_char;
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
    s.to_str().map(String::from).unwrap()
}

/// OcString 转换为 CString
pub fn from_os_str(path_or_url: impl AsRef<OsStr>) -> CString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        CString::new(path_or_url.as_ref().as_bytes()).unwrap()
    }
    #[cfg(not(unix))]
    {
        CString::new(path_or_url.as_ref().to_string_lossy().as_bytes()).unwrap()
    }
}

/// 从 CStr 转换为 OsString
pub fn to_os_string(cstr: impl AsRef<CStr>) -> OsString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(cstr.as_ref().to_bytes().to_vec())
    }
    #[cfg(not(unix))]
    {
        OsString::from(cstr.as_ref().to_string_lossy().into_owned())
    }
}

/// 将 C 字符串指针转换为 Rust 字符串引用
///
/// # Safety
///
/// - 指针必须指向一个有效的以 null 结尾的 C 字符串
/// - 字符串内容必须是有效的 UTF-8
pub unsafe fn from_c_char(ptr: *const c_char) -> String {
    CStr::from_ptr(ptr).to_string_lossy().to_string()
}

/// 将 Rust 字符串转换为 C 字符串指针
///
/// # Safety
///
/// 返回的指针需要手动释放，否则会造成内存泄漏
/// 使用 `free_cstr` 函数释放内存
pub unsafe fn to_c_char(s: &str) -> *mut c_char {
    CString::new(s).unwrap().into_raw()
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

        // 从 &OsStr
        let os_str = "/usr/local/bin";
        let cstring = from_os_str(os_str);
        assert_eq!(cstring.to_str().unwrap(), os_str);

        // to OsString
        let cstring = CString::new(os_str).unwrap();
        let os_string = to_os_string(cstring);
        assert_eq!(os_string.to_str().unwrap(), os_str);
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
