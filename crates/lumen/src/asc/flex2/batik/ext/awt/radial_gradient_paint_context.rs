//! Port of Batik's `ext.awt.RadialGradientPaintContext`.

use crate::asc::flex2::batik::ext::awt::multiple_gradient_paint_context::{at, GradientHints, HintValue, MultipleGradientPaintContext};
use crate::asc::flex2::batik::ext::awt::NO_CYCLE;
use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::AffineTransform;
use crate::asc::flex2::fonts::java2d::sun::java2d::marlin::math;

const FIXED_POINT_IMPL: i32 = 1;
const DEFAULT_IMPL: i32 = 2;
const ANTI_ALIAS_IMPL: i32 = 3;
const SCALEBACK: f32 = 0.97;
const MAX_PRECISION: i32 = 256;

/// `RadialGradientPaintContext`.
pub(crate) struct RadialGradientPaintContext {
    pub base: MultipleGradientPaintContext,
    radius: f32,
    center_x: f32,
    center_y: f32,
    focus_x: f32,
    focus_y: f32,
    radius_sq: f32,
    const_a: f32,
    const_b: f32,
    trivial: f32,
    fill_method: i32,
    inv_sq_step_float: f32,
    sqrt_lut_fixed: Vec<i32>,
}

impl RadialGradientPaintContext {
    /// The constructor; `Ok(None)` is a `NoninvertibleTransformException`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(t: &AffineTransform, hints: GradientHints, cx: f32, cy: f32, r: f32, fx: f32, fy: f32, fractions: &[f32], colors: &[i32], cycle_method: i32, color_space: i32) -> Result<Option<Self>, String> {
        let base = match MultipleGradientPaintContext::new(t, fractions, colors, cycle_method, color_space)? {
            Some(b) => b,
            None => return Ok(None),
        };
        let center_x = cx;
        let center_y = cy;
        let mut focus_x = fx;
        let mut focus_y = fy;
        let radius = r;
        let is_simple_focus = (focus_x == center_x) && (focus_y == center_y);
        let is_non_cyclic = cycle_method == NO_CYCLE;
        let radius_sq = radius * radius;
        let mut d_x = focus_x - center_x;
        let d_y = focus_y - center_y;
        let dist = (((d_x * d_x) + (d_y * d_y)) as f64).sqrt();
        if dist > (radius * SCALEBACK) as f64 {
            let angle = atan2(d_y as f64, d_x as f64);
            focus_x = ((SCALEBACK * radius) as f64 * math::cos(angle)) as f32 + center_x;
            focus_y = ((SCALEBACK * radius) as f64 * sin(angle)) as f32 + center_y;
        }
        d_x = focus_x - center_x;
        let trivial = ((radius_sq - (d_x * d_x)) as f64).sqrt() as f32;
        let const_a = base.a02 - center_x;
        let const_b = base.a12 - center_y;

        let mut fill_method = 0;
        if hints.rendering == HintValue::Quality || hints.color_rendering == HintValue::Quality {
            fill_method = ANTI_ALIAS_IMPL;
        }
        if hints.rendering == HintValue::Speed || hints.color_rendering == HintValue::Speed {
            fill_method = DEFAULT_IMPL;
        }
        if fill_method == 0 {
            fill_method = DEFAULT_IMPL;
        }

        let mut c = RadialGradientPaintContext {
            base,
            radius,
            center_x,
            center_y,
            focus_x,
            focus_y,
            radius_sq,
            const_a,
            const_b,
            trivial,
            fill_method,
            inv_sq_step_float: 0.0,
            sqrt_lut_fixed: vec![0; MAX_PRECISION as usize],
        };
        if c.fill_method == DEFAULT_IMPL && is_simple_focus && is_non_cyclic && c.base.is_simple_lookup {
            c.calculate_fixed_point_sqrt_lookup_table();
            c.fill_method = FIXED_POINT_IMPL;
        }
        Ok(Some(c))
    }

    /// `fillRaster`.
    #[allow(clippy::too_many_arguments)]
    pub fn fill_raster(&self, pixels: &mut [i32], off: i32, adjust: i32, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
        match self.fill_method {
            FIXED_POINT_IMPL => self.fixed_point_simplest_case_non_cyclic_fill_raster(pixels, off, adjust, x, y, w, h),
            ANTI_ALIAS_IMPL => self.anti_alias_fill_raster(pixels, off, adjust, x, y, w, h),
            _ => self.cyclic_circular_gradient_fill_raster(pixels, off, adjust, x, y, w, h),
        }
    }

    /// `fixedPointSimplestCaseNonCyclicFillRaster`.
    #[allow(clippy::too_many_arguments)]
    fn fixed_point_simplest_case_non_cyclic_fill_raster(&self, pixels: &mut [i32], off: i32, adjust: i32, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
        let b = &self.base;
        let fgas = b.fast_gradient_array_size;
        let index_factor = fgas as f32 / self.radius;
        let const_x = (b.a00 * x as f32) + (b.a01 * y as f32) + self.const_a;
        let const_y = (b.a10 * x as f32) + (b.a11 * y as f32) + self.const_b;
        let delta_x = index_factor * b.a00;
        let delta_y = index_factor * b.a10;
        let fixed_array_size_sq = fgas.wrapping_mul(fgas);
        let mut indexer = off;
        let temp = (delta_x * delta_x) + (delta_y * delta_y);
        let g_delta_delta = temp * 2.0;

        if temp > fixed_array_size_sq as f32 {
            let val = b.gradient_overflow;
            for _ in 0..h {
                let end = indexer + w;
                while indexer < end {
                    pixels[indexer as usize] = val;
                    indexer += 1;
                }
                indexer += adjust;
            }
            return Ok(());
        }

        let lut = &self.sqrt_lut_fixed;
        for j in 0..h {
            let d_x = index_factor * ((b.a01 * j as f32) + const_x);
            let d_y = index_factor * ((b.a11 * j as f32) + const_y);
            let mut g = (d_y * d_y) + (d_x * d_x);
            let mut g_delta = (((delta_y * d_y) + (delta_x * d_x)) * 2.0) + temp;
            let end = indexer + w;
            while indexer < end {
                if g >= fixed_array_size_sq as f32 {
                    pixels[indexer as usize] = b.gradient_overflow;
                } else {
                    let mut i_sq = g * self.inv_sq_step_float;
                    let i_sq_int = i_sq as i32;
                    i_sq -= i_sq_int as f32;
                    let mut g_index = at(lut, i_sq_int)?;
                    g_index = g_index.wrapping_add((i_sq * at(lut, i_sq_int.wrapping_add(1))?.wrapping_sub(g_index) as f32) as i32);
                    pixels[indexer as usize] = at(&b.gradient, g_index)?;
                }
                g += g_delta;
                g_delta += g_delta_delta;
                indexer += 1;
            }
            indexer += adjust;
        }
        Ok(())
    }

    /// `calculateFixedPointSqrtLookupTable`.
    fn calculate_fixed_point_sqrt_lookup_table(&mut self) {
        let fgas = self.base.fast_gradient_array_size;
        let sq_step_float = (fgas.wrapping_mul(fgas) / (MAX_PRECISION - 2)) as f32;
        let mut i = 0;
        while i < MAX_PRECISION - 1 {
            self.sqrt_lut_fixed[i as usize] = ((i as f32 * sq_step_float) as f64).sqrt() as i32;
            i += 1;
        }
        self.sqrt_lut_fixed[i as usize] = self.sqrt_lut_fixed[(i - 1) as usize];
        self.inv_sq_step_float = 1.0 / sq_step_float;
    }

    /// `cyclicCircularGradientFillRaster`.
    #[allow(clippy::too_many_arguments)]
    fn cyclic_circular_gradient_fill_raster(&self, pixels: &mut [i32], off: i32, adjust: i32, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
        let b = &self.base;
        let (center_x, center_y, focus_x, focus_y, trivial) = (self.center_x, self.center_y, self.focus_x, self.focus_y, self.trivial);
        let const_c = (-(self.radius_sq) + (center_x * center_x) + (center_y * center_y)) as f64;
        let const_x = (b.a00 * x as f32) + (b.a01 * y as f32) + b.a02;
        let const_y = (b.a10 * x as f32) + (b.a11 * y as f32) + b.a12;
        let precalc2 = 2.0 * center_y;
        let precalc3 = -2.0 * center_x;
        let mut indexer = off;
        let pix_inc = w + adjust;

        for j in 0..h {
            let mut xx = (b.a01 * j as f32) + const_x;
            let mut yy = (b.a11 * j as f32) + const_y;
            for i in 0..w {
                let (solution_x, solution_y): (f64, f64);
                if ((xx - focus_x) as f64) > -0.000001 && ((xx - focus_x) as f64) < 0.000001 {
                    solution_x = focus_x as f64;
                    solution_y = center_y as f64 + (if yy > focus_y { trivial } else { -trivial }) as f64;
                } else {
                    let slope = ((yy - focus_y) / (xx - focus_x)) as f64;
                    let yintcpt = yy as f64 - (slope * xx as f64);
                    let a = (slope * slope) + 1.0;
                    let bb = precalc3 as f64 + (-2.0 * slope * (center_y as f64 - yintcpt));
                    let c = const_c + (yintcpt * (yintcpt - precalc2 as f64));
                    let det = ((bb * bb) - (4.0 * a * c)).sqrt() as f32;
                    let mut sx = -bb;
                    sx += (if xx < focus_x { -det } else { det }) as f64;
                    sx /= 2.0 * a;
                    solution_x = sx;
                    solution_y = (slope * solution_x) + yintcpt;
                }
                let mut delta_x_sq = solution_x as f32 - focus_x;
                delta_x_sq *= delta_x_sq;
                let mut delta_y_sq = solution_y as f32 - focus_y;
                delta_y_sq *= delta_y_sq;
                let intersect_to_focus_sq = delta_x_sq + delta_y_sq;
                delta_x_sq = xx - focus_x;
                delta_x_sq *= delta_x_sq;
                delta_y_sq = yy - focus_y;
                delta_y_sq *= delta_y_sq;
                let current_to_focus_sq = delta_x_sq + delta_y_sq;
                let g = ((current_to_focus_sq / intersect_to_focus_sq) as f64).sqrt() as f32;
                pixels[(indexer + i) as usize] = b.index_into_gradients_arrays(g)?;
                xx += b.a00;
                yy += b.a10;
            }
            indexer += pix_inc;
        }
        Ok(())
    }

    /// The per-sample `g` of `antiAliasFillRaster`.
    #[inline]
    fn aa_g(&self, xx: f32, yy: f32, const_c: f64, precalc2: f32, precalc3: f32) -> f64 {
        let (center_y, focus_x, focus_y) = (self.center_y, self.focus_x, self.focus_y);
        let (solution_x, solution_y): (f64, f64);
        if ((xx - focus_x) as f64) > -0.000001 && ((xx - focus_x) as f64) < 0.000001 {
            solution_x = focus_x as f64;
            solution_y = center_y as f64 + (if yy > focus_y { self.trivial } else { -self.trivial }) as f64;
        } else {
            let slope = ((yy - focus_y) / (xx - focus_x)) as f64;
            let yintcpt = yy as f64 - (slope * xx as f64);
            let a = (slope * slope) + 1.0;
            let bb = precalc3 as f64 + (-2.0 * slope * (center_y as f64 - yintcpt));
            let c = const_c + (yintcpt * (yintcpt - precalc2 as f64));
            let det = ((bb * bb) - (4.0 * a * c)).sqrt();
            let mut sx = -bb;
            sx += if xx < focus_x { -det } else { det };
            sx /= 2.0 * a;
            solution_x = sx;
            solution_y = (slope * solution_x) + yintcpt;
        }
        let mut delta_x_sq = solution_x - focus_x as f64;
        delta_x_sq *= delta_x_sq;
        let mut delta_y_sq = solution_y - focus_y as f64;
        delta_y_sq *= delta_y_sq;
        let intersect_to_focus_sq = delta_x_sq + delta_y_sq;
        delta_x_sq = (xx - focus_x) as f64;
        delta_x_sq *= delta_x_sq;
        delta_y_sq = (yy - focus_y) as f64;
        delta_y_sq *= delta_y_sq;
        let current_to_focus_sq = delta_x_sq + delta_y_sq;
        (current_to_focus_sq / intersect_to_focus_sq).sqrt()
    }

    /// `antiAliasFillRaster`.
    #[allow(clippy::too_many_arguments)]
    fn anti_alias_fill_raster(&self, pixels: &mut [i32], off: i32, adjust: i32, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
        let b = &self.base;
        let (center_x, center_y) = (self.center_x, self.center_y);
        let const_c = (-(self.radius_sq) + (center_x * center_x) + (center_y * center_y)) as f64;
        let precalc2 = 2.0 * center_y;
        let precalc3 = -2.0 * center_x;
        let const_x = (b.a00 * (x as f32 - 0.5f32)) + (b.a01 * (y as f32 + 0.5f32)) + b.a02;
        let const_y = (b.a10 * (x as f32 - 0.5f32)) + (b.a11 * (y as f32 + 0.5f32)) + b.a12;
        let mut indexer = off - 1;
        let mut prev_gs = vec![0f64; (w + 1) as usize];

        let mut xx = const_x - b.a01;
        let mut yy = const_y - b.a11;
        for g in prev_gs.iter_mut() {
            *g = self.aa_g(xx, yy, const_c, precalc2, precalc3);
            xx += b.a00;
            yy += b.a10;
        }

        for j in 0..h {
            xx = (b.a01 * j as f32) + const_x;
            yy = (b.a11 * j as f32) + const_y;
            let mut g10 = prev_gs[0];
            let mut g11 = self.aa_g(xx, yy, const_c, precalc2, precalc3);
            prev_gs[0] = g11;
            xx += b.a00;
            yy += b.a10;
            for i in 1..=w {
                let g00 = g10;
                let g01 = g11;
                g10 = prev_gs[i as usize];
                g11 = self.aa_g(xx, yy, const_c, precalc2, precalc3);
                prev_gs[i as usize] = g11;
                let pos = ((g00 + g01 + g10 + g11) / 4.0) as f32;
                let sz = java_max((g11 - g00).abs(), (g10 - g01).abs()) as f32;
                pixels[(indexer + i) as usize] = b.index_gradient_anti_alias(pos, sz)?;
                xx += b.a00;
                yy += b.a10;
            }
            indexer += w + adjust;
        }
        Ok(())
    }
}

/// `Math.max(double, double)`.
fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && a.to_bits() == (-0.0f64).to_bits() {
        return b;
    }
    if a >= b { a } else { b }
}

fn hi(x: f64) -> i32 {
    (x.to_bits() >> 32) as i32
}

fn lo(x: f64) -> u32 {
    x.to_bits() as u32
}

/// `Math.atan2`: JDK 17 has no x86_64 intrinsic, so it is `StrictMath.atan2` (fdlibm `__ieee754_atan2`).
pub(crate) fn atan2(y: f64, x: f64) -> f64 {
    const TINY: f64 = 1.0e-300;
    let pi_o_4 = f64::from_bits(0x3FE921FB54442D18);
    let pi_o_2 = f64::from_bits(0x3FF921FB54442D18);
    let pi = f64::from_bits(0x400921FB54442D18);
    let pi_lo = f64::from_bits(0x3CA1A62633145C07);

    let hx = hi(x);
    let ix = hx & 0x7fffffff;
    let lx = lo(x);
    let hy = hi(y);
    let iy = hy & 0x7fffffff;
    let ly = lo(y);
    if ((ix as u32) | ((lx | lx.wrapping_neg()) >> 31)) > 0x7ff00000 || ((iy as u32) | ((ly | ly.wrapping_neg()) >> 31)) > 0x7ff00000 {
        return x + y;
    }
    if (hx.wrapping_sub(0x3ff00000) as u32 | lx) == 0 {
        return atan(y);
    }
    let m = ((hy >> 31) & 1) | ((hx >> 30) & 2);

    if (iy as u32 | ly) == 0 {
        match m {
            0 | 1 => return y,
            2 => return pi + TINY,
            _ => return -pi - TINY,
        }
    }
    if (ix as u32 | lx) == 0 {
        return if hy < 0 { -pi_o_2 - TINY } else { pi_o_2 + TINY };
    }
    if ix == 0x7ff00000 {
        if iy == 0x7ff00000 {
            return match m {
                0 => pi_o_4 + TINY,
                1 => -pi_o_4 - TINY,
                2 => 3.0 * pi_o_4 + TINY,
                _ => -3.0 * pi_o_4 - TINY,
            };
        } else {
            return match m {
                0 => 0.0,
                1 => -0.0,
                2 => pi + TINY,
                _ => -pi - TINY,
            };
        }
    }
    if iy == 0x7ff00000 {
        return if hy < 0 { -pi_o_2 - TINY } else { pi_o_2 + TINY };
    }

    let k = (iy - ix) >> 20;
    let z = if k > 60 {
        pi_o_2 + 0.5 * pi_lo
    } else if hx < 0 && k < -60 {
        0.0
    } else {
        atan((y / x).abs())
    };
    match m {
        0 => z,
        1 => f64::from_bits(z.to_bits() ^ 0x8000_0000_0000_0000),
        2 => pi - (z - pi_lo),
        _ => (z - pi_lo) - pi,
    }
}

/// fdlibm `atan`.
fn atan(x: f64) -> f64 {
    const ATANHI: [u64; 4] = [0x3FDDAC670561BB4F, 0x3FE921FB54442D18, 0x3FEF730BD281F69B, 0x3FF921FB54442D18];
    const ATANLO: [u64; 4] = [0x3C7A2B7F222F65E2, 0x3C81A62633145C07, 0x3C7007887AF0CBBD, 0x3C91A62633145C07];
    const AT: [u64; 11] = [
        0x3FD555555555550D,
        0xBFC999999998EBC4,
        0x3FC24924920083FF,
        0xBFBC71C6FE231671,
        0x3FB745CDC54C206E,
        0xBFB3B0F2AF749A6D,
        0x3FB10D66A0D03D51,
        0xBFADDE2D52DEFD9A,
        0x3FA97B4B24760DEB,
        0xBFA2B4442C6A6C2F,
        0x3F90AD3AE322DA11,
    ];
    let at = |i: usize| f64::from_bits(AT[i]);
    let one = 1.0f64;
    let huge = 1.0e300f64;
    let mut x = x;
    let hx = hi(x);
    let ix = hx & 0x7fffffff;
    let id: i32;
    if ix >= 0x44100000 {
        if ix > 0x7ff00000 || (ix == 0x7ff00000 && lo(x) != 0) {
            return x + x;
        }
        return if hx > 0 { f64::from_bits(ATANHI[3]) + f64::from_bits(ATANLO[3]) } else { -f64::from_bits(ATANHI[3]) - f64::from_bits(ATANLO[3]) };
    }
    if ix < 0x3fdc0000 {
        if ix < 0x3e200000 && huge + x > one {
            return x;
        }
        id = -1;
    } else {
        x = x.abs();
        if ix < 0x3ff30000 {
            if ix < 0x3fe60000 {
                id = 0;
                x = (2.0 * x - one) / (2.0 + x);
            } else {
                id = 1;
                x = (x - one) / (x + one);
            }
        } else if ix < 0x40038000 {
            id = 2;
            x = (x - 1.5) / (one + 1.5 * x);
        } else {
            id = 3;
            x = -1.0 / x;
        }
    }
    let z = x * x;
    let w = z * z;
    let s1 = z * (at(0) + w * (at(2) + w * (at(4) + w * (at(6) + w * (at(8) + w * at(10))))));
    let s2 = w * (at(1) + w * (at(3) + w * (at(5) + w * (at(7) + w * at(9)))));
    if id < 0 {
        x - x * (s1 + s2)
    } else {
        let id = id as usize;
        let z = f64::from_bits(ATANHI[id]) - ((x * (s1 + s2) - f64::from_bits(ATANLO[id])) - x);
        if hx < 0 { -z } else { z }
    }
}

/// `Math.sin`: HotSpot's x86_64 `StubRoutines::dsin` (Intel LIBM), the same kernel as `math::cos` with the table
/// index unshifted. The large-argument reduction (|x| >= about 90112, never reached here) falls back to `f64::sin`.
pub(crate) fn sin(x: f64) -> f64 {
    let hx = (x.to_bits() >> 32) as u32;
    let e = (hx & 0x7fff_0000).wrapping_sub(0x3030_0000);
    if e > 0x10c5_0000 {
        if (e as i32) <= 0x10c5_0000 {
            if e >> 20 == 3325 {
                return x * f64::from_bits(0x3fef_ffff_ffff_ffff);
            }
            return (f64::from_bits(0x4360_0000_0000_0000) * x - x) * f64::from_bits(0x3c80_0000_0000_0000);
        }
        if hx & 0x7ff0_0000 == 0x7ff0_0000 {
            return x * -0.0;
        }
        return x.sin();
    }
    let pi32inv = f64::from_bits(0x40245f306dc9c883);
    let p_1 = f64::from_bits(0x3fb921fb54400000);
    let p_2 = f64::from_bits(0x3d90b4611a600000);
    let p_3 = f64::from_bits(0x3b63198a2e037073);
    let (sc_1, sc_1h) = (f64::from_bits(0xbfc5555555555555), f64::from_bits(0xbfe0000000000000));
    let (sc_2, sc_2h) = (f64::from_bits(0x3f81111111111111), f64::from_bits(0x3fa5555555555555));
    let (sc_3, sc_3h) = (f64::from_bits(0xbf2a01a01a01a01a), f64::from_bits(0xbf56c16c16c16c17));
    let (sc_4, sc_4h) = (f64::from_bits(0x3ec71de3a556c734), f64::from_bits(0x3efa01a01a01a01a));

    let y = pi32inv * x + f64::from_bits(0x3fe0000000000000 | (x.to_bits() & 0x8000_0000_0000_0000));
    let n = y as i32;
    let nd = n as f64;
    let ct = &SIN_CTABLE[(n as u32 & 0x3f) as usize];
    let (ct0, ct1, ct2, ct3) = (f64::from_bits(ct[0]), f64::from_bits(ct[1]), f64::from_bits(ct[2]), f64::from_bits(ct[3]));

    let m1 = p_1 * nd;
    let m2 = p_2 * nd;
    let r1 = x - m1;
    let m3 = nd * p_3;
    let r = r1 - m2;
    let mut sc4 = sc_4 * r1;
    let mut sc4h = sc_4h * r1;
    let mut c7 = ct1 * r;
    let mut t3 = r1 - r;
    sc4 *= r;
    sc4h *= r;
    let r2 = r * r;
    t3 -= m2;
    let mut x1 = m3 - t3;
    let mut x2 = ct0 + ct3;
    c7 -= x2;
    x2 *= r;
    let mut sc2 = sc_2 * r2;
    let mut sc2h = sc_2h * r2;
    let mut x3 = ct3 * r;
    x2 *= r2;
    let x2h = ct1 * r2;
    let r4 = r2 * r2;
    sc4 += sc_3;
    sc4h += sc_3h;
    let mut x4 = r * ct0;
    sc2 += sc_1;
    sc2h += sc_1h;
    sc4 *= r4;
    sc4h *= r4;
    let mut x0 = x3;
    x3 += ct1;
    x1 *= c7;
    let x7 = x4;
    x4 += x3;
    sc2 += sc4;
    sc2h += sc4h;
    let x5 = ct1 - x3;
    x3 -= x4;
    x1 += ct2;
    sc2 *= x2;
    sc2h *= x2h;
    x0 += x5;
    x3 += x7;
    x0 += x1;
    x0 += x3;
    x0 += sc2;
    x0 += sc2h;
    x0 + x4
}

/// The dsin stub's `Ctable` (the dcos one).
static SIN_CTABLE: [[u64; 4]; 64] = [
    [0x0000000000000000, 0x0000000000000000, 0x0000000000000000, 0x3ff0000000000000],
    [0xbf73b92e176d6d31, 0x3fb917a6bc29b42c, 0xbc3e2718e0000000, 0x3ff0000000000000],
    [0xbf93ad06011469fb, 0x3fc8f8b83c69a60b, 0xbc626d19c0000000, 0x3ff0000000000000],
    [0xbfa60bea939d225a, 0x3fd294062ed59f06, 0xbc75d28da0000000, 0x3ff0000000000000],
    [0xbfb37ca1866b95cf, 0x3fd87de2a6aea963, 0xbc672cede0000000, 0x3ff0000000000000],
    [0xbfbe3a6873fa1279, 0x3fde2b5d3806f63b, 0x3c5e0d8920000000, 0x3ff0000000000000],
    [0xbfc592675bc57974, 0x3fe1c73b39ae68c8, 0x3c8b25dd20000000, 0x3ff0000000000000],
    [0xbfcd0dfe53aba2fd, 0x3fe44cf325091dd6, 0x3c68076a20000000, 0x3ff0000000000000],
    [0x3fca827999fcef32, 0x3fe6a09e667f3bcd, 0xbc8bdd3420000000, 0x3fe0000000000000],
    [0x3fc133cc94247758, 0x3fe8bc806b151741, 0xbc82c5e120000000, 0x3fe0000000000000],
    [0x3fac73b39ae68c87, 0x3fea9b66290ea1a3, 0x3c39f630e0000000, 0x3fe0000000000000],
    [0xbf9d4a2c7f909c4e, 0x3fec38b2f180bdb1, 0xbc76e0b180000000, 0x3fe0000000000000],
    [0xbfbe087565455a75, 0x3fed906bcf328d46, 0x3c7457e620000000, 0x3fe0000000000000],
    [0x3fa4a03176acf82d, 0x3fee9f4156c62dda, 0x3c8760b1e0000000, 0x3fd0000000000000],
    [0xbfac1d1f0e5967d5, 0x3fef6297cff75cb0, 0x3c75621720000000, 0x3fd0000000000000],
    [0xbf9ba1650f592f50, 0x3fefd88da3d12526, 0xbc887df640000000, 0x3fc0000000000000],
    [0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000, 0x0000000000000000],
    [0x3f9ba1650f592f50, 0x3fefd88da3d12526, 0xbc887df640000000, 0xbfc0000000000000],
    [0x3fac1d1f0e5967d5, 0x3fef6297cff75cb0, 0x3c75621720000000, 0xbfd0000000000000],
    [0xbfa4a03176acf82d, 0x3fee9f4156c62dda, 0x3c8760b1e0000000, 0xbfd0000000000000],
    [0x3fbe087565455a75, 0x3fed906bcf328d46, 0x3c7457e620000000, 0xbfe0000000000000],
    [0x3f9d4a2c7f909c4e, 0x3fec38b2f180bdb1, 0xbc76e0b180000000, 0xbfe0000000000000],
    [0xbfac73b39ae68c87, 0x3fea9b66290ea1a3, 0x3c39f630e0000000, 0xbfe0000000000000],
    [0xbfc133cc94247758, 0x3fe8bc806b151741, 0xbc82c5e120000000, 0xbfe0000000000000],
    [0xbfca827999fcef32, 0x3fe6a09e667f3bcd, 0xbc8bdd3420000000, 0xbfe0000000000000],
    [0x3fcd0dfe53aba2fd, 0x3fe44cf325091dd6, 0x3c68076a20000000, 0xbff0000000000000],
    [0x3fc592675bc57974, 0x3fe1c73b39ae68c8, 0x3c8b25dd20000000, 0xbff0000000000000],
    [0x3fbe3a6873fa1279, 0x3fde2b5d3806f63b, 0x3c5e0d8920000000, 0xbff0000000000000],
    [0x3fb37ca1866b95cf, 0x3fd87de2a6aea963, 0xbc672cede0000000, 0xbff0000000000000],
    [0x3fa60bea939d225a, 0x3fd294062ed59f06, 0xbc75d28da0000000, 0xbff0000000000000],
    [0x3f93ad06011469fb, 0x3fc8f8b83c69a60b, 0xbc626d19c0000000, 0xbff0000000000000],
    [0x3f73b92e176d6d31, 0x3fb917a6bc29b42c, 0xbc3e2718e0000000, 0xbff0000000000000],
    [0x0000000000000000, 0x0000000000000000, 0x0000000000000000, 0xbff0000000000000],
    [0x3f73b92e176d6d31, 0xbfb917a6bc29b42c, 0x3c3e2718e0000000, 0xbff0000000000000],
    [0x3f93ad06011469fb, 0xbfc8f8b83c69a60b, 0x3c626d19c0000000, 0xbff0000000000000],
    [0x3fa60bea939d225a, 0xbfd294062ed59f06, 0x3c75d28da0000000, 0xbff0000000000000],
    [0x3fb37ca1866b95cf, 0xbfd87de2a6aea963, 0x3c672cede0000000, 0xbff0000000000000],
    [0x3fbe3a6873fa1279, 0xbfde2b5d3806f63b, 0xbc5e0d8920000000, 0xbff0000000000000],
    [0x3fc592675bc57974, 0xbfe1c73b39ae68c8, 0xbc8b25dd20000000, 0xbff0000000000000],
    [0x3fcd0dfe53aba2fd, 0xbfe44cf325091dd6, 0xbc68076a20000000, 0xbff0000000000000],
    [0xbfca827999fcef32, 0xbfe6a09e667f3bcd, 0x3c8bdd3420000000, 0xbfe0000000000000],
    [0xbfc133cc94247758, 0xbfe8bc806b151741, 0x3c82c5e120000000, 0xbfe0000000000000],
    [0xbfac73b39ae68c87, 0xbfea9b66290ea1a3, 0xbc39f630e0000000, 0xbfe0000000000000],
    [0x3f9d4a2c7f909c4e, 0xbfec38b2f180bdb1, 0x3c76e0b180000000, 0xbfe0000000000000],
    [0x3fbe087565455a75, 0xbfed906bcf328d46, 0xbc7457e620000000, 0xbfe0000000000000],
    [0xbfa4a03176acf82d, 0xbfee9f4156c62dda, 0xbc8760b1e0000000, 0xbfd0000000000000],
    [0x3fac1d1f0e5967d5, 0xbfef6297cff75cb0, 0xbc75621720000000, 0xbfd0000000000000],
    [0x3f9ba1650f592f50, 0xbfefd88da3d12526, 0x3c887df640000000, 0xbfc0000000000000],
    [0x0000000000000000, 0xbff0000000000000, 0x0000000000000000, 0x0000000000000000],
    [0xbf9ba1650f592f50, 0xbfefd88da3d12526, 0x3c887df640000000, 0x3fc0000000000000],
    [0xbfac1d1f0e5967d5, 0xbfef6297cff75cb0, 0xbc75621720000000, 0x3fd0000000000000],
    [0x3fa4a03176acf82d, 0xbfee9f4156c62dda, 0xbc8760b1e0000000, 0x3fd0000000000000],
    [0xbfbe087565455a75, 0xbfed906bcf328d46, 0xbc7457e620000000, 0x3fe0000000000000],
    [0xbf9d4a2c7f909c4e, 0xbfec38b2f180bdb1, 0x3c76e0b180000000, 0x3fe0000000000000],
    [0x3fac73b39ae68c87, 0xbfea9b66290ea1a3, 0xbc39f630e0000000, 0x3fe0000000000000],
    [0x3fc133cc94247758, 0xbfe8bc806b151741, 0x3c82c5e120000000, 0x3fe0000000000000],
    [0x3fca827999fcef32, 0xbfe6a09e667f3bcd, 0x3c8bdd3420000000, 0x3fe0000000000000],
    [0xbfcd0dfe53aba2fd, 0xbfe44cf325091dd6, 0xbc68076a20000000, 0x3ff0000000000000],
    [0xbfc592675bc57974, 0xbfe1c73b39ae68c8, 0xbc8b25dd20000000, 0x3ff0000000000000],
    [0xbfbe3a6873fa1279, 0xbfde2b5d3806f63b, 0xbc5e0d8920000000, 0x3ff0000000000000],
    [0xbfb37ca1866b95cf, 0xbfd87de2a6aea963, 0x3c672cede0000000, 0x3ff0000000000000],
    [0xbfa60bea939d225a, 0xbfd294062ed59f06, 0x3c75d28da0000000, 0x3ff0000000000000],
    [0xbf93ad06011469fb, 0xbfc8f8b83c69a60b, 0x3c626d19c0000000, 0x3ff0000000000000],
    [0xbf73b92e176d6d31, 0xbfb917a6bc29b42c, 0x3c3e2718e0000000, 0x3ff0000000000000],
];
