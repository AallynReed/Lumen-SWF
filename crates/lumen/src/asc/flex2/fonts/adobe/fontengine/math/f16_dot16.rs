//! Port of afe.jar `com.adobe.fontengine.math.F16Dot16`.

pub struct F16Dot16;

impl F16Dot16 {
    pub const ZERO: i32 = 0;
    pub const ONE: i32 = 65536;
    pub const ONE_HALF: i32 = 32768;

    fn clamp(l: i64) -> i32 {
        l.clamp(i32::MIN as i64, i32::MAX as i64) as i32
    }

    pub fn truncate(n: i32) -> i32 {
        n & 0xFFFF0000u32 as i32
    }

    pub fn multiply(n: i32, n2: i32) -> i32 {
        Self::clamp((n as i64 * n2 as i64) >> 16)
    }

    pub fn square(n: i32) -> i32 {
        Self::multiply(n, n)
    }

    pub fn multiply_by_f2_dot14(n: i32, n2: i32) -> i32 {
        Self::clamp((((n as i64 * n2 as i64) >> 13) + 1) >> 1)
    }

    pub fn multiply_divide(n: i32, n2: i32, n3: i32) -> i32 {
        let mut l = n as i64 * n2 as i64;
        let mut l2 = n3 as i64;
        let mut neg = false;
        if l < 0 {
            l = -l;
            neg = !neg;
        }
        if l2 < 0 {
            l2 = -l2;
            neg = !neg;
        }
        let l3 = (l + l2 / 2) / l2;
        if neg {
            Self::clamp(-l3)
        } else {
            Self::clamp(l3)
        }
    }

    pub fn divide(n: i32, n2: i32) -> i32 {
        Self::clamp(((n as i64) << 16) / n2 as i64)
    }

    pub fn round(n: i32) -> i32 {
        Self::truncate(n.wrapping_add(32768))
    }

    pub fn to_double(n: i32) -> f64 {
        n as f64 / 65536.0
    }

    pub fn from_double(d: f64) -> i32 {
        (d * 65536.0) as i32
    }

    pub fn from_int(n: i32) -> i32 {
        n << 16
    }
}
