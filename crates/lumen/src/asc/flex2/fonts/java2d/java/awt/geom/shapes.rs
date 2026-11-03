//! Ports of the JDK 17 `java.awt.geom` shapes SVG rendering reaches: `Rectangle2D`, `RoundRectangle2D`, `Ellipse2D`,
//! `Line2D`, `Arc2D`, `Path2D` (`GeneralPath`), their `PathIterator`s and `getBounds2D`.

use super::affine_transform::AffineTransform;

pub const SEG_MOVETO: i32 = 0;
pub const SEG_LINETO: i32 = 1;
pub const SEG_QUADTO: i32 = 2;
pub const SEG_CUBICTO: i32 = 3;
pub const SEG_CLOSE: i32 = 4;
pub const WIND_EVEN_ODD: i32 = 0;
pub const WIND_NON_ZERO: i32 = 1;

/// One `PathIterator.currentSegment(double[])` result.
pub type Seg = (i32, [f64; 6]);

pub fn coords_for(t: i32) -> usize {
    match t {
        SEG_MOVETO | SEG_LINETO => 2,
        SEG_QUADTO => 4,
        SEG_CUBICTO => 6,
        _ => 0,
    }
}

/// `Rectangle2D.Float` (`is_float`) or `Rectangle2D.Double`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rectangle2D {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub is_float: bool,
}

impl Rectangle2D {
    pub fn float(x: f32, y: f32, w: f32, h: f32) -> Rectangle2D {
        Rectangle2D { x: x as f64, y: y as f64, w: w as f64, h: h as f64, is_float: true }
    }

    pub fn double(x: f64, y: f64, w: f64, h: f64) -> Rectangle2D {
        Rectangle2D { x, y, w, h, is_float: false }
    }

    /// `setRect(double...)`, which rounds to float for a `Rectangle2D.Float`.
    pub fn set_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        if self.is_float {
            *self = Rectangle2D::float(x as f32, y as f32, w as f32, h as f32);
        } else {
            *self = Rectangle2D::double(x, y, w, h);
        }
    }

    pub fn get_min_x(&self) -> f64 {
        self.x
    }
    pub fn get_min_y(&self) -> f64 {
        self.y
    }
    pub fn get_max_x(&self) -> f64 {
        self.x + self.w
    }
    pub fn get_max_y(&self) -> f64 {
        self.y + self.h
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    pub fn intersects(&self, x: f64, y: f64, w: f64, h: f64) -> bool {
        if self.is_empty() || w <= 0.0 || h <= 0.0 {
            return false;
        }
        let x0 = self.x;
        let y0 = self.y;
        x + w > x0 && y + h > y0 && x < x0 + self.w && y < y0 + self.h
    }

    /// `createUnion(r)`: of this rectangle's own class.
    pub fn create_union(&self, r: &Rectangle2D) -> Rectangle2D {
        let x1 = self.get_min_x().min(r.get_min_x());
        let y1 = self.get_min_y().min(r.get_min_y());
        let x2 = self.get_max_x().max(r.get_max_x());
        let y2 = self.get_max_y().max(r.get_max_y());
        let mut d = Rectangle2D { is_float: self.is_float, ..Rectangle2D::double(0.0, 0.0, 0.0, 0.0) };
        d.set_rect(x1, y1, x2 - x1, y2 - y1);
        d
    }

    /// `createIntersection(r)`.
    pub fn create_intersection(&self, r: &Rectangle2D) -> Rectangle2D {
        let x1 = self.get_min_x().max(r.get_min_x());
        let y1 = self.get_min_y().max(r.get_min_y());
        let x2 = self.get_max_x().min(r.get_max_x());
        let y2 = self.get_max_y().min(r.get_max_y());
        let mut d = Rectangle2D { is_float: self.is_float, ..Rectangle2D::double(0.0, 0.0, 0.0, 0.0) };
        d.set_rect(x1, y1, x2 - x1, y2 - y1);
        d
    }

    /// `RectIterator`.
    fn iterator(&self, at: Option<&AffineTransform>) -> Vec<Seg> {
        let (x, y, w, h) = (self.x, self.y, self.w, self.h);
        if w < 0.0 || h < 0.0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for index in 0..6 {
            if index == 5 {
                out.push((SEG_CLOSE, [0.0; 6]));
                continue;
            }
            let mut c = [0.0; 6];
            c[0] = x;
            c[1] = y;
            if index == 1 || index == 2 {
                c[0] += w;
            }
            if index == 2 || index == 3 {
                c[1] += h;
            }
            if let Some(at) = at {
                at.transform_coords(&mut c[..2]);
            }
            out.push((if index == 0 { SEG_MOVETO } else { SEG_LINETO }, c));
        }
        out
    }
}

/// `Path2D.Float` (`GeneralPath`, `is_float`) or `Path2D.Double`; a float path keeps its coordinates rounded.
#[derive(Clone, Debug, PartialEq)]
pub struct Path2D {
    pub winding_rule: i32,
    pub types: Vec<i32>,
    pub coords: Vec<f64>,
    pub is_float: bool,
}

impl Path2D {
    pub fn new_float(rule: i32) -> Path2D {
        Path2D { winding_rule: rule, types: Vec::new(), coords: Vec::new(), is_float: true }
    }

    pub fn new_double(rule: i32) -> Path2D {
        Path2D { winding_rule: rule, types: Vec::new(), coords: Vec::new(), is_float: false }
    }

    fn v(&self, x: f64) -> f64 {
        if self.is_float {
            x as f32 as f64
        } else {
            x
        }
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        let (x, y) = (self.v(x), self.v(y));
        if self.types.last() == Some(&SEG_MOVETO) {
            let n = self.coords.len();
            self.coords[n - 2] = x;
            self.coords[n - 1] = y;
        } else {
            self.types.push(SEG_MOVETO);
            self.coords.extend_from_slice(&[x, y]);
        }
    }

    pub fn line_to(&mut self, x: f64, y: f64) {
        let (x, y) = (self.v(x), self.v(y));
        self.types.push(SEG_LINETO);
        self.coords.extend_from_slice(&[x, y]);
    }

    pub fn quad_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        let c = [self.v(x1), self.v(y1), self.v(x2), self.v(y2)];
        self.types.push(SEG_QUADTO);
        self.coords.extend_from_slice(&c);
    }

    pub fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        let c = [self.v(x1), self.v(y1), self.v(x2), self.v(y2), self.v(x3), self.v(y3)];
        self.types.push(SEG_CUBICTO);
        self.coords.extend_from_slice(&c);
    }

    pub fn close_path(&mut self) {
        if self.types.last() != Some(&SEG_CLOSE) {
            self.types.push(SEG_CLOSE);
        }
    }

    /// `append(PathIterator, connect)`: a float path reads the segments as floats.
    pub fn append(&mut self, segs: &[Seg], connect: bool) {
        let mut connect = connect;
        for &(t, c) in segs {
            let c: [f64; 6] = if self.is_float { c.map(|v| v as f32 as f64) } else { c };
            match t {
                SEG_MOVETO => {
                    if !connect || self.types.is_empty() || self.coords.is_empty() {
                        self.move_to(c[0], c[1]);
                    } else {
                        let n = self.coords.len();
                        if !(*self.types.last().unwrap() != SEG_CLOSE && self.coords[n - 2] == c[0] && self.coords[n - 1] == c[1]) {
                            self.line_to(c[0], c[1]);
                        }
                    }
                }
                SEG_LINETO => self.line_to(c[0], c[1]),
                SEG_QUADTO => self.quad_to(c[0], c[1], c[2], c[3]),
                SEG_CUBICTO => self.curve_to(c[0], c[1], c[2], c[3], c[4], c[5]),
                SEG_CLOSE => self.close_path(),
                _ => {}
            }
            connect = false;
        }
    }

    fn iterator(&self, at: Option<&AffineTransform>) -> Vec<Seg> {
        let mut out = Vec::with_capacity(self.types.len());
        let mut ci = 0;
        for &t in &self.types {
            let n = coords_for(t);
            let mut c = [0.0; 6];
            c[..n].copy_from_slice(&self.coords[ci..ci + n]);
            ci += n;
            if let Some(at) = at {
                at.transform_coords(&mut c[..n]);
            }
            out.push((t, c));
        }
        out
    }

    pub fn get_bounds2d(&self) -> Rectangle2D {
        let n = self.coords.len();
        let (mut x1, mut y1, mut x2, mut y2) = (0.0, 0.0, 0.0, 0.0);
        if n > 0 {
            let mut i = n;
            i -= 1;
            y1 = self.coords[i];
            y2 = y1;
            i -= 1;
            x1 = self.coords[i];
            x2 = x1;
            while i > 0 {
                i -= 1;
                let y = self.coords[i];
                i -= 1;
                let x = self.coords[i];
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
        }
        if self.is_float {
            Rectangle2D::float(x1 as f32, y1 as f32, (x2 as f32) - (x1 as f32), (y2 as f32) - (y1 as f32))
        } else {
            Rectangle2D::double(x1, y1, x2 - x1, y2 - y1)
        }
    }
}

/// The `java.awt.Shape`s SVG rendering builds.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Rect(Rectangle2D),
    /// `RoundRectangle2D.Float(x, y, w, h, arcw, arch)`.
    RoundRect { x: f32, y: f32, w: f32, h: f32, aw: f32, ah: f32 },
    /// `Ellipse2D.Float`.
    Ellipse { x: f32, y: f32, w: f32, h: f32 },
    /// `Line2D.Float`.
    Line { x1: f32, y1: f32, x2: f32, y2: f32 },
    /// `Arc2D.Double` (OPEN).
    Arc { x: f64, y: f64, w: f64, h: f64, start: f64, extent: f64, arc_type: i32 },
    Path(Path2D),
}

impl Shape {
    /// `getPathIterator(at)`: the segments, as `currentSegment(double[])` reports them.
    pub fn get_path_iterator(&self, at: Option<&AffineTransform>) -> Vec<Seg> {
        match self {
            Shape::Rect(r) => r.iterator(at),
            Shape::RoundRect { x, y, w, h, aw, ah } => round_rect_iterator(*x as f64, *y as f64, *w as f64, *h as f64, *aw as f64, *ah as f64, at),
            Shape::Ellipse { x, y, w, h } => ellipse_iterator(*x as f64, *y as f64, *w as f64, *h as f64, at),
            Shape::Line { x1, y1, x2, y2 } => {
                let mut a = [*x1 as f64, *y1 as f64, 0.0, 0.0, 0.0, 0.0];
                let mut b = [*x2 as f64, *y2 as f64, 0.0, 0.0, 0.0, 0.0];
                if let Some(at) = at {
                    at.transform_coords(&mut a[..2]);
                    at.transform_coords(&mut b[..2]);
                }
                vec![(SEG_MOVETO, a), (SEG_LINETO, b)]
            }
            Shape::Arc { x, y, w, h, start, extent, arc_type } => arc_iterator(*x, *y, *w, *h, *start, *extent, *arc_type, at),
            Shape::Path(p) => p.iterator(at),
        }
    }

    pub fn get_winding_rule(&self) -> i32 {
        match self {
            Shape::Path(p) => p.winding_rule,
            Shape::Line { .. } | Shape::Rect(_) | Shape::RoundRect { .. } | Shape::Ellipse { .. } | Shape::Arc { .. } => WIND_NON_ZERO,
        }
    }

    pub fn get_bounds2d(&self) -> Rectangle2D {
        match self {
            Shape::Rect(r) => *r,
            Shape::RoundRect { x, y, w, h, .. } | Shape::Ellipse { x, y, w, h } => Rectangle2D::float(*x, *y, *w, *h),
            Shape::Line { x1, y1, x2, y2 } => {
                let (x, w) = if x1 < x2 { (*x1, x2 - x1) } else { (*x2, x1 - x2) };
                let (y, h) = if y1 < y2 { (*y1, y2 - y1) } else { (*y2, y1 - y2) };
                Rectangle2D::float(x, y, w, h)
            }
            Shape::Arc { .. } => {
                let p = create_transformed_shape(&AffineTransform::new(), self);
                p.get_bounds2d()
            }
            Shape::Path(p) => p.get_bounds2d(),
        }
    }
}

/// `at.createTransformedShape(s)`: `new Path2D.Double(s, at)`.
pub fn create_transformed_shape(at: &AffineTransform, s: &Shape) -> Path2D {
    match s {
        Shape::Path(p) => {
            let mut coords = p.coords.clone();
            at.transform_coords(&mut coords);
            Path2D { winding_rule: p.winding_rule, types: p.types.clone(), coords, is_float: false }
        }
        _ => {
            let mut p = Path2D::new_double(s.get_winding_rule());
            p.append(&s.get_path_iterator(Some(at)), false);
            p
        }
    }
}

fn ellipse_iterator(x: f64, y: f64, w: f64, h: f64, at: Option<&AffineTransform>) -> Vec<Seg> {
    const PCV: f64 = 0.7761423749153966;
    const NCV: f64 = 0.22385762508460333;
    const CTRLPTS: [[f64; 6]; 4] = [[1.0, PCV, PCV, 1.0, 0.5, 1.0], [NCV, 1.0, 0.0, PCV, 0.0, 0.5], [0.0, NCV, NCV, 0.0, 0.5, 0.0], [PCV, 0.0, 1.0, NCV, 1.0, 0.5]];
    if w < 0.0 || h < 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut c = [0.0; 6];
    c[0] = x + CTRLPTS[3][4] * w;
    c[1] = y + CTRLPTS[3][5] * h;
    if let Some(at) = at {
        at.transform_coords(&mut c[..2]);
    }
    out.push((SEG_MOVETO, c));
    for ctrls in &CTRLPTS {
        let mut c = [x + ctrls[0] * w, y + ctrls[1] * h, x + ctrls[2] * w, y + ctrls[3] * h, x + ctrls[4] * w, y + ctrls[5] * h];
        if let Some(at) = at {
            at.transform_coords(&mut c);
        }
        out.push((SEG_CUBICTO, c));
    }
    out.push((SEG_CLOSE, [0.0; 6]));
    out
}

fn round_rect_iterator(x: f64, y: f64, w: f64, h: f64, arcw: f64, arch: f64, at: Option<&AffineTransform>) -> Vec<Seg> {
    let angle = std::f64::consts::PI / 4.0;
    let a = 1.0 - angle.cos();
    let b = angle.tan();
    let c = (1.0 + b * b).sqrt() - 1.0 + a;
    let cv = 4.0 / 3.0 * a * b / c;
    let acv = (1.0 - cv) / 2.0;
    let ctrlpts: [&[f64]; 10] = [
        &[0.0, 0.0, 0.0, 0.5],
        &[0.0, 0.0, 1.0, -0.5],
        &[0.0, 0.0, 1.0, -acv, 0.0, acv, 1.0, 0.0, 0.0, 0.5, 1.0, 0.0],
        &[1.0, -0.5, 1.0, 0.0],
        &[1.0, -acv, 1.0, 0.0, 1.0, 0.0, 1.0, -acv, 1.0, 0.0, 1.0, -0.5],
        &[1.0, 0.0, 0.0, 0.5],
        &[1.0, 0.0, 0.0, acv, 1.0, -acv, 0.0, 0.0, 1.0, -0.5, 0.0, 0.0],
        &[0.0, 0.5, 0.0, 0.0],
        &[0.0, acv, 0.0, 0.0, 0.0, 0.0, 0.0, acv, 0.0, 0.0, 0.0, 0.5],
        &[],
    ];
    const TYPES: [i32; 10] = [SEG_MOVETO, SEG_LINETO, SEG_CUBICTO, SEG_LINETO, SEG_CUBICTO, SEG_LINETO, SEG_CUBICTO, SEG_LINETO, SEG_CUBICTO, SEG_CLOSE];
    let aw = w.min(arcw.abs());
    let ah = h.min(arch.abs());
    if aw < 0.0 || ah < 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, ctrls) in ctrlpts.iter().enumerate() {
        let mut c = [0.0; 6];
        let mut nc = 0;
        let mut j = 0;
        while j < ctrls.len() {
            c[nc] = x + ctrls[j] * w + ctrls[j + 1] * aw;
            c[nc + 1] = y + ctrls[j + 2] * h + ctrls[j + 3] * ah;
            nc += 2;
            j += 4;
        }
        if let Some(at) = at {
            at.transform_coords(&mut c[..nc]);
        }
        out.push((TYPES[i], c));
    }
    out
}

fn btan(increment: f64) -> f64 {
    let increment = increment / 2.0;
    4.0 / 3.0 * increment.sin() / (1.0 + increment.cos())
}

#[allow(clippy::too_many_arguments)]
fn arc_iterator(ax: f64, ay: f64, aw: f64, ah: f64, start: f64, extent: f64, arc_type: i32, at: Option<&AffineTransform>) -> Vec<Seg> {
    let w = aw / 2.0;
    let h = ah / 2.0;
    let x = ax + w;
    let y = ay + h;
    let ang_st_rad = -(start * 0.017453292519943295);
    let ext = -extent;
    let (mut arc_segs, mut increment, mut cv);
    if ext >= 360.0 || ext <= -360.0 {
        arc_segs = 4;
        increment = std::f64::consts::PI / 2.0;
        cv = 0.5522847498307933;
        if ext < 0.0 {
            increment = -increment;
            cv = -cv;
        }
    } else {
        arc_segs = (ext.abs() / 90.0).ceil() as i32;
        increment = (ext / arc_segs as f64) * 0.017453292519943295;
        cv = btan(increment);
        if cv == 0.0 {
            arc_segs = 0;
        }
    }
    let mut line_segs = match arc_type {
        0 => 0,
        1 => 1,
        2 => 2,
        _ => 0,
    };
    if w < 0.0 || h < 0.0 {
        arc_segs = -1;
        line_segs = -1;
    }
    let mut out = Vec::new();
    let mut index = 0;
    while index <= arc_segs + line_segs {
        let mut c = [0.0; 6];
        let mut angle = ang_st_rad;
        let t;
        if index == 0 {
            c[0] = x + angle.cos() * w;
            c[1] = y + angle.sin() * h;
            if let Some(at) = at {
                at.transform_coords(&mut c[..2]);
            }
            t = SEG_MOVETO;
        } else if index > arc_segs {
            if index == arc_segs + line_segs {
                out.push((SEG_CLOSE, c));
                index += 1;
                continue;
            }
            c[0] = x;
            c[1] = y;
            if let Some(at) = at {
                at.transform_coords(&mut c[..2]);
            }
            t = SEG_LINETO;
        } else {
            angle += increment * (index - 1) as f64;
            let mut relx = angle.cos();
            let mut rely = angle.sin();
            c[0] = x + (relx - cv * rely) * w;
            c[1] = y + (rely + cv * relx) * h;
            angle += increment;
            relx = angle.cos();
            rely = angle.sin();
            c[2] = x + (relx + cv * rely) * w;
            c[3] = y + (rely - cv * relx) * h;
            c[4] = x + relx * w;
            c[5] = y + rely * h;
            if let Some(at) = at {
                at.transform_coords(&mut c);
            }
            t = SEG_CUBICTO;
        }
        out.push((t, c));
        index += 1;
    }
    out
}
