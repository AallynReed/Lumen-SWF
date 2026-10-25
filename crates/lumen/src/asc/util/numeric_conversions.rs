//! Port of `macromedia/asc/util/NumericConversions.java`: the ECMA-262 `ToUint32`/`ToInt32` conversions.

use crate::asc::util::number_constant::java_d2l;

pub const TWO_POW31: i64 = 2147483648;
pub const TWO_POW32: i64 = 4294967296;

pub fn to_uint32(d: f64) -> i64 {
    if d.is_nan() || d.is_infinite() || 0.0 == d {
        return 0;
    }

    let result3 = d.abs().floor();
    java_d2l(result3) % TWO_POW32
}

pub fn to_int32(d: f64) -> i32 {
    let signum = if d.is_nan() || d == 0.0 { d } else { d.signum() };
    let result4 = java_d2l(signum).wrapping_mul(to_uint32(d));

    if result4 >= TWO_POW31 {
        (result4 - TWO_POW32) as i32
    } else {
        result4 as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expectations from Java's `NumericConversions`.
    #[test]
    fn matches_java() {
        let cases = [
            (-1.0, 1, -1),
            (4294967297.5, 1, 1),
            (1e300, 4294967295, -1),
            (2147483648.0, 2147483648, -2147483648),
            (-2147483649.0, 2147483649, 2147483647),
            (f64::NAN, 0, 0),
            (-0.0, 0, 0),
            (-1e300, 4294967295, 1),
            (3.7, 3, 3),
            (-3.7, 3, -3),
        ];
        for (d, u, i) in cases {
            assert_eq!((to_uint32(d), to_int32(d)), (u, i), "{}", d);
        }
    }
}
