//! FFDec's `AVM2Graph`: exception handling, finally blocks and strict-equality switches.

use super::code::op;
use super::graph::*;
use super::item::*;
use super::item::PartId as P;
use super::ld::Ld;
use super::parts::*;
use super::props::*;
use super::stack::Stack;
use crate::abc::Exception;

pub const FINALLY_STACK: i32 = 0;
pub const FINALLY_REGISTER: i32 = 1;
pub const FINALLY_INLINED: i32 = 2;
pub const FINALLY_UNKNOWN: i32 = -1;

pub fn is_finally(e: &Exception) -> bool {
    e.var_name == 0 && e.type_name == 0
}

pub fn set_local_reg(it: &It) -> Option<i32> {
    match &it.borrow().k {
        K::SetLocal { reg, .. } => Some(*reg),
        _ => None,
    }
}

pub fn local_reg(it: &It) -> Option<i32> {
    match &it.borrow().k {
        K::LocalReg { reg, .. } => Some(*reg),
        _ => None,
    }
}

pub fn value(it: &It) -> It {
    it.borrow().val()
}

pub fn is_set_type(it: &It) -> bool {
    matches!(it.borrow().k, K::SetLocal { .. } | K::SetProperty { .. } | K::SetSlot { .. } | K::SetSuper { .. } | K::InitProperty { .. })
}

/// `SetTypeAVM2Item.getObject()`.
pub fn set_type_object(it: &It) -> It {
    let b = it.borrow();
    let (src, ls) = (b.src, b.line_start);
    match &b.k {
        K::SetLocal { reg, .. } => {
            let t = return_type(b.value.as_ref().unwrap());
            mk(K::LocalReg { reg: *reg, computed: None, ty: t, is_ct: false }, src, ls, prec::PRIMARY, None)
        }
        K::SetProperty { obj, prop, ty, call_ty, is_static, .. } | K::InitProperty { obj, prop, ty, call_ty, is_static, .. } => {
            mk(K::GetProperty { obj: obj.clone(), prop: prop.clone(), ty: ty.clone(), call_ty: call_ty.clone(), is_static: *is_static, null_cond: false }, src, ls, prec::PRIMARY, None)
        }
        K::SetSlot { slot_name, scope, slot_obj, idx, ty, .. } => {
            mk(K::GetSlot { slot_name: *slot_name, scope: scope.clone(), slot_obj: slot_obj.clone(), idx: *idx, slot_ty: ty.clone() }, src, ls, prec::PRIMARY, None)
        }
        K::SetSuper { obj, prop, ty, call_ty, is_static, .. } => {
            mk(K::GetSuper { obj: obj.clone(), prop: prop.clone(), ty: ty.clone(), call_ty: call_ty.clone(), is_static: *is_static }, src, ls, prec::PRIMARY, None)
        }
        _ => panic!("not a set type item"),
    }
}

fn int_a(it: &It) -> Option<i32> {
    match &it.borrow().k {
        K::IntegerValueA { v, .. } => Some(*v),
        _ => None,
    }
}

fn last(l: &List) -> Option<It> {
    l.borrow().last().cloned()
}

impl<'t, 'c, 'a> G<'t, 'c, 'a> {
    pub fn exceptions(&self) -> &Vec<Exception> {
        &self.tr.code.exceptions
    }

    fn adr_near(&self, a: usize) -> i32 {
        self.tr.code.adr2pos_nearest(a as i64).map_or(-1, |x| x as i32)
    }

    fn adr(&self, a: usize) -> i32 {
        self.tr.code.adr2pos(a as i64).map_or(-1, |x| x as i32)
    }

    fn map_has_value<K>(&self, m: &std::collections::BTreeMap<K, P>, p: P) -> bool {
        m.values().any(|&v| self.peq(v, p))
    }

    fn fj_get(&self, ld: &Ld, p: P) -> Option<P> {
        ld.finally_jumps.borrow().iter().find(|(k, _)| self.peq(*k, p)).map(|(_, v)| *v)
    }

    /// `checkPart` without output.
    pub fn check_part(&self, _stack: Option<&St>, ld: &Ld, prev: Option<P>, part: P) -> Option<P> {
        if self.map_has_value(&ld.finally_throw_parts.borrow(), part) {
            return None;
        }
        if let Some(prev) = prev {
            if !ld.in_get_loops && self.map_has_value(&ld.ignored_switches.borrow(), prev) {
                return None;
            }
            if let Some(t) = self.fj_get(ld, prev) {
                return Some(t);
            }
        }
        Some(part)
    }

    /// `checkPartWithOutput`.
    pub fn check_part_with_output(&self, output: Option<&List>, stack: &St, ld: &mut Ld, prev: Option<P>, part: P) -> Option<P> {
        if self.map_has_value(&ld.finally_throw_parts.borrow(), part) {
            return None;
        }
        let Some(prev) = prev else { return Some(part) };
        if !ld.in_get_loops && self.map_has_value(&ld.ignored_switches.borrow(), prev) {
            return None;
        }
        let Some(target) = self.fj_get(ld, prev) else { return Some(part) };
        let mut switched_reg = -1;
        for &gp in &self.parts[target].refs {
            let found = ld.ignored_switches.borrow().iter().find(|(_, &v)| v == gp).map(|(k, _)| *k);
            if self.map_has_value(&ld.ignored_switches.borrow(), gp) {
                let fi = found.unwrap_or(-1);
                switched_reg = ld.switched_regs.borrow().get(&fi).copied().unwrap_or(-1);
                break;
            }
        }
        if let Some(out) = output {
            if switched_reg > -1 {
                if last(out).and_then(|x| set_local_reg(&x)) == Some(switched_reg) {
                    out.borrow_mut().pop();
                }
            }
            let fi = ld.finally_jumps_to_index.borrow().iter().find(|(k, _)| self.peq(*k, prev)).map(|(_, v)| *v).unwrap_or(-1);
            if ld.finally_double_push.borrow().contains(&fi) {
                let dp = ld.finally_index_to_default.borrow().get(&fi).copied().flatten();
                if dp.is_none() || dp != Some(target) {
                    stack.borrow_mut().push(bare(K::Any, prec::NOPRECEDENCE));
                }
            }
            if switched_reg == -1 {
                let top_int = !stack.borrow().is_empty() && stack.borrow().items.last().is_some_and(|x| int_a(x).is_some());
                if top_int {
                    stack.borrow_mut().pop();
                } else if last(out).is_some_and(|x| int_a(&x).is_some()) {
                    out.borrow_mut().pop();
                }
            }
        }
        Some(target)
    }

    pub fn is_part_empty(&self, part: P) -> bool {
        let p = &self.parts[part];
        if p.next.len() > 1 || p.start < 0 {
            return false;
        }
        for ip in p.start..=p.end {
            let Some(ins) = self.tr.code.ins.get(ip as usize) else { return false };
            if ins.code != op::DEBUGLINE && ins.code != op::JUMP {
                return false;
            }
        }
        true
    }

    pub fn part_is_switch(&self, part: P) -> bool {
        let e = self.parts[part].end;
        e >= 0 && (e as usize) < self.tr.code.len() && self.tr.code.ins[e as usize].code == op::LOOKUPSWITCH
    }

    fn linear_lookup_switch_target(&self, part: P) -> Option<P> {
        let mut visited: Vec<P> = Vec::new();
        let mut p = part;
        while !self.pcontains(&visited, p) {
            visited.push(p);
            if self.part_is_switch(p) {
                return Some(p);
            }
            if self.parts[p].next.len() != 1 {
                return None;
            }
            p = self.parts[p].next[0];
        }
        None
    }

    pub fn can_handle_visited(&self, ld: &Ld, part: P) -> bool {
        for (i, ex) in self.exceptions().iter().enumerate() {
            if ld.parsed_exceptions.borrow().contains(&i) {
                continue;
            }
            if self.parts[part].start == self.adr_near(ex.start) {
                return false;
            }
        }
        true
    }

    pub fn can_handle_loop(&self, ld: &Ld, part: P, pc: &Pc) -> bool {
        let Some(tbl) = pc.loops.iter().copied().find(|&l| self.arena[l].cont == Some(part) && self.arena[l].phase == 0) else { return true };
        let mut in_try = false;
        for (i, ex) in self.exceptions().iter().enumerate() {
            if ld.parsed_exceptions.borrow().contains(&i) {
                continue;
            }
            let (fs, fe) = (self.adr_near(ex.start), self.adr_near(ex.end));
            if self.parts[part].start == fs {
                in_try = true;
                for be in self.arena[tbl].back_edges.iter() {
                    if self.parts[be].start < fs || self.parts[be].start >= fe {
                        return true;
                    }
                }
            }
        }
        !in_try
    }

    pub fn prepare_branch_local_data(&self, ld: &Ld) -> Ld {
        let mut r = ld.copy();
        let items = ld.local_scope_stack.borrow().items.clone();
        r.new_local_scope();
        r.local_scope_stack.borrow_mut().items = items;
        r
    }

    pub fn get_if_expression(&self, stack: &St, output: &List) -> It {
        let result = stack.borrow_mut().pop();
        let mark = stack.borrow().get_mark("firstSetLocal");
        if let Some(sl) = mark {
            if matches!(sl.borrow().k, K::SetLocal { directly_caused_by_dup: true, .. }) {
                output.borrow_mut().push(clone_item(&sl));
                if let K::SetLocal { hide_value, .. } = &mut sl.borrow_mut().k {
                    *hide_value = true;
                }
            }
        }
        result
    }

    pub fn handle_ternar(&self, ternar: It) -> It {
        if self.tr.swf_version < 50 {
            return ternar;
        }
        let (expr, on_true, on_false) = match &ternar.borrow().k {
            K::Ternar { expr, on_true, on_false } => (expr.clone(), on_true.clone(), on_false.clone()),
            _ => return ternar.clone(),
        };
        if let K::Bin { op: Bin::Eq, l, r } = &expr.borrow().k {
            if matches!(r.borrow().k, K::Null) {
                let nc = not_coerced(&on_false);
                if let K::GetProperty { obj, .. } = &nc.borrow().k {
                    if super::eq::eq(obj, l) {
                        drop(expr.borrow());
                    }
                }
                let same_obj = matches!(&nc.borrow().k, K::GetProperty { obj, .. } if super::eq::eq(obj, l));
                if same_obj {
                    if let K::GetProperty { null_cond, .. } = &mut nc.borrow_mut().k {
                        *null_cond = true;
                    }
                    return nc;
                }
            }
        }
        if let K::Bin { op: Bin::Neq, l, r } = &expr.borrow().k {
            if matches!(r.borrow().k, K::Null) && super::eq::eq(&on_true, l) {
                let (src, ls) = (ternar.borrow().src, ternar.borrow().line_start);
                return bin(Bin::NullCoalesce, src, ls, on_true.clone(), on_false.clone());
            }
        }
        ternar
    }

    pub fn check_switch(&self, ld: &Ld, sw: &It, other_sides: &[It], output: &List) {
        let Some(sl) = last(output) else { return };
        let Some(reg) = set_local_reg(&sl) else { return };
        let Some(ip) = sl.borrow().src else { return };
        let mut usages = ld.set_local_usages_of(ip as i32);
        let mut other_reg = false;
        for o in other_sides {
            if let Some(r) = local_reg(o) {
                if r != reg {
                    break;
                }
                if let Some(gip) = o.borrow().src {
                    usages.remove(&(gip as i32));
                }
                other_reg = true;
            }
        }
        if !other_reg {
            return;
        }
        if usages.is_empty() {
            output.borrow_mut().pop();
            let v = value(&sl);
            if let K::Switch { obj, .. } = &mut sw.borrow_mut().k {
                *obj = v;
            }
        }
    }

    // Exceptions.

    fn search_first_part_outside_try_catch(&mut self, ld: &Ld, ex: &Exception, pc: &Pc) -> Option<P> {
        let start_ip = self.adr_near(ex.start);
        let end_ip = self.adr_near(ex.end);
        let all = pc.all_parts.iter();
        let start_part = self.search_part(start_ip, &all)?;
        let mut sub = ld.copy();
        sub.default_parts = super::ld::shared(Default::default());
        sub.default_ways = super::ld::shared(Default::default());
        sub.finally_index_to_default = super::ld::shared(Default::default());
        sub.finally_double_push = super::ld::shared(Default::default());
        sub.finally_jumps = super::ld::shared(Default::default());
        sub.finally_jumps_to_index = super::ld::shared(Default::default());
        sub.finally_throw_parts = super::ld::shared(Default::default());
        sub.ignored_switches = super::ld::shared(Default::default());
        let mut reachable = PartList::default();
        let loops = pc.loops.clone();
        self.get_reachable_parts(&sub, start_part, &mut reachable, &loops, &[]);
        reachable.0.into_iter().find(|&r| self.parts[r].start < start_ip || self.parts[r].start >= end_ip)
    }

    fn nearest_non_empty_part(&self, part: P) -> P {
        let mut p = part;
        while self.is_part_empty(p) {
            p = self.parts[p].next[0];
        }
        p
    }

    fn get_catched_exception_ids(&mut self, ld: &Ld, part: P, prev_ids: &[i32], catched: &mut Vec<i32>, pc: &Pc) -> i32 {
        let addr = self.tr.code.pos2adr(self.parts[part].start.max(0) as usize);
        let mut max_end: i64 = -1;
        let mut finallys: Vec<i32> = Vec::new();
        let exs = self.exceptions().clone();
        let fixed = |g: &Self, a: usize| g.tr.code.pos2adr(g.adr_near(a).max(0) as usize);
        for (e, ex) in exs.iter().enumerate() {
            let fs = fixed(self, ex.start);
            let fe = fixed(self, ex.end);
            if prev_ids.contains(&(e as i32)) || addr != fs {
                continue;
            }
            if is_finally(ex) {
                if fe >= max_end {
                    finallys.push(e as i32);
                }
            } else if fe > max_end {
                catched.clear();
                max_end = fe;
                catched.push(e as i32);
                finallys.retain(|&f| exs[f as usize].end as i64 >= fe);
            } else if fe == max_end {
                catched.push(e as i32);
            }
        }
        finallys.sort_by(|a, b| exs[*b as usize].end.cmp(&exs[*a as usize].end));
        let outside = if catched.is_empty() { None } else { self.search_first_part_outside_try_catch(ld, &exs[catched[0] as usize], pc) };
        if !finallys.is_empty() {
            let mut max = 0;
            let mut max_f = -1;
            for &f in &finallys {
                let fe = fixed(self, exs[f as usize].end);
                if fe > max {
                    max = fe;
                    max_f = f;
                }
            }
            finallys = vec![max_f];
        }
        let mut finally_index = -1;
        for &e in &finallys {
            let Some(outside) = outside else { break };
            if catched.is_empty() {
                break;
            }
            let ne = self.nearest_non_empty_part(outside);
            let of = self.search_first_part_outside_try_catch(ld, &exs[e.max(0) as usize], pc);
            if Some(ne) == of {
                finally_index = e;
                break;
            }
            if self.parts[ne].next.len() == 1 && Some(self.parts[ne].next[0]) == of {
                let first = &self.tr.code.ins[self.parts[ne].start as usize];
                let only = matches!(first.code, op::PUSHBYTE | op::JUMP | op::NOP);
                if only {
                    finally_index = e;
                    break;
                }
            }
        }
        if finally_index == -1 && !finallys.is_empty() {
            catched.clear();
            finally_index = finallys[0];
        }
        finally_index
    }

    fn find_nearest_part_outside_catch(&self, catch_parts: &PartSet) -> Option<P> {
        for p in catch_parts.iter() {
            for &n in &self.parts[p].next {
                if !catch_parts.contains(&self.parts, n) {
                    return Some(n);
                }
            }
        }
        None
    }

    /// `checkTry` (AVM2's `checkPartOutput`).
    #[allow(clippy::too_many_arguments)]
    pub fn check_part_output(&mut self, pc: &mut Pc, current_ret: &List, visited: &mut Vec<P>, ld: &mut Ld, stack: &St, _parent: Option<P>, part: P, stop: &Stops, _current_loop: Option<usize>, recursion: usize) -> bool {
        let prev_ids = ld.parsed_exception_ids.borrow().clone();
        let mut catched_ids: Vec<i32> = Vec::new();
        let finally_index = self.get_catched_exception_ids(ld, part, &prev_ids, &mut catched_ids, pc);
        let exs = self.exceptions().clone();
        let mut catched: Vec<usize> = catched_ids.iter().map(|&e| e as usize).collect();
        let finally_ex = if finally_index > -1 { Some(exs[finally_index as usize].clone()) } else { None };
        if finally_ex.is_some() {
            catched.push(finally_index as usize);
            catched_ids.push(finally_index);
        }
        let switched_reg = if finally_index != -1 { ld.switched_regs.borrow().get(&finally_index).copied().unwrap_or(-1) } else { -1 };
        if catched.is_empty() {
            return false;
        }
        for ts in pc.throw_states.iter_mut() {
            if catched_ids.contains(&ts.exception_id) || ts.exception_id == finally_index {
                ts.state = 1;
            }
        }
        ld.parsed_exceptions.borrow_mut().extend(catched.iter().copied());
        ld.parsed_exception_ids.borrow_mut().extend(catched_ids.iter().copied());
        if finally_ex.is_some() {
            if let Some(i) = catched.iter().position(|&x| x == finally_index as usize) {
                catched.remove(i);
            }
            if let Some(i) = catched_ids.iter().position(|&x| x == finally_index) {
                catched_ids.remove(i);
            }
        }
        if finally_index > -1 {
            ld.parsed_exception_ids.borrow_mut().push(finally_index);
        }
        let mut try_commands = empty_list();
        let mut catch_commands: Vec<List> = Vec::new();
        let mut finally_commands = empty_list();
        let all: Vec<P> = pc.all_parts.iter();
        let mut after_part: Option<P> = None;
        let mut to_common: Vec<P> = vec![part];
        for &e in &catched {
            if let Some(p) = self.search_part(self.adr(exs[e].target), &all) {
                to_common.push(p);
            }
        }
        if to_common.len() > 1 {
            after_part = self.get_most_common_part(ld, &to_common, pc);
        }
        if !catched.is_empty() && after_part.is_none() {
            after_part = self.search_first_part_outside_try_catch(ld, &exs[catched[0]], pc);
        }
        let mut catch_after: Vec<P> = Vec::new();
        if after_part.is_none() {
            for &e in &catched {
                for ts in &pc.throw_states {
                    if ts.exception_id == e as i32 {
                        if let Some(pa) = self.find_nearest_part_outside_catch(&ts.catch_parts) {
                            if !stop.iter().any(|(s, _)| self.peq(*s, pa)) {
                                catch_after.push(pa);
                            }
                        }
                    }
                }
            }
            if catch_after.len() == 1 {
                after_part = Some(catch_after[0]);
            } else if catch_after.len() > 1 {
                after_part = self.get_most_common_part(ld, &catch_after, pc);
            }
        }
        if after_part == Some(part) {
            after_part = None;
        }
        let mut ex_after = after_part;
        let mut stack = stack.clone();
        if finally_ex.is_none() {
            let mut stop2 = stop.clone();
            if let Some(a) = after_part {
                stop2.push((a, StopKind::Other));
            }
            for &l in &pc.loops {
                if self.arena[l].cont == Some(part) && self.arena[l].phase == 1 {
                    self.arena[l].phase = 4;
                }
            }
            stack = st_clone(&stack);
            try_commands = self.print_graph(pc, visited, ld, &stack, None, Some(part), Some(stop2), None, 0);
        }
        let mut inlined = false;
        let mut finally_as_unnamed = false;
        let finally_target_items = empty_list();
        let mut default_part: Option<P> = None;
        if let Some(fex) = &finally_ex {
            let switch_part = ld.ignored_switches.borrow().get(&finally_index).copied();
            if switch_part.is_some() {
                default_part = ld.default_parts.borrow().get(&finally_index).copied();
            }
            ld.finally_index_to_default.borrow_mut().insert(finally_index, default_part);
            let ftt = self.search_part(self.adr(fex.target), &all).expect("finally target part");
            let finally_part = self.parts[ftt].next.first().copied();
            let st2 = st_clone(&stack);
            st2.borrow_mut().clear();
            st2.borrow_mut().items.push(bare(K::ExceptionItem(fex.clone()), prec::NOPRECEDENCE));
            let mut ld2 = ld.copy();
            ld2.new_local_scope();
            self.translate_part(&finally_target_items, &mut ld2, ftt, &st2);
            if last(&finally_target_items).is_some_and(|x| matches!(x.borrow().k, K::Throw)) {
                let mut empty = true;
                for it in finally_target_items.borrow().iter() {
                    let ok = match &it.borrow().k {
                        K::SetTemporary { .. } | K::SetLocal { .. } => matches!(not_coerced(it.borrow().value.as_ref().unwrap()).borrow().k, K::ExceptionItem(_)),
                        K::Throw | K::IntegerValueA { .. } => true,
                        _ => false,
                    };
                    if !ok {
                        empty = false;
                        break;
                    }
                }
                if !empty {
                    inlined = true;
                }
            }
            let mut try_stop = stop.clone();
            if let Some(fp) = finally_part {
                try_stop.push((fp, StopKind::Other));
            }
            if let Some(dp) = default_part {
                try_stop.push((dp, StopKind::Other));
            }
            if after_part.is_none() {
                after_part = self.search_first_part_outside_try_catch(ld, fex, pc);
            }
            if let Some(a) = after_part {
                try_stop.push((a, StopKind::Other));
            }
            if switch_part.is_some() {
                after_part = default_part;
            } else {
                if !inlined {
                    after_part = None;
                }
                finally_as_unnamed = ld.finally_kinds.borrow().get(&finally_index).copied() == Some(FINALLY_UNKNOWN);
            }
            if let Some(&pdp) = ld.push_default_part.borrow().get(&finally_index) {
                ex_after = Some(pdp);
                try_stop.push((pdp, StopKind::Other));
            }
            try_commands = self.print_graph(pc, visited, ld, &stack, None, Some(part), Some(try_stop), None, 0);
            self.make_all_commands(&try_commands, &stack);
            self.process_ifs(&try_commands);
            if last(&try_commands).is_some_and(|x| is::push(&x) && int_a(&value(&x)).is_some()) {
                try_commands.borrow_mut().pop();
            }
            let mut fstop = stop.clone();
            if let Some(sp) = switch_part {
                fstop.push((sp, StopKind::Other));
            }
            if let Some(fp) = finally_part {
                finally_commands = self.print_graph(pc, visited, ld, &stack, None, Some(fp), Some(fstop), None, 0);
            }
            if let Some(sp) = switch_part {
                self.translate_part(&finally_commands, ld, sp, &stack);
                if last(&finally_commands).is_some_and(|x| matches!(x.borrow().k, K::SetTemporary { .. }) && is::pop(&value(&x))) {
                    finally_commands.borrow_mut().pop();
                }
                stack.borrow_mut().pop();
                stack.borrow_mut().pop();
            }
        }
        let mut catch_exs: Vec<Exception> = Vec::new();
        for &e in &catched {
            let ex = exs[e].clone();
            let st2 = st_clone(&stack);
            st2.borrow_mut().clear();
            st2.borrow_mut().items.push(bare(K::ExceptionItem(ex.clone()), prec::NOPRECEDENCE));
            let tpos = self.adr(ex.target);
            let catch_part = all.iter().copied().find(|&p| self.parts[p].start == tpos);
            let mut ld2 = ld.copy();
            ld2.new_local_scope();
            let mut stop2 = stop.clone();
            if let Some(a) = ex_after {
                stop2.push((a, StopKind::Other));
            }
            if let Some(dp) = default_part {
                stop2.push((dp, StopKind::Other));
            }
            for &p in &catch_after {
                stop2.push((p, StopKind::Other));
            }
            let cmds = self.print_graph(pc, visited, &mut ld2, &st2, None, catch_part, Some(stop2), None, 0);
            st2.borrow_mut().finish_block(&cmds);
            let mut pos = 0;
            if cmds.borrow().first().is_some_and(|x| matches!(x.borrow().k, K::With { .. })) {
                pos += 1;
            }
            while cmds.borrow().len() > pos {
                let x = cmds.borrow()[pos].clone();
                let is_ex_temp = matches!(x.borrow().k, K::SetTemporary { .. }) && matches!(not_coerced(&value(&x)).borrow().k, K::ExceptionItem(_));
                if !is_ex_temp {
                    break;
                }
                cmds.borrow_mut().remove(pos);
            }
            'with: while let Some(first) = cmds.borrow().first().cloned() {
                let scope = match &first.borrow().k {
                    K::With { scope, .. } => scope.clone(),
                    _ => break,
                };
                if let Some(reg) = local_reg(&scope) {
                    let items = ld.local_scope_stack.borrow().items.clone();
                    for item in items {
                        if let K::WithObject { scope: ws } = &item.borrow().k {
                            if set_local_reg(ws) == Some(reg) {
                                cmds.borrow_mut().remove(0);
                                if let (Some(sip), Some(gip)) = (ws.borrow().src, scope.borrow().src) {
                                    if let Some(s) = ld.set_local_usages.borrow_mut().get_mut(&(sip as i32)) {
                                        s.remove(&(gip as i32));
                                    }
                                }
                                continue 'with;
                            }
                        }
                    }
                }
                break;
            }
            if last(&cmds).and_then(|x| set_local_reg(&x)) == Some(switched_reg) && switched_reg != -1 {
                cmds.borrow_mut().pop();
            } else if switched_reg == -1 && last(&cmds).and_then(|x| set_local_reg(&x)) == Some(-1) {
                cmds.borrow_mut().pop();
            }
            catch_commands.push(cmds);
            catch_exs.push(ex);
        }
        for ts in pc.throw_states.iter_mut() {
            if catched_ids.contains(&ts.exception_id) || ts.exception_id == finally_index {
                ts.state = 2;
            }
        }
        if !inlined && catch_commands.is_empty() && finally_commands.borrow().is_empty() && try_commands.borrow().is_empty() {
            return false;
        }
        if switched_reg != -1 && last(current_ret).and_then(|x| set_local_reg(&x)) == Some(switched_reg) {
            current_ret.borrow_mut().pop();
        }
        if !finally_as_unnamed && !inlined && catch_exs.is_empty() && finally_commands.borrow().is_empty() {
            let v = try_commands.borrow().clone();
            current_ret.borrow_mut().extend(v);
            return true;
        }
        if finally_as_unnamed {
            catch_exs.push(finally_ex.clone().unwrap());
            catch_commands.push(finally_commands.clone());
            finally_commands = empty_list();
        }
        let mk_try = |t: List, ce: Vec<Exception>, cc: Vec<List>, f: List| mk(K::Try { try_cmds: t, catch_ex: ce, catch_cmds: cc, finally: Some(f), fin_catch_name: String::new() }, None, None, prec::NOPRECEDENCE, None);
        let mut try_item = mk_try(try_commands.clone(), catch_exs.clone(), catch_commands.clone(), finally_commands.clone());
        if inlined {
            let parent_try = if catch_exs.is_empty() && finally_commands.borrow().is_empty() { copy_list(&try_commands) } else { new_list(vec![try_item.clone()]) };
            try_item = mk_try(parent_try, vec![finally_ex.clone().unwrap()], vec![finally_target_items.clone()], empty_list());
        }
        let (tc, cc, fc) = match &try_item.borrow().k {
            K::Try { try_cmds, catch_cmds, finally, .. } => (try_cmds.clone(), catch_cmds.clone(), finally.clone().unwrap()),
            _ => unreachable!(),
        };
        if cc.is_empty() && !fc.borrow().is_empty() && tc.borrow().len() == 1 {
            let sub = tc.borrow()[0].clone();
            let sub_parts = match &sub.borrow().k {
                K::Try { try_cmds, catch_ex, catch_cmds, finally, .. } if finally.as_ref().is_none_or(|f| f.borrow().is_empty()) => Some((try_cmds.clone(), catch_ex.clone(), catch_cmds.clone())),
                _ => None,
            };
            if let Some((t, ce, ccs)) = sub_parts {
                try_item = mk_try(t, ce, ccs, fc.clone());
            }
        }
        current_ret.borrow_mut().push(try_item.clone());
        let (tc, cc, fc) = match &try_item.borrow().k {
            K::Try { try_cmds, catch_cmds, finally, .. } => (try_cmds.clone(), catch_cmds.clone(), finally.clone().unwrap()),
            _ => unreachable!(),
        };
        self.process_ifs(&tc);
        self.process_ifs(&fc);
        for c in &cc {
            self.process_ifs(c);
        }
        let mut blocks: Vec<List> = vec![tc.clone()];
        blocks.extend(cc.iter().cloned());
        let all_exit = blocks.iter().all(|b| last(b).is_some_and(|l| is::exit(&l) || is::continue_(&l) || is::break_(&l) || matches!(l.borrow().k, K::Goto { .. })));
        if all_exit {
            let mut counts: std::collections::HashMap<i64, i32> = Default::default();
            let mut max_cnt: Option<It> = None;
            let mut max = 0;
            for b in &blocks {
                let l = last(b).unwrap();
                if let Some(id) = super::structure::cont_id(&l) {
                    let c = counts.entry(id).or_insert(0);
                    *c += 1;
                    if *c > max {
                        max = *c;
                        max_cnt = Some(l.clone());
                    }
                }
            }
            if let Some(mc) = max_cnt {
                let mid = super::structure::cont_id(&mc);
                for b in &blocks {
                    let l = last(b).unwrap();
                    if super::structure::cont_id(&l).is_some() && super::structure::cont_id(&l) == mid {
                        b.borrow_mut().pop();
                    }
                }
                current_ret.borrow_mut().push(mc);
            }
        }
        if let Some(a) = after_part {
            if finally_index > -1 && ld.finally_double_push.borrow().contains(&finally_index) {
                stack.borrow_mut().push(bare(K::Any, prec::NOPRECEDENCE));
            }
            self.print_graph(pc, visited, ld, &stack, None, Some(a), Some(stop.clone()), Some(current_ret.clone()), recursion);
        }
        true
    }

    /// `check`: switches compiled as chains of `ifstricteq`/`ifstrictne`.
    #[allow(clippy::too_many_arguments)]
    pub fn check(&mut self, pc: &mut Pc, current_ret: &List, visited: &mut Vec<P>, ld: &mut Ld, stack: &St, _parent: Option<P>, part: P, stop: &Stops, output: &List) -> Option<List> {
        let top_strict = stack.borrow().items.last().is_some_and(|x| matches!(x.borrow().k, K::Bin { op: Bin::StrictEq | Bin::StrictNeq, .. }));
        if self.parts[part].next.len() != 2 || stack.borrow().is_empty() || !top_strict {
            return None;
        }
        let switch_start = Some(self.parts[part].start as usize);
        let mut switched: Option<It> = None;
        if let Some(l) = last(output) {
            if is::set_local(&l) {
                switched = Some(value(&l));
            }
        }
        let mut left: Vec<It> = Vec::new();
        let mut right: Vec<It> = Vec::new();
        let mut outs: Vec<Vec<It>> = vec![Vec::new()];
        let first_set = stack.borrow_mut().pop();
        let sides = |x: &It| match &x.borrow().k {
            K::Bin { l, r, .. } => (l.clone(), r.clone()),
            _ => unreachable!(),
        };
        let (l0, r0) = sides(&first_set);
        left.push(l0);
        right.push(r0);
        let orig_part = part;
        let mut part = part;
        let mut case_body: Vec<P> = Vec::new();
        let mut branch_num;
        if self.tr.code.ins[self.parts[part].end as usize].code == op::IFSTRICTNE {
            branch_num = 0;
            case_body.push(self.parts[part].next[1]);
        } else {
            branch_num = 1;
            case_body.push(self.parts[part].next[0]);
        }
        let mut cnt = 1;
        let mut branch_count = 2;
        let ignored: Vec<P> = pc.loops.iter().filter_map(|&l| self.arena[l].cont).collect();
        loop {
            let out = empty_list();
            if self.pcontains(&ignored, part) {
                break;
            }
            let nps = self.parts[part].next.clone();
            if nps.len() == branch_count {
                let pnd = nps[branch_num];
                let mut reached = false;
                for &cb in &case_body {
                    if cb == pnd || self.leads_to(ld, cb, pnd, pc, false) {
                        reached = true;
                        break;
                    }
                }
                if reached {
                    break;
                }
            }
            if nps.len() != branch_count {
                break;
            }
            let nb = nps[branch_num];
            let len = self.code_len();
            let nb_start = if self.parts[nb].start >= len { len - 1 } else { self.parts[nb].start };
            let last_code = self.tr.code.ins[self.last_part_ip(nb)].code;
            if self.parts[nb].height() == 2 && self.tr.code.ins[nb_start as usize].code == op::NOP && last_code == op::JUMP {
                part = nb;
                branch_num = 0;
                branch_count = 1;
            } else if last_code == op::IFSTRICTEQ || last_code == op::IFSTRICTNE {
                let top = self.translate_part_get_stack(ld, nb, stack, &out);
                let want = if last_code == op::IFSTRICTEQ { Bin::StrictEq } else { Bin::StrictNeq };
                if !matches!(&top.borrow().k, K::Bin { op, .. } if *op == want) {
                    break;
                }
                cnt += 1;
                part = nb;
                while self.parts[part].next.len() == 1 && self.parts[self.parts[part].next[0]].refs.len() == 1 {
                    part = self.parts[part].next[0];
                }
                if want == Bin::StrictEq {
                    case_body.push(self.parts[part].next[0]);
                    branch_num = 1;
                } else {
                    case_body.push(self.parts[part].next[1]);
                    branch_num = 0;
                }
                outs.push(out.borrow().clone());
                let (l, r) = sides(&top);
                left.push(l);
                right.push(r);
                branch_count = 2;
            } else {
                break;
            }
        }
        if let Some(ls) = self.linear_lookup_switch_target(self.parts[part].next[branch_num]) {
            let all_same = case_body.iter().all(|&cb| self.linear_lookup_switch_target(cb) == Some(ls));
            if all_same && pc.loops.iter().any(|&l| self.arena[l].body.contains(&self.parts, ls)) {
                stack.borrow_mut().push(first_set);
                return None;
            }
        }
        if case_body.len() == 2 {
            let mut is_if = false;
            for r in self.parts[part].refs.clone() {
                if r != orig_part && !self.leads_to(ld, orig_part, r, pc, false) {
                    is_if = true;
                    break;
                }
            }
            if !is_if {
                for r in self.parts[case_body[1]].refs.clone() {
                    if r != orig_part && !self.leads_to(ld, orig_part, r, pc, false) {
                        is_if = true;
                        break;
                    }
                }
            }
            if is_if {
                stack.borrow_mut().push(first_set);
                return None;
            }
        }
        if case_body.len() < 2 {
            stack.borrow_mut().push(first_set);
            return None;
        }
        let reg_of = |v: &[It]| -> i32 {
            let mut r = -1;
            for x in v {
                if r == -3 {
                    break;
                }
                match local_reg(x) {
                    Some(reg) => {
                        if r == -1 {
                            r = reg;
                        } else if r != reg {
                            r = -2;
                        }
                    }
                    None => r = -3,
                }
            }
            r
        };
        let left_reg = reg_of(&left);
        let right_reg = reg_of(&right);
        let mut case_values = left.clone();
        let mut other: Vec<It> = Vec::new();
        if left_reg >= 0 {
            switched = Some(mk(K::LocalReg { reg: left_reg, computed: None, ty: unbounded(), is_ct: false }, None, None, prec::PRIMARY, None));
            case_values = right.clone();
            other = left.clone();
        } else if right_reg >= 0 {
            switched = Some(mk(K::LocalReg { reg: right_reg, computed: None, ty: unbounded(), is_ct: false }, None, None, prec::PRIMARY, None));
            other = right.clone();
        }
        for i in 0..case_values.len() {
            if !outs[i].is_empty() {
                let mut v = outs[i].clone();
                v.push(case_values[i].clone());
                case_values[i] = mk(K::Comma { cmds: new_list(v) }, None, None, prec::COMMA, None);
            }
        }
        if (left_reg < 0 && right_reg < 0) || cnt == 1 {
            stack.borrow_mut().push(first_set);
            return None;
        }
        let mut default_part = self.parts[part].next[branch_num];
        let dp_start = self.parts[default_part].start;
        if self.code_len() > dp_start && self.tr.code.ins[dp_start as usize].code == op::JUMP && self.parts[default_part].refs.len() == 1 && !self.part_is_loop_cont_bre_pre(default_part, pc) {
            default_part = self.parts[default_part].next[0];
        }
        self.make_all_commands(output, stack);
        let (sw, next, ti) = self.handle_switch(pc, visited, ld, stack, stop, switched.expect("switched object"), switch_start, case_values, default_part, case_body);
        let ret = new_list(output.borrow().clone());
        let target = if ret.borrow().is_empty() { current_ret.clone() } else { ret.clone() };
        self.check_switch(ld, &sw, &other, &target);
        ret.borrow_mut().push(sw);
        if let Some(n) = next {
            match ti {
                Some(t) => ret.borrow_mut().push(t),
                None => {
                    let r = self.print_graph(pc, visited, ld, stack, None, Some(n), Some(stop.clone()), None, 0);
                    let v = r.borrow().clone();
                    ret.borrow_mut().extend(v);
                }
            }
        }
        Some(ret)
    }

    // Graph preparation: throw states, finally switches and register usage.

    /// `getThrowStates`.
    pub fn get_throw_states(&mut self, ld: &Ld, all_parts: &mut PartSet) -> Vec<ThrowState> {
        let stats = ld.stats();
        let exs = self.exceptions().clone();
        let mut ret = Vec::new();
        for (e, ex) in exs.iter().enumerate() {
            let all = all_parts.iter();
            let mut ts = ThrowState { exception_id: e as i32, ..Default::default() };
            ts.target = self.search_part(self.adr(ex.target), &all);
            let (s, en) = (self.adr_near(ex.start), self.adr_near(ex.end));
            ts.start = self.search_part(s, &all);
            for &p in &all {
                if self.parts[p].start >= s && self.parts[p].start < en {
                    ts.throwing.insert(&self.parts, p);
                }
            }
            let mut part = ts.target.expect("catch target");
            let mut new_catch = false;
            let mut scope_pos = -1;
            let mut ip = self.parts[part].start;
            while ip <= self.parts[part].end {
                let c = self.tr.code.ins[ip as usize].code;
                if c == op::NEWCATCH {
                    new_catch = true;
                }
                if c == op::PUSHSCOPE && new_catch {
                    scope_pos = stats.ins[ip as usize].scopepos_after;
                    break;
                }
                if ip == self.parts[part].end && self.parts[part].next.len() == 1 && self.parts[self.parts[part].next[0]].refs.len() == 1 {
                    part = self.parts[part].next[0];
                    ip = self.parts[part].start;
                    continue;
                }
                ip += 1;
            }
            let mut catch_parts = PartSet::new();
            if scope_pos > -1 {
                self.walk_catch_parts(&stats, part, ip, &mut catch_parts, scope_pos, all_parts, is_finally(ex));
            } else {
                part = ts.target.unwrap();
                ip = self.parts[part].start;
                let mut reg = -1;
                while ip <= self.parts[part].end {
                    let ins = &self.tr.code.ins[ip as usize];
                    if matches!(ins.code, op::NOP | op::DEBUGLINE | op::JUMP) {
                    } else if let Some(r) = ins.set_local_reg() {
                        reg = r;
                        break;
                    } else {
                        break;
                    }
                    if ip == self.parts[part].end && self.parts[part].next.len() == 1 && self.parts[self.parts[part].next[0]].refs.len() == 1 {
                        part = self.parts[part].next[0];
                        ip = self.parts[part].start;
                        continue;
                    }
                    ip += 1;
                }
                if reg != -1 {
                    let mut vis = Vec::new();
                    self.walk_catch_parts_reg(reg, part, ip + 1, &mut catch_parts, &[], &mut vis);
                }
            }
            ts.catch_parts = catch_parts;
            ret.push(ts);
        }
        ret
    }

    fn walk_catch_parts_reg(&self, reg: i32, part: P, start: i32, catch_parts: &mut PartSet, path: &[P], visited: &mut Vec<P>) {
        if self.pcontains(visited, part) {
            return;
        }
        visited.push(part);
        let mut np = path.to_vec();
        np.push(part);
        for ip in start..=self.parts[part].end {
            let Some(ins) = self.tr.code.ins.get(ip as usize) else { break };
            if ins.set_local_reg() == Some(reg) || (ins.code == op::KILL && ins.op(0) == reg) {
                return;
            }
            if ins.get_local_reg() == Some(reg) {
                for &p in &np {
                    catch_parts.insert(&self.parts, p);
                }
            }
        }
        for n in self.parts[part].next.clone() {
            let s = self.parts[n].start;
            self.walk_catch_parts_reg(reg, n, s, catch_parts, &np, visited);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_catch_parts(&mut self, stats: &super::ld::CodeStats, part: P, start: i32, catch_parts: &mut PartSet, scope_pos: i32, all_parts: &mut PartSet, finally: bool) {
        if catch_parts.contains(&self.parts, part) {
            return;
        }
        let end = self.parts[part].end;
        for ip in start..=end {
            if ip < 0 || ip as usize >= stats.ins.len() {
                continue;
            }
            if stats.ins[ip as usize].scopepos_after < scope_pos {
                if (ip as usize) < self.tr.code.len() {
                    let only_kill_jump = (ip + 1..=end).all(|j| matches!(self.tr.code.ins.get(j as usize).map(|i| i.code), Some(op::KILL | op::JUMP | op::DEBUGLINE)));
                    if only_kill_jump {
                        catch_parts.insert(&self.parts, part);
                        return;
                    }
                }
                if ip < end && !finally {
                    let second = self.new_part(ip + 1, end);
                    self.parts[part].end = ip;
                    for n in self.parts[part].next.clone() {
                        let refs = &mut self.parts[n].refs;
                        if let Some(i) = refs.iter().position(|&r| r == part) {
                            refs.remove(i);
                        }
                        refs.push(second);
                    }
                    self.parts[second].next = std::mem::take(&mut self.parts[part].next);
                    self.parts[part].next.push(second);
                    self.parts[second].refs.push(part);
                    self.parts[second].closed_time = self.parts[part].closed_time;
                    self.parts[second].level = self.parts[part].level;
                    self.parts[second].path = self.parts[part].path.clone();
                    all_parts.insert(&self.parts, second);
                }
                catch_parts.insert(&self.parts, part);
                return;
            }
        }
        catch_parts.insert(&self.parts, part);
        for n in self.parts[part].next.clone() {
            let s = self.parts[n].start;
            self.walk_catch_parts(stats, n, s, catch_parts, scope_pos, all_parts, finally);
        }
    }

    /// `beforeGetLoops`.
    pub fn before_get_loops(&mut self, ld: &mut Ld, all_parts: &PartSet, throw_states: &mut [ThrowState]) {
        let exs = self.exceptions().clone();
        let all = all_parts.iter();
        for (e, ex) in exs.iter().enumerate() {
            if is_finally(ex) {
                let p = self.search_part(self.adr(ex.target), &all);
                ld.finally_target_parts.borrow_mut().insert(e as i32, p);
            }
        }
        self.get_ignored_switches(ld, &all);
        let switch_ips: Vec<i32> = ld.ignored_switches.borrow().values().map(|&p| self.parts[p].end).collect();
        let fjs: Vec<P> = ld.finally_jumps.borrow().iter().map(|(_, v)| *v).collect();
        for t in fjs {
            if !self.map_has_value(&ld.default_parts.borrow(), t) {
                for ts in throw_states.iter_mut() {
                    ts.throwing.remove(&self.parts, t);
                }
            }
        }
        let usage = self.calculate_local_regs_usage(throw_states, ld, &switch_ips, &all);
        *ld.set_local_usages.borrow_mut() = usage;
        ld.in_get_loops = true;
    }

    fn get_ignored_switches(&mut self, ld: &mut Ld, all: &[P]) {
        let exs = self.exceptions().clone();
        let stats = ld.stats();
        for (e, ex) in exs.iter().enumerate() {
            if !is_finally(ex) {
                continue;
            }
            let ei = e as i32;
            let target_ip = self.adr(ex.target);
            let Some(ftt) = all.iter().copied().find(|&p| target_ip >= self.parts[p].start && target_ip <= self.parts[p].end) else { continue };
            let finally_part = self.parts[ftt].next.first().copied();
            let fstack = st(Stack::new(true, ld.max_temp.clone()));
            let mut ld2 = ld.copy();
            ld2.new_local_scope();
            let target_out = empty_list();
            fstack.borrow_mut().set_connected_output(Some(target_out.clone()));
            self.translate_part(&target_out, &mut ld2, ftt, &fstack);
            let mut switched_reg = -1;
            let mut kind = FINALLY_UNKNOWN;
            let mut throw_push: Option<i32> = None;
            let tout = target_out.borrow().clone();
            let top_int = (fstack.borrow().len() == 1).then(|| int_a(&fstack.borrow().items[0])).flatten();
            if let Some(v) = top_int {
                kind = FINALLY_STACK;
                throw_push = Some(v);
            } else if tout.len() >= 2 && is::set_local(&tout[tout.len() - 1]) && is::set_local(&tout[tout.len() - 2]) && int_a(&value(&tout[tout.len() - 1])).is_some() {
                switched_reg = set_local_reg(&tout[tout.len() - 1]).unwrap();
                throw_push = int_a(&value(&tout[tout.len() - 1]));
                kind = FINALLY_REGISTER;
            } else if tout.last().is_some_and(|x| matches!(x.borrow().k, K::Throw)) {
                kind = FINALLY_INLINED;
            }
            ld.finally_kinds.borrow_mut().insert(ei, kind);
            let mut default_push: Option<i32> = None;
            let mut switch_part: Option<P> = None;
            let code = &self.tr.code;
            if kind == FINALLY_STACK {
                let mut found_ips: Vec<i32> = Vec::new();
                let mut found_parts: Vec<P> = Vec::new();
                let stack_after = stats.ins[self.parts[ftt].end as usize].stackpos_after;
                if let Some(fp) = finally_part {
                    let mut vis = Vec::new();
                    self.find_all_pops(&stats, stack_after, fp, &mut found_ips, &mut found_parts, &mut vis);
                }
                'found: for i in 0..found_ips.len() {
                    let mut ip = found_ips[i];
                    if code.ins[ip as usize].code == op::LOOKUPSWITCH {
                        switch_part = Some(found_parts[i]);
                    } else if code.ins[ip as usize].code == op::POP {
                        let Some(mut pop_part) = self.search_part(ip, all) else { continue };
                        ip -= 1;
                        loop {
                            let mut j = ip;
                            let mut non_empty = false;
                            while j >= self.parts[pop_part].start {
                                let c = code.ins[j as usize].code;
                                if c == op::LOOKUPSWITCH {
                                    switch_part = self.search_part(j, all);
                                    ld.finally_double_push.borrow_mut().insert(ei);
                                    break 'found;
                                } else if c == op::LABEL {
                                } else {
                                    non_empty = true;
                                    break;
                                }
                                j -= 1;
                            }
                            if non_empty {
                                break;
                            }
                            if self.parts[pop_part].refs.len() == 1 {
                                pop_part = self.parts[pop_part].refs[0];
                                ip = self.parts[pop_part].end;
                            } else {
                                break;
                            }
                        }
                    }
                }
                let fin_end_ip = self.adr_near(ex.end) - 1;
                let mut prev_end = self.search_part(fin_end_ip, all);
                if let Some(pe) = prev_end {
                    for j in self.parts[pe].start..=self.parts[pe].end {
                        let ins = &code.ins[j as usize];
                        if ins.code == op::NOP || ins.code == op::JUMP {
                        } else if ins.code == op::PUSHBYTE {
                            default_push = Some(ins.op(0));
                            ld.push_default_part.borrow_mut().insert(ei, pe);
                        } else {
                            ld.push_default_part.borrow_mut().remove(&ei);
                            default_push = None;
                            break;
                        }
                    }
                }
                if default_push.is_none() {
                    if let Some(pe) = prev_end {
                        if code.ins[self.parts[pe].end as usize].code == op::JUMP {
                            let pe2 = self.parts[pe].next[0];
                            prev_end = Some(pe2);
                            if self.parts[pe2].next.len() == 1 && self.parts[self.parts[pe2].next[0]].refs.len() > 1 {
                                for j in self.parts[pe2].start..=self.parts[pe2].end {
                                    let ins = &code.ins[j as usize];
                                    if ins.code == op::NOP {
                                    } else if ins.code == op::PUSHBYTE {
                                        default_push = Some(ins.op(0));
                                        ld.push_default_part.borrow_mut().insert(ei, pe2);
                                        break;
                                    } else {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                let _ = prev_end;
            } else if kind == FINALLY_REGISTER {
                if let Some(fp) = finally_part {
                    let mut vis = Vec::new();
                    switch_part = self.find_lookup_switch_with_get_local(switched_reg, fp, &mut vis);
                }
                let start_ip = self.adr_near(ex.start);
                if let Some(try_part) = self.search_part(start_ip, all) {
                    let refs: Vec<P> = self.parts[try_part].refs.iter().copied().filter(|&r| self.parts[r].start >= 0).collect();
                    if refs.len() == 1 {
                        let before = refs[0];
                        if self.parts[before].height() >= 2 {
                            let mut pos = self.parts[before].end;
                            while self.parts[before].start <= pos {
                                let ins = &code.ins[pos as usize];
                                if ins.set_local_reg().is_some() {
                                    let reg = code.ins[self.parts[before].end as usize].set_local_reg().unwrap_or_else(|| ins.set_local_reg().unwrap());
                                    if reg == switched_reg && code.ins[(pos - 1) as usize].code == op::PUSHBYTE && switch_part.is_some() {
                                        default_push = Some(code.ins[(pos - 1) as usize].op(0));
                                    }
                                    break;
                                } else if ins.code == op::DEBUGLINE {
                                    pos -= 1;
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            ld.switched_regs.borrow_mut().insert(ei, switched_reg);
            let Some(sp) = switch_part else { continue };
            ld.default_ways.borrow_mut().push((sp, default_push));
            let sn = self.parts[sp].next.clone();
            let pick = |v: i32| -> P { if v < 0 || v > sn.len() as i32 - 2 { sn[0] } else { sn[(1 + v) as usize] } };
            if let Some(d) = default_push {
                ld.default_parts.borrow_mut().insert(ei, pick(d));
            }
            if let Some(t) = throw_push {
                ld.finally_throw_parts.borrow_mut().insert(ei, pick(t));
            }
            if let Some(fp) = finally_part {
                for r in self.parts[fp].refs.clone() {
                    if self.parts[r].start < 0 {
                        continue;
                    }
                    let mut rr = r;
                    loop {
                        let mut needs_prev = true;
                        let mut ip = self.parts[rr].end;
                        while ip >= self.parts[rr].start {
                            let ins = &code.ins[ip as usize];
                            if ins.code == op::JUMP {
                            } else if ins.code == op::PUSHBYTE {
                                let t = pick(ins.op(0));
                                ld.finally_jumps.borrow_mut().push((rr, t));
                                ld.finally_jumps_to_index.borrow_mut().push((rr, ei));
                                needs_prev = false;
                                break;
                            } else if ins.set_local_reg() == Some(switched_reg) || ins.code == op::COERCE_A {
                            } else {
                                needs_prev = false;
                                break;
                            }
                            ip -= 1;
                        }
                        if !needs_prev {
                            break;
                        }
                        let prevs: Vec<P> = self.parts[rr].refs.iter().copied().filter(|&x| self.parts[x].start >= 0).collect();
                        if prevs.len() == 1 {
                            rr = prevs[0];
                        } else {
                            break;
                        }
                    }
                }
            }
            ld.ignored_switches.borrow_mut().insert(ei, sp);
        }
    }

    fn find_lookup_switch_with_get_local(&self, reg: i32, part: P, visited: &mut Vec<P>) -> Option<P> {
        if self.pcontains(visited, part) {
            return None;
        }
        visited.push(part);
        let p = &self.parts[part];
        if p.height() >= 3 {
            let c = &self.tr.code.ins;
            let e = p.end as usize;
            if c[e].code == op::LOOKUPSWITCH && c[e - 1].code == op::CONVERT_I && c[e - 2].get_local_reg() == Some(reg) {
                return Some(part);
            }
        }
        for n in p.next.clone() {
            if let Some(f) = self.find_lookup_switch_with_get_local(reg, n, visited) {
                return Some(f);
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn find_all_pops(&self, stats: &super::ld::CodeStats, level: i32, part: P, ips: &mut Vec<i32>, parts: &mut Vec<P>, visited: &mut Vec<P>) {
        if self.pcontains(visited, part) {
            return;
        }
        visited.push(part);
        for ip in self.parts[part].start..=self.parts[part].end {
            if stats.ins.get(ip as usize).is_some_and(|s| s.stackpos_after == level - 1) {
                ips.push(ip);
                parts.push(part);
                return;
            }
        }
        for n in self.parts[part].next.clone() {
            self.find_all_pops(stats, level, n, ips, parts, visited);
        }
    }

    /// `calculateLocalRegsUsage`.
    fn calculate_local_regs_usage(&self, throw_states: &[ThrowState], ld: &Ld, _switch_ips: &[i32], all: &[P]) -> std::collections::BTreeMap<i32, std::collections::BTreeSet<i32>> {
        let mut usage: std::collections::BTreeMap<i32, std::collections::BTreeSet<i32>> = Default::default();
        let mut reg_of: std::collections::HashMap<i32, i32> = Default::default();
        for &p in all {
            if self.parts[p].start < 0 {
                continue;
            }
            for ip in self.parts[p].start..=self.parts[p].end {
                let Some(ins) = self.tr.code.ins.get(ip as usize) else { continue };
                if let Some(r) = ins.set_local_reg() {
                    usage.insert(ip, Default::default());
                    reg_of.insert(ip, r);
                }
            }
        }
        let keys: Vec<i32> = usage.keys().copied().collect();
        for ip in keys {
            let Some(part) = self.search_part(ip + 1, all) else { continue };
            let mut set = std::collections::BTreeSet::new();
            self.walk_local_regs_usage(throw_states, ld, &mut set, part, ip + 1, reg_of[&ip]);
            usage.insert(ip, set);
        }
        usage
    }

    fn walk_local_regs_usage(&self, throw_states: &[ThrowState], ld: &Ld, get_pos: &mut std::collections::BTreeSet<i32>, start_part: P, ip: i32, reg: i32) {
        let mut visited: Vec<P> = Vec::new();
        let mut q: std::collections::VecDeque<(P, i32)> = std::collections::VecDeque::from([(start_part, ip)]);
        'q: while let Some((part, ip)) = q.pop_front() {
            if self.pcontains(&visited, part) && part != start_part {
                continue;
            }
            if self.map_has_value(&ld.finally_throw_parts.borrow(), part) {
                visited.push(part);
                continue;
            }
            for i in ip..=self.parts[part].end {
                let Some(ins) = self.tr.code.ins.get(i as usize) else { break };
                if ins.set_local_reg() == Some(reg) {
                    continue 'q;
                }
                if ins.get_local_reg() == Some(reg) {
                    get_pos.insert(i);
                }
                if matches!(ins.code, op::INCLOCAL | op::INCLOCAL_I | op::INCLOCAL_P | op::DECLOCAL | op::DECLOCAL_I | op::DECLOCAL_P) && ins.op(0) == reg {
                    get_pos.insert(i);
                }
                if matches!(ins.code, op::INCLOCAL_P | op::DECLOCAL_P) && ins.op(1) == reg {
                    get_pos.insert(i);
                }
                if ins.code == op::HASNEXT2 && (ins.op(0) == reg || ins.op(1) == reg) {
                    get_pos.insert(i);
                }
            }
            if self.pcontains(&visited, part) {
                continue;
            }
            visited.push(part);
            let push_throws = |q: &mut std::collections::VecDeque<(P, i32)>| {
                for ts in throw_states {
                    if ts.throwing.contains(&self.parts, part) {
                        let t = ts.target.unwrap();
                        q.push_back((t, self.parts[t].start));
                    }
                }
            };
            if self.map_has_value(&ld.ignored_switches.borrow(), part) {
                push_throws(&mut q);
                return;
            }
            if let Some(t) = self.fj_get(ld, part) {
                if self.map_has_value(&ld.default_parts.borrow(), t) {
                } else if self.parts[t].next.len() == 1 {
                    q.push_back((self.parts[t].next[0], ip));
                    push_throws(&mut q);
                    continue;
                } else {
                    push_throws(&mut q);
                    continue;
                }
            }
            for n in self.parts[part].next.clone() {
                q.push_back((n, self.parts[n].start));
            }
            push_throws(&mut q);
        }
    }
}
