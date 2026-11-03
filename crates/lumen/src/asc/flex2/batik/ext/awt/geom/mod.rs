//! Port of Batik's `ext.awt.geom.ExtendedGeneralPath`: a `GeneralPath` that also records its own segments
//! (with arcs) and emits a `moveTo` only once something follows it.

use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::AffineTransform;
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::{create_transformed_shape, Path2D, Seg, Shape, SEG_CLOSE, SEG_CUBICTO, SEG_LINETO, SEG_MOVETO, SEG_QUADTO};

pub const SEG_ARCTO: i32 = 4321;

#[derive(Clone, Debug)]
pub struct ExtendedGeneralPath {
    pub path: Path2D,
    pub types: Vec<i32>,
    pub values: Vec<f32>,
    mx: f32,
    my: f32,
    cx: f32,
    cy: f32,
}

impl ExtendedGeneralPath {
    pub fn new(rule: i32) -> ExtendedGeneralPath {
        ExtendedGeneralPath { path: Path2D::new_float(rule), types: Vec::new(), values: Vec::new(), mx: 0.0, my: 0.0, cx: 0.0, cy: 0.0 }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn arc_to(&mut self, rx: f32, ry: f32, angle: f32, large_arc_flag: bool, sweep_flag: bool, x: f32, y: f32) {
        if rx == 0.0 || ry == 0.0 {
            self.line_to(x, y);
            return;
        }
        self.check_move_to();
        let x0 = self.cx as f64;
        let y0 = self.cy as f64;
        if x0 == x as f64 && y0 == y as f64 {
            return;
        }
        let arc = compute_arc(x0, y0, rx as f64, ry as f64, angle as f64, large_arc_flag, sweep_flag, x as f64, y as f64);
        let Shape::Arc { x: ax, y: ay, w, h, .. } = arc else { unreachable!() };
        let t = AffineTransform::get_rotate_instance_at(angle as f64 * 0.017453292519943295, ax + w / 2.0, ay + h / 2.0);
        let s = create_transformed_shape(&t, &arc);
        let segs: Vec<Seg> = Shape::Path(s).get_path_iterator(None);
        self.path.append(&segs, true);
        self.types.push(SEG_ARCTO);
        self.values.extend_from_slice(&[rx, ry, angle, if large_arc_flag { 1.0 } else { 0.0 }, if sweep_flag { 1.0 } else { 0.0 }, x, y]);
        self.cx = x;
        self.cy = y;
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        self.types.push(SEG_MOVETO);
        self.values.extend_from_slice(&[x, y]);
        self.cx = x;
        self.mx = x;
        self.cy = y;
        self.my = y;
    }

    pub fn line_to(&mut self, x: f32, y: f32) {
        self.check_move_to();
        self.path.line_to(x as f64, y as f64);
        self.types.push(SEG_LINETO);
        self.values.extend_from_slice(&[x, y]);
        self.cx = x;
        self.cy = y;
    }

    pub fn quad_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.check_move_to();
        self.path.quad_to(x1 as f64, y1 as f64, x2 as f64, y2 as f64);
        self.types.push(SEG_QUADTO);
        self.values.extend_from_slice(&[x1, y1, x2, y2]);
        self.cx = x2;
        self.cy = y2;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
        self.check_move_to();
        self.path.curve_to(x1 as f64, y1 as f64, x2 as f64, y2 as f64, x3 as f64, y3 as f64);
        self.types.push(SEG_CUBICTO);
        self.values.extend_from_slice(&[x1, y1, x2, y2, x3, y3]);
        self.cx = x3;
        self.cy = y3;
    }

    pub fn close_path(&mut self) {
        if self.types.last() == Some(&SEG_CLOSE) {
            return;
        }
        if !self.types.is_empty() && *self.types.last().unwrap() != SEG_MOVETO {
            self.path.close_path();
        }
        self.types.push(SEG_CLOSE);
        self.cx = self.mx;
        self.cy = self.my;
    }

    fn check_move_to(&mut self) {
        let n = self.types.len();
        if n == 0 {
            return;
        }
        let nv = self.values.len();
        match self.types[n - 1] {
            SEG_MOVETO => self.path.move_to(self.values[nv - 2] as f64, self.values[nv - 1] as f64),
            SEG_CLOSE => {
                if n == 1 {
                    return;
                }
                if self.types[n - 2] == SEG_MOVETO {
                    self.path.move_to(self.values[nv - 2] as f64, self.values[nv - 1] as f64);
                }
            }
            _ => {}
        }
    }

    pub fn get_current_point(&self) -> Option<(f64, f64)> {
        if self.values.is_empty() {
            return None;
        }
        Some((self.cx as f64, self.cy as f64))
    }
}

/// `computeArc(...)`: the `Arc2D.Double` the SVG arc parameters describe.
#[allow(clippy::too_many_arguments)]
pub fn compute_arc(x0: f64, y0: f64, rx: f64, ry: f64, angle: f64, large_arc_flag: bool, sweep_flag: bool, x: f64, y: f64) -> Shape {
    let dx2 = (x0 - x) / 2.0;
    let dy2 = (y0 - y) / 2.0;
    let angle = (angle % 360.0) * 0.017453292519943295;
    let cos_angle = angle.cos();
    let sin_angle = angle.sin();
    let x1 = cos_angle * dx2 + sin_angle * dy2;
    let y1 = -sin_angle * dx2 + cos_angle * dy2;
    let mut rx = rx.abs();
    let mut ry = ry.abs();
    let mut prx = rx * rx;
    let mut pry = ry * ry;
    let px1 = x1 * x1;
    let py1 = y1 * y1;
    let radii_check = px1 / prx + py1 / pry;
    if radii_check > 1.0 {
        rx *= radii_check.sqrt();
        ry *= radii_check.sqrt();
        prx = rx * rx;
        pry = ry * ry;
    }
    let mut sign = if large_arc_flag == sweep_flag { -1.0 } else { 1.0 };
    let mut sq = ((prx * pry) - (prx * py1) - (pry * px1)) / ((prx * py1) + (pry * px1));
    sq = if sq < 0.0 { 0.0 } else { sq };
    let coef = sign * sq.sqrt();
    let cx1 = coef * ((rx * y1) / ry);
    let cy1 = coef * -((ry * x1) / rx);
    let sx2 = (x0 + x) / 2.0;
    let sy2 = (y0 + y) / 2.0;
    let cx = sx2 + (cos_angle * cx1 - sin_angle * cy1);
    let cy = sy2 + (sin_angle * cx1 + cos_angle * cy1);
    let ux = (x1 - cx1) / rx;
    let uy = (y1 - cy1) / ry;
    let vx = (-x1 - cx1) / rx;
    let vy = (-y1 - cy1) / ry;
    let mut n = ((ux * ux) + (uy * uy)).sqrt();
    let mut p = ux;
    sign = if uy < 0.0 { -1.0 } else { 1.0 };
    let mut angle_start = (sign * (p / n).acos()) * 57.29577951308232;
    n = ((ux * ux + uy * uy) * (vx * vx + vy * vy)).sqrt();
    p = ux * vx + uy * vy;
    sign = if ux * vy - uy * vx < 0.0 { -1.0 } else { 1.0 };
    let mut angle_extent = (sign * (p / n).acos()) * 57.29577951308232;
    if !sweep_flag && angle_extent > 0.0 {
        angle_extent -= 360.0;
    } else if sweep_flag && angle_extent < 0.0 {
        angle_extent += 360.0;
    }
    angle_extent %= 360.0;
    angle_start %= 360.0;
    Shape::Arc { x: cx - rx, y: cy - ry, w: rx * 2.0, h: ry * 2.0, start: -angle_start, extent: -angle_extent, arc_type: 0 }
}
