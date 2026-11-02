//! The parts of aglj40.jar `com.adobe.agl.lang.UCharacter` (Adobe's ICU4J 4.0, Unicode 5.1) that afe uses, over
//! tables dumped from the jar.

use crate::asc::flex2::fonts::adobe::agl::unicode51_data::{CANONICAL_EQUIVALENTS, COMBINING_CLASS, MIRROR};

/// `UProperty.DECOMPOSITION_TYPE`.
pub const DECOMPOSITION_TYPE: i32 = 4099;
/// `UCharacter.DecompositionType.CANONICAL`.
pub const DT_CANONICAL: i32 = 1;

pub fn get_mirror(cp: i32) -> i32 {
    match MIRROR.binary_search_by_key(&cp, |e| e.0) {
        Ok(i) => MIRROR[i].1,
        Err(_) => cp,
    }
}

pub fn get_combining_class(cp: i32) -> i32 {
    match COMBINING_CLASS.binary_search_by_key(&cp, |e| e.0) {
        Ok(i) => COMBINING_CLASS[i].1 as i32,
        Err(_) => 0,
    }
}

pub fn is_hangul_syllable(cp: i32) -> bool {
    (0xAC00..=0xD7A3).contains(&cp)
}

/// `getIntPropertyValue(cp, DECOMPOSITION_TYPE) == DecompositionType.CANONICAL`; the other decomposition types are
/// not tabulated.
pub fn has_canonical_decomposition_type(cp: i32) -> bool {
    is_hangul_syllable(cp) || CANONICAL_EQUIVALENTS.binary_search_by_key(&cp, |e| e.0).is_ok()
}

/// Every code point with a canonical decomposition type, ascending.
pub fn canonical_decomposition_code_points() -> Vec<i32> {
    let mut v: Vec<i32> = CANONICAL_EQUIVALENTS.iter().map(|e| e.0).collect();
    v.extend(0xAC00..=0xD7A3);
    v.sort_unstable();
    v
}
