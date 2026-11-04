//! Port of Batik's `ext.awt.MultipleGradientPaintContext` (the colour tables and lookups both gradient contexts share),
//! the `createContext` methods of `LinearGradientPaint`/`RadialGradientPaint`, and the `PaintContext` calls Java2D's
//! `AlphaPaintPipe` makes on the result, for a `BufferedImage.TYPE_INT_ARGB` destination (sRGB, not premultiplied, so
//! `GraphicsUtil.coerceColorModel`/`coerceData` never change anything). Errors are `"<exception class>: <message>"`.

use std::cell::RefCell;
use std::sync::OnceLock;

use crate::asc::flex2::batik::ext::awt::linear_gradient_paint_context::LinearGradientPaintContext;
use crate::asc::flex2::batik::ext::awt::radial_gradient_paint_context::RadialGradientPaintContext;
use crate::asc::flex2::batik::ext::awt::{LinearGradientPaint, RadialGradientPaint, LINEAR_RGB, NO_CYCLE, REPEAT};
use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::AffineTransform;

pub(crate) const GRADIENT_SIZE: i32 = 256;
pub(crate) const GRADIENT_SIZE_INDEX: i32 = GRADIENT_SIZE - 1;
const MAX_GRADIENT_ARRAY_SIZE: i32 = 5000;
pub(crate) const NPE: &str = "java.lang.NullPointerException";

/// A `RenderingHints` value of `KEY_RENDERING` or `KEY_COLOR_RENDERING`; an absent key is `Default`. No other hint
/// changes the gradient pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HintValue {
    #[default]
    Default,
    Speed,
    Quality,
}

/// The two hints the gradient contexts read from `SunGraphics2D.getRenderingHints()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct GradientHints {
    pub rendering: HintValue,
    pub color_rendering: HintValue,
}

pub(crate) fn aioobe(i: i32, len: usize) -> String {
    format!("java.lang.ArrayIndexOutOfBoundsException: Index {i} out of bounds for length {len}")
}

#[inline]
pub(crate) fn at(a: &[i32], i: i32) -> Result<i32, String> {
    if i >= 0 && (i as usize) < a.len() { Ok(a[i as usize]) } else { Err(aioobe(i, a.len())) }
}

#[inline]
pub(crate) fn atf(a: &[f32], i: i32) -> Result<f32, String> {
    if i >= 0 && (i as usize) < a.len() { Ok(a[i as usize]) } else { Err(aioobe(i, a.len())) }
}

#[inline]
fn ushr(v: i32, n: u32) -> i32 {
    ((v as u32) >> n) as i32
}

/// `Math.round(float)`.
pub(crate) fn java_round_f(a: f32) -> i32 {
    let int_bits = a.to_bits() as i32;
    let biased_exp = (int_bits & 0x7F80_0000) >> 23;
    let shift = (24 - 2 + 127) - biased_exp;
    if shift & -32 == 0 {
        let mut r = (int_bits & 0x007F_FFFF) | 0x0080_0000;
        if int_bits < 0 {
            r = -r;
        }
        ((r >> shift) + 1) >> 1
    } else {
        a as i32
    }
}

/// `convertSRGBtoLinearRGB`.
fn convert_srgb_to_linear_rgb(color: i32) -> i32 {
    let input = color as f32 / 255.0f32;
    let output = if input <= 0.04045f32 { input / 12.92f32 } else { ((input as f64 + 0.055) / 1.055).powf(2.4) as f32 };
    java_round_f(output * 255.0f32)
}

/// `convertLinearRGBtoSRGB`.
fn convert_linear_rgb_to_srgb(color: i32) -> i32 {
    let input = color as f32 / 255.0f32;
    let output = if input as f64 <= 0.0031308 { input * 12.92f32 } else { (1.055f32 * ((input as f64).powf(1.0 / 2.4) as f32)) - 0.055f32 };
    java_round_f(output * 255.0f32)
}

struct Tables {
    srgb_to_linear_rgb: [i32; 256],
    linear_rgb_to_srgb: [i32; 256],
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = Tables { srgb_to_linear_rgb: [0; 256], linear_rgb_to_srgb: [0; 256] };
        for k in 0..256 {
            t.srgb_to_linear_rgb[k] = convert_srgb_to_linear_rgb(k as i32);
            t.linear_rgb_to_srgb[k] = convert_linear_rgb_to_srgb(k as i32);
        }
        t
    })
}

/// The static `SRGBtoLinearRGB` and `LinearRGBtoSRGB` tables.
pub fn conversion_tables() -> (&'static [i32; 256], &'static [i32; 256]) {
    let t = tables();
    (&t.srgb_to_linear_rgb, &t.linear_rgb_to_srgb)
}

fn convert_entire_color(rgb: i32, table: &[i32; 256]) -> i32 {
    let a1 = (rgb >> 24) & 0xff;
    let r1 = table[((rgb >> 16) & 0xff) as usize];
    let g1 = table[((rgb >> 8) & 0xff) as usize];
    let b1 = table[(rgb & 0xff) as usize];
    (a1 << 24) | (r1 << 16) | (g1 << 8) | b1
}

/// The `MultipleGradientPaintContext` state.
pub(crate) struct MultipleGradientPaintContext {
    /// `dataModel` is `srgbmodel_A` (else `srgbmodel_NA`); `model` is the same object.
    pub data_has_alpha: bool,
    pub cycle_method: i32,
    pub color_space: i32,
    pub a00: f32,
    pub a01: f32,
    pub a10: f32,
    pub a11: f32,
    pub a02: f32,
    pub a12: f32,
    pub is_simple_lookup: bool,
    pub has_discontinuity: bool,
    pub fast_gradient_array_size: i32,
    pub gradient: Vec<i32>,
    pub gradients: Vec<Option<Vec<i32>>>,
    pub gradient_average: i32,
    pub gradient_underflow: i32,
    pub gradient_overflow: i32,
    pub gradients_length: i32,
    pub normalized_intervals: Vec<f32>,
    pub fractions: Vec<f32>,
    transparency_test: i32,
}

impl MultipleGradientPaintContext {
    /// The constructor; `Ok(None)` is the `NoninvertibleTransformException` of `t.createInverse()`.
    pub fn new(t: &AffineTransform, fractions: &[f32], colors: &[i32], cycle_method: i32, color_space: i32) -> Result<Option<Self>, String> {
        let mut fix_first = false;
        let mut fix_last = false;
        let mut len = fractions.len();
        if fractions[0] != 0.0 {
            fix_first = true;
            len += 1;
        }
        if fractions[fractions.len() - 1] != 1.0 {
            fix_last = true;
            len += 1;
        }
        for i in 0..fractions.len() - 1 {
            if fractions[i] == fractions[i + 1] {
                len -= 1;
            }
        }

        let mut this_fractions = vec![0f32; len];
        let mut lo_colors = vec![0i32; len - 1];
        let mut hi_colors = vec![0i32; len - 1];
        let mut normalized_intervals = vec![0f32; len - 1];
        let gradient_underflow = colors[0];
        let gradient_overflow = colors[colors.len() - 1];
        let mut has_discontinuity = false;

        let mut idx = 0;
        if fix_first {
            this_fractions[0] = 0.0;
            lo_colors[0] = colors[0];
            hi_colors[0] = colors[0];
            normalized_intervals[0] = fractions[0];
            idx += 1;
        }
        for i in 0..fractions.len() - 1 {
            if fractions[i] == fractions[i + 1] {
                if colors[i] != colors[i + 1] {
                    has_discontinuity = true;
                }
                continue;
            }
            this_fractions[idx] = fractions[i];
            lo_colors[idx] = colors[i];
            hi_colors[idx] = colors[i + 1];
            normalized_intervals[idx] = fractions[i + 1] - fractions[i];
            idx += 1;
        }
        this_fractions[idx] = fractions[fractions.len() - 1];
        if fix_last {
            lo_colors[idx] = colors[colors.len() - 1];
            hi_colors[idx] = colors[colors.len() - 1];
            normalized_intervals[idx] = 1.0 - fractions[fractions.len() - 1];
            idx += 1;
            this_fractions[idx] = 1.0;
        }

        let t_inv = match t.create_inverse() {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        let mut c = MultipleGradientPaintContext {
            data_has_alpha: true,
            cycle_method,
            color_space,
            a00: t_inv.m00 as f32,
            a10: t_inv.m10 as f32,
            a01: t_inv.m01 as f32,
            a11: t_inv.m11 as f32,
            a02: t_inv.m02 as f32,
            a12: t_inv.m12 as f32,
            is_simple_lookup: true,
            has_discontinuity,
            fast_gradient_array_size: 0,
            gradient: Vec::new(),
            gradients: Vec::new(),
            gradient_average: 0,
            gradient_underflow,
            gradient_overflow,
            gradients_length: 0,
            normalized_intervals,
            fractions: this_fractions,
            transparency_test: 0,
        };
        c.calculate_gradient_fractions(&mut lo_colors, &mut hi_colors)?;
        Ok(Some(c))
    }

    /// `calculateGradientFractions`.
    fn calculate_gradient_fractions(&mut self, lo_colors: &mut [i32], hi_colors: &mut [i32]) -> Result<(), String> {
        if self.color_space == LINEAR_RGB {
            let s2l = &tables().srgb_to_linear_rgb;
            for i in 0..lo_colors.len() {
                lo_colors[i] = convert_entire_color(lo_colors[i], s2l);
                hi_colors[i] = convert_entire_color(hi_colors[i], s2l);
            }
        }

        self.transparency_test = 0xff00_0000u32 as i32;
        if self.cycle_method == NO_CYCLE {
            self.transparency_test &= self.gradient_underflow;
            self.transparency_test &= self.gradient_overflow;
        }

        self.gradients = vec![None; self.fractions.len() - 1];
        self.gradients_length = self.gradients.len() as i32;

        let n = self.normalized_intervals.len();
        let mut imin = 1f32;
        for i in 0..n {
            imin = if imin > self.normalized_intervals[i] { self.normalized_intervals[i] } else { imin };
        }

        let mut estimated_size: i32 = 0;
        if imin == 0.0 {
            estimated_size = i32::MAX;
            self.has_discontinuity = true;
        } else {
            for i in 0..self.normalized_intervals.len() {
                estimated_size = (estimated_size as f32 + (self.normalized_intervals[i] / imin) * GRADIENT_SIZE as f32) as i32;
            }
        }

        if estimated_size > MAX_GRADIENT_ARRAY_SIZE {
            self.calculate_multiple_array_gradient(lo_colors, hi_colors)?;
            if self.cycle_method == REPEAT {
                let first = at(self.gradients[0].as_deref().ok_or(NPE)?, 0)?;
                let last = at(self.gradients[self.gradients.len() - 1].as_deref().ok_or(NPE)?, GRADIENT_SIZE_INDEX)?;
                if first != last {
                    self.has_discontinuity = true;
                }
            }
        } else {
            self.calculate_single_array_gradient(lo_colors, hi_colors, imin)?;
            if self.cycle_method == REPEAT && self.gradient[0] != at(&self.gradient, self.fast_gradient_array_size)? {
                self.has_discontinuity = true;
            }
        }

        if ushr(self.transparency_test, 24) == 0xff {
            self.data_has_alpha = false;
        }
        Ok(())
    }

    /// `calculateSingleArrayGradient` (the fast lookup).
    fn calculate_single_array_gradient(&mut self, lo_colors: &[i32], hi_colors: &[i32], imin: f32) -> Result<(), String> {
        self.is_simple_lookup = true;
        let mut gradients_tot: i32 = 1;
        let (mut ave_a, mut ave_r, mut ave_g, mut ave_b) = (0x008000i32, 0x008000i32, 0x008000i32, 0x008000i32);

        for i in 0..self.gradients.len() {
            let n_gradients = ((self.normalized_intervals[i] / imin) * 255f32) as i32;
            gradients_tot = gradients_tot.wrapping_add(n_gradients);
            if n_gradients < 0 {
                return Err(format!("java.lang.NegativeArraySizeException: {n_gradients}"));
            }
            let mut g = vec![0i32; n_gradients as usize];
            let rgb1 = lo_colors[i];
            let rgb2 = hi_colors[i];
            interpolate(rgb1, rgb2, &mut g);
            let argb = at(&g, GRADIENT_SIZE / 2)?;
            self.gradients[i] = Some(g);
            let norm = self.normalized_intervals[i];
            ave_a = ave_a.wrapping_add((((argb >> 8) & 0xFF0000) as f32 * norm) as i32);
            ave_r = ave_r.wrapping_add(((argb & 0xFF0000) as f32 * norm) as i32);
            ave_g = ave_g.wrapping_add((((argb << 8) & 0xFF0000) as f32 * norm) as i32);
            ave_b = ave_b.wrapping_add((((argb << 16) & 0xFF0000) as f32 * norm) as i32);
            self.transparency_test &= rgb1;
            self.transparency_test &= rgb2;
        }

        self.gradient_average = ((ave_a & 0xFF0000) << 8) | (ave_r & 0xFF0000) | ((ave_g & 0xFF0000) >> 8) | ((ave_b & 0xFF0000) >> 16);

        if gradients_tot < 0 {
            return Err(format!("java.lang.NegativeArraySizeException: {gradients_tot}"));
        }
        let mut gradient = vec![0i32; gradients_tot as usize];
        let mut cur_offset = 0usize;
        for g in &self.gradients {
            let g = g.as_deref().unwrap_or(&[]);
            if cur_offset + g.len() > gradient.len() {
                return Err(format!("java.lang.ArrayIndexOutOfBoundsException: arraycopy: last destination index {} out of bounds for int[{}]", cur_offset + g.len(), gradient.len()));
            }
            gradient[cur_offset..cur_offset + g.len()].copy_from_slice(g);
            cur_offset += g.len();
        }
        let last = gradient.len() as i32 - 1;
        if last < 0 {
            return Err(aioobe(last, gradient.len()));
        }
        gradient[last as usize] = hi_colors[hi_colors.len() - 1];

        if self.color_space == LINEAR_RGB {
            let l2s = &tables().linear_rgb_to_srgb;
            for v in gradient.iter_mut() {
                *v = convert_entire_color(*v, l2s);
            }
            self.gradient_average = convert_entire_color(self.gradient_average, l2s);
        }

        self.fast_gradient_array_size = gradient.len() as i32 - 1;
        self.gradient = gradient;
        Ok(())
    }

    /// `calculateMultipleArrayGradient` (the slow lookup).
    fn calculate_multiple_array_gradient(&mut self, lo_colors: &[i32], hi_colors: &[i32]) -> Result<(), String> {
        self.is_simple_lookup = false;
        let (mut ave_a, mut ave_r, mut ave_g, mut ave_b) = (0x008000i32, 0x008000i32, 0x008000i32, 0x008000i32);

        for i in 0..self.gradients.len() {
            if self.normalized_intervals[i] == 0.0 {
                continue;
            }
            let mut g = vec![0i32; GRADIENT_SIZE as usize];
            let rgb1 = lo_colors[i];
            let rgb2 = hi_colors[i];
            interpolate(rgb1, rgb2, &mut g);
            let argb = g[(GRADIENT_SIZE / 2) as usize];
            self.gradients[i] = Some(g);
            let norm = self.normalized_intervals[i];
            ave_a = ave_a.wrapping_add((((argb >> 8) & 0xFF0000) as f32 * norm) as i32);
            ave_r = ave_r.wrapping_add(((argb & 0xFF0000) as f32 * norm) as i32);
            ave_g = ave_g.wrapping_add((((argb << 8) & 0xFF0000) as f32 * norm) as i32);
            ave_b = ave_b.wrapping_add((((argb << 16) & 0xFF0000) as f32 * norm) as i32);
            self.transparency_test &= rgb1;
            self.transparency_test &= rgb2;
        }

        self.gradient_average = ((ave_a & 0xFF0000) << 8) | (ave_r & 0xFF0000) | ((ave_g & 0xFF0000) >> 8) | ((ave_b & 0xFF0000) >> 16);

        if self.color_space == LINEAR_RGB {
            let l2s = &tables().linear_rgb_to_srgb;
            for g in self.gradients.iter_mut() {
                for v in g.as_mut().ok_or(NPE)?.iter_mut() {
                    *v = convert_entire_color(*v, l2s);
                }
            }
            self.gradient_average = convert_entire_color(self.gradient_average, l2s);
        }
        Ok(())
    }

    /// `gradients[i]`.
    #[inline]
    pub fn row(&self, i: i32) -> Result<&[i32], String> {
        if i < 0 || i as usize >= self.gradients.len() {
            return Err(aioobe(i, self.gradients.len()));
        }
        self.gradients[i as usize].as_deref().ok_or_else(|| NPE.to_string())
    }

    /// `indexIntoGradientsArrays`.
    pub fn index_into_gradients_arrays(&self, position: f32) -> Result<i32, String> {
        let mut position = position;
        let fr = &self.fractions;
        let ni = &self.normalized_intervals;
        if self.cycle_method == NO_CYCLE {
            if position >= 1.0 {
                return Ok(self.gradient_overflow);
            } else if position <= 0.0 {
                return Ok(self.gradient_underflow);
            }
        } else if self.cycle_method == REPEAT {
            position -= (position as i32) as f32;
            if position < 0.0 {
                position += 1.0;
            }
            let (mut w, mut c1, mut c2) = (0i32, 0i32, 0i32);
            if self.is_simple_lookup {
                let glen = self.gradient.len() as i32;
                position *= glen as f32;
                let idx1 = position as i32;
                if idx1.wrapping_add(1) < glen {
                    return at(&self.gradient, idx1);
                }
                w = ((position - idx1 as f32) * 65536f32) as i32;
                c1 = at(&self.gradient, idx1)?;
                c2 = at(&self.gradient, 0)?;
            } else {
                for i in 0..self.gradients_length {
                    let iu = i as usize;
                    if position < fr[iu + 1] {
                        let mut delta = position - fr[iu];
                        delta = (delta / ni[iu]) * GRADIENT_SIZE as f32;
                        let index = delta as i32;
                        let row = self.row(i)?;
                        if index.wrapping_add(1) < row.len() as i32 || i + 1 < self.gradients_length {
                            return at(row, index);
                        }
                        w = ((delta - index as f32) * 65536f32) as i32;
                        c1 = at(row, index)?;
                        c2 = at(self.row(0)?, 0)?;
                        break;
                    }
                }
            }
            let a = ((((c1 >> 8) & 0xFF0000).wrapping_add((ushr(c2, 24) - ushr(c1, 24)).wrapping_mul(w))) & 0xFF0000) << 8;
            let r = (c1 & 0xFF0000).wrapping_add((((c2 >> 16) & 0xFF) - ((c1 >> 16) & 0xFF)).wrapping_mul(w)) & 0xFF0000;
            let g = (((c1 << 8) & 0xFF0000).wrapping_add((((c2 >> 8) & 0xFF) - ((c1 >> 8) & 0xFF)).wrapping_mul(w)) & 0xFF0000) >> 8;
            let b = (((c1 << 16) & 0xFF0000).wrapping_add(((c2 & 0xFF) - (c1 & 0xFF)).wrapping_mul(w)) & 0xFF0000) >> 16;
            return Ok(a | r | g | b);
        } else {
            if position < 0.0 {
                position = -position;
            }
            let part = position as i32;
            position -= part as f32;
            if (part & 1) == 1 {
                position = 1.0 - position;
            }
        }

        if self.is_simple_lookup {
            return at(&self.gradient, (position * self.fast_gradient_array_size as f32) as i32);
        }
        for i in 0..self.gradients_length {
            let iu = i as usize;
            if position < fr[iu + 1] {
                let delta = position - fr[iu];
                let index = ((delta / ni[iu]) * GRADIENT_SIZE_INDEX as f32) as i32;
                return at(self.row(i)?, index);
            }
        }
        Ok(self.gradient_overflow)
    }

    /// `indexGradientAntiAlias`.
    pub fn index_gradient_anti_alias(&self, position: f32, sz: f32) -> Result<i32, String> {
        let mut sz = sz;
        if self.cycle_method == NO_CYCLE {
            let p1 = position - (sz / 2.0);
            let p2 = position + (sz / 2.0);
            if p1 >= 1.0 {
                return Ok(self.gradient_overflow);
            }
            if p2 <= 0.0 {
                return Ok(self.gradient_underflow);
            }
            let interior;
            let frac: f32;
            let mut top_weight = 0f32;
            let mut bottom_weight = 0f32;
            if p2 >= 1.0 {
                top_weight = (p2 - 1.0) / sz;
                if p1 <= 0.0 {
                    bottom_weight = -p1 / sz;
                    frac = 1.0;
                    interior = self.gradient_average;
                } else {
                    frac = 1.0 - p1;
                    interior = self.get_anti_alias(p1, true, 1.0, false, 1.0 - p1, 1.0)?;
                }
            } else if p1 <= 0.0 {
                bottom_weight = -p1 / sz;
                frac = p2;
                interior = self.get_anti_alias(0.0, true, p2, false, p2, 1.0)?;
            } else {
                return self.get_anti_alias(p1, true, p2, false, sz, 1.0);
            }

            let mut norm = ((65536f32 * frac) / sz) as i32;
            let mut p_a = ((ushr(interior, 20) & 0xFF0).wrapping_mul(norm)) >> 16;
            let mut p_r = (((interior >> 12) & 0xFF0).wrapping_mul(norm)) >> 16;
            let mut p_g = (((interior >> 4) & 0xFF0).wrapping_mul(norm)) >> 16;
            let mut p_b = (((interior << 4) & 0xFF0).wrapping_mul(norm)) >> 16;

            if bottom_weight != 0.0 {
                let b_pix = self.gradient_underflow;
                norm = (65536f32 * bottom_weight) as i32;
                p_a = p_a.wrapping_add(((ushr(b_pix, 20) & 0xFF0).wrapping_mul(norm)) >> 16);
                p_r = p_r.wrapping_add((((b_pix >> 12) & 0xFF0).wrapping_mul(norm)) >> 16);
                p_g = p_g.wrapping_add((((b_pix >> 4) & 0xFF0).wrapping_mul(norm)) >> 16);
                p_b = p_b.wrapping_add((((b_pix << 4) & 0xFF0).wrapping_mul(norm)) >> 16);
            }
            if top_weight != 0.0 {
                let t_pix = self.gradient_overflow;
                norm = (65536f32 * top_weight) as i32;
                p_a = p_a.wrapping_add(((ushr(t_pix, 20) & 0xFF0).wrapping_mul(norm)) >> 16);
                p_r = p_r.wrapping_add((((t_pix >> 12) & 0xFF0).wrapping_mul(norm)) >> 16);
                p_g = p_g.wrapping_add((((t_pix >> 4) & 0xFF0).wrapping_mul(norm)) >> 16);
                p_b = p_b.wrapping_add((((t_pix << 4) & 0xFF0).wrapping_mul(norm)) >> 16);
            }
            return Ok(((p_a & 0xFF0) << 20) | ((p_r & 0xFF0) << 12) | ((p_g & 0xFF0) << 4) | ((p_b & 0xFF0) >> 4));
        }

        let int_sz = sz as i32;
        let mut weight = 1f32;
        if int_sz != 0 {
            sz -= int_sz as f32;
            weight = sz / (int_sz as f32 + sz);
            if (weight as f64) < 0.1 {
                return Ok(self.gradient_average);
            }
        }
        if sz as f64 > 0.99 {
            return Ok(self.gradient_average);
        }

        let mut p1 = position - (sz / 2.0);
        let mut p2 = position + (sz / 2.0);
        let mut p1_up = true;
        let mut p2_up = false;

        if self.cycle_method == REPEAT {
            p1 -= (p1 as i32) as f32;
            p2 -= (p2 as i32) as f32;
            if p1 < 0.0 {
                p1 += 1.0;
            }
            if p2 < 0.0 {
                p2 += 1.0;
            }
        } else {
            if p2 < 0.0 {
                p1 = -p1;
                p1_up = !p1_up;
                p2 = -p2;
                p2_up = !p2_up;
            } else if p1 < 0.0 {
                p1 = -p1;
                p1_up = !p1_up;
            }
            let part1 = p1 as i32;
            p1 -= part1 as f32;
            let part2 = p2 as i32;
            p2 -= part2 as f32;
            if (part1 & 0x01) == 1 {
                p1 = 1.0 - p1;
                p1_up = !p1_up;
            }
            if (part2 & 0x01) == 1 {
                p2 = 1.0 - p2;
                p2_up = !p2_up;
            }
            if (p1 > p2) && !p1_up && p2_up {
                std::mem::swap(&mut p1, &mut p2);
                p1_up = true;
                p2_up = false;
            }
        }
        self.get_anti_alias(p1, p1_up, p2, p2_up, sz, weight)
    }

    /// `getAntiAlias`.
    fn get_anti_alias(&self, p1: f32, p1_up: bool, p2: f32, p2_up: bool, sz: f32, weight: f32) -> Result<i32, String> {
        let (mut p1, mut p2) = (p1, p2);
        let (mut ach, mut rch, mut gch, mut bch) = (0i32, 0i32, 0i32, 0i32);
        macro_rules! add_whole {
            ($pix:expr) => {{
                let pix = $pix;
                ach = ach.wrapping_add(ushr(pix, 20) & 0xFF0);
                rch = rch.wrapping_add(ushr(pix, 12) & 0xFF0);
                gch = gch.wrapping_add(ushr(pix, 4) & 0xFF0);
                bch = bch.wrapping_add((pix << 4) & 0xFF0);
            }};
        }
        macro_rules! add_part {
            ($pix:expr, $norm:expr) => {{
                let pix = $pix;
                let norm = $norm;
                ach = ach.wrapping_add(((ushr(pix, 20) & 0xFF0).wrapping_mul(norm)) >> 16);
                rch = rch.wrapping_add(((ushr(pix, 12) & 0xFF0).wrapping_mul(norm)) >> 16);
                gch = gch.wrapping_add(((ushr(pix, 4) & 0xFF0).wrapping_mul(norm)) >> 16);
                bch = bch.wrapping_add((((pix << 4) & 0xFF0).wrapping_mul(norm)) >> 16);
            }};
        }

        if self.is_simple_lookup {
            let fgas = self.fast_gradient_array_size;
            let grad = &self.gradient;
            p1 *= fgas as f32;
            p2 *= fgas as f32;
            let idx1 = p1 as i32;
            let idx2 = p2 as i32;

            if p1_up && !p2_up && (idx1 <= idx2) {
                if idx1 == idx2 {
                    return at(grad, idx1);
                }
                let mut i = idx1.wrapping_add(1);
                while i < idx2 {
                    add_whole!(at(grad, i)?);
                    i += 1;
                }
            } else {
                if p1_up {
                    let mut i = idx1.wrapping_add(1);
                    while i < fgas {
                        add_whole!(at(grad, i)?);
                        i += 1;
                    }
                } else {
                    let mut i = 0;
                    while i < idx1 {
                        add_whole!(at(grad, i)?);
                        i += 1;
                    }
                }
                if p2_up {
                    let mut i = idx2.wrapping_add(1);
                    while i < fgas {
                        add_whole!(at(grad, i)?);
                        i += 1;
                    }
                } else {
                    let mut i = 0;
                    while i < idx2 {
                        add_whole!(at(grad, i)?);
                        i += 1;
                    }
                }
            }

            let isz = (65536f32 / (sz * fgas as f32)) as i32;
            ach = ach.wrapping_mul(isz) >> 16;
            rch = rch.wrapping_mul(isz) >> 16;
            gch = gch.wrapping_mul(isz) >> 16;
            bch = bch.wrapping_mul(isz) >> 16;

            let norm = if p1_up { ((1.0 - (p1 - idx1 as f32)) * isz as f32) as i32 } else { ((p1 - idx1 as f32) * isz as f32) as i32 };
            add_part!(at(grad, idx1)?, norm);
            let norm = if p2_up { ((1.0 - (p2 - idx2 as f32)) * isz as f32) as i32 } else { ((p2 - idx2 as f32) * isz as f32) as i32 };
            add_part!(at(grad, idx2)?, norm);

            ach = ach.wrapping_add(0x08) >> 4;
            rch = rch.wrapping_add(0x08) >> 4;
            gch = gch.wrapping_add(0x08) >> 4;
            bch = bch.wrapping_add(0x08) >> 4;
        } else {
            let fr = &self.fractions;
            let ni = &self.normalized_intervals;
            let gl = self.gradients_length;
            let (mut idx1, mut idx2, mut i1, mut i2) = (0i32, 0i32, -1i32, -1i32);
            let (mut f1, mut f2) = (0f32, 0f32);
            for i in 0..gl {
                let iu = i as usize;
                if p1 < fr[iu + 1] && i1 == -1 {
                    i1 = i;
                    f1 = p1 - fr[iu];
                    f1 = (f1 / ni[iu]) * GRADIENT_SIZE_INDEX as f32;
                    idx1 = f1 as i32;
                    if i2 != -1 {
                        break;
                    }
                }
                if p2 < fr[iu + 1] && i2 == -1 {
                    i2 = i;
                    f2 = p2 - fr[iu];
                    f2 = (f2 / ni[iu]) * GRADIENT_SIZE_INDEX as f32;
                    idx2 = f2 as i32;
                    if i1 != -1 {
                        break;
                    }
                }
            }
            if i1 == -1 {
                i1 = self.gradients.len() as i32 - 1;
                idx1 = GRADIENT_SIZE_INDEX;
                f1 = idx1 as f32;
            }
            if i2 == -1 {
                i2 = self.gradients.len() as i32 - 1;
                idx2 = GRADIENT_SIZE_INDEX;
                f2 = idx2 as f32;
            }

            if (i1 == i2) && (idx1 <= idx2) && p1_up && !p2_up {
                return at(self.row(i1)?, idx1.wrapping_add(idx2).wrapping_add(1) >> 1);
            }

            let gsi = GRADIENT_SIZE_INDEX as f32;
            let base = (65536f32 / sz) as i32;
            let basef = base as f32;
            let mid = GRADIENT_SIZE >> 1;
            if (i1 < i2) && p1_up && !p2_up {
                let norm = ((basef * ni[i1 as usize] * (gsi - f1)) / gsi) as i32;
                add_part!(at(self.row(i1)?, idx1.wrapping_add(GRADIENT_SIZE) >> 1)?, norm);
                for i in i1 + 1..i2 {
                    let norm = (basef * ni[i as usize]) as i32;
                    add_part!(at(self.row(i)?, mid)?, norm);
                }
                let norm = ((basef * ni[i2 as usize] * f2) / gsi) as i32;
                add_part!(at(self.row(i2)?, idx2.wrapping_add(1) >> 1)?, norm);
            } else {
                let (norm, pix) = if p1_up {
                    (((basef * ni[i1 as usize] * (gsi - f1)) / gsi) as i32, at(self.row(i1)?, idx1.wrapping_add(GRADIENT_SIZE) >> 1)?)
                } else {
                    (((basef * ni[i1 as usize] * f1) / gsi) as i32, at(self.row(i1)?, idx1.wrapping_add(1) >> 1)?)
                };
                add_part!(pix, norm);
                let (norm, pix) = if p2_up {
                    (((basef * ni[i2 as usize] * (gsi - f2)) / gsi) as i32, at(self.row(i2)?, idx2.wrapping_add(GRADIENT_SIZE) >> 1)?)
                } else {
                    (((basef * ni[i2 as usize] * f2) / gsi) as i32, at(self.row(i2)?, idx2.wrapping_add(1) >> 1)?)
                };
                add_part!(pix, norm);
                if p1_up {
                    for i in i1 + 1..gl {
                        let norm = (basef * ni[i as usize]) as i32;
                        add_part!(at(self.row(i)?, mid)?, norm);
                    }
                } else {
                    for i in 0..i1 {
                        let norm = (basef * ni[i as usize]) as i32;
                        add_part!(at(self.row(i)?, mid)?, norm);
                    }
                }
                if p2_up {
                    for i in i2 + 1..gl {
                        let norm = (basef * ni[i as usize]) as i32;
                        add_part!(at(self.row(i)?, mid)?, norm);
                    }
                } else {
                    for i in 0..i2 {
                        let norm = (basef * ni[i as usize]) as i32;
                        add_part!(at(self.row(i)?, mid)?, norm);
                    }
                }
            }
            ach = ach.wrapping_add(0x08) >> 4;
            rch = rch.wrapping_add(0x08) >> 4;
            gch = gch.wrapping_add(0x08) >> 4;
            bch = bch.wrapping_add(0x08) >> 4;
        }

        if weight != 1.0 {
            let ga = self.gradient_average;
            let ave_w = (65536f32 * (1.0 - weight)) as i32;
            let ave_a = (ushr(ga, 24) & 0xFF).wrapping_mul(ave_w);
            let ave_r = ((ga >> 16) & 0xFF).wrapping_mul(ave_w);
            let ave_g = ((ga >> 8) & 0xFF).wrapping_mul(ave_w);
            let ave_b = (ga & 0xFF).wrapping_mul(ave_w);
            let iw = (weight * 65536f32) as i32;
            ach = ach.wrapping_mul(iw).wrapping_add(ave_a) >> 16;
            rch = rch.wrapping_mul(iw).wrapping_add(ave_r) >> 16;
            gch = gch.wrapping_mul(iw).wrapping_add(ave_g) >> 16;
            bch = bch.wrapping_mul(iw).wrapping_add(ave_b) >> 16;
        }
        Ok((ach << 24) | (rch << 16) | (gch << 8) | bch)
    }
}

/// `interpolate`.
fn interpolate(rgb1: i32, rgb2: i32, output: &mut [i32]) {
    let step_size = 1f32 / output.len() as f32;
    let a1 = (rgb1 >> 24) & 0xff;
    let r1 = (rgb1 >> 16) & 0xff;
    let g1 = (rgb1 >> 8) & 0xff;
    let b1 = rgb1 & 0xff;
    let da = ((rgb2 >> 24) & 0xff) - a1;
    let dr = ((rgb2 >> 16) & 0xff) - r1;
    let dg = ((rgb2 >> 8) & 0xff) - g1;
    let db = (rgb2 & 0xff) - b1;
    let ch = |c1: i32, d: i32, i: i32| ((c1 as f32 + (i * d) as f32 * step_size) as f64 + 0.5) as i32;
    for (i, o) in output.iter_mut().enumerate() {
        let i = i as i32;
        *o = (ch(a1, da, i) << 24) | (ch(r1, dr, i) << 16) | (ch(g1, dg, i) << 8) | ch(b1, db, i);
    }
}

/// A `WritableRaster` from `createCompatibleWritableRaster` (scanline stride = width).
pub(crate) struct Raster {
    pub data: Vec<i32>,
    pub width: i32,
    pub height: i32,
}

thread_local! {
    /// The static `cachedModel` (`true` = `srgbmodel_A`) and `cached` raster; the `WeakReference` is never cleared.
    static CACHED: RefCell<(Option<bool>, Option<Raster>)> = const { RefCell::new((None, None)) };
}

/// `getCachedRaster`.
fn get_cached_raster(cm: bool, w: i32, h: i32) -> Raster {
    let hit = CACHED.with(|c| {
        let mut c = c.borrow_mut();
        if c.0 == Some(cm) {
            if let Some(r) = &c.1 {
                if r.width >= w && r.height >= h {
                    return c.1.take();
                }
            }
        }
        None
    });
    if let Some(r) = hit {
        return r;
    }
    let (w, h) = (w.max(32), h.max(32));
    Raster { data: vec![0; (w as usize) * (h as usize)], width: w, height: h }
}

/// `putCachedRaster`.
fn put_cached_raster(cm: bool, ras: Raster) {
    CACHED.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(cras) = &c.1 {
            let (cw, ch, iw, ih) = (cras.width, cras.height, ras.width, ras.height);
            if cw >= iw && ch >= ih {
                return;
            }
            if cw.wrapping_mul(ch) >= iw.wrapping_mul(ih) {
                return;
            }
        }
        *c = (Some(cm), Some(ras));
    });
}

/// The paint `createContext` is called on.
#[derive(Clone, Copy)]
pub enum GradientPaintRef<'a> {
    Linear(&'a LinearGradientPaint),
    Radial(&'a RadialGradientPaint),
}

enum Kind {
    Linear(LinearGradientPaintContext),
    Radial(RadialGradientPaintContext),
}

/// The `PaintContext` a Batik gradient paint returns.
pub struct GradientPaintContext {
    kind: Kind,
    saved: Option<Raster>,
}

impl GradientPaintContext {
    /// `paint.createContext(cm, deviceBounds, userBounds, xform, hints)` with `cm` the `TYPE_INT_ARGB` model.
    pub fn create(paint: GradientPaintRef, _device_bounds: (i32, i32, i32, i32), _user_bounds: (f64, f64, f64, f64), xform: &AffineTransform, hints: GradientHints) -> Result<GradientPaintContext, String> {
        let kind = match paint {
            GradientPaintRef::Linear(p) => {
                let mut transform = *xform;
                transform.concatenate(&p.base.gradient_transform);
                let colors: Vec<i32> = p.base.colors.iter().map(|c| c.value).collect();
                match LinearGradientPaintContext::new(&transform, hints, p.start, p.end, &p.base.fractions, &colors, p.base.cycle_method, p.base.color_space)? {
                    Some(c) => Kind::Linear(c),
                    None => return Err("java.lang.IllegalArgumentException: transform should beinvertible".into()),
                }
            }
            GradientPaintRef::Radial(p) => {
                let mut transform = *xform;
                transform.concatenate(&p.base.gradient_transform);
                let colors: Vec<i32> = p.base.colors.iter().map(|c| c.value).collect();
                let (cx, cy) = (p.center.0 as f32, p.center.1 as f32);
                let (fx, fy) = (p.focus.0 as f32, p.focus.1 as f32);
                match RadialGradientPaintContext::new(&transform, hints, cx, cy, p.radius, fx, fy, &p.base.fractions, &colors, p.base.cycle_method, p.base.color_space)? {
                    Some(c) => Kind::Radial(c),
                    None => return Err("java.lang.IllegalArgumentException: transform should be invertible".into()),
                }
            }
        };
        Ok(GradientPaintContext { kind, saved: None })
    }

    fn base(&self) -> &MultipleGradientPaintContext {
        match &self.kind {
            Kind::Linear(c) => &c.base,
            Kind::Radial(c) => &c.base,
        }
    }

    /// `getColorModel()`: (has_alpha, is_alpha_premultiplied, linear_rgb_space).
    pub fn color_model(&self) -> (bool, bool, bool) {
        (self.base().data_has_alpha, false, false)
    }

    /// `getRaster(x, y, w, h)`: the raster's whole pixel array and its scanline stride (`null` is an empty array).
    pub fn get_raster(&mut self, x: i32, y: i32, w: i32, h: i32) -> Result<(&[i32], usize), String> {
        if w == 0 || h == 0 {
            return Ok((&[], 0));
        }
        let reuse = matches!(&self.saved, Some(r) if r.width >= w && r.height >= h);
        if !reuse {
            let cm = self.base().data_has_alpha;
            self.saved = Some(get_cached_raster(cm, w, h));
        }
        let raster = self.saved.as_mut().unwrap();
        let stride = raster.width;
        let adjust = stride - w;
        match &self.kind {
            Kind::Linear(c) => c.fill_raster(&mut raster.data, 0, adjust, x, y, w, h)?,
            Kind::Radial(c) => c.fill_raster(&mut raster.data, 0, adjust, x, y, w, h)?,
        }
        Ok((&raster.data, stride as usize))
    }

    /// `dispose()`: hands the saved raster to the static cache, as `AlphaPaintPipe.endSequence` does.
    pub fn dispose(&mut self) {
        if let Some(r) = self.saved.take() {
            put_cached_raster(self.base().data_has_alpha, r);
        }
    }
}
