//! Not a Java port: turns raw ABC blocks (e.g. a SWF's `DoABC` bodies) into a library whose scripts resolve like a
//! SWC's, with the definitions and catalog-style dependencies compc would have written for them.

use std::rc::Rc;

use super::swc::{Swc, SwcDependencySet, INHERITANCE, SIGNATURE};
use crate::abc::{ns_kind, Abc, Multiname, TraitData};

/// `ns:name` (or `name` in the unnamed package) of a multiname, choosing a multiname's first package namespace.
fn qname_string(abc: &Abc, index: usize) -> Option<String> {
    let (ns, name) = match abc.constants.multinames.get(index)? {
        Multiname::QName { ns, name, .. } => (*ns, *name),
        Multiname::Multiname { name, ns_set, .. } => {
            let set = abc.constants.ns_sets.get(*ns_set)?;
            let ns = set.iter().copied().find(|&n| abc.constants.namespaces.get(n).is_some_and(|n| n.kind == ns_kind::PACKAGE)).or_else(|| set.first().copied())?;
            (ns, *name)
        }
        Multiname::TypeName { name, .. } => return qname_string(abc, *name),
        _ => return None,
    };
    let local = abc.str(name);
    if local.is_empty() || local == "*" {
        return None;
    }
    let ns = abc.constants.namespaces.get(ns)?;
    let pkg = abc.str(ns.name);
    if pkg.is_empty() || !matches!(ns.kind, ns_kind::PACKAGE | ns_kind::PACKAGE_INTERNAL) {
        Some(local.to_string())
    } else {
        Some(format!("{pkg}:{local}"))
    }
}

/// The top-level definitions of a block and the inheritance and signature dependencies outside it.
pub fn describe(abc: &Abc) -> (Vec<String>, SwcDependencySet) {
    let mut defs: Vec<String> = Vec::new();
    for s in &abc.scripts {
        for t in &s.traits {
            if let Some(q) = qname_string(abc, t.name) {
                if !defs.contains(&q) {
                    defs.push(q);
                }
            }
        }
    }
    let mut deps = SwcDependencySet::default();
    let add = |kind: &str, index: usize, deps: &mut SwcDependencySet| {
        if let Some(q) = qname_string(abc, index) {
            if !defs.contains(&q) {
                deps.add_dependency(kind, &q);
            }
        }
    };
    let mut methods: Vec<usize> = Vec::new();
    let traits = |ts: &[crate::abc::Trait], deps: &mut SwcDependencySet, methods: &mut Vec<usize>| {
        for t in ts {
            match &t.data {
                TraitData::Slot { type_name, .. } | TraitData::Const { type_name, .. } => add(SIGNATURE, *type_name, deps),
                _ => {}
            }
            if let Some(m) = t.method() {
                methods.push(m);
            }
        }
    };
    for (i, inst) in abc.instances.iter().enumerate() {
        add(INHERITANCE, inst.super_name, &mut deps);
        for &iface in &inst.interfaces {
            add(INHERITANCE, iface, &mut deps);
        }
        methods.push(inst.iinit);
        traits(&inst.traits, &mut deps, &mut methods);
        if let Some(c) = abc.classes.get(i) {
            methods.push(c.cinit);
            traits(&c.traits, &mut deps, &mut methods);
        }
    }
    for s in &abc.scripts {
        traits(&s.traits, &mut deps, &mut methods);
    }
    for m in methods {
        if let Some(mi) = abc.methods.get(m) {
            add(SIGNATURE, mi.return_type, &mut deps);
            for &p in &mi.param_types {
                add(SIGNATURE, p, &mut deps);
            }
        }
    }
    (defs, deps)
}

/// The block with the script traits defining `exclude` (as `ns:name`) removed, so a new source can define them.
pub fn strip_definitions(abc: &Abc, exclude: &[String]) -> Abc {
    let mut abc = abc.clone();
    let names: Vec<Vec<bool>> = abc.scripts.iter().map(|s| s.traits.iter().map(|t| qname_string(&abc, t.name).is_some_and(|q| exclude.contains(&q))).collect()).collect();
    for (s, drop) in abc.scripts.iter_mut().zip(names) {
        let mut it = drop.into_iter();
        s.traits.retain(|_| !it.next().unwrap_or(false));
    }
    abc
}

/// A library of ABC blocks, one script per block, named `<location>(<index>)`.
pub fn library(location: &str, blocks: &[Vec<u8>], exclude: &[String]) -> Result<Rc<Swc>, String> {
    let mut scripts = Vec::new();
    for (i, bytes) in blocks.iter().enumerate() {
        let abc = Abc::parse(bytes).map_err(|e| format!("ABC block {i}: {e}"))?;
        let (abc, bytes) = if exclude.is_empty() {
            (abc, bytes.clone())
        } else {
            let stripped = strip_definitions(&abc, exclude);
            let b = stripped.to_bytes();
            (stripped, b)
        };
        let (defs, deps) = describe(&abc);
        scripts.push((format!("{location}({i})"), defs, deps, bytes));
    }
    Ok(Swc::from_abc_blocks(location, scripts))
}
