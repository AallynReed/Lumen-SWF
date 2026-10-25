//! Port of `org.apache.flex.forks.batik.css.parser.ScannerUtilities`. Callers pass `(char) current`, so -1 is U+FFFF.

const IDENTIFIER_START: [u32; 4] = [0x0, 0x0, 0x87FFFFFE, 0x7FFFFFE];
const NAME: [u32; 4] = [0, 0x3FF2000, 0x87FFFFFE, 0x7FFFFFE];
const HEXADECIMAL: [u32; 4] = [0, 0x3FF0000, 0x7E, 0x7E];
const STRING: [u32; 4] = [0x200, 0xFFFFFF7B, 0xFFFFFFFF, 0x7FFFFFFF];
const URI: [u32; 4] = [0x0, 0xFFFFFC7A, 0xFFFFFFFF, 0x7FFFFFFF];

fn bit(table: &[u32; 4], c: u16) -> bool {
    table[(c >> 5) as usize] & (1 << (c & 0x1F)) != 0
}

pub fn is_css_space(c: u16) -> bool {
    c <= 0x20 && ((((1u64 << b'\t') | (1u64 << b'\n') | (1u64 << b'\r') | (1u64 << 0x0C) | (1u64 << 0x20)) >> c) & 1) != 0
}

pub fn is_css_identifier_start_character(c: u16) -> bool {
    c >= 128 || bit(&IDENTIFIER_START, c)
}

pub fn is_css_name_character(c: u16) -> bool {
    c >= 128 || bit(&NAME, c)
}

pub fn is_css_hexadecimal_character(c: u16) -> bool {
    c < 128 && bit(&HEXADECIMAL, c)
}

pub fn is_css_string_character(c: u16) -> bool {
    c >= 128 || bit(&STRING, c)
}

pub fn is_css_uri_character(c: u16) -> bool {
    c >= 128 || bit(&URI, c)
}
