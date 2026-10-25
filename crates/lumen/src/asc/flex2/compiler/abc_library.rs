//! Not a Java port: turns raw ABC blocks (e.g. a SWF's `DoABC` bodies) into a library whose scripts resolve like a
//! SWC's, with the definitions and catalog-style dependencies compc would have written for them.

use std::rc::Rc;

use super::swc::{Swc, SwcDependencySet, INHERITANCE, SIGNATURE};
use crate::abc::{ns_kind, Abc, Multiname, TraitData};

fn qname_of(abc: &Abc, ns: usize, name: usize) -> Option<String> {
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
    qname_of(abc, ns, name)
}

/// The definitions a type reference may name: a multiname's name in each package of its set, since the catalog's
/// resolved name is not known here; names that resolve nowhere stay unresolved and are ignored.
fn dependency_names(abc: &Abc, index: usize, out: &mut Vec<String>) {
    match abc.constants.multinames.get(index) {
        Some(Multiname::QName { ns, name, .. }) => out.extend(qname_of(abc, *ns, *name)),
        Some(Multiname::Multiname { name, ns_set, .. }) => {
            for &n in abc.constants.ns_sets.get(*ns_set).into_iter().flatten() {
                if abc.constants.namespaces.get(n).is_some_and(|n| matches!(n.kind, ns_kind::PACKAGE | ns_kind::PACKAGE_INTERNAL)) {
                    out.extend(qname_of(abc, n, *name));
                }
            }
        }
        Some(Multiname::TypeName { name, params }) => {
            dependency_names(abc, *name, out);
            for &p in params {
                dependency_names(abc, p, out);
            }
        }
        _ => {}
    }
}

/// The top-level definitions of a block and the inheritance and signature dependencies outside it.
pub fn describe(abc: &Abc) -> (Vec<String>, SwcDependencySet) {
    let mut defs: Vec<String> = Vec::new();
    let mut known = std::collections::HashSet::new();
    for s in &abc.scripts {
        for t in &s.traits {
            if let Some(q) = qname_string(abc, t.name) {
                if known.insert(q.clone()) {
                    defs.push(q);
                }
            }
        }
    }
    let mut deps = SwcDependencySet::default();
    let add = |kind: &str, index: usize, deps: &mut SwcDependencySet| {
        let mut names = Vec::new();
        dependency_names(abc, index, &mut names);
        for q in names {
            if !known.contains(&q) {
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

/// Adds the names a block's code refers to as signature dependencies, as the references a source of the same build
/// makes from its code: `AbcCompiler` then inherits the slots of a source they name, as `As3Compiler` does.
fn add_code_dependencies(abc: &Abc, defs: &[String], deps: &mut SwcDependencySet) {
    let known: std::collections::HashSet<&str> = defs.iter().map(String::as_str).collect();
    let dialect = crate::abc::opcodes::Dialect::of(abc.major, abc.minor);
    let mut names = Vec::new();
    for b in &abc.bodies {
        let Ok(code) = crate::abc::opcodes::decode(&b.code, dialect) else { continue };
        for i in &code {
            for (k, &v) in i.kinds.iter().zip(&i.operands) {
                if *k == crate::abc::opcodes::Operand::MultinameIndex {
                    dependency_names(abc, v as usize, &mut names);
                }
            }
        }
    }
    for q in names {
        if !known.contains(q.as_str()) {
            deps.add_dependency(SIGNATURE, &q);
        }
    }
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

struct PlayerGlobalScript {
    name: String,
    defs: Vec<String>,
    deps: Vec<(&'static str, String)>,
    abc: Vec<u8>,
}

fn playerglobal_scripts() -> &'static [PlayerGlobalScript] {
    static SCRIPTS: std::sync::OnceLock<Vec<PlayerGlobalScript>> = std::sync::OnceLock::new();
    SCRIPTS.get_or_init(|| {
        let mut out = Vec::new();
        let Ok(swf) = crate::swf::Swf::parse(include_bytes!("../../../../resources/playerglobal.swf")) else { return out };
        for t in &swf.timeline.tags {
            let Some((bytes, Some(name))) = crate::abc::tag_abc(t) else { continue };
            let Ok(abc) = Abc::parse(bytes) else { continue };
            let (defs, set) = describe(&abc);
            let deps = [INHERITANCE, SIGNATURE].into_iter().flat_map(|k| set.get_dependencies(k).into_iter().map(move |d| (k, d))).collect();
            out.push(PlayerGlobalScript { name, defs, deps, abc: bytes.to_vec() });
        }
        out
    })
}

/// The crate's own `playerglobal` library SWF as an external `playerglobal.swc`, one script per `DoABC2` tag.
pub fn playerglobal() -> Rc<Swc> {
    let blocks = playerglobal_scripts()
        .iter()
        .map(|s| {
            let mut deps = SwcDependencySet::default();
            for (k, d) in &s.deps {
                deps.add_dependency(k, d);
            }
            (s.name.clone(), s.defs.clone(), deps, s.abc.clone())
        })
        .collect();
    Swc::from_abc_blocks("playerglobal.swc", blocks)
}

/// The `ns:name` definitions of `playerglobal`.
pub fn playerglobal_definitions() -> impl Iterator<Item = &'static str> {
    playerglobal_scripts().iter().flat_map(|s| s.defs.iter().map(String::as_str))
}

/// A library of ABC blocks, one script per block, named `<location>(<index>)`. With `same_build`, the names their
/// code refers to are dependencies too.
pub fn library(location: &str, blocks: &[Vec<u8>], exclude: &[String], same_build: bool) -> Result<Rc<Swc>, String> {
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
        let (defs, mut deps) = describe(&abc);
        if same_build {
            add_code_dependencies(&abc, &defs, &mut deps);
        }
        scripts.push((format!("{location}({i})"), defs, deps, bytes));
    }
    Ok(Swc::from_abc_blocks(location, scripts))
}
