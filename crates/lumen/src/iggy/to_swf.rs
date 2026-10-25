//! An Iggy movie as an uncompressed SWF.

use std::collections::HashSet;

use super::file::IggyFile;
use super::model::{bytes_at, reference, wstring_at, Font, Text, Timeline, FLAG_AUTO_SIZE, FLAG_BORDER, FLAG_HTML, FLAG_MULTILINE, FLAG_NO_SELECT, FLAG_PASSWORD, FLAG_READ_ONLY, FLAG_USE_OUTLINES, FLAG_WAS_STATIC, FLAG_WORD_WRAP};
use super::stream::err;
use super::swf::{java_round, Swf as Movie};
use crate::error::Result;
use crate::io::{bits_s, Writer};
use crate::shape::{self, index_bits, write_records, Record};
use crate::tags::{code::*, Tag};
use crate::types::Rect;

/// Iggy blend codes by SWF blend mode.
pub const BLEND: [u32; 15] = [1, 1, 2, 3, 4, 5, 6, 9, 7, 8, 10, 11, 12, 13, 14];

/// `Math.round(double)`.
pub fn jround(v: f64) -> i64 {
    if v.is_nan() {
        0
    } else {
        (v + 0.5).floor() as i64
    }
}

/// Read access to the main block, as `IggyTimeline` gives the converters.
#[derive(Clone, Copy)]
pub struct Src<'a> {
    pub data: &'a [u8],
    pub wide: bool,
}

impl<'a> Src<'a> {
    pub fn bytes(&self, a: u64, n: u64) -> Result<&'a [u8]> {
        bytes_at(self.data, a, n)
    }

    pub fn u8(&self, a: u64) -> Result<u32> {
        Ok(self.bytes(a, 1)?[0] as u32)
    }

    pub fn u16(&self, a: u64) -> Result<u32> {
        let b = self.bytes(a, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]) as u32)
    }

    pub fn i16(&self, a: u64) -> Result<i32> {
        Ok(self.u16(a)? as u16 as i16 as i32)
    }

    pub fn i32(&self, a: u64) -> Result<i32> {
        let b = self.bytes(a, 4)?;
        Ok(i32::from_le_bytes(b.try_into().unwrap()))
    }

    /// `IggyShapeStyles.u32`: a count that must fit an int.
    pub fn count(&self, a: u64) -> Result<u64> {
        let v = self.i32(a)?;
        if v < 0 {
            return err("Iggy shape count exceeds supported size");
        }
        Ok(v as u64)
    }

    pub fn f32(&self, a: u64) -> Result<f32> {
        Ok(f32::from_bits(self.i32(a)? as u32))
    }

    pub fn color(&self, a: u64) -> Result<[u8; 4]> {
        Ok(self.bytes(a, 4)?.try_into().unwrap())
    }

    pub fn reference(&self, f: u64) -> Result<u64> {
        reference(self.data, f, self.wide)
    }

    pub fn string(&self, a: u64) -> Result<Option<String>> {
        wstring_at(self.data, a)
    }

    fn ptr(&self) -> u64 {
        if self.wide {
            8
        } else {
            4
        }
    }
}

fn twips(v: f32) -> i32 {
    java_round(v * 20.0)
}

fn rect_at(s: &Src, at: u64) -> Result<Rect> {
    Ok(Rect { x_min: twips(s.f32(at)?), x_max: twips(s.f32(at + 8)?), y_min: twips(s.f32(at + 4)?), y_max: twips(s.f32(at + 12)?) })
}

fn fx(f: f32) -> i32 {
    (f as f64 * 65536.0) as i32
}

/// A MATRIX with float scale and skew.
#[derive(Clone, Copy, Debug)]
pub struct Mx {
    pub has_scale: bool,
    pub sx: f32,
    pub sy: f32,
    pub has_rotate: bool,
    pub r0: f32,
    pub r1: f32,
    pub tx: i32,
    pub ty: i32,
}

impl Mx {
    pub fn write(&self, w: &mut Writer) {
        w.ub(1, self.has_scale as u32);
        if self.has_scale {
            let (a, b) = (fx(self.sx), fx(self.sy));
            let n = bits_s(&[a, b]);
            w.ub(5, n);
            w.sb(n, a);
            w.sb(n, b);
        }
        w.ub(1, self.has_rotate as u32);
        if self.has_rotate {
            let (a, b) = (fx(self.r0), fx(self.r1));
            let n = bits_s(&[a, b]);
            w.ub(5, n);
            w.sb(n, a);
            w.sb(n, b);
        }
        let n = bits_s(&[self.tx, self.ty]);
        w.ub(5, n);
        w.sb(n, self.tx);
        w.sb(n, self.ty);
        w.flush_bits();
    }

    /// The values after a write and `readMatrix`: FB fields come back as float(int) / 65536.
    fn reread(self) -> Mx {
        let r = |f: f32| fx(f) as f32 / 65536.0;
        Mx { sx: r(self.sx), sy: r(self.sy), r0: r(self.r0), r1: r(self.r1), ..self }
    }
}

/// `IggyShapeStyles.matrix`: scale and skew times `linear`, translation in twips.
pub fn matrix_at(s: &Src, at: u64, linear: f64) -> Result<Mx> {
    s.bytes(at, 24)?;
    let g = |o: u64| s.f32(at + o);
    Ok(Mx {
        has_scale: true,
        has_rotate: true,
        sx: (g(0)? as f64 * linear) as f32,
        r1: (g(4)? as f64 * linear) as f32,
        r0: (g(8)? as f64 * linear) as f32,
        sy: (g(12)? as f64 * linear) as f32,
        tx: twips(g(16)?),
        ty: twips(g(20)?),
    })
}

/// `CXFORMWITHALPHA`.
#[derive(Clone, Copy, Debug)]
pub struct Cx {
    pub add: bool,
    pub mult: bool,
    pub m: [i32; 4],
    pub a: [i32; 4],
}

impl Cx {
    pub fn write(&self, w: &mut Writer) {
        w.ub(1, self.add as u32);
        w.ub(1, self.mult as u32);
        let mut n = 1;
        if self.mult {
            n = n.max(bits_s(&self.m));
        }
        if self.add {
            n = n.max(bits_s(&self.a));
        }
        w.ub(4, n);
        if self.mult {
            for v in self.m {
                w.sb(n, v);
            }
        }
        if self.add {
            for v in self.a {
                w.sb(n, v);
            }
        }
        w.flush_bits();
    }

    pub fn multi(&self, i: usize) -> i32 {
        if self.mult {
            self.m[i]
        } else {
            256
        }
    }

    pub fn addi(&self, i: usize) -> i32 {
        if self.add {
            self.a[i]
        } else {
            0
        }
    }
}

/// The 28-byte color transform; `button` records may leave out the alpha terms.
fn cx_at(s: &Src, at: u64, button: bool) -> Result<Cx> {
    let c = s.bytes(at, 28)?;
    let f = |o: u64| -> Result<i32> { Ok(java_round(s.f32(at + o)? * 256.0)) };
    let no_alpha = button && c[0] == 0;
    Ok(Cx {
        add: c[1] != 0,
        mult: c[2] != 0,
        m: [f(4)?, f(8)?, f(12)?, if no_alpha { 256 } else { f(16)? }],
        a: [s.i16(at + 20)?, s.i16(at + 22)?, s.i16(at + 24)?, if no_alpha { 0 } else { s.i16(at + 26)? }],
    })
}

fn compose(o: &Cx, i: &Cx) -> Cx {
    let mut r = Cx { add: true, mult: true, m: [0; 4], a: [0; 4] };
    for k in 0..4 {
        r.m[k] = java_round((o.multi(k) * i.multi(k)) as f32 / 256.0);
        r.a[k] = java_round((o.multi(k) * i.addi(k)) as f32 / 256.0) + o.addi(k);
    }
    r
}

fn ufixed8(w: &mut Writer, v: f32) {
    w.u16((v * 256.0) as i32 as u16);
}

fn fixed(w: &mut Writer, v: f32) {
    w.u32((v as f64 * 65536.0) as i64 as u32);
}

fn swf_string(w: &mut Writer, s: &str) {
    w.string(s);
}

fn tag(code: u16, body: Vec<u8>, long: bool) -> Tag {
    let mut t = Tag::new(code, body);
    t.long_header = long;
    t
}

fn count(w: &mut Writer, n: usize) {
    if n >= 0xff {
        w.u8(0xff);
        w.u16(n as u16);
    } else {
        w.u8(n as u8);
    }
}

// ---- styles ----

fn read_fill(w: &mut Writer, m: &Movie, s: &Src, at: u64, morph: bool) -> Result<()> {
    let ty = s.u8(at)?;
    let value = if s.wide { 16 } else { 8 };
    let refer = at + if s.wide { 64 } else { 56 };
    w.u8(ty as u8);
    match ty {
        0 => {
            w.bytes(&s.color(at + value)?);
            if morph {
                w.bytes(&s.color(at + value + 4)?);
            }
        }
        16 | 18 | 19 => {
            matrix_at(s, at + value, 20.0)?.reread().write(w);
            if morph {
                matrix_at(s, at + value + 24, if ty == 19 { 1.0 } else { 20.0 })?.reread().write(w);
            }
            let g = s.reference(refer)?;
            let n = s.u8(g + 2)?;
            if n > 15 {
                return err("Invalid Iggy gradient count");
            }
            w.ub(2, s.u8(g)?);
            w.ub(2, s.u8(g + 1)?);
            w.ub(4, n);
            for i in 0..n as u64 {
                let r = g + 12 + 10 * i;
                w.u8(s.u8(r + 8)? as u8);
                w.bytes(&s.color(r)?);
                if morph {
                    w.u8(s.u8(r + 9)? as u8);
                    w.bytes(&s.color(r + 4)?);
                }
            }
            if ty == 19 {
                w.i16(java_round(s.f32(g + 4)? * 256.0) as i16);
                if morph {
                    w.i16(java_round(s.f32(g + 8)? * 256.0) as i16);
                }
            }
        }
        64..=67 => {
            let id = m.dictionary_id(refer, s.reference(refer)?)?;
            w.u16(if id < 0 { 65535 } else { id as u16 });
            matrix_at(s, at + value, 20.0)?.reread().write(w);
            if morph {
                matrix_at(s, at + value + 24, 20.0)?.reread().write(w);
            }
        }
        t => return err(format!("Unsupported Iggy fill style {t}")),
    }
    Ok(())
}

/// A line style's bytes and its no-H-scale and no-V-scale flags.
fn read_line(w: &mut Writer, m: &Movie, s: &Src, at: u64, morph: bool) -> Result<(bool, bool)> {
    let width = if s.wide { 76 } else { 64 };
    w.u16(twips(s.f32(at + width)?) as u16);
    if morph {
        w.u16(twips(s.f32(at + width + 4)?) as u16);
    }
    let cap = s.u8(at + width + 8)?;
    let join = s.u8(at + width + 10)?;
    let flags = s.u8(at + width + 11)?;
    let filled = s.u8(at)? != 0;
    w.ub(2, cap);
    w.ub(2, join);
    w.ub(1, filled as u32);
    w.ub(1, flags & 1);
    w.ub(1, (flags >> 1) & 1);
    w.ub(1, (flags >> 2) & 1);
    w.ub(5, 0);
    w.ub(1, (flags >> 3) & 1);
    w.ub(2, s.u8(at + width + 9)?);
    if join == 2 {
        ufixed8(w, s.f32(at + width - 4)?);
    }
    if filled {
        read_fill(w, m, s, at, morph)?;
    } else {
        w.bytes(&s.color(at + if s.wide { 16 } else { 8 })?);
        if morph {
            w.bytes(&s.color(at + if s.wide { 20 } else { 12 })?);
        }
    }
    Ok((flags & 1 != 0, flags & 2 != 0))
}

// ---- vectors ----

#[derive(Default)]
struct Geometry {
    records: Vec<Record>,
    fills: Vec<Vec<u8>>,
    lines: Vec<Vec<u8>>,
    scaling: Vec<(bool, bool)>,
}

fn geometry(m: &Movie, s: &Src, collection: u64, morph: bool, end: bool) -> Result<Geometry> {
    let wide = s.wide;
    let n = s.u16(collection + if end { 0 } else if wide { 8 } else { 4 })? as u64;
    let groups = s.reference(collection + if end { s.ptr() } else if wide { 24 } else { 12 })?;
    let gs = if wide { 64 } else { 40 };
    s.bytes(groups, n * gs)?;
    let mut r = Geometry::default();
    let (mut x, mut y) = (0i32, 0i32);
    for g in 0..n {
        let group = groups + gs * g;
        let fill_offset = r.fills.len() as u32;
        let line_offset = r.lines.len() as u32;
        let fills = s.reference(group + if wide { 32 } else { 24 })?;
        let lines = s.reference(group + if wide { 40 } else { 28 })?;
        let fc = if !end && fills != 0 { s.count(fills)? } else { 0 };
        let lc = if !end && lines != 0 { s.count(lines)? } else { 0 };
        if fc > 32767 || lc > 32767 || fill_offset as u64 + fc > 32767 || line_offset as u64 + lc > 32767 {
            return err("Too many SWF shape styles");
        }
        for i in 0..fc {
            let mut w = Writer::new();
            read_fill(&mut w, m, s, fills + s.ptr() + if wide { 72 } else { 60 } * i, morph)?;
            r.fills.push(w.data);
        }
        for i in 0..lc {
            let mut w = Writer::new();
            r.scaling.push(read_line(&mut w, m, s, lines + s.ptr() + if wide { 88 } else { 76 } * i, morph)?);
            r.lines.push(w.data);
        }
        let nodes = s.reference(group + 16)?;
        let size = s.count(group + if wide { 24 } else { 20 })?;
        s.bytes(nodes, size * 24)?;
        let (mut f0s, mut f1s, mut ls_) = (-1i64, -1i64, -1i64);
        for i in 0..size {
            let node = nodes + 24 * i;
            let ty = s.u8(node + 16)?;
            let tx = twips(s.f32(node)?);
            let ty_ = twips(s.f32(node + 4)?);
            let f0 = if end { 0 } else { s.u16(node + if morph { 20 } else { 18 })? };
            let f1 = if end { 0 } else { s.u16(node + if morph { 18 } else { 20 })? };
            let ls = if end { 0 } else { s.u16(node + 22)? };
            if f0 as u64 > fc || f1 as u64 > fc || ls as u64 > lc {
                return err("Invalid Iggy node style index");
            }
            if ty == 1 || (!end && (f0 as i64 != f0s || f1 as i64 != f1s || ls as i64 != ls_)) {
                let mut move_to = None;
                if ty == 1 {
                    move_to = Some((tx, ty_));
                    x = tx;
                    y = ty_;
                }
                let (mut a, mut b, mut c) = (None, None, None);
                if !end && (ty != 1 || i == 0) {
                    a = Some(if f0 == 0 { 0 } else { f0 + fill_offset });
                    b = Some(if f1 == 0 { 0 } else { f1 + fill_offset });
                    c = Some(if ls == 0 { 0 } else { ls + line_offset });
                }
                r.records.push(Record::StyleChange { move_to, fill0: a, fill1: b, line: c, new_styles: None });
                if ty != 1 || i == 0 {
                    f0s = f0 as i64;
                    f1s = f1 as i64;
                    ls_ = ls as i64;
                }
            }
            if ty == 2 {
                r.records.push(Record::Straight { dx: tx - x, dy: ty_ - y });
                x = tx;
                y = ty_;
            } else if ty == 3 {
                let cx = twips(s.f32(node + 8)?);
                let cy = twips(s.f32(node + 12)?);
                r.records.push(Record::Curved { cdx: cx - x, cdy: cy - y, adx: tx - cx, ady: ty_ - cy });
                x = tx;
                y = ty_;
            } else if ty != 1 {
                return err(format!("Unsupported Iggy shape node {ty}"));
            }
        }
    }
    Ok(r)
}

fn style_array(w: &mut Writer, items: &[Vec<u8>]) {
    count(w, items.len());
    for i in items {
        w.bytes(i);
    }
}

fn shape_records(w: &mut Writer, records: &[Record], shape_num: u8) {
    let (fb, lb) = index_bits(records);
    w.ub(4, fb);
    w.ub(4, lb);
    write_records(w, records, fb, lb, shape_num);
}

fn stroke_flags(g: &Geometry) -> u8 {
    let non = g.scaling.iter().any(|&(h, v)| h || v);
    let scaling = g.scaling.iter().any(|&(h, v)| !h || !v);
    (non as u8) << 1 | scaling as u8
}

fn vector(m: &Movie, s: &Src, at: u64, id: u16, ty: i32) -> Result<Tag> {
    let wide = s.wide;
    let start = geometry(m, s, at + if wide { 48 } else { 32 }, ty == 2, false)?;
    let mut w = Writer::new();
    w.u16(id);
    rect_at(s, at + if wide { 32 } else { 16 })?.write(&mut w);
    if ty == 1 {
        shape::bounds(&start.records, &[], 4, true).write(&mut w);
        w.u8(stroke_flags(&start));
        style_array(&mut w, &start.fills);
        style_array(&mut w, &start.lines);
        shape_records(&mut w, &start.records, 4);
        return Ok(tag(DEFINE_SHAPE4, w.data, false));
    }
    let end = geometry(m, s, at + if wide { 80 } else { 48 }, true, true)?;
    rect_at(s, at + if wide { 112 } else { 72 })?.write(&mut w);
    shape::bounds(&start.records, &[], 4, true).write(&mut w);
    shape::bounds(&end.records, &[], 4, true).write(&mut w);
    w.u8(stroke_flags(&start));
    let mut inner = Writer::new();
    style_array(&mut inner, &start.fills);
    style_array(&mut inner, &start.lines);
    shape_records(&mut inner, &start.records, 4);
    w.u32(inner.data.len() as u32);
    w.bytes(&inner.data);
    shape_records(&mut w, &end.records, 4);
    Ok(tag(DEFINE_MORPH_SHAPE2, w.data, false))
}

// ---- other assets ----

fn bitmap(file: Option<&IggyFile>, s: &Src, at: u64, id: u16) -> Result<Tag> {
    let wide = s.wide;
    let header = if wide { 32 } else { 16 };
    let width = s.u16(at + header)?;
    let height = s.u16(at + header + 2)?;
    let encoded = s.reference(at + if wide { 72 } else { 48 })?;
    let raw_field = at + if wide { 64 } else { 44 };
    let raw_value = if wide { i64::from_le_bytes(s.bytes(raw_field, 8)?.try_into().unwrap()) } else { s.i32(raw_field)? as i64 };
    let raw = if raw_value == 0 || raw_value == 1 { 0 } else { s.reference(raw_field)? };
    let kind = s.u8(at + header + 5)?;
    let format = s.u8(at + header + 6)?;
    if encoded == 0 && raw == 0 {
        let mut w = Writer::new();
        w.u8(1);
        w.u16(id);
        w.u16(width as u16);
        w.u16(height as u16);
        w.u8(s.u8(at + header + 4)? as u8);
        w.u8(kind as u8);
        w.u8(format as u8);
        w.u32(s.i32(at + if wide { 80 } else { 52 })? as u32);
        for o in if wide { [48, 52, 56, 60] } else { [28, 32, 36, 40] } {
            w.u32(s.i32(at + o)? as u32);
        }
        let (mut ty, mut offset, mut fmt, mut rw, mut rh) = (0u8, u32::MAX, 65535u16, 0u16, 0u16);
        match file.and_then(|f| f.file_name.as_ref()) {
            Some(n) => {
                let stem = n.rfind('.').map_or(n.as_str(), |i| &n[..i]);
                swf_string(&mut w, &format!("{stem}.raw.iggytex"));
                for t in &file.unwrap().textures {
                    for (r, rid) in &t.records {
                        if *rid == id as u32 {
                            ty = 8;
                            offset = u32::from_le_bytes(r[0..4].try_into().unwrap());
                            fmt = u16::from_le_bytes([r[4], r[5]]);
                            rw = u16::from_le_bytes([r[6], r[7]]);
                            rh = u16::from_le_bytes([r[8], r[9]]);
                        }
                    }
                }
            }
            None => swf_string(&mut w, ""),
        }
        w.u8(ty);
        w.u32(offset);
        w.u16(fmt);
        w.u16(rw);
        w.u16(rh);
        w.u32(0);
        return Ok(tag(IGGY_EXTERNAL_IMAGE, w.data, false));
    }
    if encoded == 0 {
        let len = width as u64 * height as u64 * 4;
        if width == 0 || height == 0 || len > 268_435_456 {
            return err("Unsupported Iggy bitmap dimensions");
        }
        let px = s.bytes(raw, len)?;
        let mut data = Vec::with_capacity(len as usize);
        for p in px.chunks(4) {
            let a = p[3] as u32;
            data.push(a as u8);
            for c in [p[0], p[1], p[2]] {
                data.push(((c as u32 * a + 127) / 255) as u8);
            }
        }
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut z, &data).map_err(|e| crate::error::Error::Other(e.to_string()))?;
        let zdata = z.finish().map_err(|e| crate::error::Error::Other(e.to_string()))?;
        let mut w = Writer::new();
        w.u16(id);
        w.u8(5);
        w.u16(width as u16);
        w.u16(height as u16);
        w.bytes(&zdata);
        return Ok(tag(DEFINE_BITS_LOSSLESS2, w.data, true));
    }
    let len = s.count(at + if wide { 80 } else { 52 })?;
    let payload = s.bytes(encoded, len)?;
    let mut w = Writer::new();
    w.u16(id);
    if kind == 20 || kind == 36 {
        w.u8(format as u8);
        w.u16(width as u16);
        w.u16(height as u16);
    }
    w.bytes(payload);
    match kind {
        6 => err("Iggy DefineBits with shared JPEG tables cannot be reconstructed without its tables"),
        20 | 21 | 35 | 36 => Ok(tag(kind as u16, w.data, false)),
        k => err(format!("Unsupported Iggy embedded bitmap tag {k}")),
    }
}

fn static_text(m: &Movie, s: &Src, at: u64, id: u16) -> Result<Tag> {
    let wide = s.wide;
    s.bytes(at, if wide { 80 } else { 60 })?;
    let bounds = rect_at(s, at + if wide { 32 } else { 16 })?;
    let mx = matrix_at(s, at + if wide { 48 } else { 32 }, 1.0)?;
    struct Rec {
        font: i32,
        height: u32,
        x: i32,
        y: i32,
        color: [u8; 4],
        glyphs: Vec<(u32, i32)>,
    }
    let mut recs = Vec::new();
    let mut record = s.reference(at + if wide { 72 } else { 56 })?;
    let mut seen = HashSet::new();
    while record != 0 {
        if !seen.insert(record) {
            return err("Cycle in Iggy static text records");
        }
        s.bytes(record, if wide { 32 } else { 20 })?;
        let font = m.dictionary_id(record + 8, s.reference(record + 8)?)?;
        let n = s.u8(record + 6)? as u64;
        let g = record + if wide { 32 } else { 20 };
        s.bytes(g, 4 * n)?;
        let mut glyphs = Vec::new();
        for i in 0..n {
            glyphs.push((s.u16(g + 4 * i)?, s.i16(g + 4 * i + 2)?));
        }
        if font < 0 && n > 0 {
            return err("Missing Iggy static text font");
        }
        recs.push(Rec { font, height: s.u16(record)?, x: s.i16(record + 2)?, y: s.i16(record + 4)?, color: s.color(record + if wide { 16 } else { 12 })?, glyphs });
        record = s.reference(record + if wide { 24 } else { 16 })?;
    }
    let mut w = Writer::new();
    w.u16(id);
    bounds.write(&mut w);
    mx.write(&mut w);
    let (mut gb, mut ab) = (0, 0);
    for r in &recs {
        for &(i, a) in &r.glyphs {
            gb = gb.max(32 - i.leading_zeros());
            ab = ab.max(bits_s(&[a]));
        }
    }
    w.u8(gb as u8);
    w.u8(ab as u8);
    for r in &recs {
        let has_font = r.font >= 0;
        w.ub(1, 1);
        w.ub(3, 0);
        w.ub(1, has_font as u32);
        w.ub(1, 1);
        w.ub(1, 1);
        w.ub(1, 1);
        if has_font {
            w.u16(r.font as u16);
        }
        w.bytes(&r.color);
        w.i16(r.x as i16);
        w.i16(r.y as i16);
        if has_font {
            w.u16(r.height as u16);
        }
        w.u8(r.glyphs.len() as u8);
        for &(i, a) in &r.glyphs {
            w.ub(gb, i);
            w.sb(ab, a);
        }
        w.flush_bits();
    }
    w.u8(0);
    Ok(tag(DEFINE_TEXT2, w.data, false))
}

fn binary(s: &Src, at: u64, id: u16) -> Result<Tag> {
    let wide = s.wide;
    let len = if wide { u64::from_le_bytes(s.bytes(at + 32, 8)?.try_into().unwrap()) } else { s.i32(at + 16)? as u32 as u64 };
    if len > i32::MAX as u64 {
        return err("Invalid Iggy binary data length");
    }
    let address = s.reference(at + if wide { 40 } else { 20 })?;
    if len > 0 && address == 0 {
        return err("Missing Iggy binary data");
    }
    let mut w = Writer::new();
    w.u16(id);
    w.u32(0);
    w.bytes(s.bytes(address, len)?);
    Ok(tag(DEFINE_BINARY_DATA, w.data, false))
}

const RATES: [u32; 4] = [5500, 11025, 22050, 44100];

fn sound(s: &Src, at: u64, id: u16) -> Result<Tag> {
    let wide = s.wide;
    let header = if wide { 32 } else { 16 };
    let rate = s.u16(at + if wide { 52 } else { 32 })?;
    let Some(ri) = RATES.iter().position(|&r| r == rate) else { return err(format!("Unsupported Iggy sound rate {rate}")) };
    let format = s.u8(at + if wide { 54 } else { 34 })?;
    let size = s.u8(at + if wide { 55 } else { 35 })?;
    let channels = s.u8(at + if wide { 56 } else { 36 })?;
    if (size != 8 && size != 16) || (channels != 1 && channels != 2) {
        return err("Invalid Iggy sound format");
    }
    let samples = s.i32(at + header + 4)? as u32;
    let len = s.count(at + if wide { 48 } else { 28 })?;
    let mut data = if len == 0 { Vec::new() } else { s.bytes(s.reference(at + if wide { 40 } else { 24 })?, len)?.to_vec() };
    if format == 0 && size == 16 {
        if data.len() % 2 != 0 {
            return err("Odd Iggy 16-bit PCM payload size");
        }
        for p in data.chunks_mut(2) {
            p.swap(0, 1);
        }
    }
    let mut w = Writer::new();
    w.u16(id);
    w.u8((format << 4 | (ri as u32) << 2 | ((size == 16) as u32) << 1 | (channels == 2) as u32) as u8);
    w.u32(samples);
    w.bytes(&data);
    Ok(tag(DEFINE_SOUND, w.data, false))
}

/// `IggyTimelineFilters.read`: a 56- or 44-byte filter as its SWF FILTER bytes.
fn filter(s: &Src, at: u64) -> Result<Vec<u8>> {
    let id = s.u8(at)?;
    let passes = s.u8(at + 1)?;
    let flags = s.u8(at + 2)?;
    let mut w = Writer::new();
    w.u8(id as u8);
    let gradient = |w: &mut Writer, n: u32, colors: u64, ratios: u64| -> Result<()> {
        w.u8(n as u8);
        for i in 0..n as u64 {
            w.bytes(&s.color(colors + 4 * i)?);
        }
        for i in 0..n as u64 {
            w.u8(s.u8(ratios + i)? as u8);
        }
        Ok(())
    };
    match id {
        0 | 2 | 4 => {
            if id == 4 {
                let n = s.u8(at + 3)?;
                gradient(&mut w, n, s.reference(at + if s.wide { 32 } else { 28 })?, s.reference(at + if s.wide { 40 } else { 32 })?)?;
            } else {
                w.bytes(&s.color(at + 4)?);
            }
            fixed(&mut w, s.f32(at + 8)?);
            fixed(&mut w, s.f32(at + 12)?);
            if id != 2 {
                fixed(&mut w, s.f32(at + if id == 0 { 16 } else { 20 })?);
                fixed(&mut w, s.f32(at + if id == 0 { 20 } else { 24 })?);
            }
            ufixed8(&mut w, s.f32(at + if id == 0 { 24 } else { 16 })?);
            w.ub(1, flags & 1);
            w.ub(1, (flags >> 1) & 1);
            w.ub(1, 1);
            if id == 4 {
                w.ub(1, (flags >> 2) & 1);
            }
            w.ub(if id == 4 { 4 } else { 5 }, passes);
        }
        1 => {
            fixed(&mut w, s.f32(at + 4)?);
            fixed(&mut w, s.f32(at + 8)?);
            w.ub(5, passes);
            w.ub(3, 0);
        }
        3 | 7 => {
            if id == 3 {
                w.bytes(&s.color(at + 8)?);
                w.bytes(&s.color(at + 12)?);
            } else {
                let n = s.u8(at + 5)?;
                gradient(&mut w, n, s.reference(at + if s.wide { 40 } else { 36 })?, s.reference(at + if s.wide { 48 } else { 40 })?)?;
            }
            for i in 0..4 {
                fixed(&mut w, s.f32(at + 16 + 4 * i)?);
            }
            ufixed8(&mut w, s.f32(at + 32)?);
            w.ub(1, (flags != 0) as u32);
            w.ub(1, (s.u8(at + 3)? != 0) as u32);
            w.ub(1, 1);
            w.ub(1, (s.u8(at + 4)? != 0) as u32);
            w.ub(4, passes);
        }
        5 => {
            let (x, y) = (passes, flags);
            w.u8(x as u8);
            w.u8(y as u8);
            w.u32(s.f32(at + 4)?.to_bits());
            w.u32(s.f32(at + 8)?.to_bits());
            let mx = s.reference(at + 16)?;
            for i in 0..(x * y) as u64 {
                w.u32(s.f32(mx + 4 * i)?.to_bits());
            }
            w.bytes(&s.color(at + 12)?);
            let c = s.u8(at + 3)?;
            w.ub(6, 0);
            w.ub(1, c & 1);
            w.ub(1, (c >> 1) & 1);
        }
        6 => {
            const ORDER: [u64; 20] = [0, 1, 2, 3, 16, 4, 5, 6, 7, 17, 8, 9, 10, 11, 18, 12, 13, 14, 15, 19];
            let cm = s.reference(at + s.ptr())?;
            for &i in &ORDER {
                let v = s.f32(cm + 4 * i)?;
                w.u32((v * if i >= 16 { 255.0 } else { 1.0 }).to_bits());
            }
        }
        _ => return err(format!("Unsupported Iggy timeline filter: {id}")),
    }
    w.flush_bits();
    Ok(w.data)
}

fn blend_mode(b: u32) -> Result<Option<u8>> {
    if b == 15 {
        return Ok(None);
    }
    match (1..BLEND.len()).find(|&j| BLEND[j] == b) {
        Some(j) => Ok(Some(j as u8)),
        None => err(format!("Invalid Iggy blend mode: {b}")),
    }
}

fn button(s: &Src, at: u64, id: u16) -> Result<Tag> {
    let wide = s.wide;
    let h = s.bytes(at, if wide { 88 } else { 60 })?;
    let n = s.i16(at + if wide { 68 } else { 48 })?;
    if n < 0 {
        return err("Invalid Iggy button record count");
    }
    let track = h[if wide { 70 } else { 50 }] != 0;
    let global = if h[if wide { 71 } else { 51 }] == 0 { None } else { Some(cx_at(s, at + if wide { 40 } else { 20 }, true)?) };
    let records = s.reference(at + if wide { 72 } else { 52 })?;
    if n != 0 && records == 0 {
        return err("Missing Iggy button records");
    }
    let stride = if wide { 72 } else { 64 };
    if n != 0 {
        s.bytes(records, n as u64 * stride)?;
    }
    let mut w = Writer::new();
    w.u16(id);
    w.u8(track as u8);
    w.u16(0);
    for i in 0..n as u64 {
        let ra = records + stride * i;
        let d = s.bytes(ra, stride)?;
        let states = d[0] as u32;
        if states & !15 != 0 || states == 0 {
            return err("Invalid Iggy button states");
        }
        let ch = s.u16(ra + 4)?;
        if ch == 0 {
            return err("Invalid Iggy button character ID");
        }
        let mx = matrix_at(s, ra + 36, 1.0)?;
        let mut cx = cx_at(s, ra + 8, true)?;
        if let Some(g) = &global {
            cx = compose(g, &cx);
        }
        let blend = match d[1] as u32 {
            15 => None,
            b => match (1..BLEND.len()).find(|&j| BLEND[j] == b) {
                Some(j) => Some(j as u8),
                None => return err(format!("Invalid Iggy button blend mode {b}")),
            },
        };
        let fc = d[2] as u64;
        let mut filters = Vec::new();
        if fc != 0 {
            let f = s.reference(ra + if wide { 64 } else { 60 })?;
            if f == 0 {
                return err("Missing Iggy button filters");
            }
            for j in 0..fc {
                filters.push(filter(s, f + if wide { 56 } else { 44 } * j)?);
            }
        }
        w.ub(2, 0);
        w.ub(1, blend.is_some() as u32);
        w.ub(1, (fc != 0) as u32);
        w.ub(1, states & 1);
        w.ub(1, (states >> 3) & 1);
        w.ub(1, (states >> 2) & 1);
        w.ub(1, (states >> 1) & 1);
        w.u16(ch as u16);
        w.u16(s.u16(ra + 6)? as u16);
        mx.write(&mut w);
        cx.write(&mut w);
        if fc != 0 {
            w.u8(filters.len() as u8);
            for f in &filters {
                w.bytes(f);
            }
        }
        if let Some(b) = blend {
            w.u8(b);
        }
    }
    w.u8(0);
    Ok(tag(DEFINE_BUTTON2, w.data, false))
}

// ---- fonts and texts ----

fn glyph_records(g: &super::model::Shape, units: f64) -> Vec<Record> {
    let mut out = Vec::new();
    let (mut px, mut py) = (0i32, 0i32);
    let r = |v: f32| jround(v as f64 * units) as i32;
    for n in &g.nodes {
        if n.node_type == 1 {
            px = r(n.target_x);
            py = r(n.target_y);
            out.push(Record::StyleChange { move_to: Some((px, py)), fill0: None, fill1: None, line: None, new_styles: None });
        } else {
            let (x1, y1, x2, y2) = (r(n.target_x), r(n.target_y), r(n.control_x), r(n.control_y));
            if n.node_type == 2 {
                out.push(Record::Straight { dx: x1 - px, dy: y1 - py });
                px = x1;
                py = y1;
            } else if n.node_type == 3 {
                out.push(Record::Curved { cdx: x2 - px, cdy: y2 - py, adx: x1 - x2, ady: y1 - y2 });
                px = x1;
                py = y1;
            }
        }
    }
    if !out.is_empty() {
        if !matches!(out[0], Record::StyleChange { .. }) {
            out.insert(0, Record::StyleChange { move_to: None, fill0: None, fill1: None, line: None, new_styles: None });
        }
        if let Record::StyleChange { fill0, .. } = &mut out[0] {
            *fill0 = Some(1);
        }
    }
    out
}

fn shape_bytes(records: &[Record]) -> Vec<u8> {
    let mut w = Writer::new();
    shape_records(&mut w, records, 1);
    w.data
}

pub fn font_tag(font: &Font) -> Result<Tag> {
    let units = if font.font_type() == 3 { 20480.0 } else { 1024.0 };
    let mut shapes = Vec::new();
    for i in 0..font.char_count as usize {
        shapes.push(match font.glyphs.get(i).and_then(Option::as_ref) {
            Some(g) => glyph_records(g, units),
            None => Vec::new(),
        });
    }
    let shape_data: Vec<Vec<u8>> = shapes.iter().map(|s| shape_bytes(s)).collect();
    if font.code_points.is_none() && !shapes.is_empty() {
        let mut w = Writer::new();
        w.u16(font.font_id as u16);
        let mut off = shapes.len() * 2;
        for d in &shape_data {
            w.u16(off as u16);
            off += d.len();
        }
        for d in &shape_data {
            w.bytes(d);
        }
        return Ok(tag(DEFINE_FONT, w.data, false));
    }
    let layout = font.has_layout();
    let n = shapes.len();
    let mut w = Writer::new();
    w.u16(font.font_id as u16);
    let flags = (layout as u8) << 7 | (font.is_shift_jis() as u8) << 6 | (font.is_small_text() as u8) << 5 | (font.is_ansi() as u8) << 4 | 0x08 | 0x04 | (font.is_italic() as u8) << 1 | font.is_bold() as u8;
    w.u8(flags);
    w.u8(font.language() as u8);
    let name = font.name.clone().unwrap_or_default();
    w.u8(name.len() as u8);
    w.bytes(name.as_bytes());
    w.u16(n as u16);
    let table = (n + 1) * 4;
    let mut off = table;
    for d in &shape_data {
        w.u32(off as u32);
        off += d.len();
    }
    if n > 0 || layout {
        w.u32(off as u32);
    }
    for d in &shape_data {
        w.bytes(d);
    }
    if let Some((codes, _)) = &font.code_points {
        for &c in codes {
            w.u16(c);
        }
    }
    if layout {
        let adv: &[f32] = match &font.advances {
            Some(a) if a.len() == n => a,
            None if n == 0 => &[],
            _ => return err("Missing Iggy font advance table"),
        };
        w.u16(font.ascent as u16);
        w.u16(font.descent as u16);
        w.i16(font.leading as i16);
        for &a in adv {
            w.u16(jround(a as f64 * units) as i32 as u16);
        }
        for s in &shapes {
            shape::bounds(s, &[], 1, false).write(&mut w);
        }
        let k = font.kernings.as_deref().unwrap_or(&[]);
        w.u16(k.len() as u16);
        for &(a, b, o) in k {
            w.u16(a);
            w.u16(b);
            w.i16(o);
        }
    }
    Ok(tag(if font.font_type() == 3 { DEFINE_FONT3 } else { DEFINE_FONT2 }, w.data, false))
}

pub fn edit_text(t: &Text) -> Tag {
    let mut w = Writer::new();
    w.u16(t.text_index as u16);
    let r = |v: f32| jround(v as f64 * 20.0) as i32;
    Rect { x_min: r(t.bounds[0]), x_max: r(t.bounds[2]), y_min: r(t.bounds[1]), y_max: r(t.bounds[3]) }.write(&mut w);
    let f = t.flags;
    let has_font = t.font_index != 0;
    let has_class = t.font_class_name.is_some();
    let has_max = t.max_length != 0;
    let bit = |m: u32| (f & m != 0) as u32;
    for v in [t.initial_text.is_some() as u32, bit(FLAG_WORD_WRAP), bit(FLAG_MULTILINE), bit(FLAG_PASSWORD), bit(FLAG_READ_ONLY), 1, has_max as u32, has_font as u32, has_class as u32, bit(FLAG_AUTO_SIZE), 1, bit(FLAG_NO_SELECT), bit(FLAG_BORDER), bit(FLAG_WAS_STATIC), bit(FLAG_HTML), bit(FLAG_USE_OUTLINES)] {
        w.ub(1, v);
    }
    if has_font {
        w.u16(t.font_index as u16);
    }
    if let Some(c) = &t.font_class_name {
        swf_string(&mut w, c);
    }
    if has_font || has_class {
        w.u16(t.font_height as u16);
    }
    w.bytes(&t.color);
    if has_max {
        w.u16(t.max_length as u16);
    }
    w.u8(t.align as u8);
    w.u16(t.left_margin as u16);
    w.u16(t.right_margin as u16);
    w.u16(t.indent as u16);
    w.i16(t.leading as i16);
    swf_string(&mut w, "");
    if let Some(s) = &t.initial_text {
        swf_string(&mut w, s);
    }
    tag(DEFINE_EDIT_TEXT, w.data, false)
}

// ---- timelines ----

fn sound_info(w: &mut Writer, s: &Src, b: &[u8], address: u64) -> Result<()> {
    let wide = s.wide;
    let start = if wide { 12 } else { 8 };
    let i32_at = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let sync_stop = b[start + 2] != 0;
    let no_multiple = b[start + 3] != 0;
    let inp = i32_at(if wide { 24 } else { 16 }) as u32;
    let out = i32_at(if wide { 28 } else { 20 }) as u32;
    let loops = i32_at(if wide { 36 } else { 28 });
    let n = i32_at(if wide { 40 } else { 32 });
    if !(0..=255).contains(&n) {
        return err("Invalid Iggy sound envelope count");
    }
    let (has_in, has_out, has_loops, has_env) = (inp != 0, out != 0x7fff_ffff, loops != 1, n != 0);
    w.ub(2, 0);
    w.ub(1, sync_stop as u32);
    w.ub(1, no_multiple as u32);
    w.ub(1, has_env as u32);
    w.ub(1, has_loops as u32);
    w.ub(1, has_out as u32);
    w.ub(1, has_in as u32);
    if has_in {
        w.u32(inp);
    }
    if has_out {
        w.u32(out);
    }
    if has_loops {
        w.u16(loops as u16);
    }
    if has_env {
        let e = s.bytes(s.reference(address + if wide { 48 } else { 36 })?, n as u64 * 8)?;
        w.u8(n as u8);
        w.bytes(e);
    }
    Ok(())
}

/// `IggyTimelineConvertor.toSwf`: display commands as SWF tags, padded to `frames`.
pub fn timeline_tags(m: &Movie, t: &Timeline, frames: u64) -> Result<Vec<Tag>> {
    let s = Src { data: m.original_data(), wide: t.is64 };
    let wide = t.is64;
    let p = s.ptr() as usize;
    let mut out = Vec::new();
    let mut actual = 0;
    for c in &t.commands {
        let a = c.address;
        let b = &c.data;
        let u16_at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
        let f32_at = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        match c.ty {
            1 => {
                actual += 1;
                out.push(tag(SHOW_FRAME, Vec::new(), false));
            }
            2 => {
                let flags = b[p + 1] as u32;
                let depth = u16_at(if wide { 40 } else { 36 });
                let mv = flags & 4 != 0;
                let mx = if flags & 1 != 0 {
                    let o = if wide { 16 } else { 12 };
                    Some(Mx { has_scale: true, has_rotate: true, sx: f32_at(o), r1: f32_at(o + 4), r0: f32_at(o + 8), sy: f32_at(o + 12), tx: jround(f32_at(o + 16) as f64 * 20.0) as i32, ty: jround(f32_at(o + 20) as f64 * 20.0) as i32 })
                } else {
                    None
                };
                let cf = a + if wide { 48 } else { 40 };
                let id = m.dictionary_id(cf, s.reference(cf)?)?;
                let has_char = id >= 0;
                let has_class = flags & 32 != 0;
                let has_image = flags & 8 != 0;
                let class = if has_class || (has_image && has_char) { s.string(s.reference(a + if wide { 80 } else { 56 })?)? } else { None };
                let name = if flags & 64 != 0 { Some(s.string(s.reference(a + if wide { 64 } else { 48 })?)?.unwrap_or_default()) } else { None };
                let cx = if flags & 2 != 0 { Some(cx_at(&s, s.reference(a + if wide { 72 } else { 52 })?, false)?) } else { None };
                let ratio = u16_at(if wide { 56 } else { 44 });
                let clip = flags & 128 != 0;
                let clip_depth = u16_at(if wide { 58 } else { 46 });
                let blend = blend_mode(b[p + 3] as u32)?;
                let cache = flags & 16 != 0;
                let fc = b[p + 2] as u64;
                let has_filters = fc != 127;
                let mut filters = Vec::new();
                if has_filters && fc > 0 {
                    let fa = s.reference(a + if wide { 88 } else { 60 })?;
                    for i in 0..fc {
                        filters.push(filter(&s, fa + if wide { 56 } else { 44 } * i)?);
                    }
                }
                let mut w = Writer::new();
                for v in [false, clip, name.is_some(), true, cx.is_some(), mx.is_some(), has_char, mv, false, false, false, has_image, has_class, cache, blend.is_some(), has_filters] {
                    w.ub(1, v as u32);
                }
                w.u16(depth);
                if has_class {
                    swf_string(&mut w, class.as_deref().unwrap_or(""));
                } else if has_char {
                    w.u16(id as u16);
                }
                if let Some(mx) = mx {
                    mx.write(&mut w);
                }
                if let Some(cx) = cx {
                    cx.write(&mut w);
                }
                w.u16(ratio);
                if let Some(n) = &name {
                    swf_string(&mut w, n);
                }
                if clip {
                    w.u16(clip_depth);
                }
                if has_filters {
                    w.u8(filters.len() as u8);
                    for f in &filters {
                        w.bytes(f);
                    }
                }
                if let Some(bm) = blend {
                    w.u8(bm);
                }
                if cache {
                    w.u8(1);
                }
                out.push(tag(PLACE_OBJECT3, w.data, false));
            }
            3 => {
                out.push(tag(REMOVE_OBJECT2, u16_at(p + 2).to_le_bytes().to_vec(), false));
            }
            4 => {
                let mut w = Writer::new();
                w.u8(0x11);
                w.u8(0);
                w.u16(u16_at(p + 4));
                w.u16(u16_at(p + 6));
                out.push(tag(PLACE_OBJECT3, w.data, false));
            }
            5 => {
                let mut w = Writer::new();
                swf_string(&mut w, &s.string(s.reference(a + if wide { 16 } else { 8 })?)?.unwrap_or_default());
                if b[p + 1] != 0 {
                    w.u8(1);
                }
                out.push(tag(FRAME_LABEL, w.data, false));
            }
            6 => {
                let mut w = Writer::new();
                let class = s.reference(a + if wide { 16 } else { 12 })?;
                let code = if class != 0 {
                    swf_string(&mut w, &s.string(class)?.unwrap_or_default());
                    START_SOUND2
                } else {
                    w.u16(u16_at(if wide { 12 } else { 8 }));
                    START_SOUND
                };
                sound_info(&mut w, &s, b, a)?;
                out.push(tag(code, w.data, false));
            }
            t => return err(format!("Unsupported Iggy timeline command {t}")),
        }
    }
    while actual < frames {
        out.push(tag(SHOW_FRAME, Vec::new(), false));
        actual += 1;
    }
    Ok(out)
}

/// One dictionary asset (shape, morph, bitmap, static text, button, sound or binary data)
/// as its SWF tag.
pub fn asset_tag(file: Option<&IggyFile>, m: &Movie, e: &super::swf::DictEntry) -> Result<Tag> {
    let s = Src { data: m.original_data(), wide: m.is64 };
    let id = e.id as u16;
    match e.ty {
        1 | 2 => vector(m, &s, e.address, id, e.ty),
        4 => bitmap(file, &s, e.address, id),
        5 => static_text(m, &s, e.address, id),
        20 => binary(&s, e.address, id),
        7 => button(&s, e.address, id),
        21 => sound(&s, e.address, id),
        t => err(format!("Unsupported Iggy asset type {t}")),
    }
}

fn write_tags(w: &mut Writer, tags: &[Tag]) {
    for t in tags {
        t.write(w);
    }
}

/// `IggyToSwfConvertor.getSwf` as an uncompressed SWF file.
pub fn swf_bytes(file: &IggyFile) -> Result<Vec<u8>> {
    let m = &file.swf;
    let mut tags = Vec::new();
    let as3 = m.has_as3() || !m.abc_blocks.is_empty();
    tags.push(tag(FILE_ATTRIBUTES, vec![if as3 { 0x08 } else { 0 }, 0, 0, 0], false));
    let c = m.hdr.background_color();
    tags.push(tag(SET_BACKGROUND_COLOR, vec![c as u8, (c >> 8) as u8, (c >> 16) as u8], false));
    if !m.scenes().is_empty() || !m.frame_labels().is_empty() {
        let mut w = Writer::new();
        for list in [m.scenes(), m.frame_labels()] {
            encoded_u32(&mut w, list.len() as u64);
            for f in list {
                encoded_u32(&mut w, f.frame);
                swf_string(&mut w, &f.name);
            }
        }
        tags.push(tag(DEFINE_SCENE_AND_FRAME_LABEL_DATA, w.data, false));
    }
    let mut exported: HashSet<i32> = HashSet::new();
    for l in m.imports() {
        let mut w = Writer::new();
        swf_string(&mut w, &l.library);
        w.u8(1);
        w.u8(0);
        w.u16(l.symbols.len() as u16);
        for sym in &l.symbols {
            w.u16(sym.character_id as u16);
            swf_string(&mut w, &sym.name);
            exported.insert(sym.character_id as i32);
        }
        tags.push(tag(IMPORT_ASSETS2, w.data, false));
    }
    for f in &m.fonts {
        exported.insert(f.font_id as i32);
        tags.push(font_tag(f)?);
    }
    for t in &m.texts {
        exported.insert(t.text_index as i32);
        tags.push(edit_text(t));
    }
    for pass in 0..3 {
        for e in &m.dictionary {
            if !matches!(e.ty, 1 | 2 | 4 | 5 | 7 | 20 | 21) {
                continue;
            }
            let p = if e.ty <= 2 { 1 } else if e.ty == 7 || e.ty == 5 { 2 } else { 0 };
            if p != pass {
                continue;
            }
            tags.push(asset_tag(Some(file), m, e)?);
            exported.insert(e.id);
        }
    }
    for (id, tl) in &m.sprite_timelines {
        let frames = m.timeline_frame_count(*id)?;
        let mut w = Writer::new();
        w.u16(*id as u16);
        w.u16(frames as u16);
        write_tags(&mut w, &timeline_tags(m, tl, frames)?);
        w.u16(0);
        tags.push(tag(DEFINE_SPRITE, w.data, false));
        exported.insert(*id);
    }
    for (id, _) in &m.sprite_timelines {
        if let Some(r) = m.scaling_grid(*id)? {
            let mut w = Writer::new();
            w.u16(*id as u16);
            Rect { x_min: java_round(r[0] * 20.0), x_max: java_round(r[2] * 20.0), y_min: java_round(r[1] * 20.0), y_max: java_round(r[3] * 20.0) }.write(&mut w);
            tags.push(tag(DEFINE_SCALING_GRID, w.data, false));
        }
    }
    for abc in &m.abc_blocks {
        let mut body = vec![1, 0, 0, 0, 0];
        match crate::abc::Abc::parse(&abc.data) {
            Ok(a) => body.extend(a.to_bytes()),
            Err(_) => body.extend(&abc.data),
        }
        tags.push(tag(DO_ABC2, body, false));
    }
    let mut ids: Vec<i32> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for sym in m.symbol_classes() {
        let id = sym.character_id as i32;
        if id == 0 || exported.contains(&id) {
            ids.push(id);
            names.push(sym.name.clone());
        }
    }
    for t in &m.texts {
        if let Some(n) = &t.symbol_class_name {
            if !ids.contains(&(t.text_index as i32)) {
                ids.push(t.text_index as i32);
                names.push(n.clone());
            }
        }
    }
    if !ids.is_empty() {
        let mut w = Writer::new();
        w.u16(ids.len() as u16);
        for (id, n) in ids.iter().zip(&names) {
            w.u16(*id as u16);
            swf_string(&mut w, n);
        }
        tags.push(tag(SYMBOL_CLASS, w.data, false));
    }
    let frames = m.hdr.frame_count();
    tags.extend(timeline_tags(m, &m.timeline, frames)?);
    tags.push(tag(END, Vec::new(), false));
    let (x0, y0, x1, y1) = m.hdr.bounds();
    let rect = Rect { x_min: (x0 as f64 * 20.0) as i32, x_max: (x1 as f64 * 20.0) as i32, y_min: (y0 as f64 * 20.0) as i32, y_max: (y1 as f64 * 20.0) as i32 };
    let mut body = Writer::new();
    rect.write(&mut body);
    ufixed8(&mut body, m.hdr.frame_rate());
    body.u16(frames as u16);
    write_tags(&mut body, &tags);
    let mut out = Vec::with_capacity(body.data.len() + 8);
    out.extend_from_slice(b"FWS");
    out.push(file.swf_version());
    out.extend_from_slice(&((body.data.len() + 8) as u32).to_le_bytes());
    out.extend_from_slice(&body.data);
    Ok(out)
}

fn encoded_u32(w: &mut Writer, v: u64) {
    let mut v = v & 0xffff_ffff;
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            w.u8(b);
            break;
        }
        w.u8(b | 0x80);
    }
}

/// The Iggy movie as a parsed SWF.
pub fn to_swf(file: &IggyFile) -> Result<crate::Swf> {
    crate::Swf::parse(&swf_bytes(file)?)
}
