use serde::Serialize;

use crate::error::Result;
use crate::io::{bits_needed_signed, Reader, Writer};

/// A rectangle in twips (1/20 px).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Rect {
    pub x_min: i32,
    pub x_max: i32,
    pub y_min: i32,
    pub y_max: i32,
}

impl Rect {
    pub fn read(r: &mut Reader) -> Result<Self> {
        r.align();
        let n = r.ub(5)?;
        let rect = Rect { x_min: r.sb(n)?, x_max: r.sb(n)?, y_min: r.sb(n)?, y_max: r.sb(n)? };
        r.align();
        Ok(rect)
    }

    pub fn write(&self, w: &mut Writer) {
        let n = bits_needed_signed(&[self.x_min, self.x_max, self.y_min, self.y_max]);
        w.ub(5, n);
        for v in [self.x_min, self.x_max, self.y_min, self.y_max] {
            w.sb(n, v);
        }
        w.flush_bits();
    }

    pub fn width_px(&self) -> f64 {
        (self.x_max - self.x_min) as f64 / 20.0
    }

    pub fn height_px(&self) -> f64 {
        (self.y_max - self.y_min) as f64 / 20.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub fn read_rgb(r: &mut Reader) -> Result<Self> {
        Ok(Rgba { r: r.u8()?, g: r.u8()?, b: r.u8()?, a: 255 })
    }

    pub fn read_rgba(r: &mut Reader) -> Result<Self> {
        Ok(Rgba { r: r.u8()?, g: r.u8()?, b: r.u8()?, a: r.u8()? })
    }

    pub fn hex(&self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

/// An affine transform with translation in twips.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Default for Matrix {
    fn default() -> Self {
        Matrix::IDENTITY
    }
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 };

    pub fn read(r: &mut Reader) -> Result<Self> {
        r.align();
        let mut m = Matrix::IDENTITY;
        if r.bit()? {
            let n = r.ub(5)?;
            m.a = r.fb(n)?;
            m.d = r.fb(n)?;
        }
        if r.bit()? {
            let n = r.ub(5)?;
            m.b = r.fb(n)?;
            m.c = r.fb(n)?;
        }
        let n = r.ub(5)?;
        m.tx = r.sb(n)? as f64;
        m.ty = r.sb(n)? as f64;
        r.align();
        Ok(m)
    }

    pub fn translate(tx: f64, ty: f64) -> Self {
        Matrix { tx, ty, ..Matrix::IDENTITY }
    }

    pub fn scale(sx: f64, sy: f64) -> Self {
        Matrix { a: sx, d: sy, ..Matrix::IDENTITY }
    }

    /// `self` applied after `inner`: maps a point through `inner`, then `self`.
    pub fn concat(&self, inner: &Matrix) -> Matrix {
        Matrix {
            a: self.a * inner.a + self.c * inner.b,
            b: self.b * inner.a + self.d * inner.b,
            c: self.a * inner.c + self.c * inner.d,
            d: self.b * inner.c + self.d * inner.d,
            tx: self.a * inner.tx + self.c * inner.ty + self.tx,
            ty: self.b * inner.tx + self.d * inner.ty + self.ty,
        }
    }

    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (self.a * x + self.c * y + self.tx, self.b * x + self.d * y + self.ty)
    }

    pub fn is_identity(&self) -> bool {
        *self == Matrix::IDENTITY
    }

    pub fn lerp(&self, o: &Matrix, t: f64) -> Matrix {
        let l = |a: f64, b: f64| a + (b - a) * t;
        Matrix { a: l(self.a, o.a), b: l(self.b, o.b), c: l(self.c, o.c), d: l(self.d, o.d), tx: l(self.tx, o.tx), ty: l(self.ty, o.ty) }
    }

    pub fn transform_rect(&self, r: &Rect) -> Rect {
        let pts = [
            self.apply(r.x_min as f64, r.y_min as f64),
            self.apply(r.x_max as f64, r.y_min as f64),
            self.apply(r.x_min as f64, r.y_max as f64),
            self.apply(r.x_max as f64, r.y_max as f64),
        ];
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for (x, y) in pts {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        Rect { x_min: x0.floor() as i32, y_min: y0.floor() as i32, x_max: x1.ceil() as i32, y_max: y1.ceil() as i32 }
    }
}

impl Rect {
    pub fn is_empty(&self) -> bool {
        self.x_max <= self.x_min || self.y_max <= self.y_min
    }

    pub fn union(&self, o: &Rect) -> Rect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        Rect { x_min: self.x_min.min(o.x_min), y_min: self.y_min.min(o.y_min), x_max: self.x_max.max(o.x_max), y_max: self.y_max.max(o.y_max) }
    }
}

/// Colour transform with multipliers in 1/256 units, as stored in the SWF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct ColorTransform {
    pub mult: [i32; 4],
    pub add: [i32; 4],
}

impl Default for ColorTransform {
    fn default() -> Self {
        ColorTransform::IDENTITY
    }
}

impl ColorTransform {
    pub const IDENTITY: ColorTransform = ColorTransform { mult: [256; 4], add: [0; 4] };

    pub fn read(r: &mut Reader, alpha: bool) -> Result<Self> {
        r.align();
        let has_add = r.bit()?;
        let has_mult = r.bit()?;
        let n = r.ub(4)?;
        let k = if alpha { 4 } else { 3 };
        let mut c = ColorTransform::IDENTITY;
        if has_mult {
            for i in 0..k {
                c.mult[i] = r.sb(n)?;
            }
        }
        if has_add {
            for i in 0..k {
                c.add[i] = r.sb(n)?;
            }
        }
        r.align();
        Ok(c)
    }

    pub fn is_identity(&self) -> bool {
        *self == ColorTransform::IDENTITY
    }

    /// `self` applied after `inner`.
    pub fn concat(&self, inner: &ColorTransform) -> ColorTransform {
        let mut c = ColorTransform::IDENTITY;
        for i in 0..4 {
            c.mult[i] = (self.mult[i] * inner.mult[i]) >> 8;
            c.add[i] = ((self.mult[i] * inner.add[i]) >> 8) + self.add[i];
        }
        c
    }

    pub fn apply(&self, col: Rgba) -> Rgba {
        let ch = |v: u8, i: usize| (((v as i32 * self.mult[i]) >> 8) + self.add[i]).clamp(0, 255) as u8;
        Rgba { r: ch(col.r, 0), g: ch(col.g, 1), b: ch(col.b, 2), a: ch(col.a, 3) }
    }
}

impl Rgba {
    pub fn lerp(&self, o: &Rgba, t: f64) -> Rgba {
        let l = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round().clamp(0.0, 255.0) as u8;
        Rgba { r: l(self.r, o.r), g: l(self.g, o.g), b: l(self.b, o.b), a: l(self.a, o.a) }
    }

    pub const BLACK: Rgba = Rgba { r: 0, g: 0, b: 0, a: 255 };
}
