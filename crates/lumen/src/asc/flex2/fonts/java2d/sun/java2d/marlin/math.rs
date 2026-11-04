//! The `java.lang.Math` functions Marlin calls, computed as JDK 17 HotSpot on x86_64 computes them: `ulp`, `cbrt`
//! (`StrictMath.cbrt`, fdlibm's Java port), `acos` (`StrictMath.acos`, native fdlibm) and `cos` (the `StubRoutines::dcos`
//! intrinsic).

/// `Math.ulp(double)`.
pub fn ulp(d: f64) -> f64 {
    let exp = (((d.to_bits() >> 52) & 0x7ff) as i32) - 1023;
    match exp {
        1024 => d.abs(),
        -1023 => f64::from_bits(1),
        _ => {
            let exp = exp - 52;
            if exp >= -1022 {
                f64::from_bits(((exp + 1023) as u64) << 52)
            } else {
                f64::from_bits(1u64 << (exp - (-1022 - 52)))
            }
        }
    }
}

fn hi(x: f64) -> i32 {
    (x.to_bits() >> 32) as i32
}

fn lo(x: f64) -> u32 {
    x.to_bits() as u32
}

fn with_hi(x: f64, high: i32) -> f64 {
    f64::from_bits(((high as u32 as u64) << 32) | (x.to_bits() & 0xffff_ffff))
}

fn with_lo(x: f64, low: u32) -> f64 {
    f64::from_bits((x.to_bits() & 0xffff_ffff_0000_0000) | low as u64)
}

/// `StrictMath.cbrt` (`FdLibm.Cbrt.compute`).
pub fn cbrt(x: f64) -> f64 {
    const B1: i32 = 715094163;
    const B2: i32 = 696219795;
    let c = f64::from_bits(0x3FE15F15F15F15F1);
    let d = f64::from_bits(0xBFE691DE2532C834);
    let e = f64::from_bits(0x3FF6A0EA0EA0EA0F);
    let f = f64::from_bits(0x3FF9B6DB6DB6DB6E);
    let g = f64::from_bits(0x3FD6DB6DB6DB6DB7);

    let mut t = 0.0f64;

    if x == 0.0 || !x.is_finite() {
        return x;
    }

    let sign = if x < 0.0 { -1.0 } else { 1.0 };

    let x = x.abs();

    if x < f64::from_bits(0x0010_0000_0000_0000) {
        t = f64::from_bits(0x4350_0000_0000_0000);
        t *= x;
        t = with_hi(t, hi(t) / 3 + B2);
    } else {
        let hx = hi(x);
        t = with_hi(t, hx / 3 + B1);
    }

    let mut r = t * t / x;
    let mut s = c + r * t;
    t *= g + f / (s + e + d / s);

    t = with_lo(t, 0);
    t = with_hi(t, hi(t) + 0x00000001);

    s = t * t;
    r = x / s;
    let w = t + t;
    r = (r - t) / (w + r);
    t += t * r;

    sign * t
}

/// `StrictMath.acos` (fdlibm `__ieee754_acos`).
pub fn acos(x: f64) -> f64 {
    let one = 1.0f64;
    let pi = f64::from_bits(0x400921FB54442D18);
    let pio2_hi = f64::from_bits(0x3FF921FB54442D18);
    let pio2_lo = f64::from_bits(0x3C91A62633145C07);
    let ps0 = f64::from_bits(0x3FC5555555555555);
    let ps1 = f64::from_bits(0xBFD4D61203EB6F7D);
    let ps2 = f64::from_bits(0x3FC9C1550E884455);
    let ps3 = f64::from_bits(0xBFA48228B5688F3B);
    let ps4 = f64::from_bits(0x3F49EFE07501B288);
    let ps5 = f64::from_bits(0x3F023DE10DFDF709);
    let qs1 = f64::from_bits(0xC0033A271C8A2D4B);
    let qs2 = f64::from_bits(0x40002AE59C598AC8);
    let qs3 = f64::from_bits(0xBFE6066C1B8D0159);
    let qs4 = f64::from_bits(0x3FB3B8C5B12E9282);

    let hx = hi(x);
    let ix = hx & 0x7fffffff;
    if ix >= 0x3ff00000 {
        if ((ix - 0x3ff00000) as u32 | lo(x)) == 0 {
            if hx > 0 {
                return 0.0;
            } else {
                return pi + 2.0 * pio2_lo;
            }
        }
        return (x - x) / (x - x);
    }
    if ix < 0x3fe00000 {
        if ix <= 0x3c600000 {
            return pio2_hi + pio2_lo;
        }
        let z = x * x;
        let p = z * (ps0 + z * (ps1 + z * (ps2 + z * (ps3 + z * (ps4 + z * ps5)))));
        let q = one + z * (qs1 + z * (qs2 + z * (qs3 + z * qs4)));
        let r = p / q;
        pio2_hi - (x - (pio2_lo - x * r))
    } else if hx < 0 {
        let z = (one + x) * 0.5;
        let p = z * (ps0 + z * (ps1 + z * (ps2 + z * (ps3 + z * (ps4 + z * ps5)))));
        let q = one + z * (qs1 + z * (qs2 + z * (qs3 + z * qs4)));
        let s = z.sqrt();
        let r = p / q;
        let w = r * s - pio2_lo;
        pi - 2.0 * (s + w)
    } else {
        let z = (one - x) * 0.5;
        let s = z.sqrt();
        let df = with_lo(s, 0);
        let c = (z - df * df) / (s + df);
        let p = z * (ps0 + z * (ps1 + z * (ps2 + z * (ps3 + z * (ps4 + z * ps5)))));
        let q = one + z * (qs1 + z * (qs2 + z * (qs3 + z * qs4)));
        let r = p / q;
        let w = r * s + c;
        2.0 * (df + w)
    }
}

/// `Math.cos`: HotSpot's x86_64 `StubRoutines::dcos` (Intel LIBM), replayed lane by lane from the JDK 17 stub. The
/// large-argument reduction (|x| >= about 90112, never reached from Marlin) falls back to `f64::cos`.
pub fn cos(x: f64) -> f64 {
    let hx = (x.to_bits() >> 32) as u32;
    let e = (hx & 0x7fff_0000).wrapping_sub(0x3030_0000);
    if e > 0x10c5_0000 {
        if (e as i32) <= 0x10c5_0000 {
            return 1.0 - x.abs();
        }
        if hx & 0x7ff0_0000 == 0x7ff0_0000 {
            return x * -0.0;
        }
        return x.cos();
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
    let ct = &COS_CTABLE[((n as u32 as u64 + 0x1c7610) & 0x3f) as usize];
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

/// The dcos stub's `Ctable` (64 entries of four doubles).
static COS_CTABLE: [[u64; 4]; 64] = [
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
