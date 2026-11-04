//! Port of JDK 17 `sun.java2d.pipe.ShapeSpanIterator` (the Java class) together with its native
//! `ShapeSpanIterator.c` back end in libawt: `setNormalize`, `setOutputAreaXYXY`, `setRule`,
//! `moveTo`/`lineTo`/`quadTo`/`curveTo`/`closePath`/`pathDone` (the `PC*` entry points, including the
//! `adjust`/normalization of coordinates and control points, the path box, the curve subdivision and
//! the fixed-point DDA segment setup), `appendPath` (the Java `appendPath(PathIterator)` loop),
//! `getPathBox`, `intersectClipBox`, `nextSpan` and `skipDownTo`.
//!
//! The fixed-point and rounding were reverse-engineered from the libawt disassembly and are bit-exact
//! against a running JVM. [`PathConsumer2D`] is implemented so the Marlin stroker can feed it directly.

use crate::asc::flex2::fonts::java2d::sun::java2d::marlin::PathConsumer2D;

/// `(double) ERRSTEP_MAX` (`0x7fffffff`): `FRACTTOJINT`'s scale in `appendSegment`.
const SCALE: f64 = 2147483647.0;

const STATE_INIT: u8 = 0;
const STATE_HAVE_RECT: u8 = 1;
const STATE_HAVE_RULE: u8 = 2;
const STATE_PATH_DONE: u8 = 3;
const STATE_SEGMENTS: u8 = 4;

/// One scanline-DDA edge (the native `pathData` segment, 28 bytes).
#[derive(Clone, Copy, Default)]
struct PathSegment {
    curx: i32,    // +0x00 x at the current scanline (pixel-centre rounded)
    y0: i32,      // +0x04 current / top scanline
    y1: i32,      // +0x08 bottom scanline (exclusive)
    xerr: i32,    // +0x0c 31-bit fractional x accumulator
    bumpx: i32,   // +0x10 integer dx per scanline
    bumperr: i32, // +0x14 31-bit fractional dx per scanline
    dir: i8,      // +0x18 winding direction (+1 / -1)
}

/// `sun.java2d.pipe.ShapeSpanIterator`.
pub struct ShapeSpanIterator {
    state: u8,
    evenodd: bool,
    first: bool,
    adjust: bool,
    lox: i32,
    loy: i32,
    hix: i32,
    hiy: i32,
    curx: f32,
    cury: f32,
    movx: f32,
    movy: f32,
    adjx: f32,
    adjy: f32,
    pathlox: f32,
    pathloy: f32,
    pathhix: f32,
    pathhiy: f32,
    segments: Vec<PathSegment>,
    table: Vec<usize>,
    lo_segment: i32,
    cur_segment: i32,
    hi_segment: i32,
}

/// `cvttss2si`: truncate toward zero, with out-of-range / NaN giving `0x80000000`.
#[inline]
fn cvttss(v: f32) -> i32 {
    if v >= -2147483648.0 && v < 2147483648.0 {
        v as i32
    } else {
        i32::MIN
    }
}

/// `cvttsd2si` (32-bit result).
#[inline]
fn cvttsd(v: f64) -> i32 {
    if v > -2147483649.0 && v < 2147483648.0 {
        v as i32
    } else {
        i32::MIN
    }
}

/// `floorf` as gcc inlines it: corrected truncation when `|v| < 2^23`, else `v` unchanged.
#[inline]
fn floorf(v: f32) -> f32 {
    if v.abs() < 8388608.0 {
        let t = (v as i32) as f32;
        let r = if t > v { t - 1.0 } else { t };
        f32::from_bits(r.to_bits() | (v.to_bits() & 0x8000_0000))
    } else {
        v
    }
}

/// `ceilf` as gcc inlines it.
#[inline]
fn ceilf(v: f32) -> f32 {
    if v.abs() < 8388608.0 {
        let t = (v as i32) as f32;
        let r = if v > t { t + 1.0 } else { t };
        f32::from_bits(r.to_bits() | (v.to_bits() & 0x8000_0000))
    } else {
        v
    }
}

/// `minmax2`.
#[inline]
fn minmax2(v1: f32, v2: f32) -> (f32, f32) {
    if v1 < v2 {
        (v1, v2)
    } else {
        (v2, v1)
    }
}

/// `minmax3`.
#[inline]
fn minmax3(v1: f32, v2: f32, v3: f32) -> (f32, f32) {
    if v1 < v2 {
        if v1 < v3 {
            (v1, if v2 < v3 { v3 } else { v2 })
        } else {
            (v3, v2)
        }
    } else if v1 < v3 {
        (v2, v3)
    } else {
        (if v2 < v3 { v2 } else { v3 }, v1)
    }
}

/// `minmax4`.
#[inline]
fn minmax4(v1: f32, v2: f32, v3: f32, v4: f32) -> (f32, f32) {
    if v1 < v2 {
        if v3 < v4 {
            (if v1 < v3 { v1 } else { v3 }, if v2 < v4 { v4 } else { v2 })
        } else {
            (if v1 < v4 { v1 } else { v4 }, if v2 < v3 { v3 } else { v2 })
        }
    } else if v3 < v4 {
        (if v2 < v3 { v2 } else { v3 }, if v1 < v4 { v4 } else { v1 })
    } else {
        (if v2 < v4 { v2 } else { v4 }, if v1 < v3 { v3 } else { v1 })
    }
}

/// The `_ADJUST` snap: `floorf(v + 0.25f) + 0.25f`.
#[inline]
fn snap(v: f32) -> f32 {
    floorf(v + 0.25) + 0.25
}

impl ShapeSpanIterator {
    /// `new ShapeSpanIterator(boolean)` -> `setNormalize(adjust)`.
    pub fn new(adjust: bool) -> Self {
        ShapeSpanIterator {
            state: STATE_INIT,
            evenodd: false,
            first: true,
            adjust,
            lox: 0,
            loy: 0,
            hix: 0,
            hiy: 0,
            curx: 0.0,
            cury: 0.0,
            movx: 0.0,
            movy: 0.0,
            adjx: 0.0,
            adjy: 0.0,
            pathlox: 0.0,
            pathloy: 0.0,
            pathhix: 0.0,
            pathhiy: 0.0,
            segments: Vec::new(),
            table: Vec::new(),
            lo_segment: 0,
            cur_segment: 0,
            hi_segment: 0,
        }
    }

    /// `setOutputAreaXYXY(int,int,int,int)`.
    pub fn set_output_area_xyxy(&mut self, lox: i32, loy: i32, hix: i32, hiy: i32) {
        // native: only valid in STATE_INIT
        self.lox = lox;
        self.loy = loy;
        self.hix = hix;
        self.hiy = hiy;
        self.state = STATE_HAVE_RECT;
    }

    /// `setRule(int)` (0 == `WIND_EVEN_ODD`).
    pub fn set_rule(&mut self, rule: i32) {
        self.evenodd = rule == 0;
        self.state = STATE_HAVE_RULE;
    }

    /// `HANDLEMOVETO`: close the pending subpath, snap (`ADJUST1`), record the subpath start and
    /// add the (adjusted) point to the path box.
    pub fn move_to(&mut self, mut x: f32, mut y: f32) {
        self.handle_close();
        if self.adjust {
            let nx = snap(x);
            let ny = snap(y);
            self.adjx = nx - x;
            self.adjy = ny - y;
            x = nx;
            y = ny;
        }
        self.movx = x;
        self.movy = y;
        self.box_point(x, y);
        self.curx = x;
        self.cury = y;
    }

    /// `HANDLELINETO`.
    pub fn line_to(&mut self, mut x: f32, mut y: f32) {
        if self.adjust {
            let nx = snap(x);
            let ny = snap(y);
            self.adjx = nx - x;
            self.adjy = ny - y;
            x = nx;
            y = ny;
        }
        let (cx, cy) = (self.curx, self.cury);
        self.subdivide_line(cx, cy, x, y);
        self.box_point(x, y);
        self.curx = x;
        self.cury = y;
    }

    /// `HANDLEQUADTO`. `ADJUST2` keeps the JDK quirk of using the new endpoint's y adjustment for
    /// both control-point coordinates.
    pub fn quad_to(&mut self, mut x1: f32, mut y1: f32, mut x2: f32, mut y2: f32) {
        if self.adjust {
            let nx = snap(x2);
            let ny = snap(y2);
            let newadjy = ny - y2;
            x1 += (self.adjx + newadjy) * 0.5;
            y1 += (self.adjy + newadjy) * 0.5;
            self.adjx = nx - x2;
            self.adjy = ny - y2;
            x2 = nx;
            y2 = ny;
        }
        let (x0, y0) = (self.curx, self.cury);
        self.subdivide_quad(0, x0, y0, x1, y1, x2, y2);
        self.box_point(x1, y1);
        self.box_point(x2, y2);
        self.curx = x2;
        self.cury = y2;
    }

    /// `HANDLECUBICTO` (`ADJUST3`: first control by the old adjustment, second by the new one).
    #[allow(clippy::too_many_arguments)]
    pub fn curve_to(&mut self, mut x1: f32, mut y1: f32, mut x2: f32, mut y2: f32, mut x3: f32, mut y3: f32) {
        if self.adjust {
            let nx = snap(x3);
            let ny = snap(y3);
            x1 += self.adjx;
            y1 += self.adjy;
            self.adjx = nx - x3;
            self.adjy = ny - y3;
            x2 += self.adjx;
            y2 += self.adjy;
            x3 = nx;
            y3 = ny;
        }
        let (x0, y0) = (self.curx, self.cury);
        self.subdivide_cubic(0, x0, y0, x1, y1, x2, y2, x3, y3);
        self.box_point(x1, y1);
        self.box_point(x2, y2);
        self.box_point(x3, y3);
        self.curx = x3;
        self.cury = y3;
    }

    /// `HANDLECLOSE`.
    pub fn close_path(&mut self) {
        self.handle_close();
    }

    /// `HANDLEENDPATH`.
    pub fn path_done(&mut self) {
        self.handle_close();
        self.state = STATE_PATH_DONE;
    }

    fn handle_close(&mut self) {
        if self.curx != self.movx || self.cury != self.movy {
            let (cx, cy, mx, my) = (self.curx, self.cury, self.movx, self.movy);
            self.subdivide_line(cx, cy, mx, my);
            self.curx = mx;
            self.cury = my;
        }
    }

    /// Java `appendPath(PathIterator)`: `setRule(getWindingRule())` then replay the segments and
    /// `pathDone()`. Each segment is `(type, [6 doubles])`; doubles are narrowed to `float` exactly
    /// as the bytecode's `currentSegment(float[])` does.
    pub fn append_path(&mut self, segments: &[(i32, [f64; 6])], winding_rule: i32) {
        self.set_rule(winding_rule);
        for &(t, c) in segments {
            match t {
                0 => self.move_to(c[0] as f32, c[1] as f32),
                1 => self.line_to(c[0] as f32, c[1] as f32),
                2 => self.quad_to(c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32),
                3 => self.curve_to(c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32, c[4] as f32, c[5] as f32),
                4 => self.close_path(),
                _ => {}
            }
        }
        self.path_done();
    }

    /// `getPathBox(int[])`: `{floor(lox), floor(loy), ceil(hix), ceil(hiy)}`.
    pub fn get_path_box(&self) -> [i32; 4] {
        [
            cvttss(floorf(self.pathlox)),
            cvttss(floorf(self.pathloy)),
            cvttss(ceilf(self.pathhix)),
            cvttss(ceilf(self.pathhiy)),
        ]
    }

    /// `intersectClipBox(int,int,int,int)`.
    pub fn intersect_clip_box(&mut self, lox: i32, loy: i32, hix: i32, hiy: i32) {
        if lox > self.lox {
            self.lox = lox;
        }
        if loy > self.loy {
            self.loy = loy;
        }
        if hix < self.hix {
            self.hix = hix;
        }
        if hiy < self.hiy {
            self.hiy = hiy;
        }
    }

    /// `PDBOXPOINT`.
    #[inline]
    fn box_point(&mut self, x: f32, y: f32) {
        if self.first {
            self.pathlox = x;
            self.pathhix = x;
            self.pathloy = y;
            self.pathhiy = y;
            self.first = false;
        } else {
            if self.pathlox > x {
                self.pathlox = x;
            }
            if self.pathloy > y {
                self.pathloy = y;
            }
            if self.pathhix < x {
                self.pathhix = x;
            }
            if self.pathhiy < y {
                self.pathhiy = y;
            }
        }
    }

    /// `subdivideLine` (inlined into the entry points): drop lines below, above or right of the
    /// output area; a line wholly left of it still counts for winding as a vertical at its max x.
    fn subdivide_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        let (minx, maxx) = minmax2(x0, x1);
        let (miny, maxy) = minmax2(y0, y1);
        if (self.loy as f32) >= maxy || (self.hiy as f32) <= miny || (self.hix as f32) <= minx {
            return;
        }
        if (self.lox as f32) >= maxx {
            self.append_segment(maxx, y0, maxx, y1);
        } else {
            self.append_segment(x0, y0, x1, y1);
        }
    }

    /// `appendSegment`: build a DDA edge from `(x0,y0)`-`(x1,y1)`, dropped when it crosses no sample
    /// row inside the output band.
    fn append_segment(&mut self, mut x0: f32, mut y0: f32, mut x1: f32, mut y1: f32) {
        let dir;
        if y0 > y1 {
            std::mem::swap(&mut x0, &mut x1);
            std::mem::swap(&mut y0, &mut y1);
            dir = -1i8;
        } else {
            dir = 1i8;
        }
        let istarty = cvttss(ceilf(y0 - 0.5));
        let ilasty = cvttss(ceilf(y1 - 0.5));
        if istarty >= ilasty || istarty >= self.hiy || ilasty <= self.loy {
            return;
        }
        let dx = x1 - x0;
        let dy = y1 - y0;
        let slope = dx / dy;
        let ystartbump = istarty as f32 + 0.5 - y0;
        let xstart = x0 + dx * ystartbump / dy;
        let istartx = cvttss(ceilf(xstart - 0.5));
        let fslope = floorf(slope);
        let bumpx = cvttss(fslope);
        let bumperr = cvttsd((slope as f64 - fslope as f64) * SCALE);
        let error = cvttsd((xstart - (istartx as f32 - 0.5)) as f64 * SCALE);
        self.segments.push(PathSegment {
            curx: istartx,
            y0: istarty,
            y1: ilasty,
            xerr: error,
            bumpx,
            bumperr,
            dir,
        });
    }

    /// `subdivideQuad`.
    #[allow(clippy::too_many_arguments)]
    fn subdivide_quad(&mut self, level: i32, x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
        let (minx, maxx) = minmax3(x0, x1, x2);
        let (miny, maxy) = minmax3(y0, y1, y2);
        if (self.loy as f32) >= maxy || (self.hiy as f32) <= miny || (self.hix as f32) <= minx {
            return;
        }
        if (self.lox as f32) >= maxx {
            self.append_segment(maxx, y0, maxx, y2);
            return;
        }
        if level < 10 && !quad_flat(x0, y0, x1, y1, x2, y2) {
            let cx1 = (x0 + x1) * 0.5;
            let cx2 = (x1 + x2) * 0.5;
            let mx = (cx1 + cx2) * 0.5;
            let cy1 = (y0 + y1) * 0.5;
            let cy2 = (y1 + y2) * 0.5;
            let my = (cy1 + cy2) * 0.5;
            self.subdivide_quad(level + 1, x0, y0, cx1, cy1, mx, my);
            self.subdivide_quad(level + 1, mx, my, cx2, cy2, x2, y2);
        } else {
            self.append_segment(x0, y0, x2, y2);
        }
    }

    /// `subdivideCubic`.
    #[allow(clippy::too_many_arguments)]
    fn subdivide_cubic(&mut self, level: i32, x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
        let (minx, maxx) = minmax4(x0, x1, x2, x3);
        let (miny, maxy) = minmax4(y0, y1, y2, y3);
        if (self.loy as f32) >= maxy || (self.hiy as f32) <= miny || (self.hix as f32) <= minx {
            return;
        }
        if (self.lox as f32) >= maxx {
            self.append_segment(maxx, y0, maxx, y3);
            return;
        }
        if level < 10 && !cubic_flat(x0, y0, x1, y1, x2, y2, x3, y3) {
            let ctrx = (x1 + x2) * 0.5;
            let nx1 = (x0 + x1) * 0.5;
            let nx2 = (x2 + x3) * 0.5;
            let ctrx1 = (ctrx + nx1) * 0.5;
            let ctrx2 = (ctrx + nx2) * 0.5;
            let mx = (ctrx1 + ctrx2) * 0.5;
            let ctry = (y1 + y2) * 0.5;
            let ny1 = (y0 + y1) * 0.5;
            let ny2 = (y2 + y3) * 0.5;
            let ctry1 = (ctry + ny1) * 0.5;
            let ctry2 = (ctry + ny2) * 0.5;
            let my = (ctry1 + ctry2) * 0.5;
            self.subdivide_cubic(level + 1, x0, y0, nx1, ny1, ctrx1, ctry1, mx, my);
            self.subdivide_cubic(level + 1, mx, my, ctrx2, ctry2, nx2, ny2, x3, y3);
        } else {
            self.append_segment(x0, y0, x3, y3);
        }
    }

    /// `initSegmentTable`: sort the segments (glibc stable merge sort by `(y0, curx, maxy)`) and set
    /// the initial span window.
    fn init_segment_table(&mut self) -> bool {
        self.state = STATE_SEGMENTS;
        let n = self.segments.len();
        let segs = &self.segments;
        let mut table: Vec<usize> = (0..n).collect();
        // sortSegmentsByLeadingY: compare y0, then curx, then y1 (maxy). Stable (merge sort).
        table.sort_by(|&a, &b| {
            let sa = &segs[a];
            let sb = &segs[b];
            sa.y0
                .cmp(&sb.y0)
                .then(sa.curx.cmp(&sb.curx))
                .then(sa.y1.cmp(&sb.y1))
        });
        // first index whose segment is still live at loy (y1 > loy)
        let loy = self.loy;
        let mut k = 0;
        while k < n && self.segments[table[k]].y1 <= loy {
            k += 1;
        }
        self.table = table;
        self.lo_segment = k as i32;
        self.cur_segment = k as i32;
        self.hi_segment = k as i32;
        self.loy = loy - 1;
        true
    }

    /// `skipDownTo(int)`.
    pub fn skip_down_to(&mut self, y: i32) {
        if self.state != STATE_SEGMENTS {
            self.init_segment_table();
        }
        if self.loy < y {
            self.cur_segment = self.hi_segment;
            self.loy = y - 1;
        }
    }

    /// `nextSpan(int[])` -> the next `{lox, loy, hix, hiy}` span, or `false` when exhausted.
    pub fn next_span(&mut self, span: &mut [i32; 4]) -> bool {
        if self.state != STATE_SEGMENTS && !self.init_segment_table() {
            self.lo_segment = self.segments.len() as i32;
            return false;
        }
        let num = self.segments.len() as i32;
        let mut lo = self.lo_segment;
        let mut cur = self.cur_segment;
        let mut hi = self.hi_segment;
        let mut loy = self.loy;

        while lo < num {
            if hi > cur {
                // emit spans within the current scanline
                let x0seg = self.segments[self.table[cur as usize]].curx;
                if self.hix > x0seg {
                    let mut x0 = x0seg;
                    if x0 < self.lox {
                        x0 = self.lox;
                    }
                    let x1;
                    if self.evenodd {
                        cur += 2;
                        let xr = if cur <= hi {
                            self.segments[self.table[(cur - 1) as usize]].curx
                        } else {
                            self.hix
                        };
                        x1 = if self.hix < xr { self.hix } else { xr };
                    } else {
                        let mut wind = self.segments[self.table[cur as usize]].dir as i32;
                        cur += 1;
                        let mut found = false;
                        while cur < hi {
                            wind += self.segments[self.table[cur as usize]].dir as i32;
                            if wind == 0 {
                                found = true;
                                break;
                            }
                            cur += 1;
                        }
                        if found {
                            let xr = self.segments[self.table[cur as usize]].curx;
                            x1 = if self.hix < xr { self.hix } else { xr };
                            cur += 1;
                        } else {
                            // winding never closed: span runs to the right clip edge
                            x1 = self.hix;
                        }
                    }
                    if x0 < x1 {
                        span[0] = x0;
                        span[1] = loy;
                        span[2] = x1;
                        span[3] = loy.wrapping_add(1);
                        self.lo_segment = lo;
                        self.cur_segment = cur;
                        self.hi_segment = hi;
                        self.loy = loy;
                        return true;
                    }
                    continue;
                } else {
                    cur = hi;
                }
                if lo < num {
                    continue;
                }
                break;
            }
            // advance to the next scanline
            loy = loy.wrapping_add(1);
            if self.hiy <= loy {
                self.lo_segment = num;
                self.cur_segment = num;
                self.hi_segment = num;
                self.loy = loy;
                return false;
            }
            // compact: drop segments whose y1 <= loy, keeping survivors packed above `cur`
            cur = hi;
            let mut none_survived = true;
            let mut i = hi - 1;
            while i >= lo {
                let seg = self.table[i as usize];
                if self.segments[seg].y1 > loy {
                    cur -= 1;
                    self.table[cur as usize] = seg;
                    none_survived = false;
                }
                i -= 1;
            }
            if cur < num && none_survived {
                // no active edges: jump to the next edge's top scanline
                let ny0 = self.segments[self.table[cur as usize]].y0;
                if loy < ny0 {
                    loy = ny0;
                }
            }
            // extend `hi` to include edges that now start at/above loy
            while hi < num && self.segments[self.table[hi as usize]].y0 <= loy {
                hi += 1;
            }
            // bump active edges to the new scanline and insertion-sort them by curx
            if hi > cur {
                let first = self.table[cur as usize];
                self.bump_segment(first, loy);
                let mut k = cur + 1;
                while k < hi {
                    let seg = self.table[k as usize];
                    self.bump_segment(seg, loy);
                    let segx = self.segments[seg].curx;
                    let mut pos = k;
                    while pos > cur && self.segments[self.table[(pos - 1) as usize]].curx > segx {
                        self.table[pos as usize] = self.table[(pos - 1) as usize];
                        pos -= 1;
                    }
                    self.table[pos as usize] = seg;
                    k += 1;
                }
            }
            // the dead region below `cur` is excluded from the next compaction
            lo = cur;
        }
        self.lo_segment = lo;
        self.cur_segment = cur;
        self.hi_segment = hi;
        self.loy = loy;
        false
    }

    /// Advance one edge's DDA from its stored scanline to `loy` (the native single-step and
    /// multi-step forms differ in their carry arithmetic).
    #[inline]
    fn bump_segment(&mut self, seg: usize, loy: i32) {
        let s = &mut self.segments[seg];
        if s.y0.wrapping_add(1) == loy {
            let e = (s.xerr as u32).wrapping_add(s.bumperr as u32);
            s.curx = s.curx.wrapping_add(s.bumpx).wrapping_add((e >> 31) as i32);
            s.xerr = (e & 0x7fff_ffff) as i32;
        } else {
            let steps = loy as i64 - s.y0 as i64;
            let e = s.bumperr as i64 * steps + s.xerr as i64;
            s.curx = s.curx.wrapping_add(s.bumpx.wrapping_mul(steps as i32)).wrapping_add((e >> 31) as i32);
            s.xerr = (e as i32) & 0x7fff_ffff;
        }
        s.y0 = loy;
    }
}

/// `subdivideQuad` flatness test: the control point's perpendicular distance² from the chord
/// `P0`-`P2`, clamped at the endpoints, compared against `1.0` (the native dot-product test).
#[allow(clippy::too_many_arguments)]
fn quad_flat(x0: f32, y0: f32, cx: f32, cy: f32, x1: f32, y1: f32) -> bool {
    ctrl_flat(x1 - x0, y1 - y0, cx - x0, cy - y0)
}

/// `subdivideCubic` flatness test (both control points within one unit of the chord `P0`-`P3`).
#[allow(clippy::too_many_arguments)]
fn cubic_flat(x0: f32, y0: f32, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x1: f32, y1: f32) -> bool {
    let ux = x1 - x0;
    let uy = y1 - y0;
    if !ctrl_flat(ux, uy, c1x - x0, c1y - y0) {
        return false;
    }
    ctrl_flat(ux, uy, c2x - x0, c2y - y0)
}

/// `u = P_end - P0`, `v = C - P0`. Perpendicular distance² of `C` from the chord, clamped to the
/// nearest endpoint when the projection falls outside the segment; flat when `<= 1.0`.
#[inline]
fn ctrl_flat(ux: f32, uy: f32, vx: f32, vy: f32) -> bool {
    let ucv = ux * vx + uy * vy;
    let d2 = if ucv <= 0.0 {
        vx * vx + vy * vy
    } else {
        let wx = ux - vx;
        let wy = uy - vy;
        let ucw = ux * wx + uy * wy;
        if ucw <= 0.0 {
            wx * wx + wy * wy
        } else {
            let ulen = ux * ux + uy * uy;
            wx * wx + wy * wy - ucw * ucw / ulen
        }
    };
    !(d2 > 1.0)
}

impl PathConsumer2D for ShapeSpanIterator {
    fn move_to(&mut self, x: f32, y: f32) {
        ShapeSpanIterator::move_to(self, x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        ShapeSpanIterator::line_to(self, x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        ShapeSpanIterator::quad_to(self, x1, y1, x2, y2);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
        ShapeSpanIterator::curve_to(self, x1, y1, x2, y2, x3, y3);
    }
    fn close_path(&mut self) {
        ShapeSpanIterator::close_path(self);
    }
    fn path_done(&mut self) {
        ShapeSpanIterator::path_done(self);
    }
}
