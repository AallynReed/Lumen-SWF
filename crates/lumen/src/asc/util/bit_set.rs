//! Port of `macromedia/asc/util/BitSet.java`. A shared handle: Java's in-place updates (`set`, `reset_set`) are seen by every holder.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

#[derive(Clone)]
pub struct BitSet(Rc<RefCell<Option<Vec<i64>>>>);

fn size(s: &Option<BitSet>) -> usize {
    match s {
        Some(s) => s.0.borrow().as_ref().map_or(0, |b| b.len() << 6),
        None => 0,
    }
}

fn word(s: &BitSet, i: usize) -> i64 {
    s.0.borrow().as_ref().unwrap()[i]
}

fn words(s: &BitSet) -> Vec<i64> {
    s.0.borrow().clone().unwrap_or_default()
}

impl BitSet {
    fn with_bits(nbits: usize) -> BitSet {
        BitSet(Rc::new(RefCell::new(if nbits > 0 { Some(vec![0; (nbits + 63) >> 6]) } else { None })))
    }

    fn copy_of(s: &BitSet) -> BitSet {
        let t = BitSet::with_bits(size(&Some(s.clone())));
        if let Some(b) = s.0.borrow().as_ref() {
            if !b.is_empty() {
                *t.0.borrow_mut() = Some(b.clone());
            }
        }
        t
    }

    pub fn ptr_eq(&self, other: &BitSet) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn and(a: Option<BitSet>, b: Option<BitSet>) -> Option<BitSet> {
        if Self::is_empty(&a) || Self::is_empty(&b) {
            return None;
        }
        let minsize = size(&a).min(size(&b));
        let minlen = minsize >> 6;
        let t = BitSet::with_bits(minsize);
        let (a, b) = (a.unwrap(), b.unwrap());
        {
            let mut tb = t.0.borrow_mut();
            let tb = tb.as_mut().unwrap();
            for i in 0..minlen {
                tb[i] = word(&a, i) & word(&b, i);
            }
        }
        Some(t)
    }

    fn reset(&self, r: &Option<BitSet>) {
        let minlen = size(&Some(self.clone())).min(size(r)) >> 6;
        let rw = r.as_ref().map(words).unwrap_or_default();
        let mut b = self.0.borrow_mut();
        for i in 0..minlen {
            b.as_mut().unwrap()[i] &= !rw[i];
        }
    }

    pub fn or(a: Option<BitSet>, b: Option<BitSet>) -> Option<BitSet> {
        Self::combine(a, b, |x, y| x | y)
    }

    fn combine(a: Option<BitSet>, b: Option<BitSet>, op: fn(i64, i64) -> i64) -> Option<BitSet> {
        if Self::is_empty(&a) {
            return b;
        }
        if Self::is_empty(&b) {
            return a;
        }
        let minlen = size(&a).min(size(&b)) >> 6;
        let maxlen = size(&a).max(size(&b)) >> 6;
        let t = BitSet::with_bits(maxlen << 6);
        let (aw, bw) = (words(a.as_ref().unwrap()), words(b.as_ref().unwrap()));
        {
            let mut tb = t.0.borrow_mut();
            let tb = tb.as_mut().unwrap();
            for i in 0..minlen {
                tb[i] = op(aw[i], bw[i]);
            }
            if aw.len() > minlen {
                for i in (minlen..aw.len()).rev() {
                    tb[i] = aw[i];
                }
            } else if bw.len() > minlen {
                for i in (minlen..bw.len()).rev() {
                    tb[i] = bw[i];
                }
            }
        }
        Some(t)
    }

    fn set_bits(&self, s: &Option<BitSet>) {
        if size(s) > size(&Some(self.clone())) {
            let mut b = vec![0i64; size(s) >> 6];
            if let Some(old) = self.0.borrow().as_ref() {
                b[..old.len()].copy_from_slice(old);
            }
            *self.0.borrow_mut() = Some(b);
        }
        if let Some(s) = s {
            let sw = s.0.borrow().clone();
            if let Some(sw) = sw {
                let mut b = self.0.borrow_mut();
                let b = b.as_mut().unwrap();
                for i in (0..sw.len()).rev() {
                    b[i] |= sw[i];
                }
            }
        }
    }

    /// `set(target, s)`: `target |= s`, in place when `target` exists.
    pub fn set(target: Option<BitSet>, s: Option<BitSet>) -> Option<BitSet> {
        let mut target = target;
        if size(&s) > 0 {
            if target.is_none() {
                target = Some(BitSet::with_bits(size(&s)));
            }
            target.as_ref().unwrap().set_bits(&s);
        }
        target
    }

    pub fn reset_set(target: Option<BitSet>, r: Option<BitSet>, s: Option<BitSet>) -> Option<BitSet> {
        let mut target = target;
        if let Some(t) = &target {
            t.reset(&r);
        }
        if s.is_some() {
            if target.is_none() {
                target = Some(BitSet::with_bits(size(&s)));
            }
            target.as_ref().unwrap().set_bits(&s);
        }
        target
    }

    pub fn xor(a: Option<BitSet>, b: Option<BitSet>) -> Option<BitSet> {
        Self::combine(a, b, |x, y| x ^ y)
    }

    pub fn is_empty(b: &Option<BitSet>) -> bool {
        match b {
            None => true,
            Some(b) => b.0.borrow().as_ref().map_or(true, |w| w.iter().all(|&x| x == 0)),
        }
    }

    pub fn get(b: &Option<BitSet>, index: i32) -> bool {
        match b {
            None => false,
            Some(b) => {
                assert!(index >= 0);
                let i = (index >> 6) as usize;
                b.0.borrow().as_ref().map_or(false, |w| i < w.len() && (w[i] & (1i64 << (index & 63))) != 0)
            }
        }
    }

    /// `set(target, index, value)`.
    pub fn set_index(target: Option<BitSet>, index: i32, value: bool) -> Option<BitSet> {
        assert!(index >= 0);
        let i = (index >> 6) as usize;
        let mut target = target;
        if index as usize >= size(&target) {
            if target.is_none() {
                target = Some(BitSet::with_bits(index as usize + 1));
            }
            let t = target.as_ref().unwrap();
            let mut b = vec![0i64; i + 1];
            if let Some(old) = t.0.borrow().as_ref() {
                b[..old.len()].copy_from_slice(old);
            }
            *t.0.borrow_mut() = Some(b);
        }
        {
            let t = target.as_ref().unwrap();
            let mut b = t.0.borrow_mut();
            let b = b.as_mut().unwrap();
            if value {
                b[i] |= 1i64 << (index & 63);
            } else {
                b[i] &= !(1i64 << (index & 63));
            }
        }
        target
    }

    pub fn copy(s: &Option<BitSet>) -> Option<BitSet> {
        match s {
            Some(b) if !Self::is_empty(s) => Some(BitSet::copy_of(b)),
            _ => None,
        }
    }

    pub fn equals(a: &Option<BitSet>, b: &Option<BitSet>) -> bool {
        let minlen = size(a).min(size(b)) >> 6;
        let aw = a.as_ref().map(words).unwrap_or_default();
        let bw = b.as_ref().map(words).unwrap_or_default();
        for i in 0..minlen {
            if aw[i] != bw[i] {
                return false;
            }
        }
        if aw.iter().skip(minlen).any(|&x| x != 0) {
            return false;
        }
        if bw.iter().skip(minlen).any(|&x| x != 0) {
            return false;
        }
        true
    }

    pub fn hash_code(&self) -> i32 {
        let mut h: i32 = 0;
        if let Some(b) = self.0.borrow().as_ref() {
            for &w in b.iter().rev() {
                h ^= w as i32;
            }
        }
        h
    }

    pub fn and_count(a: &Option<BitSet>, b: &Option<BitSet>) -> i32 {
        let minlen = size(a).min(size(b)) >> 6;
        let aw = a.as_ref().map(words).unwrap_or_default();
        let bw = b.as_ref().map(words).unwrap_or_default();
        (0..minlen).map(|i| (aw[i] & bw[i]).count_ones() as i32).sum()
    }

    pub fn count(s: &Option<BitSet>) -> i32 {
        s.as_ref().map(words).unwrap_or_default().iter().map(|w| w.count_ones() as i32).sum()
    }

    pub fn next_set_bit(s: &Option<BitSet>, i: i32) -> i32 {
        assert!(i >= 0);
        let w = match s {
            Some(s) => words(s),
            None => return -1,
        };
        let mut j = (i >> 6) as usize;
        if j >= w.len() {
            return -1;
        }
        let mut value = w[j] & !((1i64 << (i & 63)).wrapping_sub(1));
        loop {
            if value != 0 {
                return value.trailing_zeros() as i32 + ((j as i32) << 6);
            }
            j += 1;
            if j >= w.len() {
                return -1;
            }
            value = w[j];
        }
    }
}

impl fmt::Display for BitSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut b = String::from("{");
        if let Some(w) = self.0.borrow().as_ref() {
            for i in 0..(w.len() << 6) {
                if (w[i >> 6] & (1i64 << (i & 63))) != 0 {
                    if b.len() > 1 {
                        b.push_str(", ");
                    }
                    b.push_str(&i.to_string());
                }
            }
        }
        b.push('}');
        f.write_str(&b)
    }
}

impl fmt::Debug for BitSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
