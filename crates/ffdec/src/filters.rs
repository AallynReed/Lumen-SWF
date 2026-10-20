//! FILTERLIST entries of PlaceObject3 and DefineButton2 records.

use crate::error::Result;
use crate::io::Reader;
use crate::types::Rgba;

#[derive(Debug, Clone, PartialEq)]
pub struct Shadow {
    pub color: Rgba,
    pub blur_x: f64,
    pub blur_y: f64,
    pub angle: f64,
    pub distance: f64,
    pub strength: f64,
    pub inner: bool,
    pub knockout: bool,
    pub composite_source: bool,
    pub passes: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bevel {
    pub colors: Vec<Rgba>,
    pub ratios: Vec<u8>,
    pub blur_x: f64,
    pub blur_y: f64,
    pub angle: f64,
    pub distance: f64,
    pub strength: f64,
    pub inner: bool,
    pub knockout: bool,
    pub composite_source: bool,
    pub on_top: bool,
    pub passes: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    DropShadow(Shadow),
    Blur { blur_x: f64, blur_y: f64, passes: u32 },
    Glow(Shadow),
    Bevel(Bevel),
    GradientGlow(Bevel),
    Convolution { cols: u8, rows: u8, divisor: f32, bias: f32, matrix: Vec<f32>, default_color: Rgba, clamp: bool, preserve_alpha: bool },
    ColorMatrix([f32; 20]),
    GradientBevel(Bevel),
}

fn bevel_tail(r: &mut Reader, colors: Vec<Rgba>, ratios: Vec<u8>) -> Result<Bevel> {
    let blur_x = r.fixed()?;
    let blur_y = r.fixed()?;
    let angle = r.fixed()?;
    let distance = r.fixed()?;
    let strength = r.fixed8()? as f64;
    Ok(Bevel {
        colors,
        ratios,
        blur_x,
        blur_y,
        angle,
        distance,
        strength,
        inner: r.bit()?,
        knockout: r.bit()?,
        composite_source: r.bit()?,
        on_top: r.bit()?,
        passes: r.ub(4)?,
    })
}

fn gradient_bevel(r: &mut Reader) -> Result<Bevel> {
    let n = r.u8()? as usize;
    let mut colors = Vec::with_capacity(n);
    for _ in 0..n {
        colors.push(Rgba::read_rgba(r)?);
    }
    let ratios = r.bytes(n)?.to_vec();
    bevel_tail(r, colors, ratios)
}

pub fn read_filter(r: &mut Reader) -> Result<Option<Filter>> {
    Ok(Some(match r.u8()? {
        0 => {
            let color = Rgba::read_rgba(r)?;
            let blur_x = r.fixed()?;
            let blur_y = r.fixed()?;
            let angle = r.fixed()?;
            let distance = r.fixed()?;
            let strength = r.fixed8()? as f64;
            Filter::DropShadow(Shadow {
                color,
                blur_x,
                blur_y,
                angle,
                distance,
                strength,
                inner: r.bit()?,
                knockout: r.bit()?,
                composite_source: r.bit()?,
                passes: r.ub(5)?,
            })
        }
        1 => {
            let blur_x = r.fixed()?;
            let blur_y = r.fixed()?;
            let passes = r.ub(5)?;
            r.ub(3)?;
            Filter::Blur { blur_x, blur_y, passes }
        }
        2 => {
            let color = Rgba::read_rgba(r)?;
            let blur_x = r.fixed()?;
            let blur_y = r.fixed()?;
            let strength = r.fixed8()? as f64;
            Filter::Glow(Shadow {
                color,
                blur_x,
                blur_y,
                angle: 0.0,
                distance: 0.0,
                strength,
                inner: r.bit()?,
                knockout: r.bit()?,
                composite_source: r.bit()?,
                passes: r.ub(5)?,
            })
        }
        3 => {
            // Highlight comes first, contrary to the specification.
            let highlight = Rgba::read_rgba(r)?;
            let shadow = Rgba::read_rgba(r)?;
            let mut s = shadow;
            s.a = 0;
            let mut h = highlight;
            h.a = 0;
            Filter::Bevel(bevel_tail(r, vec![shadow, s, h, highlight], vec![0, 127, 128, 255])?)
        }
        4 => Filter::GradientGlow(gradient_bevel(r)?),
        5 => {
            let cols = r.u8()?;
            let rows = r.u8()?;
            let divisor = r.f32()?;
            let bias = r.f32()?;
            let mut matrix = Vec::with_capacity(cols as usize * rows as usize);
            for _ in 0..cols as usize * rows as usize {
                matrix.push(r.f32()?);
            }
            let default_color = Rgba::read_rgba(r)?;
            r.ub(6)?;
            let clamp = r.bit()?;
            let preserve_alpha = r.bit()?;
            Filter::Convolution { cols, rows, divisor, bias, matrix, default_color, clamp, preserve_alpha }
        }
        6 => {
            let mut m = [0f32; 20];
            for v in m.iter_mut() {
                *v = r.f32()?;
            }
            Filter::ColorMatrix(m)
        }
        7 => Filter::GradientBevel(gradient_bevel(r)?),
        _ => return Ok(None),
    }))
}

pub fn read_filter_list(r: &mut Reader) -> Result<Vec<Filter>> {
    let n = r.u8()?;
    let mut out = Vec::new();
    for _ in 0..n {
        match read_filter(r)? {
            Some(f) => out.push(f),
            None => break,
        }
    }
    Ok(out)
}
