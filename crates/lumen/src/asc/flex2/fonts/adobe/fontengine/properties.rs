//! Port of afe.jar `com.adobe.fontengine.Properties`.

use crate::asc::flex2::fonts::adobe::fontengine::unicode_data::{CASE_TABLE, CASE_TABLE_COMPLEX, CASE_TABLE_CP, CASE_TABLE_DELTA};

pub const MAX_CASE_EXPANSION: i32 = 3;
pub const CASE_TAILORING_NONE: i32 = 0;
pub const CASE_TAILORING_DOTTED_I: i32 = 1;
pub const CASE_TAILORING_SHARP_S: i32 = 2;
pub const CASE_TAILORING_SUBSCRIPT_IOTA: i32 = 4;

/// Java's `%` on ints (sign of the dividend).
fn jmod2(n: i32) -> i32 {
    n % 2
}

pub fn get_full_upper_case(n: i32, n2: i32, out: &mut [i32; 3]) -> i32 {
    let mut n4 = search_range_table(&CASE_TABLE, 9, 19, 399, n);
    match n4 >> 29 & 7 {
        0 | 5 => {
            out[0] = n;
            1
        }
        1 => {
            out[0] = if jmod2(n) == 0 { n } else { n - 1 };
            1
        }
        2 => {
            out[0] = if jmod2(n) == 1 { n } else { n - 1 };
            1
        }
        3 => {
            let n3 = n4 >> 21 & 0xFF;
            out[0] = n - n3;
            1
        }
        4 => {
            out[0] = n - CASE_TABLE_DELTA[(n4 >> 21 & 0xFF) as usize];
            1
        }
        6 => {
            let n3 = n4 >> 21 & 0xFF;
            n4 = CASE_TABLE_COMPLEX[n3 as usize];
            if (n4 >> 28 & n2) != 0 {
                n4 = CASE_TABLE_COMPLEX[n3 as usize + 1];
            }
            let mut n5 = n4 & 0xFFFF;
            let n6 = n4 >> 26 & 3;
            if n6 == 0 {
                out[0] = n;
                return 1;
            }
            for slot in out.iter_mut().take(n6 as usize) {
                *slot = CASE_TABLE_CP[n5 as usize] as i32;
                n5 += 1;
            }
            n6
        }
        _ => 0,
    }
}

pub fn get_full_lower_case(n: i32, n2: i32, out: &mut [i32; 3]) -> i32 {
    let mut n4 = search_range_table(&CASE_TABLE, 9, 19, 399, n);
    match n4 >> 29 & 7 {
        0 | 4 => {
            out[0] = n;
            1
        }
        1 => {
            out[0] = if jmod2(n) == 0 { n + 1 } else { n };
            1
        }
        2 => {
            out[0] = if jmod2(n) == 1 { n + 1 } else { n };
            1
        }
        3 => {
            let n3 = n4 >> 21 & 0xFF;
            out[0] = n + 2 - n3;
            1
        }
        5 => {
            out[0] = n + CASE_TABLE_DELTA[(n4 >> 21 & 0xFF) as usize];
            1
        }
        6 => {
            let n3 = n4 >> 21 & 0xFF;
            n4 = CASE_TABLE_COMPLEX[n3 as usize];
            if (n4 >> 28 & n2) != 0 {
                n4 = CASE_TABLE_COMPLEX[n3 as usize + 1];
            }
            let mut n5 = n4 & 0xFFFF;
            n5 += n4 >> 26 & 3;
            n5 += n4 >> 24 & 3;
            let n6 = n4 >> 22 & 3;
            if n6 == 0 {
                out[0] = n;
                return 1;
            }
            for slot in out.iter_mut().take(n6 as usize) {
                *slot = CASE_TABLE_CP[n5 as usize] as i32;
                n5 += 1;
            }
            n6
        }
        _ => 0,
    }
}

pub fn search_range_table(a: &[i32], n: i32, n2: i32, n3: i32, n4: i32) -> i32 {
    let (mut n5, mut n6);
    if n4 < 128 {
        n5 = 0;
        n6 = n;
    } else if n4 < 256 {
        n5 = n - 1;
        n6 = n2;
    } else {
        n5 = n2 - 1;
        n6 = n3;
    }
    while n5 < n6 {
        let n7 = (n5 + n6) / 2;
        let n8 = a[n7 as usize] & 0x1FFFFF;
        let n9 = a[n7 as usize + 1] & 0x1FFFFF;
        if n4 < n8 {
            n6 = n7;
            continue;
        }
        if n9 <= n4 {
            n5 = n7 + 1;
            continue;
        }
        return a[n7 as usize];
    }
    0
}
