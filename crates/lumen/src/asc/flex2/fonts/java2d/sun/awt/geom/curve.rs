//! Port of JDK 17 `sun.awt.geom.Curve`: the monotonic-in-y curve segments `Area` keeps, as one enum over `Order0`
//! to `Order3`.

use std::sync::Arc;

use super::math;
use super::order0::Order0;
use super::order1::Order1;
use super::order2::Order2;
use super::order3::Order3;

pub const INCREASING: i32 = 1;
pub const DECREASING: i32 = -1;
pub const TMIN: f64 = 1E-3;

/// `Curve`.
#[derive(Clone, Debug)]
pub enum Curve {
    Order0(Order0),
    Order1(Order1),
    Order2(Order2),
    Order3(Order3),
}

/// `Rectangle2D.Double`, as `Curve.enlarge` grows it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn set_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        *self = Rect { x, y, w, h };
    }

    /// `Rectangle2D.add(double, double)`.
    pub fn add(&mut self, newx: f64, newy: f64) {
        let x1 = math::min(self.x, newx);
        let x2 = math::max(self.x + self.w, newx);
        let y1 = math::min(self.y, newy);
        let y2 = math::max(self.y + self.h, newy);
        self.set_rect(x1, y1, x2 - x1, y2 - y1);
    }
}

/// `insertMove`.
pub fn insert_move(curves: &mut Vec<Arc<Curve>>, x: f64, y: f64) {
    curves.push(Arc::new(Curve::Order0(Order0::new(x, y))));
}

/// `insertLine`.
pub fn insert_line(curves: &mut Vec<Arc<Curve>>, x0: f64, y0: f64, x1: f64, y1: f64) {
    if y0 < y1 {
        curves.push(Arc::new(Curve::Order1(Order1::new(x0, y0, x1, y1, INCREASING))));
    } else if y0 > y1 {
        curves.push(Arc::new(Curve::Order1(Order1::new(x1, y1, x0, y0, DECREASING))));
    }
}

/// `insertQuad`.
pub fn insert_quad(curves: &mut Vec<Arc<Curve>>, x0: f64, y0: f64, coords: &mut [f64; 23]) {
    let y1 = coords[3];
    if y0 > y1 {
        let (cx, cy, x1) = (coords[0], coords[1], coords[2]);
        Order2::insert(curves, coords, x1, y1, cx, cy, x0, y0, DECREASING);
    } else if y0 == y1 && y0 == coords[1] {
    } else {
        let (cx, cy, x1) = (coords[0], coords[1], coords[2]);
        Order2::insert(curves, coords, x0, y0, cx, cy, x1, y1, INCREASING);
    }
}

/// `insertCubic`.
pub fn insert_cubic(curves: &mut Vec<Arc<Curve>>, x0: f64, y0: f64, coords: &mut [f64; 23]) {
    let y1 = coords[5];
    let (cx0, cy0, cx1, cy1, x1) = (coords[0], coords[1], coords[2], coords[3], coords[4]);
    if y0 > y1 {
        Order3::insert(curves, coords, x1, y1, cx1, cy1, cx0, cy0, x0, y0, DECREASING);
    } else if y0 == y1 && y0 == coords[1] && y0 == coords[3] {
    } else {
        Order3::insert(curves, coords, x0, y0, cx0, cy0, cx1, cy1, x1, y1, INCREASING);
    }
}

/// `orderof`.
pub fn orderof(x1: f64, x2: f64) -> i32 {
    if x1 < x2 {
        return -1;
    }
    if x1 > x2 {
        return 1;
    }
    0
}

macro_rules! dispatch {
    ($self:ident, $c:ident => $e:expr) => {
        match $self {
            Curve::Order0($c) => $e,
            Curve::Order1($c) => $e,
            Curve::Order2($c) => $e,
            Curve::Order3($c) => $e,
        }
    };
}

impl Curve {
    pub fn get_direction(&self) -> i32 {
        dispatch!(self, c => c.direction)
    }

    /// `getWithDirection`.
    pub fn get_with_direction(self: &Arc<Self>, direction: i32) -> Arc<Curve> {
        if self.get_direction() == direction {
            self.clone()
        } else {
            self.get_reversed_curve()
        }
    }

    pub fn get_order(&self) -> i32 {
        match self {
            Curve::Order0(_) => 0,
            Curve::Order1(_) => 1,
            Curve::Order2(_) => 2,
            Curve::Order3(_) => 3,
        }
    }

    pub fn get_x_top(&self) -> f64 {
        dispatch!(self, c => c.get_x_top())
    }

    pub fn get_y_top(&self) -> f64 {
        dispatch!(self, c => c.get_y_top())
    }

    pub fn get_x_bot(&self) -> f64 {
        dispatch!(self, c => c.get_x_bot())
    }

    pub fn get_y_bot(&self) -> f64 {
        dispatch!(self, c => c.get_y_bot())
    }

    pub fn get_x_min(&self) -> f64 {
        dispatch!(self, c => c.get_x_min())
    }

    pub fn get_x_max(&self) -> f64 {
        dispatch!(self, c => c.get_x_max())
    }

    pub fn get_x0(&self) -> f64 {
        dispatch!(self, c => c.get_x0())
    }

    pub fn get_y0(&self) -> f64 {
        dispatch!(self, c => c.get_y0())
    }

    pub fn get_x1(&self) -> f64 {
        dispatch!(self, c => c.get_x1())
    }

    pub fn get_y1(&self) -> f64 {
        dispatch!(self, c => c.get_y1())
    }

    /// `XforY`.
    pub fn x_for_y(&self, y: f64) -> f64 {
        dispatch!(self, c => c.x_for_y(y))
    }

    /// `TforY`.
    pub fn t_for_y(&self, y: f64) -> f64 {
        dispatch!(self, c => c.t_for_y(y))
    }

    /// `XforT`.
    pub fn x_for_t(&self, t: f64) -> f64 {
        dispatch!(self, c => c.x_for_t(t))
    }

    /// `YforT`.
    pub fn y_for_t(&self, t: f64) -> f64 {
        dispatch!(self, c => c.y_for_t(t))
    }

    /// `dYforT`.
    pub fn d_y_for_t(&self, t: f64, deriv: i32) -> f64 {
        dispatch!(self, c => c.d_y_for_t(t, deriv))
    }

    /// `nextVertical`.
    pub fn next_vertical(&self, t0: f64, t1: f64) -> f64 {
        dispatch!(self, c => c.next_vertical(t0, t1))
    }

    pub fn enlarge(&self, r: &mut Rect) {
        dispatch!(self, c => c.enlarge(r))
    }

    /// `getSubCurve(ystart, yend, dir)`.
    pub fn get_sub_curve(self: &Arc<Self>, ystart: f64, yend: f64, dir: i32) -> Arc<Curve> {
        match &**self {
            Curve::Order0(_) => self.clone(),
            Curve::Order1(c) => c.get_sub_curve(self, ystart, yend, dir),
            Curve::Order2(c) => c.get_sub_curve(self, ystart, yend, dir),
            Curve::Order3(c) => c.get_sub_curve(self, ystart, yend, dir),
        }
    }

    pub fn get_reversed_curve(self: &Arc<Self>) -> Arc<Curve> {
        match &**self {
            Curve::Order0(_) => self.clone(),
            Curve::Order1(c) => Arc::new(Curve::Order1(c.get_reversed_curve())),
            Curve::Order2(c) => Arc::new(Curve::Order2(c.get_reversed_curve())),
            Curve::Order3(c) => Arc::new(Curve::Order3(c.get_reversed_curve())),
        }
    }

    /// `getSegment(double[])`: the segment type, with the coordinates it writes.
    pub fn get_segment(&self, coords: &mut [f64; 6]) -> i32 {
        dispatch!(self, c => c.get_segment(coords))
    }

    /// `compareTo(Curve, double[])`, with `Order1`'s override for two lines.
    pub fn compare_to(&self, that: &Curve, yrange: &mut [f64; 2]) -> i32 {
        if let (Curve::Order1(a), Curve::Order1(b)) = (self, that) {
            return a.compare_to(b, yrange);
        }
        self.compare_to_curve(that, yrange)
    }

    /// `Curve.compareTo`.
    fn compare_to_curve(&self, that: &Curve, yrange: &mut [f64; 2]) -> i32 {
        let y0 = yrange[0];
        let mut y1 = yrange[1];
        y1 = math::min(math::min(y1, self.get_y_bot()), that.get_y_bot());
        if y1 <= yrange[0] {
            panic!("backstepping from {} to {}", yrange[0], y1);
        }
        yrange[1] = y1;
        if self.get_x_max() <= that.get_x_min() {
            if self.get_x_min() == that.get_x_max() {
                return 0;
            }
            return -1;
        }
        if self.get_x_min() >= that.get_x_max() {
            return 1;
        }
        let mut s0 = self.t_for_y(y0);
        let mut ys0 = self.y_for_t(s0);
        if ys0 < y0 {
            s0 = self.refine_t_for_y(s0, ys0, y0);
            ys0 = self.y_for_t(s0);
        }
        let mut s1 = self.t_for_y(y1);
        if self.y_for_t(s1) < y0 {
            s1 = self.refine_t_for_y(s1, self.y_for_t(s1), y0);
        }
        let mut t0 = that.t_for_y(y0);
        let mut yt0 = that.y_for_t(t0);
        if yt0 < y0 {
            t0 = that.refine_t_for_y(t0, yt0, y0);
            yt0 = that.y_for_t(t0);
        }
        let mut t1 = that.t_for_y(y1);
        if that.y_for_t(t1) < y0 {
            t1 = that.refine_t_for_y(t1, that.y_for_t(t1), y0);
        }
        let mut xs0 = self.x_for_t(s0);
        let mut xt0 = that.x_for_t(t0);
        let scale = math::max(y0.abs(), y1.abs());
        let ymin = math::max(scale * 1E-14, 1E-300);
        if self.fairly_close(xs0, xt0) {
            let mut bump = ymin;
            let maxbump = math::min(ymin * 1E13, (y1 - y0) * 0.1);
            let mut y = y0 + bump;
            if !y1.is_finite() {
                return 0;
            }
            while y <= y1 {
                if self.fairly_close(self.x_for_y(y), that.x_for_y(y)) {
                    bump *= 2.0;
                    if bump > maxbump {
                        bump = maxbump;
                    }
                } else {
                    y -= bump;
                    loop {
                        bump /= 2.0;
                        let newy = y + bump;
                        if newy <= y {
                            break;
                        }
                        if self.fairly_close(self.x_for_y(newy), that.x_for_y(newy)) {
                            y = newy;
                        }
                    }
                    break;
                }
                y += bump;
            }
            if y > y0 {
                if y < y1 {
                    yrange[1] = y;
                }
                return 0;
            }
        }
        while s0 < s1 && t0 < t1 {
            let sh = self.next_vertical(s0, s1);
            let xsh = self.x_for_t(sh);
            let ysh = self.y_for_t(sh);
            let th = that.next_vertical(t0, t1);
            let xth = that.x_for_t(th);
            let yth = that.y_for_t(th);
            match self.find_intersect(that, yrange, ymin, 0, 0, s0, xs0, ys0, sh, xsh, ysh, t0, xt0, yt0, th, xth, yth) {
                Ok(true) => break,
                Ok(false) => {}
                Err(_) => return 0,
            }
            if ysh < yth {
                if ysh > yrange[0] {
                    if ysh < yrange[1] {
                        yrange[1] = ysh;
                    }
                    break;
                }
                s0 = sh;
                xs0 = xsh;
                ys0 = ysh;
            } else {
                if yth > yrange[0] {
                    if yth < yrange[1] {
                        yrange[1] = yth;
                    }
                    break;
                }
                t0 = th;
                xt0 = xth;
                yt0 = yth;
            }
        }
        let ymid = (yrange[0] + yrange[1]) / 2.0;
        orderof(self.x_for_y(ymid), that.x_for_y(ymid))
    }

    /// `findIntersect`; `Err` is the `InternalError` `compareTo` catches.
    #[allow(clippy::too_many_arguments)]
    fn find_intersect(
        &self,
        that: &Curve,
        yrange: &mut [f64; 2],
        ymin: f64,
        slevel: i32,
        tlevel: i32,
        s0: f64,
        xs0: f64,
        ys0: f64,
        s1: f64,
        xs1: f64,
        ys1: f64,
        t0: f64,
        xt0: f64,
        yt0: f64,
        t1: f64,
        xt1: f64,
        yt1: f64,
    ) -> Result<bool, &'static str> {
        if ys0 > yt1 || yt0 > ys1 {
            return Ok(false);
        }
        if math::min(xs0, xs1) > math::max(xt0, xt1) || math::max(xs0, xs1) < math::min(xt0, xt1) {
            return Ok(false);
        }
        if s1 - s0 > TMIN {
            let s = (s0 + s1) / 2.0;
            let xs = self.x_for_t(s);
            let ys = self.y_for_t(s);
            if s == s0 || s == s1 {
                return Err("no s progress!");
            }
            if t1 - t0 > TMIN {
                let t = (t0 + t1) / 2.0;
                let xt = that.x_for_t(t);
                let yt = that.y_for_t(t);
                if t == t0 || t == t1 {
                    return Err("no t progress!");
                }
                if ys >= yt0 && yt >= ys0 && self.find_intersect(that, yrange, ymin, slevel + 1, tlevel + 1, s0, xs0, ys0, s, xs, ys, t0, xt0, yt0, t, xt, yt)? {
                    return Ok(true);
                }
                if ys >= yt && self.find_intersect(that, yrange, ymin, slevel + 1, tlevel + 1, s0, xs0, ys0, s, xs, ys, t, xt, yt, t1, xt1, yt1)? {
                    return Ok(true);
                }
                if yt >= ys && self.find_intersect(that, yrange, ymin, slevel + 1, tlevel + 1, s, xs, ys, s1, xs1, ys1, t0, xt0, yt0, t, xt, yt)? {
                    return Ok(true);
                }
                if ys1 >= yt && yt1 >= ys && self.find_intersect(that, yrange, ymin, slevel + 1, tlevel + 1, s, xs, ys, s1, xs1, ys1, t, xt, yt, t1, xt1, yt1)? {
                    return Ok(true);
                }
            } else {
                if ys >= yt0 && self.find_intersect(that, yrange, ymin, slevel + 1, tlevel, s0, xs0, ys0, s, xs, ys, t0, xt0, yt0, t1, xt1, yt1)? {
                    return Ok(true);
                }
                if yt1 >= ys && self.find_intersect(that, yrange, ymin, slevel + 1, tlevel, s, xs, ys, s1, xs1, ys1, t0, xt0, yt0, t1, xt1, yt1)? {
                    return Ok(true);
                }
            }
        } else if t1 - t0 > TMIN {
            let t = (t0 + t1) / 2.0;
            let xt = that.x_for_t(t);
            let yt = that.y_for_t(t);
            if t == t0 || t == t1 {
                return Err("no t progress!");
            }
            if yt >= ys0 && self.find_intersect(that, yrange, ymin, slevel, tlevel + 1, s0, xs0, ys0, s1, xs1, ys1, t0, xt0, yt0, t, xt, yt)? {
                return Ok(true);
            }
            if ys1 >= yt && self.find_intersect(that, yrange, ymin, slevel, tlevel + 1, s0, xs0, ys0, s1, xs1, ys1, t, xt, yt, t1, xt1, yt1)? {
                return Ok(true);
            }
        } else {
            let xlk = xs1 - xs0;
            let ylk = ys1 - ys0;
            let xnm = xt1 - xt0;
            let ynm = yt1 - yt0;
            let xmk = xt0 - xs0;
            let ymk = yt0 - ys0;
            let det = xnm * ylk - ynm * xlk;
            if det != 0.0 {
                let detinv = 1.0 / det;
                let mut s = (xnm * ymk - ynm * xmk) * detinv;
                let mut t = (xlk * ymk - ylk * xmk) * detinv;
                if s >= 0.0 && s <= 1.0 && t >= 0.0 && t <= 1.0 {
                    s = s0 + s * (s1 - s0);
                    t = t0 + t * (t1 - t0);
                    let y = (self.y_for_t(s) + that.y_for_t(t)) / 2.0;
                    if y <= yrange[1] && y > yrange[0] {
                        yrange[1] = y;
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    /// `refineTforY`.
    pub fn refine_t_for_y(&self, mut t0: f64, _yt0: f64, y0: f64) -> f64 {
        let mut t1 = 1.0;
        loop {
            let th = (t0 + t1) / 2.0;
            if th == t0 || th == t1 {
                return t1;
            }
            let y = self.y_for_t(th);
            if y < y0 {
                t0 = th;
            } else if y > y0 {
                t1 = th;
            } else {
                return t1;
            }
        }
    }

    /// `fairlyClose`.
    pub fn fairly_close(&self, v1: f64, v2: f64) -> bool {
        (v1 - v2).abs() < math::max(v1.abs(), v2.abs()) * 1E-8
    }
}
