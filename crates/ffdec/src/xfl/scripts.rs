//! ActionScript in the XFL: AS1/2 frame, button, clip and `#initclip` scripts, AS3 frame
//! scripts taken from the class that registers them, and the `.as` files next to the FLA.

use std::collections::{BTreeMap, HashMap};

use super::convert::{Converter, Item, KEY_MODE_NORMAL};
use super::num::java_double;
use super::xml::XmlWriter;
use crate::abc::decomp::item::{It, List, K};
use crate::action::decomp::items::direct_string;
use crate::action::scripts::{Kind, Source};
use crate::amf::{Amf, Obj, Value};
use crate::tags::code::*;

pub const PUBLISH_DATA: &str = "PUB_PRST_DATA_EMBED_SWF_";

/// The AS3 class behind a timeline (`ScriptPack`), by raw dotted name.
#[derive(Debug, Clone)]
pub struct Pack {
    pub class: String,
}

/// `AccessibilityBag`: per-instance accessibility attributes over frame ranges.
#[derive(Debug, Clone, Default)]
pub struct Accessibility {
    items: Vec<(String, String, String, i32, i32)>,
}

impl Accessibility {
    pub fn attributes(&self, instance: &str, frame: i32) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        for (name, key, value, start, end) in &self.items {
            if name == instance && frame >= *start && frame <= *end {
                match out.iter_mut().find(|(k, _)| k == key) {
                    Some(e) => e.1 = value.clone(),
                    None => out.push((key.clone(), value.clone())),
                }
            }
        }
        out
    }
}

/// `persistentData` of a placed instance's AMF metadata (dynamic object members only).
pub fn persistent_data(amf: &Amf, v: &Value, w: &mut XmlWriter) {
    let Value::Obj(i) = v else { return };
    let Some(Obj::Object { dynamic: true, members, .. }) = amf.objs.get(*i) else { return };
    w.start("persistentData");
    let mut exported: Vec<&str> = Vec::new();
    for (n, v) in members {
        match v {
            Value::Int(x) => {
                exported.push(n);
                w.start("PD");
                w.attr("n", n);
                w.attr("t", "i");
                w.attr_i("v", *x);
                w.end();
                exported.push(n);
            }
            Value::Double(d) => {
                w.start("PD");
                w.attr("n", n);
                w.attr("t", "d");
                w.attr("v", &java_double(*d));
                w.end();
                exported.push(n);
            }
            Value::Str(s) => {
                w.start("PD");
                w.attr("n", n);
                w.attr("v", s);
                w.end();
                exported.push(n);
            }
            _ => {}
        }
    }
    for n in exported {
        w.start("PD");
        w.attr("n", &format!("{PUBLISH_DATA}{n}"));
        w.attr("t", "i");
        w.attr_i("v", 1);
        w.end();
    }
    w.end();
}

/// `getMembersToClassName`: `a.b.C` from nested member reads.
fn members_to_class_name(it: &It) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = it.clone();
    loop {
        let next = match &cur.borrow().k {
            K::AGetMember { obj, member, .. } => {
                parts.insert(0, direct_string(member)?);
                obj.clone()
            }
            K::AGetVar { name, .. } => {
                parts.insert(0, direct_string(name)?);
                return Some(parts.join("."));
            }
            _ => return None,
        };
        cur = next;
    }
}

fn direct_text(it: &It) -> Option<String> {
    match &it.borrow().k {
        K::ADirect { v, .. } => Some(match v {
            crate::abc::decomp::item::AVal::Str(s) => s.clone(),
            other => format!("{other:?}"),
        }),
        _ => None,
    }
}

/// The `Object.registerClass(linkage, a.b.C)` call of an `#initclip`: removed from the tree,
/// its class name returned when there is exactly one.
fn register_class_op(tree: &List, linkage: Option<&str>, found: &std::sync::Mutex<Vec<String>>) {
    let mut remove = Vec::new();
    let mut names = Vec::new();
    for (i, it) in tree.borrow().iter().enumerate() {
        let b = it.borrow();
        let K::ACallMethod { obj, name, args } = &b.k else { continue };
        let K::AGetVar { name: var, .. } = &obj.borrow().k else { continue };
        if direct_text(var).as_deref() != Some("Object") || direct_text(name).as_deref() != Some("registerClass") {
            continue;
        }
        let args = args.borrow();
        if args.len() != 2 {
            continue;
        }
        let Some(first) = direct_text(&args[0]) else { continue };
        if linkage.is_some_and(|l| l != first) {
            continue;
        }
        let Some(cls) = members_to_class_name(&args[1]) else { continue };
        names.push(cls);
        remove.push(i);
    }
    if remove.len() != 1 {
        return;
    }
    found.lock().unwrap().push(names.remove(0));
    tree.borrow_mut().remove(remove[0]);
}

#[derive(Default)]
pub struct ScriptIndex {
    paths: HashMap<usize, Vec<usize>>,
    sources: HashMap<Vec<usize>, Vec<Source>>,
}

impl ScriptIndex {
    pub fn of(swf: &crate::Swf) -> ScriptIndex {
        let mut ix = ScriptIndex::default();
        swf.walk(|path, t| {
            ix.paths.insert(t as *const _ as usize, path.to_vec());
        });
        fn go(nodes: &[crate::action::scripts::Node], out: &mut HashMap<Vec<usize>, Vec<Source>>) {
            for n in nodes {
                if let Some(s) = &n.source {
                    out.entry(s.tag.clone()).or_default().push(s.clone());
                }
                go(&n.children, out);
            }
        }
        go(&crate::action::scripts::tree(swf), &mut ix.sources);
        for v in ix.sources.values_mut() {
            v.sort_by_key(|s| s.index);
        }
        ix
    }

    fn of_tag(&self, key: usize) -> &[Source] {
        self.paths.get(&key).and_then(|p| self.sources.get(p)).map_or(&[], |v| &v[..])
    }
}

impl<'a> Converter<'a> {
    fn script_index(&self) -> &ScriptIndex {
        self.script_ix.get_or_init(|| ScriptIndex::of(self.swf))
    }

    fn sources_of(&self, tag: &crate::tags::Tag) -> Vec<Source> {
        self.script_index().of_tag(tag as *const _ as usize).to_vec()
    }

    fn as2(&self, s: &Source) -> String {
        self.ex.as2_source(s)
    }

    fn as12(&self, tag: &crate::tags::Tag, kind: Kind) -> String {
        self.sources_of(tag).iter().filter(|s| s.kind == kind).map(|s| self.as2(s)).collect()
    }

    pub fn button_script(&self, id: u16) -> String {
        self.char_tag(id).map(|t| self.as12(t, Kind::Button)).unwrap_or_default()
    }

    pub fn button2_scripts(&self, id: u16) -> Option<String> {
        let t = self.char_tag(id)?;
        let s: Vec<Source> = self.sources_of(t).into_iter().filter(|s| s.kind == Kind::ButtonCond).collect();
        if s.is_empty() {
            return None;
        }
        Some(s.iter().map(|s| self.as2(s)).collect())
    }

    pub fn clip_action_scripts(&self, key: usize) -> String {
        let ix = self.script_index();
        ix.of_tag(key).iter().filter(|s| s.kind == Kind::ClipAction).map(|s| self.as2(s)).collect()
    }

    /// The `#initclip` code of a sprite and the class its `Object.registerClass` names.
    pub fn init_clip_script(&self, id: u16, linkage: Option<&str>) -> (String, Option<String>) {
        let found = std::sync::Mutex::new(Vec::new());
        let mut script = String::new();
        for t in &self.swf.timeline.tags {
            if t.code == DO_INIT_ACTION && t.character_id() == Some(id) {
                for s in self.sources_of(t).iter().filter(|s| s.kind == Kind::DoInitAction) {
                    script += &self.ex.as2_source_with(s, &|tree| register_class_op(tree, linkage, &found));
                }
            }
        }
        let found = found.into_inner().unwrap();
        (script, if found.len() == 1 { found.into_iter().next() } else { None })
    }

    pub fn character_pack(&self, id: u16) -> Option<Pack> {
        if !self.as3 {
            return None;
        }
        let cl = self.classes.get(&id)?;
        self.ex.has_as3_class(cl).then(|| Pack { class: cl.clone() })
    }

    pub fn document_pack(&self) -> Option<Pack> {
        if !self.as3 {
            return None;
        }
        let names = crate::names::Names::new(self.swf);
        let cl = names.classes.get(&0).and_then(|v| v.first())?.clone();
        self.ex.has_as3_class(&cl).then_some(Pack { class: cl })
    }

    pub fn accessibility(&self, pack: &Pack) -> Accessibility {
        let (_, items) = self.ex.as3_accessibility(&pack.class);
        Accessibility { items: items.into_iter().map(|i| (i.instance, i.key, i.value, i.start, i.end)).collect() }
    }

    /// The document's accessibility attributes, in the order a Java `HashMap` of them iterates.
    pub fn root_accessibility(&self, pack: &Pack) -> Vec<(String, String)> {
        let (root, _) = self.ex.as3_accessibility(&pack.class);
        let hash = |s: &str| {
            let h = s.encode_utf16().fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32));
            (h ^ ((h as u32) >> 16) as i32) & 15
        };
        let mut v: Vec<(usize, (String, String))> = root.into_iter().enumerate().collect();
        v.sort_by_key(|(i, (k, _))| (hash(k), *i));
        v.into_iter().map(|(_, e)| e).collect()
    }

    /// `convertActionScriptLayer`.
    pub fn convert_script_layer(&mut self, scene_start: i32, init_clip: &str, items: &[Item<'a>], w: &mut XmlWriter, pack: Option<&Pack>) -> bool {
        let mut has = false;
        let mut script = init_clip.to_string();
        if !script.is_empty() {
            script = format!("#initclip\r\n{script}#endinitclip\r\n");
        }
        let mut frame_scripts: BTreeMap<i32, String> = match pack {
            Some(p) => self.ex.as3_frame_scripts(&p.class),
            None => BTreeMap::new(),
        };
        let mut duration = 0;
        let mut frame = 0;
        for it in items {
            if it.code() == DO_ACTION {
                script += &self.as12(it.tag().unwrap(), Kind::DoAction);
            }
            if let Some(s) = frame_scripts.remove(&(scene_start + frame)) {
                script += &s;
            }
            if it.code() == SHOW_FRAME {
                if script.is_empty() {
                    duration += 1;
                } else {
                    if !has {
                        let color = self.random_outline();
                        w.start_with("DOMLayer", &[("name", "Script Layer"), ("color", &color)]);
                        w.start("frames");
                        has = true;
                    }
                    if duration > 0 {
                        w.start_with("DOMFrame", &[("index", &(frame - duration).to_string())]);
                        if duration > 1 {
                            w.attr_i("duration", duration as i64);
                        }
                        w.attr_i("keyMode", KEY_MODE_NORMAL as i64);
                        w.value("elements", "");
                        w.end();
                    }
                    w.start_with("DOMFrame", &[("index", &frame.to_string())]);
                    w.attr_i("keyMode", KEY_MODE_NORMAL as i64);
                    w.start("Actionscript");
                    w.start("script");
                    w.cdata(&script);
                    w.end();
                    w.end();
                    w.value("elements", "");
                    w.end();
                    script.clear();
                    duration = 0;
                }
                frame += 1;
            }
        }
        if has {
            w.end();
            w.end();
        }
        has
    }

    /// `.as` files: AS2 `__Packages` classes, or every AS3 script.
    pub fn export_scripts(&self) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        if self.as3 {
            let (sources, assets) = self.ex.as3_pack_sources(true, false);
            for (name, text) in sources {
                out.push((name, text.into_bytes()));
            }
            out.extend(assets);
            return out;
        }
        let names = crate::names::Names::new(self.swf);
        for t in &self.swf.timeline.tags {
            if t.code != DO_INIT_ACTION {
                continue;
            }
            let Some(id) = t.character_id() else { continue };
            let Some(sprite) = self.char_tag(id) else { continue };
            if sprite.code != DEFINE_SPRITE || sprite.sprite.as_ref().is_some_and(|s| s.tags.iter().any(|t| t.code != END)) {
                continue;
            }
            let data = self.as12(t, Kind::DoInitAction);
            let exp = names.export.get(&id).cloned().unwrap_or_else(|| "_unk_".into());
            let path = exp.strip_prefix("__Packages.").unwrap_or(&exp).replace('.', "/");
            out.push((format!("{path}.as"), data.into_bytes()));
        }
        out
    }
}
