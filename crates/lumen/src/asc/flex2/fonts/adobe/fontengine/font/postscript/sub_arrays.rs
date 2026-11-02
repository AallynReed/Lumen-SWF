//! Port of afe.jar `com.adobe.fontengine.font.postscript.SubArrays` (`arrayCompare`).

/// `SubArrays.arrayCompare(a, n, b, n2, n3)`: both ranges must run past their arrays' ends by the same amount.
pub fn array_compare(a: &[u8], mut n: i32, b: &[u8], mut n2: i32, n3: i32) -> bool {
    let mut n4 = n + n3;
    let mut n5 = 0;
    let mut n6 = 0;
    if (a.len() as i32) < n4 {
        n5 = n4 - a.len() as i32;
    }
    if (b.len() as i32) < n2 + n3 {
        n6 = n2 + n3 - b.len() as i32;
    }
    if n5 != n6 {
        return false;
    }
    n4 -= n6;
    while n < n4 {
        if a[n as usize] != b[n2 as usize] {
            return false;
        }
        n += 1;
        n2 += 1;
    }
    true
}

pub struct SubArrays;

impl SubArrays {
    pub fn array_compare(a: &[u8], n: i32, b: &[u8], n2: i32, n3: i32) -> bool {
        array_compare(a, n, b, n2, n3)
    }
}
