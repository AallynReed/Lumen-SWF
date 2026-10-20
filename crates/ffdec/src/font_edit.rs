//! DefineFont2/DefineFont3 written back, and glyphs added from TrueType and OpenType fonts like
//! FFDec's FontTag.addCharacter and SHAPERECORD.fontCharacterToSHAPE.

use crate::error::{Error, Result};
use crate::io::{JavaRound, Reader, Writer};
use crate::shape::{bounds, index_bits, read_records, write_records, Record};
use crate::tags::code::{DEFINE_FONT2, DEFINE_FONT3};
use crate::types::Rect;
use crate::Swf;

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub ascent: u16,
    pub descent: u16,
    pub leading: i16,
    pub advances: Vec<u16>,
    pub bounds: Vec<Rect>,
    pub kerning: Vec<(u16, u16, i16)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Font2 {
    pub id: u16,
    pub v3: bool,
    /// HasLayout 0x80, ShiftJIS 0x40, SmallText 0x20, ANSI 0x10, WideOffsets 0x08,
    /// WideCodes 0x04, Italic 0x02, Bold 0x01.
    pub flags: u8,
    pub language: u8,
    pub name: Vec<u8>,
    /// GFx fonts may strip their shapes, leaving a zero first offset.
    pub stripped: bool,
    pub glyphs: Vec<Vec<Record>>,
    pub codes: Vec<u16>,
    pub layout: Option<Layout>,
}

fn glyph_bytes(records: &[Record]) -> Vec<u8> {
    let mut w = Writer::new();
    let (fb, lb) = index_bits(records);
    w.ub(4, fb);
    w.ub(4, lb);
    write_records(&mut w, records, fb, lb, 1);
    w.data
}

impl Font2 {
    pub fn read(code: u16, body: &[u8]) -> Result<Font2> {
        let mut r = Reader::new(body);
        let id = r.u16()?;
        let flags = r.u8()?;
        let language = r.u8()?;
        let len = r.u8()? as usize;
        let name = r.bytes(len)?.to_vec();
        let n = r.u16()? as usize;
        let base = r.pos();
        let wide = flags & 0x08 != 0;
        let off = |r: &mut Reader| -> Result<usize> { Ok(if wide { r.u32()? as usize } else { r.u16()? as usize }) };
        let mut offsets = Vec::with_capacity(n);
        let mut stripped = false;
        if n > 0 {
            let first = off(&mut r)?;
            stripped = first == 0;
            offsets.push(first);
            if !stripped {
                for _ in 1..n {
                    offsets.push(off(&mut r)?);
                }
            }
        }
        let code_table = if n > 0 || flags & 0x80 != 0 { off(&mut r)? } else { 0 };
        let mut glyphs = Vec::with_capacity(n);
        for i in 0..n {
            glyphs.push(if stripped {
                Vec::new()
            } else {
                let mut gr = Reader::new(body.get(base + offsets[i]..).ok_or_else(|| Error::Other("glyph offset out of range".into()))?);
                let fb = gr.ub(4)?;
                let lb = gr.ub(4)?;
                read_records(&mut gr, 1, fb, lb)?
            });
        }
        let mut r = Reader::new(body.get(base + code_table..).ok_or_else(|| Error::Other("code table out of range".into()))?);
        let wide_codes = flags & 0x04 != 0;
        let mut codes = Vec::with_capacity(n);
        for _ in 0..n {
            codes.push(if wide_codes { r.u16()? } else { r.u8()? as u16 });
        }
        let layout = if flags & 0x80 != 0 {
            let (ascent, descent, leading) = (r.u16()?, r.u16()?, r.i16()?);
            let advances = (0..n).map(|_| r.u16()).collect::<Result<_>>()?;
            let bounds = (0..n).map(|_| Rect::read(&mut r)).collect::<Result<_>>()?;
            let mut kerning = Vec::new();
            if !r.is_empty() {
                for _ in 0..r.u16()? {
                    let (a, b) = if wide_codes { (r.u16()?, r.u16()?) } else { (r.u8()? as u16, r.u8()? as u16) };
                    kerning.push((a, b, r.i16()?));
                }
            }
            Some(Layout { ascent, descent, leading, advances, bounds, kerning })
        } else {
            None
        };
        Ok(Font2 { id, v3: code == DEFINE_FONT3, flags, language, name, stripped, glyphs, codes, layout })
    }

    pub fn code(&self) -> u16 {
        if self.v3 { DEFINE_FONT3 } else { DEFINE_FONT2 }
    }

    /// The tag body, widening offsets and codes when they no longer fit, as FFDec does.
    pub fn write(&mut self) -> Vec<u8> {
        let shapes: Vec<Vec<u8>> = if self.stripped { Vec::new() } else { self.glyphs.iter().map(|g| glyph_bytes(g)).collect() };
        let n = self.glyphs.len();
        let total: usize = shapes.iter().map(Vec::len).sum();
        if self.flags & 0x08 == 0 && !self.stripped && (n + 1) * 2 + total > 0xffff {
            self.flags |= 0x08;
        }
        if self.codes.iter().any(|&c| c > 0xff) {
            self.flags |= 0x04;
        }
        let wide = self.flags & 0x08 != 0;
        let mut w = Writer::new();
        w.u16(self.id);
        w.u8(self.flags);
        w.u8(self.language);
        w.u8(self.name.len() as u8);
        w.bytes(&self.name);
        w.u16(n as u16);
        let off = |w: &mut Writer, v: usize| if wide { w.u32(v as u32) } else { w.u16(v as u16) };
        let table = (if self.stripped { 2 } else { n + 1 }) * if wide { 4 } else { 2 };
        if self.stripped {
            if n > 0 {
                off(&mut w, 0);
            }
            if n > 0 || self.layout.is_some() {
                off(&mut w, table);
            }
        } else {
            let mut at = table;
            for s in &shapes {
                off(&mut w, at);
                at += s.len();
            }
            if n > 0 || self.layout.is_some() {
                off(&mut w, at);
            }
            for s in &shapes {
                w.bytes(s);
            }
        }
        let wide_codes = self.flags & 0x04 != 0;
        for &c in &self.codes {
            if wide_codes { w.u16(c) } else { w.u8(c as u8) }
        }
        if let Some(l) = &self.layout {
            w.u16(l.ascent);
            w.u16(l.descent);
            w.i16(l.leading);
            for &a in &l.advances {
                w.u16(a);
            }
            for b in &l.bounds {
                b.write(&mut w);
            }
            w.u16(l.kerning.len() as u16);
            for &(a, b, k) in &l.kerning {
                if wide_codes {
                    w.u16(a);
                    w.u16(b);
                } else {
                    w.u8(a as u8);
                    w.u8(b as u8);
                }
                w.i16(k);
            }
        }
        w.data
    }

    fn divider(&self) -> f64 {
        if self.v3 { 20.0 } else { 1.0 }
    }
}

/// Collects a glyph outline the way Java's AWT reports it: y down, points rounded.
struct Outline {
    scale: f64,
    multiplier: i32,
    recs: Vec<Record>,
    last: (i32, i32),
    start: (i32, i32),
}

impl Outline {
    fn p(&self, x: f32, y: f32) -> (f64, f64) {
        (x as f64 * self.scale, -(y as f64) * self.scale)
    }

    fn quad(&mut self, c: (f64, f64), a: (f64, f64)) {
        let (cx, cy) = (c.0.jround() as i32, c.1.jround() as i32);
        let (ax, ay) = (a.0.jround() as i32, a.1.jround() as i32);
        let m = self.multiplier;
        self.recs.push(Record::Curved { cdx: m * (cx - self.last.0), cdy: m * (cy - self.last.1), adx: m * (ax - cx), ady: m * (ay - cy) });
        self.last = (ax, ay);
    }
}

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        let (px, py) = self.p(x, y);
        let (rx, ry) = (px.jround() as i32, py.jround() as i32);
        self.recs.push(Record::StyleChange { move_to: Some((self.multiplier * rx, self.multiplier * ry)), fill0: None, fill1: None, line: None, new_styles: None });
        self.last = (rx, ry);
        self.start = (rx, ry);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (px, py) = self.p(x, y);
        let (rx, ry) = (px.jround() as i32, py.jround() as i32);
        let m = self.multiplier;
        self.recs.push(Record::Straight { dx: m * (rx - self.last.0), dy: m * (ry - self.last.1) });
        self.last = (rx, ry);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (c, a) = (self.p(x1, y1), self.p(x, y));
        self.quad(c, a);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let r = |p: (f64, f64)| (p.0.jround(), p.1.jround());
        let (p0, p1, p2, p3) = ((self.last.0 as f64, self.last.1 as f64), r(self.p(x1, y1)), r(self.p(x2, y2)), r(self.p(x, y)));
        for [c, a] in approximate_cubic(p0, p1, p2, p3) {
            self.quad(c, a);
        }
    }

    fn close(&mut self) {
        if self.start != self.last {
            let m = self.multiplier;
            self.recs.push(Record::Straight { dx: m * (self.start.0 - self.last.0), dy: m * (self.start.1 - self.last.1) });
            self.last = self.start;
        }
    }
}

type Pt = (f64, f64);

fn on_segment(a: Pt, b: Pt, r: f64) -> Pt {
    (a.0 + (b.0 - a.0) * r, a.1 + (b.1 - a.1) * r)
}

/// SHAPERECORD.approximateCubic (from Apache FOP): four quadratic curves.
fn approximate_cubic(p0: Pt, p1: Pt, p2: Pt, p3: Pt) -> [[Pt; 2]; 4] {
    let pa = on_segment(p0, p1, 0.75);
    let pb = on_segment(p3, p2, 0.75);
    let (dx, dy) = ((p3.0 - p0.0) / 16.0, (p3.1 - p0.1) / 16.0);
    let pc1 = on_segment(p0, p1, 3.0 / 8.0);
    let pc2 = on_segment(pa, pb, 3.0 / 8.0);
    let pc2 = (pc2.0 - dx, pc2.1 - dy);
    let pc3 = on_segment(pb, pa, 3.0 / 8.0);
    let pc3 = (pc3.0 + dx, pc3.1 + dy);
    let pc4 = on_segment(p3, p2, 3.0 / 8.0);
    [[pc1, on_segment(pc1, pc2, 0.5)], [pc2, on_segment(pa, pb, 0.5)], [pc3, on_segment(pc3, pc4, 0.5)], [pc4, p3]]
}

/// fontCharacterToSHAPE: the glyph at `size` units per em (above 1024, at 1024 and scaled up by
/// a whole factor), with its fill style set on the first record; plus its advance at 1024.
/// Format and line controls are empty, as Java's AWT draws them.
fn glyph(face: &ttf_parser::Face, c: char, size: f64) -> Option<(Vec<Record>, i32)> {
    let empty = || vec![Record::StyleChange { move_to: None, fill0: Some(1), fill1: None, line: None, new_styles: None }];
    if matches!(c as u32, 0x09 | 0x0a | 0x0d | 0x200c..=0x200f | 0x2028..=0x202e | 0x206a..=0x206f) {
        return Some((empty(), 0));
    }
    let gid = face.glyph_index(c)?;
    let (size, multiplier) = if size > 1024.0 { (1024.0, (size / 1024.0) as i32) } else { (size, 1) };
    let upem = face.units_per_em() as f64;
    let mut o = Outline { scale: size / upem, multiplier, recs: Vec::new(), last: (0, 0), start: (0, 0) };
    face.outline_glyph(gid, &mut o);
    let mut recs = o.recs;
    if !matches!(recs.first(), Some(Record::StyleChange { .. })) {
        recs.insert(0, Record::StyleChange { move_to: None, fill0: None, fill1: None, line: None, new_styles: None });
    }
    if let Some(Record::StyleChange { fill0, .. }) = recs.first_mut() {
        *fill0 = Some(1);
    }
    let advance = (face.glyph_hor_advance(gid).unwrap_or(0) as f64 * 1024.0 / upem).jround() as i32;
    Some((recs, advance))
}

fn kerning(face: &ttf_parser::Face, a: char, b: char, size: f64) -> i16 {
    let (Some(ga), Some(gb)) = (face.glyph_index(a), face.glyph_index(b)) else { return 0 };
    let Some(kern) = face.tables().kern else { return 0 };
    let v = kern.subtables.into_iter().filter(|s| s.horizontal && !s.variable).find_map(|s| s.glyphs_kerning(ga, gb)).unwrap_or(0);
    (v as f64 * size / face.units_per_em() as f64).jround() as i16
}

/// Moves glyph indices at or after `from` of the font's static texts by `by`, as FFDec's
/// shiftGlyphIndices does when glyphs are inserted.
fn shift_texts(swf: &mut Swf, font: u16, from: u32) -> Result<()> {
    let mut paths = Vec::new();
    for (i, t) in swf.timeline.tags.iter().enumerate() {
        if crate::export::text::is_text(t.code) && t.code != crate::tags::code::DEFINE_EDIT_TEXT {
            paths.push(i);
        }
    }
    for i in paths {
        let tag = &swf.timeline.tags[i];
        let mut text = crate::text::StaticText::read(tag)?;
        let mut cur = 0;
        let mut changed = false;
        for r in text.records.iter_mut() {
            if let Some((f, _)) = r.font {
                cur = f;
            }
            if cur != font {
                continue;
            }
            for g in r.glyphs.iter_mut() {
                if g.0 >= from {
                    g.0 += 1;
                    changed = true;
                }
            }
        }
        if changed {
            swf.set_tag_body(&[i], text.write())?;
        }
    }
    Ok(())
}

/// Adds or replaces the glyphs of `chars` in the DefineFont2/3 at `path`, from the TrueType or
/// OpenType font `data`. Characters the font file lacks are left alone; the count of those set
/// is returned.
pub fn add_characters(swf: &mut Swf, path: &[usize], data: &[u8], chars: &[char]) -> Result<usize> {
    let face = ttf_parser::Face::parse(data, 0).map_err(|e| Error::Other(format!("reading the font: {e}")))?;
    let tag = swf.tag(path).ok_or_else(|| Error::Other(format!("no tag at {path:?}")))?;
    if tag.code != DEFINE_FONT2 && tag.code != DEFINE_FONT3 {
        return Err(Error::Other(format!("{} fonts cannot take characters from a font file", tag.name())));
    }
    let code = tag.code;
    let mut font = Font2::read(code, &tag.body)?;
    if font.stripped {
        return Err(Error::Other("this font has no glyph shapes".into()));
    }
    let divider = font.divider() as i32;
    let size = (font.divider() * 1024.0).jround();
    let mut done = 0;
    for &c in chars {
        let Ok(code) = u16::try_from(c as u32) else { continue };
        let Some((shape, adv1024)) = glyph(&face, c, size) else { continue };
        let pos = font.codes.iter().position(|&k| k == code);
        let exists = pos.is_some();
        let pos = pos.unwrap_or_else(|| font.codes.iter().position(|&k| k > code).unwrap_or(font.codes.len()));
        if exists {
            font.glyphs[pos] = shape.clone();
        } else {
            shift_texts(swf, font.id, pos as u32)?;
            font.glyphs.insert(pos, shape.clone());
            font.codes.insert(pos, code);
        }
        if let Some(l) = font.layout.as_mut() {
            let adv = (divider * adv1024).clamp(0, 0xffff) as u16;
            let b = bounds(&shape, &[], 1, false);
            if exists {
                l.advances[pos] = adv;
                l.bounds[pos] = b;
            } else {
                l.advances.insert(pos, adv);
                l.bounds.insert(pos, b);
            }
            l.kerning.retain(|k| k.0 != code && k.1 != code);
            for &other in &font.codes {
                let Some(o) = char::from_u32(other as u32) else { continue };
                for (a, b, ca, cb) in [(c, o, code, other), (o, c, other, code)] {
                    let k = kerning(&face, a, b, size);
                    if k != 0 && !l.kerning.iter().any(|e| e.0 == ca && e.1 == cb) {
                        l.kerning.push((ca, cb, k));
                    }
                }
            }
        }
        done += 1;
    }
    let path = swf.find_character(font.id).ok_or_else(|| Error::Other("font not found".into()))?;
    let body = font.write();
    swf.set_tag(&path, code, body)?;
    Ok(done)
}

/// The characters a DefineFont2/3 has glyphs for.
pub fn characters(swf: &Swf, path: &[usize]) -> Result<Vec<char>> {
    let tag = swf.tag(path).ok_or_else(|| Error::Other(format!("no tag at {path:?}")))?;
    let font = Font2::read(tag.code, &tag.body)?;
    Ok(font.codes.iter().filter_map(|&c| char::from_u32(c as u32)).collect())
}
