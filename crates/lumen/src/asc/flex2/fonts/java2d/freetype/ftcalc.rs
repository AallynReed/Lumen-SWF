//! Port of FreeType 2.14.2 `src/base/ftcalc.c` (as built into JDK 17.0.19).

use crate::asc::flex2::fonts::java2d::freetype::fterrors::*;
use crate::asc::flex2::fonts::java2d::freetype::fttrigon::ft_vector_length;
use crate::asc::flex2::fonts::java2d::freetype::fttypes::*;

#[inline]
fn move_sign(x: i32, s: &mut i32) -> u64 {
    if x < 0 {
        *s = -*s;
        0u64.wrapping_sub(x as i64 as u64)
    } else {
        x as u64
    }
}

pub fn ft_round_fix(a: FtFixed) -> FtFixed {
    add_long(a, 0x8000 - (a < 0) as i32) & !0xFFFF
}

pub fn ft_ceil_fix(a: FtFixed) -> FtFixed {
    add_long(a, 0xFFFF) & !0xFFFF
}

pub fn ft_floor_fix(a: FtFixed) -> FtFixed {
    a & !0xFFFF
}

#[inline]
pub fn ft_msb(z: u32) -> i32 {
    31 - z.leading_zeros() as i32
}

pub fn ft_hypot(x: FtFixed, y: FtFixed) -> FtFixed {
    ft_vector_length(&FtVector { x, y })
}

pub fn ft_mul_div(a_: i32, b_: i32, c_: i32) -> i32 {
    let mut s = 1;
    let a = move_sign(a_, &mut s);
    let b = move_sign(b_, &mut s);
    let c = move_sign(c_, &mut s);
    let d = if c > 0 { (a.wrapping_mul(b).wrapping_add(c >> 1)) / c } else { 0x7FFF_FFFF };
    let d_ = d as i32;
    if s < 0 {
        neg_long(d_)
    } else {
        d_
    }
}

pub fn ft_mul_div_no_round(a_: i32, b_: i32, c_: i32) -> i32 {
    let mut s = 1;
    let a = move_sign(a_, &mut s);
    let b = move_sign(b_, &mut s);
    let c = move_sign(c_, &mut s);
    let d = if c > 0 { a.wrapping_mul(b) / c } else { 0x7FFF_FFFF };
    let d_ = d as i32;
    if s < 0 {
        neg_long(d_)
    } else {
        d_
    }
}

/// `FT_MulFix_64` (FT_CONFIG_OPTION_INLINE_MULFIX with a 64-bit type).
#[inline]
pub fn ft_mul_fix(a: i32, b: i32) -> i32 {
    let mut ab = (a as i64).wrapping_mul(b as i64);
    ab = ab.wrapping_add(0x8000 + (ab >> 63));
    (ab >> 16) as i32
}

pub fn ft_div_fix(a_: i32, b_: i32) -> i32 {
    let mut s = 1;
    let a = move_sign(a_, &mut s);
    let b = move_sign(b_, &mut s);
    let q = if b > 0 { ((a << 16).wrapping_add(b >> 1)) / b } else { 0x7FFF_FFFF };
    let q_ = q as i32;
    if s < 0 {
        neg_long(q_)
    } else {
        q_
    }
}

pub fn ft_matrix_multiply(a: &FtMatrix, b: &mut FtMatrix) {
    let xx = add_long(ft_mul_fix(a.xx, b.xx), ft_mul_fix(a.xy, b.yx));
    let xy = add_long(ft_mul_fix(a.xx, b.xy), ft_mul_fix(a.xy, b.yy));
    let yx = add_long(ft_mul_fix(a.yx, b.xx), ft_mul_fix(a.yy, b.yx));
    let yy = add_long(ft_mul_fix(a.yx, b.xy), ft_mul_fix(a.yy, b.yy));
    b.xx = xx;
    b.xy = xy;
    b.yx = yx;
    b.yy = yy;
}

pub fn ft_matrix_invert(matrix: &mut FtMatrix) -> FtError {
    let delta = ft_mul_fix(matrix.xx, matrix.yy).wrapping_sub(ft_mul_fix(matrix.xy, matrix.yx));
    if delta == 0 {
        return FT_ERR_INVALID_ARGUMENT;
    }
    matrix.xy = ft_div_fix(matrix.xy, delta).wrapping_neg();
    matrix.yx = ft_div_fix(matrix.yx, delta).wrapping_neg();
    let xx = matrix.xx;
    let yy = matrix.yy;
    matrix.xx = ft_div_fix(yy, delta);
    matrix.yy = ft_div_fix(xx, delta);
    FT_ERR_OK
}

pub fn ft_matrix_multiply_scaled(a: &FtMatrix, b: &mut FtMatrix, scaling: i32) {
    let val = 0x10000i32.wrapping_mul(scaling);
    let xx = add_long(ft_mul_div(a.xx, b.xx, val), ft_mul_div(a.xy, b.yx, val));
    let xy = add_long(ft_mul_div(a.xx, b.xy, val), ft_mul_div(a.xy, b.yy, val));
    let yx = add_long(ft_mul_div(a.yx, b.xx, val), ft_mul_div(a.yy, b.yx, val));
    let yy = add_long(ft_mul_div(a.yx, b.xy, val), ft_mul_div(a.yy, b.yy, val));
    b.xx = xx;
    b.xy = xy;
    b.yx = yx;
    b.yy = yy;
}

pub fn ft_vector_transform_scaled(vector: &mut FtVector, matrix: &FtMatrix, scaling: i32) {
    let val = 0x10000i32.wrapping_mul(scaling);
    let xz = add_long(ft_mul_div(vector.x, matrix.xx, val), ft_mul_div(vector.y, matrix.xy, val));
    let yz = add_long(ft_mul_div(vector.x, matrix.yx, val), ft_mul_div(vector.y, matrix.yy, val));
    vector.x = xz;
    vector.y = yz;
}

pub fn ft_vector_norm_len(vector: &mut FtVector) -> u32 {
    let x_ = vector.x;
    let y_ = vector.y;
    let (mut sx, mut sy) = (1i32, 1i32);
    let mut x = move_sign(x_, &mut sx) as u32;
    let mut y = move_sign(y_, &mut sy) as u32;
    if x == 0 {
        if y > 0 {
            vector.y = sy * 0x10000;
        }
        return y;
    } else if y == 0 {
        if x > 0 {
            vector.x = sx * 0x10000;
        }
        return x;
    }
    let mut l = if x > y { x.wrapping_add(y >> 1) } else { y.wrapping_add(x >> 1) };
    let mut shift = 31 - ft_msb(l);
    shift -= 15 + (l >= (0xAAAA_AAAAu32 >> shift)) as i32;
    if shift > 0 {
        x <<= shift;
        y <<= shift;
        l = if x > y { x.wrapping_add(y >> 1) } else { y.wrapping_add(x >> 1) };
    } else {
        x >>= -shift;
        y >>= -shift;
        l >>= -shift;
    }
    let mut b: i32 = 0x10000 - l as i32;
    let x_ = x as i32;
    let y_ = y as i32;
    let mut u: u32;
    let mut v: u32;
    loop {
        u = x_.wrapping_add(x_.wrapping_mul(b) >> 16) as u32;
        v = y_.wrapping_add(y_.wrapping_mul(b) >> 16) as u32;
        let mut z: i32 = (u.wrapping_mul(u).wrapping_add(v.wrapping_mul(v)) as i32).wrapping_neg() / 0x200;
        z = z.wrapping_mul((0x10000 + b) >> 8) / 0x10000;
        b = b.wrapping_add(z);
        if z <= 0 {
            break;
        }
    }
    vector.x = if sx < 0 { (u as i32).wrapping_neg() } else { u as i32 };
    vector.y = if sy < 0 { (v as i32).wrapping_neg() } else { v as i32 };
    l = (0x10000i32.wrapping_add((u.wrapping_mul(x).wrapping_add(v.wrapping_mul(y)) as i32) / 0x10000)) as u32;
    if shift > 0 {
        l = (l.wrapping_add(1 << (shift - 1))) >> shift;
    } else {
        l <<= -shift;
    }
    l
}

pub fn ft_sqrt_fixed(v: u32) -> u32 {
    if v == 0 {
        return 0;
    }
    let r: u64 = ((v as u64) << 16) - 1;
    let mut q: u32 = 1 << ((17 + ft_msb(v)) >> 1);
    loop {
        let t = q;
        q = (t.wrapping_add((r / t as u64) as u32).wrapping_add(1)) >> 1;
        if q == t {
            break;
        }
    }
    q
}

pub fn ft_corner_orientation(in_x: FtPos, in_y: FtPos, out_x: FtPos, out_y: FtPos) -> i32 {
    let delta = (in_x as i64).wrapping_mul(out_y as i64).wrapping_sub((in_y as i64).wrapping_mul(out_x as i64));
    (delta > 0) as i32 - (delta < 0) as i32
}

#[inline]
pub fn ft_hypot_approx(x: FtPos, y: FtPos) -> FtPos {
    let x = ft_abs(x);
    let y = ft_abs(y);
    if x > y {
        x.wrapping_add((3i32.wrapping_mul(y)) >> 3)
    } else {
        y.wrapping_add((3i32.wrapping_mul(x)) >> 3)
    }
}

pub fn ft_corner_is_flat(in_x: FtPos, in_y: FtPos, out_x: FtPos, out_y: FtPos) -> i32 {
    let ax = in_x.wrapping_add(out_x);
    let ay = in_y.wrapping_add(out_y);
    let d_in = ft_hypot_approx(in_x, in_y);
    let d_out = ft_hypot_approx(out_x, out_y);
    let d_hypot = ft_hypot_approx(ax, ay);
    (d_in.wrapping_add(d_out).wrapping_sub(d_hypot) < (d_hypot >> 4)) as i32
}
