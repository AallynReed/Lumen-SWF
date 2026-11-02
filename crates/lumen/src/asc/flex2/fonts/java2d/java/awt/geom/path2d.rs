//! Port of JDK 17 `java.awt.geom.Path2D.Float` (and `GeneralPath`, which only adds constructors), the parts glyph outlines use.

use crate::asc::flex2::fonts::java2d::freetype_scaler::{GeneralPathData, JAVA_WIND_NON_ZERO, SEG_CLOSE, SEG_CUBICTO, SEG_LINETO, SEG_MOVETO, SEG_QUADTO};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path2DFloat {
    pub winding_rule: i32,
    pub point_types: Vec<u8>,
    pub float_coords: Vec<f32>,
}

pub type GeneralPath = Path2DFloat;

/// Coordinates a segment type carries, as `PathIterator` reports them.
pub fn coords_for(seg_type: u8) -> usize {
    match seg_type {
        SEG_MOVETO | SEG_LINETO => 2,
        SEG_QUADTO => 4,
        SEG_CUBICTO => 6,
        _ => 0,
    }
}

impl Path2DFloat {
    /// `new GeneralPath(rule)`.
    pub fn new(rule: i32) -> Path2DFloat {
        Path2DFloat { winding_rule: rule, point_types: Vec::new(), float_coords: Vec::new() }
    }

    /// `new GeneralPath(rule, pointTypes, numTypes, pointCoords, numCoords)` as the scaler natives build it.
    pub fn from_data(d: GeneralPathData) -> Path2DFloat {
        Path2DFloat { winding_rule: d.winding_rule, point_types: d.types, float_coords: d.coords }
    }

    pub fn empty() -> Path2DFloat {
        Path2DFloat::new(JAVA_WIND_NON_ZERO)
    }

    fn need_room(&self, need_move: bool) {
        debug_assert!(!(self.point_types.is_empty() && need_move), "missing initial moveto in path definition");
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        if let Some(&SEG_MOVETO) = self.point_types.last() {
            let n = self.float_coords.len();
            self.float_coords[n - 2] = x;
            self.float_coords[n - 1] = y;
        } else {
            self.need_room(false);
            self.point_types.push(SEG_MOVETO);
            self.float_coords.extend_from_slice(&[x, y]);
        }
    }

    pub fn line_to(&mut self, x: f32, y: f32) {
        self.need_room(true);
        self.point_types.push(SEG_LINETO);
        self.float_coords.extend_from_slice(&[x, y]);
    }

    pub fn quad_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.need_room(true);
        self.point_types.push(SEG_QUADTO);
        self.float_coords.extend_from_slice(&[x1, y1, x2, y2]);
    }

    pub fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
        self.need_room(true);
        self.point_types.push(SEG_CUBICTO);
        self.float_coords.extend_from_slice(&[x1, y1, x2, y2, x3, y3]);
    }

    pub fn close_path(&mut self) {
        if self.point_types.last() != Some(&SEG_CLOSE) {
            self.need_room(true);
            self.point_types.push(SEG_CLOSE);
        }
    }

    /// `append(pi, connect)` with `pi = other.getPathIterator(null)`.
    pub fn append(&mut self, other: &Path2DFloat, connect: bool) {
        let mut connect = connect;
        let mut ci = 0;
        for &t in &other.point_types {
            let n = coords_for(t);
            let c = &other.float_coords[ci..ci + n];
            ci += n;
            match t {
                SEG_MOVETO => {
                    if !connect || self.point_types.is_empty() || self.float_coords.is_empty() {
                        self.move_to(c[0], c[1]);
                    } else {
                        let nc = self.float_coords.len();
                        if !(*self.point_types.last().unwrap() != SEG_CLOSE && self.float_coords[nc - 2] == c[0] && self.float_coords[nc - 1] == c[1]) {
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

    /// `transform(AffineTransform.getTranslateInstance(x, y))`.
    pub fn translate(&mut self, x: f32, y: f32) {
        let (tx, ty) = (x as f64, y as f64);
        for p in self.float_coords.chunks_mut(2) {
            p[0] = (p[0] as f64 + tx) as f32;
            p[1] = (p[1] as f64 + ty) as f32;
        }
    }

    /// The segments `getPathIterator(null)` yields: type and the reused six-float array `currentSegment` fills.
    pub fn segments(&self) -> Vec<(i32, [f32; 6])> {
        let mut out = Vec::with_capacity(self.point_types.len());
        let mut ci = 0;
        let mut c = [0f32; 6];
        for &t in &self.point_types {
            let n = coords_for(t);
            c[..n].copy_from_slice(&self.float_coords[ci..ci + n]);
            ci += n;
            out.push((t as i32, c));
        }
        out
    }
}
