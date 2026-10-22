//! Body statistics, deleting the replaced script and `ABC.pack`, and the replacement driver.

use std::collections::{HashMap, HashSet, VecDeque};

use super::gen::Cx;
use super::index::{AbcId, Owner};
use super::ins::{code_bytes, op, Code};
use super::parser::Parser;
use super::R;
use crate::abc::opcodes::{decode, def, Dialect, Instruction, Operand};
use crate::abc::stats::deltas;
use crate::abc::{method_flags, ns_kind, Abc, Exception, TraitData};

#[derive(Default, Clone, Copy)]
struct InsStat {
    seen: bool,
    scope_after: i64,
}

struct Stats {
    max_stack: i64,
    max_scope: i64,
    max_local: i64,
    has_activation: bool,
    has_dxns: bool,
    init_scope: i64,
    ins: Vec<InsStat>,
}

fn as_instruction(i: &super::ins::Ins) -> Option<Instruction> {
    let d = def(i.code)?;
    Some(Instruction { offset: 0, len: i.len(), code: i.code, name: d.name, kinds: d.operands, operands: i.ops.clone() })
}

impl Cx {
    /// `MethodBody.autoFillStats`, over a body still being generated.
    pub fn auto_fill(&mut self, bi: usize, init_scope: usize) {
        let Some((code, exc)) = self.pending.get(&bi).cloned() else { return };
        let ins: Vec<Instruction> = match code.iter().map(|i| as_instruction(&i.borrow())).collect::<Option<Vec<_>>>() {
            Some(v) => v,
            None => return,
        };
        let mut addr = Vec::with_capacity(ins.len() + 1);
        let mut a = 0usize;
        for i in &ins {
            addr.push(a);
            a += i.len;
        }
        addr.push(a);
        let pos_of = |ad: i64| -> Option<usize> {
            if ad < 0 {
                return None;
            }
            addr.binary_search(&(ad as usize)).ok()
        };
        let mut st = Stats { max_stack: 0, max_scope: 0, max_local: 1, has_activation: false, has_dxns: false, init_scope: init_scope as i64, ins: vec![InsStat::default(); ins.len()] };
        if !self.walk(&ins, &addr, &pos_of, &mut st, 0, 0, init_scope as i64) {
            return;
        }
        let mut scope_pos = -1i64;
        for e in &exc {
            let Some(end) = pos_of(e.end as i64) else { continue };
            if scope_pos == -1 {
                if end == 0 {
                    continue;
                }
                scope_pos = st.ins[end - 1].scope_after;
            }
            let visited: Vec<bool> = st.ins.iter().map(|s| s.seen).collect();
            let Some(target) = pos_of(e.target as i64) else { continue };
            if !self.walk(&ins, &addr, &pos_of, &mut st, target, 1, scope_pos) {
                return;
            }
            let mut max_ip = 0;
            for (i, s) in st.ins.iter().enumerate() {
                if s.seen && !visited[i] {
                    max_ip = i;
                }
            }
            scope_pos = st.ins[max_ip].scope_after;
        }
        let act = usize::from(st.has_activation);
        let method = self.abc.bodies[bi].method;
        {
            let b = &mut self.abc.bodies[bi];
            b.max_stack = st.max_stack.max(0) as usize;
            b.max_scope_depth = st.max_scope.max(0) as usize + act;
            b.max_regs = st.max_local.max(0) as usize;
            b.init_scope_depth = init_scope + act;
        }
        let mi = &mut self.abc.methods[method];
        if st.has_dxns {
            mi.flags |= method_flags::SET_DXNS;
        } else {
            mi.flags &= !method_flags::SET_DXNS;
        }
        if st.has_activation {
            mi.flags |= method_flags::NEED_ACTIVATION;
        } else {
            mi.flags &= !method_flags::NEED_ACTIVATION;
        }
        let min_regs = mi.param_types.len() + 1 + usize::from(mi.flags & method_flags::NEED_REST != 0);
        let b = &mut self.abc.bodies[bi];
        if b.max_regs < min_regs {
            b.max_regs = min_regs;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(&mut self, ins: &[Instruction], addr: &[usize], pos_of: &dyn Fn(i64) -> Option<usize>, st: &mut Stats, pos: usize, stack: i64, scope: i64) -> bool {
        let mut q = VecDeque::from([(pos, stack, scope)]);
        while let Some((mut pos, mut stack, mut scope)) = q.pop_front() {
            while pos < ins.len() {
                let i = &ins[pos];
                if st.ins[pos].seen {
                    break;
                }
                if i.code == op::NEW_FUNCTION {
                    if let Some(inner) = self.abc.body_of.get(i.operands[0] as usize).copied().flatten() {
                        let s = st.init_scope + i64::from(st.has_activation);
                        self.auto_fill(inner, s.max(0) as usize);
                    }
                }
                st.ins[pos].seen = true;
                let (pop, push, sd) = deltas(i, &self.abc).unwrap_or((0, 0, 0));
                stack += push - pop;
                scope += sd;
                st.ins[pos].scope_after = scope;
                st.max_stack = st.max_stack.max(stack);
                st.max_scope = st.max_scope.max(scope);
                if matches!(i.code, op::DXNS | op::DXNS_LATE) {
                    st.has_dxns = true;
                }
                if i.code == op::NEW_ACTIVATION {
                    st.has_activation = true;
                }
                let fixed = match i.code {
                    0xd0..=0xd3 => Some((i.code - 0xd0) as i64),
                    0xd4..=0xd7 => Some((i.code - 0xd4) as i64),
                    _ => None,
                };
                if let Some(r) = fixed {
                    st.max_local = st.max_local.max(r + 1);
                } else {
                    for (k, kind) in i.kinds.iter().enumerate() {
                        if *kind == Operand::LocalRegIndex {
                            st.max_local = st.max_local.max(i.operands[k] + 1);
                        }
                    }
                }
                if matches!(i.code, op::RETURN_VALUE | op::RETURN_VOID | op::THROW) {
                    break;
                }
                if i.code == op::JUMP {
                    match pos_of(addr[pos] as i64 + i.len as i64 + i.operands[0]) {
                        Some(p) => {
                            pos = p;
                            continue;
                        }
                        None => return false,
                    }
                } else if matches!(i.code, 0x0c..=0x0f | 0x11..=0x1a) {
                    match pos_of(addr[pos] as i64 + i.len as i64 + i.operands[0]) {
                        Some(p) => q.push_back((p, stack, scope)),
                        None => return false,
                    }
                }
                if i.code == op::LOOKUP_SWITCH {
                    for (k, o) in i.operands.iter().enumerate() {
                        if k == 1 {
                            continue;
                        }
                        match pos_of(addr[pos] as i64 + o) {
                            Some(p) => q.push_back((p, stack, scope)),
                            None => return false,
                        }
                    }
                }
                pos += 1;
            }
        }
        true
    }

    /// Writes generated bodies out as bytes.
    pub fn finalize(&mut self) {
        let mut ks: Vec<usize> = self.pending.keys().copied().collect();
        ks.sort();
        for bi in ks {
            let (code, exc) = self.pending.remove(&bi).unwrap();
            let b = &mut self.abc.bodies[bi];
            b.code = code_bytes(&code);
            b.exceptions = exc.iter().map(|e| Exception { start: e.start, end: e.end, target: e.target, type_name: e.ty, var_name: e.name }).collect();
        }
    }

    fn delete_trait(&mut self, owner: Owner, index: usize) {
        self.deleted.traits.insert((owner, index));
        let t = match owner {
            Owner::Script(s) => self.abc.scripts[s].traits[index].clone(),
            Owner::Instance(id) => {
                let c = self.class_pos_of(id);
                self.abc.instances[c].traits[index].clone()
            }
            Owner::Class(id) => {
                let c = self.class_pos_of(id);
                self.abc.classes[c].traits[index].clone()
            }
        };
        match t.data {
            TraitData::Class { class, .. } => self.delete_class(class),
            TraitData::Method { method, .. } | TraitData::Getter { method, .. } | TraitData::Setter { method, .. } | TraitData::Function { method, .. } => {
                self.deleted.methods.insert(method);
            }
            _ => {}
        }
    }

    fn class_pos_of(&self, id: usize) -> usize {
        self.class_ids.iter().position(|&x| x == id).unwrap()
    }

    fn delete_class(&mut self, c: usize) {
        let id = self.class_ids[c];
        self.deleted.classes.insert(id);
        for i in 0..self.abc.classes[c].traits.len() {
            self.delete_trait(Owner::Class(id), i);
        }
        self.deleted.methods.insert(self.abc.classes[c].cinit);
        for i in 0..self.abc.instances[c].traits.len() {
            self.delete_trait(Owner::Instance(id), i);
        }
        self.deleted.methods.insert(self.abc.instances[c].iinit);
    }

    /// `ScriptPack.delete`.
    pub fn delete_pack(&mut self, script: usize, traits: &[usize], simple: bool) {
        if simple {
            self.deleted.scripts.insert(script);
            self.deleted.methods.insert(self.abc.scripts[script].init);
            for i in 0..self.abc.scripts[script].traits.len() {
                self.delete_trait(Owner::Script(script), i);
            }
        } else {
            for &t in traits {
                self.delete_trait(Owner::Script(script), t);
            }
        }
    }

    fn newfunction_targets(&self, body: usize) -> Vec<usize> {
        let b = &self.abc.bodies[body];
        let Ok(code) = decode(&b.code, Dialect::of(self.abc.major, self.abc.minor)) else { return Vec::new() };
        code.iter().filter(|i| i.code == op::NEW_FUNCTION).map(|i| i.operands[0] as usize).collect()
    }

    fn pack_methods(&mut self, deleted: &mut HashSet<usize>) {
        let n = self.abc.methods.len();
        let mut to_delete: Vec<usize> = Vec::new();
        let mut usage: HashMap<usize, i64> = HashMap::new();
        for m in 0..n {
            let Some(b) = self.abc.body_of.get(m).copied().flatten() else { continue };
            for t in self.newfunction_targets(b) {
                if t >= n {
                    continue;
                }
                if deleted.contains(&m) {
                    if !deleted.contains(&t) && !to_delete.contains(&t) {
                        to_delete.push(t);
                    }
                } else {
                    *usage.entry(t).or_insert(0) += 1;
                }
            }
        }
        while !to_delete.is_empty() {
            let m = to_delete.remove(0);
            let cnt = usage.get(&m).copied().unwrap_or(0);
            if cnt == 0 && !deleted.contains(&m) {
                deleted.insert(m);
                if let Some(b) = self.abc.body_of.get(m).copied().flatten() {
                    for t in self.newfunction_targets(b) {
                        if t >= n {
                            continue;
                        }
                        if !deleted.contains(&t) && !to_delete.contains(&t) {
                            to_delete.push(t);
                        }
                        *usage.entry(t).or_insert(1) -= 1;
                    }
                }
            }
        }
    }

    /// `ABC.pack`: drops what was deleted and renumbers the rest.
    pub fn pack(&mut self) {
        let view = self.deleted_view();
        for (si, s) in self.abc.scripts.iter_mut().enumerate() {
            let mut i = s.traits.len();
            while i > 0 {
                i -= 1;
                if view.traits.contains(&(Owner::Script(si), i)) {
                    s.traits.remove(i);
                }
            }
        }
        for c in 0..self.abc.instances.len() {
            let mut i = self.abc.instances[c].traits.len();
            while i > 0 {
                i -= 1;
                if view.traits.contains(&(Owner::Instance(c), i)) {
                    self.abc.instances[c].traits.remove(i);
                }
            }
            let mut i = self.abc.classes[c].traits.len();
            while i > 0 {
                i -= 1;
                if view.traits.contains(&(Owner::Class(c), i)) {
                    self.abc.classes[c].traits.remove(i);
                }
            }
        }
        let mut deleted = self.deleted.methods.clone();
        self.pack_methods(&mut deleted);
        let old_ids = self.class_ids.clone();
        let mut keep_class = Vec::new();
        for (c, id) in old_ids.iter().enumerate() {
            keep_class.push(!self.deleted.classes.contains(id));
            let _ = c;
        }
        let final_ids: Vec<usize> = old_ids.iter().zip(&keep_class).filter(|(_, k)| **k).map(|(id, _)| *id).collect();
        let pos_of_id: HashMap<usize, usize> = final_ids.iter().enumerate().map(|(p, id)| (*id, p)).collect();
        let cur_to_final: Vec<Option<usize>> = old_ids.iter().map(|id| pos_of_id.get(id).copied()).collect();
        let n = self.abc.methods.len();
        let mut method_map = vec![None; n];
        let mut next = 0;
        for (m, slot) in method_map.iter_mut().enumerate() {
            if !deleted.contains(&m) {
                *slot = Some(next);
                next += 1;
            }
        }
        let dialect = Dialect::of(self.abc.major, self.abc.minor);
        let mut bodies = std::mem::take(&mut self.abc.bodies);
        let mut kept = 0;
        self.body_map = bodies
            .iter()
            .map(|b| {
                if method_map.get(b.method).copied().flatten().is_some() {
                    kept += 1;
                    Some(kept - 1)
                } else {
                    None
                }
            })
            .collect();
        bodies.retain(|b| method_map.get(b.method).copied().flatten().is_some());
        for b in &mut bodies {
            b.method = method_map[b.method].unwrap();
            rewrite_code(b, dialect, &|kind, v| match kind {
                Operand::ClassIndex => pos_of_id.get(&(v as usize)).map(|p| *p as i64),
                Operand::MethodIndex => method_map.get(v as usize).copied().flatten().map(|m| m as i64),
                _ => None,
            });
        }
        let fix_traits = |traits: &mut Vec<crate::abc::Trait>| {
            for t in traits.iter_mut() {
                match &mut t.data {
                    TraitData::Class { class, .. } => {
                        if let Some(Some(p)) = cur_to_final.get(*class) {
                            *class = *p;
                        }
                    }
                    TraitData::Method { method, .. } | TraitData::Getter { method, .. } | TraitData::Setter { method, .. } | TraitData::Function { method, .. } => {
                        if let Some(Some(m)) = method_map.get(*method) {
                            *method = *m;
                        }
                    }
                    _ => {}
                }
            }
        };
        for b in &mut bodies {
            fix_traits(&mut b.traits);
        }
        let mut instances = Vec::new();
        let mut classes = Vec::new();
        let old_inst = std::mem::take(&mut self.abc.instances);
        let old_cls = std::mem::take(&mut self.abc.classes);
        for ((mut ii, mut ci), keep) in old_inst.into_iter().zip(old_cls).zip(&keep_class) {
            if !*keep {
                continue;
            }
            fix_traits(&mut ii.traits);
            fix_traits(&mut ci.traits);
            if let Some(Some(m)) = method_map.get(ii.iinit) {
                ii.iinit = *m;
            }
            if let Some(Some(m)) = method_map.get(ci.cinit) {
                ci.cinit = *m;
            }
            instances.push(ii);
            classes.push(ci);
        }
        self.abc.instances = instances;
        self.abc.classes = classes;
        self.class_ids = final_ids;
        let mut scripts = Vec::new();
        for (si, mut s) in std::mem::take(&mut self.abc.scripts).into_iter().enumerate() {
            if self.deleted.scripts.contains(&si) {
                continue;
            }
            fix_traits(&mut s.traits);
            if let Some(Some(m)) = method_map.get(s.init) {
                s.init = *m;
            }
            scripts.push(s);
        }
        self.abc.scripts = scripts;
        let methods = std::mem::take(&mut self.abc.methods);
        self.abc.methods = methods.into_iter().enumerate().filter(|(m, _)| !deleted.contains(m)).map(|(_, mi)| mi).collect();
        self.abc.bodies = bodies;
        let mut body_of = vec![None; self.abc.methods.len()];
        for (i, b) in self.abc.bodies.iter().enumerate() {
            if let Some(s) = body_of.get_mut(b.method) {
                *s = Some(i);
            }
        }
        self.abc.body_of = body_of;
        self.deleted = Default::default();
    }
}

/// Re-encodes a body with some operands replaced, keeping branches and exceptions pointing
/// at the same instructions.
fn rewrite_code(b: &mut crate::abc::MethodBody, dialect: Dialect, f: &dyn Fn(Operand, i64) -> Option<i64>) {
    let Ok(code) = decode(&b.code, dialect) else { return };
    let mut changed = false;
    let mut out: Vec<Instruction> = code.clone();
    for i in &mut out {
        for k in 0..i.kinds.len().min(i.operands.len()) {
            if let Some(v) = f(i.kinds[k], i.operands[k]) {
                if v != i.operands[k] {
                    i.operands[k] = v;
                    changed = true;
                }
            }
        }
    }
    if !changed {
        return;
    }
    let lens: Vec<usize> = out.iter().map(|i| crate::abc::asm::encode(i).len()).collect();
    let mut new_off = Vec::with_capacity(out.len());
    let mut a = 0;
    for l in &lens {
        new_off.push(a);
        a += l;
    }
    let total_old = code.last().map_or(0, |i| i.offset + i.len);
    let map = |old: i64| -> Option<i64> {
        if old as usize == total_old {
            return Some(a as i64);
        }
        code.binary_search_by_key(&(old as usize), |i| i.offset).ok().map(|p| new_off[p] as i64)
    };
    for (p, i) in out.iter_mut().enumerate() {
        let old = &code[p];
        for k in 0..i.kinds.len() {
            match i.kinds[k] {
                Operand::Offset => {
                    if let Some(t) = map(old.offset as i64 + old.len as i64 + old.operands[k]) {
                        i.operands[k] = t - (new_off[p] + lens[p]) as i64;
                    }
                }
                Operand::CaseBaseoffset => {
                    if let Some(t) = map(old.offset as i64 + old.operands[k]) {
                        i.operands[k] = t - new_off[p] as i64;
                    }
                }
                Operand::OptCaseOffsets => {
                    for j in k + 1..i.operands.len() {
                        if let Some(t) = map(old.offset as i64 + old.operands[j]) {
                            i.operands[j] = t - new_off[p] as i64;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut bytes = Vec::new();
    for i in &out {
        bytes.extend(crate::abc::asm::encode(i));
    }
    for e in &mut b.exceptions {
        for v in [&mut e.start, &mut e.end, &mut e.target] {
            if let Some(n) = map(*v as i64) {
                *v = n as usize;
            }
        }
    }
    b.code = bytes;
}

/// Drops the instructions a script initializer uses to create class `class`:
/// the global object or `findpropstrict`, the ancestor scopes, `newclass`, the `popscope`s and
/// the store.
fn strip_class_init(b: &mut crate::abc::MethodBody, class: usize, dialect: Dialect) -> Option<()> {
    let code = decode(&b.code, dialect).ok()?;
    let k = code.iter().position(|i| i.code == op::NEW_CLASS && i.operands[0] as usize == class)?;
    let mut e = k + 1;
    while code.get(e)?.code == op::POP_SCOPE {
        e += 1;
    }
    if !matches!(code[e].code, op::INIT_PROPERTY | op::SET_PROPERTY | op::SET_SLOT) {
        return None;
    }
    let mut j = k.checked_sub(1)?;
    while matches!(code[j].code, op::GET_LEX | op::PUSH_SCOPE | op::DUP | op::PUSH_NULL) {
        j = j.checked_sub(1)?;
    }
    if !matches!(code[j].code, op::FIND_PROPERTY_STRICT | op::FIND_PROPERTY | op::GET_SCOPE_OBJECT | op::GET_GLOBAL_SCOPE) {
        return None;
    }
    let kept: Vec<usize> = (0..code.len()).filter(|&p| p < j || p > e).collect();
    let mut new_off = vec![0usize; code.len() + 1];
    let mut a = 0;
    for p in 0..code.len() {
        new_off[p] = a;
        if p < j || p > e {
            a += code[p].len;
        }
    }
    new_off[code.len()] = a;
    let total = code.last().map_or(0, |i| i.offset + i.len);
    let map = |old: i64| -> Option<i64> {
        if old as usize == total {
            return Some(a as i64);
        }
        code.binary_search_by_key(&(old as usize), |i| i.offset).ok().map(|p| new_off[p] as i64)
    };
    let mut bytes = Vec::new();
    for &p in &kept {
        let old = &code[p];
        let mut i = old.clone();
        for k in 0..i.kinds.len() {
            match i.kinds[k] {
                Operand::Offset => {
                    let t = map(old.offset as i64 + old.len as i64 + old.operands[k])?;
                    i.operands[k] = t - (new_off[p] + old.len) as i64;
                }
                Operand::CaseBaseoffset => i.operands[k] = map(old.offset as i64 + old.operands[k])? - new_off[p] as i64,
                Operand::OptCaseOffsets => {
                    for q in k + 1..i.operands.len() {
                        i.operands[q] = map(old.offset as i64 + old.operands[q])? - new_off[p] as i64;
                    }
                }
                _ => {}
            }
        }
        bytes.extend(crate::abc::asm::encode(&i));
    }
    for ex in &mut b.exceptions {
        for v in [&mut ex.start, &mut ex.end, &mut ex.target] {
            *v = map(*v as i64)? as usize;
        }
    }
    b.code = bytes;
    Some(())
}

/// The traits of a script pack, as `getPacks` groups them, and whether it is the script's
/// only public definition.
pub fn pack_of(abc: &Abc, script: usize, trait_index: Option<usize>) -> (Vec<usize>, bool) {
    let names = super::index::raw_names(abc);
    let traits = &abc.scripts[script].traits;
    let public = |i: usize| matches!(names.simple_ns_kind(traits[i].name), ns_kind::PACKAGE | ns_kind::PACKAGE_INTERNAL);
    let count = (0..traits.len()).filter(|&i| public(i)).count();
    let simple = count == 1;
    // A whole script is replaced like a script's only definition.
    let Some(t) = trait_index else { return ((0..traits.len()).collect(), true) };
    (crate::abc::scripts::pack_traits(abc, script, t), simple)
}

/// `FFDecAs3ScriptReplacer.replaceScript`: compiles `text` in place of a script pack of
/// `abcs[selected]` and returns the new ABC.
pub fn replace_script(abcs: &[Abc], selected: usize, script: usize, trait_index: Option<usize>, text: &str, document_class: Option<&str>) -> R<Abc> {
    replace_script_renamed(abcs, selected, script, trait_index, text, document_class, &[])
}

/// `replace_script` for text printed with `renames` (printed name, name in the ABC).
pub fn replace_script_renamed(abcs: &[Abc], selected: usize, script: usize, trait_index: Option<usize>, text: &str, document_class: Option<&str>, renames: &[(String, String)]) -> R<Abc> {
    let abc = &abcs[selected];
    let (traits, simple) = pack_of(abc, script, trait_index);
    let fail = |m: &str| super::CompileError { line: 0, message: m.into() };
    let classes: Vec<usize> = traits.iter().filter_map(|&t| match abc.scripts[script].traits[t].data {
        TraitData::Class { class, .. } => Some(class),
        _ => None,
    }).collect();
    if !simple && (trait_index.is_none() || !matches!(abc.scripts[script].traits[trait_index.unwrap()].data, TraitData::Class { .. })) {
        return Err(fail("Only a class can be compiled on its own in a script that defines several things"));
    }
    let others: Vec<Abc> = abcs.iter().enumerate().filter(|(i, _)| *i != selected).map(|(_, a)| a.clone()).collect();
    let mut cx = Cx::new(abc.clone(), others);
    cx.document_class = document_class.map(String::from);
    let mut oi = 0;
    for i in 0..abcs.len() {
        if i == selected {
            cx.index.add_abc(0, &cx.abc.clone(), AbcId::Sel, None);
        } else {
            let a = cx.others[oi].clone();
            cx.index.add_abc(0, &a, AbcId::Swf(oi), None);
            oi += 1;
        }
    }
    let ids = crate::abc::decomp::names::Idents::default();
    let (cns, dns) = (HashMap::new(), HashSet::new());
    let names = crate::abc::decomp::names::Names { abc, ids: &ids, custom_ns: &cns, display_ns: &dns };
    let mut script_name = format!("script_{script}");
    for &t in &traits {
        let tr = &abc.scripts[script].traits[t];
        if matches!(names.simple_ns_kind(tr.name), ns_kind::PACKAGE | ns_kind::PACKAGE_INTERNAL) {
            script_name = names.name(tr.name, &[], false, true);
            // A whole script is named after its first definition, the file the others include.
            if trait_index.is_none() {
                break;
            }
        }
    }
    let old_bodies = abc.bodies.len();
    let mut class_pos = abc.instances.len();
    for &t in &traits {
        if let TraitData::Class { class, .. } = abc.scripts[script].traits[t].data {
            class_pos = class + 1;
        }
    }
    let trace = std::env::var("FFDEC_AS3_TRACE").is_ok();
    if trace {
        eprintln!("index built");
    }
    cx.delete_pack(script, &traits, simple);
    if !simple {
        let init = abc.scripts[script].init;
        let bi = abc.body_of.get(init).copied().flatten().ok_or_else(|| fail("The script has no initializer"))?;
        let dialect = Dialect::of(abc.major, abc.minor);
        for &c in &classes {
            strip_class_init(&mut cx.abc.bodies[bi], c, dialect).ok_or_else(|| fail("The script initializer does not set up this class in a way that can be replaced"))?;
        }
    }
    cx.refresh();
    if trace {
        eprintln!("refreshed");
    }
    let new_index = cx.abc.scripts.len();
    let parsed = {
        let mut p = Parser::new(&mut cx, text);
        p.replacements = renames.iter().cloned().collect();
        p.parse_script(&format!("{script_name}.as"), script)?
    };
    if trace {
        eprintln!("parsed");
    }
    cx.abc.scripts.push(crate::abc::Script { init: 0, traits: Vec::new() });
    cx.generate_script(&parsed, new_index, class_pos)?;
    if trace {
        eprintln!("generated");
    }
    cx.finalize();
    if simple {
        let s = cx.abc.scripts.remove(new_index);
        cx.abc.scripts[script] = s;
        cx.deleted.scripts.remove(&script);
        cx.deleted.traits.retain(|(o, _)| *o != Owner::Script(script));
    }
    cx.pack();
    if !simple {
        // Before the old script, so the entry script stays last.
        let s = cx.abc.scripts.pop().unwrap();
        cx.abc.scripts.insert(script, s);
    }
    let generated: Vec<usize> = cx.body_map.iter().enumerate().filter(|(i, _)| *i >= old_bodies).filter_map(|(_, b)| *b).collect();
    for b in generated {
        if let Err((at, what)) = crate::abc::stats::fit_body(&mut cx.abc, b) {
            let _ = at;
            return Err(super::CompileError { line: 0, message: format!("the compiled code would not pass the player's verifier ({what})") });
        }
    }
    Ok(cx.abc)
}

pub fn unused_code(_: Code) {}
