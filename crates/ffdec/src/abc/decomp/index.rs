//! Type information about classes and properties: FFDec's `AbcIndexing`.
//!
//! The index keeps its own copies of names and types, so it outlives the ABCs it was built from.
//! The player's own classes come from an embedded `playerglobal` library.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use super::ctx::{Ctx, LocalData};
use super::item::*;
use super::names::{Dotted, Idents, Names};
use crate::abc::{ns_kind, tag_abc, Abc, Multiname, Trait, TraitData};

/// A type as stored in the index (`TypeItem`, `ApplyTypeAVM2Item`, `UnboundedTypeItem`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TySpec {
    Unbounded,
    Null,
    Type(Dotted, Option<String>),
    Apply(Box<TySpec>, Vec<TySpec>),
}

impl TySpec {
    pub fn of(it: &It) -> Option<TySpec> {
        match &it.borrow().k {
            K::UnboundedType => Some(TySpec::Unbounded),
            K::Null => Some(TySpec::Null),
            K::Type { full, ns } => Some(TySpec::Type(full.clone(), ns.clone())),
            K::ApplyType { obj, params } => {
                let o = TySpec::of(obj)?;
                let ps: Option<Vec<TySpec>> = params.borrow().iter().map(TySpec::of).collect();
                Some(TySpec::Apply(Box::new(o), ps?))
            }
            _ => None,
        }
    }

    pub fn item(&self) -> It {
        match self {
            TySpec::Unbounded => unbounded(),
            TySpec::Null => null(None, None),
            TySpec::Type(full, ns) => bare(K::Type { full: full.clone(), ns: ns.clone() }, prec::NOPRECEDENCE),
            TySpec::Apply(o, ps) => bare(K::ApplyType { obj: o.item(), params: new_list(ps.iter().map(TySpec::item).collect()) }, prec::PRIMARY),
        }
    }

    fn no_ns(&self) -> TySpec {
        match self {
            TySpec::Type(f, _) => TySpec::Type(f.clone(), None),
            o => o.clone(),
        }
    }

    fn named(d: Dotted) -> TySpec {
        TySpec::Type(d, None)
    }
}

/// `multinameToType` over raw names; `None` for cyclic type names.
pub fn mn_spec(names: &Names, mn: usize) -> Option<TySpec> {
    mn_spec_depth(names, mn, 0)
}

fn mn_spec_depth(names: &Names, mn: usize, depth: u8) -> Option<TySpec> {
    if mn == 0 {
        return Some(TySpec::Unbounded);
    }
    if depth > 16 {
        return None;
    }
    match names.mn(mn)? {
        Multiname::TypeName { name, params } => {
            let obj = mn_spec_depth(names, *name, depth + 1)?;
            let mut ps = Vec::new();
            for &p in params {
                let r = mn_spec_depth(names, p, depth + 1)?;
                ps.push(if p == 0 { TySpec::Null } else { r });
            }
            Some(TySpec::Apply(Box::new(obj), ps))
        }
        Multiname::QName { ns, .. } if *ns != 0 && names.ns_kind(*ns) == ns_kind::PRIVATE => {
            Some(TySpec::Type(Dotted::parse_with_suffix(&names.name(mn, &[], true, true)), Some(format!("ns:{ns}"))))
        }
        _ => Some(TySpec::named(names.name_with_ns(mn, true))),
    }
}

/// `AbcIndexing.multinameToType`; `None` for cyclic type names.
pub fn multiname_to_type(ctx: &Ctx, mn: usize) -> Option<It> {
    mn_spec(&ctx.names(), mn).map(|s| s.item())
}

/// `multinameToType` with Java's null mapped to `*`.
pub fn mn_type(ctx: &Ctx, mn: usize) -> It {
    multiname_to_type(ctx, mn).unwrap_or_else(unbounded)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PropKey {
    name: String,
    parent: TySpec,
    ns: usize,
    abc: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct NsKey {
    name: String,
    ns: Dotted,
    ns_index: usize,
    abc: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ClassKey {
    ty: TySpec,
    pkg: Option<Dotted>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClassRef {
    level: usize,
    id: usize,
}

#[derive(Debug, Clone)]
struct ClassInfo {
    full_name: Dotted,
    full_name_no_suffix: Dotted,
    parent: Option<ClassRef>,
    instance_traits: Vec<(String, u8)>,
    static_traits: Vec<(String, u8)>,
}

#[derive(Debug, Clone)]
struct TraitInfo {
    ret: TySpec,
    call_ret: TySpec,
}

/// Result of `findPropertyTypeOrCallType`.
pub struct PropType {
    pub ty: It,
    pub call_ty: It,
    pub is_static: bool,
}

fn prop_key(name: &str, parent: TySpec, abc_id: usize, ns: usize, kind: u8) -> PropKey {
    let keep = abc_id != 0 && ns != 0 && !matches!(kind, ns_kind::PACKAGE | ns_kind::PACKAGE_INTERNAL | ns_kind::PROTECTED | ns_kind::STATIC_PROTECTED | ns_kind::NAMESPACE);
    PropKey { name: name.into(), parent, ns: if keep { ns } else { 0 }, abc: if keep { abc_id } else { 0 } }
}

fn ns_key(name: &str, ns: Dotted, abc_id: usize, ns_index: usize, kind: u8) -> NsKey {
    let keep = abc_id != 0 && ns_index != 0 && kind == ns_kind::PRIVATE;
    NsKey { name: name.into(), ns, ns_index: if keep { ns_index } else { 0 }, abc: if keep { abc_id } else { 0 } }
}

/// FFDec's `AbcIndexing`.
#[derive(Default)]
pub struct AbcIndex {
    level: usize,
    parent: Option<Arc<AbcIndex>>,
    abc_ids: Vec<usize>,
    classes: HashMap<ClassKey, usize>,
    class_list: Vec<ClassInfo>,
    instance_props: HashMap<PropKey, TraitInfo>,
    class_props: HashMap<PropKey, TraitInfo>,
    script_props: HashMap<NsKey, TraitInfo>,
    instance_ambiguous: HashMap<(String, TySpec), Vec<u8>>,
    class_ambiguous: HashMap<(String, TySpec), Vec<u8>>,
    ns_maps: Vec<HashMap<String, Dotted>>,
    trait_paths: Vec<(Dotted, String)>,
}

fn raw_names(abc: &Abc) -> (Idents, HashMap<String, Dotted>, HashSet<usize>) {
    (Idents::default(), HashMap::new(), crate::abc::usages::colliding_multinames(abc))
}

impl AbcIndex {
    pub fn new(parent: Option<Arc<AbcIndex>>) -> AbcIndex {
        let level = parent.as_ref().map_or(0, |p| p.level + 1);
        AbcIndex { level, parent, ..Default::default() }
    }

    /// An index over `abcs`, on top of the player's global library.
    pub fn for_abcs(abcs: &[&Abc]) -> Arc<AbcIndex> {
        let mut idx = AbcIndex::new(Some(player_global()));
        for abc in abcs {
            idx.add_abc(abc, *abc as *const Abc as usize);
        }
        Arc::new(idx)
    }

    fn class(&self, r: ClassRef) -> &ClassInfo {
        if r.level == self.level {
            return &self.class_list[r.id];
        }
        self.parent.as_ref().expect("class level").class(r)
    }

    fn trait_ret(names: &Names, abc: &Abc, t: &Trait) -> TySpec {
        let m = |i: usize| mn_spec(names, i).unwrap_or(TySpec::Unbounded);
        match &t.data {
            TraitData::Slot { type_name, .. } | TraitData::Const { type_name, .. } => {
                if *type_name == 0 {
                    TySpec::Unbounded
                } else {
                    m(*type_name)
                }
            }
            TraitData::Getter { method, .. } => m(abc.methods[*method].return_type),
            TraitData::Setter { method, .. } => match abc.methods[*method].param_types.first() {
                Some(&p) => m(p),
                None => TySpec::Unbounded,
            },
            TraitData::Method { .. } | TraitData::Function { .. } => TySpec::named(Dotted::new(&["Function"])),
            TraitData::Class { .. } => TySpec::Unbounded,
        }
    }

    fn trait_call_ret(names: &Names, abc: &Abc, t: &Trait) -> TySpec {
        let m = |i: usize| mn_spec(names, i).unwrap_or(TySpec::Unbounded);
        match &t.data {
            TraitData::Method { method, .. } | TraitData::Function { method, .. } => m(abc.methods[*method].return_type),
            _ => TySpec::Unbounded,
        }
    }

    fn own_ns(abc: &Abc, mn: usize) -> usize {
        match abc.constants.multinames.get(mn) {
            Some(Multiname::QName { ns, .. }) => *ns,
            _ => 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn index_traits(&mut self, names: &Names, abc: &Abc, abc_id: usize, name_index: usize, traits: &[Trait], kind: u8) {
        let owner = mn_spec(names, name_index).unwrap_or(TySpec::Unbounded);
        for t in traits {
            let ti = TraitInfo { ret: Self::trait_ret(names, abc, t), call_ret: Self::trait_call_ret(names, abc, t) };
            let plain = names.name(t.name, &[], true, false);
            let tns = Self::own_ns(abc, t.name);
            let tkind = names.ns_kind(tns);
            let amb = match kind {
                0 => None::<&mut HashMap<(String, TySpec), Vec<u8>>>,
                1 => Some(&mut self.instance_ambiguous),
                _ => Some(&mut self.class_ambiguous),
            };
            if let Some(map) = amb {
                let key = (plain.clone(), owner.clone());
                match map.get_mut(&key) {
                    None => {
                        map.insert(key, vec![tkind]);
                    }
                    Some(list) => {
                        if list.iter().any(|&k| k != tkind) {
                            list.push(tkind);
                        }
                    }
                }
            }
            match kind {
                0 => {
                    let ns = if tns == 0 { Dotted::default() } else { names.ns_name(tns) };
                    let key = ns_key(&names.name(t.name, &[], true, true), ns, abc_id, tns, tkind);
                    self.script_props.entry(key).or_insert(ti);
                }
                1 => {
                    self.instance_props.insert(prop_key(&plain, owner.clone(), abc_id, tns, tkind), ti);
                }
                _ => {
                    self.class_props.insert(prop_key(&plain, owner.clone(), abc_id, tns, tkind), ti);
                }
            }
        }
    }

    fn script_pkg(names: &Names, abc: &Abc, script: usize) -> Option<Dotted> {
        let mut pkg = None;
        for t in &abc.scripts[script].traits {
            if names.simple_ns_kind(t.name) == ns_kind::PACKAGE {
                pkg = names.simple_ns_name(t.name);
            }
        }
        pkg
    }

    /// `addAbc`.
    pub fn add_abc(&mut self, abc: &Abc, abc_id: usize) {
        let (ids, cns, dns) = raw_names(abc);
        let names = Names { abc, ids: &ids, custom_ns: &cns, display_ns: &dns };
        let mut added: Vec<(usize, usize)> = Vec::new();
        for (si, script) in abc.scripts.iter().enumerate() {
            self.index_traits(&names, abc, abc_id, 0, &script.traits, 0);
            for t in &script.traits {
                let TraitData::Class { class, .. } = t.data else { continue };
                let Some(ii) = abc.instances.get(class) else { continue };
                let kind = names.simple_ns_kind(t.name);
                let script_index = if kind == ns_kind::PACKAGE { None } else { Some(si) };
                let cname = mn_spec(&names, ii.name).unwrap_or(TySpec::Unbounded);
                let pkg = script_index.and_then(|s| Self::script_pkg(&names, abc, s));
                let id = self.class_list.len();
                let tn = |ts: &[Trait]| ts.iter().map(|t| (names.name(t.name, &[], true, true), names.simple_ns_kind(t.name))).collect::<Vec<_>>();
                self.class_list.push(ClassInfo {
                    full_name: names.name_with_ns(ii.name, true),
                    full_name_no_suffix: names.name_with_ns(ii.name, false),
                    parent: None,
                    instance_traits: tn(&ii.traits),
                    static_traits: tn(&abc.classes[class].traits),
                });
                self.classes.insert(ClassKey { ty: cname.no_ns(), pkg }, id);
                added.push((id, class));
                self.index_traits(&names, abc, abc_id, ii.name, &ii.traits, 1);
                self.index_traits(&names, abc, abc_id, ii.name, &abc.classes[class].traits, 2);
            }
        }
        for (id, class) in added {
            let ii = &abc.instances[class];
            if ii.super_name > 0 {
                let parent = TySpec::named(names.name_with_ns(ii.super_name, true));
                self.class_list[id].parent = self.find_class_spec(&parent, None);
            }
        }
        for script in &abc.scripts {
            for t in &script.traits {
                let ns = Self::own_ns(abc, t.name);
                let pkg = if ns == 0 { Dotted::default() } else { names.ns_name(ns) };
                self.trait_paths.push((pkg, names.name(t.name, &[], true, false)));
            }
        }
        let mut map = HashMap::new();
        for script in &abc.scripts {
            for t in &script.traits {
                if let TraitData::Slot { value: Some(v), .. } | TraitData::Const { value: Some(v), .. } = t.data {
                    if v.kind == ns_kind::NAMESPACE {
                        let nsn = abc.constants.namespaces.get(v.index).map_or(0, |n| n.name);
                        map.insert(abc.str(nsn).to_string(), names.name_with_ns(t.name, true));
                    }
                }
            }
        }
        self.ns_maps.push(map);
        self.abc_ids.push(abc_id);
    }

    /// Object names `getPackageObjects` lists for a package.
    pub fn package_objects(&self, pkg: &Dotted) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut add = |n: String| {
            if !out.contains(&n) {
                out.push(n);
            }
        };
        for k in self.classes.keys() {
            if k.pkg.as_ref() == Some(pkg) {
                if let TySpec::Type(d, _) = &k.ty {
                    add(d.last().to_string());
                }
            }
        }
        for k in self.script_props.keys() {
            if k.ns == *pkg {
                add(k.name.clone());
            }
        }
        if let Some(p) = &self.parent {
            for n in p.package_objects(pkg) {
                add(n);
            }
        }
        out
    }

    /// Names of the top-level definitions in `pkg` across the indexed ABCs (not the player library).
    pub fn names_in_package(&self, pkg: &Dotted) -> Vec<String> {
        self.trait_paths.iter().filter(|(p, _)| p == pkg).map(|(_, n)| n.clone()).collect()
    }

    /// Public and protected trait names of a class and its ancestors (`getAllClassTraitNames` with `isParent`).
    pub fn inherited_trait_names(&self, class_name: &Dotted, pkg: Option<Dotted>) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = self.find_class_spec(&TySpec::named(class_name.clone()), pkg);
        let mut guard = 0;
        while let Some(r) = cur {
            guard += 1;
            if guard > 64 {
                break;
            }
            let ci = self.class(r);
            for (n, k) in &ci.instance_traits {
                if *k == ns_kind::PACKAGE || *k == ns_kind::PROTECTED {
                    out.push(n.clone());
                }
            }
            for (n, k) in &ci.static_traits {
                if *k == ns_kind::PACKAGE || *k == ns_kind::STATIC_PROTECTED {
                    out.push(n.clone());
                }
            }
            cur = ci.parent;
        }
        out
    }

    /// Full names of a class's ancestors, nearest first.
    pub fn ancestors(&self, class_name: &Dotted, pkg: Option<Dotted>) -> Vec<Dotted> {
        let mut out = Vec::new();
        let mut cur = self.find_class_spec(&TySpec::named(class_name.clone()), pkg).and_then(|r| self.class(r).parent);
        while let Some(r) = cur {
            if out.len() > 64 {
                break;
            }
            out.push(self.class(r).full_name.clone());
            cur = self.class(r).parent;
        }
        out
    }

    /// Whether the class or one of its ancestors in this index alone defines `name` (`findProperty` on the local index).
    pub fn local_instance_property(&self, class_name: &Dotted, name: &str) -> bool {
        let mut cur = self.classes.get(&ClassKey { ty: TySpec::named(class_name.clone()), pkg: None }).copied();
        let mut guard = 0;
        while let Some(id) = cur {
            guard += 1;
            if guard > 64 {
                break;
            }
            let ci = &self.class_list[id];
            let key = PropKey { name: name.into(), parent: TySpec::named(ci.full_name.clone()), ns: 0, abc: 0 };
            if self.instance_props.contains_key(&key) {
                return true;
            }
            cur = match ci.parent {
                Some(r) if r.level == self.level => Some(r.id),
                _ => None,
            };
        }
        false
    }

    fn find_class_spec(&self, cls: &TySpec, pkg: Option<Dotted>) -> Option<ClassRef> {
        if pkg.is_some() {
            if let Some(&id) = self.classes.get(&ClassKey { ty: cls.no_ns(), pkg }) {
                return Some(ClassRef { level: self.level, id });
            }
        }
        if let Some(&id) = self.classes.get(&ClassKey { ty: cls.no_ns(), pkg: None }) {
            return Some(ClassRef { level: self.level, id });
        }
        self.parent.as_ref()?.find_class_spec(cls, None)
    }

    fn find_property(&self, prop: &PropKey, find_static: bool, find_instance: bool, find_protected: bool, depth: u8) -> Option<(TraitInfo, bool)> {
        if depth > 64 {
            return None;
        }
        if find_static {
            if let Some(t) = self.class_props.get(prop) {
                return Some((t.clone(), true));
            }
        }
        if find_instance {
            if let Some(t) = self.instance_props.get(prop) {
                return Some((t.clone(), false));
            }
        }
        let ci = self.find_class_spec(&prop.parent, None);
        if let Some(ci) = ci {
            if let Some(pr) = self.class(ci).parent {
                if prop.abc == 0 || prop.ns == 0 {
                    let parent_def = PropKey { name: prop.name.clone(), parent: TySpec::named(self.class(pr).full_name.clone()), ns: 0, abc: 0 };
                    if let Some(r) = self.find_property(&parent_def, find_static, find_instance, find_protected, depth + 1) {
                        return Some(r);
                    }
                }
            }
            if find_protected && prop.ns == 0 {
                if let Some(r) = self.find_property(prop, find_static, find_instance, false, depth + 1) {
                    return Some(r);
                }
            }
        }
        self.parent.as_ref()?.find_property(prop, find_static, find_instance, find_protected, depth + 1)
    }

    fn ambiguous(&self, name: &str, parent: &TySpec, find_static: bool, find_instance: bool, depth: u8) -> Option<bool> {
        if depth > 64 {
            return None;
        }
        let key = (name.to_string(), parent.clone());
        if find_static {
            if let Some(l) = self.class_ambiguous.get(&key) {
                return Some(l.len() > 1);
            }
        }
        if find_instance {
            if let Some(l) = self.instance_ambiguous.get(&key) {
                return Some(l.len() > 1);
            }
        }
        let ci = self.find_class_spec(parent, None)?;
        let pr = self.class(ci).parent?;
        let pn = TySpec::named(self.class(pr).full_name_no_suffix.clone());
        self.ambiguous(name, &pn, find_static, find_instance, depth + 1)
    }

    fn script_property(&self, name: &str, ns: &Dotted) -> Option<TraitInfo> {
        let key = NsKey { name: name.into(), ns: ns.clone(), ns_index: 0, abc: 0 };
        match self.script_props.get(&key) {
            Some(t) => Some(t.clone()),
            None => self.parent.as_ref()?.script_property(name, ns),
        }
    }

    /// `nsValueToName`.
    pub fn ns_value_to_name(&self, uri: &str) -> Option<Dotted> {
        for m in &self.ns_maps {
            if let Some(d) = m.get(uri) {
                if !d.is_empty() {
                    return Some(d.clone());
                }
            }
            match uri {
                "http://www.adobe.com/2006/actionscript/flash/proxy" => return Some(Dotted::new(&["flash", "utils", "flash_proxy"])),
                "http://adobe.com/AS3/2006/builtin" => return Some(Dotted::new(&["-"])),
                _ => {}
            }
        }
        self.parent.as_ref()?.ns_value_to_name(uri)
    }

    /// The custom namespace names of every namespace URI in `abc`.
    pub fn custom_namespaces(&self, abc: &Abc) -> HashMap<String, Dotted> {
        let mut out = HashMap::new();
        for ns in &abc.constants.namespaces {
            if ns.kind != ns_kind::NAMESPACE && ns.kind != ns_kind::PACKAGE_INTERNAL {
                continue;
            }
            let uri = abc.str(ns.name);
            if out.contains_key(uri) {
                continue;
            }
            if let Some(d) = self.ns_value_to_name(uri) {
                out.insert(uri.to_string(), d);
            }
        }
        out
    }
}

/// The index of the embedded player global library, built once.
pub fn player_global() -> Arc<AbcIndex> {
    static PG: OnceLock<Arc<AbcIndex>> = OnceLock::new();
    PG.get_or_init(|| {
        let mut idx = AbcIndex::new(None);
        let data = include_bytes!("../../../resources/playerglobal.swf");
        if let Ok(swf) = crate::Swf::parse(data) {
            let mut abcs = Vec::new();
            swf.walk(|_, t| {
                if let Some((d, _)) = tag_abc(t) {
                    if let Ok(a) = Abc::parse(d) {
                        abcs.push(a);
                    }
                }
            });
            for (i, a) in abcs.iter().enumerate() {
                idx.add_abc(a, i + 1);
            }
        }
        Arc::new(idx)
    })
    .clone()
}

fn abc_id(ctx: &Ctx) -> usize {
    ctx.abc as *const Abc as usize
}

/// `AbcIndexing.findPropertyTypeOrCallType`.
pub fn find_property_type(ctx: &Ctx, owner: &It, name: &str, ns: usize, find_static: bool, find_instance: bool, find_protected: bool) -> Option<PropType> {
    let idx = ctx.index.as_ref()?;
    let parent = TySpec::of(owner)?;
    let kind = ctx.names().ns_kind(ns);
    let key = prop_key(name, parent, abc_id(ctx), ns, kind);
    let (t, st) = idx.find_property(&key, find_static, find_instance, find_protected, 0)?;
    Some(PropType { ty: t.ret.item(), call_ty: t.call_ret.item(), is_static: st })
}

/// The package `ClassDef` derives from a script's public traits.
pub fn script_package(ctx: &Ctx, script: i32) -> Option<Dotted> {
    if script >= 0 && (script as usize) < ctx.abc.scripts.len() {
        AbcIndex::script_pkg(&ctx.names(), ctx.abc, script as usize)
    } else {
        None
    }
}

/// `AbcIndexing.findClass(...) != null`.
pub fn find_class(ctx: &Ctx, ty: &It, script: i32) -> bool {
    let Some(idx) = ctx.index.as_ref() else { return false };
    let Some(spec) = TySpec::of(ty) else { return false };
    let pkg = if script >= 0 && (script as usize) < ctx.abc.scripts.len() { AbcIndex::script_pkg(&ctx.names(), ctx.abc, script as usize) } else { None };
    idx.find_class_spec(&spec, pkg).is_some()
}

/// `AbcIndexing.findScriptProperty`: (return type, call return type).
pub fn find_script_property(ctx: &Ctx, name: &str, ns: &Dotted) -> Option<(It, It)> {
    let t = ctx.index.as_ref()?.script_property(name, ns)?;
    Some((t.ret.item(), t.call_ret.item()))
}

/// `AbcIndexing.isPropertyAmbiguous` for a multiname property of `parent`.
pub fn is_property_ambiguous(ld: &LocalData, mn: usize, parent: &It) -> Option<bool> {
    let raw = ld.names().name(mn, &ld.fqn, true, true);
    is_property_ambiguous_name(ld, &raw, parent)
}

pub fn is_property_ambiguous_name(ld: &LocalData, name: &str, parent: &It) -> Option<bool> {
    let idx = ld.ctx.index.as_ref()?;
    idx.ambiguous(name, &TySpec::of(parent)?, true, true, 0)
}
