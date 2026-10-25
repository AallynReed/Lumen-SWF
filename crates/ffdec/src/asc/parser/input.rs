//! Port of `macromedia/asc/parser/InputBuffer.java`.

use super::chars::*;

// Features.PASS_ESCAPES_TO_BACKEND
const PASS_ESCAPES_TO_BACKEND: bool = false;

pub struct InputBuffer {
    text: Vec<u16>,
    text_pos: i32,
    text_mark_pos: i32,
    line_map: Option<Vec<i32>>,
    start_line_number: i32,
    start_source_pos: i32,
    pub origin: Option<String>,
    pub report_pos: bool,
    cached_last_line_map_pos: i32,
    cached_last_line_map_index: i32,
}

impl InputBuffer {
    pub fn new_from_stream(in_: &[u8], encoding: Option<&str>, origin: Option<String>) -> InputBuffer {
        let text = Self::create_buffer(in_, encoding);
        Self::init(text, origin, 0, 0)
    }

    pub fn new(in_: &str, origin: Option<String>) -> InputBuffer {
        Self::init(crate::asc::util::jstr::units(&in_).collect(), origin, 0, 0)
    }

    pub fn new_at(in_: &str, origin: Option<String>, start_pos: i32, start_line: i32) -> InputBuffer {
        let mut b = Self::init(crate::asc::util::jstr::units(&in_).collect(), origin, start_pos, start_line);
        b.start_source_pos = start_pos;
        b.start_line_number = start_line;
        b
    }

    pub fn new_utf16(in_: Vec<u16>, origin: Option<String>) -> InputBuffer {
        Self::init(in_, origin, 0, 0)
    }

    fn init(text: Vec<u16>, origin: Option<String>, start_pos: i32, start_line: i32) -> InputBuffer {
        InputBuffer {
            text,
            text_pos: 0,
            text_mark_pos: 0,
            line_map: None,
            start_line_number: start_line,
            start_source_pos: start_pos,
            origin,
            report_pos: true,
            cached_last_line_map_pos: 0,
            cached_last_line_map_index: 0,
        }
    }

    fn create_buffer(b: &[u8], encoding: Option<&str>) -> Vec<u16> {
        if b.is_empty() {
            return Vec::new();
        }

        let mut encoding: Option<String> = encoding.map(|e| e.to_string());
        let bb: &[u8];

        if b.len() > 3 && b[0] == 0xef && b[1] == 0xbb && b[2] == 0xbf {
            encoding = Some("UTF8".to_string());
            bb = &b[3..];
        } else if b.len() > 3 && b[0] == 0xff && b[1] == 0xfe || b[0] == 0xfe && b[1] == 0xff {
            encoding = Some("UTF16".to_string());
            bb = &b[3..];
        } else {
            if encoding.is_none() {
                encoding = Some("UTF8".to_string());
            }
            bb = b;
        }

        decode(encoding.as_deref().unwrap(), bb)
    }

    fn build_line_map(&mut self, max: i32) {
        let src = &self.text;
        let mut line: usize = 0;
        let mut pos: i32 = 0;
        let mut lb = vec![0i32; (max + 1) as usize];

        while pos < max {
            lb[line] = pos;
            line += 1;
            loop {
                let ch = src[pos as usize];
                if ch == b'\r' as u16 || ch == b'\n' as u16 {
                    if ch == b'\r' as u16 && (pos + 1) < max && src[(pos + 1) as usize] == b'\n' as u16 {
                        pos += 2;
                    } else {
                        pos += 1;
                    }
                    break;
                }
                pos += 1;
                if pos >= max {
                    break;
                }
            }
        }

        lb[line] = pos;
        line += 1;

        lb.truncate(line);
        self.line_map = Some(lb);
    }

    fn get_line_map_index(&mut self, src_pos: i32) -> i32 {
        let pos = src_pos - self.start_source_pos;

        if pos < 0 {
            return 0;
        }

        if self.line_map.is_none() {
            let len = self.text.len() as i32;
            self.build_line_map(len);
        }

        if pos == self.cached_last_line_map_pos {
            return self.cached_last_line_map_index;
        }

        self.cached_last_line_map_pos = pos;

        let line_map = self.line_map.as_ref().unwrap();
        let mut low: i32 = 0;
        let mut high: i32 = line_map.len() as i32 - 1;

        while low <= high {
            let mid = (low + high) >> 1;
            let mid_pos = line_map[mid as usize];

            if mid_pos < pos {
                low = mid + 1;
            } else if mid_pos > pos {
                high = mid - 1;
            } else {
                self.cached_last_line_map_index = mid + 1;
                return self.cached_last_line_map_index;
            }
        }
        self.cached_last_line_map_index = low;
        self.cached_last_line_map_index
    }

    pub fn get_ln_num(&mut self, src_pos: i32) -> i32 {
        let mut l = self.get_line_map_index(src_pos);
        if l == 0 {
            l = 1;
        }
        l + self.start_line_number
    }

    pub fn get_line_start_pos(&mut self, src_pos: i32) -> i32 {
        let mut l = self.get_line_map_index(src_pos);

        if l == 0 {
            l = 1;
        }

        self.line_map.as_ref().unwrap()[(l - 1) as usize]
    }

    pub fn source(&self) -> &[u16] {
        &self.text
    }

    pub fn text_mark(&mut self) -> i32 {
        self.text_mark_pos = self.text_pos - 1;
        self.text_mark_pos
    }

    pub fn text_pos(&self) -> i32 {
        self.text_pos
    }

    pub fn nextchar(&mut self) -> i32 {
        if self.text_pos >= self.text.len() as i32 {
            self.text_pos = self.text.len() as i32 + 1;
            return 0;
        }

        let c = self.text[self.text_pos as usize] as i32;
        self.text_pos += 1;
        c
    }

    pub fn retract(&mut self) {
        if self.text_pos > 0 {
            self.text_pos -= 1;
        }
    }

    fn java_type_of_to_character_class(java_class_of: i32) -> i32 {
        match java_class_of {
            JAVA_UPPERCASE_LETTER => Lu,
            JAVA_LOWERCASE_LETTER => Ll,
            JAVA_TITLECASE_LETTER => Lt,
            JAVA_NON_SPACING_MARK => Mn,
            JAVA_COMBINING_SPACING_MARK => Mc,
            JAVA_ENCLOSING_MARK => Me,
            JAVA_DECIMAL_DIGIT_NUMBER => Nd,
            JAVA_LETTER_NUMBER => Nl,
            JAVA_OTHER_NUMBER => No,
            JAVA_SPACE_SEPARATOR => Zs,
            JAVA_LINE_SEPARATOR => Zl,
            JAVA_PARAGRAPH_SEPARATOR => Zp,
            JAVA_CONTROL => Cc,
            JAVA_FORMAT => Cf,
            JAVA_SURROGATE => Cs,
            JAVA_PRIVATE_USE => Co,
            JAVA_UNASSIGNED => Cn,

            JAVA_MODIFIER_LETTER => Lm,
            JAVA_OTHER_LETTER => Lo,
            JAVA_CONNECTOR_PUNCTUATION => Pc,
            JAVA_DASH_PUNCTUATION => Pd,
            JAVA_START_PUNCTUATION => Ps,
            JAVA_END_PUNCTUATION => Pe,
            JAVA_INITIAL_QUOTE_PUNCTUATION => Pi,
            JAVA_FINAL_QUOTE_PUNCTUATION => Pf,
            JAVA_OTHER_PUNCTUATION => Po,
            JAVA_MATH_SYMBOL => Sm,
            JAVA_CURRENCY_SYMBOL => Sc,
            JAVA_MODIFIER_SYMBOL => Sk,
            JAVA_OTHER_SYMBOL => So,

            _ => Cn,
        }
    }

    /// Returns the class as `i32`; Java returns the same value as a `char`.
    pub fn next_char_class(&mut self, c: u16, advance: bool) -> i32 {
        let mut c = c;
        let mut distance = 0;

        if c == b'\\' as u16 && self.text[self.text_pos as usize] == b'u' as u16 {
            let mut this_char: i32 = 0;
            let mut y = self.text_pos + 1;

            while y < self.text_pos + 5 && y < self.text.len() as i32 {
                let digit = java_digit16(self.text[y as usize]);
                if digit == -1 {
                    break;
                }
                this_char = (this_char << 4) + digit;
                y += 1;
            }

            if y == self.text_pos + 5 && java_is_defined(this_char as u16) {
                c = this_char as u16;
                distance = 5;
            } else {
                distance = 1;
            }
        }
        if advance {
            self.text_pos += distance;
        }

        Self::java_type_of_to_character_class(java_get_type(c))
    }

    pub fn position_of_next(&self) -> i32 {
        self.text_pos + self.start_source_pos
    }

    pub fn position_of_mark(&self) -> i32 {
        if !self.report_pos {
            return -1;
        }

        if self.text_mark_pos == -1 {
            return self.text_mark_pos + self.start_source_pos;
        }

        self.text_mark_pos + self.start_source_pos
    }

    fn has_escape(src: &[u16], from: i32, to: i32) -> bool {
        for i in from..to {
            if src[i as usize] == b'\\' as u16 {
                return true;
            }
        }
        false
    }

    fn has_u_escape(src: &[u16], from: i32, to: i32) -> bool {
        for i in from..to {
            if src[i as usize] == b'\\' as u16 && i < to && src[(i + 1) as usize] == b'u' as u16 {
                return true;
            }
        }
        false
    }

    fn escape_unicode(src: &[u16], from: i32, to: i32) -> String {
        if !Self::has_u_escape(src, from, to) {
            return substring(src, from, to);
        }

        let len = to - from;
        let mut buf: Vec<u16> = Vec::with_capacity(len as usize);

        let mut i = from;
        while i < to {
            let mut c = src[i as usize];
            if c == b'\\' as u16 && i < to && src[(i + 1) as usize] == b'u' as u16 {
                let mut this_char: i32 = 0;
                let mut y = i + 2;
                while y < i + 6 && y < to + 1 {
                    let digit = java_digit16(src[y as usize]);
                    if digit == -1 {
                        break;
                    }
                    this_char = (this_char << 4) + digit;
                    y += 1;
                }
                if y != i + 6 || !java_is_defined(this_char as u16) {
                    i += 1;
                    c = src[i as usize];
                } else {
                    c = this_char as u16;
                    i += 5;
                }
            }
            buf.push(c);
            i += 1;
        }
        java_string(&buf)
    }

    fn escape_string(src: &[u16], from: i32, to: i32) -> String {
        if !Self::has_escape(src, from, to) {
            return substring(src, from, to);
        }

        let len = to - from;
        let mut buf: Vec<u16> = Vec::with_capacity(len as usize);

        let mut i = from;
        while i < to {
            let mut c = src[i as usize];
            if c == b'\\' as u16 {
                let c2 = src[(i + 1) as usize];

                if c2 == b'\'' as u16 || c2 == b'"' as u16 {
                    i += 1;
                    continue;
                } else if c2 == b'\r' as u16 || c2 == b'\n' as u16 {
                    if c2 == b'\r' as u16 && src[(i + 2) as usize] == b'\n' as u16 {
                        i += 1;
                    }
                    i += 1;
                    i += 1;
                    continue;
                } else if c2 == b'\\' as u16 {
                    c = b'\\' as u16;
                    i += 1;
                } else if c2 == b'u' as u16 {
                    let mut this_char: i32 = 0;
                    let mut y = i + 2;
                    while y < i + 6 && y < to + 1 {
                        let digit = java_digit16(src[y as usize]);
                        if digit == -1 {
                            break;
                        }
                        this_char = (this_char << 4) + digit;
                        y += 1;
                    }
                    if y != i + 6 || !java_is_defined(this_char as u16) {
                        i += 1;
                        c = src[i as usize];
                    } else {
                        c = this_char as u16;
                        i += 5;
                    }
                } else if PASS_ESCAPES_TO_BACKEND {
                    i += 1;
                    c = src[i as usize];
                } else {
                    match c2 {
                        0x62 => {
                            c = 0x08;
                            i += 1;
                        }
                        0x66 => {
                            c = 0x0c;
                            i += 1;
                        }
                        0x6e => {
                            c = b'\n' as u16;
                            i += 1;
                        }
                        0x72 => {
                            c = b'\r' as u16;
                            i += 1;
                        }
                        0x74 => {
                            c = b'\t' as u16;
                            i += 1;
                        }
                        0x76 => {
                            c = 0xb;
                            i += 1;
                        }
                        0x78 => {
                            let d1;
                            let d2;
                            if i + 4 > to || {
                                d1 = java_digit16(src[(i + 2) as usize]);
                                d1 == -1
                            } || {
                                d2 = java_digit16(src[(i + 3) as usize]);
                                d2 == -1
                            } {
                                i += 1;
                                c = b'x' as u16;
                            } else {
                                i += 3;
                                c = ((d1 << 4) + d2) as u16;
                            }
                        }
                        _ => {
                            i += 1;
                            c = src[i as usize];
                        }
                    }
                }
            }
            buf.push(c);
            i += 1;
        }
        java_string(&buf)
    }

    pub fn copy(&self) -> String {
        substring(&self.text, self.text_mark_pos, self.text_pos)
    }

    pub fn copy_replace_string_escapes(&self, needs_escape: bool) -> String {
        if needs_escape {
            return Self::escape_string(&self.text, self.text_mark_pos, self.text_pos);
        }

        substring(&self.text, self.text_mark_pos, self.text_pos)
    }

    pub fn substring_replace_unicode_escapes(&self, begin: i32, end: i32) -> Option<String> {
        let len = end - begin;

        if len <= 0 {
            return None;
        }

        Some(Self::escape_unicode(&self.text, begin, end))
    }

    pub fn copy_replace_unicode_escapes(&self) -> String {
        Self::escape_unicode(&self.text, self.text_mark_pos, self.text_pos)
    }

    pub fn copy_replace_unicode_escapes_if(&self, needs_escape: bool) -> String {
        if needs_escape {
            return Self::escape_unicode(&self.text, self.text_mark_pos, self.text_pos);
        }

        substring(&self.text, self.text_mark_pos, self.text_pos)
    }

    pub fn mark_char_at(&self, offset: i32) -> u16 {
        self.text[(self.text_mark_pos - offset) as usize]
    }

    pub fn mark_length(&self) -> i32 {
        self.text_pos - self.text_mark_pos
    }

    pub fn get_col_pos(&mut self, src_pos: i32) -> i32 {
        let start = self.get_line_start_pos(src_pos);
        (src_pos - start) + 1
    }

    pub fn get_line_text(&mut self, src_pos: i32) -> String {
        let start = self.get_line_start_pos(src_pos);

        let mut i = start;
        while i < self.text.len() as i32 {
            let c = self.text[i as usize];

            if c == b'\n' as u16 || c == b'\r' as u16 || c == 0x00 || c == 0x2028 || c == 0x2029 {
                break;
            }
            i += 1;
        }
        substring(&self.text, start, i)
    }

    pub fn get_line_pointer(col: i32) -> String {
        let mut padding = String::with_capacity((1 + col.max(0)) as usize);
        for _ in 0..col - 1 {
            padding.push('.');
        }
        padding.push('^');
        padding
    }

    pub fn clear_unused_buffers(&mut self) {}
}

/// `String.substring(begin, end)` over UTF-16 text; out-of-range bounds panic like Java throws.
fn substring(src: &[u16], begin: i32, end: i32) -> String {
    if begin < 0 || end > src.len() as i32 || begin > end {
        panic!("String index out of range: begin {}, end {}, length {}", begin, end, src.len());
    }
    java_string(&src[begin as usize..end as usize])
}

fn java_string(units: &[u16]) -> String {
    crate::asc::util::jstr::from_utf16(units)
}

/// `Charset.forName(encoding).decode(bb)` for the common charsets; anything else decodes as UTF-8.
fn decode(encoding: &str, bb: &[u8]) -> Vec<u16> {
    let name = encoding.to_ascii_uppercase().replace('_', "-");
    match name.as_str() {
        "UTF16" | "UTF-16" => decode_utf16(bb, None),
        "UTF-16BE" | "UNICODEBIGUNMARKED" => decode_utf16(bb, Some(true)),
        "UTF-16LE" | "UNICODELITTLEUNMARKED" => decode_utf16(bb, Some(false)),
        "ISO-8859-1" | "ISO8859-1" | "LATIN1" | "8859-1" | "CP819" => bb.iter().map(|&b| b as u16).collect(),
        "US-ASCII" | "ASCII" | "ASCII7" => bb.iter().map(|&b| if b < 0x80 { b as u16 } else { 0xfffd }).collect(),
        _ => decode_utf8(bb),
    }
}

fn decode_utf16(bb: &[u8], big_endian: Option<bool>) -> Vec<u16> {
    // sun.nio.cs.UnicodeDecoder (JDK 17) with REPLACE: each malformed unit or trailing remainder becomes one U+FFFD
    let mut order = big_endian;
    let mut out = Vec::with_capacity(bb.len() / 2);
    let mut i = 0;
    let unit = |be: bool, b1: u8, b2: u8| if be { u16::from_be_bytes([b1, b2]) } else { u16::from_le_bytes([b1, b2]) };
    while bb.len() - i > 1 {
        let (b1, b2) = (bb[i], bb[i + 1]);
        i += 2;
        if order.is_none() {
            let c = u16::from_be_bytes([b1, b2]);
            if c == 0xfeff {
                order = Some(true);
                continue;
            } else if c == 0xfffe {
                order = Some(false);
                continue;
            }
            order = Some(true);
        }
        let be = order.unwrap();
        let c = unit(be, b1, b2);
        if c == 0xfffe {
            out.push(0xfffd);
            continue;
        }
        if (0xd800..0xe000).contains(&c) {
            if c < 0xdc00 {
                if bb.len() - i < 2 {
                    i -= 2;
                    break;
                }
                let c2 = unit(be, bb[i], bb[i + 1]);
                i += 2;
                if !(0xdc00..0xe000).contains(&c2) {
                    out.push(0xfffd);
                    continue;
                }
                out.push(c);
                out.push(c2);
                continue;
            }
            out.push(0xfffd);
            continue;
        }
        out.push(c);
    }
    if i < bb.len() {
        out.push(0xfffd);
    }
    out
}

fn decode_utf8(sa: &[u8]) -> Vec<u16> {
    crate::asc::util::jstr::decode_java_utf8(sa)
}
