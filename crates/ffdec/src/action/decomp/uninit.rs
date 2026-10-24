//! Class fields used through `this` or the class but never declared, added to AS2 classes.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use crate::abc::decomp::item::*;

use super::items::*;

/// Class name to (field, is static), instance fields first, each group in natural order.
pub type Uninitialized = HashMap<String, Vec<(String, bool)>>;

const CLASSES: &str = include_str!("as2_classes.txt");

/// `ActionScript2Classes`: the player's class traits and inheritance.
fn builtin() -> (HashMap<String, Vec<String>>, HashMap<String, Vec<String>>) {
    let mut traits: HashMap<String, Vec<String>> = HashMap::new();
    let mut inheritance: HashMap<String, Vec<String>> = HashMap::new();
    let mut cls = String::new();
    for line in CLASSES.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(' ').collect();
        if parts[0] == "class" {
            cls = parts[1].to_string();
            traits.insert(cls.clone(), Vec::new());
            continue;
        }
        let pos = usize::from(parts[0] == "static");
        if parts[pos] == "extends" {
            inheritance.entry(cls.clone()).or_default().push(parts[pos + 1].to_string());
            continue;
        }
        if let Some(n) = parts.get(pos + 1) {
            traits.get_mut(&cls).unwrap().push(n.to_string());
        }
    }
    (traits, inheritance)
}

fn is_digit(c: u16) -> bool {
    char::from_u32(c as u32).is_some_and(|c| c.is_numeric()) || c == '.' as u16 || c == ',' as u16
}

fn is_space(c: u16) -> bool {
    char::from_u32(c as u32).is_some_and(|c| c.is_whitespace() && c != '\t' && c != '\n' && c != '\r') || c == 0xa0
}

/// `natorder.NaturalOrderComparator`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let a: Vec<u16> = a.encode_utf16().collect();
    let b: Vec<u16> = b.encode_utf16().collect();
    let at = |s: &[u16], i: usize| s.get(i).copied().unwrap_or(0);
    let compare_right = |a: &[u16], b: &[u16]| -> i32 {
        let mut bias = 0;
        let mut i = 0;
        loop {
            let (ca, cb) = (at(a, i), at(b, i));
            if !is_digit(ca) && !is_digit(cb) {
                return bias;
            }
            if !is_digit(ca) {
                return -1;
            }
            if !is_digit(cb) {
                return 1;
            }
            if ca == 0 && cb == 0 {
                return bias;
            }
            if bias == 0 {
                bias = (ca as i32 - cb as i32).signum();
            }
            i += 1;
        }
    };
    let (mut ia, mut ib) = (0, 0);
    loop {
        let (mut nza, mut nzb) = (0, 0);
        let mut ca = at(&a, ia);
        let mut cb = at(&b, ib);
        while is_space(ca) || ca == '0' as u16 {
            nza = if ca == '0' as u16 { nza + 1 } else { 0 };
            ia += 1;
            ca = at(&a, ia);
        }
        while is_space(cb) || cb == '0' as u16 {
            nzb = if cb == '0' as u16 { nzb + 1 } else { 0 };
            ib += 1;
            cb = at(&b, ib);
        }
        let digit = |c: u16| char::from_u32(c as u32).is_some_and(|c| c.is_numeric());
        if digit(ca) && digit(cb) {
            let bias = compare_right(&a[ia.min(a.len())..], &b[ib.min(b.len())..]);
            if bias != 0 {
                return bias.cmp(&0);
            }
        }
        if ca == 0 && cb == 0 {
            if nza != nzb {
                return (nza as i32 - nzb as i32).cmp(&0);
            }
            if a.len() == b.len() {
                return a.cmp(&b);
            }
            return a.len().cmp(&b.len());
        }
        if ca != cb {
            return ca.cmp(&cb);
        }
        ia += 1;
        ib += 1;
    }
}

fn members_path(item: &It) -> Option<Vec<String>> {
    let mut ret = Vec::new();
    let mut cur = item.clone();
    loop {
        let next = match &cur.borrow().k {
            K::AGetMember { obj, member, .. } => {
                ret.insert(0, direct_string(member)?);
                obj.clone()
            }
            _ => break,
        };
        cur = next;
    }
    let k = cur.borrow().k.clone();
    match &k {
        K::ADirect { v: AVal::Register { name: Some(n), .. }, .. } if n == "this" => {
            ret.insert(0, "this".into());
            Some(ret)
        }
        K::AGetVar { name, .. } => {
            ret.insert(0, direct_string(name)?);
            Some(ret)
        }
        _ => None,
    }
}

fn full_path(item: &It) -> Option<Vec<String>> {
    let k = item.borrow().k.clone();
    let (name, obj) = match &k {
        K::AGetMember { .. } => return members_path(item),
        K::ASetVar { name, .. } => return Some(vec![direct_string(name)?]),
        K::ASetMember { obj, member, .. } => (member.clone(), Some(obj.clone())),
        K::ACallMethod { obj, name, .. } | K::ANewMethod { obj, name, .. } => (name.clone(), Some(obj.clone())),
        K::ADelete { obj, prop } => (prop.clone(), obj.clone()),
        _ => return None,
    };
    let n = direct_string(&name)?;
    let mut p = members_path(&obj?)?;
    p.push(n);
    Some(p)
}

struct Info {
    traits: HashMap<String, Vec<String>>,
    inheritance: HashMap<String, Vec<String>>,
}

impl Info {
    fn contains_trait(&self, class: &str, name: &str) -> bool {
        let Some(t) = self.traits.get(class) else { return false };
        if t.iter().any(|x| x == name) {
            return true;
        }
        self.inheritance.get(class).into_iter().flatten().any(|p| self.traits.get(p).is_some_and(|t| t.iter().any(|x| x == name)))
    }
}

fn visit_all(item: &It, f: &mut dyn FnMut(&It)) {
    f(item);
    visit_recursively(item, &mut |i| {
        f(i);
        true
    });
}

/// `calculateAs2UninitializedClassTraits` over decompiled trees: `classes` are the
/// `__Packages` initializers, `others` every other script of the SWF.
pub fn detect(classes: &[List], others: &[List]) -> Uninitialized {
    let (traits, inheritance) = builtin();
    let mut info = Info { traits, inheritance };
    for tree in classes {
        for item in tree.borrow().iter() {
            let k = item.borrow().k.clone();
            match &k {
                K::AInterface { name, supers } => {
                    let cn = members_path(name).unwrap_or_default().join(".");
                    let list: Vec<String> = supers.borrow().iter().map(|s| members_path(s).unwrap_or_default().join(".")).collect();
                    info.inheritance.insert(cn, list);
                }
                K::AClass(c) => {
                    let cn = members_path(&c.name).unwrap_or_default().join(".");
                    let t = info.traits.entry(cn.clone()).or_default();
                    for (key, v) in &c.traits {
                        let Some(name) = direct_string(key) else { continue };
                        let is_fn = v.as_ref().is_some_and(|v| matches!(v.borrow().k, K::AFunction(_)));
                        if is_fn && (name.starts_with("__get__") || name.starts_with("__set__")) {
                            t.push(name[7..].to_string());
                        }
                        t.push(name);
                    }
                    let mut inh = vec![match &c.extends {
                        Some(e) => members_path(e).unwrap_or_default().join("."),
                        None => "Object".into(),
                    }];
                    for i in c.implements.borrow().iter() {
                        inh.push(members_path(i).unwrap_or_default().join("."));
                    }
                    info.inheritance.insert(cn, inh);
                }
                _ => {}
            }
        }
    }
    let mut instance: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut statics: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for tree in classes {
        for item in tree.borrow().iter() {
            let K::AClass(c) = item.borrow().k.clone() else { continue };
            let cn = members_path(&c.name).unwrap_or_default().join(".");
            for (key, v) in &c.traits {
                if !is_direct(key) {
                    continue;
                }
                let Some(v) = v else { continue };
                visit_all(v, &mut |i| {
                    let Some(path) = full_path(i) else { return };
                    if path.len() == 2 && path[0] == "this" {
                        let name = &path[1];
                        let known = instance.get(&cn).is_some_and(|l| l.contains(name));
                        if !info.contains_trait(&cn, name) && !known {
                            instance.entry(cn.clone()).or_default().push(name.clone());
                        }
                    }
                });
            }
        }
    }
    let classes_known: Vec<String> = info.inheritance.keys().cloned().collect();
    for cn in &classes_known {
        let mut i = 0;
        while i < info.inheritance[cn].len() {
            let parent = info.inheritance[cn][i].clone();
            if let Some(pp) = info.inheritance.get(&parent).cloned() {
                for p in pp {
                    if !info.inheritance[cn].contains(&p) {
                        info.inheritance.get_mut(cn).unwrap().push(p);
                    }
                }
            }
            i += 1;
        }
    }
    for tree in classes.iter().chain(others.iter()) {
        for item in tree.borrow().iter() {
            visit_all(item, &mut |i| {
                if !matches!(i.borrow().k, K::ASetMember { .. } | K::ACallMethod { .. } | K::ANewMethod { .. } | K::ADelete { .. } | K::AGetMember { .. }) {
                    return;
                }
                let Some(path) = full_path(i) else { return };
                let name = path.last().unwrap().clone();
                let cn = path[..path.len() - 1].join(".");
                if !info.inheritance.contains_key(&cn) {
                    return;
                }
                let in_inst = instance.get(&cn).is_some_and(|l| l.contains(&name));
                let in_stat = statics.get(&cn).is_some_and(|l| l.contains(&name));
                if !info.contains_trait(&cn, &name) && !in_inst && !in_stat {
                    statics.entry(cn).or_default().push(name);
                }
            });
        }
    }
    let mut result: Uninitialized = HashMap::new();
    for (cn, mut names) in instance {
        names.sort_by(|a, b| natural_cmp(a, b));
        result.entry(cn).or_default().extend(names.into_iter().map(|n| (n, false)));
    }
    for (cn, mut names) in statics {
        names.sort_by(|a, b| natural_cmp(a, b));
        let list = result.entry(cn).or_default();
        for n in names {
            match list.iter_mut().find(|(x, _)| *x == n) {
                Some(e) => e.1 = true,
                None => list.push((n, true)),
            }
        }
    }
    result
}
