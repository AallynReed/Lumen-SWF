//! Shape records (SHAPE). Style arrays come in phase 3; glyph shapes need none.

use crate::error::{Error, Result};
use crate::io::Reader;

#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    StyleChange { move_to: Option<(i32, i32)>, fill0: Option<u32>, fill1: Option<u32>, line: Option<u32> },
    Straight { dx: i32, dy: i32 },
    Curved { cdx: i32, cdy: i32, adx: i32, ady: i32 },
}

/// Reads records up to the end record. A style change carrying new style arrays, which only
/// SHAPEWITHSTYLE (never a glyph) contains, is an error for now.
pub fn read_records(r: &mut Reader, fill_bits: u32, line_bits: u32) -> Result<Vec<Record>> {
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
        if flags & 16 != 0 {
            return Err(Error::Other("new styles inside a glyph shape".into()));
        }
        out.push(Record::StyleChange { move_to, fill0, fill1, line });
    }
    r.align();
    Ok(out)
}

/// A glyph SHAPE: 4-bit fill and line bit counts, then records.
pub fn read_glyph(data: &[u8]) -> Result<Vec<Record>> {
    let mut r = Reader::new(data);
    let fill_bits = r.ub(4)?;
    let line_bits = r.ub(4)?;
    read_records(&mut r, fill_bits, line_bits)
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
        // move to (0,0) with fill1 = 1
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
