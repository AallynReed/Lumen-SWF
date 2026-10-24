//! Bezier edges, intersections and polynomials, and the parts of
//! Java2D's `Path2D` and `FlatteningPathIterator` the shape fixers use.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct P {
    pub x: f64,
    pub y: f64,
}

pub fn p(x: f64, y: f64) -> P {
    P { x, y }
}

impl P {
    /// The key `Point2D.hashCode`/`equals` agree on: exact bits.
    pub fn bits(&self) -> (u64, u64) {
        (self.x.to_bits(), self.y.to_bits())
    }

    pub fn distance(&self, o: &P) -> f64 {
        let dx = self.x - o.x;
        let dy = self.y - o.y;
        (dx * dx + dy * dy).sqrt()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct R {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl R {
    pub fn min_x(&self) -> f64 {
        self.x
    }
    pub fn min_y(&self) -> f64 {
        self.y
    }
    pub fn max_x(&self) -> f64 {
        self.x + self.w
    }
    pub fn max_y(&self) -> f64 {
        self.y + self.h
    }
    pub fn center_x(&self) -> f64 {
        self.x + self.w / 2.0
    }
    pub fn center_y(&self) -> f64 {
        self.y + self.h / 2.0
    }
    /// `Rectangle2D.contains(Rectangle2D)`.
    pub fn contains(&self, o: &R) -> bool {
        if self.w <= 0.0 || self.h <= 0.0 || o.w <= 0.0 || o.h <= 0.0 {
            return false;
        }
        o.x >= self.x && o.y >= self.y && o.x + o.w <= self.x + self.w && o.y + o.h <= self.y + self.h
    }
}

/// `Point2D.equals`.
fn peq(a: &P, b: &P) -> bool {
    a.x == b.x && a.y == b.y
}

fn bbox_of(points: &[P]) -> R {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, -f64::MAX, -f64::MAX);
    for p in points {
        if p.x < min_x {
            min_x = p.x;
        }
        if p.x > max_x {
            max_x = p.x;
        }
        if p.y < min_y {
            min_y = p.y;
        }
        if p.y > max_y {
            max_y = p.y;
        }
    }
    R { x: min_x, y: min_y, w: max_x - min_x, h: max_y - min_y }
}

fn eval_quad(p0: P, p1: P, p2: P, t: f64) -> P {
    let u = 1.0 - t;
    p(u * u * p0.x + 2.0 * u * t * p1.x + t * t * p2.x, u * u * p0.y + 2.0 * u * t * p1.y + t * t * p2.y)
}

/// `BezierEdge.quadraticBBox`.
pub fn quadratic_bbox(p0: P, p1: P, p2: P) -> R {
    let mut cand = vec![0.0, 1.0];
    let dx = p0.x - 2.0 * p1.x + p2.x;
    let dy = p0.y - 2.0 * p1.y + p2.y;
    const EPS: f64 = 1e-12;
    if dx.abs() > EPS {
        let tx = (p0.x - p1.x) / dx;
        if tx > 0.0 && tx < 1.0 {
            cand.push(tx);
        }
    }
    if dy.abs() > EPS {
        let ty = (p0.y - p1.y) / dy;
        if ty > 0.0 && ty < 1.0 {
            cand.push(ty);
        }
    }
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for t in cand {
        let pt = eval_quad(p0, p1, p2, t.clamp(0.0, 1.0));
        if pt.x < min_x {
            min_x = pt.x;
        }
        if pt.y < min_y {
            min_y = pt.y;
        }
        if pt.x > max_x {
            max_x = pt.x;
        }
        if pt.y > max_y {
            max_y = pt.y;
        }
    }
    R { x: min_x, y: min_y, w: max_x - min_x, h: max_y - min_y }
}

/// `BezierEdge`: a line (two points) or a quadratic curve (three).
#[derive(Debug, Clone)]
pub struct BezierEdge {
    pub points: Vec<P>,
    bbox: R,
    empty: bool,
}

impl PartialEq for BezierEdge {
    fn eq(&self, o: &Self) -> bool {
        self.key() == o.key()
    }
}

impl Eq for BezierEdge {}

impl std::hash::Hash for BezierEdge {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key().hash(state)
    }
}

impl BezierEdge {
    pub fn new(points: Vec<P>) -> BezierEdge {
        let mut b = BezierEdge { points, bbox: R::default(), empty: true };
        b.calc();
        b
    }

    pub fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> BezierEdge {
        BezierEdge::new(vec![p(x0, y0), p(x1, y1)])
    }

    pub fn quad(x0: f64, y0: f64, cx: f64, cy: f64, x1: f64, y1: f64) -> BezierEdge {
        BezierEdge::new(vec![p(x0, y0), p(cx, cy), p(x1, y1)])
    }

    /// The identity `equals` and `hashCode` use: the exact point bits.
    pub fn key(&self) -> Vec<(u64, u64)> {
        self.points.iter().map(P::bits).collect()
    }

    fn calc(&mut self) {
        self.bbox = if self.points.len() == 2 { bbox_of(&self.points) } else { quadratic_bbox(self.points[0], self.points[1], self.points[2]) };
        let b = self.points[0];
        self.empty = self.points[1..].iter().all(|q| peq(q, &b));
    }

    pub fn is_empty(&self) -> bool {
        self.empty
    }

    pub fn begin(&self) -> P {
        self.points[0]
    }

    pub fn end(&self) -> P {
        *self.points.last().unwrap()
    }

    pub fn set_begin(&mut self, q: P) {
        self.points[0] = q;
        self.calc();
    }

    pub fn set_end(&mut self, q: P) {
        let n = self.points.len();
        self.points[n - 1] = q;
        self.calc();
    }

    pub fn bbox(&self) -> R {
        self.bbox
    }

    pub fn reverse(&self) -> BezierEdge {
        let mut pts = self.points.clone();
        pts.reverse();
        BezierEdge::new(pts)
    }

    fn intersections(&self, b2: &BezierEdge) -> Vec<P> {
        if !rect_intersection(&self.bbox, &b2.bbox) {
            return Vec::new();
        }
        let a = &self.points;
        let b = &b2.points;
        match (a.len(), b.len()) {
            (2, 2) => intersect_line_line(a[0], a[1], b[0], b[1], true),
            (2, _) => intersect_bezier2_line(b[0], b[1], b[2], a[0], a[1]),
            (_, 2) => intersect_bezier2_line(a[0], a[1], a[2], b[0], b[1]),
            _ => intersect_bezier2_bezier2(a[0], a[1], a[2], b[0], b[1], b[2]),
        }
    }

    /// `intersects(b2, t1Ref, t2Ref, intPoints)`.
    pub fn intersects(&self, b2: &BezierEdge, t1: &mut Vec<f64>, t2: &mut Vec<f64>, pts: &mut Vec<P>) -> bool {
        let inter = self.intersections(b2);
        for q in &inter {
            let mid = |e: &BezierEdge| {
                let p1 = e.points[0];
                let p3 = e.end();
                let p2 = if e.points.len() == 3 { e.points[1] } else { p((p1.x + p3.x) / 2.0, (p1.y + p3.y) / 2.0) };
                (p1, p2, p3)
            };
            let (p1, p2, p3) = mid(self);
            t1.push(closest_point_to_bezier(*q, p1, p2, p3));
            let (p1, p2, p3) = mid(b2);
            t2.push(closest_point_to_bezier(*q, p1, p2, p3));
        }
        pts.extend_from_slice(&inter);
        !inter.is_empty()
    }

    /// `split(t, left, right)`.
    pub fn split_at(&self, t: f64) -> (BezierEdge, BezierEdge) {
        let (l, mut r) = subdivide(&self.points, t);
        r[0] = *l.last().unwrap();
        (BezierEdge::new(l), BezierEdge::new(r))
    }

    pub fn split(&self, ts: &[f64]) -> Vec<BezierEdge> {
        let mut result = Vec::new();
        let mut prev = 0.0;
        let mut remaining = self.clone();
        for &t in ts {
            let local = (t - prev) / (1.0 - prev);
            let (l, r) = remaining.split_at(local);
            result.push(l);
            remaining = r;
            prev = t;
        }
        result.push(remaining);
        result
    }

    pub fn round_n(&mut self, n: f64) {
        for q in &mut self.points {
            *q = p(jround(q.x * n) / n, jround(q.y * n) / n);
        }
        self.calc();
    }

    pub fn round_x(&mut self) {
        self.round_n(2.0);
    }

    /// `shrinkToLine`: a curve whose control point lies on its chord becomes a line.
    pub fn shrink_to_line(&mut self) {
        if self.points.len() == 3 {
            let a = self.points[0];
            let b = self.points[1];
            let c = self.points[2];
            let det = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
            if det == 0.0 {
                self.points.remove(1);
                self.calc();
            }
        }
    }
}

/// `Math.round` as a double.
pub fn jround(v: f64) -> f64 {
    (v + 0.5).floor()
}

pub fn rect_intersection(r1: &R, r2: &R) -> bool {
    let xmin = r1.x.max(r2.x);
    let xmax = (r1.x + r1.w).min(r2.x + r2.w);
    if xmax.total_cmp(&xmin).is_ge() {
        let ymin = r1.y.max(r2.y);
        let ymax = (r1.y + r1.h).min(r2.y + r2.h);
        if ymax.total_cmp(&ymin).is_ge() {
            return true;
        }
    }
    false
}

fn mul(a: P, s: f64) -> P {
    p(a.x * s, a.y * s)
}

fn add(a: P, b: P) -> P {
    p(a.x + b.x, a.y + b.y)
}

fn lerp(a: P, b: P, t: f64) -> P {
    let omt = 1.0 - t;
    p(a.x * omt + b.x * t, a.y * omt + b.y * t)
}

fn dot(a: P, b: P) -> f64 {
    a.x * b.x + a.y * b.y
}

fn contains(v: &[P], q: &P) -> bool {
    v.iter().any(|x| peq(x, q))
}

pub fn intersect_bezier2_bezier2(a1: P, a2: P, a3: P, b1: P, b2: P, b3: P) -> Vec<P> {
    let mut result = Vec::new();
    let c12 = add(a1, add(mul(a2, -2.0), a3));
    let c11 = add(mul(a1, -2.0), mul(a2, 2.0));
    let c10 = a1;
    let c22 = add(b1, add(mul(b2, -2.0), b3));
    let c21 = add(mul(b1, -2.0), mul(b2, 2.0));
    let c20 = b1;
    let a = c12.x * c11.y - c11.x * c12.y;
    let b = c22.x * c11.y - c11.x * c22.y;
    let c = c21.x * c11.y - c11.x * c21.y;
    let d = c11.x * (c10.y - c20.y) + c11.y * (-c10.x + c20.x);
    let e = c22.x * c12.y - c12.x * c22.y;
    let f = c21.x * c12.y - c12.x * c21.y;
    let g = c12.x * (c10.y - c20.y) + c12.y * (-c10.x + c20.x);
    let mut poly = Polynomial::new(&[-e * e, -2.0 * e * f, a * b - f * f - 2.0 * e * g, a * c - 2.0 * f * g, a * d - g * g]);
    let roots = poly.roots().unwrap_or_default();
    for s in roots {
        if (0.0..=1.0).contains(&s) {
            let mut xp = Polynomial::new(&[c12.x, c11.x, c10.x - c20.x - s * c21.x - s * s * c22.x]);
            xp.simplify_equals(1e-12);
            let x_roots = xp.roots().unwrap_or_default();
            let mut yp = Polynomial::new(&[c12.y, c11.y, c10.y - c20.y - s * c21.y - s * s * c22.y]);
            yp.simplify_equals(1e-12);
            let y_roots = yp.roots().unwrap_or_default();
            if !x_roots.is_empty() && !y_roots.is_empty() {
                'check: for &xr in &x_roots {
                    if (0.0..=1.0).contains(&xr) {
                        for &yr in &y_roots {
                            if (xr - yr).abs() < 1e-4 {
                                let res = add(mul(c22, s * s), add(mul(c21, s), c20));
                                if !contains(&result, &res) {
                                    result.push(res);
                                }
                                break 'check;
                            }
                        }
                    }
                }
            }
        }
    }
    if peq(&a1, &b1) && !contains(&result, &a1) {
        result.insert(0, a1);
    }
    if peq(&a3, &b3) && !contains(&result, &a3) {
        result.push(a3);
    }
    if peq(&a1, &b3) && !contains(&result, &a1) {
        result.insert(0, a1);
    }
    if peq(&a3, &b1) && !contains(&result, &a3) {
        result.insert(0, a3);
    }
    result
}

pub fn intersect_bezier2_line(p1: P, p2: P, p3: P, a1: P, a2: P) -> Vec<P> {
    let min = p(a1.x.min(a2.x), a1.y.min(a2.y));
    let max = p(a1.x.max(a2.x), a1.y.max(a2.y));
    let mut result = Vec::new();
    let c2 = add(p1, add(mul(p2, -2.0), p3));
    let c1 = add(mul(p1, -2.0), mul(p2, 2.0));
    let c0 = p1;
    let n = p(a1.y - a2.y, a2.x - a1.x);
    let cl = a1.x * a2.y - a2.x * a1.y;
    let roots = Polynomial::new(&[dot(n, c2), dot(n, c1), dot(n, c0) + cl]).roots().unwrap_or_default();
    for t in roots {
        if (0.0..=1.0).contains(&t) {
            let p4 = lerp(p1, p2, t);
            let p5 = lerp(p2, p3, t);
            let p6 = lerp(p4, p5, t);
            if a1.x == a2.x {
                if min.y <= p6.y && p6.y <= max.y {
                    result.push(p6);
                }
            } else if a1.y == a2.y {
                if min.x <= p6.x && p6.x <= max.x {
                    result.push(p6);
                }
            } else if min.x <= p6.x && p6.x <= max.x && min.y <= p6.y && p6.y <= max.y {
                result.push(p6);
            }
        }
    }
    if peq(&a1, &p1) && !contains(&result, &a1) {
        result.insert(0, a1);
    }
    if peq(&a2, &p3) && !contains(&result, &a2) {
        result.push(a2);
    }
    if peq(&a1, &p3) && !contains(&result, &a1) {
        result.insert(0, a1);
    }
    if peq(&a2, &p1) && !contains(&result, &a2) {
        result.insert(0, a2);
    }
    result
}

pub fn intersect_line_line(mut a1: P, mut a2: P, mut b1: P, mut b2: P, add_coincident: bool) -> Vec<P> {
    let mut result = Vec::new();
    let ua_t = (b2.x - b1.x) * (a1.y - b1.y) - (b2.y - b1.y) * (a1.x - b1.x);
    let ub_t = (a2.x - a1.x) * (a1.y - b1.y) - (a2.y - a1.y) * (a1.x - b1.x);
    let u_b = (b2.y - b1.y) * (a2.x - a1.x) - (b2.x - b1.x) * (a2.y - a1.y);
    if u_b != 0.0 {
        let ua = ua_t / u_b;
        let ub = ub_t / u_b;
        if (0.0..=1.0).contains(&ua) && (0.0..=1.0).contains(&ub) {
            result.push(p(a1.x + ua * (a2.x - a1.x), a1.y + ua * (a2.y - a1.y)));
        }
    } else if ua_t == 0.0 || ub_t == 0.0 {
        if !add_coincident {
            return result;
        }
        let (mut a1v, mut a2v, mut b1v, mut b2v) = if !(a1.x == b1.x && a2.x == b2.x && a1.x == a2.x) { (a1.x, a2.x, b1.x, b2.x) } else { (a1.y, a2.y, b1.y, b2.y) };
        if a1v > a2v {
            std::mem::swap(&mut a1v, &mut a2v);
        }
        if b1v > b2v {
            std::mem::swap(&mut b1v, &mut b2v);
        }
        if b1v < a1v {
            std::mem::swap(&mut a1, &mut b1);
            std::mem::swap(&mut a2, &mut b2);
            std::mem::swap(&mut a1v, &mut b1v);
            std::mem::swap(&mut a2v, &mut b2v);
        }
        if a2v < b1v || a2v == b1v {
            return result;
        }
        result.push(b1);
        result.push(a2);
    }
    result
}

/// `BezierUtils.subdivide` (de Casteljau).
pub fn subdivide(c: &[P], t: f64) -> (Vec<P>, Vec<P>) {
    let degree = c.len() - 1;
    let n = degree + 1;
    let mut pts: Vec<P> = c.to_vec();
    let t1 = 1.0 - t;
    for i in 1..=degree {
        for j in 0..=degree - i {
            let ij = i * n + j;
            let a = pts[(i - 1) * n + j];
            let b = pts[(i - 1) * n + j + 1];
            let v = p(t1 * a.x + t * b.x, t1 * a.y + t * b.y);
            while ij >= pts.len() {
                pts.push(p(0.0, 0.0));
            }
            pts[ij] = v;
        }
    }
    let left = (0..=degree).map(|j| pts[j * n]).collect();
    let right = (0..=degree).map(|j| pts[(degree - j) * n + j]).collect();
    (left, right)
}

const MAX_DEPTH: i32 = 64;
const Z_QUAD: [f64; 6] = [1.0, 2.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0, 2.0 / 3.0, 1.0];

fn bezier_point(t: f64, p0: P, p1: P, p2: P) -> P {
    p((1.0 - t) * (1.0 - t) * p0.x + 2.0 * (1.0 - t) * t * p1.x + t * t * p2.x, (1.0 - t) * (1.0 - t) * p0.y + 2.0 * (1.0 - t) * t * p1.y + t * t * p2.y)
}

/// `BezierUtils.closestPointToBezier`.
pub fn closest_point_to_bezier(q: P, p0: P, p1: P, p2: P) -> f64 {
    let d0 = ((p0.x - q.x) * (p0.x - q.x) + (p0.y - q.y) * (p0.y - q.y)).sqrt();
    let d1 = ((p2.x - q.x) * (p2.x - q.x) + (p2.y - q.y) * (p2.y - q.y)).sqrt();
    let n = 2;
    let w = to_bezier_form(q, &[p0, p1, p2]);
    let roots = find_roots(&w, 2 * n - 1, 0);
    let (mut t_min, mut d_min) = if d0 < d1 { (0.0, d0) } else { (1.0, d1) };
    for t in roots {
        if (0.0..=1.0).contains(&t) {
            let pt = bezier_point(t, p0, p1, p2);
            let d = ((pt.x - q.x) * (pt.x - q.x) + (pt.y - q.y) * (pt.y - q.y)).sqrt();
            if d < d_min {
                t_min = t;
                d_min = d;
            }
        }
    }
    t_min
}

fn to_bezier_form(q: P, v: &[P]) -> Vec<P> {
    let n = v.len() - 1;
    let degree = 2 * n - 1;
    let c: Vec<P> = v.iter().map(|x| p(x.x - q.x, x.y - q.y)).collect();
    let s = n as f64;
    let d: Vec<P> = (0..n).map(|i| p(s * (v[i + 1].x - v[i].x), s * (v[i + 1].y - v[i].y))).collect();
    let mut cd = Vec::new();
    for di in d.iter().take(n) {
        for cc in c.iter().take(n + 1) {
            cd.push(di.x * cc.x + di.y * cc.y);
        }
    }
    let d_inv = 1.0 / degree as f64;
    let mut w: Vec<P> = (0..=degree).map(|i| p(i as f64 * d_inv, 0.0)).collect();
    let m = n - 1;
    for k in 0..=n + m {
        let lb = k.saturating_sub(m);
        let ub = k.min(n);
        for i in lb..=ub {
            let j = k - i;
            let index = j * (n + 1) + i;
            w[i + j].y += cd[index] * Z_QUAD[index];
        }
    }
    w
}

fn crossing_count(v: &[P], degree: usize) -> i32 {
    let mut n = 0;
    let mut old = if v[0].y < 0.0 { -1 } else { 1 };
    for q in v.iter().take(degree + 1).skip(1) {
        let s = if q.y < 0.0 { -1 } else { 1 };
        if s != old {
            n += 1;
        }
        old = s;
    }
    n
}

fn is_control_polygon_linear(v: &[P], degree: usize) -> bool {
    let a = v[0].y - v[degree].y;
    let b = v[degree].x - v[0].x;
    let c = v[0].x * v[degree].y - v[degree].x * v[0].y;
    let ab = a * a + b * b;
    let mut dist = vec![0.0];
    for q in v.iter().take(degree).skip(1) {
        let mut d = a * q.x + b * q.y + c;
        if d > 0.0 {
            d = (d * d) / ab;
        }
        if d < 0.0 {
            d = -((d * d) / ab);
        }
        dist.push(d);
    }
    let (mut above, mut below) = (0.0f64, 0.0f64);
    for &d in dist.iter().skip(1) {
        if d < 0.0 {
            below = below.min(d);
        }
        if d > 0.0 {
            above = above.max(d);
        }
    }
    let (a1, b1, c1) = (0.0, 1.0, 0.0);
    let det = a1 * b - a * b1;
    let d_inv = 1.0 / det;
    let i1 = (b1 * (c + above) - b * c1) * d_inv;
    let i2 = (b1 * (c + below) - b * c1) * d_inv;
    let error = 0.5 * (i1.max(i2) - i1.min(i2));
    error < 2f64.powi(-MAX_DEPTH - 1)
}

fn find_roots(w: &[P], degree: usize, depth: i32) -> Vec<f64> {
    match crossing_count(w, degree) {
        0 => return Vec::new(),
        1 => {
            if depth >= MAX_DEPTH {
                let m = (2 * degree - 1).min(w.len() - 1);
                return vec![0.5 * (w[0].x + w[m].x)];
            }
            if is_control_polygon_linear(w, degree) {
                let xnm = w[degree].x - w[0].x;
                let ynm = w[degree].y - w[0].y;
                let det_inv = -1.0 / ynm;
                return vec![(xnm * w[0].y - ynm * w[0].x) * det_inv];
            }
        }
        _ => {}
    }
    let (l, r) = subdivide(w, 0.5);
    let mut t = find_roots(&l, degree, depth + 1);
    t.extend(find_roots(&r, degree, depth + 1));
    t
}

/// `Polynomial` (kld-polynomial), coefficients from the constant term up.
#[derive(Debug, Clone)]
pub struct Polynomial {
    coefs: Vec<f64>,
}

fn sign(x: f64) -> f64 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

struct Rb {
    min_x: f64,
    max_x: f64,
}

impl Polynomial {
    /// Coefficients from the highest power down, as the Java constructor takes them.
    pub fn new(c: &[f64]) -> Polynomial {
        Polynomial { coefs: c.iter().rev().copied().collect() }
    }

    fn degree(&self) -> i32 {
        self.coefs.len() as i32 - 1
    }

    fn eval(&self, x: f64) -> Option<f64> {
        if x.is_nan() {
            return None;
        }
        let mut r = 0.0;
        for c in self.coefs.iter().rev() {
            r = r * x + c;
        }
        Some(r)
    }

    pub fn simplify_equals(&mut self, tolerance: f64) {
        let scale = self.coefs.iter().fold(0.0f64, |s, c| s.max(c.abs()));
        let threshold = tolerance * scale;
        let mut i = self.degree();
        while i >= 0 {
            if self.coefs[i as usize].abs() <= threshold {
                self.coefs.pop();
                i -= 1;
            } else {
                break;
            }
        }
    }

    fn bounds_upper_real_fujiwara(&self) -> (f64, f64) {
        let mut a = self.coefs.clone();
        let n = a.len() - 1;
        let an = a[n];
        if an != 1.0 {
            a = self.coefs.iter().map(|c| c / an).collect();
        }
        let b: Vec<f64> = a.iter().enumerate().map(|(i, &v)| if i < n { (if i == 0 { v / 2.0 } else { v }).abs().powf(1.0 / (n - i) as f64) } else { v }).collect();
        let (mut max_pos, mut near_pos) = (0.0, 0.0);
        for i in 0..b.len() {
            if i < n && a[i] < 0.0 {
                if max_pos < b[i] {
                    near_pos = max_pos;
                    max_pos = b[i];
                } else if near_pos < b[i] {
                    near_pos = b[i];
                }
            }
        }
        let (mut max_neg, mut near_neg) = (0.0, 0.0);
        for i in 0..b.len() {
            if i < n && (if n % 2 == i % 2 { a[i] < 0.0 } else { a[i] > 0.0 }) {
                if max_neg < b[i] {
                    near_neg = max_neg;
                    max_neg = b[i];
                } else if near_neg < b[i] {
                    near_neg = b[i];
                }
            }
        }
        let _ = (near_pos, near_neg);
        (-2.0 * max_neg, 2.0 * max_pos)
    }

    fn bounds_lower_real_fujiwara(&self) -> (f64, f64) {
        let rev = Polynomial { coefs: self.coefs.iter().rev().copied().collect() };
        let r = rev.bounds_upper_real_fujiwara();
        (1.0 / r.0, 1.0 / r.1)
    }

    fn bounds(&self) -> Rb {
        let urb = self.bounds_upper_real_fujiwara();
        let mut rb = Rb { min_x: urb.0, max_x: urb.1 };
        if urb.0 == 0.0 && urb.1 == 0.0 {
            return rb;
        }
        if urb.0 == 0.0 {
            rb.min_x = self.bounds_lower_real_fujiwara().1;
        } else if urb.1 == 0.0 {
            rb.max_x = self.bounds_lower_real_fujiwara().0;
        }
        if rb.min_x > rb.max_x {
            rb.min_x = 0.0;
            rb.max_x = 0.0;
        }
        rb
    }

    fn zero_error_estimate(&self, max_abs_x: Option<f64>) -> Option<f64> {
        const ERRF: f64 = 1e-15;
        let max_abs_x = match max_abs_x {
            Some(m) => m,
            None => {
                let rb = self.bounds();
                rb.min_x.abs().max(rb.max_x.abs())
            }
        };
        if max_abs_x < 0.001 {
            return Some(2.0 * self.eval(ERRF)?.abs());
        }
        let n = self.coefs.len() - 1;
        let an = self.coefs[n];
        let mut m = 0.0;
        for (i, &v) in self.coefs.iter().enumerate() {
            let nm = v / an * max_abs_x.powf(i as f64);
            m = if nm > m { nm } else { m };
        }
        Some(10.0 * ERRF * m)
    }

    fn cubic_roots(&self) -> Option<Vec<f64>> {
        let mut r = Vec::new();
        if self.degree() == 3 {
            let c3 = self.coefs[3];
            let c2 = self.coefs[2] / c3;
            let c1 = self.coefs[1] / c3;
            let c0 = self.coefs[0] / c3;
            let a = (3.0 * c1 - c2 * c2) / 3.0;
            let b = (2.0 * c2 * c2 * c2 - 9.0 * c1 * c2 + 27.0 * c0) / 27.0;
            let offset = c2 / 3.0;
            let mut discrim = b * b / 4.0 + a * a * a / 27.0;
            let half_b = b / 2.0;
            let zero = self.zero_error_estimate(None)?;
            if discrim.abs() <= zero {
                discrim = 0.0;
            }
            if discrim > 0.0 {
                let e = discrim.sqrt();
                let mut tmp = -half_b + e;
                let mut root = if tmp >= 0.0 { tmp.powf(1.0 / 3.0) } else { -(-tmp).powf(1.0 / 3.0) };
                tmp = -half_b - e;
                if tmp >= 0.0 {
                    root += tmp.powf(1.0 / 3.0);
                } else {
                    root -= (-tmp).powf(1.0 / 3.0);
                }
                r.push(root - offset);
            } else if discrim < 0.0 {
                let distance = (-a / 3.0).sqrt();
                let angle = (-discrim).sqrt().atan2(-half_b) / 3.0;
                let cos = angle.cos();
                let sin = angle.sin();
                let sqrt3 = 3f64.sqrt();
                r.push(2.0 * distance * cos - offset);
                r.push(-distance * (cos + sqrt3 * sin) - offset);
                r.push(-distance * (cos - sqrt3 * sin) - offset);
            } else {
                let tmp = if half_b >= 0.0 { -half_b.powf(1.0 / 3.0) } else { (-half_b).powf(1.0 / 3.0) };
                r.push(2.0 * tmp - offset);
                r.push(-tmp - offset);
            }
        }
        Some(r)
    }

    fn derivative(&self) -> Polynomial {
        Polynomial { coefs: (1..self.coefs.len()).map(|i| i as f64 * self.coefs[i]).collect() }
    }

    /// `getRoots`; `None` where Java throws.
    pub fn roots(&mut self) -> Option<Vec<f64>> {
        self.simplify_equals(1e-12);
        match self.degree() {
            1 => {
                let a = self.coefs[1];
                Some(if a != 0.0 { vec![-self.coefs[0] / a] } else { Vec::new() })
            }
            2 => {
                let a = self.coefs[2];
                let b = self.coefs[1] / a;
                let c = self.coefs[0] / a;
                let d = b * b - 4.0 * c;
                Some(if d > 0.0 {
                    let e = d.sqrt();
                    vec![0.5 * (-b + e), 0.5 * (-b - e)]
                } else if d == 0.0 {
                    vec![0.5 * -b]
                } else {
                    Vec::new()
                })
            }
            3 => self.cubic_roots(),
            4 => self.quartic_roots(),
            _ => Some(Vec::new()),
        }
    }

    fn quartic_roots(&self) -> Option<Vec<f64>> {
        let mut results = Vec::new();
        let n = self.degree() as usize;
        let mut poly = self.clone();
        let lead = poly.coefs[n];
        for c in &mut poly.coefs {
            *c /= lead;
        }
        const ERRF: f64 = 1e-15;
        if poly.coefs[0].abs() < 10.0 * ERRF * poly.coefs[3].abs() {
            poly.coefs[0] = 0.0;
        }
        let poly_d = poly.derivative();
        let mut derrt = poly_d.clone().roots()?;
        derrt.sort_by(|a, b| if a.total_cmp(b).is_eq() { std::cmp::Ordering::Equal } else if a - b < 0.0 { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater });
        let nr = derrt.len() as i32 - 1;
        let rb = self.bounds();
        let maxabs = rb.min_x.abs().max(rb.max_x.abs());
        let zero = self.zero_error_estimate(Some(maxabs))?;
        let mut dery: Vec<f64> = Vec::new();
        for &d in &derrt {
            dery.push(poly.eval(d)?);
        }
        for d in &mut dery {
            if d.abs() < zero {
                *d = 0.0;
            }
        }
        let mut i: i32 = 0;
        let dx = (0.1 * (rb.max_x - rb.min_x) / n as f64).max(ERRF);
        let mut guesses = Vec::new();
        let mut minmax = Vec::new();
        if nr > -1 {
            if dery[0] != 0.0 {
                if sign(dery[0]).total_cmp(&sign(poly.eval(derrt[0] - dx)? - dery[0])).is_ne() {
                    guesses.push(derrt[0] - dx);
                    minmax.push((rb.min_x, derrt[0]));
                }
            } else {
                results.push(derrt[0]);
                results.push(derrt[0]);
                i += 1;
            }
            while i < nr {
                let iu = i as usize;
                if dery[iu + 1] == 0.0 {
                    results.push(derrt[iu + 1]);
                    results.push(derrt[iu + 1]);
                    i += 1;
                } else if sign(dery[iu]).total_cmp(&sign(dery[iu + 1])).is_ne() {
                    guesses.push((derrt[iu] + derrt[iu + 1]) / 2.0);
                    minmax.push((derrt[iu], derrt[iu + 1]));
                }
                i += 1;
            }
            let nu = nr as usize;
            if dery[nu] != 0.0 && sign(dery[nu]).total_cmp(&sign(poly.eval(derrt[nu] + dx)? - dery[nu])).is_ne() {
                guesses.push(derrt[nu] + dx);
                minmax.push((derrt[nu], rb.max_x));
            }
        }
        for (g, mm) in guesses.iter_mut().zip(&minmax) {
            *g = newton_secant_bisection(*g, &poly, &poly_d, 32, mm.0, mm.1)?;
        }
        results.extend(guesses);
        Some(results)
    }
}

fn newton_secant_bisection(x0: f64, f: &Polynomial, df: &Polynomial, max_iter: usize, mut min: f64, mut max: f64) -> Option<f64> {
    let mut x = x0;
    let mut prev_dfx = 0.0;
    let mut prev_corr = 0.0;
    let min_corr = 10f64.powf(-14.0);
    if min > max {
        return None;
    }
    let mut y_min = f.eval(min)?;
    let mut y_max = f.eval(max)?;
    if sign(y_min).total_cmp(&sign(y_max)).is_eq() {
        return None;
    }
    for _ in 0..max_iter {
        let mut dfx = df.eval(x)?;
        if dfx == 0.0 {
            if prev_dfx == 0.0 {
                return None;
            }
            dfx = prev_dfx;
        }
        prev_dfx = dfx;
        let y = f.eval(x)?;
        let mut corr = y / dfx;
        let mut x_new = x - corr;
        if corr.abs() <= min_corr * x.abs() || prev_corr == (x - corr) - x {
            break;
        }
        if sign(y).total_cmp(&sign(y_max)).is_eq() {
            max = x;
            y_max = y;
        } else if sign(y).total_cmp(&sign(y_min)).is_eq() {
            min = x;
            y_min = y;
        } else {
            x = x_new;
            break;
        }
        if x_new < min || x_new > max {
            if sign(y_min).total_cmp(&sign(y_max)).is_eq() {
                break;
            }
            let dy = y_max - y_min;
            let dxr = max - min;
            corr = if dy == 0.0 {
                x - (min + dxr * 0.5)
            } else if (dy / y_min.min(y_max)).abs() > 50.0 {
                x - (min + dxr * (0.5 + if y_min.abs() < y_max.abs() { -0.25 } else { 0.25 }))
            } else {
                x - (min - y_min / dy * dxr)
            };
            x_new = x - corr;
            if corr.abs() <= min_corr * x.abs() || prev_corr == (x - corr) - x {
                break;
            }
        }
        prev_corr = x - x_new;
        x = x_new;
    }
    Some(x)
}

/// One segment of a Java2D path.
#[derive(Debug, Clone, Copy)]
pub enum Seg {
    Move(f64, f64),
    Line(f64, f64),
    Quad(f64, f64, f64, f64),
    Close,
}

/// `GeneralPath` (`Path2D.Float`) with the even-odd rule: coordinates are stored as floats.
#[derive(Debug, Clone, Default)]
pub struct Path {
    pub segs: Vec<Seg>,
}

fn f(v: f64) -> f64 {
    v as f32 as f64
}

impl Path {
    pub fn move_to(&mut self, x: f64, y: f64) {
        if let Some(Seg::Move(..)) = self.segs.last() {
            self.segs.pop();
        }
        self.segs.push(Seg::Move(f(x), f(y)));
    }
    pub fn line_to(&mut self, x: f64, y: f64) {
        self.segs.push(Seg::Line(f(x), f(y)));
    }
    pub fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        self.segs.push(Seg::Quad(f(cx), f(cy), f(x), f(y)));
    }
    pub fn close(&mut self) {
        if !matches!(self.segs.last(), Some(Seg::Close)) {
            self.segs.push(Seg::Close);
        }
    }

    /// JDK 17 `Path2D.Float.getBounds2D`: over every coordinate, control points included.
    pub fn bounds(&self) -> R {
        let mut pts: Vec<(f32, f32)> = Vec::new();
        for s in &self.segs {
            match *s {
                Seg::Move(x, y) | Seg::Line(x, y) => pts.push((x as f32, y as f32)),
                Seg::Quad(cx, cy, x, y) => {
                    pts.push((cx as f32, cy as f32));
                    pts.push((x as f32, y as f32));
                }
                Seg::Close => {}
            }
        }
        let Some(&(lx, ly)) = pts.last() else { return R::default() };
        let (mut x1, mut y1, mut x2, mut y2) = (lx, ly, lx, ly);
        for &(x, y) in pts.iter().rev().skip(1) {
            if x < x1 {
                x1 = x;
            }
            if y < y1 {
                y1 = y;
            }
            if x > x2 {
                x2 = x;
            }
            if y > y2 {
                y2 = y;
            }
        }
        R { x: x1 as f64, y: y1 as f64, w: (x2 - x1) as f64, h: (y2 - y1) as f64 }
    }

    /// `Path2D.contains(x, y)` with even-odd winding (`Path2D.Float.pointCrossings`).
    pub fn contains(&self, px: f64, py: f64) -> bool {
        if (px * 0.0 + py * 0.0) != 0.0 || self.segs.len() < 2 {
            return false;
        }
        let (mut mx, mut my) = match self.segs[0] {
            Seg::Move(x, y) | Seg::Line(x, y) => (x, y),
            _ => (0.0, 0.0),
        };
        let (mut cx, mut cy) = (mx, my);
        let mut crossings = 0i32;
        for s in &self.segs[1..] {
            match *s {
                Seg::Move(x, y) => {
                    if cy != my {
                        crossings += line_crossings(px, py, cx, cy, mx, my);
                    }
                    mx = x;
                    my = y;
                    cx = x;
                    cy = y;
                }
                Seg::Line(x, y) => {
                    crossings += line_crossings(px, py, cx, cy, x, y);
                    cx = x;
                    cy = y;
                }
                Seg::Quad(qx, qy, x, y) => {
                    crossings += quad_crossings(px, py, cx, cy, qx, qy, x, y, 0);
                    cx = x;
                    cy = y;
                }
                Seg::Close => {
                    if cy != my {
                        crossings += line_crossings(px, py, cx, cy, mx, my);
                    }
                    cx = mx;
                    cy = my;
                }
            }
        }
        if cy != my {
            crossings += line_crossings(px, py, cx, cy, mx, my);
        }
        crossings & 1 != 0
    }

    /// `FlatteningPathIterator(path, flatness, limit)` as line segments.
    pub fn flatten(&self, flatness: f64, limit: u32) -> Vec<Seg> {
        let mut out = Vec::new();
        let (mut cx, mut cy) = (0.0, 0.0);
        for s in &self.segs {
            match *s {
                Seg::Move(x, y) => {
                    out.push(Seg::Move(x, y));
                    cx = x;
                    cy = y;
                }
                Seg::Line(x, y) => {
                    out.push(Seg::Line(x, y));
                    cx = x;
                    cy = y;
                }
                Seg::Quad(qx, qy, x, y) => {
                    flatten_quad(&mut out, [cx, cy, qx, qy, x, y], flatness * flatness, limit);
                    cx = x;
                    cy = y;
                }
                Seg::Close => out.push(Seg::Close),
            }
        }
        out
    }
}

fn line_crossings(px: f64, py: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> i32 {
    if py < y0 && py < y1 {
        return 0;
    }
    if py >= y0 && py >= y1 {
        return 0;
    }
    if px >= x0 && px >= x1 {
        return 0;
    }
    if px < x0 && px < x1 {
        return if y0 < y1 { 1 } else { -1 };
    }
    let xint = x0 + (py - y0) * (x1 - x0) / (y1 - y0);
    if px >= xint {
        return 0;
    }
    if y0 < y1 { 1 } else { -1 }
}

#[allow(clippy::too_many_arguments)]
fn quad_crossings(px: f64, py: f64, x0: f64, y0: f64, xc: f64, yc: f64, x1: f64, y1: f64, level: i32) -> i32 {
    if py < y0 && py < yc && py < y1 {
        return 0;
    }
    if py >= y0 && py >= yc && py >= y1 {
        return 0;
    }
    if px >= x0 && px >= xc && px >= x1 {
        return 0;
    }
    if px < x0 && px < xc && px < x1 {
        if py >= y0 {
            if py < y1 {
                return 1;
            }
        } else if py >= y1 {
            return -1;
        }
        return 0;
    }
    if level > 52 {
        return line_crossings(px, py, x0, y0, x1, y1);
    }
    let x0c = (x0 + xc) / 2.0;
    let y0c = (y0 + yc) / 2.0;
    let xc1 = (xc + x1) / 2.0;
    let yc1 = (yc + y1) / 2.0;
    let xm = (x0c + xc1) / 2.0;
    let ym = (y0c + yc1) / 2.0;
    if xm.is_nan() || ym.is_nan() {
        return 0;
    }
    quad_crossings(px, py, x0, y0, x0c, y0c, xm, ym, level + 1) + quad_crossings(px, py, xm, ym, xc1, yc1, x1, y1, level + 1)
}

/// `QuadCurve2D.getFlatnessSq` of the control point against the chord.
fn quad_flatness_sq(c: &[f64; 6]) -> f64 {
    pt_seg_dist_sq(c[0], c[1], c[4], c[5], c[2], c[3])
}

fn pt_seg_dist_sq(x1: f64, y1: f64, x2: f64, y2: f64, px: f64, py: f64) -> f64 {
    let x2 = x2 - x1;
    let y2 = y2 - y1;
    let mut px = px - x1;
    let mut py = py - y1;
    let mut dot = px * x2 + py * y2;
    let proj;
    if dot <= 0.0 {
        proj = 0.0;
    } else {
        px = x2 - px;
        py = y2 - py;
        dot = px * x2 + py * y2;
        proj = if dot <= 0.0 { 0.0 } else { dot * dot / (x2 * x2 + y2 * y2) };
    }
    let len = px * px + py * py - proj;
    if len < 0.0 { 0.0 } else { len }
}

fn flatten_quad(out: &mut Vec<Seg>, c: [f64; 6], flat_sq: f64, limit: u32) {
    if limit == 0 || quad_flatness_sq(&c) < flat_sq {
        out.push(Seg::Line(c[4], c[5]));
        return;
    }
    let x1 = c[0];
    let y1 = c[1];
    let mut ctrlx = c[2];
    let mut ctrly = c[3];
    let x2 = c[4];
    let y2 = c[5];
    let ctrlx1 = (x1 + ctrlx) / 2.0;
    let ctrly1 = (y1 + ctrly) / 2.0;
    let ctrlx2 = (x2 + ctrlx) / 2.0;
    let ctrly2 = (y2 + ctrly) / 2.0;
    ctrlx = (ctrlx1 + ctrlx2) / 2.0;
    ctrly = (ctrly1 + ctrly2) / 2.0;
    flatten_quad(out, [x1, y1, ctrlx1, ctrly1, ctrlx, ctrly], flat_sq, limit - 1);
    flatten_quad(out, [ctrlx, ctrly, ctrlx2, ctrly2, x2, y2], flat_sq, limit - 1);
}
