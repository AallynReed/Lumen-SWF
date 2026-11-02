//! Port of afe.jar `com.adobe.fontengine.font.SWFFont4Description` and its `TLFSubsetCompletionIterator` and
//! `FTESubsetCompletionIterator`.

use std::collections::HashMap;
use std::io::Write;
use std::sync::OnceLock;

use crate::asc::flex2::fonts::adobe::agl::lang::u_character;
use crate::asc::flex2::fonts::adobe::agl::text::canonical_iterator::CanonicalIterator;
use crate::asc::flex2::fonts::adobe::agl::text::normalizer::{append_code_point, Normalizer, DONE};
use crate::asc::flex2::fonts::adobe::fontengine::font::permission::Permission;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;
use crate::asc::flex2::fonts::adobe::fontengine::properties;
use crate::asc::util::jhash::JHashSet;

pub trait SWFFont4Description {
    fn can_display(&self, n: i32) -> FontResult<bool>;
    fn get_first_char(&self) -> FontResult<i32>;
    fn get_last_char(&self) -> FontResult<i32>;
    fn get_permissions(&self) -> FontResult<Permission>;
    fn get_family(&self) -> FontResult<Option<String>>;
    fn get_sub_family(&self) -> FontResult<Option<String>>;
    fn is_bold(&self) -> FontResult<bool>;
    fn is_italic(&self) -> FontResult<bool>;
    fn stream_font_data(&self, it: &mut dyn Iterator<Item = i32>, out: &mut dyn Write) -> FontResult<()>;
    /// `streamFontData(OutputStream)`.
    fn stream_font_data_all(&self, out: &mut dyn Write) -> FontResult<()>;
}

/// `TLFSubsetCompletionIterator`: the wrapped sequence, then the members of `HashSet{9, 8232, 8233}`.
pub struct TLFSubsetCompletionIterator {
    items: std::vec::IntoIter<i32>,
}

impl TLFSubsetCompletionIterator {
    pub fn new(iter: impl IntoIterator<Item = i32>) -> TLFSubsetCompletionIterator {
        let mut v: Vec<i32> = iter.into_iter().collect();
        let mut hash_set = JHashSet::new();
        for n in [9, 8232, 8233] {
            hash_set.insert(n);
        }
        v.extend(hash_set.iter().copied());
        TLFSubsetCompletionIterator { items: v.into_iter() }
    }
}

impl Iterator for TLFSubsetCompletionIterator {
    type Item = i32;
    fn next(&mut self) -> Option<i32> {
        self.items.next()
    }
}

/// `FTESubsetCompletionIterator`: yields the completed `HashSet` in Java's iteration order.
pub struct FTESubsetCompletionIterator {
    completion: std::vec::IntoIter<i32>,
}

const CROSSOVER_FOR_DECOMPOSITION: usize = 900;

/// Key/replacement pairs of `REPLACEMENT_MAP`, in Java's insertion order (a later duplicate key wins).
static REPLACEMENTS: &[(i32, &[i32])] = &[
    (9, &[32]), (10, &[32]), (13, &[32]), (133, &[32]), (173, &[32]), (847, &[32]), (4447, &[32]), (4448, &[32]), (6068, &[32]),
    (6069, &[32]), (8203, &[32]), (8204, &[32]), (8205, &[32]), (8206, &[32]), (8207, &[32]), (8232, &[32]), (8233, &[32]),
    (8234, &[32]), (8235, &[32]), (8236, &[32]), (8237, &[32]), (8238, &[32]), (8288, &[32]), (8289, &[32]), (8290, &[32]),
    (8291, &[32]), (8292, &[32]), (8293, &[32]), (8294, &[32]), (8295, &[32]), (8296, &[32]), (8297, &[32]), (8298, &[32]),
    (8299, &[32]), (8300, &[32]), (8301, &[32]), (8302, &[32]), (8303, &[32]), (12644, &[32]), (65279, &[32]), (65440, &[32]),
    (65520, &[32]), (65521, &[32]), (65522, &[32]), (65523, &[32]), (65524, &[32]), (65525, &[32]), (65526, &[32]),
    (65527, &[32]), (65528, &[32]), (119155, &[32]), (119156, &[32]), (119157, &[32]), (119158, &[32]), (119159, &[32]),
    (119160, &[32]), (119161, &[32]), (119162, &[32]), (2507, &[2503, 2494]), (2508, &[2503, 2519]), (2888, &[2887, 2902]),
    (2891, &[2887, 2878]), (2892, &[2887, 2903]), (3018, &[3014, 3006]), (3019, &[3015, 3006]), (3020, &[3014, 3031]),
    (3144, &[3142, 3158]), (3264, &[3263, 3285]), (3271, &[3270, 3285]), (3272, &[3270, 3286]), (3274, &[3270, 3266]),
    (3275, &[3270, 3266, 3285]), (3402, &[3398, 3390]), (3403, &[3399, 3390]), (3404, &[3398, 3415]), (3546, &[3545, 3530]),
    (3548, &[3545, 3535]), (3549, &[3545, 3535, 3530]), (3550, &[3545, 3551]), (3597, &[3597, 63247]), (3600, &[3600, 63232]),
    (3633, &[3633, 63248]), (3635, &[3661, 3634, 63249]), (3636, &[3636, 63233]), (3637, &[3637, 63234]),
    (3638, &[3638, 63235]), (3639, &[3639, 63236]), (3640, &[3640, 63256]), (3641, &[3641, 63257]), (3642, &[3642, 63258]),
    (3655, &[3655, 63250]), (3656, &[3656, 63251, 63242, 63237]), (3657, &[3657, 63252, 63243, 63238]),
    (3658, &[3658, 63253, 63244, 63239]), (3659, &[3659, 63254, 63245, 63240]), (3660, &[3660, 63255, 63246, 63241]),
    (3661, &[3661, 63249]), (3763, &[3789, 3762]), (3955, &[3953, 3954]), (3957, &[3953, 3956]), (3958, &[4018, 3968]),
    (3959, &[4018, 3953, 3968]), (3960, &[4019, 3968]), (3961, &[4019, 3953, 3968]), (3969, &[3953, 3968]),
    (6078, &[6081, 6078]), (6079, &[6081, 6079]), (6080, &[6081, 6080]), (6084, &[6081, 6084]), (6085, &[6081, 6085]),
    (160, &[160, 32]), (8192, &[8192, 32]), (8193, &[8193, 32]), (8194, &[8194, 32]), (8195, &[8195, 32]), (8196, &[8196, 32]),
    (8197, &[8197, 32]), (8198, &[8198, 32]), (8199, &[8199, 32, 48]), (8200, &[8200, 32, 46]), (8201, &[8201, 32]),
    (8202, &[8202, 32]), (8203, &[8203, 32]), (8239, &[8239, 32]), (8287, &[8287, 32]), (12288, &[12288, 32]),
];

fn replacement_map() -> &'static HashMap<i32, &'static [i32]> {
    static MAP: OnceLock<HashMap<i32, &'static [i32]>> = OnceLock::new();
    MAP.get_or_init(|| REPLACEMENTS.iter().copied().collect())
}

impl FTESubsetCompletionIterator {
    pub fn new(iter: impl IntoIterator<Item = i32>) -> FTESubsetCompletionIterator {
        let mut hash_set = JHashSet::new();
        let mut hash_set2 = JHashSet::new();
        Self::fill_internal_set(&mut iter.into_iter(), &mut hash_set, &mut hash_set2);
        let set = Self::case_completions(&hash_set);
        for &n in set.iter() {
            hash_set.insert(n);
        }
        for &n in set.iter() {
            hash_set2.insert(n);
        }
        let set2 = if hash_set.len() < CROSSOVER_FOR_DECOMPOSITION { Self::expand_combining_sequences(&hash_set, &hash_set2) } else { Self::decompose_unicode(&hash_set) };
        for &n in set2.iter() {
            hash_set.insert(n);
        }
        let v: Vec<i32> = hash_set.iter().copied().collect();
        FTESubsetCompletionIterator { completion: v.into_iter() }
    }

    fn fill_internal_set(iter: &mut dyn Iterator<Item = i32>, set: &mut JHashSet<i32>, set2: &mut JHashSet<i32>) {
        set.insert(32);
        set2.insert(32);
        let map = replacement_map();
        let mut queue: std::collections::VecDeque<i32> = std::collections::VecDeque::new();
        let mut it = iter.peekable();
        while it.peek().is_some() || !queue.is_empty() {
            let mut n;
            if let Some(q) = queue.pop_front() {
                n = q;
            } else {
                n = it.next().unwrap();
                let n2 = u_character::get_mirror(n);
                if n2 != n {
                    match map.get(&n2) {
                        Some(list) => queue.extend(list.iter().copied()),
                        None => queue.push_back(n2),
                    }
                }
                if let Some(list) = map.get(&n) {
                    queue.extend(list.iter().copied());
                    n = queue.pop_front().unwrap();
                }
            }
            set.insert(n);
            set2.insert(n);
        }
    }

    fn expand_combining_sequences(set: &JHashSet<i32>, set2: &JHashSet<i32>) -> JHashSet<i32> {
        let mut hash_set = JHashSet::with_capacity(set.len());
        let mut set3: Vec<i32> = set.iter().copied().collect();
        let mut buf: Vec<u16> = Vec::with_capacity(4);
        while !set3.is_empty() {
            let mut hash_set2 = JHashSet::new();
            for &n in &set3 {
                for &n2 in set2.iter() {
                    buf.clear();
                    append_code_point(&mut buf, n);
                    append_code_point(&mut buf, n2);
                    let mut normalizer = Normalizer::new_nfc(&buf);
                    loop {
                        let n3 = normalizer.next();
                        if n3 == DONE {
                            break;
                        }
                        if set.contains(&n3) || hash_set.contains(&n3) || set2.contains(&n3) {
                            continue;
                        }
                        hash_set.insert(n3);
                        hash_set2.insert(n3);
                    }
                }
            }
            set3 = hash_set2.iter().copied().collect();
        }
        hash_set
    }

    fn decompose_unicode(set: &JHashSet<i32>) -> JHashSet<i32> {
        let mut hash_set = JHashSet::new();
        for i in u_character::canonical_decomposition_code_points() {
            if i >= 0x10FFFF {
                break;
            }
            let mut ci = CanonicalIterator::new(i);
            while let Some(s) = ci.next() {
                if s.iter().all(|c| set.contains(c)) {
                    hash_set.insert(i);
                    break;
                }
            }
        }
        hash_set
    }

    fn case_completions(set: &JHashSet<i32>) -> JHashSet<i32> {
        let mut out = [0i32; 3];
        let mut hash_set = JHashSet::new();
        for n in [4, 5] {
            for &n2 in set.iter() {
                let n4 = properties::get_full_upper_case(n2, n, &mut out);
                if n4 > 0 && (n4 > 1 || n2 != out[0]) {
                    for &c in &out[..n4 as usize] {
                        hash_set.insert(c);
                    }
                }
                let n4 = properties::get_full_lower_case(n2, n, &mut out);
                if n4 <= 0 || n4 <= 1 && n2 == out[0] {
                    continue;
                }
                for &c in &out[..n4 as usize] {
                    hash_set.insert(c);
                }
            }
        }
        hash_set
    }
}

impl Iterator for FTESubsetCompletionIterator {
    type Item = i32;
    fn next(&mut self) -> Option<i32> {
        self.completion.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hexes(s: &str) -> Vec<i32> {
        s.split(',').filter(|x| !x.is_empty()).map(|x| i32::from_str_radix(x, 16).unwrap()).collect()
    }

    /// Replays `fte.txt` from `LayoutDump.java`: `FTE(TLF(ranges))` in Java's iteration order.
    #[test]
    fn completion_iterators_match_java() {
        let path = std::env::var("LAYOUT_DUMPS").unwrap_or_else(|_| "S:/Desktop/Projects/Rust-FFDec/target/fonts_ws/layout/dumps".into()) + "/fte.txt";
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        let lines: Vec<&str> = text.lines().collect();
        let (mut n, mut cps) = (0, 0);
        for w in lines.chunks(2) {
            let (name, input) = w[0]["CASE ".len()..].split_once(' ').unwrap();
            let want = hexes(&w[1]["FTE ".len()..]);
            let got: Vec<i32> = FTESubsetCompletionIterator::new(TLFSubsetCompletionIterator::new(hexes(input))).collect();
            assert_eq!(got, want, "case {}", name);
            n += 1;
            cps += want.len();
        }
        eprintln!("fte: {} cases, {} code points in Java order", n, cps);
    }
}
