//! Constant pool lookups: a value stored twice is found at its last index; type names never match.

use crate::abc::{Abc, Multiname, Namespace};

/// The last index past the placeholder whose entry matches.
fn last<T>(v: &[T], f: impl Fn(&T) -> bool) -> Option<usize> {
    (1..v.len()).rev().find(|&i| f(&v[i]))
}

fn ensure<T>(v: &mut Vec<T>, zero: T) {
    if v.is_empty() {
        v.push(zero);
    }
}

pub fn string_id(abc: &mut Abc, s: Option<&str>, add: bool) -> i64 {
    let Some(s) = s else { return 0 };
    let c = &mut abc.constants;
    if let Some(i) = last(&c.strings, |x| x == s) {
        return i as i64;
    }
    if !add {
        return -1;
    }
    ensure(&mut c.strings, String::new());
    c.strings.push(s.to_string());
    c.strings.len() as i64 - 1
}

pub fn str_id(abc: &mut Abc, s: &str) -> usize {
    string_id(abc, Some(s), true) as usize
}

pub fn namespace_id_of(abc: &mut Abc, kind: u8, name_index: i64, mut sub: usize, add: bool) -> i64 {
    if name_index == -1 {
        return -1;
    }
    let c = &mut abc.constants;
    for (n, ns) in c.namespaces.iter().enumerate().skip(1) {
        if ns.name as i64 == name_index && ns.kind == kind {
            if sub == 0 {
                return n as i64;
            }
            sub -= 1;
        }
    }
    if !add {
        return -1;
    }
    ensure(&mut c.namespaces, Namespace { kind: 0, name: 0 });
    c.namespaces.push(Namespace { kind, name: name_index as usize });
    c.namespaces.len() as i64 - 1
}

pub fn namespace_id(abc: &mut Abc, kind: u8, name: &str, sub: usize, add: bool) -> i64 {
    let n = string_id(abc, Some(name), add);
    namespace_id_of(abc, kind, n, sub, add)
}

pub fn ns(abc: &mut Abc, kind: u8, name: &str) -> usize {
    namespace_id(abc, kind, name, 0, true) as usize
}

/// `getNamespaceSetId`: every member of a stored set must be among `set`.
pub fn ns_set_id(abc: &mut Abc, set: &[usize], add: bool) -> i64 {
    let c = &mut abc.constants;
    'sets: for (i, ts) in c.ns_sets.iter().enumerate().skip(1) {
        if ts.len() != set.len() {
            continue;
        }
        for a in ts {
            if !set.contains(a) {
                continue 'sets;
            }
        }
        return i as i64;
    }
    if !add {
        return -1;
    }
    ensure(&mut c.ns_sets, Vec::new());
    c.ns_sets.push(set.to_vec());
    c.ns_sets.len() as i64 - 1
}

pub fn multiname_id(abc: &mut Abc, m: Multiname, add: bool) -> i64 {
    let v = &mut abc.constants.multinames;
    if !matches!(m, Multiname::TypeName { .. }) {
        if let Some(i) = last(v, |y| *y == m) {
            return i as i64;
        }
    }
    if !add {
        return -1;
    }
    ensure(v, Multiname::QName { attr: false, ns: 0, name: 0 });
    v.push(m);
    v.len() as i64 - 1
}

pub fn mn(abc: &mut Abc, m: Multiname) -> usize {
    multiname_id(abc, m, true) as usize
}

pub fn qname(abc: &mut Abc, ns: usize, name: &str) -> usize {
    let name = str_id(abc, name);
    mn(abc, Multiname::QName { attr: false, ns, name })
}

/// `getQnameId(name, kind, nsName, true)`.
pub fn qname_of(abc: &mut Abc, name: &str, kind: u8, ns_name: &str) -> usize {
    let n = str_id(abc, name);
    let ns = ns(abc, kind, ns_name);
    mn(abc, Multiname::QName { attr: false, ns, name: n })
}

pub fn int_id(abc: &mut Abc, v: i32) -> usize {
    let c = &mut abc.constants.ints;
    if let Some(i) = last(c, |x| *x == v) {
        return i;
    }
    ensure(c, 0);
    c.push(v);
    c.len() - 1
}

fn java_double_bits(v: f64) -> u64 {
    if v.is_nan() { f64::NAN.to_bits() } else { v.to_bits() }
}

pub fn double_id(abc: &mut Abc, v: f64) -> usize {
    let c = &mut abc.constants.doubles;
    if let Some(i) = last(c, |x| java_double_bits(*x) == java_double_bits(v)) {
        return i;
    }
    ensure(c, f64::NAN);
    c.push(v);
    c.len() - 1
}

pub fn float_id(abc: &mut Abc, v: f32) -> usize {
    let c = &mut abc.constants.floats;
    if let Some(i) = last(c, |x| x.to_bits() == v.to_bits() || (x.is_nan() && v.is_nan())) {
        return i;
    }
    ensure(c, f32::NAN);
    c.push(v);
    c.len() - 1
}

pub fn decimal_id(abc: &mut Abc, v: [u8; 16]) -> usize {
    let c = &mut abc.constants.decimals;
    if let Some(i) = last(c, |x| *x == v) {
        return i;
    }
    ensure(c, [0; 16]);
    c.push(v);
    c.len() - 1
}

/// `getNamespaceSubIndex`: how many earlier namespaces share this one's kind and name.
pub fn ns_sub_index(abc: &Abc, ns: usize) -> usize {
    let c = &abc.constants.namespaces;
    let Some(&n) = c.get(ns) else { return 0 };
    c[1..ns.min(c.len())].iter().filter(|x| x.name == n.name && x.kind == n.kind).count()
}
