//! Script packs: the public definitions of each script, listed by package.

use super::{ns_kind, Abc, TraitData};

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub package: String,
    pub name: String,
    pub script: usize,
    /// Trait index in the script, or None for a script without public definitions.
    pub trait_index: Option<usize>,
    /// `class`, `interface`, `function`, `var`, `const` or `script`.
    pub kind: &'static str,
}

impl Entry {
    pub fn full_name(&self) -> String {
        if self.package.is_empty() { self.name.clone() } else { format!("{}.{}", self.package, self.name) }
    }

    /// Identifier used in tree nodes: `script:trait`.
    pub fn id(&self) -> String {
        match self.trait_index {
            Some(t) => format!("{}:{t}", self.script),
            None => format!("{}", self.script),
        }
    }
}

pub fn parse_id(s: &str) -> Option<(usize, Option<usize>)> {
    match s.split_once(':') {
        Some((a, b)) => Some((a.parse().ok()?, Some(b.parse().ok()?))),
        None => Some((s.parse().ok()?, None)),
    }
}

pub fn entries(abc: &Abc) -> Vec<Entry> {
    let mut out = Vec::new();
    for (si, script) in abc.scripts.iter().enumerate() {
        let mut any = false;
        for (ti, t) in script.traits.iter().enumerate() {
            let kind = abc.ns_kind_of(t.name);
            if kind != ns_kind::PACKAGE && kind != ns_kind::PACKAGE_INTERNAL {
                continue;
            }
            any = true;
            let k = match &t.data {
                TraitData::Class { class, .. } => {
                    if abc.instances.get(*class).is_some_and(|i| i.flags & super::instance_flags::INTERFACE != 0) {
                        "interface"
                    } else {
                        "class"
                    }
                }
                TraitData::Method { .. } | TraitData::Getter { .. } | TraitData::Setter { .. } | TraitData::Function { .. } => "function",
                TraitData::Const { .. } => "const",
                TraitData::Slot { .. } => "var",
            };
            out.push(Entry { package: abc.package_of(t.name), name: abc.local_name(t.name), script: si, trait_index: Some(ti), kind: k });
        }
        // Scripts with several public definitions also have a pack of their own, which
        // includes the others.
        if script.traits.iter().filter(|t| is_public(abc, t)).count() != 1 {
            out.push(Entry { package: String::new(), name: format!("script_{si}"), script: si, trait_index: None, kind: "script" });
        }
    }
    out
}

fn is_public(abc: &Abc, t: &super::Trait) -> bool {
    matches!(abc.ns_kind_of(t.name), ns_kind::PACKAGE | ns_kind::PACKAGE_INTERNAL)
}

/// The public trait a file-private trait was written next to. mxmlc names the private
/// namespace after the source file (`Name.as$N`), which tells the file apart when one script
/// holds several public definitions.
fn private_home(abc: &Abc, script: usize, t: usize) -> Option<usize> {
    let traits = &abc.scripts[script].traits;
    if abc.ns_kind_of(traits[t].name) != ns_kind::PRIVATE {
        return None;
    }
    let ns = abc.package_of(traits[t].name);
    let stem = ns.rsplit_once(".as$")?.0;
    let stem = stem.rsplit(['/', ':', '\\']).next()?;
    traits.iter().position(|p| is_public(abc, p) && abc.local_name(p.name) == stem)
}

/// The traits of the pack of public trait `t`: `ScriptInfo.getPacks` takes the private traits
/// after it (and, for the first, those before it); a private trait whose namespace names
/// another public definition's file goes with that one instead.
pub fn pack_traits(abc: &Abc, script: usize, t: usize) -> Vec<usize> {
    let traits = &abc.scripts[script].traits;
    let first_public = traits.iter().position(|p| is_public(abc, p));
    let mut out = vec![t];
    let mut take = |k: usize, out: &mut Vec<usize>| {
        if private_home(abc, script, k).is_none_or(|h| h == t) {
            out.push(k);
        }
    };
    if first_public == Some(t) {
        for k in 0..t {
            take(k, &mut out);
        }
    }
    let mut k = t + 1;
    while k < traits.len() && !is_public(abc, &traits[k]) {
        take(k, &mut out);
        k += 1;
    }
    for k in 0..traits.len() {
        if !out.contains(&k) && private_home(abc, script, k) == Some(t) {
            out.push(k);
        }
    }
    out
}

/// The class index a script entry defines, when it is a class.
pub fn class_of(abc: &Abc, e: &Entry) -> Option<usize> {
    let t = abc.scripts.get(e.script)?.traits.get(e.trait_index?)?;
    match t.data {
        TraitData::Class { class, .. } => Some(class),
        _ => None,
    }
}

/// P-code of a script entry: the whole class for classes, the trait otherwise.
pub fn entry_pcode(abc: &Abc, e: &Entry) -> String {
    let Some(script) = abc.scripts.get(e.script) else { return String::new() };
    let body = if let Some(c) = class_of(abc, e) {
        super::pcode::class_pcode(abc, c)
    } else {
        match e.trait_index.and_then(|t| script.traits.get(t)) {
            Some(t) => super::pcode::trait_pcode(abc, t),
            None => return super::pcode::script_pcode(abc, e.script),
        }
    };
    // A pack also shows the script initializer and the script's private definitions.
    let mut out = format!("; script initializer\n{}\n{body}", super::pcode::method_pcode(abc, script.init));
    for t in &script.traits {
        let kind = abc.ns_kind_of(t.name);
        if kind == ns_kind::PACKAGE || kind == ns_kind::PACKAGE_INTERNAL {
            continue;
        }
        out.push('\n');
        match t.data {
            TraitData::Class { class, .. } => out.push_str(&super::pcode::class_pcode(abc, class)),
            _ => out.push_str(&super::pcode::trait_pcode(abc, t)),
        }
    }
    out
}

/// A method that can be edited on its own in the p-code trait list.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodRef {
    pub label: String,
    pub method: usize,
    pub owner: Option<super::asm::TraitLoc>,
}

impl MethodRef {
    /// Its p-code: the trait with its method, or the bare method for initializers.
    pub fn pcode(&self, abc: &Abc) -> String {
        match self.owner.and_then(|l| abc.trait_at(l)) {
            Some(t) => super::pcode::trait_pcode(abc, t),
            None => super::pcode::method_pcode(abc, self.method),
        }
    }
}

/// The methods shown in an entry's p-code, in the same order.
pub fn methods(abc: &Abc, e: &Entry) -> Vec<MethodRef> {
    use super::asm::TraitLoc;
    let mut out: Vec<MethodRef> = Vec::new();
    let mut add = |label: String, method: usize, owner: Option<TraitLoc>| {
        let n = out.iter().filter(|m| m.label == label || m.label.starts_with(&format!("{label} ("))).count();
        let label = if n == 0 { label } else { format!("{label} ({})", n + 1) };
        out.push(MethodRef { label, method, owner });
    };
    let trait_label = |t: &super::Trait, prefix: &str| {
        let kind = match t.data {
            TraitData::Getter { .. } => "get ",
            TraitData::Setter { .. } => "set ",
            _ => "",
        };
        format!("{prefix}{kind}{}", abc.local_name(t.name))
    };
    let Some(script) = abc.scripts.get(e.script) else { return out };
    add("script initializer".into(), script.init, None);
    let class = |c: usize, prefix: &str, add: &mut dyn FnMut(String, usize, Option<TraitLoc>)| {
        if let Some(ii) = abc.instances.get(c) {
            add(format!("{prefix}instance initializer"), ii.iinit, None);
        }
        if let Some(cl) = abc.classes.get(c) {
            add(format!("{prefix}class initializer"), cl.cinit, None);
            for (i, t) in cl.traits.iter().enumerate() {
                if let Some(m) = t.method() {
                    add(trait_label(t, &format!("{prefix}static ")), m, Some(TraitLoc::Class(c, i)));
                }
            }
        }
        if let Some(ii) = abc.instances.get(c) {
            for (i, t) in ii.traits.iter().enumerate() {
                if let Some(m) = t.method() {
                    add(trait_label(t, prefix), m, Some(TraitLoc::Instance(c, i)));
                }
            }
        }
    };
    let public = |t: &super::Trait| matches!(abc.ns_kind_of(t.name), ns_kind::PACKAGE | ns_kind::PACKAGE_INTERNAL);
    match class_of(abc, e) {
        Some(c) => class(c, "", &mut add),
        None => {
            if let Some((ti, t)) = e.trait_index.and_then(|ti| script.traits.get(ti).map(|t| (ti, t))) {
                if let Some(m) = t.method() {
                    add(trait_label(t, ""), m, Some(TraitLoc::Script(e.script, ti)));
                }
            }
        }
    }
    for (ti, t) in script.traits.iter().enumerate() {
        if public(t) {
            continue;
        }
        match t.data {
            TraitData::Class { class: c, .. } => class(c, &format!("{}.", abc.local_name(t.name)), &mut add),
            _ => {
                if let Some(m) = t.method() {
                    add(trait_label(t, ""), m, Some(TraitLoc::Script(e.script, ti)));
                }
            }
        }
    }
    out
}
