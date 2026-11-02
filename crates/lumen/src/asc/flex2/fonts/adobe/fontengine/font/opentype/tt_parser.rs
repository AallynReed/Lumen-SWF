//! Port of afe.jar `com.adobe.fontengine.font.opentype.TTParser`: TrueType outlines to an `OutlineConsumer`.

use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::glyf::Glyf;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::hmtx::Hmtx;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::open_type_font::OpenTypeFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{ExceptionKind, FontEngineException, FontResult};

/// `Math.round(double)` (ties toward positive infinity) as a double.
fn jround(d: f64) -> f64 {
    if d.is_nan() {
        return 0.0;
    }
    let f = d.floor();
    let r = if d - f >= 0.5 { f + 1.0 } else { f };
    r.clamp(i64::MIN as f64, i64::MAX as f64)
}

/// `(int)Math.round(double)`.
fn jround_int(d: f64) -> i32 {
    jround(d) as i64 as i32
}

#[derive(Clone, Copy, Debug, Default)]
struct Coord {
    x: i32,
    y: i32,
    flags: i32,
}

impl Coord {
    fn set_xy(&mut self, c: Coord) {
        self.x = c.x;
        self.y = c.y;
    }

    fn set_xy_at_mid_point(&mut self, c: Coord, c2: Coord) {
        self.x = jround_int((c.x.wrapping_add(c2.x)) as f64 / 2.0);
        self.y = jround_int((c.y.wrapping_add(c2.y)) as f64 / 2.0);
    }
}

fn index_error(i: usize, len: usize) -> FontEngineException {
    FontEngineException::new(ExceptionKind::FontEngine, format!("java.lang.IndexOutOfBoundsException: Index {} out of bounds for length {}", i, len))
}

#[derive(Clone, Debug, Default)]
pub struct TTParser {
    points: Vec<Coord>,
    end_points: Vec<i32>,
}

impl TTParser {
    pub const EXACT_PATH: i32 = 1;
    pub const APPROX_PATH: i32 = 2;

    pub fn new() -> TTParser {
        TTParser::default()
    }

    fn get_simple_glyph(&mut self, glyf: &Glyf, mut n: i32, n2: i32) -> FontResult<()> {
        if n2 == 0 {
            return Ok(());
        }
        let d = &glyf.glyf.data;
        let n6 = self.points.len() as i32;
        let mut n7 = 0;
        for _ in 0..n2 {
            let n4 = d.getuint16(n)?;
            n += 2;
            if n4 < n7 {
                return Err(FontEngineException::invalid_glyph("endpoints in a simple TT glyph must be increasing"));
            }
            n7 = n4;
            self.end_points.push(n4 + n6);
        }
        let n5 = self.end_points[self.end_points.len() - 1] + 1 - n6;
        n += d.getuint16(n)? + 2;
        let mut n4 = 0;
        while n4 < n5 {
            let flags = d.getuint8(n)?;
            n += 1;
            self.points.push(Coord { x: 0, y: 0, flags });
            if flags & 8 != 0 {
                let mut n9 = d.getuint8(n)?;
                n += 1;
                while n9 > 0 {
                    n9 -= 1;
                    self.points.push(Coord { x: 0, y: 0, flags });
                    n4 += 1;
                }
            }
            n4 += 1;
        }
        let mut v = 0i32;
        for n3 in 0..n5 {
            let idx = (n3 + n6) as usize;
            let len = self.points.len();
            let c = self.points.get_mut(idx).ok_or_else(|| index_error(idx, len))?;
            if c.flags & 2 != 0 {
                let b = d.getuint8(n)?;
                n += 1;
                v = if c.flags & 0x10 == 0 { v.wrapping_sub(b) as i16 as i32 } else { v.wrapping_add(b) as i16 as i32 };
            } else if c.flags & 0x10 == 0 {
                v = v.wrapping_add(d.getint16(n)?) as i16 as i32;
                n += 2;
            }
            c.x = v;
        }
        let mut v = 0i32;
        for i in 0..n5 {
            let idx = (i + n6) as usize;
            let len = self.points.len();
            let c = self.points.get_mut(idx).ok_or_else(|| index_error(idx, len))?;
            if c.flags & 4 != 0 {
                let b = d.getuint8(n)?;
                n += 1;
                v = if c.flags & 0x20 == 0 { v.wrapping_sub(b) as i16 as i32 } else { v.wrapping_add(b) as i16 as i32 };
            } else if c.flags & 0x20 == 0 {
                v = v.wrapping_add(d.getint16(n)?) as i16 as i32;
                n += 2;
            }
            c.y = v;
        }
        Ok(())
    }

    fn get_composite_glyph(&mut self, glyf: &Glyf, mut n: i32, n2: i32) -> FontResult<i32> {
        let d = &glyf.glyf.data;
        let mut n3 = n2;
        loop {
            let mut matrix: Option<Matrix> = None;
            let n6 = d.getuint16(n)?;
            n += 2;
            let n7 = d.getuint16(n)?;
            n += 2;
            let bl = n6 & 2 != 0;
            let (mut n5, mut n4);
            if n6 & 1 != 0 {
                if bl {
                    n5 = d.getint16(n)?;
                    n4 = d.getint16(n + 2)?;
                } else {
                    n5 = d.getuint16(n)?;
                    n4 = d.getuint16(n + 2)?;
                }
                n += 4;
            } else {
                if bl {
                    n5 = d.getint8(n)?;
                    n4 = d.getint8(n + 1)?;
                } else {
                    n5 = d.getuint8(n)?;
                    n4 = d.getuint8(n + 1)?;
                }
                n += 2;
            }
            if n6 & 8 != 0 {
                let d2 = d.getint16(n)? as f64 / 16384.0;
                n += 2;
                matrix = Some(Matrix::new(d2, 0.0, 0.0, d2, 0.0, 0.0));
            } else if n6 & 0x80 != 0 {
                let d2 = d.getint16(n)? as f64 / 16384.0;
                n += 2;
                let d3 = d.getint16(n)? as f64 / 16384.0;
                n += 2;
                let d4 = d.getint16(n)? as f64 / 16384.0;
                n += 2;
                let d5 = d.getint16(n)? as f64 / 16384.0;
                n += 2;
                matrix = Some(Matrix::new(d2, d3, d4, d5, 0.0, 0.0));
            } else if n6 & 0x40 != 0 {
                let d2 = d.getint16(n)? as f64 / 16384.0;
                let d5 = d.getint16(n + 2)? as f64 / 16384.0;
                matrix = Some(Matrix::new(d2, 0.0, 0.0, d5, 0.0, 0.0));
                n += 4;
            }
            if n6 & 0x200 != 0 {
                n3 = n7;
            }
            let n8 = self.points.len();
            let n9 = n;
            let mut loc = glyf.get_glyph_location(n7)?;
            if glyf.get_glyph_location(n7 + 1)? != loc {
                let n11 = d.getint16(loc)?;
                loc += 10;
                if n11 < 0 {
                    n3 = self.get_composite_glyph(glyf, loc, n7)?;
                } else {
                    self.get_simple_glyph(glyf, loc, n11)?;
                }
                if !bl {
                    let len = self.points.len();
                    let c = *self.points.get(n5 as usize).ok_or_else(|| index_error(n5 as usize, len))?;
                    let i2 = (n4 as i64 + n8 as i64) as usize;
                    let c2 = *self.points.get(i2).ok_or_else(|| index_error(i2, len))?;
                    n5 = c.x.wrapping_sub(c2.x);
                    n4 = c.y.wrapping_sub(c2.y);
                }
                if let Some(m) = matrix {
                    for c in &mut self.points[n8..] {
                        let n12 = c.x;
                        c.x = jround_int(m.apply_to_xy_get_x(c.x as f64, c.y as f64) + n5 as f64);
                        c.y = jround_int(m.apply_to_xy_get_y(n12 as f64, c.y as f64) + n4 as f64);
                    }
                } else if n5 != 0 || n4 != 0 {
                    for c in &mut self.points[n8..] {
                        c.x = c.x.wrapping_add(n5);
                        c.y = c.y.wrapping_add(n4);
                    }
                }
            }
            if n6 & 0x20 == 0 {
                return Ok(n3);
            }
            n = n9;
        }
    }

    fn adjust_points_by_side_bearing(&mut self, n: i32, hmtx: Option<&Hmtx>, n2: i32) -> FontResult<()> {
        let n3 = hmtx.ok_or_else(npe)?.get_left_side_bearing(n)?;
        let n4 = n3.wrapping_sub(n2);
        if n4 != 0 {
            for c in &mut self.points {
                c.x = c.x.wrapping_add(n4);
            }
        }
        Ok(())
    }

    fn add_curve(a: &[Coord; 6], c: &mut dyn OutlineConsumer) {
        c.curveto(
            jround(a[0].x.wrapping_add(a[1].x.wrapping_mul(2)) as f64 / 3.0),
            jround(a[0].y.wrapping_add(a[1].y.wrapping_mul(2)) as f64 / 3.0),
            jround(a[1].x.wrapping_mul(2).wrapping_add(a[2].x) as f64 / 3.0),
            jround(a[1].y.wrapping_mul(2).wrapping_add(a[2].y) as f64 / 3.0),
            a[2].x as f64,
            a[2].y as f64,
        );
    }

    fn combine_pair(a: &mut [Coord; 6], c: &mut dyn OutlineConsumer) -> bool {
        let sub = |p: i32, q: i32| p.wrapping_sub(q);
        let sq = |dx: i32, dy: i32| dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)) as f64;
        let d5 = sub(a[3].y, a[1].y) as f64;
        let d6 = sub(a[1].x, a[3].x) as f64;
        let ok = (d5 != 0.0 || a[1].y != a[2].y) && (d6 != 0.0 || a[1].x != a[2].x) && {
            let d4 = d5 * d5 + d6 * d6;
            d4 != 0.0 && {
                let d3 = d5 * sub(a[2].x, a[1].x) as f64 + d6 * sub(a[2].y, a[1].y) as f64;
                d3 * d3 / d4 < 1.0
                    && (d5 * (sub(a[0].x, a[1].x) as f64) + d6 * (sub(a[0].y, a[1].y) as f64) < 0.0) == (d5 * (sub(a[4].x, a[1].x) as f64) + d6 * (sub(a[4].y, a[1].y) as f64) < 0.0)
                    && {
                        let d2 = sq(sub(a[2].x, a[0].x), sub(a[2].y, a[0].y));
                        let d = sq(sub(a[4].x, a[2].x), sub(a[4].y, a[2].y));
                        d2 <= 3.0 * d && d <= 3.0 * d2
                    }
            }
        };
        if ok {
            c.curveto(
                jround(a[1].x.wrapping_mul(4).wrapping_sub(a[0].x) as f64 / 3.0),
                jround(a[1].y.wrapping_mul(4).wrapping_sub(a[0].y) as f64 / 3.0),
                jround(a[3].x.wrapping_mul(4).wrapping_sub(a[4].x) as f64 / 3.0),
                jround(a[3].y.wrapping_mul(4).wrapping_sub(a[4].y) as f64 / 3.0),
                a[4].x as f64,
                a[4].y as f64,
            );
            let a4 = a[4];
            a[0].set_xy(a4);
            return true;
        }
        Self::add_curve(a, c);
        let (a2, a3, a4) = (a[2], a[3], a[4]);
        a[0].set_xy(a2);
        a[1].set_xy(a3);
        a[2].set_xy(a4);
        false
    }

    fn emit_contours_approx_path(&self, oc: &mut dyn OutlineConsumer) {
        let mut a = [Coord::default(); 6];
        let mut n2: i32 = 0;
        for i in 0..self.end_points.len() {
            let n = self.end_points[i];
            if n2 >= n {
                n2 = n + 1;
                continue;
            }
            let mut n3 = 0;
            let mut n4 = n - n2 + 1;
            let coord2 = n2 as usize;
            let coord3 = n as usize;
            let mut n5 = n2 as usize;
            let mut cur: usize;
            if self.points[coord2].flags & 1 != 0 {
                cur = coord2;
                a[0].set_xy(self.points[cur]);
            } else if self.points[coord3].flags & 1 != 0 {
                cur = coord3;
                n5 = n as usize;
                a[0].set_xy(self.points[cur]);
            } else {
                cur = coord3;
                n5 = n as usize;
                n4 += 1;
                a[0].set_xy_at_mid_point(self.points[coord2], self.points[coord3]);
            }
            oc.moveto(a[0].x as f64, a[0].y as f64);
            while n4 > 0 {
                n4 -= 1;
                if cur == coord3 {
                    cur = coord2;
                    n5 = n2 as usize;
                } else {
                    n5 += 1;
                    cur = n5;
                }
                let coord = self.points[cur];
                if coord.flags & 1 != 0 {
                    match n3 {
                        0 => {
                            if n4 > 0 {
                                oc.lineto(coord.x as f64, coord.y as f64);
                                a[0].set_xy(coord);
                            }
                        }
                        1 => {
                            a[2].set_xy(coord);
                            n3 = 3;
                        }
                        2 | 4 => {
                            a[4].set_xy(coord);
                            n3 = if Self::combine_pair(&mut a, oc) { 0 } else { 3 };
                        }
                        3 => {
                            Self::add_curve(&a, oc);
                            if n4 > 0 {
                                oc.lineto(coord.x as f64, coord.y as f64);
                                a[0].set_xy(coord);
                            }
                            n3 = 0;
                        }
                        _ => {}
                    }
                    continue;
                }
                match n3 {
                    0 => {
                        a[1].set_xy(coord);
                        n3 = 1;
                    }
                    1 => {
                        a[3].set_xy(coord);
                        let (a1, a3) = (a[1], a[3]);
                        a[2].set_xy_at_mid_point(a1, a3);
                        n3 = 2;
                    }
                    2 | 4 => {
                        let was4 = n3 == 4;
                        a[5].set_xy(coord);
                        let (a3, a5) = (a[3], a[5]);
                        a[4].set_xy_at_mid_point(a3, a5);
                        if Self::combine_pair(&mut a, oc) {
                            let a5 = a[5];
                            a[1].set_xy(a5);
                            n3 = 1;
                        } else {
                            let a5 = a[5];
                            a[3].set_xy(a5);
                            n3 = if was4 { 2 } else { 4 };
                        }
                    }
                    3 => {
                        a[3].set_xy(coord);
                        n3 = 4;
                    }
                    _ => {}
                }
            }
            match n3 {
                2 => {
                    a[3].set_xy(self.points[cur]);
                    let (a1, a3) = (a[1], a[3]);
                    a[2].set_xy_at_mid_point(a1, a3);
                    Self::add_curve(&a, oc);
                }
                3 | 4 => Self::add_curve(&a, oc),
                _ => {}
            }
            n2 = n + 1;
        }
        oc.endchar();
    }

    fn emit_contours_exact_path(&self, oc: &mut dyn OutlineConsumer) {
        let mut n: i32 = 0;
        for i in 0..self.end_points.len() {
            let mut d = 0.0;
            let mut d2 = 0.0;
            let n3 = self.end_points[i];
            if n >= n3 {
                n = n3 + 1;
                continue;
            }
            let mut n4 = n;
            let mut searching = true;
            let mut coord = Coord::default();
            while searching && n4 < n3 {
                coord = self.points[n4 as usize];
                if coord.flags & 1 != 0 {
                    oc.moveto(coord.x as f64, coord.y as f64);
                    searching = false;
                    continue;
                }
                n4 += 1;
            }
            if searching {
                let coord3 = self.points[n3 as usize];
                n4 = n;
                coord = self.points[n4 as usize];
                oc.moveto(coord.x.wrapping_add(coord3.x) as f64 / 2.0, coord.y.wrapping_add(coord3.y) as f64 / 2.0);
                d = coord3.x.wrapping_add(coord.x.wrapping_mul(5)) as f64 / 6.0;
                d2 = coord3.y.wrapping_add(coord.y.wrapping_mul(5)) as f64 / 6.0;
            }
            let mut n5 = n4;
            loop {
                let was = n5;
                n5 += 1;
                if was == n3 {
                    n5 = n;
                }
                let coord2 = self.points[n5 as usize];
                if coord.flags & 1 != 0 {
                    if coord2.flags & 1 != 0 {
                        oc.lineto(coord2.x as f64, coord2.y as f64);
                    } else {
                        d = coord2.x as f64;
                        d2 = coord2.y as f64;
                    }
                } else if coord2.flags & 1 != 0 {
                    oc.curveto_quad(d, d2, coord2.x as f64, coord2.y as f64);
                } else {
                    oc.curveto_quad(d, d2, coord.x.wrapping_add(coord2.x) as f64 / 2.0, coord.y.wrapping_add(coord2.y) as f64 / 2.0);
                    d = coord2.x as f64;
                    d2 = coord2.y as f64;
                }
                coord = self.points[n5 as usize];
                if n5 == n4 {
                    break;
                }
            }
            n = n3 + 1;
        }
        oc.endchar();
    }

    fn emit_contours(&mut self, oc: &mut dyn OutlineConsumer, n: i32) {
        let mut n2: i32 = 0;
        for i in 0..self.end_points.len() {
            let n3 = self.end_points[i];
            if n2 >= n3 {
                n2 = n3 + 1;
                continue;
            }
            let mut n4 = n3;
            let mut j = n3 + n2 + 1 - n4;
            while n4 > j {
                self.points.swap(j as usize, n4 as usize);
                n4 -= 1;
                j += 1;
            }
            n2 = n3 + 1;
        }
        if n == 1 {
            self.emit_contours_exact_path(oc);
        } else {
            self.emit_contours_approx_path(oc);
        }
    }

    /// `parse(font, gid, consumer, pathType)`.
    pub fn parse_with(&mut self, font: &OpenTypeFont, n: i32, oc: &mut dyn OutlineConsumer, n2: i32) -> FontResult<()> {
        let glyf = font.glyf.as_ref().ok_or_else(npe)?;
        self.points.clear();
        self.end_points.clear();
        let mut n3 = match glyf.get_glyph_location(n) {
            Ok(v) => v,
            Err(e) if is_invalid_font(&e) => {
                oc.endchar();
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        let r: FontResult<()> = (|| {
            let inner: FontResult<Option<i32>> = (|| {
                if glyf.get_glyph_location(n + 1)? == n3 {
                    return Ok(None);
                }
                Ok(Some(glyf.glyf.data.getint16(n3)?))
            })();
            let n5 = match inner {
                Ok(Some(v)) => v,
                Ok(None) => {
                    oc.endchar();
                    return Ok(());
                }
                Err(e) if is_invalid_font(&e) => {
                    oc.endchar();
                    return Ok(());
                }
                Err(e) => return Err(e),
            };
            let mut n6 = n;
            n3 += 10;
            let head = font.head.as_ref().ok_or_else(|| FontEngineException::invalid_font("OpenType font with 'glyf' table needs a 'head' table"))?;
            let upem = head.get_units_per_em()? as f64;
            oc.set_matrix(&Matrix::new(1.0 / upem, 0.0, 0.0, 1.0 / upem, 0.0, 0.0));
            let n4;
            if n5 < 0 {
                n6 = self.get_composite_glyph(glyf, n3, n)?;
                n4 = glyf.glyf.data.getint16(glyf.get_glyph_location(n6)? + 2)?;
            } else {
                self.get_simple_glyph(glyf, n3, n5)?;
                n4 = glyf.glyf.data.getint16(n3 - 8)?;
            }
            self.adjust_points_by_side_bearing(n6, font.hmtx.as_ref(), n4)?;
            self.emit_contours(oc, n2);
            Ok(())
        })();
        r.map_err(|e| if is_invalid_font(&e) { FontEngineException::from_cause(ExceptionKind::InvalidGlyph, e.to_java_string()) } else { e })
    }

    pub fn parse(&mut self, font: &OpenTypeFont, n: i32, oc: &mut dyn OutlineConsumer) -> FontResult<()> {
        self.parse_with(font, n, oc, 1)
    }
}

/// `instanceof InvalidFontException` (which `InvalidGlyphException` extends).
pub fn is_invalid_font(e: &FontEngineException) -> bool {
    matches!(e.kind, ExceptionKind::InvalidFont | ExceptionKind::InvalidGlyph)
}

fn npe() -> FontEngineException {
    FontEngineException::new(ExceptionKind::FontEngine, "java.lang.NullPointerException")
}
