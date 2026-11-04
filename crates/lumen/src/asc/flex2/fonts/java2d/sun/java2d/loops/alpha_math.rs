//! Port of JDK 17 `AlphaMath.c` / `AlphaMath.h`: the `mul8table` and `div8table`
//! lookup tables as libawt's `initAlphaTables` builds them.
//!
//! Both tables are reproduced exactly as the native code computes them (verified against a dump of
//! all 65536 entries of each from a running JVM, zero differences):
//! * `mul8table[a][b] == (a*b + 127) / 255` (the vectorised integer form in `initAlphaTables`).
//! * `div8table[a][b]`: `div8table[0][*] == 0`; otherwise, with the native 32-bit reciprocal
//!   `k = (0xff000000 + (a>>1)) / a`, `div8table[a][b] == min(255, (b*k + 0x800000) >> 24)`.

/// `mul8table[a*256 + b]`: `(a*b + 127) / 255`.
pub static MUL8: [u8; 65536] = build_mul8();

/// `div8table[a*256 + b]`.
pub static DIV8: [u8; 65536] = build_div8();

const fn build_mul8() -> [u8; 65536] {
    let mut t = [0u8; 65536];
    let mut a = 0usize;
    while a < 256 {
        let mut b = 0usize;
        while b < 256 {
            t[a * 256 + b] = ((a * b + 127) / 255) as u8;
            b += 1;
        }
        a += 1;
    }
    t
}

const fn build_div8() -> [u8; 65536] {
    let mut t = [0u8; 65536];
    let mut a = 1usize;
    while a < 256 {
        let k = (0xff00_0000u64 + (a as u64 >> 1)) / a as u64;
        let mut b = 0usize;
        while b < 256 {
            let v = (b as u64 * k + 0x0080_0000) >> 24;
            t[a * 256 + b] = if v > 255 { 255 } else { v as u8 };
            b += 1;
        }
        a += 1;
    }
    t
}

/// `MUL8(a, b)` of `AlphaMath.h`.
#[inline]
pub fn mul8(a: i32, b: i32) -> i32 {
    MUL8[(a as usize) * 256 + b as usize] as i32
}

/// `DIV8(a, b)` of `AlphaMath.h`.
#[inline]
pub fn div8(a: i32, b: i32) -> i32 {
    DIV8[(a as usize) * 256 + b as usize] as i32
}
