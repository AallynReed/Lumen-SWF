//! DefineFont/2/3 glyph tables, and their conversion to TrueType like FFDec's `FontExporter`.

use crate::error::{Error, Result};
use crate::io::Reader;
use crate::shape::{self, Point};
use crate::tags::{code::*, Tag};
use crate::Swf;

use super::ttf;

pub fn is_font(code: u16) -> bool {
    matches!(code, DEFINE_FONT | DEFINE_FONT2 | DEFINE_FONT3 | DEFINE_FONT4)
}

pub struct Font {
    pub id: u16,
    pub name: String,
    pub copyright: String,
    pub bold: bool,
    pub italic: bool,
    /// Glyph coordinates are in EM units of 1024 × divider (20 for DefineFont3).
    pub divider: f64,
    pub ascent: Option<u16>,
    pub descent: Option<u16>,
    pub leading: Option<i16>,
    pub glyphs: Vec<Glyph>,
    /// Layout kerning pairs: first code, second code, adjustment.
    pub kerning: Vec<(u16, u16, i16)>,
}

pub struct Glyph {
    pub shape: Vec<shape::Record>,
    pub code: u16,
    pub advance: Option<u16>,
}

impl Font {
    /// Parses a DefineFont/2/3. DefineFont takes its codes from DefineFontInfo/2, and the
    /// name of any version can come from DefineFontName.
    pub fn read(swf: &Swf, tag: &Tag) -> Result<Font> {
        let mut r = Reader::new(&tag.body);
        let id = r.u16()?;
        let mut font = match tag.code {
            DEFINE_FONT => read_v1(id, &mut r, swf)?,
            DEFINE_FONT2 | DEFINE_FONT3 => read_v2(id, &mut r, tag.code == DEFINE_FONT3)?,
            _ => return Err(Error::Other(format!("{} has no glyph table", tag.name()))),
        };
        swf.walk(|_, t| {
            if t.code == DEFINE_FONT_NAME && t.character_id() == Some(id) {
                let mut r = Reader::new(&t.body);
                let _ = r.u16();
                if let (Ok(name), Ok(copy)) = (r.string(), r.string()) {
                    if font.name.is_empty() {
                        font.name = name;
                    }
                    font.copyright = copy;
                }
            }
        });
        Ok(font)
    }

    pub fn char_for(&self, glyph: usize) -> Option<char> {
        self.glyphs.get(glyph).and_then(|g| char::from_u32(g.code as u32))
    }

    pub fn to_ttf(&self) -> Vec<u8> {
        let d = self.divider;
        let tx = |v: i32| (v as f64 / d).ceil() as i32;
        let ty = |v: i32| -((v as f64 / d).ceil() as i32);
        let mut seen = std::collections::HashSet::new();
        let mut glyphs = Vec::new();
        for (i, g) in self.glyphs.iter().enumerate() {
            let mut unicode = g.code as u32;
            // Missing or duplicate codes go to the Private Use Area, as FFDec does.
            if unicode == 0 || !seen.insert(unicode) {
                unicode = 0xE000 + i as u32;
            }
            let contours: Vec<Vec<Point>> = shape::contours(&g.shape)
                .into_iter()
                .map(|c| c.into_iter().map(|p| Point { x: tx(p.x), y: ty(p.y), on: p.on }).collect())
                .collect();
            let advance = match g.advance {
                Some(a) => (a as f64 / d).round().max(0.0) as u16,
                None => {
                    let (min, max) = contours.iter().flatten().fold((i32::MAX, i32::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
                    let w = if min <= max { max - min } else { 0 };
                    (w + 100).max(0) as u16
                }
            };
            glyphs.push(ttf::Glyph { unicode, advance, contours });
        }
        if !glyphs.iter().any(|g| g.unicode == 32) {
            glyphs.push(ttf::Glyph { unicode: 32, advance: ttf::UNITS_PER_EM / 4, contours: Vec::new() });
        }
        let metric = |v: Option<u16>, default: i16| v.map_or(default, |v| ((v as f64 / d).round() as i32).min(1024) as i16);
        let family = if self.name.trim().is_empty() { "noname".to_string() } else { self.name.trim().to_string() };
        ttf::build(&ttf::Font {
            family,
            copyright: self.copyright.clone(),
            bold: self.bold,
            italic: self.italic,
            ascent: metric(self.ascent, 1024 * 4 / 5),
            descent: metric(self.descent, 1024 / 5),
            line_gap: self.leading.map_or(0, |l| (l as f64 / d).round() as i16),
            glyphs,
        })
    }
}

fn read_v1(id: u16, r: &mut Reader, swf: &Swf) -> Result<Font> {
    let body = r.rest();
    let first = u16::from_le_bytes([*body.first().unwrap_or(&0), *body.get(1).unwrap_or(&0)]) as usize;
    let n = first / 2;
    let mut offsets = Vec::with_capacity(n + 1);
    let mut or = Reader::new(body);
    for _ in 0..n {
        offsets.push(or.u16()? as usize);
    }
    let glyphs = offsets
        .iter()
        .map(|&o| Glyph { shape: body.get(o..).and_then(|d| shape::read_glyph(d).ok()).unwrap_or_default(), code: 0, advance: None })
        .collect();
    let mut font = Font {
        id,
        name: String::new(),
        copyright: String::new(),
        bold: false,
        italic: false,
        divider: 1.0,
        ascent: None,
        descent: None,
        leading: None,
        glyphs,
        kerning: Vec::new(),
    };
    swf.walk(|_, t| {
        if (t.code == DEFINE_FONT_INFO || t.code == DEFINE_FONT_INFO2) && t.character_id() == Some(id) {
            let _ = apply_font_info(&mut font, t);
        }
    });
    Ok(font)
}

fn apply_font_info(font: &mut Font, t: &Tag) -> Result<()> {
    let mut r = Reader::new(&t.body);
    r.u16()?;
    let len = r.u8()? as usize;
    font.name = name_from(r.bytes(len)?);
    let flags = r.u8()?;
    font.italic = flags & 0x04 != 0;
    font.bold = flags & 0x02 != 0;
    let wide = flags & 0x01 != 0 || t.code == DEFINE_FONT_INFO2;
    if t.code == DEFINE_FONT_INFO2 {
        r.u8()?;
    }
    for g in font.glyphs.iter_mut() {
        g.code = if wide { r.u16()? } else { r.u8()? as u16 };
    }
    Ok(())
}

fn name_from(b: &[u8]) -> String {
    String::from_utf8_lossy(b).trim_end_matches('\0').to_string()
}

fn read_v2(id: u16, r: &mut Reader, v3: bool) -> Result<Font> {
    let flags = r.u8()?;
    let has_layout = flags & 0x80 != 0;
    let wide_offsets = flags & 0x08 != 0;
    let wide_codes = flags & 0x04 != 0;
    r.u8()?;
    let len = r.u8()? as usize;
    let name = name_from(r.bytes(len)?);
    let n = r.u16()? as usize;
    let body = r.clone().rest();
    let off = |r: &mut Reader| -> Result<usize> { Ok(if wide_offsets { r.u32()? as usize } else { r.u16()? as usize }) };
    let mut offsets = Vec::with_capacity(n);
    for _ in 0..n {
        offsets.push(off(r)?);
    }
    // A zero first offset marks GFx fonts with stripped shapes.
    let stripped = offsets.first() == Some(&0);
    let code_table = if n > 0 || has_layout { off(r)? } else { 0 };
    let glyphs = offsets
        .iter()
        .map(|&o| Glyph {
            shape: if stripped { Vec::new() } else { body.get(o..).and_then(|d| shape::read_glyph(d).ok()).unwrap_or_default() },
            code: 0,
            advance: None,
        })
        .collect();
    let mut font = Font {
        id,
        name,
        copyright: String::new(),
        bold: flags & 0x01 != 0,
        italic: flags & 0x02 != 0,
        divider: if v3 { 20.0 } else { 1.0 },
        ascent: None,
        descent: None,
        leading: None,
        glyphs,
        kerning: Vec::new(),
    };
    let mut cr = Reader::new(body.get(code_table..).unwrap_or(&[]));
    for g in font.glyphs.iter_mut() {
        g.code = if wide_codes { cr.u16()? } else { cr.u8()? as u16 };
    }
    if has_layout {
        font.ascent = Some(cr.u16()?);
        font.descent = Some(cr.u16()?);
        font.leading = Some(cr.i16()?);
        for g in font.glyphs.iter_mut() {
            g.advance = Some(cr.u16()?);
        }
        let kerning = (|| -> Result<Vec<(u16, u16, i16)>> {
            for _ in 0..n {
                crate::types::Rect::read(&mut cr)?;
            }
            let k = cr.u16()?;
            (0..k)
                .map(|_| {
                    let (a, b) = if wide_codes { (cr.u16()?, cr.u16()?) } else { (cr.u8()? as u16, cr.u8()? as u16) };
                    Ok((a, b, cr.i16()?))
                })
                .collect()
        })();
        font.kerning = kerning.unwrap_or_default();
    }
    Ok(font)
}

/// DefineFont4 carries a complete OpenType (CFF) font.
pub fn font4_data(tag: &Tag) -> Result<&[u8]> {
    let mut r = Reader::new(&tag.body);
    r.u16()?;
    let flags = r.u8()?;
    r.string()?;
    if flags & 0x04 == 0 {
        return Err(Error::Other("DefineFont4 has no font data".into()));
    }
    Ok(r.rest())
}

pub fn font4_name(tag: &Tag) -> String {
    let mut r = Reader::new(&tag.body);
    let _ = r.skip(3);
    r.string().unwrap_or_default()
}
