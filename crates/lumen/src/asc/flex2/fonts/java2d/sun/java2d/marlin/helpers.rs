//! `sun.java2d.marlin.Helpers` and `Helpers.PolyStack`.

use super::curve::Curve;
use super::marlin_const::{OUTCODE_BOTTOM, OUTCODE_LEFT, OUTCODE_RIGHT, OUTCODE_TOP};
use super::math;
use super::DPathConsumer2D;

/// `Helpers.within(double, double, double)`.
pub fn within(x: f64, y: f64, err: f64) -> bool {
    let d = y - x;
    d <= err && d >= -err
}

/// `Helpers.quadraticRoots`.
pub fn quadratic_roots(a: f64, b: f64, c: f64, zeroes: &mut [f64], off: usize) -> usize {
    let mut ret = off;
    if a != 0.0 {
        let dis = b * b - 4.0 * a * c;
        if dis > 0.0 {
            let sqrt_dis = dis.sqrt();
            if b >= 0.0 {
                zeroes[ret] = (2.0 * c) / (-b - sqrt_dis);
                ret += 1;
                zeroes[ret] = (-b - sqrt_dis) / (2.0 * a);
                ret += 1;
            } else {
                zeroes[ret] = (-b + sqrt_dis) / (2.0 * a);
                ret += 1;
                zeroes[ret] = (2.0 * c) / (-b + sqrt_dis);
                ret += 1;
            }
        } else if dis == 0.0 {
            zeroes[ret] = -b / (2.0 * a);
            ret += 1;
        }
    } else if b != 0.0 {
        zeroes[ret] = -c / b;
        ret += 1;
    }
    ret - off
}

const PI_DIV_3: f64 = 1.0471975511965976;

/// `Helpers.cubicRootsInAB`.
#[allow(clippy::too_many_arguments)]
pub fn cubic_roots_in_ab(d: f64, a: f64, b: f64, c: f64, pts: &mut [f64], off: usize, aa: f64, bb: f64) -> usize {
    if d == 0.0 {
        let num = quadratic_roots(a, b, c, pts, off);
        return filter_out_not_in_ab(pts, off, num, aa, bb) - off;
    }
    let a = a / d;
    let b = b / d;
    let c = c / d;

    let sub = (1.0 / 3.0) * a;
    let sq_a = a * a;
    let p = (1.0 / 3.0) * ((-1.0 / 3.0) * sq_a + b);
    let q = 0.5 * ((2.0 / 27.0) * a * sq_a - sub * b + c);

    let cb_p = p * p * p;
    let dd = q * q + cb_p;

    let num;
    if dd < 0.0 {
        let phi = (1.0 / 3.0) * math::acos(-q / (-cb_p).sqrt());
        let t = 2.0 * (-p).sqrt();

        pts[off] = t * math::cos(phi) - sub;
        pts[off + 1] = -t * math::cos(phi + PI_DIV_3) - sub;
        pts[off + 2] = -t * math::cos(phi - PI_DIV_3) - sub;
        num = 3;
    } else {
        let sqrt_d = dd.sqrt();
        let u = math::cbrt(sqrt_d - q);
        let v = -math::cbrt(sqrt_d + q);

        pts[off] = u + v - sub;
        if within(dd, 0.0, 1e-8) {
            pts[off + 1] = -0.5 * (u + v) - sub;
            num = 2;
        } else {
            num = 1;
        }
    }
    filter_out_not_in_ab(pts, off, num, aa, bb) - off
}

/// `Helpers.filterOutNotInAB`.
pub fn filter_out_not_in_ab(nums: &mut [f64], off: usize, len: usize, a: f64, b: f64) -> usize {
    let mut ret = off;
    for i in off..off + len {
        if nums[i] >= a && nums[i] < b {
            nums[ret] = nums[i];
            ret += 1;
        }
    }
    ret
}

/// `Helpers.fastLineLen`.
pub fn fast_line_len(x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
    let dx = x1 - x0;
    let dy = y1 - y0;
    dx.abs() + dy.abs()
}

/// `Helpers.linelen`.
pub fn linelen(x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
    let dx = x1 - x0;
    let dy = y1 - y0;
    (dx * dx + dy * dy).sqrt()
}

/// `Helpers.fastQuadLen`.
pub fn fast_quad_len(x0: f64, y0: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let dx1 = x1 - x0;
    let dx2 = x2 - x1;
    let dy1 = y1 - y0;
    let dy2 = y2 - y1;
    dx1.abs() + dx2.abs() + dy1.abs() + dy2.abs()
}

/// `Helpers.fastCurvelen`.
#[allow(clippy::too_many_arguments)]
pub fn fast_curvelen(x0: f64, y0: f64, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) -> f64 {
    let dx1 = x1 - x0;
    let dx2 = x2 - x1;
    let dx3 = x3 - x2;
    let dy1 = y1 - y0;
    let dy2 = y2 - y1;
    let dy3 = y3 - y2;
    dx1.abs() + dx2.abs() + dx3.abs() + dy1.abs() + dy2.abs() + dy3.abs()
}

/// `Helpers.findSubdivPoints`.
pub fn find_subdiv_points(c: &mut Curve, pts: &[f64], ts: &mut [f64], type_: usize, w2: f64) -> usize {
    let x12 = pts[2] - pts[0];
    let y12 = pts[3] - pts[1];
    if y12 != 0.0 && x12 != 0.0 {
        let hypot = (x12 * x12 + y12 * y12).sqrt();
        let cos = x12 / hypot;
        let sin = y12 / hypot;
        let x1 = cos * pts[0] + sin * pts[1];
        let y1 = cos * pts[1] - sin * pts[0];
        let x2 = cos * pts[2] + sin * pts[3];
        let y2 = cos * pts[3] - sin * pts[2];
        let x3 = cos * pts[4] + sin * pts[5];
        let y3 = cos * pts[5] - sin * pts[4];

        match type_ {
            8 => {
                let x4 = cos * pts[6] + sin * pts[7];
                let y4 = cos * pts[7] - sin * pts[6];
                c.set8(x1, y1, x2, y2, x3, y3, x4, y4);
            }
            6 => c.set6(x1, y1, x2, y2, x3, y3),
            _ => {}
        }
    } else {
        c.set(pts, type_);
    }

    let mut ret = 0;
    ret += c.dx_roots(ts, ret);
    ret += c.dy_roots(ts, ret);
    if type_ == 8 {
        ret += c.inf_points(ts, ret);
    }
    ret += c.roots_of_roc_minus_w(ts, ret, w2, 0.0001);

    ret = filter_out_not_in_ab(ts, 0, ret, 0.0001, 0.9999);
    isort(ts, ret);
    ret
}

/// `Helpers.findClipPoints`.
pub fn find_clip_points(curve: &mut Curve, pts: &[f64], ts: &mut [f64], type_: usize, out_code_or: i32, clip_rect: &[f64; 4]) -> usize {
    curve.set(pts, type_);

    let mut ret = 0;
    if (out_code_or & OUTCODE_LEFT) != 0 {
        ret += curve.x_points(ts, ret, clip_rect[2]);
    }
    if (out_code_or & OUTCODE_RIGHT) != 0 {
        ret += curve.x_points(ts, ret, clip_rect[3]);
    }
    if (out_code_or & OUTCODE_TOP) != 0 {
        ret += curve.y_points(ts, ret, clip_rect[0]);
    }
    if (out_code_or & OUTCODE_BOTTOM) != 0 {
        ret += curve.y_points(ts, ret, clip_rect[1]);
    }
    isort(ts, ret);
    ret
}

/// `Helpers.subdivide(src, left, right, type)` with `right` aliasing `src` (as `Dasher.LengthIterator` calls it).
pub fn subdivide(src: &mut [f64; 8], left: &mut [f64; 8], type_: usize) {
    let s = *src;
    match type_ {
        8 => subdivide_cubic(&s, left, src),
        6 => subdivide_quad(&s, left, src),
        _ => panic!("Unsupported curve type"),
    }
}

/// `Helpers.isort`.
pub fn isort(a: &mut [f64], len: usize) {
    for i in 1..len {
        let ai = a[i];
        let mut j = i as isize - 1;
        while j >= 0 && a[j as usize] > ai {
            a[(j + 1) as usize] = a[j as usize];
            j -= 1;
        }
        a[(j + 1) as usize] = ai;
    }
}

/// `Helpers.subdivideCubic`.
pub fn subdivide_cubic(src: &[f64; 8], left: &mut [f64; 8], right: &mut [f64; 8]) {
    let mut x1 = src[0];
    let mut y1 = src[1];
    let mut cx1 = src[2];
    let mut cy1 = src[3];
    let mut cx2 = src[4];
    let mut cy2 = src[5];
    let mut x2 = src[6];
    let mut y2 = src[7];

    left[0] = x1;
    left[1] = y1;

    right[6] = x2;
    right[7] = y2;

    x1 = (x1 + cx1) / 2.0;
    y1 = (y1 + cy1) / 2.0;
    x2 = (x2 + cx2) / 2.0;
    y2 = (y2 + cy2) / 2.0;

    let mut cx = (cx1 + cx2) / 2.0;
    let mut cy = (cy1 + cy2) / 2.0;

    cx1 = (x1 + cx) / 2.0;
    cy1 = (y1 + cy) / 2.0;
    cx2 = (x2 + cx) / 2.0;
    cy2 = (y2 + cy) / 2.0;
    cx = (cx1 + cx2) / 2.0;
    cy = (cy1 + cy2) / 2.0;

    left[2] = x1;
    left[3] = y1;
    left[4] = cx1;
    left[5] = cy1;
    left[6] = cx;
    left[7] = cy;

    right[0] = cx;
    right[1] = cy;
    right[2] = cx2;
    right[3] = cy2;
    right[4] = x2;
    right[5] = y2;
}

/// `Helpers.subdivideCubicAt(t, src, srcoff, left, leftoff, rightoff)` with `src` and `left` the same array `pts`.
pub fn subdivide_cubic_at(t: f64, pts: &mut [f64], srcoff: usize, leftoff: usize, rightoff: usize) {
    let mut x1 = pts[srcoff];
    let mut y1 = pts[srcoff + 1];
    let mut cx1 = pts[srcoff + 2];
    let mut cy1 = pts[srcoff + 3];
    let mut cx2 = pts[srcoff + 4];
    let mut cy2 = pts[srcoff + 5];
    let mut x2 = pts[srcoff + 6];
    let mut y2 = pts[srcoff + 7];

    pts[leftoff] = x1;
    pts[leftoff + 1] = y1;

    pts[rightoff + 6] = x2;
    pts[rightoff + 7] = y2;

    x1 = x1 + t * (cx1 - x1);
    y1 = y1 + t * (cy1 - y1);
    x2 = cx2 + t * (x2 - cx2);
    y2 = cy2 + t * (y2 - cy2);

    let mut cx = cx1 + t * (cx2 - cx1);
    let mut cy = cy1 + t * (cy2 - cy1);

    cx1 = x1 + t * (cx - x1);
    cy1 = y1 + t * (cy - y1);
    cx2 = cx + t * (x2 - cx);
    cy2 = cy + t * (y2 - cy);
    cx = cx1 + t * (cx2 - cx1);
    cy = cy1 + t * (cy2 - cy1);

    pts[leftoff + 2] = x1;
    pts[leftoff + 3] = y1;
    pts[leftoff + 4] = cx1;
    pts[leftoff + 5] = cy1;
    pts[leftoff + 6] = cx;
    pts[leftoff + 7] = cy;

    pts[rightoff] = cx;
    pts[rightoff + 1] = cy;
    pts[rightoff + 2] = cx2;
    pts[rightoff + 3] = cy2;
    pts[rightoff + 4] = x2;
    pts[rightoff + 5] = y2;
}

/// `Helpers.subdivideQuad`.
pub fn subdivide_quad(src: &[f64; 8], left: &mut [f64; 8], right: &mut [f64; 8]) {
    let mut x1 = src[0];
    let mut y1 = src[1];
    let mut cx = src[2];
    let mut cy = src[3];
    let mut x2 = src[4];
    let mut y2 = src[5];

    left[0] = x1;
    left[1] = y1;

    right[4] = x2;
    right[5] = y2;

    x1 = (x1 + cx) / 2.0;
    y1 = (y1 + cy) / 2.0;
    x2 = (x2 + cx) / 2.0;
    y2 = (y2 + cy) / 2.0;
    cx = (x1 + x2) / 2.0;
    cy = (y1 + y2) / 2.0;

    left[2] = x1;
    left[3] = y1;
    left[4] = cx;
    left[5] = cy;

    right[0] = cx;
    right[1] = cy;
    right[2] = x2;
    right[3] = y2;
}

/// `Helpers.subdivideQuadAt(t, src, srcoff, left, leftoff, rightoff)` with `src` and `left` the same array `pts`.
pub fn subdivide_quad_at(t: f64, pts: &mut [f64], srcoff: usize, leftoff: usize, rightoff: usize) {
    let mut x1 = pts[srcoff];
    let mut y1 = pts[srcoff + 1];
    let mut cx = pts[srcoff + 2];
    let mut cy = pts[srcoff + 3];
    let mut x2 = pts[srcoff + 4];
    let mut y2 = pts[srcoff + 5];

    pts[leftoff] = x1;
    pts[leftoff + 1] = y1;

    pts[rightoff + 4] = x2;
    pts[rightoff + 5] = y2;

    x1 = x1 + t * (cx - x1);
    y1 = y1 + t * (cy - y1);
    x2 = cx + t * (x2 - cx);
    y2 = cy + t * (y2 - cy);
    cx = x1 + t * (x2 - x1);
    cy = y1 + t * (y2 - y1);

    pts[leftoff + 2] = x1;
    pts[leftoff + 3] = y1;
    pts[leftoff + 4] = cx;
    pts[leftoff + 5] = cy;

    pts[rightoff] = cx;
    pts[rightoff + 1] = cy;
    pts[rightoff + 2] = x2;
    pts[rightoff + 3] = y2;
}

/// `Helpers.subdivideLineAt(t, src, srcoff, left, leftoff, rightoff)` with `src` and `left` the same array `pts`.
pub fn subdivide_line_at(t: f64, pts: &mut [f64], srcoff: usize, leftoff: usize, rightoff: usize) {
    let mut x1 = pts[srcoff];
    let mut y1 = pts[srcoff + 1];
    let x2 = pts[srcoff + 2];
    let y2 = pts[srcoff + 3];

    pts[leftoff] = x1;
    pts[leftoff + 1] = y1;

    pts[rightoff + 2] = x2;
    pts[rightoff + 3] = y2;

    x1 = x1 + t * (x2 - x1);
    y1 = y1 + t * (y2 - y1);

    pts[leftoff + 2] = x1;
    pts[leftoff + 3] = y1;

    pts[rightoff] = x1;
    pts[rightoff + 1] = y1;
}

/// `Helpers.subdivideAt(t, src, srcoff, left, leftoff, type)` with `src` and `left` the same array `pts`.
pub fn subdivide_at(t: f64, pts: &mut [f64], srcoff: usize, leftoff: usize, type_: usize) {
    if type_ == 8 {
        subdivide_cubic_at(t, pts, srcoff, leftoff, leftoff + type_);
    } else if type_ == 4 {
        subdivide_line_at(t, pts, srcoff, leftoff, leftoff + type_);
    } else {
        subdivide_quad_at(t, pts, srcoff, leftoff, leftoff + type_);
    }
}

/// `Helpers.outcode`.
pub fn outcode(x: f64, y: f64, clip_rect: &[f64; 4]) -> i32 {
    let mut code = if y < clip_rect[0] {
        OUTCODE_TOP
    } else if y >= clip_rect[1] {
        OUTCODE_BOTTOM
    } else {
        0
    };

    if x < clip_rect[2] {
        code |= OUTCODE_LEFT;
    } else if x >= clip_rect[3] {
        code |= OUTCODE_RIGHT;
    }
    code
}

const TYPE_LINETO: u8 = 0;
const TYPE_QUADTO: u8 = 1;
const TYPE_CUBICTO: u8 = 2;

/// `Helpers.PolyStack`.
#[derive(Default)]
pub struct PolyStack {
    curves: Vec<f64>,
    curve_types: Vec<u8>,
}

impl PolyStack {
    pub fn new() -> PolyStack {
        PolyStack::default()
    }

    /// `PolyStack.dispose()`.
    pub fn dispose(&mut self) {
        self.curves.clear();
        self.curve_types.clear();
    }

    /// `PolyStack.pushCubic`.
    pub fn push_cubic(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.curve_types.push(TYPE_CUBICTO);
        self.curves.extend_from_slice(&[x2, y2, x1, y1, x0, y0]);
    }

    /// `PolyStack.pushQuad`.
    pub fn push_quad(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        self.curve_types.push(TYPE_QUADTO);
        self.curves.extend_from_slice(&[x1, y1, x0, y0]);
    }

    /// `PolyStack.pushLine`.
    pub fn push_line(&mut self, x: f64, y: f64) {
        self.curve_types.push(TYPE_LINETO);
        self.curves.extend_from_slice(&[x, y]);
    }

    /// `PolyStack.pullAll`.
    pub fn pull_all(&mut self, io: &mut dyn DPathConsumer2D) {
        let c = &self.curves;
        let mut e = 0;
        for &t in &self.curve_types {
            match t {
                TYPE_LINETO => {
                    io.line_to(c[e], c[e + 1]);
                    e += 2;
                }
                TYPE_CUBICTO => {
                    io.curve_to(c[e], c[e + 1], c[e + 2], c[e + 3], c[e + 4], c[e + 5]);
                    e += 6;
                }
                TYPE_QUADTO => {
                    io.quad_to(c[e], c[e + 1], c[e + 2], c[e + 3]);
                    e += 4;
                }
                _ => {}
            }
        }
        self.curves.clear();
        self.curve_types.clear();
    }

    /// `PolyStack.popAll`.
    pub fn pop_all(&mut self, io: &mut dyn DPathConsumer2D) {
        let c = &self.curves;
        let mut e = c.len();
        for &t in self.curve_types.iter().rev() {
            match t {
                TYPE_LINETO => {
                    e -= 2;
                    io.line_to(c[e], c[e + 1]);
                }
                TYPE_CUBICTO => {
                    e -= 6;
                    io.curve_to(c[e], c[e + 1], c[e + 2], c[e + 3], c[e + 4], c[e + 5]);
                }
                TYPE_QUADTO => {
                    e -= 4;
                    io.quad_to(c[e], c[e + 1], c[e + 2], c[e + 3]);
                }
                _ => {}
            }
        }
        self.curves.clear();
        self.curve_types.clear();
    }
}
