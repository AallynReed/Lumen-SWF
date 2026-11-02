//! aglj40.jar `com.adobe.agl.text.CanonicalIterator` over single code points, from strings dumped from the jar.
//! Hangul syllables are generated; their order is not Java's `HashSet` order, which no caller depends on.

use crate::asc::flex2::fonts::adobe::agl::lang::u_character::is_hangul_syllable;
use crate::asc::flex2::fonts::adobe::agl::unicode51_data::CANONICAL_EQUIVALENTS;

pub struct CanonicalIterator {
    strings: Vec<Vec<i32>>,
    pos: usize,
}

impl CanonicalIterator {
    /// `new CanonicalIterator(String.valueOf(cp))` for a code point with a canonical decomposition type; any other
    /// code point yields only itself.
    pub fn new(cp: i32) -> CanonicalIterator {
        let strings = if is_hangul_syllable(cp) {
            let s = cp - 0xAC00;
            let (l, v, t) = (0x1100 + s / 588, 0x1161 + s % 588 / 28, s % 28);
            if t == 0 {
                vec![vec![cp], vec![l, v]]
            } else {
                vec![vec![cp], vec![l, v, 0x11A7 + t], vec![cp - t, 0x11A7 + t]]
            }
        } else {
            match CANONICAL_EQUIVALENTS.binary_search_by_key(&cp, |e| e.0) {
                Ok(i) => CANONICAL_EQUIVALENTS[i].1.iter().map(|s| s.to_vec()).collect(),
                Err(_) => vec![vec![cp]],
            }
        };
        CanonicalIterator { strings, pos: 0 }
    }

    /// The next equivalent string as code points; `None` where Java returns null.
    pub fn next(&mut self) -> Option<&[i32]> {
        if self.pos < self.strings.len() {
            self.pos += 1;
            return Some(&self.strings[self.pos - 1]);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asc::flex2::fonts::adobe::agl::lang::u_character::{canonical_decomposition_code_points, get_combining_class, get_mirror, has_canonical_decomposition_type};

    /// Checks the tables against `icu_tables.txt` from `IcuDump.java`, including the generated Hangul strings.
    #[test]
    fn tables_match_aglj40() {
        let path = std::env::var("ICU_DUMPS").unwrap_or_else(|_| "S:/Desktop/Projects/Rust-FFDec/target/fonts_ws/layout/icu".into()) + "/icu_tables.txt";
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        let h = |s: &str| i32::from_str_radix(s, 16).unwrap();
        let (mut mirrors, mut ccc, mut canon) = (std::collections::HashMap::new(), std::collections::HashMap::new(), Vec::new());
        for l in text.lines() {
            let p: Vec<&str> = l.split(' ').collect();
            match p[0] {
                "M" => {
                    mirrors.insert(h(p[1]), h(p[2]));
                }
                "C" => {
                    ccc.insert(h(p[1]), p[2].parse::<i32>().unwrap());
                }
                "T" => {
                    let cp = h(p[1]);
                    canon.push(cp);
                    let want: Vec<Vec<i32>> = p[2..].iter().map(|s| s.split(',').map(h).collect()).collect();
                    let mut it = CanonicalIterator::new(cp);
                    let mut got = Vec::new();
                    while let Some(s) = it.next() {
                        got.push(s.to_vec());
                    }
                    if is_hangul_syllable(cp) {
                        let (mut a, mut b) = (got.clone(), want.clone());
                        a.sort();
                        b.sort();
                        assert_eq!(a, b, "{:x}", cp);
                    } else {
                        assert_eq!(got, want, "{:x}", cp);
                    }
                }
                _ => {}
            }
        }
        for cp in 0..=0x10FFFF {
            assert_eq!(get_mirror(cp), *mirrors.get(&cp).unwrap_or(&cp));
            assert_eq!(get_combining_class(cp), *ccc.get(&cp).unwrap_or(&0));
            assert_eq!(has_canonical_decomposition_type(cp), canon.binary_search(&cp).is_ok());
        }
        assert_eq!(canonical_decomposition_code_points(), canon);
    }
}
