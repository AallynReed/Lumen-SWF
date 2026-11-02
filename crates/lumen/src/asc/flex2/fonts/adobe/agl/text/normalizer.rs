//! NFC as aglj40.jar `com.adobe.agl.text.Normalizer` (`new Normalizer(iter, Normalizer.NFC, 0)` then `next()`)
//! computes it, over the Unicode 5.1 tables dumped from the jar.

use crate::asc::flex2::fonts::adobe::agl::lang::u_character::get_combining_class;
use crate::asc::flex2::fonts::adobe::agl::unicode51_data::{CANONICAL_DECOMPOSITION, COMPOSITION};

const S_BASE: i32 = 0xAC00;
const L_BASE: i32 = 0x1100;
const V_BASE: i32 = 0x1161;
const T_BASE: i32 = 0x11A7;
const L_COUNT: i32 = 19;
const V_COUNT: i32 = 21;
const T_COUNT: i32 = 28;
const S_COUNT: i32 = L_COUNT * V_COUNT * T_COUNT;

/// `Normalizer.DONE`.
pub const DONE: i32 = -1;

pub struct Normalizer {
    out: Vec<i32>,
    pos: usize,
}

impl Normalizer {
    /// `new Normalizer(UCharacterIterator.getInstance(text), Normalizer.NFC, 0)`.
    pub fn new_nfc(text: &[u16]) -> Normalizer {
        Normalizer { out: nfc(&code_points(text)), pos: 0 }
    }

    pub fn next(&mut self) -> i32 {
        if self.pos < self.out.len() {
            self.pos += 1;
            return self.out[self.pos - 1];
        }
        DONE
    }
}

/// `UCharacterIterator.nextCodePoint` over UTF-16: pairs surrogates, passes lone ones through.
pub fn code_points(text: &[u16]) -> Vec<i32> {
    let mut v = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let c = text[i] as i32;
        if (0xD800..0xDC00).contains(&c) && i + 1 < text.len() && (0xDC00..0xE000).contains(&(text[i + 1] as i32)) {
            v.push(0x10000 + ((c - 0xD800) << 10) + (text[i + 1] as i32 - 0xDC00));
            i += 2;
        } else {
            v.push(c);
            i += 1;
        }
    }
    v
}

/// `StringBuffer.appendCodePoint`.
pub fn append_code_point(buf: &mut Vec<u16>, cp: i32) {
    if cp >= 0x10000 {
        let c = cp - 0x10000;
        buf.push((0xD800 + (c >> 10)) as u16);
        buf.push((0xDC00 + (c & 0x3FF)) as u16);
    } else {
        buf.push(cp as u16);
    }
}

fn decompose(cp: i32, out: &mut Vec<i32>) {
    if (S_BASE..S_BASE + S_COUNT).contains(&cp) {
        let s = cp - S_BASE;
        out.push(L_BASE + s / (V_COUNT * T_COUNT));
        out.push(V_BASE + s % (V_COUNT * T_COUNT) / T_COUNT);
        if s % T_COUNT != 0 {
            out.push(T_BASE + s % T_COUNT);
        }
        return;
    }
    match CANONICAL_DECOMPOSITION.binary_search_by_key(&cp, |e| e.0) {
        Ok(i) => out.extend_from_slice(CANONICAL_DECOMPOSITION[i].1),
        Err(_) => out.push(cp),
    }
}

fn compose_pair(a: i32, b: i32) -> Option<i32> {
    if (L_BASE..L_BASE + L_COUNT).contains(&a) && (V_BASE..V_BASE + V_COUNT).contains(&b) {
        return Some(S_BASE + ((a - L_BASE) * V_COUNT + (b - V_BASE)) * T_COUNT);
    }
    if (S_BASE..S_BASE + S_COUNT).contains(&a) && (a - S_BASE) % T_COUNT == 0 && (T_BASE + 1..T_BASE + T_COUNT).contains(&b) {
        return Some(a + (b - T_BASE));
    }
    COMPOSITION.binary_search_by(|e| (e.0, e.1).cmp(&(a, b))).ok().map(|i| COMPOSITION[i].2)
}

/// Canonical decomposition, canonical ordering, then canonical composition.
pub fn nfc(input: &[i32]) -> Vec<i32> {
    let mut d = Vec::with_capacity(input.len() + 4);
    for &cp in input {
        decompose(cp, &mut d);
    }
    let cc: Vec<i32> = d.iter().map(|&c| get_combining_class(c)).collect();
    let mut idx: Vec<usize> = (0..d.len()).collect();
    let mut i = 0;
    while i < idx.len() {
        if cc[idx[i]] == 0 {
            i += 1;
            continue;
        }
        let start = i;
        while i < idx.len() && cc[idx[i]] != 0 {
            i += 1;
        }
        idx[start..i].sort_by_key(|&k| cc[k]);
    }
    let mut out: Vec<i32> = Vec::with_capacity(d.len());
    let mut starter: Option<usize> = None;
    let mut last_cc = 0;
    for &k in &idx {
        let (ch, c) = (d[k], cc[k]);
        if let Some(si) = starter {
            let adjacent = out.len() - 1 == si;
            if adjacent || (last_cc != 0 && last_cc < c) {
                if let Some(comp) = compose_pair(out[si], ch) {
                    out[si] = comp;
                    continue;
                }
            }
        }
        if c == 0 {
            starter = Some(out.len());
        }
        last_cc = c;
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(a: i32, b: i32) -> Vec<i32> {
        let mut buf = Vec::new();
        append_code_point(&mut buf, a);
        append_code_point(&mut buf, b);
        let mut n = Normalizer::new_nfc(&buf);
        let mut v = Vec::new();
        loop {
            let c = n.next();
            if c == DONE {
                break;
            }
            v.push(c);
        }
        v
    }

    /// Replays `NfcPairs.java`: per lead code point, a hash over NFC(lead + trail) for every trail.
    #[test]
    fn nfc_pairs_match_aglj40() {
        let path = std::env::var("ICU_DUMPS").unwrap_or_else(|_| "S:/Desktop/Projects/Rust-FFDec/target/fonts_ws/layout/icu".into()) + "/nfc_pairs.txt";
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        let lines: Vec<&str> = text.lines().collect();
        let set: Vec<i32> = lines[1].split_whitespace().map(|x| i32::from_str_radix(x, 16).unwrap()).collect();
        let h_at = lines.iter().position(|l| *l == "H").unwrap();
        let trail: Vec<i32> = lines[h_at + 1].split_whitespace().map(|x| i32::from_str_radix(x, 16).unwrap()).collect();
        let (mut leads, mut pairs, mut bad) = (0, 0u64, Vec::new());
        for l in &lines[2..h_at] {
            let p: Vec<&str> = l.split(' ').collect();
            let a = i32::from_str_radix(p[0], 16).unwrap();
            let trails = if (0xAC00..=0xD7A3).contains(&a) { &trail } else { &set };
            let mut h: i64 = 0;
            let mut nch = 0;
            for &b in trails.iter() {
                let r = pair(a, b);
                h = h.wrapping_mul(31).wrapping_add(b as i64);
                h = h.wrapping_mul(31).wrapping_add(r.len() as i64);
                for &x in &r {
                    h = h.wrapping_mul(31).wrapping_add(x as i64);
                }
                if !(r.len() == 2 && r[0] == a && r[1] == b) {
                    nch += 1;
                }
                pairs += 1;
            }
            leads += 1;
            if format!("{:x} {}", h as u64, nch) != format!("{} {}", p[1], p[2]) {
                bad.push(format!("{:x}: java {} {} rust {:x} {}", a, p[1], p[2], h as u64, nch));
            }
        }
        eprintln!("nfc: {} leads, {} pairs, {} leads differ", leads, pairs, bad.len());
        for b in bad.iter().take(30) {
            eprintln!("{}", b);
        }
        assert!(bad.is_empty());
    }
}
