//! Method body decompilation: FFDec's `MethodBody.convert`/`toString` and `AVM2Code.toGraphTargetItems`.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::code::{op, Code};
use super::ctx::{Ctx, LocalData};
use super::graph::G;
use super::index;
use super::item::*;
use super::ld::{shared, Ld, SecondPass, Shared};
use super::names::Dotted;
use super::print;
use super::props::*;
use super::translate::Tr;
use super::writer::Writer;
use crate::abc::{method_flags, Multiname, Trait, TraitData};

pub const TRAIT_INSTANCE_INITIALIZER: i32 = -1;
pub const TRAIT_CLASS_INITIALIZER: i32 = -2;
pub const TRAIT_SCRIPT_INITIALIZER: i32 = -3;

/// A value assigned to a slot trait by an initializer (`AssignedValue`).
#[derive(Clone)]
pub struct Assigned {
    pub command: Option<It>,
    pub value: It,
    pub initializer: i32,
    pub method: usize,
}

/// FFDec's `ConvertData`: initializer values keyed by trait identity.
#[derive(Default, Clone)]
pub struct ConvertData {
    pub assigned: Vec<(*const Trait, Assigned)>,
    pub this_has_default_to_primitive: bool,
}

impl ConvertData {
    pub fn get(&self, t: &Trait) -> Option<&Assigned> {
        self.assigned.iter().find(|(p, _)| std::ptr::eq(*p, t)).map(|(_, a)| a)
    }

    fn contains(&self, t: &Trait) -> bool {
        self.get(t).is_some()
    }
}

/// FFDec's `decompilationTimeoutSingleMethod`, in seconds.
pub const TIMEOUT_SECS: u64 = 60;

thread_local! {
    static DEADLINE: Cell<Option<std::time::Instant>> = const { Cell::new(None) };
}

/// Panic payload for a method that ran past its time limit.
struct Timeout;

const TIMEOUT_ERROR: &str = "\u{0}timeout";

/// Aborts the current conversion once its time limit has passed.
pub fn check_deadline() {
    if let Some(d) = DEADLINE.get() {
        if std::time::Instant::now() > d {
            std::panic::resume_unwind(Box::new(Timeout));
        }
    }
}

/// What to decompile and in which context.
#[derive(Clone)]
pub struct MethodReq<'x> {
    pub method: usize,
    pub is_static: bool,
    pub script_index: i32,
    pub class_index: i32,
    pub init_traits: Option<&'x [Trait]>,
    pub init_trait_classes: Vec<usize>,
    pub initializer: i32,
    pub fqn: Vec<Dotted>,
    pub call_stack: Vec<usize>,
    pub scope: Vec<It>,
    pub path: String,
}

impl<'x> MethodReq<'x> {
    pub fn new(method: usize) -> Self {
        MethodReq { method, is_static: false, script_index: -1, class_index: -1, init_traits: None, init_trait_classes: Vec::new(), initializer: 0, fqn: Vec::new(), call_stack: vec![method], scope: Vec::new(), path: String::new() }
    }
}

/// A converted body ready to print.
pub struct Converted {
    pub items: List,
    pub local_reg_names: HashMap<i32, String>,
}

/// `MethodBody.getLocalRegNames`.
pub fn local_reg_names(ctx: &Ctx, method: usize, code: &Code, max_regs: usize) -> HashMap<i32, String> {
    let abc = ctx.abc;
    let mi = &abc.methods[method];
    let mut ret = HashMap::new();
    for i in 1..=mi.param_types.len() {
        let mut name = format!("param{i}");
        if mi.flags & method_flags::HAS_PARAM_NAMES != 0 && ctx.cfg.param_names {
            name = abc.str(mi.param_names.get(i - 1).copied().unwrap_or(0)).to_string();
        }
        ret.insert(i as i32, name);
    }
    let mut pos = mi.param_types.len() as i32 + 1;
    if mi.flags & method_flags::NEED_ARGUMENTS != 0 {
        ret.insert(pos, "arguments".into());
        pos += 1;
    }
    if mi.flags & method_flags::NEED_REST != 0 {
        ret.insert(pos, "rest".into());
    }
    if ctx.cfg.local_names_from_debug {
        let fmt = |r: i32| ctx.cfg.reg_name(r);
        for (k, v) in code.local_reg_names_from_debug(abc, max_regs, &fmt) {
            ret.insert(k, v);
        }
    }
    ret
}

/// `convertMethodBody` without deobfuscation: the prepared instruction list.
pub fn prepare_code(ctx: &Ctx, body: &crate::abc::MethodBody) -> Code {
    let mut code = Code::new(ctx.abc, body);
    code.fix_jumps();
    code.push_false_if_false();
    code.calculate_debug_file_line(ctx.abc);
    code
}

/// `MethodBody.convert`: decompiles to items, then prints once to a null writer like FFDec.
pub fn convert(ctx: &Ctx, req: &MethodReq, cd: &mut ConvertData, seen: &HashSet<usize>) -> Result<Converted, String> {
    convert_into(ctx, req, cd, seen, &mut Writer::nul())
}

/// `convert` printing into a given null writer, so callers can tell whether anything was printed.
pub fn convert_into(ctx: &Ctx, req: &MethodReq, cd: &mut ConvertData, seen: &HashSet<usize>, w: &mut Writer) -> Result<Converted, String> {
    let outer = DEADLINE.get();
    if outer.is_none() {
        DEADLINE.set(Some(std::time::Instant::now() + std::time::Duration::from_secs(TIMEOUT_SECS)));
    }
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| convert_inner(ctx, req, cd, seen, w)));
    DEADLINE.set(outer);
    match r {
        Ok(v) => v,
        Err(e) if e.is::<Timeout>() => {
            if outer.is_some() {
                std::panic::resume_unwind(e);
            }
            let n = ctx.abc.body(req.method).map_or(0, |b| prepare_code(ctx, b).len());
            Err(format!("{TIMEOUT_ERROR}{n}"))
        }
        Err(e) => Err(e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "decompilation failed".into())),
    }
}

fn convert_inner(ctx: &Ctx, req: &MethodReq, cd: &mut ConvertData, seen: &HashSet<usize>, w: &mut Writer) -> Result<Converted, String> {
    let abc = ctx.abc;
    let bi = abc.body_of.get(req.method).copied().flatten().ok_or("method has no body")?;
    let body = &abc.bodies[bi];
    let code = prepare_code(ctx, body);
    let names = local_reg_names(ctx, req.method, &code, body.max_regs);
    let items = to_graph_target_items(ctx, req, cd, &code, &names);
    let mut seen = seen.clone();
    seen.insert(req.method);
    let mut ld = LocalData::new(ctx);
    ld.local_reg_names = names.clone();
    ld.fqn = req.fqn.clone();
    ld.seen_methods = seen;
    ld.class_index = req.class_index;
    ld.call_stack = req.call_stack.clone();
    let v: Vec<It> = items.borrow().clone();
    print::graph_to_string(&v, w, &ld);
    Ok(Converted { items, local_reg_names: names })
}

/// `MethodBody.toString`: prints converted items.
pub fn to_source(ctx: &Ctx, req: &MethodReq, conv: &Converted, seen: &HashSet<usize>, class_index: i32, w: &mut Writer) {
    let abc = ctx.abc;
    let mut seen = seen.clone();
    seen.insert(req.method);
    let mut fqn2 = req.fqn.clone();
    if let Some(body) = abc.body(req.method) {
        for t in &body.traits {
            let tn = Dotted::parse_with_suffix(&ctx.names().name(t.name, &[], false, true));
            if let Some(p) = fqn2.iter().position(|x| *x == tn) {
                fqn2.remove(p);
            }
        }
    }
    let mut ld = LocalData::new(ctx);
    ld.local_reg_names = conv.local_reg_names.clone();
    ld.fqn = fqn2;
    ld.seen_methods = seen;
    ld.class_index = class_index;
    ld.call_stack = req.call_stack.clone();
    let v: Vec<It> = conv.items.borrow().clone();
    print::graph_to_string(&v, w, &ld);
}

/// Convert and print a body in one go, with FFDec's error comment on failure.
pub fn decompile_to_writer(ctx: &Ctx, req: &MethodReq, cd: &mut ConvertData, seen: &HashSet<usize>, class_index: i32, w: &mut Writer) {
    match convert(ctx, req, cd, seen) {
        Ok(conv) => {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| to_source(ctx, req, &conv, seen, class_index, w)));
            if r.is_err() {
                error_comment(w, "printing failed");
            }
        }
        Err(e) => error_comment(w, &e),
    }
}

pub fn error_comment(w: &mut Writer, msg: &str) {
    if let Some(n) = msg.strip_prefix(TIMEOUT_ERROR) {
        w.append_no_hilight("/*").new_line();
        w.append_no_hilight(" * Decompilation error").new_line();
        w.append_no_hilight(" * Timeout (1 minute) was reached").new_line();
        if n != "0" {
            w.append_no_hilight(&format!(" * Instruction count: {n}")).new_line();
        }
        w.append_no_hilight(" */").new_line();
        w.append_no_hilight("throw new flash.errors.IllegalOperationError(\"Not decompiled due to timeout\");").new_line();
        return;
    }
    w.append("/*").new_line();
    w.append(" * Decompilation error").new_line();
    w.append(" * Code may be obfuscated").new_line();
    w.append(" * Tip: You can try enabling \"Automatic deobfuscation\" in Settings").new_line();
    w.append(" * Error type: ").append(msg).new_line();
    w.append(" */").new_line();
    w.append("throw new flash.errors.IllegalOperationError(\"Not decompiled due to error\");").new_line();
}

fn register_count(code: &Code) -> usize {
    let mut max = -1;
    for ins in &code.ins {
        if let Some(r) = ins.set_local_reg().or(ins.get_local_reg()) {
            max = max.max(r);
        }
    }
    (max + 1) as usize
}

/// `AVM2Graph.translateViaGraph`, run twice like FFDec's `SecondPassException` retry.
#[allow(clippy::too_many_arguments)]
fn translate_via_graph(ctx: &Ctx, req: &MethodReq, code: &Code, names: &HashMap<i32, String>, regs: &Shared<HashMap<i32, It>>, types: &Shared<HashMap<i32, It>>, assign: &Shared<HashMap<i32, i32>>, second: Option<SecondPass>, cd: &ConvertData) -> Result<List, SecondPass> {
    let abc = ctx.abc;
    let body = abc.body(req.method).unwrap();
    let tr = Tr {
        ctx,
        body,
        code: code.clone(),
        method: req.method,
        is_static: req.is_static,
        script_index: req.script_index,
        class_index: req.class_index,
        local_reg_names: names.clone(),
        fqn: req.fqn.clone(),
        swf_version: ctx.swf_version,
        call_stack: req.call_stack.clone(),
        path: req.path.clone(),
    };
    let mut g = G::new(&tr);
    let mut alternate = Vec::new();
    for e in &code.exceptions {
        alternate.push(code.adr2pos_nearest(e.start as i64).map_or(-1, |p| p as i32));
        alternate.push(code.adr2pos(e.target as i64).map_or(-1, |p| p as i32));
    }
    g.heads = g.make_graph(&alternate);
    let mut ld = Ld::new(Rc::new(Cell::new(0)));
    ld.second_pass = second.map(Rc::new);
    ld.this_has_default_to_primitive = cd.this_has_default_to_primitive;
    ld.local_regs = regs.clone();
    ld.local_reg_types = types.clone();
    ld.local_reg_assignment_ips = assign.clone();
    ld.scope_stack.borrow_mut().items = req.scope.clone();
    g.translate_pass(&mut ld, 0)
}

/// `AVM2Code.toGraphTargetItems`.
pub fn to_graph_target_items(ctx: &Ctx, req: &MethodReq, cd: &mut ConvertData, code: &Code, names: &HashMap<i32, String>) -> List {
    let abc = ctx.abc;
    let mi = &abc.methods[req.method];
    let reg_count = register_count(code);
    let regs = shared(HashMap::new());
    if reg_count > 0 {
        regs.borrow_mut().insert(0, undefined(None, None));
    }
    let types = shared(HashMap::new());
    for (i, &pt) in mi.param_types.iter().enumerate() {
        types.borrow_mut().insert(i as i32 + 1, index::mn_type(ctx, pt));
    }
    let assign = shared(HashMap::new());
    let list = match translate_via_graph(ctx, req, code, names, &regs, &types, &assign, None, cd) {
        Ok(l) => l,
        Err(spd) => match translate_via_graph(ctx, req, code, names, &regs, &types, &assign, Some(spd), cd) {
            Ok(l) => l,
            Err(_) => empty_list(),
        },
    };
    if let Some(init) = req.init_traits {
        if init_traits(ctx, req, cd, &list, init) {
            return list;
        }
    }
    let mut d: Vec<Option<It>> = vec![None; reg_count];
    let mut r = 1usize;
    let decl = |r: usize, t: It| {
        let sl = mk(K::SetLocal { reg: r as i32, set: SetInfo::default(), ty: t.clone(), hide_value: false, caused_by_dup: false, directly_caused_by_dup: false }, None, None, prec::ASSIGNMENT, Some(null(None, None)));
        mk(K::Declaration { assignment: sl, ty: Some(t), type_is_null: false, show_value: true }, None, None, prec::ASSIGNMENT, None)
    };
    for &pt in &mi.param_types {
        let t = if pt == 0 { unbounded() } else { ty_dotted(ctx.names().name_with_ns(pt, true)) };
        if d.len() > r {
            d[r] = Some(decl(r, t));
        }
        r += 1;
    }
    if mi.flags & method_flags::NEED_ARGUMENTS != 0 {
        if d.len() > r {
            d[r] = Some(decl(r, ty("Array")));
        }
        r += 1;
    }
    if mi.flags & method_flags::NEED_REST != 0 {
        if d.len() > r {
            d[r] = Some(decl(r, ty("Array")));
        }
        r += 1;
    }
    let mut ldn = LocalData::new(ctx);
    ldn.local_reg_names = names.clone();
    let param_names: Vec<String> = (0..r).map(|ir| ldn.reg_name(ir as i32)).collect();
    let mut st = DeclState { declared_slots: Vec::new(), declared_slots_dec: Vec::new(), declared_props: Vec::new(), declared_props_dec: Vec::new() };
    inject_declarations(ctx, req.method, 0, &param_names, &list, 1, &mut d, &mut st);
    unite_locals_declaration_types(&d, &list);
    list
}

/// The initializer-trait handling of `toGraphTargetItems`. Returns true when the list is final.
fn init_traits(ctx: &Ctx, req: &MethodReq, cd: &mut ConvertData, list: &List, init: &[Trait]) -> bool {
    let abc = ctx.abc;
    let names = ctx.names();
    let mn = |i: usize| abc.constants.multinames.get(i).filter(|_| i != 0);
    let mut class_traits: Vec<(i64, &[Trait])> = Vec::new();
    if req.initializer == TRAIT_SCRIPT_INITIALIZER {
        for &c in &req.init_trait_classes {
            class_traits.push((abc.instances[c].name as i64, &abc.classes[c].traits));
        }
    }
    class_traits.push((-1, init));
    let activation: Vec<&Multiname> = abc.body(req.method).map(|b| b.traits.iter().filter(|t| t.name > 0).filter_map(|t| mn(t.name)).collect()).unwrap_or_default();
    let mut inherited: Vec<&Multiname> = Vec::new();
    let mut inherited_names: HashSet<usize> = HashSet::new();
    if !req.is_static {
        collect_uninitialized_inherited(ctx, req.class_index, cd, &mut inherited, &mut inherited_names);
    }
    let slot_of = |t: &Trait| match t.data {
        TraitData::Slot { slot_id, .. } | TraitData::Const { slot_id, .. } => Some(slot_id),
        _ => None,
    };
    let mut i = 0usize;
    'loopi: while i < list.borrow().len() {
        let ti = list.borrow()[i].clone();
        let k = ti.borrow().k.clone();
        if let K::SetSlot { slot_obj, idx, .. } = &k {
            if matches!(slot_obj.borrow().k, K::Global) && req.initializer == TRAIT_SCRIPT_INITIALIZER {
                for t in init {
                    if slot_of(t) == Some(*idx as usize) {
                        let value = ti.borrow().value.clone();
                        if let Some(value) = value {
                            if !cd.contains(t) {
                                set_function_name(ctx, &value, t, &req.fqn);
                                cd.assigned.push((t as *const Trait, Assigned { command: Some(ti.clone()), value, initializer: req.initializer, method: req.method }));
                                i += 1;
                                continue 'loopi;
                            }
                        }
                    }
                }
            }
        }
        let set = match &k {
            K::InitProperty { prop, obj, .. } | K::SetProperty { prop, obj, .. } => Some((prop.clone(), obj.clone())),
            _ => None,
        };
        if let Some((prop, obj)) = set {
            let m_index = match &prop.borrow().k {
                K::FullMultiname { mn, .. } => *mn,
                _ => 0,
            };
            let value = ti.borrow().value.clone();
            let m = mn(m_index);
            let om: Option<&Multiname> = match &obj.borrow().k {
                K::GetProperty { prop: p, .. } => match &p.borrow().k {
                    K::FullMultiname { mn: x, .. } => mn(*x),
                    _ => None,
                },
                K::GetLex { mn: x, .. } => mn(*x),
                _ => None,
            };
            for &(cm_index, ts) in &class_traits {
                let cm = if cm_index == -1 { None } else { mn(cm_index as usize) };
                for (j, t) in ts.iter().enumerate() {
                    let tm = mn(t.name);
                    if tm.is_some() && tm == m && (cm_index == -1 || om == cm) {
                        if slot_of(t).is_some() {
                            if let Some(value) = value.clone() {
                                if !cd.contains(t) {
                                    if !req.is_static {
                                        let mut subs = all_sub_items(&value);
                                        subs.push(value.clone());
                                        let later: Vec<&Multiname> = init.iter().skip(j + 1).filter(|t| t.name > 0).filter_map(|t| mn(t.name)).collect();
                                        let mut un: Vec<&Multiname> = Vec::new();
                                        let mut un_names: HashSet<usize> = HashSet::new();
                                        collect_uninitialized(ctx, init, cd, &mut un, &mut un_names);
                                        un.extend(inherited.iter().copied());
                                        un_names.extend(inherited_names.iter().copied());
                                        let refers = |x: Option<&Multiname>| -> bool {
                                            let Some(x) = x else { return false };
                                            later.contains(&x) || activation.contains(&x) || un.contains(&x) || name_index(x).is_some_and(|n| n > 0 && un_names.contains(&n))
                                        };
                                        for item in &subs {
                                            let ik = item.borrow().k.clone();
                                            match ik {
                                                K::GetProperty { prop: p, obj: o, .. } => {
                                                    let x = match &p.borrow().k {
                                                        K::FullMultiname { mn: x, .. } => mn(*x),
                                                        _ => None,
                                                    };
                                                    if refers(x) || matches!(o.borrow().k, K::NewActivation) {
                                                        i += 1;
                                                        continue 'loopi;
                                                    }
                                                }
                                                K::GetLex { mn: x, .. } => {
                                                    if refers(mn(x)) {
                                                        i += 1;
                                                        continue 'loopi;
                                                    }
                                                }
                                                K::GetSlot { slot_obj, .. } if matches!(slot_obj.borrow().k, K::NewActivation) => {
                                                    i += 1;
                                                    continue 'loopi;
                                                }
                                                K::LocalReg { .. } => {
                                                    i += 1;
                                                    continue 'loopi;
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                    set_function_name(ctx, &value, t, &req.fqn);
                                    let command = if cm_index == -1 { Some(ti.clone()) } else { None };
                                    cd.assigned.push((t as *const Trait, Assigned { command, value, initializer: req.initializer, method: req.method }));
                                    if !req.is_static {
                                        list.borrow_mut().remove(i);
                                        continue 'loopi;
                                    }
                                    i += 1;
                                    continue 'loopi;
                                }
                            }
                            break;
                        }
                    }
                }
            }
        } else if !req.is_static {
            if req.initializer == TRAIT_INSTANCE_INITIALIZER && matches!(k, K::SetLocal { .. } | K::Declaration { .. }) {
                i += 1;
                continue;
            }
            break;
        }
        i += 1;
    }
    let len = list.borrow().len();
    let mut last_pos = len as i64 - 1;
    if last_pos < 0 {
        last_pos = 0;
    }
    let lp = last_pos as usize;
    if list.borrow().len() > lp && is::script_end(&list.borrow()[lp]) {
        last_pos -= 1;
    }
    if last_pos < 0 {
        last_pos = 0;
    }
    let lp = last_pos as usize;
    if list.borrow().len() > lp && matches!(list.borrow()[lp].borrow().k, K::ReturnVoid) {
        list.borrow_mut().remove(lp);
    }
    if req.initializer == TRAIT_SCRIPT_INITIALIZER && list.borrow().len() > lp {
        let rv = list.borrow()[lp].clone();
        if matches!(rv.borrow().k, K::ReturnValue) {
            let v = rv.borrow().val();
            if is::local_reg(&v) {
                list.borrow_mut().remove(lp);
            } else {
                list.borrow_mut()[lp] = v;
            }
        }
    }
    if req.initializer == TRAIT_CLASS_INITIALIZER || req.initializer == TRAIT_SCRIPT_INITIALIZER {
        initializer_slots(ctx, req, cd, list, init);
        let kept: Vec<It> = list.borrow().iter().filter(|t| !matches!(t.borrow().k, K::ReturnVoid | K::InitProperty { .. })).cloned().collect();
        *list.borrow_mut() = kept;
        if list.borrow().is_empty() {
            return true;
        }
    }
    let _ = names;
    false
}

fn name_index(m: &Multiname) -> Option<usize> {
    match m {
        Multiname::QName { name, .. } | Multiname::RtqName { name, .. } | Multiname::Multiname { name, .. } => Some(*name),
        _ => None,
    }
}

fn set_function_name(ctx: &Ctx, value: &It, t: &Trait, fqn: &[Dotted]) {
    if let K::NewFunction { name, .. } = &mut value.borrow_mut().k {
        *name = ctx.names().name(t.name, fqn, true, true);
    }
}

fn collect_uninitialized<'a>(ctx: &Ctx<'a>, traits: &'a [Trait], cd: &ConvertData, out: &mut Vec<&'a Multiname>, out_names: &mut HashSet<usize>) {
    for t in traits {
        let value = match &t.data {
            TraitData::Slot { value, .. } | TraitData::Const { value, .. } => value,
            _ => continue,
        };
        if cd.contains(t) || value.is_some_and(|v| v.kind != 0) {
            continue;
        }
        if t.name > 0 {
            if let Some(m) = ctx.abc.constants.multinames.get(t.name) {
                out.push(m);
                if let Some(n) = name_index(m).filter(|n| *n > 0) {
                    out_names.insert(n);
                }
            }
        }
    }
}

fn collect_uninitialized_inherited<'a>(ctx: &Ctx<'a>, class_index: i32, cd: &ConvertData, out: &mut Vec<&'a Multiname>, out_names: &mut HashSet<usize>) {
    let abc = ctx.abc;
    if class_index < 0 || class_index as usize >= abc.instances.len() {
        return;
    }
    let mut visited = HashSet::new();
    let mut walk = class_index as usize;
    while walk < abc.instances.len() && visited.insert(walk) {
        let ii = &abc.instances[walk];
        if walk != class_index as usize {
            collect_uninitialized(ctx, &ii.traits, cd, out, out_names);
        }
        if ii.super_name == 0 {
            break;
        }
        let sup = abc.constants.multinames.get(ii.super_name);
        let Some(parent) = abc.instances.iter().position(|x| abc.constants.multinames.get(x.name) == sup) else { break };
        walk = parent;
    }
}

/// Class and script initializers: assigned slot values become `TraitSlotConst` items.
fn initializer_slots(ctx: &Ctx, req: &MethodReq, cd: &ConvertData, list: &List, init: &[Trait]) {
    let abc = ctx.abc;
    let names = ctx.names();
    let mut cmd_assigned: Vec<(It, Assigned, usize)> = Vec::new();
    for (p, a) in &cd.assigned {
        let Some(ti) = init.iter().position(|t| std::ptr::eq(t, *p)) else { continue };
        match &a.command {
            None => {
                let script_init = abc.scripts.get(req.script_index.max(0) as usize).map_or(0, |s| s.init);
                let item = bare(K::TraitSlotConst { script: req.script_index, class: req.class_index, trait_index: ti as i32, is_static: true, initializer: TRAIT_SCRIPT_INITIALIZER, method: script_init, value: Some(a.value.clone()) }, prec::PRIMARY);
                list.borrow_mut().push(item);
            }
            Some(c) => cmd_assigned.push((c.clone(), a.clone(), ti)),
        }
    }
    let mut i = 0;
    while i < list.borrow().len() {
        let ti = list.borrow()[i].clone();
        if let Some((_, a, tix)) = cmd_assigned.iter().find(|(c, _, _)| same(c, &ti)) {
            let t = &init[*tix];
            let ns_kind = names.simple_ns_kind(t.name);
            if req.class_index == -1 && (ns_kind == crate::abc::ns_kind::PACKAGE || ns_kind == crate::abc::ns_kind::PACKAGE_INTERNAL) {
                list.borrow_mut().remove(i);
                continue;
            }
            let (src, ls) = (ti.borrow().src, ti.borrow().line_start);
            let item = mk(K::TraitSlotConst { script: req.script_index, class: req.class_index, trait_index: *tix as i32, is_static: req.is_static, initializer: req.initializer, method: req.method, value: Some(a.value.clone()) }, src, ls, prec::PRIMARY, None);
            list.borrow_mut()[i] = item;
        }
        i += 1;
    }
    if req.initializer == TRAIT_SCRIPT_INITIALIZER {
        fn visit_block(items: &List) {
            let n = items.borrow().len();
            for i in 0..n {
                let item = items.borrow()[i].clone();
                if is::set_local(&item) {
                    let v = item.borrow().val();
                    items.borrow_mut()[i] = v;
                }
                if is::block(&item) {
                    for s in subs(&item) {
                        visit_block(&s);
                    }
                }
            }
        }
        visit_block(list);
        package_script_slots(ctx, list, init);
    }
}

fn trait_ns(ctx: &Ctx, t: &Trait) -> Option<(u8, usize)> {
    match ctx.abc.constants.multinames.get(t.name)? {
        Multiname::QName { ns, .. } => Some((ctx.names().ns_kind(*ns), *ns)),
        _ => None,
    }
}

fn package_script_slots(ctx: &Ctx, list: &List, init: &[Trait]) {
    use crate::abc::ns_kind::{PACKAGE, PACKAGE_INTERNAL, PRIVATE};
    let names = ctx.names();
    let mut current: Option<(It, String)> = None;
    let mut i = 0;
    while i < list.borrow().len() {
        let ti = list.borrow()[i].clone();
        let tsc = match &ti.borrow().k {
            K::TraitSlotConst { trait_index, .. } => Some(*trait_index as usize),
            _ => None,
        };
        if let Some(tix) = tsc {
            if let Some((kind, ns)) = trait_ns(ctx, &init[tix]) {
                if kind == PACKAGE || kind == PACKAGE_INTERNAL {
                    let pkg = names.ns_name(ns).raw();
                    let add_to = |p: &It| {
                        if let K::Package { items, .. } = &p.borrow().k {
                            items.borrow_mut().push(ti.clone());
                        }
                    };
                    match &current {
                        Some((p, n)) if *n == pkg => {
                            add_to(p);
                            list.borrow_mut().remove(i);
                            continue;
                        }
                        _ => {
                            let p = bare(K::Package { items: empty_list(), name: pkg.clone() }, prec::PRIMARY);
                            add_to(&p);
                            list.borrow_mut()[i] = p.clone();
                            current = Some((p, pkg));
                        }
                    }
                }
            }
        } else if let Some((p, pkg)) = current.clone() {
            let mut inside = true;
            visit_recursively(&ti, &mut |item| {
                if let K::GetSlot { slot_obj, idx, .. } = &item.borrow().k {
                    if matches!(slot_obj.borrow().k, K::Global) {
                        for t in init {
                            if let TraitData::Slot { slot_id, .. } | TraitData::Const { slot_id, .. } = t.data {
                                if slot_id as i32 == *idx {
                                    if let Some((kind, ns)) = trait_ns(ctx, t) {
                                        if (kind == PACKAGE_INTERNAL && pkg != names.ns_raw_name(ns)) || kind == PRIVATE {
                                            inside = false;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                true
            });
            if inside {
                if let K::Package { items, .. } = &p.borrow().k {
                    items.borrow_mut().push(ti.clone());
                }
                list.borrow_mut().remove(i);
                continue;
            }
            current = None;
        }
        i += 1;
    }
}

struct DeclState {
    declared_slots: Vec<usize>,
    declared_slots_dec: Vec<It>,
    declared_props: Vec<String>,
    declared_props_dec: Vec<It>,
}

fn set_declaration(it: &It, d: &It) {
    let mut b = it.borrow_mut();
    match &mut b.k {
        K::SetLocal { set, .. } | K::SetProperty { set, .. } | K::InitProperty { set, .. } | K::SetSlot { set, .. } | K::SetSuper { set, .. } => set.declaration = Some(d.clone()),
        _ => {}
    }
}

fn decl_mut(d: &It, f: impl FnOnce(&mut Option<It>, &mut bool, &mut bool)) {
    if let K::Declaration { ty, type_is_null, show_value, .. } = &mut d.borrow_mut().k {
        f(ty, type_is_null, show_value);
    }
}

fn decl_ty(d: &It) -> Option<It> {
    match &d.borrow().k {
        K::Declaration { ty, .. } => ty.clone(),
        _ => None,
    }
}

fn is_inc_dec_item(it: &It) -> bool {
    matches!(it.borrow().k, K::PostIncrement { .. } | K::PostDecrement { .. } | K::Un { op: Un::PreIncrement, .. })
}

/// `handleDeclareReg`.
fn handle_declare_reg(minreg: usize, assignment: &It, d: &mut [Option<It>], reg: usize) -> It {
    if reg < minreg || reg >= d.len() {
        return assignment.clone();
    }
    let value = assignment.borrow().value.clone();
    let mut vtype;
    if is_inc_dec_item(assignment) {
        vtype = return_type(assignment);
    } else {
        let vk = value.as_ref().map(|v| v.borrow().k.clone());
        let ak = assignment.borrow().k.clone();
        vtype = match (&vk, &ak) {
            (Some(K::Convert { ty }), _) | (Some(K::Coerce { ty }), _) => ty.clone(),
            (_, K::LocalReg { ty, .. }) => ty.clone(),
            (_, K::GetSlot { slot_ty, .. }) => slot_ty.clone(),
            (Some(_), _) => {
                let v = value.clone().unwrap();
                let t = return_type(&v);
                if is::simple_value(&v) || matches!(v.borrow().k, K::GetLex { .. } | K::LocalReg { .. }) {
                    t
                } else if is_unknown(&t) {
                    unbounded()
                } else {
                    t
                }
            }
            (None, _) => unbounded(),
        };
    }
    let mut is_null = false;
    if is_type(&vtype, "null") {
        vtype = unbounded();
        is_null = true;
    }
    match d[reg].clone() {
        None => {
            let (src, ls, p) = {
                let b = assignment.borrow();
                (b.src, b.line_start, prec_of(assignment))
            };
            let dec = mk(K::Declaration { assignment: assignment.clone(), ty: Some(vtype), type_is_null: is_null, show_value: true }, src, ls, p, None);
            set_declaration(assignment, &dec);
            d[reg] = Some(dec.clone());
            dec
        }
        Some(dec) => {
            let cur = decl_ty(&dec).unwrap_or_else(unbounded);
            let tin = matches!(dec.borrow().k, K::Declaration { type_is_null: true, .. });
            if tin {
                decl_mut(&dec, |t, n, _| {
                    *t = Some(vtype.clone());
                    *n = is_null;
                });
            } else if is_unbounded(&cur) {
            } else if !super::eq::eq(&cur, &vtype) {
                decl_mut(&dec, |t, _, _| *t = Some(unbounded()));
            }
            set_declaration(assignment, &dec);
            assignment.clone()
        }
    }
}

fn items_on_line(item: &It) -> Vec<It> {
    let mut v = vec![item.clone()];
    visit_recursively_no_block(item, &mut |i, _| v.push(i.clone()));
    v
}

fn in_object(it: &It) -> Option<It> {
    match &it.borrow().k {
        K::ForIn { expr, .. } | K::ForEachIn { expr, .. } => match &expr.borrow().k {
            K::InFor { object, .. } => Some(object.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn set_in_object(it: &It, v: It) {
    let expr = match &it.borrow().k {
        K::ForIn { expr, .. } | K::ForEachIn { expr, .. } => expr.clone(),
        _ => return,
    };
    let mut b = expr.borrow_mut();
    if let K::InFor { object, .. } = &mut b.k {
        *object = v;
    }
}

/// `injectDeclarations`.
#[allow(clippy::too_many_arguments)]
fn inject_declarations(ctx: &Ctx, method: usize, level: usize, param_names: &[String], items: &List, minreg: usize, d: &mut [Option<It>], st: &mut DeclState) {
    let abc = ctx.abc;
    let names = ctx.names();
    let mut traits: Vec<(String, &Trait)> = Vec::new();
    if let Some(body) = abc.body(method) {
        for t in &body.traits {
            if matches!(t.data, TraitData::Slot { .. } | TraitData::Const { .. }) {
                let n = names.name(t.name, &[], true, true);
                match traits.iter_mut().find(|(k, _)| *k == n) {
                    Some(e) => e.1 = t,
                    None => traits.push((n, t)),
                }
            }
        }
    }
    let trait_of = |n: &str| traits.iter().find(|(k, _)| k == n).map(|(_, t)| *t);
    let type_of = |t: &Trait| match t.data {
        TraitData::Slot { type_name, .. } | TraitData::Const { type_name, .. } => index::mn_type(ctx, type_name),
        _ => unbounded(),
    };
    if level == 0 {
        let mut begin: Vec<String> = Vec::new();
        let n = items.borrow().len();
        for i in 0..n {
            let item = items.borrow()[i].clone();
            let k = item.borrow().k.clone();
            let (prop_name, value) = match &k {
                K::StoreNewActivation { .. } => continue,
                K::SetSlot { slot_name, .. } => {
                    let Some(sn) = slot_name else { break };
                    (names.name(*sn, &[], true, true), item.borrow().val())
                }
                K::SetProperty { obj, prop, .. } => {
                    if !is::find_property(obj) {
                        break;
                    }
                    match &prop.borrow().k {
                        K::FullMultiname { resolved, .. } => (resolved.clone().unwrap_or_default(), item.borrow().val()),
                        _ => break,
                    }
                }
                _ => break,
            };
            let value = not_coerced(&value);
            let mut has_prev = false;
            visit_recursively_no_block(&value, &mut |sub, _| {
                let pm = match &sub.borrow().k {
                    K::GetProperty { obj, prop, .. } if is::find_property(obj) => match &prop.borrow().k {
                        K::FullMultiname { mn, .. } => Some(*mn),
                        _ => None,
                    },
                    K::GetLex { mn, .. } => Some(*mn),
                    _ => None,
                };
                if let Some(pm) = pm {
                    let pn = names.name(pm, &[], true, true);
                    if trait_of(&pn).is_some() && !param_names.contains(&pn) && !begin.contains(&pn) {
                        has_prev = true;
                    }
                }
            });
            if has_prev {
                break;
            }
            if !begin.contains(&prop_name) {
                begin.push(prop_name);
            }
        }
        let mut pos = 0;
        for (tn, t) in &traits {
            if param_names.contains(tn) || begin.contains(tn) {
                continue;
            }
            let t_ty = type_of(t);
            let full = names.name_with_ns(t.name, true);
            let lex = bare(K::GetLex { mn: t.name, ty: t_ty.clone(), call_ty: unbounded(), is_static: false, full }, prec::PRIMARY);
            let dec = mk(K::Declaration { assignment: lex, ty: Some(t_ty), type_is_null: false, show_value: false }, None, None, prec::PRIMARY, None);
            st.declared_slots_dec.push(dec.clone());
            st.declared_slots.push(t.name);
            items.borrow_mut().insert(pos, dec.clone());
            pos += 1;
            st.declared_props_dec.push(dec);
            st.declared_props.push(tn.clone());
        }
    }
    let mut i = 0;
    while i < items.borrow().len() {
        let current = items.borrow()[i].clone();
        let on_line = items_on_line(&current);
        if matches!(current.borrow().k, K::ForEachIn { .. }) {
            if let Some(o) = in_object(&current) {
                if let Some(reg) = super::avm2graph::local_reg(&o) {
                    if d.get(reg as usize).is_some_and(|x| x.is_none()) {
                        let nv = handle_declare_reg(minreg, &o, d, reg as usize);
                        set_in_object(&current, nv);
                    }
                }
            }
        }
        if matches!(current.borrow().k, K::ForIn { .. }) {
            if let Some(o) = in_object(&current) {
                if let Some(reg) = super::avm2graph::local_reg(&o) {
                    let nv = handle_declare_reg(minreg, &o, d, reg as usize);
                    set_in_object(&current, nv);
                }
            }
        }
        for sub in on_line {
            if let Some(reg) = super::avm2graph::set_local_reg(&sub) {
                let dec = handle_declare_reg(minreg, &sub, d, reg as usize);
                if !same(&dec, &sub) {
                    if same(&sub, &current) {
                        items.borrow_mut()[i] = dec;
                    } else {
                        decl_mut(&dec, |_, _, s| *s = false);
                        items.borrow_mut().insert(i, dec);
                        i += 1;
                    }
                }
            }
            if is_inc_dec_item(&sub) {
                let obj = match &sub.borrow().k {
                    K::PostIncrement { obj, .. } | K::PostDecrement { obj, .. } => Some(obj.clone()),
                    _ => sub.borrow().value.clone(),
                };
                if let Some(reg) = obj.as_ref().and_then(super::avm2graph::local_reg) {
                    handle_declare_reg(minreg, &sub, d, reg as usize);
                }
            }
            let sp = match &sub.borrow().k {
                K::SetProperty { obj, prop, .. } if is::find_property(obj) => match &prop.borrow().k {
                    K::FullMultiname { resolved, .. } => Some(resolved.clone().unwrap_or_default()),
                    _ => None,
                },
                _ => None,
            };
            if let Some(rn) = sp {
                if !param_names.contains(&rn) {
                    if !st.declared_props.contains(&rn) {
                        if let Some(t) = trait_of(&rn) {
                            let t_ty = type_of(t);
                            let (src, ls) = (sub.borrow().src, sub.borrow().line_start);
                            let dec = mk(K::Declaration { assignment: sub.clone(), ty: Some(t_ty), type_is_null: false, show_value: true }, src, ls, prec_of(&sub), None);
                            set_declaration(&sub, &dec);
                            st.declared_props_dec.push(dec.clone());
                            st.declared_props.push(rn.clone());
                            st.declared_slots_dec.push(dec.clone());
                            st.declared_slots.push(t.name);
                            if same(&sub, &current) {
                                items.borrow_mut()[i] = dec;
                            } else {
                                decl_mut(&dec, |_, _, s| *s = false);
                                items.borrow_mut().insert(i, dec);
                                i += 1;
                            }
                        }
                    } else {
                        let idx = st.declared_props.iter().position(|x| *x == rn).unwrap();
                        set_declaration(&sub, &st.declared_props_dec[idx].clone());
                    }
                }
            }
            let ss = match &sub.borrow().k {
                K::SetSlot { scope, slot_name, .. } if matches!(scope.borrow().k, K::NewActivation) => Some(*slot_name),
                _ => None,
            };
            if let Some(sn) = ss {
                let spn = sn.map(|s| names.name(s, &[], true, false)).unwrap_or_default();
                if !param_names.contains(&spn) {
                    let index = st.declared_slots.iter().position(|&s| abc.str(name_index_of(ctx, s)) == spn);
                    match index {
                        None => {
                            let t_ty = trait_of(&spn).map(type_of).unwrap_or_else(unbounded);
                            let (src, ls) = (sub.borrow().src, sub.borrow().line_start);
                            let dec = mk(K::Declaration { assignment: sub.clone(), ty: Some(t_ty), type_is_null: false, show_value: true }, src, ls, prec_of(&sub), None);
                            set_declaration(&sub, &dec);
                            st.declared_slots_dec.push(dec.clone());
                            st.declared_slots.push(sn.unwrap_or(0));
                            st.declared_props_dec.push(dec.clone());
                            st.declared_props.push(spn);
                            if same(&sub, &current) {
                                items.borrow_mut()[i] = dec;
                            } else {
                                decl_mut(&dec, |_, _, s| *s = false);
                                items.borrow_mut().insert(i, dec);
                                i += 1;
                            }
                        }
                        Some(idx) => set_declaration(&sub, &st.declared_slots_dec[idx].clone()),
                    }
                }
            }
        }
        if is::block(&current) {
            for s in subs(&current) {
                inject_declarations(ctx, method, level + 1, param_names, &s, minreg, d, st);
            }
        }
        i += 1;
    }
}

fn name_index_of(ctx: &Ctx, mn: usize) -> usize {
    ctx.abc.constants.multinames.get(mn).filter(|_| mn != 0).and_then(name_index).unwrap_or(0)
}

/// `uniteLocalsDeclarationTypes`.
fn unite_locals_declaration_types(d: &[Option<It>], items: &List) {
    let n = items.borrow().len();
    for i in 0..n {
        let current = items.borrow()[i].clone();
        for item in items_on_line(&current) {
            let mut b = item.borrow_mut();
            match &mut b.k {
                K::SetLocal { reg, ty, .. } | K::LocalReg { reg, ty, .. } => {
                    if let Some(Some(dec)) = d.get(*reg as usize) {
                        if let Some(t) = decl_ty(dec) {
                            *ty = t;
                        } else {
                            *ty = unbounded();
                        }
                    }
                }
                _ => {}
            }
        }
        if is::block(&current) {
            for s in subs(&current) {
                unite_locals_declaration_types(d, &s);
            }
        }
    }
}

/// `NewFunctionAVM2Item.appendTo`.
pub fn print_new_function(it: &It, w: &mut Writer, ld: &LocalData) {
    let (name, path, is_static, script, class, method, scope) = match &it.borrow().k {
        K::NewFunction { name, path, is_static, script, class, method, scope } => (name.clone(), path.clone(), *is_static, *script, *class, *method, scope.clone()),
        _ => return,
    };
    if ld.seen_methods.contains(&method) {
        w.append(&format!("\u{a7}\u{a7}method({method})"));
        return;
    }
    let ctx = ld.ctx;
    let abc = ctx.abc;
    w.append("function");
    if !name.is_empty() {
        w.append(" ").append(&name);
    }
    w.append_no_hilight("(");
    param_str(ctx, method, &ld.fqn, w);
    w.append_no_hilight("):");
    w.append(&return_type_str(ctx, method, &ld.fqn));
    w.start_block();
    if abc.body(method).is_some() {
        let mut call_stack = ld.call_stack.clone();
        call_stack.push(method);
        let req = MethodReq { method, is_static, script_index: script, class_index: class, init_traits: None, init_trait_classes: Vec::new(), initializer: 0, fqn: ld.fqn.clone(), call_stack, scope, path: format!("{path}/inner") };
        let mut cd = ConvertData::default();
        match convert(ctx, &req, &mut cd, &ld.seen_methods) {
            Ok(conv) => to_source(ctx, &req, &conv, &ld.seen_methods, ld.class_index, w),
            Err(e) => error_comment(w, &e),
        }
    }
    w.end_block();
}

/// `MethodInfo.getParamStr`.
pub fn param_str(ctx: &Ctx, method: usize, fqn: &[Dotted], w: &mut Writer) {
    let abc = ctx.abc;
    let mi = &abc.methods[method];
    let names = ctx.names();
    let mut regs: HashMap<i32, String> = HashMap::new();
    if let Some(body) = abc.body(method) {
        if ctx.cfg.local_names_from_debug {
            let code = Code::new(abc, body);
            let fmt = |r: i32| ctx.cfg.reg_name(r);
            regs = code.local_reg_names_from_debug(abc, body.max_regs, &fmt);
        }
    }
    let n = mi.param_types.len();
    for (i, &pt) in mi.param_types.iter().enumerate() {
        if i > 0 {
            w.append_no_hilight(", ");
        }
        if !regs.is_empty() {
            w.append(&ctx.ids.print(regs.get(&(i as i32 + 1)).map(String::as_str).unwrap_or("")));
        } else if mi.param_names.len() > i && mi.param_names[i] != 0 && ctx.cfg.param_names {
            w.append(&ctx.ids.print(abc.str(mi.param_names[i])));
        } else {
            w.append(&format!("param{}", i + 1));
        }
        w.append_no_hilight(":");
        if pt == 0 {
            w.append("*");
        } else {
            w.append(&names.name(pt, fqn, false, true));
        }
        if mi.flags & method_flags::HAS_OPTIONAL != 0 && i + mi.optional.len() >= n {
            let oi = i - (n - mi.optional.len());
            w.append_no_hilight(" = ");
            w.append(&value_kind_str(ctx, &mi.optional[oi]));
        }
    }
    if mi.flags & method_flags::NEED_REST != 0 {
        let mut add = String::new();
        if n > 0 {
            add.push_str(", ");
        }
        add.push_str("... ");
        let rest = if !regs.is_empty() { regs.get(&(n as i32 + 1)).cloned().unwrap_or_default() } else { "rest".into() };
        w.append(&add);
        w.append(&rest);
    }
}

/// `MethodInfo.getReturnTypeStr`.
pub fn return_type_str(ctx: &Ctx, method: usize, fqn: &[Dotted]) -> String {
    let rt = ctx.abc.methods[method].return_type;
    if rt == 0 {
        return "*".into();
    }
    let m = ctx.abc.constants.multinames.get(rt);
    if let Some(m) = m {
        if !matches!(m, Multiname::TypeName { .. }) {
            if let Some(ni) = name_index(m).filter(|n| *n > 0) {
                if ctx.abc.str(ni) == "void" {
                    return "void".into();
                }
            }
        }
    }
    ctx.names().name(rt, fqn, false, true)
}

/// `ValueKind.toString(abc)`.
pub fn value_kind_str(ctx: &Ctx, v: &crate::abc::ValueKind) -> String {
    let abc = ctx.abc;
    let c = &abc.constants;
    let num = |d: f64| crate::abc::names::ecma_number(d);
    match v.kind {
        0x03 => num(c.ints.get(v.index).copied().unwrap_or(0) as f64),
        0x04 => num(c.uints.get(v.index).copied().unwrap_or(0) as f64),
        0x06 => num(c.doubles.get(v.index).copied().unwrap_or(f64::NAN)),
        0x02 if abc.minor >= 17 => super::values::decimal_to_string(&c.decimals.get(v.index).copied().unwrap_or([0; 16])),
        0x02 => {
            let f = c.floats.get(v.index).copied().unwrap_or(f32::NAN);
            format!("{}{}", num(f as f64), if f.is_finite() { "f" } else { "" })
        }
        0x01 => format!("\"{}\"", crate::abc::names::escape_as(abc.str(v.index))),
        0x0b => "true".into(),
        0x0a => "false".into(),
        0x0c => "null".into(),
        0x00 => "undefined".into(),
        0x08 | 0x16 | 0x17 | 0x18 | 0x19 | 0x1a | 0x05 => {
            let ns = c.namespaces.get(v.index).map_or(0, |n| n.name);
            format!("\"{}\"", abc.str(ns))
        }
        _ => "?".into(),
    }
}

pub fn print_trait_slot_const(it: &It, w: &mut Writer, ld: &LocalData) {
    super::script::print_trait_slot_const(it, w, ld);
}

#[allow(dead_code)]
fn is_debug(code: &Code, i: usize) -> bool {
    code.ins.get(i).is_some_and(|x| x.code == op::DEBUG)
}
