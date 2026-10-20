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
