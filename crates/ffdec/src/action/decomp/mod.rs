//! AS1/2 decompiler: a port of FFDec's `ActionGraph` on the shared graph engine.

pub mod class;
pub mod cleaner;
pub mod graph;
pub mod items;
pub mod print;
pub mod translate;
pub mod uninit;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::abc::decomp::ctx::{Config, Ctx, LocalData};
use crate::abc::decomp::graph::G;
use crate::abc::decomp::item::*;
use crate::abc::decomp::ld::Ld;
use crate::abc::decomp::translate::Tr;
use crate::abc::decomp::writer::Writer;
use crate::abc::{Abc, MethodBody};

use super::{Action, ActionList};
use translate::{ASecondPass, ActTr, JMap};

type Key = (u64, u64, u64, usize, usize, String);

/// `ActionSecondPassContext`: second-pass data per graph, collected before rendering.
#[derive(Default)]
struct SpCtx {
    data: RefCell<Vec<(Key, Rc<ASecondPass>)>>,
    collecting: Cell<bool>,
}

thread_local! {
    static SP: RefCell<Option<Rc<SpCtx>>> = const { RefCell::new(None) };
}

fn key_of(tr: &ActTr, start_ip: usize) -> Key {
    let list = &tr.actions[..tr.len];
    let addr = |a: Option<&Action>| a.map_or(u64::MAX, |a| a.address);
    (addr(list.first()), addr(list.get(start_ip)), addr(list.last()), list.len(), start_ip, tr.path.clone())
}

fn fork(tr: &ActTr) -> ActTr {
    let mut t = ActTr::new(tr.actions.clone(), tr.len, tr.version, tr.path.clone(), tr.inside_do_init, tr.inside_function, HashMap::new(), tr.vars.clone(), tr.funcs.clone());
    t.reg_names = tr.reg_names.clone();
    t
}

enum Once {
    Ok(List),
    SecondPass(ASecondPass, List),
}

fn translate_once(tr: &ActTr, start_ip: usize) -> Once {
    let abc = Abc::empty();
    let ctx = Ctx::new(&abc, Config::default());
    let body = MethodBody { method: 0, max_stack: 0, max_regs: 0, init_scope_depth: 0, max_scope_depth: 0, code: vec![], exceptions: vec![], traits: vec![] };
    let dummy = Tr {
        ctx: &ctx,
        body: &body,
        code: tr.code(start_ip),
        method: 0,
        is_static: false,
        script_index: -1,
        class_index: -1,
        local_reg_names: HashMap::new(),
        fqn: vec![],
        swf_version: 0,
        call_stack: vec![],
        path: tr.path.clone(),
    };
    let mut g = G::new(&dummy);
    g.act = Some(tr);
    g.heads = g.make_graph(&[]);
    if g.heads.is_empty() {
        return Once::Ok(empty_list());
    }
    let mut ld = Ld::new(Rc::new(Cell::new(0)));
    match g.as2_translate(&mut ld) {
        Ok(l) => Once::Ok(l),
        Err((d, first)) => Once::SecondPass(d, first),
    }
}

/// `ActionGraph.translateViaGraph`.
pub fn translate_via_graph(mut tr: ActTr, start_ip: usize) -> Result<List, String> {
    let ctx = SP.with(|c| c.borrow().clone());
    if let Some(ctx) = ctx {
        if tr.second_pass.is_none() {
            let key = key_of(&tr, start_ip);
            if ctx.collecting.get() {
                return Ok(match translate_once(&tr, start_ip) {
                    Once::Ok(l) => l,
                    Once::SecondPass(d, first) => {
                        ctx.data.borrow_mut().push((key, Rc::new(d)));
                        first
                    }
                });
            }
            let find = |k: &Key| ctx.data.borrow().iter().find(|(x, _)| x == k).map(|(_, d)| d.clone());
            let mut data = find(&key);
            if data.is_none() {
                let (vb, fb) = (tr.vars.borrow().clone(), tr.funcs.borrow().clone());
                ctx.collecting.set(true);
                let _ = translate_via_graph(fork(&tr), start_ip);
                ctx.collecting.set(false);
                *tr.vars.borrow_mut() = vb;
                *tr.funcs.borrow_mut() = fb;
                data = find(&key);
            }
            tr.second_pass = data;
        }
    }
    match translate_once(&tr, start_ip) {
        Once::Ok(l) => Ok(l),
        Once::SecondPass(d, first) => {
            if tr.second_pass.is_some() {
                return Ok(first);
            }
            tr.second_pass = Some(Rc::new(d));
            match translate_once(&tr, start_ip) {
                Once::Ok(l) => Ok(l),
                Once::SecondPass(_, f) => Ok(f),
            }
        }
    }
}

/// `Action.actionsToTree` for a whole action record.
pub fn actions_to_tree(list: &ActionList, inside_do_init: bool, path: &str, uninit: &Rc<uninit::Uninitialized>) -> Result<List, String> {
    let list = &cleaner::clean(list);
    let actions = Rc::new(list.actions.clone());
    let len = actions.len();
    let vars = Rc::new(RefCell::new(JMap::default()));
    let funcs = Rc::new(RefCell::new(JMap::default()));
    let ctx = Rc::new(SpCtx::default());
    ctx.collecting.set(true);
    SP.with(|c| *c.borrow_mut() = Some(ctx.clone()));
    let mk = || {
        let mut t = ActTr::new(actions.clone(), len, list.version, path.to_string(), inside_do_init, false, HashMap::new(), vars.clone(), funcs.clone());
        t.uninit = uninit.clone();
        t
    };
    let _ = translate_via_graph(mk(), 0);
    *vars.borrow_mut() = JMap::default();
    *funcs.borrow_mut() = JMap::default();
    ctx.collecting.set(false);
    let r = translate_via_graph(mk(), 0);
    SP.with(|c| *c.borrow_mut() = None);
    r
}

/// `Helper.appendTimeoutCommentAs2`.
fn timeout_comment(w: &mut Writer, actions: usize) {
    w.append_no_hilight("/*").new_line();
    w.append_no_hilight(" * Decompilation error").new_line();
    w.append_no_hilight(" * Timeout (1 minute) was reached").new_line();
    if actions > 0 {
        w.append_no_hilight(&format!(" * Action count: {actions}")).new_line();
    }
    w.append_no_hilight(" */").new_line();
    w.append_no_hilight("throw new Error(\"Not decompiled due to timeout\");").new_line();
}

fn error_comment(w: &mut Writer, kind: &str) {
    w.append_no_hilight("/*").new_line();
    w.append_no_hilight(" * Decompilation error").new_line();
    w.append_no_hilight(" * Code may be obfuscated").new_line();
    w.append_no_hilight(&format!(" * Error type: {kind}")).new_line();
    w.append_no_hilight(" */").new_line();
    w.append_no_hilight("throw new flash.errors.IllegalOperationError(\"Not decompiled due to error\");").new_line();
}

/// `Action.actionsToSource`: decompiled ActionScript of an action record, wrapped in its
/// `on(...)`/`onClipEvent(...)` header when it has one.
pub fn actions_to_source(list: &ActionList, inside_do_init: bool, header: Option<&str>, path: &str, uninit: &uninit::Uninitialized) -> String {
    crate::abc::decomp::with_big_stack(|| {
        let uninit = Rc::new(uninit.clone());
        let tree = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::abc::decomp::method::with_deadline(|| actions_to_tree(list, inside_do_init, path, &uninit))));
        let tree = match tree {
            Ok(Some(t)) => Ok(t),
            Ok(None) => {
                SP.with(|c| *c.borrow_mut() = None);
                let mut w = Writer::new();
                timeout_comment(&mut w, list.actions.len());
                return w.finish();
            }
            Err(e) => {
                SP.with(|c| *c.borrow_mut() = None);
                Err(e)
            }
        };
        let abc = Abc::empty();
        let ctx = Ctx::new(&abc, Config::default());
        let mut ld = LocalData::new(&ctx);
        ld.as2 = true;
        let mut w = Writer::new();
        if let Some(h) = header {
            w.append_no_hilight(h);
            w.append_no_hilight("{").new_line();
            w.indent();
        }
        match tree {
            Ok(Ok(tree)) => {
                let items: Vec<It> = tree.borrow().clone();
                let printed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::abc::decomp::print::graph_to_string(&items, &mut Writer::nul(), &ld);
                    let mut w2 = std::mem::replace(&mut w, Writer::new());
                    crate::abc::decomp::print::graph_to_string(&items, &mut w2, &ld);
                    w2
                }));
                match printed {
                    Ok(w2) => w = w2,
                    Err(_) => error_comment(&mut w, "Exception"),
                }
            }
            Ok(Err(e)) => error_comment(&mut w, &e),
            Err(_) => error_comment(&mut w, "Exception"),
        }
        if header.is_some() {
            w.unindent();
            w.append_no_hilight("}").new_line();
        }
        w.finish()
    })
}

/// Whether a source is an AS2 class initializer (`needsCalculatingAS2UninitializeClassTraits`).
pub fn is_class_source(swf: &crate::Swf, names: &crate::names::Names, s: &super::scripts::Source) -> bool {
    if s.kind != super::scripts::Kind::DoInitAction {
        return false;
    }
    let id = swf.tag(&s.tag).and_then(|t| t.character_id());
    id.and_then(|id| names.export.get(&id)).is_some_and(|e| e.starts_with("__Packages."))
}

/// `SWF.calculateAs2UninitializedClassTraits`: decompiles every script once to find the fields
/// AS2 classes use without declaring.
pub fn swf_uninitialized(swf: &crate::Swf) -> uninit::Uninitialized {
    let names = crate::names::Names::new(swf);
    let sources: Vec<super::scripts::Source> = super::scripts::export_paths(&super::scripts::tree(swf)).into_iter().map(|(_, s)| s).collect();
    if !sources.iter().any(|s| is_class_source(swf, &names, s)) {
        return Default::default();
    }
    let lists: Vec<(bool, bool, ActionList)> = sources.iter().map(|s| (is_class_source(swf, &names, s), s.kind == super::scripts::Kind::DoInitAction, s.actions(swf))).collect();
    crate::abc::decomp::with_big_stack(|| {
        let empty = Rc::new(uninit::Uninitialized::default());
        let (mut classes, mut others) = (Vec::new(), Vec::new());
        for (is_class, init, list) in &lists {
            let tree = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| actions_to_tree(list, *init, "", &empty))).ok().and_then(|r| r.ok()).unwrap_or_else(empty_list);
            if *is_class {
                classes.push(tree);
            } else {
                others.push(tree);
            }
        }
        uninit::detect(&classes, &others)
    })
}

/// Decompiled source of one script, with the class fields `uninit` found for the SWF.
pub fn source_text(swf: &crate::Swf, s: &super::scripts::Source, uninit: &uninit::Uninitialized) -> String {
    let names = crate::names::Names::new(swf);
    let empty = uninit::Uninitialized::default();
    let u = if is_class_source(swf, &names, s) { uninit } else { &empty };
    actions_to_source(&s.actions(swf), s.kind == super::scripts::Kind::DoInitAction, s.header.as_deref(), "", u)
}
