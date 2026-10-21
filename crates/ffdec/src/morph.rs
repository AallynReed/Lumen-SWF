//! DefineMorphShape/2: start and end styles and edges, interpolated by ratio like FFDec's
//! `MorphShapeTag.getShapeAtRatio`.

use crate::error::{Error, Result};
use crate::io::Reader;
use crate::shape::{self, Fill, GradRecord, Gradient, LineStyle, Record, Shape, Styles};
use crate::tags::{code::*, Tag};
use crate::types::{Matrix, Rect, Rgba};

pub const MAX_RATIO: f64 = 65535.0;

#[derive(Debug, Clone)]
pub struct MorphGradient {
    pub spread: u8,
    pub interpolation: u8,
    pub records: Vec<(u8, Rgba, u8, Rgba)>,
    pub focal: (f64, f64),
}

#[derive(Debug, Clone)]
pub enum MorphFill {
    Solid(Rgba, Rgba),
    Gradient { kind: u8, start: Matrix, end: Matrix, gradient: MorphGradient },
    Bitmap { id: u16, start: Matrix, end: Matrix, repeat: bool, smooth: bool },
}

#[derive(Debug, Clone)]
pub struct MorphLine {
    pub widths: (u16, u16),
    pub colors: (Rgba, Rgba),
    pub start_cap: u8,
    pub end_cap: u8,
    pub join: u8,
    pub miter_limit: f64,
    pub no_close: bool,
    pub no_h_scale: bool,
    pub no_v_scale: bool,
    pub pixel_hinting: bool,
    pub fill: Option<MorphFill>,
}

pub struct MorphShape {
    pub id: u16,
    pub start_bounds: Rect,
    pub end_bounds: Rect,
    pub version: u8,
    pub fills: Vec<MorphFill>,
    pub lines: Vec<MorphLine>,
    pub start: Vec<Record>,
    pub end: Vec<Record>,
}

pub fn is_morph(code: u16) -> bool {
    code == DEFINE_MORPH_SHAPE || code == DEFINE_MORPH_SHAPE2
}

fn read_fill(r: &mut Reader) -> Result<MorphFill> {
    let kind = r.u8()?;
    Ok(match kind {
        0x00 => MorphFill::Solid(Rgba::read_rgba(r)?, Rgba::read_rgba(r)?),
        0x10 | 0x12 | 0x13 => {
            let start = Matrix::read(r)?;
            let end = Matrix::read(r)?;
            let spread = r.ub(2)? as u8;
            let interpolation = r.ub(2)? as u8;
            let n = r.ub(4)?;
            let mut records = Vec::with_capacity(n as usize);
            for _ in 0..n {
                records.push((r.u8()?, Rgba::read_rgba(r)?, r.u8()?, Rgba::read_rgba(r)?));
            }
            let focal = if kind == 0x13 { (r.fixed8()? as f64, r.fixed8()? as f64) } else { (0.0, 0.0) };
            MorphFill::Gradient { kind, start, end, gradient: MorphGradient { spread, interpolation, records, focal } }
        }
        0x40..=0x43 => MorphFill::Bitmap {
            id: r.u16()?,
            start: Matrix::read(r)?,
            end: Matrix::read(r)?,
            repeat: kind == 0x40 || kind == 0x42,
            smooth: kind <= 0x41,
        },
        k => return Err(Error::Other(format!("unknown morph fill style type 0x{k:02x}"))),
    })
}

fn read_line(r: &mut Reader, version: u8) -> Result<MorphLine> {
    let widths = (r.u16()?, r.u16()?);
    if version == 1 {
        return Ok(MorphLine {
            widths,
            colors: (Rgba::read_rgba(r)?, Rgba::read_rgba(r)?),
            start_cap: 0,
            end_cap: 0,
            join: 0,
            miter_limit: 3.0,
            no_close: false,
            no_h_scale: false,
            no_v_scale: false,
            pixel_hinting: false,
            fill: None,
        });
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
    let (colors, fill) = if has_fill {
        let f = read_fill(r)?;
        let c = if let MorphFill::Solid(a, b) = &f { (*a, *b) } else { (Rgba::BLACK, Rgba::BLACK) };
        (c, Some(f))
    } else {
        ((Rgba::read_rgba(r)?, Rgba::read_rgba(r)?), None)
    };
    Ok(MorphLine { widths, colors, start_cap, end_cap, join, miter_limit, no_close, no_h_scale, no_v_scale, pixel_hinting, fill })
}

fn fill_at(f: &MorphFill, t: f64) -> Fill {
    match f {
        MorphFill::Solid(a, b) => Fill::Solid(a.lerp(b, t)),
        MorphFill::Gradient { kind, start, end, gradient: g } => {
            let records = g
                .records
                .iter()
                .map(|(r0, c0, r1, c1)| GradRecord { ratio: (*r0 as f64 + (*r1 as f64 - *r0 as f64) * t).round() as u8, color: c0.lerp(c1, t) })
                .collect();
            let gr = Gradient {
                matrix: start.lerp(end, t),
                spread: g.spread,
                interpolation: g.interpolation,
                records,
                focal: g.focal.0 + (g.focal.1 - g.focal.0) * t,
            };
            match kind {
                0x10 => Fill::Linear(gr),
                0x12 => Fill::Radial(gr),
                _ => Fill::Focal(gr),
            }
        }
        MorphFill::Bitmap { id, start, end, repeat, smooth } => Fill::Bitmap { id: *id, matrix: start.lerp(end, t), repeat: *repeat, smooth: *smooth },
    }
}

fn lerp_i(a: i32, b: i32, ratio: f64) -> i32 {
    a + ((b - a) as f64 * ratio / MAX_RATIO).round() as i32
}

fn straight_to_curve(dx: i32, dy: i32) -> (i32, i32, i32, i32) {
    let cdx = dx / 2;
    let cdy = dy / 2;
    (cdx, cdy, dx - cdx, dy - cdy)
}

impl MorphShape {
    pub fn read(tag: &Tag) -> Result<MorphShape> {
        if !is_morph(tag.code) {
            return Err(Error::Other(format!("{} is not a morph shape", tag.name())));
        }
        let version = if tag.code == DEFINE_MORPH_SHAPE2 { 2 } else { 1 };
        let mut r = Reader::new(&tag.body);
        let id = r.u16()?;
        let start_bounds = Rect::read(&mut r)?;
        let end_bounds = Rect::read(&mut r)?;
        if version == 2 {
            Rect::read(&mut r)?;
            Rect::read(&mut r)?;
            r.u8()?;
        }
        let offset = r.u32()? as usize;
        let end_start = r.pos() + offset;
        let mut n = r.u8()? as usize;
        if n == 0xff {
            n = r.u16()? as usize;
        }
        let mut fills = Vec::with_capacity(n);
        for _ in 0..n {
            fills.push(read_fill(&mut r)?);
        }
        let mut n = r.u8()? as usize;
        if n == 0xff {
            n = r.u16()? as usize;
        }
        let mut lines = Vec::with_capacity(n);
        for _ in 0..n {
            lines.push(read_line(&mut r, version)?);
        }
        let fb = r.ub(4)?;
        let lb = r.ub(4)?;
        let start = shape::read_records(&mut r, 0, fb, lb)?;
        let end = match tag.body.get(end_start..) {
            Some(d) if !d.is_empty() => {
                let mut er = Reader::new(d);
                let fb = er.ub(4)?;
                let lb = er.ub(4)?;
                shape::read_records(&mut er, 0, fb, lb)?
            }
            _ => start.clone(),
        };
        Ok(MorphShape { id, start_bounds, end_bounds, version, fills, lines, start, end })
    }

    pub fn bounds_at(&self, ratio: u16) -> Rect {
        let t = ratio as f64;
        let s = &self.start_bounds;
        let e = &self.end_bounds;
        Rect {
            x_min: lerp_i(s.x_min, e.x_min, t),
            x_max: lerp_i(s.x_max, e.x_max, t),
            y_min: lerp_i(s.y_min, e.y_min, t),
            y_max: lerp_i(s.y_max, e.y_max, t),
        }
    }

    /// The start edges with the start styles, as `getStartEdges` with `getStartFillStyles`.
    pub fn start_shape(&self) -> Shape {
        let mut s = self.shape_at(0);
        s.records = self.start.clone();
        s
    }

    /// The shape at `ratio` (0 = start, 65535 = end). Straight edges paired with curves
    /// are converted to curves first.
    pub fn shape_at(&self, ratio: u16) -> Shape {
        let rf = ratio as f64;
        let t = rf / MAX_RATIO;
        let styles = Styles {
            fills: self.fills.iter().map(|f| fill_at(f, t)).collect(),
            lines: self
                .lines
                .iter()
                .map(|l| LineStyle {
                    width: lerp_i(l.widths.0 as i32, l.widths.1 as i32, rf).max(0) as u16,
                    color: l.colors.0.lerp(&l.colors.1, t),
                    start_cap: l.start_cap,
                    end_cap: l.end_cap,
                    join: l.join,
                    miter_limit: l.miter_limit,
                    no_close: l.no_close,
                    no_h_scale: l.no_h_scale,
                    no_v_scale: l.no_v_scale,
                    pixel_hinting: false,
                    fill: l.fill.as_ref().map(|f| fill_at(f, t)),
                })
                .collect(),
        };
        let mut out = Vec::with_capacity(self.start.len());
        let (mut i, mut j) = (0, 0);
        let (mut sx, mut sy, mut ex, mut ey) = (0, 0, 0, 0);
        while i < self.start.len() && j < self.end.len() {
            let a = &self.start[i];
            let b = &self.end[j];
            match (a, b) {
                (Record::StyleChange { .. }, _) | (_, Record::StyleChange { .. }) => {
                    let mut rec = match a {
                        Record::StyleChange { .. } => {
                            i += 1;
                            a.clone()
                        }
                        _ => Record::StyleChange { move_to: None, fill0: None, fill1: None, line: None, new_styles: None },
                    };
                    if let Record::StyleChange { move_to: Some((x, y)), .. } = a {
                        sx = *x;
                        sy = *y;
                    }
                    let mut end_moved = false;
                    if let Record::StyleChange { move_to, .. } = b {
                        j += 1;
                        if let Some((x, y)) = move_to {
                            ex = *x;
                            ey = *y;
                            end_moved = true;
                        }
                    }
                    if let Record::StyleChange { move_to, .. } = &mut rec {
                        if move_to.is_some() || end_moved {
                            *move_to = Some((lerp_i(sx, ex, rf), lerp_i(sy, ey, rf)));
                        }
                    }
                    out.push(rec);
                }
                _ => {
                    i += 1;
                    j += 1;
                    let curve = |r: &Record| match *r {
                        Record::Curved { cdx, cdy, adx, ady } => (cdx, cdy, adx, ady),
                        Record::Straight { dx, dy } => straight_to_curve(dx, dy),
                        _ => unreachable!(),
                    };
                    let advance = |r: &Record, x: &mut i32, y: &mut i32| match *r {
                        Record::Curved { cdx, cdy, adx, ady } => {
                            *x += cdx + adx;
                            *y += cdy + ady;
                        }
                        Record::Straight { dx, dy } => {
                            *x += dx;
                            *y += dy;
                        }
                        _ => {}
                    };
                    advance(a, &mut sx, &mut sy);
                    advance(b, &mut ex, &mut ey);
                    if let (Record::Straight { dx: dx0, dy: dy0 }, Record::Straight { dx: dx1, dy: dy1 }) = (a, b) {
                        out.push(Record::Straight { dx: lerp_i(*dx0, *dx1, rf), dy: lerp_i(*dy0, *dy1, rf) });
                    } else {
                        let (c0, c1, c2, c3) = curve(a);
                        let (d0, d1, d2, d3) = curve(b);
                        out.push(Record::Curved {
                            cdx: lerp_i(c0, d0, rf),
                            cdy: lerp_i(c1, d1, rf),
                            adx: lerp_i(c2, d2, rf),
                            ady: lerp_i(c3, d3, rf),
                        });
                    }
                }
            }
        }
        Shape { styles, records: out }
    }
}
