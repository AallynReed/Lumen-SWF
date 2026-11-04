//! `sun.java2d.marlin.FloatMath`.

/// `FloatMath.floor_int(double)`.
pub fn floor_int(a: f64) -> i32 {
    let int_a = a as i32;
    if a >= int_a as f64 || int_a == i32::MIN || a.is_nan() {
        int_a
    } else {
        int_a - 1
    }
}
