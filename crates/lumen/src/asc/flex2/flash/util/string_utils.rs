//! Port of the `flash.util.StringUtils` helpers MXML uses, on Java UTF-16 units.

use crate::asc::util::jstr;

const BACKSLASH: u16 = 0x5c;

pub fn find_nth_unescaped(char_to_find: char, n: i32, s: &str) -> i32 {
    let c = char_to_find as u16;
    let a = jstr::to_utf16(s);
    let mut count = 0;
    for i in 0..a.len() {
        if i > 0 && a[i - 1] == BACKSLASH {
            continue;
        }
        if a[i] == c {
            count += 1;
            if count == n {
                return i as i32;
            }
        }
    }
    -1
}

pub fn find_next_unescaped(char_to_find: char, start_idx: i32, s: &str) -> i32 {
    let c = char_to_find as u16;
    let a = jstr::to_utf16(s);
    if start_idx >= a.len() as i32 {
        return -1;
    }
    for i in start_idx.max(0) as usize..a.len() {
        if i > 0 && a[i - 1] == BACKSLASH {
            continue;
        }
        if a[i] == c {
            return i as i32;
        }
    }
    -1
}

pub fn cleanup_escaped_char(escaped_char: char, to_clean: &str) -> String {
    let e = escaped_char as u16;
    let chars = jstr::to_utf16(to_clean);
    if !chars.contains(&e) || !chars.contains(&BACKSLASH) {
        return to_clean.to_string();
    }
    let mut buf = Vec::with_capacity(chars.len());
    for i in 0..chars.len() - 1 {
        if chars[i] != BACKSLASH || chars[i + 1] != e {
            buf.push(chars[i]);
        }
    }
    buf.push(chars[chars.len() - 1]);
    jstr::from_utf16(&buf)
}

pub fn cleanup_escaped_char_for_xml(escaped_char: char, to_clean: &str) -> String {
    let e = escaped_char as u16;
    let chars = jstr::to_utf16(to_clean);
    if !chars.contains(&e) || !chars.contains(&BACKSLASH) {
        return to_clean.to_string();
    }
    let mut buf = String::new();
    let mut i = 0;
    while i < chars.len() - 1 {
        if chars[i] != BACKSLASH || chars[i + 1] != e {
            jstr::push_unit(&mut buf, chars[i]);
        } else {
            buf.push_str(&format!("&#x{:x};", chars[i + 1]));
            i += 1;
        }
        i += 1;
    }
    if i == chars.len() - 1 {
        jstr::push_unit(&mut buf, chars[chars.len() - 1]);
    }
    buf
}

pub fn find_closing_token(open_token: char, close_token: char, s: &str, start_idx: i32) -> i32 {
    let a = jstr::to_utf16(s);
    let (open, close) = (open_token as u16, close_token as u16);
    let mut close_idx = (start_idx + 1) as usize;
    let mut sub_token_count = 0;
    while close_idx < a.len() {
        let c = a[close_idx];
        if a[close_idx - 1] == BACKSLASH {
            close_idx += 1;
            continue;
        }
        if c == open {
            sub_token_count += 1;
        } else if c == close {
            sub_token_count -= 1;
        }
        if sub_token_count < 0 {
            break;
        }
        close_idx += 1;
    }
    if close_idx < a.len() {
        close_idx as i32
    } else {
        -1
    }
}

pub fn format_string(s: &str) -> String {
    let units = jstr::to_utf16(s);
    let mut result = String::with_capacity(s.len() + 2);
    result.push('"');
    for (i, &c) in units.iter().enumerate() {
        match c {
            0x5c => {
                if i + 1 < units.len() && units[i + 1] == b'u' as u16 {
                    result.push('\\');
                } else {
                    result.push_str("\\\\");
                }
            }
            0x22 => result.push_str("\\\""),
            0x08 => result.push_str("\\b"),
            0x09 => result.push_str("\\t"),
            0x0c => result.push_str("\\f"),
            0x0d => result.push_str("\\r"),
            0x0a => result.push_str("\\n"),
            c if c < 0x20 => {
                result.push_str("\\x");
                result.push_str(&c.to_string());
                result.push('X');
            }
            c => jstr::push_unit(&mut result, c),
        }
    }
    result.push('"');
    result
}

/// `Character.isWhitespace(char)`.
pub fn is_java_whitespace(c: u16) -> bool {
    match c {
        0x09..=0x0d | 0x1c..=0x1f => true,
        0xa0 | 0x2007 | 0x202f => false,
        _ => char::from_u32(c as u32).is_some_and(|ch| ch.is_whitespace() && !matches!(c, 0x85)),
    }
}

/// `String.trim()`.
pub fn java_trim(s: &str) -> String {
    let a = jstr::to_utf16(s);
    let mut st = 0;
    let mut len = a.len();
    while st < len && a[st] <= 0x20 {
        st += 1;
    }
    while st < len && a[len - 1] <= 0x20 {
        len -= 1;
    }
    jstr::from_utf16(&a[st..len])
}

pub fn collapse_whitespace(input: &str, replacement_char: char) -> String {
    let mut sb = Vec::new();
    let mut last_was_space = true;
    for c in jstr::to_utf16(input) {
        if is_java_whitespace(c) {
            if !last_was_space {
                sb.push(replacement_char as u16);
            }
            last_was_space = true;
        } else {
            sb.push(c);
            last_was_space = false;
        }
    }
    java_trim(&jstr::from_utf16(&sb))
}
