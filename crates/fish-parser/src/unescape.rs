pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'a' => out.push('\x07'),
                    'e' => out.push('\x1b'),
                    'f' => out.push('\x0c'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'v' => out.push('\x0b'),
                    '\n' => {}
                    'x' | 'X' => {
                        let mut hex = String::new();
                        for _ in 0..2 {
                            if let Some(&h) = chars.peek() {
                                if h.is_ascii_hexdigit() {
                                    hex.push(chars.next().unwrap());
                                    continue;
                                }
                            }
                            break;
                        }
                        if let Ok(val) = u8::from_str_radix(&hex, 16) {
                            out.push(val as char);
                        } else {
                            out.push(next);
                            out.push_str(&hex);
                        }
                    }
                    'u' => {
                        let mut hex = String::new();
                        for _ in 0..4 {
                            if let Some(&h) = chars.peek() {
                                if h.is_ascii_hexdigit() {
                                    hex.push(chars.next().unwrap());
                                    continue;
                                }
                            }
                            break;
                        }
                        if let Ok(val) = u32::from_str_radix(&hex, 16) {
                            if let Some(ch) = char::from_u32(val) {
                                out.push(ch);
                            } else {
                                out.push(next);
                                out.push_str(&hex);
                            }
                        } else {
                            out.push(next);
                            out.push_str(&hex);
                        }
                    }
                    'U' => {
                        let mut hex = String::new();
                        for _ in 0..8 {
                            if let Some(&h) = chars.peek() {
                                if h.is_ascii_hexdigit() {
                                    hex.push(chars.next().unwrap());
                                    continue;
                                }
                            }
                            break;
                        }
                        if let Ok(val) = u32::from_str_radix(&hex, 16) {
                            if let Some(ch) = char::from_u32(val) {
                                out.push(ch);
                            } else {
                                out.push(next);
                                out.push_str(&hex);
                            }
                        } else {
                            out.push(next);
                            out.push_str(&hex);
                        }
                    }
                    'c' => {
                        if let Some(&ch) = chars.peek() {
                            if ch.is_ascii_alphabetic() {
                                chars.next();
                                let ctrl = (ch.to_ascii_uppercase() as u8) & 0x1f;
                                out.push(ctrl as char);
                            } else {
                                out.push(next);
                            }
                        } else {
                            out.push(next);
                        }
                    }
                    '0'..='7' => {
                        let mut oct = String::new();
                        oct.push(next);
                        for _ in 0..2 {
                            if let Some(&o) = chars.peek() {
                                if ('0'..='7').contains(&o) {
                                    oct.push(chars.next().unwrap());
                                    continue;
                                }
                            }
                            break;
                        }
                        if let Ok(val) = u8::from_str_radix(&oct, 8) {
                            out.push(val as char);
                        } else {
                            out.push_str(&oct);
                        }
                    }
                    _ => out.push(next),
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn unescape_single_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next == '\'' || next == '\\' {
                    chars.next();
                    out.push(next);
                    continue;
                }
            }
            out.push(c);
        } else {
            out.push(c);
        }
    }
    out
}

pub fn unescape_double_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next == '"' || next == '$' || next == '\\' {
                    chars.next();
                    out.push(next);
                    continue;
                } else if next == '\n' {
                    chars.next();
                    continue;
                }
            }
            out.push(c);
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unescape_basic_control_chars() {
        assert_eq!(unescape(r"\a\e\f\n\r\t\v"), "\x07\x1b\x0c\n\r\t\x0b");
    }

    #[test]
    fn test_unescape_hex() {
        assert_eq!(unescape(r"\x41\x09"), "A\t");
        assert_eq!(unescape(r"\X42"), "B");
    }

    #[test]
    fn test_unescape_octal() {
        assert_eq!(unescape(r"\011"), "\t");
        assert_eq!(unescape(r"\101"), "A");
    }

    #[test]
    fn test_unescape_unicode() {
        assert_eq!(unescape(r"\u0041"), "A");
        assert_eq!(unescape(r"\U00000041"), "A");
        assert_eq!(unescape(r"\u4e2d\u6587"), "中文");
    }

    #[test]
    fn test_unescape_control_sequence() {
        // \ci -> ctrl-i -> \t (0x09)
        assert_eq!(unescape(r"\ci"), "\t");
        assert_eq!(unescape(r"\ca"), "\x01");
    }

    #[test]
    fn test_unescape_single_quoted() {
        assert_eq!(unescape_single_quoted(r"hello\'world"), "hello'world");
        assert_eq!(unescape_single_quoted(r"hello\\world"), "hello\\world");
        assert_eq!(unescape_single_quoted(r"hello\nworld"), "hello\\nworld");
    }

    #[test]
    fn test_unescape_double_quoted() {
        assert_eq!(unescape_double_quoted(r#"hello\"world"#), "hello\"world");
        assert_eq!(unescape_double_quoted(r"hello\$world"), "hello$world");
        assert_eq!(unescape_double_quoted(r"hello\\world"), "hello\\world");
        assert_eq!(unescape_double_quoted("hello\\\nworld"), "helloworld");
        // Other escapes are preserved literally
        assert_eq!(unescape_double_quoted(r"hello\nworld"), "hello\\nworld");
    }
}
