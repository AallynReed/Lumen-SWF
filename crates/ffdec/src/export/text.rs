//! Plain-text export of DefineText/2 (glyphs mapped through their fonts) and DefineEditText.

use std::collections::HashMap;

use crate::error::Result;
use crate::io::Reader;
use crate::tags::{code::*, Tag};
use crate::types::Rect;

use super::font::Font;

pub fn is_text(code: u16) -> bool {
    matches!(code, DEFINE_TEXT | DEFINE_TEXT2 | DEFINE_EDIT_TEXT)
}

/// One string per text record.
pub fn records(tag: &Tag, fonts: &HashMap<u16, Font>) -> Result<Vec<String>> {
    if tag.code == DEFINE_EDIT_TEXT {
        return Ok(vec![edit_text(tag)?]);
    }
    let mut r = Reader::new(&tag.body);
    r.u16()?;
    Rect::read(&mut r)?;
    skip_matrix(&mut r)?;
    let glyph_bits = r.u8()? as u32;
    let advance_bits = r.u8()? as u32;
    let mut font: Option<u16> = None;
    let mut out = Vec::new();
    loop {
        let flags = r.u8()?;
        if flags == 0 {
            break;
        }
        if flags & 0x08 != 0 {
            font = Some(r.u16()?);
        }
        if flags & 0x04 != 0 {
            r.skip(if tag.code == DEFINE_TEXT2 { 4 } else { 3 })?;
        }
        if flags & 0x01 != 0 {
            r.i16()?;
        }
        if flags & 0x02 != 0 {
            r.i16()?;
        }
        if flags & 0x08 != 0 {
            r.u16()?;
        }
        let count = r.u8()?;
        let mut s = String::new();
        let f = font.and_then(|id| fonts.get(&id));
        for _ in 0..count {
            let glyph = r.ub(glyph_bits)? as usize;
            r.sb(advance_bits)?;
            match f.and_then(|f| f.char_for(glyph)) {
                Some(c) => s.push(c),
                None => s.push('\u{fffd}'),
            }
        }
        r.align();
        if f.is_none() {
            s = format!("Font {} not found", font.unwrap_or(0));
        }
        out.push(s);
    }
    Ok(out)
}

fn skip_matrix(r: &mut Reader) -> Result<()> {
    r.align();
    for _ in 0..2 {
        if r.bit()? {
            let n = r.ub(5)?;
            r.ub(n)?;
            r.ub(n)?;
        }
    }
    let n = r.ub(5)?;
    r.ub(n)?;
    r.ub(n)?;
    r.align();
    Ok(())
}

/// The initial text of a DefineEditText, with HTML markup reduced to plain text.
pub fn edit_text(tag: &Tag) -> Result<String> {
    let (text, html) = edit_text_raw(tag)?;
    Ok(if html { strip_html(&text) } else { text })
}

/// The initial text of a DefineEditText as stored, and whether it is HTML.
pub fn edit_text_raw(tag: &Tag) -> Result<(String, bool)> {
    let mut r = Reader::new(&tag.body);
    r.u16()?;
    Rect::read(&mut r)?;
    let f1 = r.u8()?;
    let f2 = r.u8()?;
    if f1 & 0x01 != 0 {
        r.u16()?;
    }
    if f2 & 0x80 != 0 {
        r.string()?;
    }
    if f1 & 0x01 != 0 {
        r.u16()?;
    }
    if f1 & 0x04 != 0 {
        r.skip(4)?;
    }
    if f1 & 0x02 != 0 {
        r.u16()?;
    }
    if f2 & 0x20 != 0 {
        r.skip(9)?;
    }
    r.string()?;
    if f1 & 0x80 == 0 {
        return Ok((String::new(), f2 & 0x02 != 0));
    }
    Ok((r.string()?, f2 & 0x02 != 0))
}

pub fn html_to_plain(s: &str) -> String {
    strip_html(s)
}

fn strip_html(s: &str) -> String {
    let mut out = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match (in_tag, c) {
            (false, '<') => {
                in_tag = true;
                tag.clear();
            }
            (true, '>') => {
                in_tag = false;
                let t = tag.trim().to_ascii_lowercase();
                if (t == "br" || t == "br/" || t == "/p" || t == "/li") && !out.ends_with('\n') {
                    out.push('\n');
                }
            }
            (true, c) => tag.push(c),
            (false, c) => out.push(c),
        }
    }
    let out = out.trim_end_matches('\n');
    out.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&nbsp;", "\u{a0}").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    #[test]
    fn html_to_plain() {
        assert_eq!(super::strip_html("<p align=\"left\"><font face=\"A\">Hi &amp; bye</font></p><p>2</p>"), "Hi & bye\n2");
    }
}
