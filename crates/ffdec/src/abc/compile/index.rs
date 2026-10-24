//! Classes and traits of every ABC in the SWF, over the player's global library.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use crate::abc::decomp::index::{mn_spec, TySpec};
use crate::abc::decomp::names::{Dotted, Idents, Names};
use crate::abc::{ns_kind, tag_abc, Abc, Multiname, Trait, TraitData};

/// Which ABC an entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AbcId {
    Sel,
    Swf(usize),
    Pg(usize),
}

#[derive(Debug, Clone)]
pub struct TraitIndex {
    pub tr: Trait,
    pub abc: AbcId,
    pub ret: TySpec,
    pub call_ret: TySpec,
    pub value: Option<(usize, u8)>,
    pub obj_type: TySpec,
    pub script_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassRef {
    pub level: usize,
    pub id: usize,
}

#[derive(Debug, Clone)]
pub struct ClassEntry {
    pub index: usize,
    pub abc: AbcId,
    pub parent: Option<ClassRef>,
    pub script_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PropKey {
    pub name: String,
    pub parent: TySpec,
    pub ns_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NsKey {
    pub name: String,
    pub ns: Dotted,
    pub ns_index: usize,
    pub abc: Option<AbcId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ClassKey {
    ty: TySpec,
    pkg: Option<Dotted>,
}

#[derive(Default, Clone)]
pub struct Level {
    classes: HashMap<ClassKey, ClassRef>,
    arena: Vec<ClassEntry>,
    instance_props: HashMap<PropKey, TraitIndex>,
    class_props: HashMap<PropKey, TraitIndex>,
    instance_ns: HashMap<NsKey, TraitIndex>,
    class_ns: HashMap<NsKey, TraitIndex>,
    script_props: HashMap<NsKey, TraitIndex>,
}

/// Traits, classes and scripts of the selected ABC that `ScriptPack.delete` marked.
#[derive(Debug, Default, Clone)]
pub struct Deleted {
    pub scripts: HashSet<usize>,
    pub classes: HashSet<usize>,
    pub traits: HashSet<(Owner, usize)>,
    pub methods: HashSet<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Owner {
    Script(usize),
    Instance(usize),
    Class(usize),
}

pub fn raw_names(abc: &Abc) -> Names<'_> {
    static IDS: OnceLock<IdHolder> = OnceLock::new();
    let h = IDS.get_or_init(IdHolder::default);
    Names { abc, ids: &h.ids, custom_ns: &h.cns, display_ns: &h.dns }
}

#[derive(Default)]
struct IdHolder {
    ids: Idents,
    cns: HashMap<String, Dotted>,
    dns: HashSet<usize>,
}

unsafe impl Sync for IdHolder {}
unsafe impl Send for IdHolder {}

/// `multinameToType`, with Java's null (a cyclic type name) as `*`.
pub fn mn_type(abc: &Abc, mn: usize) -> TySpec {
    mn_spec(&raw_names(abc), mn).unwrap_or(TySpec::Unbounded)
}

fn function_ty() -> TySpec {
    TySpec::Type(Dotted::new(&["Function"]), None)
}

pub fn trait_ret(abc: &Abc, t: &Trait) -> TySpec {
    match &t.data {
        TraitData::Slot { type_name, .. } | TraitData::Const { type_name, .. } => {
            if *type_name == 0 {
                TySpec::Unbounded
            } else {
                mn_type(abc, *type_name)
            }
        }
        TraitData::Getter { method, .. } => abc.methods.get(*method).map_or(TySpec::Unbounded, |m| mn_type(abc, m.return_type)),
        TraitData::Setter { method, .. } => match abc.methods.get(*method).and_then(|m| m.param_types.first()) {
            Some(&p) => mn_type(abc, p),
            None => TySpec::Unbounded,
        },
        TraitData::Method { .. } | TraitData::Function { .. } => function_ty(),
        TraitData::Class { .. } => TySpec::Unbounded,
    }
}

fn trait_call_ret(abc: &Abc, t: &Trait) -> TySpec {
    match &t.data {
        TraitData::Method { method, .. } | TraitData::Function { method, .. } => abc.methods.get(*method).map_or(TySpec::Unbounded, |m| mn_type(abc, m.return_type)),
        _ => TySpec::Unbounded,
    }
}

fn own_ns(abc: &Abc, mn: usize) -> usize {
    match abc.constants.multinames.get(mn) {
        Some(Multiname::QName { ns, .. }) => *ns,
        _ => 0,
    }
}

fn ns_kind_of(abc: &Abc, ns: usize) -> u8 {
    abc.constants.namespaces.get(ns).map_or(0, |n| n.kind)
}

/// `PropertyDef`'s namespace index: kept only for private and explicit namespaces.
pub fn prop_ns_index(abc: Option<&Abc>, ns: usize) -> usize {
    let Some(abc) = abc else { return 0 };
    if ns == 0 {
        return 0;
    }
    match ns_kind_of(abc, ns) {
        ns_kind::PACKAGE | ns_kind::PACKAGE_INTERNAL | ns_kind::PROTECTED | ns_kind::STATIC_PROTECTED | ns_kind::NAMESPACE => 0,
        _ => ns,
    }
}

pub fn prop_key(name: &str, parent: TySpec, abc: Option<&Abc>, ns: usize) -> PropKey {
    PropKey { name: name.to_string(), parent, ns_index: prop_ns_index(abc, ns) }
}

pub fn ns_key(name: &str, ns: Dotted, abc: Option<(&Abc, AbcId)>, ns_index: usize) -> NsKey {
    if let Some((a, id)) = abc {
        if ns_index > 0 && ns_kind_of(a, ns_index) == ns_kind::PRIVATE {
            return NsKey { name: name.to_string(), ns, ns_index, abc: Some(id) };
        }
    }
    NsKey { name: name.to_string(), ns, ns_index: 0, abc: None }
}

fn no_ns(t: &TySpec) -> TySpec {
    match t {
        TySpec::Type(f, _) => TySpec::Type(f.clone(), None),
        o => o.clone(),
    }
}

/// The package `ClassDef` takes from a script's package-namespaced traits.
pub fn script_pkg(abc: &Abc, script: usize) -> Option<Dotted> {
    let names = raw_names(abc);
    let mut pkg = None;
    for t in &abc.scripts.get(script)?.traits {
        if names.simple_ns_kind(t.name) == ns_kind::PACKAGE {
            pkg = names.simple_ns_name(t.name);
        }
    }
    pkg
}

impl Level {
    fn index_traits(&mut self, abc: &Abc, id: AbcId, name_index: usize, traits: &[Trait], owner: Option<Owner>, deleted: Option<&Deleted>, which: u8, script_index: usize) {
        let names = raw_names(abc);
        let obj_type = mn_type(abc, name_index);
        for (ti, t) in traits.iter().enumerate() {
            if let (Some(d), Some(o)) = (deleted, owner) {
                if d.traits.contains(&(o, ti)) {
                    continue;
                }
            }
            let value = match &t.data {
                TraitData::Slot { value, .. } | TraitData::Const { value, .. } => Some(value.map_or((0, 0), |v| (v.index, v.kind))),
                _ => None,
            };
            let entry = TraitIndex { tr: t.clone(), abc: id, ret: trait_ret(abc, t), call_ret: trait_call_ret(abc, t), value, obj_type: obj_type.clone(), script_index };
            let ns = own_ns(abc, t.name);
            match which {
                0 => {
                    let nsname = if ns == 0 { Dotted::default() } else { names.ns_name(ns) };
                    let k = ns_key(&names.name(t.name, &[], true, true), nsname, Some((abc, id)), ns);
                    self.script_props.entry(k).or_insert(entry);
                }
                _ => {
                    let k = prop_key(&names.name(t.name, &[], true, false), obj_type.clone(), Some(abc), ns);
                    let nsname = if ns == 0 { Dotted::default() } else { names.ns_name(ns) };
                    let nk = ns_key(&names.name(t.name, &[], true, true), nsname, Some((abc, id)), ns);
                    if which == 1 {
                        self.instance_props.insert(k, entry.clone());
                        self.instance_ns.entry(nk).or_insert(entry);
                    } else {
                        self.class_props.insert(k, entry.clone());
                        self.class_ns.entry(nk).or_insert(entry);
                    }
                }
            }
        }
    }

    pub fn remove_abc(&mut self, id: AbcId) {
        self.classes.retain(|_, r| self.arena[r.id].abc != id);
        self.instance_props.retain(|_, t| t.abc != id);
        self.class_props.retain(|_, t| t.abc != id);
        self.script_props.retain(|_, t| t.abc != id);
        self.class_ns.retain(|_, t| t.abc != id);
        self.instance_ns.retain(|_, t| t.abc != id);
    }
}

pub struct PlayerGlobal {
    pub abcs: Vec<Abc>,
    pub level: Level,
}

pub fn player_global() -> &'static PlayerGlobal {
    static PG: OnceLock<PlayerGlobal> = OnceLock::new();
    PG.get_or_init(|| {
        let data = include_bytes!("../../../resources/playerglobal.swf");
        let mut abcs = Vec::new();
        if let Ok(swf) = crate::Swf::parse(data) {
            swf.walk(|_, t| {
                if let Some((d, _)) = tag_abc(t) {
                    if let Ok(a) = Abc::parse(d) {
                        abcs.push(a);
                    }
                }
            });
        }
        let mut idx = Index { levels: vec![Level::default(), Level::default()], with_player: false, found: Default::default() };
        for (i, a) in abcs.iter().enumerate() {
            idx.add_abc(1, a, AbcId::Pg(i), None);
        }
        let level = idx.levels.pop().unwrap();
        PlayerGlobal { abcs, level }
    })
}

/// The SWF's index; level 1 is the player global library.
pub struct Index {
    pub levels: Vec<Level>,
    /// Whether the player global library sits under these levels.
    pub with_player: bool,
    /// `find_property` results, cleared whenever an ABC is added.
    found: std::cell::RefCell<HashMap<(PropKey, u8), Option<(TraitIndex, bool)>>>,
}

impl Index {
    pub fn new() -> Index {
        Index { levels: vec![Level::default()], with_player: true, found: Default::default() }
    }

    fn level(&self, l: usize) -> Option<&Level> {
        if l < self.levels.len() {
            Some(&self.levels[l])
        } else if l == self.levels.len() && self.with_player {
            Some(&player_global().level)
        } else {
            None
        }
    }

    pub fn class(&self, r: ClassRef) -> &ClassEntry {
        &self.level(r.level).unwrap().arena[r.id]
    }

    /// `addAbc`, into level `l`.
    pub fn add_abc(&mut self, l: usize, abc: &Abc, id: AbcId, deleted: Option<&Deleted>) {
        self.found.get_mut().clear();
        let names = raw_names(abc);
        let mut added = Vec::new();
        for (si, script) in abc.scripts.iter().enumerate() {
            if deleted.is_some_and(|d| d.scripts.contains(&si)) && false {
                continue;
            }
            self.levels[l].index_traits(abc, id, 0, &script.traits, Some(Owner::Script(si)), deleted, 0, si);
            for (ti, t) in script.traits.iter().enumerate() {
                if deleted.is_some_and(|d| d.traits.contains(&(Owner::Script(si), ti))) {
                    continue;
                }
                let TraitData::Class { class, .. } = t.data else { continue };
                if deleted.is_some_and(|d| d.classes.contains(&class)) {
                    continue;
                }
                let Some(ii) = abc.instances.get(class) else { continue };
                let kind = names.simple_ns_kind(t.name);
                let script_index = if kind == ns_kind::PACKAGE { None } else { Some(si) };
                let lv = &mut self.levels[l];
                let r = ClassRef { level: l, id: lv.arena.len() };
                lv.arena.push(ClassEntry { index: class, abc: id, parent: None, script_index });
                added.push(r);
                let cname = mn_type(abc, ii.name);
                let pkg = script_index.and_then(|s| script_pkg(abc, s));
                lv.classes.insert(ClassKey { ty: no_ns(&cname), pkg }, r);
                lv.index_traits(abc, id, ii.name, &ii.traits, Some(Owner::Instance(class)), deleted, 1, si);
                if let Some(c) = abc.classes.get(class) {
                    lv.index_traits(abc, id, ii.name, &c.traits, Some(Owner::Class(class)), deleted, 2, si);
                }
            }
        }
        for r in added {
            let ci = self.levels[l].arena[r.id].index;
            let sup = abc.instances[ci].super_name;
            if sup > 0 {
                let parent = TySpec::Type(names.name_with_ns(sup, true), None);
                let p = self.find_class_from(l, &parent, None);
                self.levels[l].arena[r.id].parent = p;
            }
        }
    }

    /// `refreshAbc` for the selected ABC.
    pub fn refresh(&mut self, abc: &Abc, deleted: &Deleted) {
        self.levels[0].remove_abc(AbcId::Sel);
        self.add_abc(0, abc, AbcId::Sel, Some(deleted));
    }

    /// `findClass(cls, abc, scriptIndex)`, with the package already taken from the script.
    pub fn find_class(&self, cls: &TySpec, pkg: Option<Dotted>) -> Option<ClassRef> {
        self.find_class_from(0, cls, pkg)
    }

    fn find_class_from(&self, l: usize, cls: &TySpec, pkg: Option<Dotted>) -> Option<ClassRef> {
        let lv = self.level(l)?;
        let ty = no_ns(cls);
        if pkg.is_some() {
            if let Some(r) = lv.classes.get(&ClassKey { ty: ty.clone(), pkg: pkg.clone() }) {
                return Some(*r);
            }
        }
        if let Some(r) = lv.classes.get(&ClassKey { ty, pkg: None }) {
            return Some(*r);
        }
        self.find_class_from(l + 1, cls, pkg)
    }

    pub fn find_script_property(&self, name: &str, ns: &Dotted) -> Option<&TraitIndex> {
        let k = NsKey { name: name.to_string(), ns: ns.clone(), ns_index: 0, abc: None };
        let mut l = 0;
        while let Some(lv) = self.level(l) {
            if let Some(t) = lv.script_props.get(&k) {
                return Some(t);
            }
            l += 1;
        }
        None
    }

    /// `findProperty`; the bool is whether it was found static.
    pub fn find_property<'a>(&self, abc_of: &dyn Fn(AbcId) -> &'a Abc, key: &PropKey, find_static: bool, find_instance: bool, find_protected: bool) -> Option<(TraitIndex, bool)> {
        let k = (key.clone(), find_static as u8 | (find_instance as u8) << 1 | (find_protected as u8) << 2);
        if let Some(r) = self.found.borrow().get(&k) {
            return r.clone();
        }
        let r = self.find_property_from(0, abc_of, key, find_static, find_instance, find_protected);
        self.found.borrow_mut().insert(k, r.clone());
        r
    }

    fn find_property_from<'a>(&self, l: usize, abc_of: &dyn Fn(AbcId) -> &'a Abc, key: &PropKey, find_static: bool, find_instance: bool, find_protected: bool) -> Option<(TraitIndex, bool)> {
        let lv = self.level(l)?;
        if find_static {
            if let Some(t) = lv.class_props.get(key) {
                return Some((t.clone(), true));
            }
        }
        if find_instance {
            if let Some(t) = lv.instance_props.get(key) {
                return Some((t.clone(), false));
            }
        }
        let ci = self.find_class_from(l, &key.parent, None);
        if let Some(ci) = ci {
            let c = self.class(ci).clone();
            if let Some(p) = c.parent {
                if key.ns_index == 0 {
                    let pc = self.class(p).clone();
                    let pabc = abc_of(pc.abc);
                    let pname = raw_names(pabc).name_with_ns(pabc.instances[pc.index].name, true);
                    let pdef = PropKey { name: key.name.clone(), parent: TySpec::Type(pname, None), ns_index: 0 };
                    if let Some(r) = self.find_property_from(l, abc_of, &pdef, find_static, find_instance, find_protected) {
                        return Some(r);
                    }
                }
            }
            if find_protected && key.ns_index == 0 {
                let cabc = abc_of(c.abc);
                let prot = cabc.instances[c.index].protected_ns;
                let k2 = prop_key(&key.name, key.parent.clone(), Some(cabc), prot);
                if let Some(r) = self.find_property_from(l, abc_of, &k2, find_static, find_instance, false) {
                    return Some(r);
                }
            }
        }
        self.find_property_from(l + 1, abc_of, key, find_static, find_instance, find_protected)
    }

    /// `findNsProperty`.
    pub fn find_ns_property<'a>(&self, abc_of: &dyn Fn(AbcId) -> &'a Abc, key: &NsKey, find_static: bool, find_instance: bool) -> Option<(TraitIndex, bool)> {
        let lv = &self.levels[0];
        if find_static {
            if let Some(t) = lv.class_ns.get(key) {
                return Some((t.clone(), true));
            }
        }
        if find_instance {
            if let Some(t) = lv.instance_ns.get(key) {
                return Some((t.clone(), false));
            }
        }
        let pk = PropKey { name: key.name.clone(), parent: TySpec::Type(key.ns.clone(), None), ns_index: if key.abc.is_some() { key.ns_index } else { 0 } };
        if !self.with_player {
            return None;
        }
        self.find_property_from(1, abc_of, &pk, find_static, find_instance, true)
    }
}

impl Default for Index {
    fn default() -> Self {
        Index::new()
    }
}

/// The full name of class `r`, as `getNameWithNamespace(..., true)`.
pub fn class_name<'a>(index: &Index, abc_of: &dyn Fn(AbcId) -> &'a Abc, r: ClassRef) -> Dotted {
    let c = index.class(r);
    let a = abc_of(c.abc);
    raw_names(a).name_with_ns(a.instances[c.index].name, true)
}

pub fn pg_abc(i: usize) -> &'static Abc {
    &player_global().abcs[i]
}

pub type Shared = Arc<Vec<Abc>>;
