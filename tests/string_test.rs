#[test]
fn main() {
    // 1. 普通字符串字面量 (String literals)
    println!(
        "1. &str: {:?}, {:?}, {:?}, {:?}",
        'R', "foo", r"foo", r#"foo "bar" baz"#
    );
    assert_eq!("foo", r"foo");
    assert_eq!("R", r"R");
    assert_eq!("\x52", "R");
    assert_eq!("foo \"bar\" baz", "foo \"bar\" baz");
    assert_eq!("foo \"bar\" baz", r#"foo "bar" baz"#);
    assert_eq!("foo #\"# bar", r##"foo #"# bar"##);

    // 2. 字节字符串字面量 (Byte string literals)
    println!(
        "2. Byte string &[u8]: {:?}, {:?}, {:?}, {:?}",
        b'R', b"foo", br"foo", br#"foo "bar" baz"#
    );
    assert_eq!(b'R', 82_u8);
    assert_eq!(br"foo", b"foo");
    assert_eq!(b"foo \"bar\" baz", br#"foo "bar" baz"#);
    assert_eq!(b"foo #\"# bar", br##"foo #"# bar"##);
    assert_eq!(b"R", br"R");
    assert_eq!(b"\x52", b"R");
    assert_eq!(b"\\x52", br"\x52");

    // 3. C字符串字面量 (C string literals)
    println!("3. &CStr: {:?}, {:?}, {:?}", c"foo", cr"foo", cr#""foo""#);
    assert_eq!(c"foo", cr"foo");
    assert_eq!(c"foo \"bar\" baz", cr#"foo "bar" baz"#);
    assert_eq!(c"\x52", c"R");
    assert_eq!(c"\x52", cr"R");
    assert_eq!(c"\\x52", cr"\x52");

    // C字符串字面量自动添加 null 终止符
    assert_eq!(c"foo".to_bytes(), b"foo");
    assert_eq!(c"foo".to_bytes_with_nul(), b"foo\0");

    // 原始C字符串字面量不处理转义字符
    assert_eq!(cr"foo\0".to_bytes(), b"foo\\0");
    assert_eq!(cr"foo\0".to_bytes_with_nul(), b"foo\\0\0");

    // 带引号的C字符串示例
    assert_eq!(cr#""foo""#.to_bytes(), b"\"foo\"");
    assert_eq!(cr#""foo""#.to_bytes_with_nul(), b"\"foo\"\0");
}
