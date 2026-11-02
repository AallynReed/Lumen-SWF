//! Port of afe.jar `com.adobe.fontengine.font.postscript.PostscriptTokenParser`.

use crate::asc::flex2::fonts::adobe::fontengine::font::orig_font_type::OrigFontType;

/// Java's `\s`.
fn is_ws(c: u16) -> bool {
    matches!(c, 0x20 | 0x09 | 0x0a | 0x0b | 0x0c | 0x0d)
}

fn skip_ws(s: &[u16], mut i: usize) -> usize {
    while i < s.len() && is_ws(s[i]) {
        i += 1;
    }
    i
}

fn starts_at(s: &[u16], i: usize, lit: &str) -> bool {
    let l: Vec<u16> = lit.encode_utf16().collect();
    i + l.len() <= s.len() && s[i..i + l.len()] == l[..]
}

/// `\s+def` at `i`.
fn ws_def(s: &[u16], i: usize) -> bool {
    let j = skip_ws(s, i);
    j > i && starts_at(s, j, "def")
}

/// `/FSType\s*(-?\d+)(\s+def)`, first match.
pub fn get_fs_type(s: Option<&str>) -> Option<i32> {
    let s: Vec<u16> = s?.encode_utf16().collect();
    for start in 0..s.len() {
        if !starts_at(&s, start, "/FSType") {
            continue;
        }
        let g1 = skip_ws(&s, start + 7);
        let mut i = g1;
        if i < s.len() && s[i] == '-' as u16 {
            i += 1;
        }
        let d0 = i;
        while i < s.len() && (b'0' as u16..=b'9' as u16).contains(&s[i]) {
            i += 1;
        }
        if i == d0 || !ws_def(&s, i) {
            continue;
        }
        let t = String::from_utf16_lossy(&s[g1..i]);
        // Integer.parseInt overflow would throw NumberFormatException out of afe.
        return Some(t.parse::<i32>().expect("java.lang.NumberFormatException"));
    }
    None
}

/// `/OrigFontType\s*/((?:Type1)|(?:TrueType)|(?:OCF)|(?:CID))(\s+def)`, first match.
pub fn get_orig_font_type(s: Option<&str>) -> Option<OrigFontType> {
    let s: Vec<u16> = s?.encode_utf16().collect();
    for start in 0..s.len() {
        if !starts_at(&s, start, "/OrigFontType") {
            continue;
        }
        let i = skip_ws(&s, start + 13);
        if !starts_at(&s, i, "/") {
            continue;
        }
        for t in [OrigFontType::K_TYPE1, OrigFontType::K_TRUETYPE, OrigFontType::K_OCF, OrigFontType::K_CID] {
            let lit = t.to_java_string();
            let lit = lit.as_str();
            if starts_at(&s, i + 1, lit) && ws_def(&s, i + 1 + lit.len()) {
                return Some(t);
            }
        }
    }
    None
}
