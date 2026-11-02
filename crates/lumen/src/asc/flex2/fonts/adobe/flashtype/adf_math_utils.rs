//! Port of flex-fontkit.jar `flash.fonts.flashtype.ADFMathUtils`.

pub const ADF_N_EXP_BITS: i32 = 5;
pub const ADF_N_MAN_BITS: i32 = 10;
pub const ADF_MAX_EXP_VAL: i32 = 31;
pub const ADF_EXP_BIAS: i32 = 16;
pub const ADF_SGN_MASK: i32 = 32768;
pub const ADF_EXP_MASK: i32 = 31744;
pub const ADF_MAN_MASK: i32 = 1023;
pub const ADF_MAN_SCALE: i32 = 1024;

pub fn adf_abs(f: f32) -> f32 {
    if f < 0.0 {
        -f
    } else {
        f
    }
}

pub fn adf_min(f: f32, f2: f32) -> f32 {
    if f < f2 {
        f
    } else {
        f2
    }
}

pub fn adf_max(f: f32, f2: f32) -> f32 {
    if f > f2 {
        f
    } else {
        f2
    }
}

pub fn adf_serialize_u16(l_array: &mut [i64], n: usize, n2: u32, n3: u32, n4: i32) {
    l_array[n] = l_array[n].wrapping_add((n4 & 0xFF).wrapping_shl(n2) as i64);
    l_array[n] = l_array[n].wrapping_add((n4 >> 8).wrapping_shl(n3) as i64);
}

pub fn adf_deserialize_u16(l_array: &[i64], n: usize, n2: u32, n3: u32) -> i64 {
    let l = l_array[n] >> n2 & 0xFF;
    let l2 = l_array[n] >> n3 & 0xFF;
    l2 << 8 | l
}

pub fn adf_serialize_u32(n: i32) -> i32 {
    let mut n2 = n & 0xFF;
    n2 = n2.wrapping_add((n >> 8 & 0xFF) << 8);
    n2 = n2.wrapping_add((n >> 16 & 0xFF) << 16);
    n2.wrapping_add((n >> 24 & 0xFF) << 24)
}

pub fn adf_deserialize_u32(n: i32) -> i32 {
    let n2 = n & 0xFF;
    let n3 = n >> 8 & 0xFF;
    let n4 = n >> 16 & 0xFF;
    let n5 = n >> 24 & 0xFF;
    n5 << 24 | n4 << 16 | n3 << 8 | n2
}

pub fn adf_pack_f32(f: f32) -> ADFPackF32Result {
    let mut r = ADFPackF32Result::default();
    if f == 0.0 {
        r.packed_value = 0;
        return r;
    }
    let mut fp = FloatingPointValue::new(f as f64);
    frexp(&mut fp);
    let mut d = fp.mantissa;
    let n = fp.exp;
    let n2 = n.wrapping_add(16);
    if n2 < 0 {
        r.packed_value = 0;
        r.too_small = true;
        return r;
    }
    if n2 > 31 {
        r.too_large = true;
        return r;
    }
    if d >= 0.0 {
        r.packed_value = 0;
    } else {
        d *= -1.0;
        r.packed_value = 32768;
    }
    r.packed_value |= n2 << 10;
    r.packed_value |= ((d * 1024.0) as i32 as u16) as i32;
    r
}

pub fn adf_unpack_f32(l: i64) -> f32 {
    // Java compares (l & 0x8000) with 1, so the sign is always positive.
    let d = if (l & 0x8000) == 1 { -1.0 } else { 1.0 };
    let n = ((l & 0x7C00) >> 10) as i32 - 16;
    let d2 = (l & 0x3FF) as f64 / 1024.0;
    let mut fp = FloatingPointValue::new_mantissa_exp(d2, n);
    ldexp(&mut fp);
    (d * fp.value) as f32
}

pub fn adf_get_real_quadratic_roots_single(d: f64) -> RootValue {
    adf_get_real_quadratic_roots(&[d, 0.0, 0.0])
}

pub fn adf_get_real_quadratic_roots(d_array: &[f64]) -> RootValue {
    let d = d_array[2];
    let d2 = d_array[1];
    let d3 = d_array[0];
    if d == 0.0 {
        if d2 == 0.0 {
            return RootValue::new(0);
        }
        let mut rv = RootValue::new(1);
        rv.roots = Some(vec![-d3 / d2]);
        return rv;
    }
    let mut d4 = d2 * d2 - 4.0 * d * d3;
    if d4 == 0.0 {
        let mut rv = RootValue::new(1);
        rv.roots = Some(vec![-0.5 * d2 / d]);
        return rv;
    }
    if d4 > 0.0 {
        d4 = d4.sqrt();
        let d5 = (-d2 + if d2 < 0.0 { -d4 } else { d4 }) * 0.5;
        let mut rv = RootValue::new(2);
        rv.roots = Some(vec![d5 / d, d3 / d5]);
        return rv;
    }
    RootValue::new(0)
}

pub fn adf_get_real_cubic_roots(d_array: &[f64]) -> RootValue {
    if d_array[3] == 0.0 {
        return adf_get_real_quadratic_roots(d_array);
    }
    if d_array[0] == 0.0 {
        let mut rv = adf_get_real_quadratic_roots_single(d_array[1]);
        let num = rv.num as usize;
        let mut d_array2 = vec![0.0f64; num + 1];
        if let Some(r) = &rv.roots {
            d_array2[..num].copy_from_slice(&r[..num]);
        }
        d_array2[num] = 0.0;
        rv.roots = Some(d_array2);
        rv.num += 1;
        return rv;
    }
    let d = 0.3333333333333333f64;
    let d2 = 1.0 / d_array[3];
    let d3 = d_array[2] * d2;
    let d4 = d_array[1] * d2;
    let d5 = d_array[0] * d2;
    let d6 = d3 * d;
    let d7 = d3 * d3;
    let d8 = (2.0 * d3 * d7 / 27.0 - d3 * d4 * d + d5) * 0.5;
    let d9 = (-d7 * d + d4) * d;
    let d10 = d9 * d9 * d9;
    let d11 = d8 * d8 + d10;
    if d11 == 0.0 {
        if d8 == 0.0 {
            let mut rv = RootValue::new(1);
            rv.roots = Some(vec![0.0 - d6]);
            return rv;
        }
        let d12 = if d8 > 0.0 { -java_pow_third(d8) } else { java_pow_third(-d8) };
        let mut rv = RootValue::new(2);
        rv.roots = Some(vec![2.0 * d12 - d6, -d12 - d6]);
        return rv;
    }
    if d11 < 0.0 {
        let d13 = 1.0471975511965979f64;
        let d14 = java_acos(-d8 / (-d10).sqrt()) * d;
        let d15 = 2.0 * (-d9).sqrt();
        let mut rv = RootValue::new(3);
        rv.roots = Some(vec![d15 * java_cos(d14) - d6, -d15 * java_cos(d14 + d13) - d6, -d15 * java_cos(d14 - d13) - d6]);
        return rv;
    }
    let d16 = d11.sqrt();
    let d17 = if d16 - d8 < 0.0 { -java_pow_third(-(d16 - d8)) } else { java_pow_third(d16 - d8) };
    let d18 = if d16 + d8 < 0.0 { -java_pow_third(-(d16 + d8)) } else { java_pow_third(d16 + d8) };
    let mut rv = RootValue::new(1);
    rv.roots = Some(vec![d17 - d18 - d6]);
    rv
}

pub fn frexp(fp: &mut FloatingPointValue) {
    let v = fp.value;
    let l = if v.is_nan() { 0x7ff8000000000000u64 as i64 } else { v.to_bits() as i64 };
    if fp.value == 0.0 {
        fp.exp = 0;
        fp.mantissa = 0.0;
    } else {
        fp.mantissa = f64::from_bits((0x800FFFFFFFFFFFFFu64 as i64 & l | 0x3FE0000000000000) as u64);
        fp.exp = (((0x7FF0000000000000 & l) >> 52) - 1022) as i32;
    }
}

pub fn ldexp(fp: &mut FloatingPointValue) {
    fp.value = fp.mantissa * 2.0f64.powf(fp.exp as f64);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FloatingPointValue {
    pub value: f64,
    pub mantissa: f64,
    pub exp: i32,
}

impl FloatingPointValue {
    pub fn new(d: f64) -> Self {
        FloatingPointValue { value: d, ..Default::default() }
    }

    pub fn new_mantissa_exp(d: f64, n: i32) -> Self {
        FloatingPointValue { mantissa: d, exp: n, ..Default::default() }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RootValue {
    pub num: i32,
    pub roots: Option<Vec<f64>>,
}

impl RootValue {
    pub fn new(n: i32) -> Self {
        RootValue { num: n, roots: None }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ADFPackF32Result {
    pub packed_value: i32,
    pub too_small: bool,
    pub too_large: bool,
}

/// Math.acos: HotSpot uses fdlibm's __ieee754_acos.
pub fn java_acos(x: f64) -> f64 {
    let c = |b: u64| f64::from_bits(b);
    let pi = c(0x400921FB54442D18);
    let pio2_hi = c(0x3FF921FB54442D18);
    let pio2_lo = c(0x3C91A62633145C07);
    let ps = [c(0x3FC5555555555555), c(0xBFD4D61203EB6F7D), c(0x3FC9C1550E884455), c(0xBFA48228B5688F3B), c(0x3F49EFE07501B288), c(0x3F023DE10DFDF709)];
    let qs = [c(0xC0033A271C8A2D4B), c(0x40002AE59C598AC8), c(0xBFE6066C1B8D0159), c(0x3FB3B8C5B12E9282)];
    let pq = |z: f64| {
        let p = z * (ps[0] + z * (ps[1] + z * (ps[2] + z * (ps[3] + z * (ps[4] + z * ps[5])))));
        let q = 1.0 + z * (qs[0] + z * (qs[1] + z * (qs[2] + z * qs[3])));
        p / q
    };
    let bits = x.to_bits();
    let hx = (bits >> 32) as u32 as i32;
    let lx = bits as u32;
    let ix = hx & 0x7fffffff;
    if ix >= 0x3ff00000 {
        if ((ix - 0x3ff00000) as u32 | lx) == 0 {
            return if hx > 0 { 0.0 } else { pi + 2.0 * pio2_lo };
        }
        return (x - x) / (x - x);
    }
    if ix < 0x3fe00000 {
        if ix <= 0x3c600000 {
            return pio2_hi + pio2_lo;
        }
        let r = pq(x * x);
        pio2_hi - (x - (pio2_lo - x * r))
    } else if hx < 0 {
        let z = (1.0 + x) * 0.5;
        let s = z.sqrt();
        let r = pq(z);
        let w = r * s - pio2_lo;
        pi - 2.0 * (s + w)
    } else {
        let z = (1.0 - x) * 0.5;
        let s = z.sqrt();
        let df = f64::from_bits(s.to_bits() & 0xFFFFFFFF00000000);
        let cc = (z - df * df) / (s + df);
        let r = pq(z);
        let w = r * s + cc;
        2.0 * (df + w)
    }
}

/// Math.cos: HotSpot's intrinsic, matched by rounding a double-double evaluation (|x| < 5pi/4).
pub fn java_cos(x: f64) -> f64 {
    let a = x.abs();
    let r = if a <= 0.7853981633974483 {
        dd_cos(DD(a, 0.0))
    } else if a <= 2.356194490192345 {
        dd_sin(DD::add(DD_PI_2, DD(-a, 0.0)))
    } else if a <= 3.9269908169872414 {
        let v = dd_cos(DD::add(DD_PI, DD(-a, 0.0)));
        DD(-v.0, -v.1)
    } else {
        return x.cos();
    };
    r.0 + r.1
}

/// Math.pow(x, 0.3333333333333333) as HotSpot's intrinsic gives it, matched by rounding a double-double
/// cbrt(x) * x^(-1/(3*2^54)) (the exponent is 1/3 - 1/(3*2^54)).
pub fn java_pow_third(x: f64) -> f64 {
    if !(x > 0.0) || !x.is_finite() {
        return x.powf(0.3333333333333333);
    }
    let c0 = x.cbrt();
    let cube = DD::mul(DD::two_prod(c0, c0), DD(c0, 0.0));
    let r = DD::add(DD(x, 0.0), DD(-cube.0, -cube.1));
    let c = DD::two_sum(c0, (r.0 + r.1) / (3.0 * c0 * c0));
    let delta = x.ln() / 54043195528445952.0;
    let v = DD::add(c, DD(c.0 * (delta * delta * 0.5 - delta), 0.0));
    v.0 + v.1
}

const DD_PI_2: DD = DD(1.5707963267948966, 6.123233995736766e-17);
const DD_PI: DD = DD(3.141592653589793, 1.2246467991473532e-16);

fn inv_fact() -> &'static [DD; 28] {
    static F: std::sync::OnceLock<[DD; 28]> = std::sync::OnceLock::new();
    F.get_or_init(|| {
        let mut f = [DD(1.0, 0.0); 28];
        for k in 1..28 {
            f[k] = DD::div(f[k - 1], DD(k as f64, 0.0));
        }
        f
    })
}

/// Taylor series in t*t over 1/k! for k = start, start + 2, ..., 27.
fn dd_series(t2: DD, start: usize) -> DD {
    let f = inv_fact();
    let mut k = 27 - (27 - start) % 2;
    let mut p = f[k];
    while k > start {
        k -= 2;
        p = DD::add(DD::mul(DD(-p.0, -p.1), t2), f[k]);
    }
    p
}

fn dd_cos(t: DD) -> DD {
    dd_series(DD::mul(t, t), 0)
}

fn dd_sin(t: DD) -> DD {
    DD::mul(dd_series(DD::mul(t, t), 1), t)
}

#[derive(Clone, Copy, Debug)]
struct DD(f64, f64);

impl DD {
    fn two_sum(a: f64, b: f64) -> DD {
        let s = a + b;
        let bb = s - a;
        DD(s, (a - (s - bb)) + (b - bb))
    }

    fn quick(a: f64, b: f64) -> DD {
        let s = a + b;
        DD(s, b - (s - a))
    }

    fn split(a: f64) -> (f64, f64) {
        let t = 134217729.0 * a;
        let hi = t - (t - a);
        (hi, a - hi)
    }

    fn two_prod(a: f64, b: f64) -> DD {
        let p = a * b;
        let (ah, al) = DD::split(a);
        let (bh, bl) = DD::split(b);
        DD(p, ((ah * bh - p) + ah * bl + al * bh) + al * bl)
    }

    fn add(x: DD, y: DD) -> DD {
        let s = DD::two_sum(x.0, y.0);
        let t = DD::two_sum(x.1, y.1);
        let u = DD::quick(s.0, s.1 + t.0);
        DD::quick(u.0, u.1 + t.1)
    }

    fn mul(x: DD, y: DD) -> DD {
        let p = DD::two_prod(x.0, y.0);
        DD::quick(p.0, p.1 + (x.0 * y.1 + x.1 * y.0))
    }

    fn div(x: DD, y: DD) -> DD {
        let q1 = x.0 / y.0;
        let r = DD::add(x, DD::mul(y, DD(-q1, 0.0)));
        let q2 = r.0 / y.0;
        let r = DD::add(r, DD::mul(y, DD(-q2, 0.0)));
        let q3 = r.0 / y.0;
        DD::add(DD::quick(q1, q2), DD(q3, 0.0))
    }
}
