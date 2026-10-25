//! Java `String` (UTF-16, lone surrogates allowed) carried in a Rust `String` by an injective encoding:
//! a lone surrogate unit u is `ESC` + U+10F000 + (u - 0xD800), a genuine U+10FFFF is `ESC ESC`, all else is unchanged.
//! The helpers below give `java.lang.String` semantics (UTF-16 indices, ordering, hashing, UTF-8 bytes) on encoded strings.

use std::cmp::Ordering;

pub const ESC: char = '\u{10FFFF}';
const LONE_BASE: u32 = 0x10F000;

/// The UTF-16 code units of an encoded string.
pub struct Units<'a> {
    chars: std::str::Chars<'a>,
    low: Option<u16>,
}

impl Iterator for Units<'_> {
    type Item = u16;

    fn next(&mut self) -> Option<u16> {
        if let Some(l) = self.low.take() {
            return Some(l);
        }
        let mut c = self.chars.next()?;
        if c == ESC {
            match self.chars.next() {
                Some(ESC) | None => c = ESC,
                Some(d) => return Some((d as u32 - LONE_BASE + 0xD800) as u16),
            }
        }
        let mut buf = [0u16; 2];
        let e = c.encode_utf16(&mut buf);
        if e.len() == 2 {
            self.low = Some(e[1]);
        }
        Some(e[0])
    }
}

pub fn units(s: &str) -> Units<'_> {
    Units { chars: s.chars(), low: None }
}

pub fn from_utf16(units: &[u16]) -> String {
    let mut s = String::with_capacity(units.len());
    for r in char::decode_utf16(units.iter().copied()) {
        push_unit_or_char(&mut s, r);
    }
    s
}

fn push_unit_or_char(s: &mut String, r: Result<char, std::char::DecodeUtf16Error>) {
    match r {
        Ok(ESC) => {
            s.push(ESC);
            s.push(ESC);
        }
        Ok(c) => s.push(c),
        Err(e) => {
            s.push(ESC);
            s.push(char::from_u32(LONE_BASE + (e.unpaired_surrogate() as u32 - 0xD800)).unwrap());
        }
    }
}

pub fn to_utf16(s: &str) -> Vec<u16> {
    units(s).collect()
}

/// `String.valueOf(char)`.
pub fn from_unit(u: u16) -> String {
    from_utf16(&[u])
}

/// `String.length()`.
pub fn utf16_len(s: &str) -> i32 {
    if s.is_ascii() {
        return s.len() as i32;
    }
    units(s).count() as i32
}

fn out_of_range(what: &str, len: usize) -> ! {
    panic!("String index out of range: {what}, length {len}")
}

/// `String.charAt(i)`.
pub fn char_at(s: &str, i: i32) -> u16 {
    if s.is_ascii() {
        if i < 0 || i as usize >= s.len() {
            out_of_range(&format!("index {i}"), s.len());
        }
        return s.as_bytes()[i as usize] as u16;
    }
    let u = to_utf16(s);
    if i < 0 || i as usize >= u.len() {
        out_of_range(&format!("index {i}"), u.len());
    }
    u[i as usize]
}

/// `String.substring(begin, end)`.
pub fn substring(s: &str, begin: i32, end: i32) -> String {
    if s.is_ascii() {
        if begin < 0 || end as usize > s.len() || begin > end {
            out_of_range(&format!("begin {begin}, end {end}"), s.len());
        }
        return s[begin as usize..end as usize].to_string();
    }
    let u = to_utf16(s);
    if begin < 0 || end as usize > u.len() || begin > end {
        out_of_range(&format!("begin {begin}, end {end}"), u.len());
    }
    from_utf16(&u[begin as usize..end as usize])
}

/// `String.substring(begin)`.
pub fn substring_from(s: &str, begin: i32) -> String {
    substring(s, begin, utf16_len(s))
}

fn find(h: &[u16], n: &[u16], from: i32) -> i32 {
    let from = from.max(0) as usize;
    if n.is_empty() {
        return from.min(h.len()) as i32;
    }
    if n.len() > h.len() {
        return -1;
    }
    (from..=h.len() - n.len()).find(|&i| h[i..i + n.len()] == *n).map_or(-1, |i| i as i32)
}

/// `String.indexOf(String, fromIndex)`.
pub fn index_of(s: &str, needle: &str, from: i32) -> i32 {
    find(&to_utf16(s), &to_utf16(needle), from)
}

/// `String.indexOf(char, fromIndex)`.
pub fn index_of_unit(s: &str, ch: u16, from: i32) -> i32 {
    let from = from.max(0) as usize;
    units(s).enumerate().skip(from).find(|&(_, u)| u == ch).map_or(-1, |(i, _)| i as i32)
}

/// `String.lastIndexOf(String)`.
pub fn last_index_of(s: &str, needle: &str) -> i32 {
    let (h, n) = (to_utf16(s), to_utf16(needle));
    if n.len() > h.len() {
        return -1;
    }
    (0..=h.len() - n.len()).rev().find(|&i| h[i..i + n.len()] == *n).map_or(-1, |i| i as i32)
}

/// `String.lastIndexOf(char)`.
pub fn last_index_of_unit(s: &str, ch: u16) -> i32 {
    to_utf16(s).iter().rposition(|&u| u == ch).map_or(-1, |i| i as i32)
}

/// `String.startsWith(prefix, offset)`.
pub fn starts_with_at(s: &str, prefix: &str, offset: i32) -> bool {
    let (h, p) = (to_utf16(s), to_utf16(prefix));
    offset >= 0 && offset as usize + p.len() <= h.len() && h[offset as usize..offset as usize + p.len()] == *p
}

/// `String.compareTo`, as Java's difference.
pub fn compare_to(a: &str, b: &str) -> i32 {
    let (mut ia, mut ib) = (units(a), units(b));
    let (mut la, mut lb) = (0i32, 0i32);
    loop {
        match (ia.next(), ib.next()) {
            (Some(x), Some(y)) => {
                if x != y {
                    return x as i32 - y as i32;
                }
                la += 1;
                lb += 1;
            }
            (Some(_), None) => return (la + 1 + ia.count() as i32) - lb,
            (None, Some(_)) => return la - (lb + 1 + ib.count() as i32),
            (None, None) => return la - lb,
        }
    }
}

/// `String.compareTo` as an `Ordering`.
pub fn compare(a: &str, b: &str) -> Ordering {
    units(a).cmp(units(b))
}

/// `String.hashCode()`.
pub fn java_hash_code(s: &str) -> i32 {
    units(s).fold(0i32, |h, u| h.wrapping_mul(31).wrapping_add(u as i32))
}

/// `String.getBytes("UTF8")`: a lone surrogate becomes '?'.
pub fn to_java_utf8_bytes(s: &str) -> Vec<u8> {
    if !s.contains(ESC) {
        return s.as_bytes().to_vec();
    }
    let mut out = Vec::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == ESC {
            match it.next() {
                Some(ESC) | None => {}
                Some(_) => {
                    out.push(b'?');
                    continue;
                }
            }
        }
        let mut b = [0u8; 4];
        out.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
    }
    out
}

/// The text Java prints for the string through a UTF-8 writer (lone surrogates as '?').
pub fn to_display(s: &str) -> String {
    if !s.contains(ESC) {
        return s.to_string();
    }
    String::from_utf8(to_java_utf8_bytes(s)).unwrap()
}

/// `new String(bytes, "UTF8")` (JDK 17, malformed input replaced with U+FFFD).
pub fn from_java_utf8_bytes(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        if !s.contains(ESC) {
            return s.to_string();
        }
    }
    from_utf16(&decode_java_utf8(bytes))
}

/// `sun.nio.cs.UTF_8` decoding (JDK 17) with REPLACE, to UTF-16 units.
pub fn decode_java_utf8(sa: &[u8]) -> Vec<u16> {
    let not_cont = |b: u8| (b & 0xc0) != 0x80;
    let mut out = Vec::with_capacity(sa.len());
    let sl = sa.len();
    let mut sp = 0;
    while sp < sl {
        let b1 = sa[sp];
        if b1 < 0x80 {
            out.push(b1 as u16);
            sp += 1;
        } else if (b1 >> 5) == 0b110 && (b1 & 0x1e) != 0 {
            if sl - sp < 2 {
                break;
            }
            let b2 = sa[sp + 1];
            if not_cont(b2) {
                out.push(0xfffd);
                sp += 1;
                continue;
            }
            out.push((((b1 as u16) & 0x1f) << 6) | ((b2 as u16) & 0x3f));
            sp += 2;
        } else if (b1 >> 4) == 0b1110 {
            let rem = sl - sp;
            let mal3_2 = |b2: u8| (b1 == 0xe0 && (b2 & 0xe0) == 0x80) || not_cont(b2);
            if rem < 3 {
                if rem > 1 && mal3_2(sa[sp + 1]) {
                    out.push(0xfffd);
                    sp += 1;
                    continue;
                }
                break;
            }
            let (b2, b3) = (sa[sp + 1], sa[sp + 2]);
            if mal3_2(b2) || not_cont(b3) {
                out.push(0xfffd);
                sp += if mal3_2(b2) { 1 } else { 2 };
                continue;
            }
            let c = (((b1 as u16) & 0x0f) << 12) | (((b2 as u16) & 0x3f) << 6) | ((b3 as u16) & 0x3f);
            out.push(if (0xd800..0xe000).contains(&c) { 0xfffd } else { c });
            sp += 3;
        } else if (b1 >> 3) == 0b11110 {
            let rem = sl - sp;
            let mal4_2 = |b2: u8| (b1 == 0xf0 && !(0x90..=0xbf).contains(&b2)) || (b1 == 0xf4 && (b2 & 0xf0) != 0x80) || not_cont(b2);
            if rem < 4 {
                if b1 > 0xf4 || (rem > 1 && mal4_2(sa[sp + 1])) {
                    out.push(0xfffd);
                    sp += 1;
                    continue;
                }
                if rem > 2 && not_cont(sa[sp + 2]) {
                    out.push(0xfffd);
                    sp += 2;
                    continue;
                }
                break;
            }
            let (b2, b3, b4) = (sa[sp + 1], sa[sp + 2], sa[sp + 3]);
            let uc = (((b1 as u32) & 0x07) << 18) | (((b2 as u32) & 0x3f) << 12) | (((b3 as u32) & 0x3f) << 6) | ((b4 as u32) & 0x3f);
            if not_cont(b2) || not_cont(b3) || not_cont(b4) || !(0x10000..0x110000).contains(&uc) {
                out.push(0xfffd);
                sp += if b1 > 0xf4 || mal4_2(b2) {
                    1
                } else if not_cont(b3) {
                    2
                } else {
                    3
                };
                continue;
            }
            let v = uc - 0x10000;
            out.push(0xd800 | (v >> 10) as u16);
            out.push(0xdc00 | (v & 0x3ff) as u16);
            sp += 4;
        } else {
            out.push(0xfffd);
            sp += 1;
        }
    }
    if sp < sl {
        out.push(0xfffd);
    }
    out
}

/// Java `a + b`: a lone high surrogate ending `a` pairs with a lone low surrogate starting `b`.
pub fn concat(a: &str, b: &str) -> String {
    let mut s = String::with_capacity(a.len() + b.len());
    s.push_str(a);
    push_str(&mut s, b);
    s
}

/// `StringBuilder.append(String)`.
pub fn push_str(dst: &mut String, b: &str) {
    if let (Some(h), Some(l)) = (trailing_lone(dst), leading_lone(b)) {
        if (0xD800..0xDC00).contains(&h) && (0xDC00..0xE000).contains(&l) {
            let t = dst.len() - ESC.len_utf8() - '\u{10F000}'.len_utf8();
            dst.truncate(t);
            dst.push(char::from_u32(0x10000 + ((h as u32 - 0xD800) << 10) + (l as u32 - 0xDC00)).unwrap());
            dst.push_str(&b[ESC.len_utf8() * 2..]);
            return;
        }
    }
    dst.push_str(b);
}

/// `StringBuilder.append(char)`.
pub fn push_unit(dst: &mut String, u: u16) {
    push_str(dst, &from_unit(u));
}

fn leading_lone(s: &str) -> Option<u16> {
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(ESC), Some(d)) if d != ESC => Some((d as u32 - LONE_BASE + 0xD800) as u16),
        _ => None,
    }
}

fn trailing_lone(s: &str) -> Option<u16> {
    let mut it = s.chars().rev();
    let d = it.next()?;
    if d == ESC || d < '\u{10F000}' {
        return None;
    }
    let mut n = 0;
    for c in it {
        if c != ESC {
            break;
        }
        n += 1;
    }
    (n % 2 == 1).then(|| (d as u32 - LONE_BASE + 0xD800) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(u: &[u16]) {
        let s = from_utf16(u);
        assert_eq!(to_utf16(&s), u);
        assert_eq!(utf16_len(&s), u.len() as i32);
    }

    #[test]
    fn round_trip_and_injective() {
        rt(&[0x61, 0xD800]);
        rt(&[0xDFFF, 0xDBFF, 0xDFFF, 0xDBFF]);
        rt(&[0xDBFF, 0xDFFF, 0xDBC0, 0xDC00, 0xD83D, 0xDE00, 0xDE00]);
        rt(&[]);
        let all: Vec<String> = [&[0x61, 0xD800][..], &[0x61, 0x3F], &[0xDBFF, 0xDFFF], &[0xDFFF], &[0xDBC0, 0xDC00]].iter().map(|u| from_utf16(u)).collect();
        for i in 0..all.len() {
            for j in 0..all.len() {
                assert_eq!(all[i] == all[j], i == j);
            }
        }
    }

    #[test]
    fn java_semantics() {
        let s = from_utf16(&[0x61, 0xD83D, 0xDE00, 0xD800, 0x62]);
        assert_eq!(utf16_len(&s), 5);
        assert_eq!(char_at(&s, 2), 0xDE00);
        assert_eq!(char_at(&s, 3), 0xD800);
        assert_eq!(to_utf16(&substring(&s, 2, 4)), [0xDE00, 0xD800]);
        assert_eq!(index_of(&s, "b", 0), 4);
        assert_eq!(index_of_unit(&s, 0xD800, 0), 3);
        assert_eq!(last_index_of_unit(&s, 0xDE00), 2);
        assert!(starts_with_at(&s, &from_utf16(&[0xDE00]), 2));
        assert_eq!(java_hash_code("abc"), 96354);
        assert_eq!(java_hash_code(&s), [0x61u16, 0xD83D, 0xDE00, 0xD800, 0x62].iter().fold(0i32, |h, &u| h.wrapping_mul(31).wrapping_add(u as i32)));
        assert_eq!(compare_to("\u{E000}", "\u{10000}"), 0xE000 - 0xD800);
        assert_eq!(compare("\u{E000}", "\u{10000}"), Ordering::Greater);
        assert_eq!(compare_to("ab", "abcd"), -2);
        assert_eq!(to_java_utf8_bytes(&s), b"a\xF0\x9F\x98\x80?b");
        assert_eq!(to_java_utf8_bytes(&from_utf16(&[0xDBFF, 0xDFFF])), b"\xF4\x8F\xBF\xBF");
        assert_eq!(to_display(&s), "a\u{1F600}?b");
        assert_eq!(to_utf16(&from_java_utf8_bytes(b"\xED\xA0\x80x\xF4\x8F\xBF\xBF")), [0xFFFD, 0x78, 0xDBFF, 0xDFFF]);
    }

    #[test]
    fn utf8_matches_jdk17() {
        let cases: &[(&str, &[u16])] = &[
            ("edA080", &[0xFFFD]),
            ("f09f9880", &[0xD83D, 0xDE00]),
            ("edA0BDedB880", &[0xFFFD, 0xFFFD]),
            ("f4908080", &[0xFFFD, 0xFFFD, 0xFFFD, 0xFFFD]),
            ("c0af78", &[0xFFFD, 0xFFFD, 0x78]),
            ("e282", &[0xFFFD]),
            ("f09f98", &[0xFFFD]),
            ("80", &[0xFFFD]),
            ("e0808f", &[0xFFFD, 0xFFFD, 0xFFFD]),
            ("e0a0", &[0xFFFD]),
            ("f08f8080", &[0xFFFD, 0xFFFD, 0xFFFD, 0xFFFD]),
            ("ff41", &[0xFFFD, 0x41]),
            ("c2c3", &[0xFFFD, 0xFFFD]),
            ("e2827e", &[0xFFFD, 0x7E]),
            ("f09f9841", &[0xFFFD, 0x41]),
            ("f5808080", &[0xFFFD, 0xFFFD, 0xFFFD, 0xFFFD]),
            ("edBFBF", &[0xFFFD]),
            ("f48fbfbf", &[0xDBFF, 0xDFFF]),
            ("c280e28282f0908080", &[0x80, 0x2082, 0xD800, 0xDC00]),
        ];
        for (hex, want) in cases {
            let b: Vec<u8> = (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap()).collect();
            assert_eq!(to_utf16(&from_java_utf8_bytes(&b)), *want, "{hex}");
        }
        let lone = from_utf16(&[0x61, 0xD800, 0x62, 0xDBFF, 0xDFFF, 0xDFFF, 0xD83D]);
        assert_eq!(to_java_utf8_bytes(&lone), b"a?b\xF4\x8F\xBF\xBF??");
    }

    #[test]
    fn concat_pairs_surrogates() {
        let h = from_unit(0xD83D);
        let l = from_unit(0xDE00);
        assert_eq!(concat(&h, &l), "\u{1F600}");
        assert_eq!(concat(&concat("x", &h), &concat(&l, "y")), "x\u{1F600}y");
        assert_eq!(to_utf16(&concat(&l, &h)), [0xDE00, 0xD83D]);
        let e = from_utf16(&[0xDBFF, 0xDFFF]);
        assert_eq!(to_utf16(&concat(&e, &l)), [0xDBFF, 0xDFFF, 0xDE00]);
    }
}
