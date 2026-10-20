//! DefineText/DefineText2 records and FFDec's formatted text for them (StaticTextTag's
//! getFormattedText and setFormattedText, with its TextLexer).

use std::collections::{HashMap, HashSet};

use crate::action::asm::ParseError;
use crate::io::JavaRound;
use crate::error::{Error, Result};
use crate::export::font::Font;
use crate::io::{Reader, Writer};
use crate::tags::{code::DEFINE_TEXT2, Tag};
use crate::types::Rect;
use crate::Swf;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TextMatrix {
    pub scale: Option<(f32, f32)>,
    pub rotate: Option<(f32, f32)>,
    pub tx: i32,
    pub ty: i32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Record {
    /// Font id and text height.
    pub font: Option<(u16, u16)>,
    /// RGB for DefineText (alpha 255), RGBA for DefineText2.
    pub color: Option<[u8; 4]>,
    pub x: Option<i16>,
    pub y: Option<i16>,
    /// Glyph index and advance.
    pub glyphs: Vec<(u32, i32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StaticText {
    pub id: u16,
    pub rgba: bool,
    pub bounds: Rect,
    pub matrix: TextMatrix,
    pub records: Vec<Record>,
}

fn fb(r: &mut Reader, n: u32) -> Result<f32> {
    Ok(r.sb(n)? as f32 / 65536.0)
}

fn bits_s(cur: u32, v: i32) -> u32 {
    cur.max(crate::io::bits_s(&[v]))
}

fn bits_u(cur: u32, v: u32) -> u32 {
    if v == 0 { cur } else { cur.max(32 - v.leading_zeros()) }
}

fn to_fixed(v: f32) -> i32 {
    (v as f64 * 65536.0) as i64 as i32
}

impl StaticText {
    pub fn read(tag: &Tag) -> Result<StaticText> {
        let rgba = tag.code == DEFINE_TEXT2;
        let mut r = Reader::new(&tag.body);
        let id = r.u16()?;
        let bounds = Rect::read(&mut r)?;
        let mut matrix = TextMatrix::default();
        if r.bit()? {
            let n = r.ub(5)?;
            matrix.scale = Some((fb(&mut r, n)?, fb(&mut r, n)?));
        }
        if r.bit()? {
            let n = r.ub(5)?;
            matrix.rotate = Some((fb(&mut r, n)?, fb(&mut r, n)?));
        }
        let n = r.ub(5)?;
        matrix.tx = r.sb(n)?;
        matrix.ty = r.sb(n)?;
        r.align();
        let gb = r.u8()? as u32;
        let ab = r.u8()? as u32;
        let mut records = Vec::new();
        loop {
            let flags = r.u8()?;
            if flags == 0 {
                break;
            }
            let mut rec = Record::default();
            let font = if flags & 0x08 != 0 { Some(r.u16()?) } else { None };
            if flags & 0x04 != 0 {
                rec.color = Some([r.u8()?, r.u8()?, r.u8()?, if rgba { r.u8()? } else { 255 }]);
            }
            if flags & 0x01 != 0 {
                rec.x = Some(r.i16()?);
            }
            if flags & 0x02 != 0 {
                rec.y = Some(r.i16()?);
            }
            if let Some(f) = font {
                rec.font = Some((f, r.u16()?));
            }
            let count = r.u8()?;
            for _ in 0..count {
                rec.glyphs.push((r.ub(gb)?, r.sb(ab)?));
            }
            r.align();
            records.push(rec);
        }
        Ok(StaticText { id, rgba, bounds, matrix, records })
    }

    /// The tag body, with the smallest glyph and advance fields, as FFDec writes it.
    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u16(self.id);
        self.bounds.write(&mut w);
        let m = &self.matrix;
        w.ub(1, m.scale.is_some() as u32);
        if let Some((sx, sy)) = m.scale {
            let n = bits_s(bits_s(0, to_fixed(sx)), to_fixed(sy));
            w.ub(5, n);
            w.sb(n, to_fixed(sx));
            w.sb(n, to_fixed(sy));
        }
        w.ub(1, m.rotate.is_some() as u32);
        if let Some((r0, r1)) = m.rotate {
            let n = bits_s(bits_s(0, to_fixed(r0)), to_fixed(r1));
            w.ub(5, n);
            w.sb(n, to_fixed(r0));
            w.sb(n, to_fixed(r1));
        }
        let n = bits_s(bits_s(0, m.tx), m.ty);
        w.ub(5, n);
        w.sb(n, m.tx);
        w.sb(n, m.ty);
        w.flush_bits();
        let (mut gb, mut ab) = (0, 0);
        for g in self.records.iter().flat_map(|r| &r.glyphs) {
            gb = bits_u(gb, g.0);
            ab = bits_s(ab, g.1);
        }
        w.u8(gb as u8);
        w.u8(ab as u8);
        for rec in &self.records {
            let flags = 0x80 | (rec.font.is_some() as u8) << 3 | (rec.color.is_some() as u8) << 2 | (rec.y.is_some() as u8) << 1 | rec.x.is_some() as u8;
            w.u8(flags);
            if let Some((f, _)) = rec.font {
                w.u16(f);
            }
            if let Some(c) = rec.color {
                w.bytes(&c[..if self.rgba { 4 } else { 3 }]);
            }
            if let Some(x) = rec.x {
                w.i16(x);
            }
            if let Some(y) = rec.y {
                w.i16(y);
            }
            if let Some((_, h)) = rec.font {
                w.u16(h);
            }
            w.u8(rec.glyphs.len() as u8);
            for &(g, a) in &rec.glyphs {
                w.ub(gb, g);
                w.sb(ab, a);
            }
            w.flush_bits();
        }
        w.u8(0);
        w.data
    }
}

struct Fonts<'a> {
    swf: &'a Swf,
    loaded: HashMap<u16, Option<Font>>,
}

impl<'a> Fonts<'a> {
    fn get(&mut self, id: u16) -> Option<&Font> {
        let swf = self.swf;
        self.loaded
            .entry(id)
            .or_insert_with(|| swf.find_character(id).and_then(|p| swf.tag(&p)).and_then(|t| Font::read(swf, t).ok()))
            .as_ref()
    }
}

fn glyph_char(font: &Font, glyph: u32) -> char {
    font.glyphs.get(glyph as usize).and_then(|g| char::from_u32(g.code as u32)).unwrap_or('\0')
}

fn char_glyph(font: &Font, c: char) -> Option<u32> {
    font.glyphs.iter().position(|g| g.code as u32 == c as u32).map(|i| i as u32)
}

fn has_layout(font: &Font) -> bool {
    font.ascent.is_some()
}

/// The system font FFDec measures a font without layout with: the one named in the tag, or
/// the part before an underscore, else Times New Roman or Arial.
fn system_face(font: &Font) -> Option<(Vec<u8>, u32)> {
    use crate::render::raster::fonts;
    use resvg::usvg::fontdb::{Family, Query, Style, Weight};
    let db = fonts();
    let weight = if font.bold { Weight::BOLD } else { Weight::NORMAL };
    let style = if font.italic { Style::Italic } else { Style::Normal };
    let names = [Some(font.name.as_str()), font.name.split_once('_').map(|p| p.0), Some("Times New Roman"), Some("Arial")];
    let id = names.into_iter().flatten().filter(|n| !n.is_empty()).find_map(|n| {
        let id = db.query(&Query { families: &[Family::Name(n)], weight, style, ..Default::default() })?;
        let face = db.face(id)?;
        face.families.iter().any(|(f, _)| f.eq_ignore_ascii_case(n)).then_some(id)
    })?;
    db.with_face_data(id, |data, index| (data.to_vec(), index))
}

/// FFDec's getAdvance: from the font's layout, or for a font without one from the system font
/// at the text's whole pixel size, as Java's AWT measures it with fractional metrics.
fn advance(font: &Font, glyph: u32, height: i32, c: char, next: Option<char>) -> Option<i32> {
    if has_layout(font) {
        let kern = next.map_or(0, |n| font.kerning.iter().find(|k| k.0 as u32 == c as u32 && k.1 as u32 == n as u32).map_or(0, |k| k.2 as i32));
        let adv = font.glyphs.get(glyph as usize).and_then(|g| g.advance).unwrap_or(0) as f64;
        return Some((height as f64 * (adv + kern as f64) / (font.divider * 1024.0)).jround() as i32);
    }
    let size = (height as f64 / 20.0) as i32;
    let (data, index) = system_faces(font)?;
    let face = ttf_parser::Face::parse(&data, index).ok()?;
    let units = face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).unwrap_or(0) as f32;
    let px = units * size as f32 / face.units_per_em() as f32;
    Some((20.0 * px as f64).jround() as i32)
}

fn system_faces(font: &Font) -> Option<(Vec<u8>, u32)> {
    thread_local! {
        static CACHE: std::cell::RefCell<HashMap<(String, bool, bool), Option<(Vec<u8>, u32)>>> = Default::default();
    }
    CACHE.with(|c| c.borrow_mut().entry((font.name.clone(), font.bold, font.italic)).or_insert_with(|| system_face(font)).clone())
}

fn detect_letter_spacing(rec: &Record, font: &Font, height: i32) -> Option<i32> {
    let g = &rec.glyphs;
    if g.len() < 2 {
        return Some(0);
    }
    let (mut min, mut negatives, mut num_min) = (i32::MAX, 0, 0);
    for i in 0..g.len() - 1 {
        let c = glyph_char(font, g[i].0);
        let next = glyph_char(font, g[i + 1].0);
        let ls = g[i].1 - advance(font, g[i].0, height, c, Some(next))?;
        if ls < 0 {
            negatives += 1;
        }
        if ls == min {
            num_min += 1;
        }
        if ls < min {
            min = ls;
            num_min = 1;
        }
    }
    if min < 0 && negatives < g.len() / 2 {
        min = 0;
    }
    Some(if num_min == 1 { 0 } else { min })
}

fn escape(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            '\u{8}' => o.push_str("\\b"),
            '\u{c}' => o.push_str("\\f"),
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            c if (c as u32) < 32 => o.push_str(&format!("\\x{:02x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

/// Java's Float.toString.
fn java_float(v: f32) -> String {
    let a = v.abs();
    if a != 0.0 && !(1e-3..1e7).contains(&a) {
        let s = format!("{v:e}");
        let (m, e) = s.split_once('e').unwrap();
        let m = if m.contains('.') { m.to_string() } else { format!("{m}.0") };
        return format!("{m}E{e}");
    }
    let s = format!("{v:?}");
    if s.contains('.') { s } else { format!("{s}.0") }
}

fn has_style(r: &Record) -> bool {
    r.font.is_some() || r.color.is_some() || r.x.is_some() || r.y.is_some()
}

/// FFDec's formatted text: bounds and matrix, then each record's style in brackets and its
/// characters, with letter spacing that differs from the font's advances spelled out.
pub fn formatted(swf: &Swf, t: &StaticText, ignore_letter_spacing: bool) -> String {
    let mut fonts = Fonts { swf, loaded: HashMap::new() };
    let mut w = String::from("[\n");
    w += &format!("xmin {}\nymin {}\nxmax {}\nymax {}\n", t.bounds.x_min, t.bounds.y_min, t.bounds.x_max, t.bounds.y_max);
    if t.matrix.tx != 0 {
        w += &format!("translatex {}\n", t.matrix.tx);
    }
    if t.matrix.ty != 0 {
        w += &format!("translatey {}\n", t.matrix.ty);
    }
    if let Some((sx, sy)) = t.matrix.scale {
        w += &format!("scalexf {}\nscaleyf {}\n", java_float(sx), java_float(sy));
    }
    if let Some((r0, r1)) = t.matrix.rotate {
        w += &format!("rotateskew0f {}\nrotateskew1f {}\n", java_float(r0), java_float(r1));
    }
    w += "]";
    let (mut font_id, mut height, mut prev_ls) = (None::<u16>, 12, 0);
    for rec in &t.records {
        let mut ls = 0;
        if has_style(rec) {
            w += "[\n";
            if let Some((f, h)) = rec.font {
                if fonts.get(f).is_some() {
                    font_id = Some(f);
                }
                w += &format!("font {f}\nheight {h}\n");
                height = h as i32;
            }
            if let Some(font) = font_id.and_then(|f| fonts.get(f)) {
                if !ignore_letter_spacing {
                    ls = detect_letter_spacing(rec, font, height).unwrap_or(0);
                    if ls != prev_ls {
                        w += &format!("letterspacing {ls}\n");
                    }
                    prev_ls = ls;
                }
            }
            if let Some([r, g, b, a]) = rec.color {
                w += &if t.rgba { format!("color #{a:02x}{r:02x}{g:02x}{b:02x}\n") } else { format!("color #{r:02x}{g:02x}{b:02x}\n") };
            }
            if let Some(x) = rec.x {
                w += &format!("x {x}\n");
            }
            if let Some(y) = rec.y {
                w += &format!("y {y}\n");
            }
        }
        let Some(font) = font_id.and_then(|f| fonts.get(f)) else {
            w += &format!("Font {} not found", rec.font.map_or(0, |f| f.0));
            continue;
        };
        let spaced = !ignore_letter_spacing;
        let chars: Vec<char> = rec.glyphs.iter().map(|g| glyph_char(font, g.0)).collect();
        let key = |i: usize| -> String { chars[i..(i + 2).min(chars.len())].iter().collect() };
        let delta = |i: usize| rec.glyphs[i].1 - advance(font, rec.glyphs[i].0, height, chars[i], chars.get(i + 1).copied()).unwrap_or(0);
        let mut spacing: Vec<(String, i32)> = Vec::new();
        let mut ignored = HashSet::new();
        let mut char_spacing: Vec<(char, Vec<i32>)> = Vec::new();
        let mut char_count: HashMap<char, usize> = HashMap::new();
        if spaced && rec.glyphs.len() > 1 {
            for i in 0..rec.glyphs.len() {
                let (c, d, k) = (chars[i], delta(i), key(i));
                let v = d - ls;
                match char_spacing.iter_mut().find(|e| e.0 == c) {
                    Some(e) if !e.1.contains(&v) => e.1.push(v),
                    Some(_) => {}
                    None => char_spacing.push((c, vec![v])),
                }
                *char_count.entry(c).or_default() += 1;
                if d != ls {
                    if ignored.contains(&k) {
                        continue;
                    }
                    if let Some(p) = spacing.iter().position(|e| e.0 == k) {
                        if spacing[p].1 != v {
                            spacing.remove(p);
                            ignored.insert(k);
                            continue;
                        }
                    } else {
                        spacing.push((k, v));
                    }
                }
            }
        }
        let mut simple: Vec<(char, i32)> = Vec::new();
        spacing.retain(|(k, _)| {
            let c = k.chars().next().unwrap();
            let set = &char_spacing.iter().find(|e| e.0 == c).unwrap().1;
            if set.len() == 1 && char_count[&c] > 1 {
                if !simple.iter().any(|s| s.0 == c) {
                    simple.push((c, set[0]));
                }
                false
            } else {
                true
            }
        });
        if !has_style(rec) && (!simple.is_empty() || !spacing.is_empty()) {
            w += "[\nresetspacing\n";
        }
        for (c, v) in &simple {
            w += &format!("spacing \"{}\" {v}\n", escape(&c.to_string()));
        }
        for (k, v) in &spacing {
            let mut it = k.chars();
            let first = it.next().unwrap();
            let second: String = it.collect();
            w += &format!("spacingpair \"{}\" \"{}\" {v}\n", escape(&first.to_string()), escape(&second));
        }
        if has_style(rec) || !simple.is_empty() || !spacing.is_empty() {
            w += "]";
        }
        for i in 0..chars.len() {
            match chars[i] {
                '[' => w += "\\[",
                ']' => w += "\\]",
                '\\' => w += "\\\\",
                c => w.push(c),
            }
            if spaced && !spacing.iter().any(|e| e.0 == key(i)) && !simple.iter().any(|s| s.0 == chars[i]) {
                let d = delta(i);
                if d != ls {
                    w += &format!("[space {}]", d - ls);
                }
            }
        }
    }
    w
}

#[derive(Debug, PartialEq)]
enum Tok {
    Param(String),
    Value(String),
    Text(Option<String>),
}

#[derive(PartialEq)]
enum State {
    Initial,
    Parameter,
    Value,
}

struct Lexer {
    s: Vec<char>,
    pos: usize,
    line: usize,
    state: State,
    finished: bool,
}

fn is_param_start(c: char) -> bool {
    c.is_ascii_lowercase() || c == '_'
}

fn is_param(c: char) -> bool {
    is_param_start(c) || c.is_ascii_digit()
}

impl Lexer {
    fn err<T>(&self, message: impl Into<String>) -> std::result::Result<T, ParseError> {
        Err(ParseError { line: self.line, message: message.into() })
    }

    fn at(&self, i: usize) -> Option<char> {
        self.s.get(i).copied()
    }

    fn newline(&mut self) {
        let c = self.s[self.pos];
        self.pos += 1;
        if c == '\r' && self.at(self.pos) == Some('\n') {
            self.pos += 1;
        }
        self.line += 1;
    }

    fn hex(&self, from: usize, n: usize) -> Option<char> {
        let t: String = self.s.get(from..from + n)?.iter().collect();
        if t.len() == n && t.chars().all(|c| c.is_ascii_hexdigit()) { char::from_u32(u32::from_str_radix(&t, 16).ok()?) } else { None }
    }

    /// A quoted value; `end` closes it and newlines are skipped (the lexer's error fallback).
    fn quoted(&mut self, end: char) -> std::result::Result<Option<String>, ParseError> {
        let mut out = String::new();
        let mut repeat = 1;
        loop {
            let Some(c) = self.at(self.pos) else { return Ok(None) };
            if c == end {
                self.pos += 1;
                return Ok(Some(out));
            }
            if c == '\r' || c == '\n' {
                self.newline();
                if end == '\u{a7}' {
                    return Ok(None);
                }
                continue;
            }
            if c != '\\' {
                out.extend(std::iter::repeat_n(c, repeat));
                repeat = 1;
                self.pos += 1;
                continue;
            }
            let e = self.at(self.pos + 1);
            let simple = match e {
                Some('b') => Some('\u{8}'),
                Some('t') => Some('\t'),
                Some('n') => Some('\n'),
                Some('f') => Some('\u{c}'),
                Some('r') => Some('\r'),
                Some('\\') => Some('\\'),
                Some('\u{a7}') => Some('\u{a7}'),
                Some('"') if end == '"' => Some('"'),
                Some('\'') if end == '"' => Some('\''),
                _ => None,
            };
            if let Some(v) = simple {
                out.extend(std::iter::repeat_n(v, repeat));
                repeat = 1;
                self.pos += 2;
            } else if let Some(v) = (e == Some('x')).then(|| self.hex(self.pos + 2, 2)).flatten() {
                out.extend(std::iter::repeat_n(v, repeat));
                repeat = 1;
                self.pos += 4;
            } else if let Some(v) = (e == Some('u')).then(|| self.hex(self.pos + 2, 4)).flatten() {
                out.extend(std::iter::repeat_n(v, repeat));
                repeat = 1;
                self.pos += 6;
            } else if e == Some('{') && self.s[self.pos + 2..].iter().position(|&c| c == '}').is_some_and(|n| n > 0 && self.s[self.pos + 2..self.pos + 2 + n].iter().all(char::is_ascii_digit)) {
                let n = self.s[self.pos + 2..].iter().position(|&c| c == '}').unwrap();
                repeat = self.s[self.pos + 2..self.pos + 2 + n].iter().collect::<String>().parse().unwrap_or(1);
                self.pos += n + 3;
            } else if end == '"' {
                repeat = 1;
                self.pos += 2;
            } else {
                return self.err(format!("Illegal escape sequence \"\\{}\"", e.unwrap_or(' ')));
            }
        }
    }

    fn next_value(&mut self) -> std::result::Result<Option<String>, ParseError> {
        match self.lex()? {
            Some(Tok::Param(v)) | Some(Tok::Value(v)) => Ok(Some(v)),
            _ => Ok(None),
        }
    }

    fn lex(&mut self) -> std::result::Result<Option<Tok>, ParseError> {
        let mut text: Option<String> = None;
        loop {
            let Some(c) = self.at(self.pos) else {
                if self.state != State::Initial || self.finished {
                    return Ok(None);
                }
                self.finished = true;
                return Ok(Some(Tok::Text(text)));
            };
            match self.state {
                State::Initial => {
                    if c == '[' {
                        self.pos += 1;
                        self.state = State::Parameter;
                        if text.is_some() {
                            return Ok(Some(Tok::Text(text)));
                        }
                        continue;
                    }
                    let t = text.get_or_insert_with(String::new);
                    if c == '\\' {
                        let e = self.at(self.pos + 1);
                        let v = match e {
                            Some('[') => '[',
                            Some(']') => ']',
                            Some('b') => '\u{8}',
                            Some('t') => '\t',
                            Some('n') => '\n',
                            Some('f') => '\u{c}',
                            Some('r') => '\r',
                            Some('"') => '"',
                            Some('\'') => '\'',
                            Some('\\') => '\\',
                            Some('x') => match self.hex(self.pos + 2, 2) {
                                Some(v) => {
                                    t.push(v);
                                    self.pos += 4;
                                    continue;
                                }
                                None => return self.err("Illegal escape sequence \"\\x\""),
                            },
                            Some('\r') | Some('\n') | None => {
                                t.push('\\');
                                self.pos += 1;
                                continue;
                            }
                            Some(o) => return self.err(format!("Illegal escape sequence \"\\{o}\"")),
                        };
                        t.push(v);
                        self.pos += 2;
                    } else {
                        if c == '\n' || (c == '\r' && self.at(self.pos + 1) != Some('\n')) {
                            self.line += 1;
                        }
                        t.push(c);
                        self.pos += 1;
                    }
                }
                State::Parameter | State::Value => {
                    if c == ' ' {
                        self.pos += 1;
                    } else if c == '\r' || c == '\n' {
                        self.newline();
                    } else if c == ']' {
                        self.pos += 1;
                        self.state = State::Initial;
                    } else if self.state == State::Parameter {
                        if is_param_start(c) {
                            let n = self.s[self.pos..].iter().take_while(|&&c| is_param(c)).count();
                            let v: String = self.s[self.pos..self.pos + n].iter().collect();
                            self.pos += n;
                            self.state = State::Value;
                            return Ok(Some(Tok::Param(v)));
                        }
                        self.pos += 1;
                    } else if c == '"' || c == '\u{a7}' {
                        self.pos += 1;
                        if let Some(v) = self.quoted(c)? {
                            return Ok(Some(Tok::Value(v)));
                        }
                    } else {
                        let n = self.s[self.pos..].iter().take_while(|&&c| !matches!(c, ' ' | '\r' | '\n' | ']' | '"' | '\u{a7}')).count();
                        let p = if is_param_start(c) { self.s[self.pos..].iter().take_while(|&&c| is_param(c)).count() } else { 0 };
                        let v: String = self.s[self.pos..self.pos + n].iter().collect();
                        self.pos += n;
                        return Ok(Some(if p == n { Tok::Param(v) } else { Tok::Value(v) }));
                    }
                }
            }
        }
    }
}

fn int(lx: &mut Lexer, what: &str) -> std::result::Result<i32, ParseError> {
    let v = lx.next_value()?.unwrap_or_default();
    v.parse().or_else(|_| lx.err(format!("Invalid {what} - number expected. Found: {v}")))
}

fn float(lx: &mut Lexer, what: &str) -> std::result::Result<f32, ParseError> {
    let v = lx.next_value()?.unwrap_or_default();
    v.trim().parse().or_else(|_| lx.err(format!("Invalid {what} value - float number expected. Found: {v}")))
}

fn color(lx: &mut Lexer, rgba: bool) -> std::result::Result<[u8; 4], ParseError> {
    let v = lx.next_value()?.unwrap_or_default();
    let hex = v.strip_prefix('#').filter(|h| h.len() == if rgba { 8 } else { 6 } && h.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
    let Some(h) = hex else {
        return lx.err(if rgba { format!("Invalid color. Valid format is #aarrggbb. Found: {v}") } else { format!("Invalid color. Valid format is #rrggbb. Found: {v}") });
    };
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap();
    Ok(if rgba { [b(2), b(4), b(6), b(0)] } else { [b(0), b(2), b(4), 255] })
}

/// Parses formatted text into `t`. `texts`, when given, replaces each text run in turn, which
/// is how FFDec imports plain records into an existing layout.
pub fn set_formatted(swf: &Swf, t: &mut StaticText, formatted: &str, texts: Option<&[String]>) -> std::result::Result<(), ParseError> {
    let mut fonts = Fonts { swf, loaded: HashMap::new() };
    let mut lx = Lexer { s: formatted.chars().collect(), pos: 0, line: 1, state: State::Initial, finished: false };
    let mut records: Vec<Record> = Vec::new();
    let mut color_v: Option<[u8; 4]> = None;
    let (mut font_id, mut height, mut letter_spacing) = (None::<u16>, -1i32, 0i32);
    let mut font: Option<u16> = None;
    let (mut x, mut y) = (None::<i16>, None::<i16>);
    let mut matrix = TextMatrix::default();
    let mut bounds = Rect::default();
    let mut text_idx = 0;
    let mut simple: HashMap<char, i32> = HashMap::new();
    let mut pairs: HashMap<String, i32> = HashMap::new();
    let mut append = false;
    let fx = |v: f32, m: &mut Option<(f32, f32)>, first: bool| {
        let (a, b) = m.unwrap_or((0.0, 0.0));
        *m = Some(if first { (v, b) } else { (a, v) });
    };
    while let Some(tok) = lx.lex()? {
        match tok {
            Tok::Param(name) => {
                if matches!(name.as_str(), "font" | "height" | "letterspacing" | "color" | "x" | "y" | "resetspacing") {
                    simple.clear();
                    pairs.clear();
                }
                if name != "space" {
                    append = false;
                }
                match name.as_str() {
                    "resetspacing" => {}
                    "color" => color_v = Some(color(&mut lx, t.rgba)?),
                    "font" => {
                        let v = lx.next_value()?.unwrap_or_default();
                        let Ok(id) = v.parse::<u16>() else { return lx.err(format!("Invalid font id - number expected. Found: {v}")) };
                        if fonts.get(id).is_none() {
                            return lx.err("Font not found.");
                        }
                        font_id = Some(id);
                        font = Some(id);
                    }
                    "height" => height = int(&mut lx, "font height")?,
                    "letterspacing" => letter_spacing = int(&mut lx, "font letter spacing")?,
                    "x" => x = Some(int(&mut lx, "x position")? as i16),
                    "y" => y = Some(int(&mut lx, "y position")? as i16),
                    "xmin" => bounds.x_min = int(&mut lx, "xmin position")?,
                    "xmax" => bounds.x_max = int(&mut lx, "xmax position")?,
                    "ymin" => bounds.y_min = int(&mut lx, "ymin position")?,
                    "ymax" => bounds.y_max = int(&mut lx, "ymax position")?,
                    "scalex" => fx(int(&mut lx, "scalex value")? as f32 / 65536.0, &mut matrix.scale, true),
                    "scaley" => fx(int(&mut lx, "scaley value")? as f32 / 65536.0, &mut matrix.scale, false),
                    "scalexf" => fx(float(&mut lx, "scalexf")?, &mut matrix.scale, true),
                    "scaleyf" => fx(float(&mut lx, "scaleyf")?, &mut matrix.scale, false),
                    "rotateskew0" => fx(int(&mut lx, "rotateskew0 value")? as f32 / 65536.0, &mut matrix.rotate, true),
                    "rotateskew1" => fx(int(&mut lx, "rotateskew1 value")? as f32 / 65536.0, &mut matrix.rotate, false),
                    "rotateskew0f" => fx(float(&mut lx, "rotateskew0")?, &mut matrix.rotate, true),
                    "rotateskew1f" => fx(float(&mut lx, "rotateskew1")?, &mut matrix.rotate, false),
                    "translatex" => matrix.tx = int(&mut lx, "translatex value")?,
                    "translatey" => matrix.ty = int(&mut lx, "translatey value")?,
                    "space" => {
                        let space = int(&mut lx, "space value")?;
                        let Some(last) = records.last_mut() else { return lx.err("space parameter must be placed after some text") };
                        if let Some(g) = last.glyphs.last_mut() {
                            g.1 += space;
                        }
                    }
                    "spacing" => {
                        let c = lx.next_value()?.unwrap_or_default();
                        if c.chars().count() != 1 {
                            return lx.err(format!("Invalid spacing character - single character expected. Found: {c}"));
                        }
                        let v = int(&mut lx, "spacing value")?;
                        simple.insert(c.chars().next().unwrap(), v);
                    }
                    "spacingpair" => {
                        let c1 = lx.next_value()?.unwrap_or_default();
                        if c1.chars().count() != 1 {
                            return lx.err(format!("Invalid spacing character1 - single character expected. Found: {c1}"));
                        }
                        let c2 = lx.next_value()?.unwrap_or_default();
                        if c2.chars().count() > 1 {
                            return lx.err(format!("Invalid spacing character2 - single character expected. Found: {c2}"));
                        }
                        let v = int(&mut lx, "spacing value")?;
                        pairs.insert(c1 + &c2, v);
                    }
                    _ => return lx.err(format!("Unrecognized parameter name: {name}")),
                }
            }
            Tok::Value(_) => {}
            Tok::Text(v) => {
                let txt = match texts {
                    Some(ts) if text_idx < ts.len() => {
                        text_idx += 1;
                        Some(ts[text_idx - 1].clone())
                    }
                    _ => v,
                };
                let Some(txt) = txt else { continue };
                if font.is_none() && txt.is_empty() {
                    continue;
                }
                let Some(fid) = font else { return lx.err("Font not defined") };
                let f = fonts.get(fid).unwrap();
                let txt = txt.trim_matches(|c| c == '\r' || c == '\n');
                let chars: Vec<char> = txt.chars().collect();
                if let Some(&c) = chars.iter().find(|&&c| char_glyph(f, c).is_none()) {
                    return lx.err(format!("Font {fid} has no character '{c}'"));
                }
                if !append {
                    let mut rec = Record::default();
                    if let Some(id) = font_id.take() {
                        rec.font = Some((id, height as u16));
                    }
                    rec.color = color_v.take();
                    rec.x = x.take();
                    rec.y = y.take();
                    records.push(rec);
                }
                let rec = records.last_mut().unwrap();
                for (i, &c) in chars.iter().enumerate() {
                    let next = chars.get(i + 1).copied();
                    let g = char_glyph(f, c).unwrap();
                    let Some(mut adv) = advance(f, g, height, c, next) else {
                        return lx.err(format!("Font {fid} has no layout and no system font to measure it with"));
                    };
                    adv += letter_spacing;
                    if let Some(s) = simple.get(&c) {
                        adv += s;
                    }
                    let key: String = std::iter::once(c).chain(next).collect();
                    if let Some(s) = pairs.get(&key) {
                        adv += s;
                    }
                    rec.glyphs.push((g, adv));
                }
                if rec.glyphs.len() > 255 {
                    return lx.err("A text record holds at most 255 characters");
                }
                append = true;
            }
        }
    }
    t.records = records;
    t.matrix = matrix;
    t.bounds = bounds;
    Ok(())
}

/// Replaces a DefineText/2 like FFDec's TextImporter: formatted text when it starts with a
/// bracket line, otherwise plain records, separated as text export writes them, fitted into the
/// existing layout.
pub fn import(swf: &mut Swf, path: &[usize], text: &str) -> std::result::Result<(), ParseError> {
    let fail = |e: Error| ParseError { line: 0, message: e.to_string() };
    let tag = swf.tag(path).ok_or(ParseError { line: 0, message: "tag not found".into() })?;
    let mut t = StaticText::read(tag).map_err(fail)?;
    let text = text.replace("\r\n", "\n");
    let sep = "\n--- RECORDSEPARATOR ---\n";
    if !text.contains(sep) && text.starts_with("[\n") {
        set_formatted(swf, &mut t, &text, None)?;
    } else {
        let mut records: Vec<String> = text.split(sep).map(str::to_string).collect();
        while records.len() > 1 && records.last().is_some_and(String::is_empty) {
            records.pop();
        }
        let current = formatted(swf, &t, false);
        set_formatted(swf, &mut t, &current, Some(&records))?;
    }
    let body = t.write();
    swf.set_tag_body(path, body).map_err(fail)
}
