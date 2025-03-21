use std::ffi::{CStr, CString, OsStr, OsString};
use std::os::raw::c_char;
use std::path::Path;

/// &Path -> &Cstr
pub fn from_path<P: AsRef<Path> + ?Sized>(path: &P) -> CString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        CString::new(path.as_ref().as_os_str().as_bytes()).unwrap()
    }

    #[cfg(not(unix))]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = path.as_ref().as_os_str().encode_wide().collect();
        let bytes: Vec<u8> = wide.iter().flat_map(|c| c.to_le_bytes().to_vec()).collect();
        CString::new(bytes).unwrap()
    }
}

/// Option<&Path> -> Option<CString>
pub fn from_path_opt<P: AsRef<Path> + ?Sized>(path: Option<&P>) -> Option<CString> {
    path.map(from_path)
}

/// &Cstr -> &Path
/// - Unix: 使用原始字节直接构造路径（允许任意字节）
/// - Windows: 假设输入为 UTF-16 LE 编码的字节序列
pub fn to_path<C: AsRef<CStr> + ?Sized>(cstr: &C) -> &Path {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Path::new(OsStr::from_bytes(cstr.as_ref().to_bytes()))
    }

    #[cfg(not(unix))]
    {
        let bytes = cstr.as_ref().to_bytes();
        match std::str::from_utf8(bytes) {
            Ok(s) => Path::new(s),
            Err(_) => {
                // not UTF-8
                let os_str = unsafe { OsStr::from_encoded_bytes_unchecked(bytes) };
                Path::new(os_str)
            }
        }
    }
}

/// &str -> CString
pub fn from_str<S: AsRef<str> + ?Sized>(s: &S) -> CString {
    CString::new(s.as_ref()).unwrap()
}

/// Option<&str> -> Option<CString>
pub fn from_str_opt<S: AsRef<str> + ?Sized>(s: Option<&S>) -> Option<CString> {
    s.map(from_str)
}

/// &Cstr -> String
pub fn to_string<C: AsRef<CStr> + ?Sized>(cstr: &C) -> Result<String, std::str::Utf8Error> {
    cstr.as_ref().to_str().map(String::from)
}

/// OsStr -> CString
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

/// CStr -> OsString
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
    if ptr.is_null() {
        return String::new();
    }
    let cstr = CStr::from_ptr(ptr);
    match cstr.to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => {
            // NOT UTF-8
            cstr.to_string_lossy().into_owned()
        }
    }
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

        // UTF-8 中文路径
        let chinese = CString::new("测试/文件.txt").unwrap();
        let utf8_path = to_path(&chinese);
        assert_eq!(utf8_path.to_str().unwrap(), "测试/文件.txt");

        // GBK编码路径测试 (测试.txt)
        let gbk = CString::new(vec![0xB2, 0xE2, 0xCA, 0xD4, 0x2E, 0x74, 0x78, 0x74]).unwrap();
        let gbk_path = to_path(&gbk);
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            assert_eq!(
                gbk_path.as_os_str().as_bytes(),
                &[0xB2, 0xE2, 0xCA, 0xD4, 0x2E, 0x74, 0x78, 0x74]
            );
        }

        #[cfg(not(unix))]
        {
            // Windows平台：比较转换后的字符串
            // GBK "测试.txt" 在Windows中应该正确显示
            assert_eq!(gbk_path, Path::new("测试.txt"));
        }
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
        let cstring = from_path_opt(path);
        assert!(cstring.is_some());
        assert_eq!(cstring.unwrap().to_str().unwrap(), "/usr/local");

        // Optional str
        let s: Option<&str> = Some("hello");
        let cstring = from_str_opt(s);
        assert!(cstring.is_some());
        assert_eq!(cstring.unwrap().to_str().unwrap(), "hello");

        // None cases
        let none_path: Option<&Path> = None;
        assert!(from_path_opt(none_path).is_none());

        let none_str: Option<&str> = None;
        assert!(from_str_opt(none_str).is_none());
    }
}
