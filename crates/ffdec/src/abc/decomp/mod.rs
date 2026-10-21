//! AS3 decompiler: a port of FFDec's AVM2 graph decompiler.

pub mod avm2graph;
pub mod avm2post;
pub mod code;
pub mod ctx;
pub mod deobf;
pub mod ecma;
pub mod eq;
pub mod graph;
pub mod index;
pub mod ld;
pub mod item;
pub mod loops;
pub mod method;
pub mod names;
pub mod parts;
pub mod post;
pub mod print;
pub mod props;
pub mod script;
pub mod stack;
pub mod structure;
pub mod translate;
pub mod values;
pub mod writer;

use std::sync::Arc;

use crate::abc::scripts::Entry;
use crate::abc::Abc;

/// Type information for a SWF's ABCs; build once and reuse for every script.
pub fn index_for(abcs: &[&Abc]) -> Arc<index::AbcIndex> {
    index::AbcIndex::for_abcs(abcs)
}

/// What the decompiler needs to know about the SWF around an ABC.
#[derive(Debug, Clone, Default)]
pub struct SwfContext {
    pub version: u8,
    pub doc: Option<ctx::DocClass>,
}

impl SwfContext {
    pub fn of(swf: &crate::Swf) -> SwfContext {
        let names = crate::names::Names::new(swf);
        let doc = names.classes.get(&0).and_then(|v| v.first()).map(|name| {
            let r = &swf.frame_rect;
            let bg = swf.timeline.tags.iter().find(|t| t.code == crate::tags::code::SET_BACKGROUND_COLOR).and_then(|t| crate::types::Rgba::read_rgb(&mut crate::io::Reader::new(&t.body)).ok()).map(|c| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b));
            ctx::DocClass {
                name: name.clone(),
                width: ((r.x_max - r.x_min) as f64 / 20.0).round() as i64,
                height: ((r.y_max - r.y_min) as f64 / 20.0).round() as i64,
                background: bg,
                frame_rate: swf.frame_rate,
            }
        });
        SwfContext { version: swf.header.version, doc }
    }
}

/// Decompiled ActionScript of one script entry.
pub fn entry_source(index: &Arc<index::AbcIndex>, abc: &Abc, e: &Entry, swf: &SwfContext) -> String {
    entry_source_with(index, abc, e, swf, ctx::Config::default())
}

/// `entry_source` with explicit decompiler settings.
pub fn entry_source_with(index: &Arc<index::AbcIndex>, abc: &Abc, e: &Entry, swf: &SwfContext, cfg: ctx::Config) -> String {
    let r = with_big_stack(|| {
        let mut ctx = ctx::Ctx::new(abc, cfg).with_index(index.clone());
        ctx.swf_version = swf.version;
        ctx.doc = swf.doc.clone();
        let pack = script::pack_of(&ctx, e.script, e.trait_index);
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| script::pack_source(&ctx, &pack))).ok()
    });
    r.unwrap_or_else(|| "/*\r\n * Decompilation error\r\n */\r\n".into())
}

/// The names an entry's decompiled text uses for identifiers that are not valid ActionScript,
/// as (printed name, name in the ABC).
pub fn entry_renames(index: &Arc<index::AbcIndex>, abc: &Abc, e: &Entry, swf: &SwfContext) -> Vec<(String, String)> {
    with_big_stack(|| {
        let mut ctx = ctx::Ctx::new(abc, ctx::Config::default()).with_index(index.clone());
        ctx.swf_version = swf.version;
        ctx.doc = swf.doc.clone();
        let pack = script::pack_of(&ctx, e.script, e.trait_index);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| script::pack_source(&ctx, &pack)));
        ctx.ids.renamed().into_iter().map(|(raw, printed)| (printed, raw)).collect()
    })
}

/// Every script pack's class path, with its script's dependencies when `deps` is set.
pub fn script_packs(index: &Arc<index::AbcIndex>, abc: &Abc, deps: bool) -> Vec<(names::Dotted, Vec<(names::Dotted, u8)>)> {
    let ctx = ctx::Ctx::new(abc, ctx::Config::default()).with_index(index.clone());
    script::swc_packs(&ctx, deps)
}

/// Runs `f` on a thread with a large stack: the structuring code recurses once per nested block.
pub fn with_big_stack<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|s| std::thread::Builder::new().stack_size(1 << 30).spawn_scoped(s, f).expect("decompiler thread").join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
}

/// The script and class index of the public class named `class_name` (raw dotted name).
pub fn find_class(abc: &Abc, class_name: &str) -> Option<(usize, usize)> {
    let ctx = ctx::Ctx::new(abc, ctx::Config::default());
    let names = ctx.names();
    for (s, script) in abc.scripts.iter().enumerate() {
        for t in &script.traits {
            if let crate::abc::TraitData::Class { class, .. } = t.data {
                if names.name_with_ns(abc.instances[class].name, false).raw() == class_name {
                    return Some((s, class));
                }
            }
        }
    }
    None
}

fn call_name(it: &item::It) -> Option<(String, item::List)> {
    let b = it.borrow();
    let item::K::CallProperty { prop, args, .. } = &b.k else { return None };
    let p = prop.borrow();
    let item::K::FullMultiname { resolved: Some(r), .. } = &p.k else { return None };
    Some((r.clone(), args.clone()))
}

fn is_acc_call(name: &str) -> bool {
    name.starts_with("__setTab_") || name.starts_with("__setAcc_")
}

/// `getFrameScriptsFromPack`: the bodies of the methods the class constructor registers with
/// `addFrameScript`, by zero-based frame.
pub fn frame_scripts(index: &Arc<index::AbcIndex>, abc: &Abc, script: usize, class: usize, swf: &SwfContext) -> std::collections::BTreeMap<i32, String> {
    with_big_stack(|| {
        let mut ctx = ctx::Ctx::new(abc, ctx::Config::default()).with_index(index.clone());
        ctx.swf_version = swf.version;
        ctx.doc = swf.doc.clone();
        let mut out = std::collections::BTreeMap::new();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let names = ctx.names();
            let inst = &abc.instances[class];
            let mut req = method::MethodReq::new(inst.iinit);
            req.script_index = script as i32;
            req.class_index = class as i32;
            req.initializer = method::TRAIT_INSTANCE_INITIALIZER;
            let mut cd = method::ConvertData::default();
            let Ok(conv) = method::convert(&ctx, &req, &mut cd, &Default::default()) else { return };
            let mut frames: Vec<(i32, String)> = Vec::new();
            for it in conv.items.borrow().iter() {
                let Some((name, args)) = call_name(it) else { continue };
                if name != "addFrameScript" {
                    continue;
                }
                let args = args.borrow();
                let mut i = 0;
                while i + 1 < args.len() {
                    if let item::K::IntegerValueA { v, .. } = &args[i].borrow().k {
                        let target = args[i + 1].borrow();
                        match &target.k {
                            item::K::GetLex { mn, .. } => frames.push((*v, names.name(*mn, &[], true, false))),
                            item::K::GetProperty { obj, prop, .. } => {
                                if matches!(obj.borrow().k, item::K::This { .. }) {
                                    if let item::K::FullMultiname { mn, .. } = &prop.borrow().k {
                                        frames.push((*v, names.name(*mn, &[], true, false)));
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    i += 2;
                }
            }
            let mut methods: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for t in &inst.traits {
                if let crate::abc::TraitData::Method { method, .. } | crate::abc::TraitData::Getter { method, .. } | crate::abc::TraitData::Setter { method, .. } = t.data {
                    methods.insert(names.name(t.name, &[], true, false), method);
                }
            }
            for (frame, trait_name) in frames {
                let Some(&m) = methods.get(&trait_name) else { continue };
                let mut req = method::MethodReq::new(m);
                req.script_index = script as i32;
                req.class_index = class as i32;
                let mut cd = method::ConvertData::default();
                let Ok(conv) = method::convert(&ctx, &req, &mut cd, &Default::default()) else { continue };
                conv.items.borrow_mut().retain(|it| !call_name(it).is_some_and(|(n, _)| is_acc_call(&n)));
                let mut w = writer::Writer::new();
                method::to_source(&ctx, &req, &conv, &Default::default(), class as i32, &mut w);
                out.insert(frame, w.finish());
            }
        }));
        out
    })
}

/// One accessibility attribute of an instance over a frame range (`AccessibilityItem`).
#[derive(Debug, Clone)]
pub struct AccItem {
    pub instance: String,
    pub key: String,
    pub value: String,
    pub start: i32,
    pub end: i32,
}

fn resolved(it: &item::It) -> Option<String> {
    match &it.borrow().k {
        item::K::FullMultiname { resolved, .. } => resolved.clone(),
        _ => None,
    }
}

fn is_this(it: &item::It) -> bool {
    matches!(it.borrow().k, item::K::This { .. })
}

/// `obj.prop` with a resolved name, as (object, name).
fn get_prop(it: &item::It) -> Option<(item::It, Option<String>)> {
    match &it.borrow().k {
        item::K::GetProperty { obj, prop, .. } => Some((obj.clone(), resolved(prop))),
        _ => None,
    }
}

fn int_value(it: &item::It) -> Option<i32> {
    match &it.borrow().k {
        item::K::IntegerValueA { v, .. } => Some(*v),
        _ => None,
    }
}

/// The attribute a `SetProperty` statement assigns: `this.x.tabIndex = n`, or
/// `this.x.accessibilityProperties.p = v` (`this.root` for the document with `root`).
fn acc_assignment(it: &item::It, root: bool) -> Option<(String, String, String)> {
    let b = it.borrow();
    let item::K::SetProperty { obj, prop, .. } = &b.k else { return None };
    let value = b.value.clone()?;
    if let item::K::ConstructProp { prop: cp, .. } = &value.borrow().k {
        if resolved(cp).as_deref() == Some("AccessibilityProperties") {
            return None;
        }
    }
    let name = resolved(prop)?;
    if name == "tabIndex" && !root {
        let (o, inst) = get_prop(obj)?;
        if is_this(&o) {
            if let (Some(inst), Some(v)) = (inst, int_value(&value)) {
                return Some((inst, "tabIndex".into(), v.to_string()));
            }
        }
        return None;
    }
    let (o, parent) = get_prop(obj)?;
    if parent.as_deref() != Some("accessibilityProperties") {
        return None;
    }
    let (o2, inst) = get_prop(&o)?;
    let inst = inst?;
    if !is_this(&o2) || (root && inst != "root") {
        return None;
    }
    if !["name", "description", "forceSimple", "noAutoLabeling", "shortcut"].contains(&name.as_str()) {
        return None;
    }
    let invert = name == "noAutoLabeling";
    let key = match name.as_str() {
        "noAutoLabeling" => "autoLabeling".to_string(),
        "name" => "accName".to_string(),
        n => n.to_string(),
    };
    let v = match &value.borrow().k {
        item::K::Str(s) => s.clone(),
        item::K::True => if invert { "false" } else { "true" }.to_string(),
        item::K::False => if invert { "true" } else { "false" }.to_string(),
        _ => String::new(),
    };
    Some((inst, key, v))
}

fn bin(it: &item::It, op: item::Bin) -> Option<(item::It, item::It)> {
    match &it.borrow().k {
        item::K::Bin { op: o, l, r } if *o == op => Some((l.clone(), r.clone())),
        _ => None,
    }
}

/// `getAccessibilityFromPack` and `getRootAccessibilityFromPack`: the attributes the
/// class's generated `__setAcc_`/`__setTab_` methods assign, and the document's own.
pub fn accessibility(index: &Arc<index::AbcIndex>, abc: &Abc, script: usize, class: usize, swf: &SwfContext) -> (Vec<(String, String)>, Vec<AccItem>) {
    with_big_stack(|| {
        let mut ctx = ctx::Ctx::new(abc, ctx::Config::default()).with_index(index.clone());
        ctx.swf_version = swf.version;
        ctx.doc = swf.doc.clone();
        let mut root: Vec<(String, String)> = Vec::new();
        let mut items: Vec<AccItem> = Vec::new();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let names = ctx.names();
            let inst = &abc.instances[class];
            let convert = |m: usize, init: bool| -> Option<item::List> {
                let mut req = method::MethodReq::new(m);
                req.script_index = script as i32;
                req.class_index = class as i32;
                if init {
                    req.initializer = method::TRAIT_INSTANCE_INITIALIZER;
                }
                method::convert(&ctx, &req, &mut method::ConvertData::default(), &Default::default()).ok().map(|c| c.items)
            };
            let Some(ctor) = convert(inst.iinit, true) else { return };
            let mut all_frames: Vec<String> = Vec::new();
            let mut frame_traits: Vec<String> = Vec::new();
            for it in ctor.borrow().iter() {
                if let Some((_, key, v)) = acc_assignment(it, true) {
                    match root.iter_mut().find(|e| e.0 == key) {
                        Some(e) => e.1 = v,
                        None => root.push((key, v)),
                    }
                }
                let Some((name, args)) = call_name(it) else { continue };
                if is_acc_call(&name) && args.borrow().is_empty() {
                    all_frames.push(name.clone());
                }
                if name == "addFrameScript" {
                    let args = args.borrow();
                    let mut i = 0;
                    while i + 1 < args.len() {
                        if int_value(&args[i]).is_some() {
                            match &args[i + 1].borrow().k {
                                item::K::GetLex { mn, .. } => frame_traits.push(names.name(*mn, &[], true, false)),
                                item::K::GetProperty { obj, prop, .. } if is_this(obj) => {
                                    if let item::K::FullMultiname { mn, .. } = &prop.borrow().k {
                                        frame_traits.push(names.name(*mn, &[], true, false));
                                    }
                                }
                                _ => {}
                            }
                        }
                        i += 2;
                    }
                }
            }
            let methods: Vec<(String, usize)> = inst
                .traits
                .iter()
                .filter_map(|t| match t.data {
                    crate::abc::TraitData::Method { method, .. } | crate::abc::TraitData::Getter { method, .. } | crate::abc::TraitData::Setter { method, .. } => Some((names.name(t.name, &[], true, false), method)),
                    _ => None,
                })
                .collect();
            let mut frame_acc: Vec<String> = Vec::new();
            let mut range_acc: Vec<String> = Vec::new();
            for (name, m) in &methods {
                if name == "__setTab_handler" || name == "__setAcc_handler" {
                    if abc.methods[*m].param_types.len() == 1 {
                        if let Some(body) = convert(*m, false) {
                            for it in body.borrow().iter() {
                                if let Some((n, a)) = call_name(it) {
                                    if is_acc_call(&n) && a.borrow().len() == 1 {
                                        range_acc.push(n);
                                    }
                                }
                            }
                        }
                    }
                }
                if frame_traits.contains(name) {
                    if let Some(body) = convert(*m, false) {
                        for it in body.borrow().iter() {
                            if let Some((n, a)) = call_name(it) {
                                if is_acc_call(&n) && a.borrow().is_empty() {
                                    frame_acc.push(n);
                                }
                            }
                        }
                    }
                }
            }
            for (name, m) in &methods {
                if frame_acc.contains(name) {
                    if let Some(body) = convert(*m, false) {
                        let first = body.borrow().first().cloned();
                        if let Some(first) = first {
                            if let item::K::If { expr, on_true, .. } = &first.borrow().k {
                                let frame = bin(expr, item::Bin::Or).and_then(|(_, r)| bin(&r, item::Bin::Neq)).and_then(|(_, r)| int_value(&r));
                                if let Some(frame) = frame {
                                    for it in on_true.borrow().iter() {
                                        if let Some((i, k, v)) = acc_assignment(it, false) {
                                            items.push(AccItem { instance: i, key: k, value: v, start: frame, end: frame });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if range_acc.contains(name) {
                    if let Some(body) = convert(*m, false) {
                        let first = body.borrow().first().cloned();
                        if let Some(first) = first {
                            if let item::K::If { expr, on_true, .. } = &first.borrow().k {
                                let range = (|| {
                                    let (l1, _) = bin(expr, item::Bin::And)?;
                                    let (l2, le) = bin(&l1, item::Bin::And)?;
                                    let (_, ge) = bin(&l2, item::Bin::And)?;
                                    let start = int_value(&bin(&ge, item::Bin::Ge)?.1)?;
                                    let end = int_value(&bin(&le, item::Bin::Le)?.1)?;
                                    Some((start, end))
                                })();
                                if let Some((start, end)) = range {
                                    for it in on_true.borrow().iter() {
                                        if let Some((i, k, v)) = acc_assignment(it, false) {
                                            items.push(AccItem { instance: i, key: k, value: v, start, end });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for (name, m) in &methods {
                if all_frames.contains(name) {
                    if let Some(body) = convert(*m, false) {
                        for it in body.borrow().iter() {
                            if let Some((i, k, v)) = acc_assignment(it, false) {
                                items.push(AccItem { instance: i, key: k, value: v, start: 1, end: i32::MAX });
                            }
                        }
                    }
                }
            }
        }));
        (root, items)
    })
}
