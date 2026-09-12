use std::ffi::{CStr, CString, OsStr, OsString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};

/// &Path -> &Cstr
pub fn path_to_cstring<P: AsRef<Path> + ?Sized>(path: &P) -> CString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        CString::new(path.as_ref().as_os_str().as_bytes()).unwrap()
    }

    #[cfg(not(unix))]
    {
        // Windows 下 OsStr 内部为 WTF-8，to_string_lossy() 可直接得到 UTF-8 字节。
        // 与 os_str_to_cstring 保持一致，避免 UTF-16 (from_utf16_lossy) 的有损往返。
        CString::new(path.as_ref().as_os_str().to_string_lossy().as_bytes()).unwrap()
    }
}

/// Option<&Path> -> `Option<CString>`
pub fn path_to_cstring_opt<P: AsRef<Path> + ?Sized>(path: Option<&P>) -> Option<CString> {
    path.map(path_to_cstring)
}

/// &Cstr -> 路径
/// - Unix: 使用原始字节直接构造路径（允许任意字节）
/// - Windows: 输入视为 UTF-8 字节序列（与 [`path_to_cstring`]、[`os_str_to_cstring`] 的编码一致）
///
/// 返回拥有所有权的 [`PathBuf`]：Windows 上把任意字节安全地映射为合法路径需要分配，
/// 无法再借用返回 `&Path`。
pub fn cstr_to_path<C: AsRef<CStr> + ?Sized>(cstr: &C) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        PathBuf::from(OsStr::from_bytes(cstr.as_ref().to_bytes()))
    }

    #[cfg(not(unix))]
    {
        // let bytes = cstr.as_ref().to_bytes();
        // match std::str::from_utf8(bytes) {
        //     Ok(s) => Path::new(s),
        //     Err(_) => {
        //         // not UTF-8
        //         let os_str = unsafe { OsStr::from_encoded_bytes_unchecked(bytes) };
        //         Path::new(os_str)
        //     }
        // }
        // 按 UTF-8 解码（丢失字节替换为 U+FFFD）。不再使用 from_encoded_bytes_unchecked，
        // 因为直接拼凑出的字节不保证满足 Windows OsStr 的 WTF-8 不变量，属未定义行为。
        PathBuf::from(cstr.as_ref().to_string_lossy().into_owned())
    }
}

/// &str -> CString
pub fn str_to_cstring<S: AsRef<str> + ?Sized>(s: &S) -> CString {
    CString::new(s.as_ref()).unwrap()
}

/// Option<&str> -> `Option<CString>`
pub fn str_to_cstring_opt<S: AsRef<str> + ?Sized>(s: Option<&S>) -> Option<CString> {
    s.map(str_to_cstring)
}

/// &Cstr -> String
pub fn cstr_to_string<C: AsRef<CStr> + ?Sized>(cstr: &C) -> Result<String, std::str::Utf8Error> {
    cstr.as_ref().to_str().map(String::from)
}

/// &Cstr -> String
///
/// 宽松版本：非法 UTF-8 字节会被替换为 `U+FFFD`，始终成功。
pub fn cstr_to_string_lossy<C: AsRef<CStr> + ?Sized>(cstr: &C) -> String {
    cstr.as_ref().to_string_lossy().into_owned()
}

/// OsStr -> String
///
/// 跨平台统一返回可读字符串：Unix 下非法 UTF-8 字节替换为 `U+FFFD`，
/// Windows 下由 WTF-8 转 UTF-8。适用于日志、展示等无需保留原始字节的场景。
pub fn os_str_to_string(os: impl AsRef<OsStr>) -> String {
    os.as_ref().to_string_lossy().into_owned()
}

/// Path -> String
///
/// 便捷封装 [`cstr_to_string`/`os_str_to_string`]：路径转可读字符串（lossy）。
pub fn path_to_string(path: impl AsRef<Path>) -> String {
    os_str_to_string(path.as_ref().as_os_str())
}

/// &str -> OsString
///
/// 显式封装 `OsString::from`，与 `str_to_cstring`/`os_str_to_string` 配对，
/// 明确"从 UTF-8 str 构造平台 OsStr"的意图，避免隐式 `From`。
pub fn str_to_os_string(s: impl AsRef<str>) -> OsString {
    OsString::from(s.as_ref())
}

/// OsStr -> CString
pub fn os_str_to_cstring(path_or_url: impl AsRef<OsStr>) -> CString {
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
pub fn cstr_to_os_string(cstr: impl AsRef<CStr>) -> OsString {
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
pub unsafe fn c_char_to_str(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let cstr = unsafe { CStr::from_ptr(ptr) };
    match cstr.to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => {
            // NOT UTF-8
            cstr.to_string_lossy().into_owned()
        }
    }
}

/// 将逗号分隔的 C 字符串指针转换为 `Vec<String>`，空指针返回空 Vec。
///
/// FFmpeg 格式结构体（如 `AVOutputFormat.extensions`）常用逗号分隔
/// 多个别名或扩展名（如 `"mkv,mka,mks"`）。
///
/// # Safety
///
/// - 指针为 NULL 或指向一个有效的以 null 结尾的 C 字符串
/// - 字符串在调用期间保持有效（FFmpeg 静态结构体始终满足）
pub unsafe fn c_char_to_str_list(ptr: *const c_char) -> Vec<String> {
    if ptr.is_null() {
        return Vec::new();
    }
    let cstr = unsafe { CStr::from_ptr(ptr) };
    cstr.to_string_lossy()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn test_path_conversion() {
        // 使用平台无关的路径分隔符
        let path_str = if cfg!(unix) {
            "/usr/local/bin"
        } else {
            r"C:\Users\local\bin"
        };

        // 从 &str 路径
        let cstring = path_to_cstring(Path::new(path_str));
        assert_eq!(cstring.to_str().unwrap(), path_str);

        // 从 PathBuf
        let path_buf = PathBuf::from(path_str);
        let cstring = path_to_cstring(&path_buf);
        assert_eq!(cstring.to_str().unwrap(), path_str);

        // UTF-8 中文路径 (使用平台特定分隔符)
        let chinese_path = if cfg!(unix) {
            "测试/文件.txt"
        } else {
            r"测试\文件.txt"
        };
        let chinese = CString::new(chinese_path).unwrap();
        let utf8_path = cstr_to_path(&chinese);
        assert_eq!(utf8_path.to_str().unwrap(), chinese_path);

        #[cfg(unix)]
        {
            // NOT UTF-8
            use std::os::unix::ffi::OsStrExt;
            let gbk = CString::new(vec![0xB2, 0xE2, 0xCA, 0xD4, 0x2E, 0x74, 0x78, 0x74]).unwrap();
            let gbk_path = cstr_to_path(&gbk);
            assert_eq!(
                gbk_path.as_os_str().as_bytes(),
                &[0xB2, 0xE2, 0xCA, 0xD4, 0x2E, 0x74, 0x78, 0x74]
            );
        }

        #[cfg(not(unix))]
        {
            use std::os::windows::ffi::{OsStrExt, OsStringExt};
            // Windows 下使用 UTF-16 测试
            let test_str = "测试.txt";
            let path = Path::new(test_str);
            let os_str = path.as_os_str();
            let wide_chars: Vec<u16> = os_str.encode_wide().collect();
            let os_string = OsString::from_wide(&wide_chars);
            let cstring = path_to_cstring(&os_string);
            let result_path = cstr_to_path(&cstring);
            assert_eq!(result_path.to_str().unwrap(), test_str);
        }
    }

    #[test]
    fn test_str_conversion() {
        // 从 &str
        let s = "hello world";
        let cstring = str_to_cstring(s);
        assert_eq!(cstring.to_str().unwrap(), s);

        // 从 String
        let string = String::from("hello world");
        let cstring = str_to_cstring(&string);
        assert_eq!(cstring.to_str().unwrap(), string);

        // 从 &OsStr
        let os_str = "/usr/local/bin";
        let cstring = os_str_to_cstring(os_str);
        assert_eq!(cstring.to_str().unwrap(), os_str);

        // to OsString
        let cstring = CString::new(os_str).unwrap();
        let os_string = cstr_to_os_string(cstring);
        assert_eq!(os_string.to_str().unwrap(), os_str);
    }

    #[test]
    fn test_c_char_to_str() {
        let c = CString::new("hello").unwrap();
        assert_eq!(unsafe { c_char_to_str(c.as_ptr()) }, "hello");
        // null pointer
        assert_eq!(unsafe { c_char_to_str(std::ptr::null()) }, "");
    }

    #[test]
    fn test_c_char_to_str_list() {
        let c = CString::new("mkv,mka,mks").unwrap();
        assert_eq!(
            unsafe { c_char_to_str_list(c.as_ptr()) },
            vec!["mkv".to_string(), "mka".to_string(), "mks".to_string()]
        );
        // null pointer
        assert!(unsafe { c_char_to_str_list(std::ptr::null()) }.is_empty());
        // single value
        let c = CString::new("mp4").unwrap();
        assert_eq!(
            unsafe { c_char_to_str_list(c.as_ptr()) },
            vec!["mp4".to_string()]
        );
    }

    #[test]
    fn test_optional_conversion() {
        // 使用平台特定的路径
        let test_path = if cfg!(unix) {
            "/usr/local"
        } else {
            r"C:\Users\local"
        };

        // Optional Path
        let path: Option<&Path> = Some(Path::new(test_path));
        let cstring = path_to_cstring_opt(path);
        assert!(cstring.is_some());
        assert_eq!(cstring.unwrap().to_str().unwrap(), test_path);

        // Optional str
        let s: Option<&str> = Some("hello");
        let cstring = str_to_cstring_opt(s);
        assert!(cstring.is_some());
        assert_eq!(cstring.unwrap().to_str().unwrap(), "hello");

        // None cases
        let none_path: Option<&Path> = None;
        assert!(path_to_cstring_opt(none_path).is_none());

        let none_str: Option<&str> = None;
        assert!(str_to_cstring_opt(none_str).is_none());
    }

    #[test]
    fn test_cstr_to_string_lossy() {
        // 合法 UTF-8
        let ok = CString::new("hello").unwrap();
        assert_eq!(cstr_to_string_lossy(&ok), "hello");

        // 非法 UTF-8 字节被替换为 U+FFFD（\u{FFFD}）
        let bad_bytes = CString::new(vec![0xFF, 0xFE]).unwrap();
        let got = cstr_to_string_lossy(&bad_bytes);
        assert_eq!(got, "\u{FFFD}\u{FFFD}");

        // 与严格版本在有效输入上一致
        assert_eq!(cstr_to_string_lossy(&ok), cstr_to_string(&ok).unwrap());
    }

    #[test]
    fn test_os_str_and_path_to_string() {
        let s = "媒体/文件.txt";
        // OsStr -> String
        assert_eq!(os_str_to_string(s), s);
        // Path -> String
        assert_eq!(path_to_string(Path::new(s)), s);
        // PathBuf -> String
        assert_eq!(path_to_string(PathBuf::from(s)), s);
        // 断言 os_str/path_to_string 与 lossy 一致（含非法字节时也往返为 lossy 字符串）
        assert_eq!(os_str_to_string(PathBuf::from(s).as_os_str()), s);
    }

    #[test]
    fn test_str_to_os_string() {
        let s = "测试字符串";
        let os = str_to_os_string(s);
        // OsString 可安全转回 str
        assert_eq!(os.to_str(), Some(s));

        // 与 OsString::from 等价
        assert_eq!(str_to_os_string("path/to/x"), OsString::from("path/to/x"));
    }
}
