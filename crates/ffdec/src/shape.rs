//! Shapes: style arrays, shape records, and the glyph contours fonts are built from.

use crate::error::{Error, Result};
use crate::io::{bits_s, Reader, Writer};
use crate::tags::{code::*, Tag};
use crate::types::{Matrix, Rect, Rgba};

#[derive(Debug, Clone, PartialEq)]
pub struct GradRecord {
    pub ratio: u8,
    pub color: Rgba,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub matrix: Matrix,
    /// 0 pad, 1 reflect, 2 repeat.
    pub spread: u8,
    /// 0 sRGB, 1 linear RGB.
    pub interpolation: u8,
    pub records: Vec<GradRecord>,
    pub focal: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Fill {
    Solid(Rgba),
    Linear(Gradient),
    Radial(Gradient),
    Focal(Gradient),
    Bitmap { id: u16, matrix: Matrix, repeat: bool, smooth: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineStyle {
    pub width: u16,
    pub color: Rgba,
    /// 0 round, 1 none, 2 square.
    pub start_cap: u8,
    pub end_cap: u8,
    /// 0 round, 1 bevel, 2 miter.
    pub join: u8,
    pub miter_limit: f64,
    pub no_close: bool,
    pub no_h_scale: bool,
    pub no_v_scale: bool,
    pub pixel_hinting: bool,
    pub fill: Option<Fill>,
}

impl LineStyle {
    pub fn simple(width: u16, color: Rgba) -> LineStyle {
        LineStyle { width, color, start_cap: 0, end_cap: 0, join: 0, miter_limit: 3.0, no_close: false, no_h_scale: false, no_v_scale: false, pixel_hinting: false, fill: None }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Styles {
    pub fills: Vec<Fill>,
    pub lines: Vec<LineStyle>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    StyleChange {
        move_to: Option<(i32, i32)>,
        fill0: Option<u32>,
        fill1: Option<u32>,
        line: Option<u32>,
        new_styles: Option<Box<Styles>>,
    },
    Straight { dx: i32, dy: i32 },
    Curved { cdx: i32, cdy: i32, adx: i32, ady: i32 },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Shape {
    pub styles: Styles,
    pub records: Vec<Record>,
}

fn color(r: &mut Reader, alpha: bool) -> Result<Rgba> {
    if alpha { Rgba::read_rgba(r) } else { Rgba::read_rgb(r) }
}

fn gradient(r: &mut Reader, matrix: Matrix, shape_num: u8, focal: bool) -> Result<Gradient> {
    let spread = r.ub(2)? as u8;
    let interpolation = r.ub(2)? as u8;
    let n = r.ub(4)?;
    let mut records = Vec::with_capacity(n as usize);
    for _ in 0..n {
        records.push(GradRecord { ratio: r.u8()?, color: color(r, shape_num >= 3)? });
    }
    let focal = if focal { r.fixed8()? as f64 } else { 0.0 };
    Ok(Gradient { matrix, spread, interpolation, records, focal })
}

pub fn read_fill(r: &mut Reader, shape_num: u8) -> Result<Fill> {
    let kind = r.u8()?;
    Ok(match kind {
        0x00 => Fill::Solid(color(r, shape_num >= 3)?),
        0x10 | 0x12 | 0x13 => {
            let m = Matrix::read(r)?;
            let g = gradient(r, m, shape_num, kind == 0x13)?;
            match kind {
                0x10 => Fill::Linear(g),
                0x12 => Fill::Radial(g),
                _ => Fill::Focal(g),
            }
        }
        0x40..=0x43 => {
            let id = r.u16()?;
            let matrix = Matrix::read(r)?;
            Fill::Bitmap { id, matrix, repeat: kind == 0x40 || kind == 0x42, smooth: kind <= 0x41 }
        }
        k => return Err(Error::Other(format!("unknown fill style type 0x{k:02x}"))),
    })
}

fn read_line(r: &mut Reader, shape_num: u8) -> Result<LineStyle> {
    let width = r.u16()?;
    if shape_num < 4 {
        return Ok(LineStyle::simple(width, color(r, shape_num >= 3)?));
    }
    let start_cap = r.ub(2)? as u8;
    let join = r.ub(2)? as u8;
    let has_fill = r.bit()?;
    let no_h_scale = r.bit()?;
    let no_v_scale = r.bit()?;
    let pixel_hinting = r.bit()?;
    r.ub(5)?;
    let no_close = r.bit()?;
    let end_cap = r.ub(2)? as u8;
    let miter_limit = if join == 2 { r.u16()? as f64 / 256.0 } else { 3.0 };
    let (color, fill) = if has_fill {
        let f = read_fill(r, shape_num)?;
        (if let Fill::Solid(c) = &f { *c } else { Rgba::BLACK }, Some(f))
    } else {
        (Rgba::read_rgba(r)?, None)
    };
    Ok(LineStyle { width, color, start_cap, end_cap, join, miter_limit, no_close, no_h_scale, no_v_scale, pixel_hinting, fill })
}

pub fn read_styles(r: &mut Reader, shape_num: u8) -> Result<Styles> {
    let mut n = r.u8()? as usize;
    if shape_num > 1 && n == 0xff {
        n = r.u16()? as usize;
    }
    let mut fills = Vec::with_capacity(n);
    for _ in 0..n {
        fills.push(read_fill(r, shape_num)?);
    }
    let mut n = r.u8()? as usize;
    if n == 0xff {
        n = r.u16()? as usize;
    }
    let mut lines = Vec::with_capacity(n);
    for _ in 0..n {
        lines.push(read_line(r, shape_num)?);
    }
    Ok(Styles { fills, lines })
}

/// Reads records up to the end record. `shape_num` 0 means a glyph or morph edge list,
/// which may not carry new style arrays.
pub fn read_records(r: &mut Reader, shape_num: u8, mut fill_bits: u32, mut line_bits: u32) -> Result<Vec<Record>> {
    let mut out = Vec::new();
    loop {
        if r.bit()? {
            if r.bit()? {
                let n = r.ub(4)? + 2;
                if r.bit()? {
                    out.push(Record::Straight { dx: r.sb(n)?, dy: r.sb(n)? });
                } else if r.bit()? {
                    out.push(Record::Straight { dx: 0, dy: r.sb(n)? });
                } else {
                    out.push(Record::Straight { dx: r.sb(n)?, dy: 0 });
                }
            } else {
                let n = r.ub(4)? + 2;
                out.push(Record::Curved { cdx: r.sb(n)?, cdy: r.sb(n)?, adx: r.sb(n)?, ady: r.sb(n)? });
            }
            continue;
        }
        let flags = r.ub(5)?;
        if flags == 0 {
            break;
        }
        let move_to = if flags & 1 != 0 {
            let n = r.ub(5)?;
            Some((r.sb(n)?, r.sb(n)?))
        } else {
            None
        };
        let fill0 = if flags & 2 != 0 { Some(r.ub(fill_bits)?) } else { None };
        let fill1 = if flags & 4 != 0 { Some(r.ub(fill_bits)?) } else { None };
        let line = if flags & 8 != 0 { Some(r.ub(line_bits)?) } else { None };
        let new_styles = if flags & 16 != 0 {
            if shape_num == 0 {
                return Err(Error::Other("new styles inside a glyph or morph shape".into()));
            }
            let s = read_styles(r, shape_num)?;
            fill_bits = r.ub(4)?;
            line_bits = r.ub(4)?;
            Some(Box::new(s))
        } else {
            None
        };
        out.push(Record::StyleChange { move_to, fill0, fill1, line, new_styles });
    }
    r.align();
    Ok(out)
}

/// A glyph SHAPE: 4-bit fill and line bit counts, then records.
pub fn read_glyph(data: &[u8]) -> Result<Vec<Record>> {
    let mut r = Reader::new(data);
    let fill_bits = r.ub(4)?;
    let line_bits = r.ub(4)?;
    read_records(&mut r, 0, fill_bits, line_bits)
}

pub fn shape_num(code: u16) -> Option<u8> {
    Some(match code {
        DEFINE_SHAPE => 1,
        DEFINE_SHAPE2 => 2,
        DEFINE_SHAPE3 => 3,
        DEFINE_SHAPE4 => 4,
        _ => return None,
    })
}

pub fn is_shape(code: u16) -> bool {
    shape_num(code).is_some()
}

/// A parsed DefineShape/2/3/4.
pub struct ShapeTag {
    pub id: u16,
    pub bounds: Rect,
    pub shape_num: u8,
    pub shape: Shape,
    /// DefineShape4's fill-winding flag: non-zero instead of even-odd.
    pub nonzero: bool,
    /// DefineShape4's edge bounds and flags byte.
    pub edge_bounds: Rect,
    pub flags: u8,
}

impl ShapeTag {
    pub fn read(tag: &Tag) -> Result<ShapeTag> {
        let shape_num = shape_num(tag.code).ok_or_else(|| Error::Other(format!("{} is not a shape", tag.name())))?;
        let mut r = Reader::new(&tag.body);
        let id = r.u16()?;
        let bounds = Rect::read(&mut r)?;
        let (mut edge_bounds, mut flags) = (Rect::default(), 0);
        if shape_num == 4 {
            edge_bounds = Rect::read(&mut r)?;
            flags = r.u8()?;
        }
        let nonzero = flags & 0x04 != 0;
        let styles = read_styles(&mut r, shape_num)?;
        let fill_bits = r.ub(4)?;
        let line_bits = r.ub(4)?;
        let records = read_records(&mut r, shape_num, fill_bits, line_bits)?;
        Ok(ShapeTag { id, bounds, shape_num, shape: Shape { styles, records }, nonzero, edge_bounds, flags })
    }

    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u16(self.id);
        self.bounds.write(&mut w);
        if self.shape_num == 4 {
            self.edge_bounds.write(&mut w);
            w.u8(self.flags);
        }
        self.shape.write(&mut w, self.shape_num);
        w.data
    }
}

fn bits_u(v: u32) -> u32 {
    32 - v.leading_zeros()
}

fn write_color(w: &mut Writer, c: Rgba, alpha: bool) {
    w.bytes(&[c.r, c.g, c.b]);
    if alpha {
        w.u8(c.a);
    }
}

fn write_gradient(w: &mut Writer, g: &Gradient, shape_num: u8, focal: bool) {
    w.ub(2, g.spread as u32);
    w.ub(2, g.interpolation as u32);
    w.ub(4, g.records.len() as u32);
    for r in &g.records {
        w.u8(r.ratio);
        write_color(w, r.color, shape_num >= 3);
    }
    if focal {
        w.i16((g.focal as f32 * 256.0) as i32 as i16);
    }
}

pub fn write_fill(w: &mut Writer, f: &Fill, shape_num: u8) {
    match f {
        Fill::Solid(c) => {
            w.u8(0x00);
            write_color(w, *c, shape_num >= 3);
        }
        Fill::Linear(g) | Fill::Radial(g) | Fill::Focal(g) => {
            let kind = match f {
                Fill::Linear(_) => 0x10,
                Fill::Radial(_) => 0x12,
                _ => 0x13,
            };
            w.u8(kind);
            g.matrix.write(w);
            write_gradient(w, g, shape_num, kind == 0x13);
        }
        Fill::Bitmap { id, matrix, repeat, smooth } => {
            w.u8(match (repeat, smooth) {
                (true, true) => 0x40,
                (false, true) => 0x41,
                (true, false) => 0x42,
                (false, false) => 0x43,
            });
            w.u16(*id);
            matrix.write(w);
        }
    }
}

fn write_count(w: &mut Writer, n: usize, extended: bool) {
    if extended && n >= 0xff {
        w.u8(0xff);
        w.u16(n as u16);
    } else {
        w.u8(n as u8);
    }
}

fn write_line(w: &mut Writer, l: &LineStyle, shape_num: u8) {
    w.u16(l.width);
    if shape_num < 4 {
        write_color(w, l.color, shape_num >= 3);
        return;
    }
    w.ub(2, l.start_cap as u32);
    w.ub(2, l.join as u32);
    w.ub(1, l.fill.is_some() as u32);
    w.ub(1, l.no_h_scale as u32);
    w.ub(1, l.no_v_scale as u32);
    w.ub(1, l.pixel_hinting as u32);
    w.ub(5, 0);
    w.ub(1, l.no_close as u32);
    w.ub(2, l.end_cap as u32);
    if l.join == 2 {
        w.u16((l.miter_limit as f32 * 256.0) as i32 as u16);
    }
    match &l.fill {
        Some(f) => write_fill(w, f, shape_num),
        None => write_color(w, l.color, true),
    }
}

pub fn write_styles(w: &mut Writer, s: &Styles, shape_num: u8) {
    write_count(w, s.fills.len(), shape_num > 1);
    for f in &s.fills {
        write_fill(w, f, shape_num);
    }
    write_count(w, s.lines.len(), true);
    for l in &s.lines {
        write_line(w, l, shape_num);
    }
}

/// Style index bits for the records up to the first new style arrays, as FFDec counts them.
pub fn index_bits(records: &[Record]) -> (u32, u32) {
    let (mut fb, mut lb) = (0, 0);
    for r in records {
        if let Record::StyleChange { fill0, fill1, line, new_styles, .. } = r {
            for f in [fill0, fill1].into_iter().flatten() {
                fb = fb.max(bits_u(*f));
            }
            if let Some(l) = line {
                lb = lb.max(bits_u(*l));
            }
            if new_styles.is_some() {
                break;
            }
        }
    }
    (fb, lb)
}

/// Writes records and the end record, like FFDec's writeSHAPERECORDS.
pub fn write_records(w: &mut Writer, records: &[Record], mut fill_bits: u32, mut line_bits: u32, shape_num: u8) {
    for r in records {
        match *r {
            Record::Curved { cdx, cdy, adx, ady } => {
                let n = bits_s(&[cdx, cdy, adx, ady]).saturating_sub(2);
                w.ub(2, 0b10);
                w.ub(4, n);
                for v in [cdx, cdy, adx, ady] {
                    w.sb(n + 2, v);
                }
            }
            Record::Straight { dx, dy } => {
                let n = bits_s(&[dx, dy]).saturating_sub(2);
                w.ub(2, 0b11);
                w.ub(4, n);
                let general = dx != 0 && dy != 0;
                w.ub(1, general as u32);
                if !general {
                    w.ub(1, (dx == 0) as u32);
                }
                if general || dx != 0 {
                    w.sb(n + 2, dx);
                }
                if general || dx == 0 {
                    w.sb(n + 2, dy);
                }
            }
            Record::StyleChange { move_to, fill0, fill1, line, ref new_styles } => {
                w.ub(1, 0);
                w.ub(1, new_styles.is_some() as u32);
                w.ub(1, line.is_some() as u32);
                w.ub(1, fill1.is_some() as u32);
                w.ub(1, fill0.is_some() as u32);
                w.ub(1, move_to.is_some() as u32);
                if let Some((x, y)) = move_to {
                    let n = bits_s(&[x, y]);
                    w.ub(5, n);
                    w.sb(n, x);
                    w.sb(n, y);
                }
                if let Some(f) = fill0 {
                    w.ub(fill_bits, f);
                }
                if let Some(f) = fill1 {
                    w.ub(fill_bits, f);
                }
                if let Some(l) = line {
                    w.ub(line_bits, l);
                }
                if let Some(s) = new_styles {
                    write_styles(w, s, shape_num);
                    fill_bits = bits_u(s.fills.len() as u32);
                    line_bits = bits_u(s.lines.len() as u32);
                    w.ub(4, fill_bits);
                    w.ub(4, line_bits);
                }
            }
        }
    }
    w.ub(6, 0);
    w.flush_bits();
}

impl Shape {
    /// SHAPEWITHSTYLE: styles, index bit counts, records.
    pub fn write(&self, w: &mut Writer, shape_num: u8) {
        write_styles(w, &self.styles, shape_num);
        let (fb, lb) = index_bits(&self.records);
        w.ub(4, fb);
        w.ub(4, lb);
        write_records(w, &self.records, fb, lb, shape_num);
    }
}

/// Point on a contour; `on == false` marks a quadratic control point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
    pub on: bool,
}

/// Splits records into contours of absolute coordinates.
pub fn contours(records: &[Record]) -> Vec<Vec<Point>> {
    let mut out: Vec<Vec<Point>> = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    let (mut x, mut y) = (0, 0);
    for rec in records {
        match *rec {
            Record::StyleChange { move_to, .. } => {
                if let Some((mx, my)) = move_to {
                    if cur.len() > 1 {
                        out.push(std::mem::take(&mut cur));
                    }
                    cur.clear();
                    x = mx;
                    y = my;
                    cur.push(Point { x, y, on: true });
                }
            }
            Record::Straight { dx, dy } => {
                if cur.is_empty() {
                    cur.push(Point { x, y, on: true });
                }
                x += dx;
                y += dy;
                cur.push(Point { x, y, on: true });
            }
            Record::Curved { cdx, cdy, adx, ady } => {
                if cur.is_empty() {
                    cur.push(Point { x, y, on: true });
                }
                cur.push(Point { x: x + cdx, y: y + cdy, on: false });
                x += cdx + adx;
                y += cdy + ady;
                cur.push(Point { x, y, on: true });
            }
        }
    }
    if cur.len() > 1 {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::Writer;

    #[test]
    fn square_glyph() {
        let mut w = Writer::new();
        w.ub(4, 1);
        w.ub(4, 0);
        w.ub(1, 0);
        w.ub(5, 0b00101);
        w.ub(5, 1);
        w.sb(1, 0);
        w.sb(1, 0);
        w.ub(1, 1);
        for (dx, dy) in [(100, 0), (0, 100), (-100, 0), (0, -100)] {
            w.ub(1, 1);
            w.ub(1, 1);
            w.ub(4, 8 - 2);
            w.ub(1, 0);
            if dx != 0 {
                w.ub(1, 0);
                w.sb(8, dx);
            } else {
                w.ub(1, 1);
                w.sb(8, dy);
            }
        }
        w.ub(1, 0);
        w.ub(5, 0);
        let recs = read_glyph(&w.data).unwrap();
        assert_eq!(recs.len(), 5);
        let c = contours(&recs);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].len(), 5);
        assert_eq!((c[0][2].x, c[0][2].y), (100, 100));
    }
}
