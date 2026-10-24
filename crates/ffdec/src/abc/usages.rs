//! Definitions whose names collide and are printed with a `#namespace` suffix.

use std::collections::{HashMap, HashSet};

use super::{ns_kind, Abc, Multiname, Trait, TraitData};

const TRAITS_CLASS: u8 = 1;
const TRAITS_INSTANCE: u8 = 2;
const TRAITS_SCRIPT: u8 = 3;

#[derive(Clone, Copy)]
enum Def {
    Class { class: usize },
    /// A slot, const, method, getter or setter; `kind` tells methods apart from accessors.
    Trait { method: bool, kind: u8, class: i64, traits_type: u8, parent: i64 },
}

#[derive(Clone, Copy)]
struct Usage {
    mn: usize,
    script: i64,
    def: Def,
}

struct Walk<'a> {
    abc: &'a Abc,
    out: Vec<Usage>,
}

impl Walk<'_> {
    fn traits(&mut self, traits: &[Trait], traits_type: u8, script: i64, class: i64, parent: i64) {
        let abc = self.abc;
        for (t, tr) in traits.iter().enumerate() {
            match tr.data {
                TraitData::Class { class: c, .. } => {
                    self.out.push(Usage { mn: tr.name, script, def: Def::Class { class: c } });
                    self.out.push(Usage { mn: abc.instances[c].name, script, def: Def::Class { class: c } });
                    self.method(abc.instances[c].iinit, -1, c as i64, 0, TRAITS_INSTANCE);
                    self.method(abc.classes[c].cinit, -1, c as i64, 0, TRAITS_CLASS);
                    self.traits(&abc.instances[c].traits, TRAITS_INSTANCE, -1, c as i64, -1);
                    self.traits(&abc.classes[c].traits, TRAITS_CLASS, -1, c as i64, -1);
                }
                TraitData::Slot { .. } | TraitData::Const { .. } => {
                    self.out.push(Usage { mn: tr.name, script, def: Def::Trait { method: false, kind: 0, class, traits_type, parent } });
                }
                TraitData::Method { method, .. } | TraitData::Getter { method, .. } | TraitData::Setter { method, .. } => {
                    let kind = match tr.data {
                        TraitData::Method { .. } => 1,
                        TraitData::Getter { .. } => 2,
                        _ => 3,
                    };
                    self.out.push(Usage { mn: tr.name, script, def: Def::Trait { method: true, kind, class, traits_type, parent } });
                    self.method(method, script, class, t as i64, traits_type);
                }
                TraitData::Function { .. } => {}
            }
        }
    }

    fn method(&mut self, method: usize, script: i64, class: i64, trait_index: i64, traits_type: u8) {
        if let Some(b) = self.abc.body_of.get(method).copied().flatten() {
            self.traits(&self.abc.bodies[b].traits, traits_type, script, class, trait_index);
        }
    }
}

/// `sameMultinameName`.
fn same_name(abc: &Abc, a: &Usage, b: &Usage, include_public: bool) -> bool {
    let c = &abc.constants;
    let (Some(Multiname::QName { attr: aa, ns: an, name: anm }), Some(Multiname::QName { attr: ba, ns: bn, name: bnm })) = (c.multinames.get(a.mn), c.multinames.get(b.mn)) else {
        return false;
    };
    if aa != ba || c.strings.get(*anm) != c.strings.get(*bnm) {
        return false;
    }
    let ns = |i: usize| c.namespaces.get(i).filter(|_| i != 0).map_or((0, None), |n| (n.kind, (n.name != 0).then(|| c.strings.get(n.name)).flatten()));
    let (ak, aname) = ns(*an);
    let (bk, bname) = ns(*bn);
    if ak == ns_kind::NAMESPACE && bk == ns_kind::NAMESPACE {
        return aname == bname;
    }
    if ak == ns_kind::NAMESPACE || bk == ns_kind::NAMESPACE {
        return false;
    }
    let package = |k: u8| k == ns_kind::PACKAGE || k == ns_kind::PACKAGE_INTERNAL;
    if include_public && package(ak) && package(bk) {
        return aname == bname;
    }
    a.script == b.script
}

/// `prev.collides(other)`.
fn collides(abc: &Abc, prev: &Usage, other: &Usage) -> bool {
    match (prev.def, other.def) {
        (Def::Class { class }, o) => {
            let inside = match o {
                Def::Class { class: oc } => oc as i64,
                Def::Trait { class: oc, .. } => oc,
            };
            inside != class as i64 && matches!(o, Def::Class { .. }) && same_name(abc, prev, other, true)
        }
        (Def::Trait { method, kind, class, traits_type, parent }, Def::Trait { method: om, kind: ok, class: oc, traits_type: ot, parent: op }) => {
            oc == class && ot == traits_type && op == parent && same_name(abc, other, prev, false) && !(method && om && kind != ok)
        }
        _ => false,
    }
}

/// The multinames to print with their namespace suffix.
pub fn colliding_multinames(abc: &Abc) -> HashSet<usize> {
    let mut w = Walk { abc, out: Vec::new() };
    for (s, script) in abc.scripts.iter().enumerate() {
        w.method(script.init, s as i64, -1, 0, TRAITS_SCRIPT);
        w.traits(&script.traits, TRAITS_SCRIPT, s as i64, -1, -1);
    }
    let mut by_mn: HashMap<usize, Vec<Usage>> = HashMap::new();
    for u in w.out {
        by_mn.entry(u.mn).or_default().push(u);
    }
    let mut groups: HashMap<(bool, &str), Vec<usize>> = HashMap::new();
    for (i, m) in abc.constants.multinames.iter().enumerate().skip(1) {
        if let Multiname::QName { attr, name, .. } = m {
            groups.entry((*attr, abc.constants.strings.get(*name).map_or("", |s| s.as_str()))).or_default().push(i);
        }
    }
    let mut out = HashSet::new();
    let none = Vec::new();
    for indices in groups.values().filter(|v| v.len() > 1) {
        let mut seen: Vec<&Usage> = Vec::new();
        for &i in indices {
            let usages = by_mn.get(&i).unwrap_or(&none);
            for u in usages {
                for p in &seen {
                    if collides(abc, p, u) {
                        out.insert(u.mn);
                        out.insert(p.mn);
                    }
                }
            }
            seen.extend(usages);
        }
    }
    out
}
