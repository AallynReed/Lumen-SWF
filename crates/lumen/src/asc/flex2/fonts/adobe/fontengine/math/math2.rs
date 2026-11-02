//! Port of afe.jar `com.adobe.fontengine.math.Math2`.

pub struct Math2;

const BITS: [i32; 16] = [0, 1, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4];

impl Math2 {
    pub fn round_half_up(d: f64) -> f64 {
        (d + 0.5).floor()
    }

    pub fn round_half_down(d: f64) -> f64 {
        (d - 0.5).ceil()
    }

    pub fn epsilon_equals_f32(f: f32, f2: f32) -> bool {
        (f - f2).abs() < 1.0E-7f32
    }

    pub fn epsilon_equals(d: f64, d2: f64) -> bool {
        (d - d2).abs() < 1.0E-8
    }

    pub fn power_of2(mut n: i32) -> i32 {
        if n < 0 {
            n = n.wrapping_neg();
        }
        if n < 65536 {
            if n < 256 {
                if n < 16 {
                    return BITS[n as usize];
                }
                return BITS[(n >> 4) as usize] + 4;
            }
            if n < 4096 {
                return BITS[(n >> 8) as usize] + 8;
            }
            return BITS[(n >> 12) as usize] + 12;
        }
        if n < 0x1000000 {
            if n < 0x100000 {
                return BITS[(n >> 16) as usize] + 16;
            }
            return BITS[(n >> 20) as usize] + 20;
        }
        if n < 0x10000000 {
            return BITS[(n >> 24) as usize] + 24;
        }
        BITS[(n >> 28) as usize] + 28
    }
}
