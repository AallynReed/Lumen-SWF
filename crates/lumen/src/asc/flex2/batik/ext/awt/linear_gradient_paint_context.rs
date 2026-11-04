//! Port of Batik's `ext.awt.LinearGradientPaintContext`.

use crate::asc::flex2::batik::ext::awt::multiple_gradient_paint_context::{at, atf, GradientHints, HintValue, MultipleGradientPaintContext, GRADIENT_SIZE_INDEX, NPE};
use crate::asc::flex2::batik::ext::awt::{NO_CYCLE, REPEAT};
use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::AffineTransform;

const DEFAULT_IMPL: i32 = 1;
const ANTI_ALIAS_IMPL: i32 = 3;

/// `LinearGradientPaintContext`.
pub(crate) struct LinearGradientPaintContext {
    pub base: MultipleGradientPaintContext,
    dgd_x: f32,
    dgd_y: f32,
    gc: f32,
    pix_sz: f32,
    fill_method: i32,
}

impl LinearGradientPaintContext {
    /// The constructor; `Ok(None)` is a `NoninvertibleTransformException`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(t: &AffineTransform, hints: GradientHints, d_start: (f64, f64), d_end: (f64, f64), fractions: &[f32], colors: &[i32], cycle_method: i32, color_space: i32) -> Result<Option<Self>, String> {
        let base = match MultipleGradientPaintContext::new(t, fractions, colors, cycle_method, color_space)? {
            Some(b) => b,
            None => return Ok(None),
        };
        let (start_x, start_y) = (d_start.0 as f32, d_start.1 as f32);
        let (end_x, end_y) = (d_end.0 as f32, d_end.1 as f32);

        let dx = end_x - start_x;
        let dy = end_y - start_y;
        let d_sq = dx * dx + dy * dy;
        let const_x = dx / d_sq;
        let const_y = dy / d_sq;
        let dgd_x = base.a00 * const_x + base.a10 * const_y;
        let dgd_y = base.a01 * const_x + base.a11 * const_y;
        let dgd_x_abs = dgd_x.abs();
        let dgd_y_abs = dgd_y.abs();
        let pix_sz = if dgd_x_abs > dgd_y_abs { dgd_x_abs } else { dgd_y_abs };
        let gc = (base.a02 - start_x) * const_x + (base.a12 - start_y) * const_y;

        let mut fill_method = DEFAULT_IMPL;
        if cycle_method == REPEAT || base.has_discontinuity {
            if hints.rendering == HintValue::Quality {
                fill_method = ANTI_ALIAS_IMPL;
            }
            if hints.color_rendering == HintValue::Speed {
                fill_method = DEFAULT_IMPL;
            } else if hints.color_rendering == HintValue::Quality {
                fill_method = ANTI_ALIAS_IMPL;
            }
        }
        Ok(Some(LinearGradientPaintContext { base, dgd_x, dgd_y, gc, pix_sz, fill_method }))
    }

    /// `fillHardNoCycle`.
    #[allow(clippy::too_many_arguments)]
    fn fill_hard_no_cycle(&self, pixels: &mut [i32], off: i32, adjust: i32, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
        let b = &self.base;
        let dgd_x = self.dgd_x;
        let gsi = GRADIENT_SIZE_INDEX as f32;
        let init_const = (dgd_x * x as f32) + self.gc;
        let mut off = off;
        for i in 0..h {
            let mut g = init_const + self.dgd_y * y.wrapping_add(i) as f32;
            let row_limit = off + w;
            if dgd_x == 0.0 {
                let val = if g <= 0.0 {
                    b.gradient_underflow
                } else if g >= 1.0 {
                    b.gradient_overflow
                } else {
                    let mut grad_idx = 0;
                    while grad_idx < b.gradients_length - 1 {
                        if g < atf(&b.fractions, grad_idx + 1)? {
                            break;
                        }
                        grad_idx += 1;
                    }
                    let delta = g - atf(&b.fractions, grad_idx)?;
                    let idx = ((delta * gsi) / atf(&b.normalized_intervals, grad_idx)?) + 0.5f32;
                    at(b.row(grad_idx)?, idx as i32)?
                };
                while off < row_limit {
                    pixels[off as usize] = val;
                    off += 1;
                }
            } else {
                let grad_steps_f: f32;
                let pre_grad_steps_f: f32;
                let (pre_val, post_val);
                if dgd_x >= 0.0 {
                    grad_steps_f = (1.0 - g) / dgd_x;
                    pre_grad_steps_f = (((0.0 - g) / dgd_x) as f64).ceil() as f32;
                    pre_val = b.gradient_underflow;
                    post_val = b.gradient_overflow;
                } else {
                    grad_steps_f = (0.0 - g) / dgd_x;
                    pre_grad_steps_f = (((1.0 - g) / dgd_x) as f64).ceil() as f32;
                    pre_val = b.gradient_overflow;
                    post_val = b.gradient_underflow;
                }
                let grad_steps = if grad_steps_f > w as f32 { w } else { grad_steps_f as i32 };
                let pre_grad_steps = if pre_grad_steps_f > w as f32 { w } else { pre_grad_steps_f as i32 };
                let grad_limit = off.wrapping_add(grad_steps);
                if pre_grad_steps > 0 {
                    let pre_grad_limit = off + pre_grad_steps;
                    while off < pre_grad_limit {
                        pixels[off as usize] = pre_val;
                        off += 1;
                    }
                    g += dgd_x * pre_grad_steps as f32;
                }
                if dgd_x > 0.0 {
                    let mut grad_idx = 0;
                    while grad_idx < b.gradients_length - 1 {
                        if g < atf(&b.fractions, grad_idx + 1)? {
                            break;
                        }
                        grad_idx += 1;
                    }
                    while off < grad_limit {
                        let delta = g - atf(&b.fractions, grad_idx)?;
                        let grad = grad_ref(b, grad_idx)?;
                        let steps_d = (((atf(&b.fractions, grad_idx + 1)? - g) / dgd_x) as f64).ceil();
                        let steps = if steps_d > w as f64 { w } else { steps_d as i32 };
                        let mut sub_grad_limit = off.wrapping_add(steps);
                        if sub_grad_limit > grad_limit {
                            sub_grad_limit = grad_limit;
                        }
                        let ni = atf(&b.normalized_intervals, grad_idx)?;
                        let mut idx = ((((delta * gsi) / ni) * 65536f32) as i32).wrapping_add(1 << 15);
                        let step = (((dgd_x * gsi) / ni) * 65536f32) as i32;
                        while off < sub_grad_limit {
                            pixels[off as usize] = at(grad.ok_or(NPE)?, idx >> 16)?;
                            off += 1;
                            idx = idx.wrapping_add(step);
                        }
                        g = (g as f64 + dgd_x as f64 * steps_d) as f32;
                        grad_idx += 1;
                    }
                } else {
                    let mut grad_idx = b.gradients_length - 1;
                    while grad_idx > 0 {
                        if g > atf(&b.fractions, grad_idx)? {
                            break;
                        }
                        grad_idx -= 1;
                    }
                    while off < grad_limit {
                        let delta = g - atf(&b.fractions, grad_idx)?;
                        let grad = grad_ref(b, grad_idx)?;
                        let steps_d = ((delta / -dgd_x) as f64).ceil();
                        let steps = if steps_d > w as f64 { w } else { steps_d as i32 };
                        let mut sub_grad_limit = off.wrapping_add(steps);
                        if sub_grad_limit > grad_limit {
                            sub_grad_limit = grad_limit;
                        }
                        let ni = atf(&b.normalized_intervals, grad_idx)?;
                        let mut idx = ((((delta * gsi) / ni) * 65536f32) as i32).wrapping_add(1 << 15);
                        let step = (((dgd_x * gsi) / ni) * 65536f32) as i32;
                        while off < sub_grad_limit {
                            pixels[off as usize] = at(grad.ok_or(NPE)?, idx >> 16)?;
                            off += 1;
                            idx = idx.wrapping_add(step);
                        }
                        g = (g as f64 + dgd_x as f64 * steps_d) as f32;
                        grad_idx -= 1;
                    }
                }
                while off < row_limit {
                    pixels[off as usize] = post_val;
                    off += 1;
                }
            }
            off += adjust;
        }
        Ok(())
    }

    /// `fillSimpleNoCycle`.
    #[allow(clippy::too_many_arguments)]
    fn fill_simple_no_cycle(&self, pixels: &mut [i32], off: i32, adjust: i32, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
        let b = &self.base;
        let fgas = b.fast_gradient_array_size;
        let dgd_x = self.dgd_x;
        let init_const = (dgd_x * x as f32) + self.gc;
        let step = dgd_x * fgas as f32;
        let fp_step = (step * 65536f32) as i32;
        let grad = &b.gradient;
        let mut off = off;
        for i in 0..h {
            let mut g = init_const + self.dgd_y * y.wrapping_add(i) as f32;
            g *= fgas as f32;
            g = (g as f64 + 0.5) as f32;
            let row_limit = off + w;
            let mut check = dgd_x * fgas as f32 * w as f32;
            if check < 0.0 {
                check = -check;
            }
            if (check as f64) < 0.3 {
                let val = if g <= 0.0 {
                    b.gradient_underflow
                } else if g >= fgas as f32 {
                    b.gradient_overflow
                } else {
                    at(grad, g as i32)?
                };
                while off < row_limit {
                    pixels[off as usize] = val;
                    off += 1;
                }
            } else {
                let mut grad_steps;
                let mut pre_grad_steps;
                let (pre_val, post_val);
                if dgd_x > 0.0 {
                    grad_steps = ((fgas as f32 - g) / step) as i32;
                    pre_grad_steps = ((0.0 - g / step) as f64).ceil() as i32;
                    pre_val = b.gradient_underflow;
                    post_val = b.gradient_overflow;
                } else {
                    grad_steps = ((0.0 - g) / step) as i32;
                    pre_grad_steps = (((fgas as f32 - g) / step) as f64).ceil() as i32;
                    pre_val = b.gradient_overflow;
                    post_val = b.gradient_underflow;
                }
                if grad_steps > w {
                    grad_steps = w;
                }
                let grad_limit = off.wrapping_add(grad_steps);
                if pre_grad_steps > 0 {
                    if pre_grad_steps > w {
                        pre_grad_steps = w;
                    }
                    let pre_grad_limit = off + pre_grad_steps;
                    while off < pre_grad_limit {
                        pixels[off as usize] = pre_val;
                        off += 1;
                    }
                    g += step * pre_grad_steps as f32;
                }
                let mut fp_g = (g * 65536f32) as i32;
                while off < grad_limit {
                    pixels[off as usize] = at(grad, fp_g >> 16)?;
                    off += 1;
                    fp_g = fp_g.wrapping_add(fp_step);
                }
                while off < row_limit {
                    pixels[off as usize] = post_val;
                    off += 1;
                }
            }
            off += adjust;
        }
        Ok(())
    }

    /// `fillSimpleRepeat`.
    #[allow(clippy::too_many_arguments)]
    fn fill_simple_repeat(&self, pixels: &mut [i32], off: i32, adjust: i32, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
        let b = &self.base;
        let fgas = b.fast_gradient_array_size;
        let dgd_x = self.dgd_x;
        let init_const = (dgd_x * x as f32) + self.gc;
        let mut step = (dgd_x - (dgd_x as i32) as f32) * fgas as f32;
        if step < 0.0 {
            step += fgas as f32;
        }
        let grad = &b.gradient;
        let mut off = off;
        for i in 0..h {
            let mut g = init_const + self.dgd_y * y.wrapping_add(i) as f32;
            g -= (g as i32) as f32;
            if g < 0.0 {
                g += 1.0;
            }
            g *= fgas as f32;
            g = (g as f64 + 0.5) as f32;
            let row_limit = off + w;
            while off < row_limit {
                let mut idx = g as i32;
                if idx >= fgas {
                    g -= fgas as f32;
                    idx -= fgas;
                }
                pixels[off as usize] = at(grad, idx)?;
                off += 1;
                g += step;
            }
            off += adjust;
        }
        Ok(())
    }

    /// `fillSimpleReflect`.
    #[allow(clippy::too_many_arguments)]
    fn fill_simple_reflect(&self, pixels: &mut [i32], off: i32, adjust: i32, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
        let b = &self.base;
        let fgas = b.fast_gradient_array_size;
        let init_const = (self.dgd_x * x as f32) + self.gc;
        let grad = &b.gradient;
        let mut off = off;
        for i in 0..h {
            let mut g = init_const + self.dgd_y * y.wrapping_add(i) as f32;
            g -= 2i32.wrapping_mul((g / 2.0f32) as i32) as f32;
            let mut step = self.dgd_x;
            if g < 0.0 {
                g = -g;
                step = -step;
            }
            step -= 2.0 * ((step as i32) as f32 / 2.0f32);
            if step < 0.0 {
                step = (step as f64 + 2.0) as f32;
            }
            let reflect_max = 2i32.wrapping_mul(fgas);
            g *= fgas as f32;
            g = (g as f64 + 0.5) as f32;
            step *= fgas as f32;
            let row_limit = off + w;
            while off < row_limit {
                let mut idx = g as i32;
                if idx >= reflect_max {
                    g -= reflect_max as f32;
                    idx -= reflect_max;
                }
                pixels[off as usize] = if idx <= fgas { at(grad, idx)? } else { at(grad, reflect_max - idx)? };
                off += 1;
                g += step;
            }
            off += adjust;
        }
        Ok(())
    }

    /// `fillRaster`.
    #[allow(clippy::too_many_arguments)]
    pub fn fill_raster(&self, pixels: &mut [i32], off: i32, adjust: i32, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
        let b = &self.base;
        let init_const = (self.dgd_x * x as f32) + self.gc;
        if self.fill_method == ANTI_ALIAS_IMPL {
            let mut off = off;
            for i in 0..h {
                let mut g = init_const + self.dgd_y * y.wrapping_add(i) as f32;
                let row_limit = off + w;
                while off < row_limit {
                    pixels[off as usize] = b.index_gradient_anti_alias(g, self.pix_sz)?;
                    off += 1;
                    g += self.dgd_x;
                }
                off += adjust;
            }
        } else if !b.is_simple_lookup {
            if b.cycle_method == NO_CYCLE {
                self.fill_hard_no_cycle(pixels, off, adjust, x, y, w, h)?;
            } else {
                let mut off = off;
                for i in 0..h {
                    let mut g = init_const + self.dgd_y * y.wrapping_add(i) as f32;
                    let row_limit = off + w;
                    while off < row_limit {
                        pixels[off as usize] = b.index_into_gradients_arrays(g)?;
                        off += 1;
                        g += self.dgd_x;
                    }
                    off += adjust;
                }
            }
        } else if b.cycle_method == NO_CYCLE {
            self.fill_simple_no_cycle(pixels, off, adjust, x, y, w, h)?;
        } else if b.cycle_method == REPEAT {
            self.fill_simple_repeat(pixels, off, adjust, x, y, w, h)?;
        } else {
            self.fill_simple_reflect(pixels, off, adjust, x, y, w, h)?;
        }
        Ok(())
    }
}

/// `gradients[i]` loaded into a local (null is only an error once it is indexed).
fn grad_ref(b: &MultipleGradientPaintContext, i: i32) -> Result<Option<&[i32]>, String> {
    if i < 0 || i as usize >= b.gradients.len() {
        return Err(crate::asc::flex2::batik::ext::awt::multiple_gradient_paint_context::aioobe(i, b.gradients.len()));
    }
    Ok(b.gradients[i as usize].as_deref())
}
