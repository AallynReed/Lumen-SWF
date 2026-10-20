//! Script packs: the public definitions of each script, listed by package as FFDec does.

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
        if !any {
            out.push(Entry { package: String::new(), name: format!("script_{si}"), script: si, trait_index: None, kind: "script" });
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
    // Like FFDec, a pack also shows the script initializer and the script's private definitions.
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
