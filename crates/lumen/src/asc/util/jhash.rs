//! `java.util.HashMap`/`HashSet` with Java's iteration order, plus Java `String.compareTo` ordering, for ported code whose output depends on them.

use std::borrow::Borrow;
use std::cmp::Ordering;

/// A key's Java `hashCode()`.
pub trait JavaHash {
    fn java_hash(&self) -> i32;
}

/// Java `String.hashCode()` over the UTF-16 code units.
pub fn string_hash(s: &str) -> i32 {
    crate::asc::util::jstr::java_hash_code(s)
}

impl JavaHash for str {
    fn java_hash(&self) -> i32 {
        string_hash(self)
    }
}

impl JavaHash for String {
    fn java_hash(&self) -> i32 {
        string_hash(self)
    }
}

impl<T: JavaHash + ?Sized> JavaHash for &T {
    fn java_hash(&self) -> i32 {
        (**self).java_hash()
    }
}

impl JavaHash for i32 {
    fn java_hash(&self) -> i32 {
        *self
    }
}

impl JavaHash for i64 {
    fn java_hash(&self) -> i32 {
        (*self ^ ((*self as u64) >> 32) as i64) as i32
    }
}

impl JavaHash for u16 {
    fn java_hash(&self) -> i32 {
        *self as i32
    }
}

impl JavaHash for bool {
    fn java_hash(&self) -> i32 {
        if *self {
            1231
        } else {
            1237
        }
    }
}

impl JavaHash for f64 {
    fn java_hash(&self) -> i32 {
        // `Double.doubleToLongBits` canonicalises NaN.
        let bits = if self.is_nan() { 0x7ff8000000000000i64 } else { self.to_bits() as i64 };
        bits.java_hash()
    }
}

impl<T: JavaHash> JavaHash for Option<T> {
    fn java_hash(&self) -> i32 {
        self.as_ref().map_or(0, |v| v.java_hash())
    }
}

/// `List.hashCode()`.
impl<T: JavaHash> JavaHash for [T] {
    fn java_hash(&self) -> i32 {
        let mut h: i32 = 1;
        for e in self {
            h = h.wrapping_mul(31).wrapping_add(e.java_hash());
        }
        h
    }
}

impl<T: JavaHash> JavaHash for Vec<T> {
    fn java_hash(&self) -> i32 {
        self.as_slice().java_hash()
    }
}

const DEFAULT_INITIAL_CAPACITY: usize = 16;
const MAXIMUM_CAPACITY: usize = 1 << 30;
const TREEIFY_THRESHOLD: usize = 8;
const MIN_TREEIFY_CAPACITY: usize = 64;

/// `HashMap.hash(key)`.
fn spread(h: i32) -> i32 {
    h ^ ((h as u32) >> 16) as i32
}

/// `HashMap.tableSizeFor(cap)`.
fn table_size_for(cap: usize) -> usize {
    cap.max(1).next_power_of_two().min(MAXIMUM_CAPACITY)
}

/// `java.util.HashMap` (JDK 8+) without tree bins: buckets keep Java's node order, including the resize split and
/// the forced resize `treeifyBin` does while the table is smaller than 64. A bin of more than 8 keys in a table of 64
/// or more would be treeified by Java, whose order this does not reproduce.
#[derive(Clone)]
pub struct JHashMap<K, V> {
    table: Vec<Vec<(i32, K, V)>>,
    size: usize,
    threshold: usize,
    treeified: bool,
}

impl<K: JavaHash + Eq, V> Default for JHashMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: JavaHash + Eq, V> JHashMap<K, V> {
    pub fn new() -> Self {
        JHashMap { table: Vec::new(), size: 0, threshold: 0, treeified: false }
    }

    /// `new HashMap(initialCapacity)`.
    pub fn with_capacity(initial_capacity: usize) -> Self {
        JHashMap { table: Vec::new(), size: 0, threshold: table_size_for(initial_capacity), treeified: false }
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// True once a bin got big enough that Java would have turned it into a tree, after which iteration order may differ.
    pub fn order_may_differ_from_java(&self) -> bool {
        self.treeified
    }

    fn resize(&mut self) {
        let old_cap = self.table.len();
        let old_thr = self.threshold;
        let (new_cap, new_thr);
        if old_cap > 0 {
            if old_cap >= MAXIMUM_CAPACITY {
                self.threshold = usize::MAX;
                return;
            }
            new_cap = old_cap << 1;
            new_thr = if new_cap < MAXIMUM_CAPACITY && old_cap >= DEFAULT_INITIAL_CAPACITY {
                old_thr << 1
            } else {
                let ft = new_cap as f32 * 0.75;
                if new_cap < MAXIMUM_CAPACITY && ft < MAXIMUM_CAPACITY as f32 {
                    ft as usize
                } else {
                    usize::MAX
                }
            };
        } else if old_thr > 0 {
            new_cap = old_thr;
            let ft = new_cap as f32 * 0.75;
            new_thr = if new_cap < MAXIMUM_CAPACITY && ft < MAXIMUM_CAPACITY as f32 { ft as usize } else { usize::MAX };
        } else {
            new_cap = DEFAULT_INITIAL_CAPACITY;
            new_thr = (DEFAULT_INITIAL_CAPACITY as f32 * 0.75) as usize;
        }
        self.threshold = new_thr;
        let old = std::mem::take(&mut self.table);
        self.table = (0..new_cap).map(|_| Vec::new()).collect();
        for (j, bin) in old.into_iter().enumerate() {
            for (h, k, v) in bin {
                let idx = if (h as u32 as usize) & old_cap == 0 { j } else { j + old_cap };
                self.table[idx].push((h, k, v));
            }
        }
    }

    fn index(&self, h: i32) -> usize {
        (self.table.len() - 1) & (h as u32 as usize)
    }

    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: JavaHash + Eq + ?Sized,
    {
        if self.table.is_empty() {
            return None;
        }
        let h = spread(key.java_hash());
        self.table[self.index(h)].iter().find(|(eh, k, _)| *eh == h && k.borrow() == key).map(|(_, _, v)| v)
    }

    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: JavaHash + Eq + ?Sized,
    {
        if self.table.is_empty() {
            return None;
        }
        let h = spread(key.java_hash());
        let idx = self.index(h);
        self.table[idx].iter_mut().find(|(eh, k, _)| *eh == h && k.borrow() == key).map(|(_, _, v)| v)
    }

    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: JavaHash + Eq + ?Sized,
    {
        self.get(key).is_some()
    }

    /// `put`: returns the previous value; an existing key keeps its position.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if self.table.is_empty() {
            self.resize();
        }
        let h = spread(key.java_hash());
        let idx = self.index(h);
        if let Some(e) = self.table[idx].iter_mut().find(|(eh, k, _)| *eh == h && *k == key) {
            return Some(std::mem::replace(&mut e.2, value));
        }
        let bin_count = self.table[idx].len();
        self.table[idx].push((h, key, value));
        if bin_count >= TREEIFY_THRESHOLD - 1 {
            if self.table.len() < MIN_TREEIFY_CAPACITY {
                self.resize();
            } else {
                self.treeified = true;
            }
        }
        self.size += 1;
        if self.size > self.threshold {
            self.resize();
        }
        None
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: JavaHash + Eq + ?Sized,
    {
        if self.table.is_empty() {
            return None;
        }
        let h = spread(key.java_hash());
        let idx = self.index(h);
        let pos = self.table[idx].iter().position(|(eh, k, _)| *eh == h && k.borrow() == key)?;
        self.size -= 1;
        Some(self.table[idx].remove(pos).2)
    }

    /// `clear()` keeps the table's capacity, as Java does.
    pub fn clear(&mut self) {
        for bin in &mut self.table {
            bin.clear();
        }
        self.size = 0;
    }

    /// `putAll(m)`, including its pre-sizing.
    pub fn put_all(&mut self, m: impl IntoIterator<Item = (K, V)>, m_size: usize) {
        if m_size > 0 {
            if self.table.is_empty() {
                let ft = (m_size as f32 / 0.75) + 1.0;
                let t = if ft < MAXIMUM_CAPACITY as f32 { ft as usize } else { MAXIMUM_CAPACITY };
                if t > self.threshold {
                    self.threshold = table_size_for(t);
                }
            } else {
                while m_size > self.threshold && self.table.len() < MAXIMUM_CAPACITY {
                    self.resize();
                }
            }
            for (k, v) in m {
                self.insert(k, v);
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.table.iter().flat_map(|b| b.iter().map(|(_, k, v)| (k, v)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)> {
        self.table.iter_mut().flat_map(|b| b.iter_mut().map(|(_, k, v)| (&*k, v)))
    }

    /// Keys changed through this stay in their old bucket, as a Java key mutated in place does.
    pub fn keys_mut_in_place(&mut self) -> impl Iterator<Item = &mut K> {
        self.table.iter_mut().flat_map(|b| b.iter_mut().map(|(_, k, _)| k))
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }
}

/// `java.util.HashSet`, a `JHashMap` with unit values.
#[derive(Clone)]
pub struct JHashSet<K> {
    map: JHashMap<K, ()>,
}

impl<K: JavaHash + Eq> Default for JHashSet<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: JavaHash + Eq> JHashSet<K> {
    pub fn new() -> Self {
        JHashSet { map: JHashMap::new() }
    }

    pub fn with_capacity(initial_capacity: usize) -> Self {
        JHashSet { map: JHashMap::with_capacity(initial_capacity) }
    }

    /// `new HashSet(collection)`.
    pub fn from_collection(items: impl IntoIterator<Item = K>, size: usize) -> Self {
        let cap = ((size as f32 / 0.75) as usize + 1).max(16);
        let mut s = Self::with_capacity(cap);
        for k in items {
            s.insert(k);
        }
        s
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// `add`: true when the key was not present.
    pub fn insert(&mut self, key: K) -> bool {
        self.map.insert(key, ()).is_none()
    }

    pub fn contains<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: JavaHash + Eq + ?Sized,
    {
        self.map.contains_key(key)
    }

    pub fn remove<Q>(&mut self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: JavaHash + Eq + ?Sized,
    {
        self.map.remove(key).is_some()
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &K> {
        self.map.keys()
    }

    pub fn order_may_differ_from_java(&self) -> bool {
        self.map.order_may_differ_from_java()
    }

    pub fn keys_mut_in_place(&mut self) -> impl Iterator<Item = &mut K> {
        self.map.keys_mut_in_place()
    }
}

/// Java `String.compareTo`: lexicographic over UTF-16 code units.
pub fn java_compare(a: &str, b: &str) -> Ordering {
    crate::asc::util::jstr::compare(a, b)
}

/// Java `String.compareTo` as a difference, as Java returns it.
pub fn java_compare_to(a: &str, b: &str) -> i32 {
    crate::asc::util::jstr::compare_to(a, b)
}

/// A `String` key ordered like Java's `String.compareTo`, for `TreeMap<String, _>` ports.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct JStr(pub String);

impl PartialOrd for JStr {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for JStr {
    fn cmp(&self, other: &Self) -> Ordering {
        java_compare(&self.0, &other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_hash_matches_java() {
        assert_eq!(string_hash("hello"), 99162322);
        assert_eq!(string_hash(""), 0);
        // A supplementary character hashes as its two UTF-16 surrogates.
        assert_eq!(string_hash("\u{10000}"), 0xD800 * 31 + 0xDC00);
    }

    #[test]
    fn table_size_for_matches_java() {
        for (cap, want) in [(0, 1), (1, 1), (2, 2), (3, 4), (16, 16), (17, 32), (100, 128), (64, 64)] {
            assert_eq!(table_size_for(cap), want, "cap {}", cap);
        }
    }

    #[test]
    fn iteration_order_matches_java() {
        let mut m = JHashMap::new();
        for k in ["b", "a", "c"] {
            m.insert(k.to_string(), 0);
        }
        let keys: Vec<_> = m.keys().cloned().collect();
        assert_eq!(keys, ["a", "b", "c"]);
        let mut m = JHashMap::new();
        for i in 0..20 {
            m.insert(format!("k{}", i), i);
        }
        assert_eq!(m.len(), 20);
        assert_eq!(m.get("k7"), Some(&7));
        assert_eq!(m.remove("k7"), Some(7));
        assert_eq!(m.get("k7"), None);
    }

    #[test]
    fn matches_java_hashmap_order() {
        // Orders printed by java.util.HashMap (JDK 17), with U+00E9 shown as '~'.
        let show = |keys: Vec<String>| keys.join(" ").replace('\u{e9}', "~");
        let mut m = JHashMap::new();
        for i in 0..100 {
            m.insert(format!("k{}", i), i);
        }
        for i in (0..100).step_by(7) {
            m.remove(format!("k{}", i).as_str());
        }
        for (k, v) in [("Aa", 1), ("BB", 2), ("flash.display:Sprite", 3), ("\u{e9}t\u{e9}", 4)] {
            m.insert(k.to_string(), v);
        }
        assert_eq!(show(m.keys().cloned().collect()), "k31 k30 k33 k32 k34 k37 k36 k39 k38 k1 k40 k2 k3 k4 k41 k5 k44 k6 k43 k46 k8 k45 k9 k48 k47 flash.display:Sprite Aa BB k51 k50 k53 k52 k55 k54 k57 k59 k58 k60 k62 k61 k64 k66 k65 k68 k67 k69 k71 k73 k72 k75 k74 k76 k79 k78 ~t~ k80 k82 k81 k83 k86 k85 k88 k87 k89 k90 k93 k92 k95 k94 k97 k96 k11 k99 k10 k13 k12 k15 k17 k16 k19 k18 k20 k22 k24 k23 k26 k25 k27 k29");
        let mut c = JHashMap::new();
        for i in 0..9 {
            c.insert(i * 64, i);
        }
        c.insert(5, 5);
        assert_eq!(show(c.keys().map(|k| k.to_string()).collect()), "0 64 128 192 256 320 384 448 512 5");
        let mut p = JHashMap::with_capacity(3);
        let entries: Vec<(String, i32)> = m.iter().map(|(k, v)| (k.clone(), *v)).collect();
        let n = entries.len();
        p.put_all(entries, n);
        assert_eq!(show(p.keys().cloned().collect()), "k71 k73 k72 k31 k75 k30 k74 k33 k32 k76 k79 k34 k78 k37 k36 k39 k38 ~t~ k80 k82 k81 k1 k40 k2 k83 k3 k86 k4 k41 k85 k5 k44 k88 k6 k43 k87 k46 k8 k45 k89 k9 k48 k47 flash.display:Sprite Aa BB k90 k93 k92 k51 k95 k50 k94 k53 k97 k52 k96 k55 k11 k99 k54 k10 k57 k13 k12 k59 k15 k58 k17 k16 k19 k18 k60 k62 k61 k64 k20 k66 k22 k65 k68 k24 k67 k23 k26 k69 k25 k27 k29");
    }
}
