//! Port of FreeType 2.14.2 `src/base/fttrigon.c` (as built into JDK 17.0.19).

use crate::asc::flex2::fonts::java2d::freetype::ftcalc::ft_msb;
use crate::asc::flex2::fonts::java2d::freetype::fttypes::*;

pub type FtAngle = i32;

pub const FT_ANGLE_PI: FtAngle = 180 << 16;
pub const FT_ANGLE_2PI: FtAngle = FT_ANGLE_PI * 2;
pub const FT_ANGLE_PI2: FtAngle = FT_ANGLE_PI / 2;
pub const FT_ANGLE_PI4: FtAngle = FT_ANGLE_PI / 4;

const FT_TRIG_SCALE: u32 = 0xDBD9_5B16;
const FT_TRIG_SAFE_MSB: i32 = 29;
const FT_TRIG_MAX_ITERS: i32 = 23;

static FT_TRIG_ARCTAN_TABLE: [FtAngle; 22] = [
    1740967, 919879, 466945, 234379, 117304, 58666, 29335, 14668, 7334, 3667, 1833, 917, 458, 229, 115, 57, 29, 14, 7,
    4, 2, 1,
];

fn ft_trig_downscale(mut val: FtFixed) -> FtFixed {
    let mut s = 1;
    if val < 0 {
        val = val.wrapping_neg();
        s = -1;
    }
    val = (((val as u32 as u64) * FT_TRIG_SCALE as u64 + 0x4000_0000) >> 32) as FtFixed;
    if s < 0 {
        val.wrapping_neg()
    } else {
        val
    }
}

fn ft_trig_prenorm(vec: &mut FtVector) -> i32 {
    let x = vec.x;
    let y = vec.y;
    let mut shift = ft_msb((ft_abs(x) | ft_abs(y)) as u32);
    if shift <= FT_TRIG_SAFE_MSB {
        shift = FT_TRIG_SAFE_MSB - shift;
        vec.x = ((x as u32) << shift) as FtPos;
        vec.y = ((y as u32) << shift) as FtPos;
    } else {
        shift -= FT_TRIG_SAFE_MSB;
        vec.x = x >> shift;
        vec.y = y >> shift;
        shift = -shift;
    }
    shift
}

fn ft_trig_pseudo_polarize(vec: &mut FtVector) {
    let mut theta: FtAngle;
    let mut x = vec.x;
    let mut y = vec.y;
    let mut xtemp;
    if y > x {
        if y > x.wrapping_neg() {
            theta = FT_ANGLE_PI2;
            xtemp = y;
            y = x.wrapping_neg();
            x = xtemp;
        } else {
            theta = if y > 0 { FT_ANGLE_PI } else { -FT_ANGLE_PI };
            x = x.wrapping_neg();
            y = y.wrapping_neg();
        }
    } else if y < x.wrapping_neg() {
        theta = -FT_ANGLE_PI2;
        xtemp = y.wrapping_neg();
        y = x;
        x = xtemp;
    } else {
        theta = 0;
    }
    let mut b: i32 = 1;
    let mut k = 0usize;
    for i in 1..FT_TRIG_MAX_ITERS {
        if y > 0 {
            xtemp = x.wrapping_add(y.wrapping_add(b) >> i);
            y = y.wrapping_sub(x.wrapping_add(b) >> i);
            x = xtemp;
            theta = theta.wrapping_add(FT_TRIG_ARCTAN_TABLE[k]);
        } else {
            xtemp = x.wrapping_sub(y.wrapping_add(b) >> i);
            y = y.wrapping_add(x.wrapping_add(b) >> i);
            x = xtemp;
            theta = theta.wrapping_sub(FT_TRIG_ARCTAN_TABLE[k]);
        }
        k += 1;
        b <<= 1;
    }
    if theta >= 0 {
        theta = ft_pad_round(theta, 16);
    } else {
        theta = -ft_pad_round(theta.wrapping_neg(), 16);
    }
    vec.x = x;
    vec.y = theta;
}

pub fn ft_atan2(dx: FtFixed, dy: FtFixed) -> FtAngle {
    if dx == 0 && dy == 0 {
        return 0;
    }
    let mut v = FtVector { x: dx, y: dy };
    ft_trig_prenorm(&mut v);
    ft_trig_pseudo_polarize(&mut v);
    v.y
}

pub fn ft_vector_length(vec: &FtVector) -> FtFixed {
    let mut v = *vec;
    if v.x == 0 {
        return ft_abs(v.y);
    } else if v.y == 0 {
        return ft_abs(v.x);
    }
    let shift = ft_trig_prenorm(&mut v);
    ft_trig_pseudo_polarize(&mut v);
    v.x = ft_trig_downscale(v.x);
    if shift > 0 {
        return (v.x.wrapping_add(1 << (shift - 1))) >> shift;
    }
    ((v.x as u32) << -shift) as FtFixed
}
