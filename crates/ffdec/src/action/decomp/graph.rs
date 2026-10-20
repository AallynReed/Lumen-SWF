//! `ActionGraph`: the AS1/2 overrides of the graph engine.

use std::collections::HashSet;
use std::rc::Rc;

use crate::abc::decomp::avm2post::{contains_pop_items, handle_set_temporary_declarations, promote_push_items};
use crate::abc::decomp::graph::*;
use crate::abc::decomp::item::PartId as P;
use crate::abc::decomp::item::*;
use crate::abc::decomp::ld::Ld;
use crate::abc::decomp::parts::PartSet;
use crate::abc::decomp::post::FinalData;
use crate::abc::decomp::props::{invert, through_duplicate};
use crate::abc::decomp::stack::Stack;
use crate::abc::decomp::structure::{if_branches, if_expr};

use super::super::Op;
use super::items::*;
use super::translate::{ASecondPass, ActTr};

fn last(l: &List) -> Option<It> {
    l.borrow().last().cloned()
}

fn bin_sides(it: &It) -> (It, It) {
    match &it.borrow().k {
        K::Bin { l, r, .. } => (l.clone(), r.clone()),
        _ => unreachable!(),
    }
}

impl<'t, 'c, 'a> G<'t, 'c, 'a> {
    pub fn act(&self) -> &'t ActTr {
        self.act.expect("action graph")
    }

    fn find_part(&self, ip: i32, all: &PartSet) -> Option<P> {
        all.iter().into_iter().find(|&p| self.parts[p].contains_ip(ip))
    }

    /// `ActionGraph.check`: `switch` from chains of `===` comparisons.
    #[allow(clippy::too_many_arguments)]
    pub fn as2_check(&mut self, pc: &mut Pc, current_ret: &List, visited: &mut Vec<P>, ld: &mut Ld, stack: &St, part: P, stop: &Stops, output: &List) -> Option<List> {
        let _ = current_ret;
        if let Some(l) = last(output) {
            let enumerated = matches!(l.borrow().k, K::AStoreReg { .. }) && l.borrow().value.as_ref().is_some_and(|v| matches!(v.borrow().k, K::AEnumerate { .. }));
            if enumerated {
                output.borrow_mut().pop();
            }
        }
        let top_strict = stack.borrow().items.last().is_some_and(|x| matches!(x.borrow().k, K::Bin { op: Bin::StrictEq, .. }));
        if self.parts[part].next.len() != 2 || stack.borrow().is_empty() || !top_strict {
            return None;
        }
        let switch_start = Some(self.parts[part].start as usize);
        let set = stack.borrow_mut().pop();
        let (sl, sr) = bin_sides(&set);
        let mut case_values = vec![sr];
        let switched = if matches!(sl.borrow().k, K::AStoreReg { .. }) { sl.borrow().val() } else { sl.clone() };
        let mut case_body = vec![self.parts[part].next[0]];
        let mut part = part;
        let mut second_found = false;
        if let Some(spd) = self.act().second_pass.clone() {
            for si in 0..spd.switch_parts.len() {
                let (s0, e0) = spd.switch_parts[si][0];
                if self.parts[part].start == s0 && self.parts[part].end == e0 {
                    let last_start = spd.switch_parts[si].last().unwrap().0;
                    if let Some(p) = self.find_part(last_start, &pc.all_parts) {
                        part = p;
                    }
                    case_body = spd.switch_on_false_parts[si].iter().filter_map(|&(s, _)| self.find_part(s, &pc.all_parts)).collect();
                    case_values = spd.switch_case_expressions[si].clone();
                    second_found = true;
                }
            }
        }
        if !second_found {
            stack.borrow_mut().push(set);
            return None;
        }
        let part = self.parts[part].next[1];
        let mut default_part = part;
        let ds = self.parts[default_part].start;
        if self.code_len() > ds && matches!(self.act().action(ds as usize).op, Op::Jump { .. }) && self.parts[default_part].refs.len() == 1 && !self.part_is_loop_cont_bre_pre(default_part, pc) {
            default_part = self.parts[default_part].next[0];
        }
        self.make_all_commands(output, stack);
        let (sw, next, ti) = self.handle_switch(pc, visited, ld, stack, stop, switched, switch_start, case_values, default_part, case_body);
        let ret = new_list(output.borrow().clone());
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

    /// `ActionGraph.makeAllCommands`.
    pub fn as2_make_all_commands(&self, commands: &List, stack: &St) {
        let enumerate = match last(commands) {
            Some(l) if matches!(l.borrow().k, K::AEnumerate { .. }) => commands.borrow_mut().pop(),
            _ => None,
        };
        {
            let mut s = stack.borrow_mut();
            let (keep, rest): (Vec<It>, Vec<It>) = s.items.drain(..).partition(|i| matches!(i.borrow().k, K::AEnumerated));
            s.items = rest;
            s.finish_block(commands);
            s.items = keep;
        }
        let vars = self.act().vars.clone();
        let entries: Vec<(String, Option<It>)> = vars.borrow().0.clone();
        for (name, val) in entries {
            if !name.starts_with("__register") {
                continue;
            }
            let Some(v) = val else { continue };
            let (used, reg) = match &v.borrow().k {
                K::ATempReg { used, reg } => (used.get(), *reg),
                _ => continue,
            };
            if used {
                continue;
            }
            let st = v.borrow().val();
            if !is_set_type_action(&st) {
                continue;
            }
            let n = commands.borrow().len();
            for i in 0..n {
                let c = commands.borrow()[i].clone();
                let K::ATempRegMark { reg: tr } = c.borrow().k.clone() else { continue };
                if let Some(sv) = set_type_value(&st) {
                    if let K::AStoreReg { reg: r2, temporary, .. } = &mut sv.borrow_mut().k {
                        if *r2 == reg {
                            *temporary = false;
                        }
                    }
                }
                if same(&tr, &v) {
                    commands.borrow_mut()[i] = st.clone();
                    vars.borrow_mut().put(&name, None);
                    break;
                }
            }
        }
        if let Some(e) = enumerate {
            commands.borrow_mut().push(e);
        }
    }

    /// `ActionGraph.finalProcess`.
    pub fn as2_final_process(&self, list: &List, level: i32) {
        if level == 0 {
            let mut removed: Vec<It> = Vec::new();
            let mut i = list.borrow().len();
            while i > 0 {
                i -= 1;
                let x = list.borrow()[i].clone();
                if is::script_end(&x) {
                    continue;
                }
                if matches!(x.borrow().k, K::AFunction(_)) {
                    removed.insert(0, list.borrow_mut().remove(i));
                } else {
                    break;
                }
            }
            let mut l = list.borrow_mut();
            for (k, r) in removed.into_iter().enumerate() {
                l.insert(k, r);
            }
        }
        self.tell_targets(list);
        self.for_in_loops(list);
        let mut t = 0;
        while t < list.borrow().len() {
            let mut it = list.borrow()[t].clone();
            if is::push(&it) {
                let v = it.borrow().val();
                it = v;
            }
            if matches!(it.borrow().k, K::AEnumerated) {
                list.borrow_mut().remove(t);
                continue;
            }
            t += 1;
        }
        self.final_process_generic(list);
        let mut i = 1;
        while i < list.borrow().len() {
            let prev = list.borrow()[i - 1].clone();
            let cur = list.borrow()[i].clone();
            if matches!(cur.borrow().k, K::Goto { .. }) && (is::break_(&prev) || is::continue_(&prev) || is::exit(&prev)) {
                if let K::Goto { name, .. } = &cur.borrow().k {
                    if !name.is_empty() {
                        self.act().removed_goto_labels.borrow_mut().insert(name.clone());
                    }
                }
                list.borrow_mut().remove(i);
                continue;
            }
            i += 1;
        }
    }

    fn tell_targets(&self, list: &List) {
        loop {
            let mut target_start: i64 = -1;
            let mut target_end: i64 = -1;
            let mut start_item: Option<It> = None;
            let mut target: Option<It> = None;
            let mut t = 0;
            while t < list.borrow().len() {
                let it = list.borrow()[t].clone();
                if is::push(&it) {
                    let pv = it.borrow().val();
                    if matches!(&pv.borrow().k, K::AGetProp { index: 11, .. }) {
                        list.borrow_mut().remove(t);
                        continue;
                    }
                    if matches!(pv.borrow().k, K::AFunction(_)) {
                        list.borrow_mut()[t] = pv.clone();
                    }
                }
                if is::set_temporary(&it) {
                    let sv = it.borrow().val();
                    if matches!(&sv.borrow().k, K::AGetProp { index: 11, .. }) {
                        list.borrow_mut().remove(t);
                        continue;
                    }
                }
                let ik = it.borrow().k.clone();
                if let K::ASetTarget(s) = &ik {
                    if s.is_empty() {
                        if target_start > -1 {
                            target_end = t as i64;
                            break;
                        }
                    } else {
                        target = Some(dstr(s));
                        target_start = t as i64;
                        start_item = Some(it.clone());
                    }
                }
                if let K::ASetTarget2 { target: st } = &ik {
                    if is::pop(st) || matches!(st.borrow().k, K::Temporary { .. }) {
                        list.borrow_mut().remove(t);
                        continue;
                    }
                    if is_empty_string(st) {
                        if target_start > -1 {
                            target_end = t as i64;
                            break;
                        }
                    } else {
                        target_start = t as i64;
                        target = Some(st.clone());
                        start_item = Some(it.clone());
                    }
                }
                t += 1;
            }
            if target_start > -1 && target_end == -1 {
                let n = list.borrow().len();
                target_end = n as i64;
                if list.borrow().last().is_some_and(is::script_end) {
                    target_end -= 1;
                }
            }
            let (Some(si), true, true) = (start_item.clone(), target_start > -1, target_end > -1) else { break };
            let (ts, te) = (target_start as usize, target_end as usize);
            let items: Vec<It> = list.borrow().clone();
            let mut nl: Vec<It> = items[..ts].to_vec();
            let tel: Vec<It> = items[ts + 1..te].to_vec();
            let (src, ls) = (si.borrow().src, si.borrow().line_start);
            nl.push(amk(K::ATellTarget { target: target.clone().unwrap(), cmds: new_list(tel) }, src, ls, prec::PRIMARY, None));
            if te + 1 < items.len() {
                nl.extend(items[te + 1..].iter().cloned());
            }
            *list.borrow_mut() = nl;
        }
        let mut target_start: i64 = -1;
        let mut target_end: i64 = -1;
        let mut start_item: Option<It> = None;
        let mut target: Option<It> = None;
        let mut t: i64 = 0;
        while (t as usize) < list.borrow().len() {
            let it = list.borrow()[t as usize].clone();
            let ik = it.borrow().k.clone();
            match &ik {
                K::ASetTarget(s) => {
                    if s.is_empty() {
                        if target_start > -1 {
                            target_end = t;
                        } else {
                            target_start = t;
                            start_item = Some(it.clone());
                            target = Some(dstr(s));
                        }
                    } else if target_start > -1 {
                        target_end = t;
                    }
                }
                K::ASetTarget2 { target: st } => {
                    if is_empty_string(st) {
                        if target_start > -1 {
                            target_end = t;
                        } else {
                            target_start = t;
                            start_item = Some(it.clone());
                            target = Some(st.clone());
                        }
                    } else if target_start > -1 {
                        target_end = t;
                    }
                }
                _ => {}
            }
            let is_tell = matches!(ik, K::ATellTarget { .. });
            if is_tell && target_start > -1 {
                target_end = t;
            }
            if target_start > -1 && target_end > -1 {
                let (ts, te) = (target_start as usize, target_end as usize);
                let items: Vec<It> = list.borrow().clone();
                let mut nl: Vec<It> = items[..ts].to_vec();
                let tel: Vec<It> = items[ts + 1..te].to_vec();
                let si = start_item.clone().unwrap();
                let (src, ls) = (si.borrow().src, si.borrow().line_start);
                nl.push(amk(K::ATellTarget { target: target.clone().unwrap(), cmds: new_list(tel) }, src, ls, prec::PRIMARY, None));
                let from = te + if is_tell { 0 } else { 1 };
                if from < items.len() {
                    nl.extend(items[from..].iter().cloned());
                }
                *list.borrow_mut() = nl;
                target_start = -1;
                target_end = -1;
                target = None;
                t = -1;
            }
            t += 1;
        }
    }

    fn for_in_loops(&self, list: &List) {
        let mut t = 1;
        while t < list.borrow().len() {
            let it = list.borrow()[t].clone();
            let mut checked_body: Option<List> = None;
            let mut checked_cond: Option<It> = None;
            let mut checked_loop: Option<i64> = None;
            match it.borrow().k.clone() {
                K::While { expr, cmds, loop_id, .. } => {
                    checked_body = Some(cmds);
                    checked_cond = expr.borrow().last().cloned();
                    checked_loop = Some(loop_id);
                }
                K::If { expr, on_true, on_false, .. } if on_false.borrow().is_empty() => {
                    checked_body = Some(on_true);
                    checked_cond = Some(expr);
                }
                _ => {}
            }
            let mut enumerate: Option<It> = None;
            let mut cmp: Option<It> = None;
            let prev = list.borrow()[t - 1].clone();
            if let (Some(c), Some(b)) = (checked_cond.clone(), checked_body.clone()) {
                if is::true_(&c) {
                    let first = b.borrow().first().cloned();
                    if let Some(ifi) = first.filter(is::if_) {
                        let (ot, of) = if_branches(&ifi);
                        if of.borrow().is_empty() && ot.borrow().last().is_some_and(is::break_) {
                            let e = if_expr(&ifi);
                            checked_cond = Some(e.clone());
                            let ek = e.borrow().k.clone();
                            if let K::Bin { op: Bin::Eq, l, r } = ek {
                                if matches!(r.borrow().k, K::ADirect { v: AVal::Null, .. }) && matches!(prev.borrow().k, K::AEnumerate { .. }) {
                                    enumerate = Some(prev.clone());
                                    if matches!(l.borrow().k, K::AStoreReg { .. }) {
                                        cmp = Some(e.clone());
                                        b.borrow_mut().remove(0);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(c) = &checked_cond {
                if let K::Bin { op: Bin::Neq, l, r } = c.borrow().k.clone() {
                    if matches!(r.borrow().k, K::ADirect { v: AVal::Null, .. }) && matches!(prev.borrow().k, K::AEnumerate { .. }) {
                        enumerate = Some(prev.clone());
                        if matches!(l.borrow().k, K::AStoreReg { .. }) {
                            cmp = Some(c.clone());
                        }
                    }
                }
            }
            if let Some(cmp) = cmp {
                let (cl, _) = bin_sides(&cmp);
                let enumerate = enumerate.unwrap();
                let K::AEnumerate { obj: mut eobj } = enumerate.borrow().k.clone() else { unreachable!() };
                let mut done = false;
                if let Some(b) = &checked_body {
                    let first = b.borrow().first().cloned();
                    if let Some(sti) = first.filter(is_set_type_action) {
                        let sv = set_type_value(&sti);
                        let reg_of_sv = sv.as_ref().and_then(|v| match &v.borrow().k {
                            K::ADirect { v: AVal::Register { num, .. }, .. } => Some(*num),
                            _ => None,
                        });
                        let cl_reg = match &cl.borrow().k {
                            K::AStoreReg { reg, .. } => Some(*reg),
                            _ => None,
                        };
                        let enum_val = cl.borrow().value.as_ref().is_some_and(|v| matches!(through_duplicate(v).borrow().k, K::AEnumerated));
                        if reg_of_sv.is_some() && enum_val && reg_of_sv == cl_reg {
                            list.borrow_mut().remove(t);
                            b.borrow_mut().remove(0);
                            let lid = match checked_loop {
                                Some(l) => l,
                                None => {
                                    let id = self.act().loop_counter();
                                    b.borrow_mut().push(break_item(None, None, id));
                                    id
                                }
                            };
                            sti.borrow_mut().value = Some(direct(AVal::Null));
                            let fi = amk(K::AForIn { loop_id: lid, var: sti.clone(), obj: eobj.clone(), cmds: b.clone(), label_used: false }, None, None, prec::NOPRECEDENCE, None);
                            list.borrow_mut().insert(t, fi);
                            list.borrow_mut().remove(t - 1);
                            t -= 1;
                            let cv = cl.borrow().val();
                            if let K::Temporary { temp } = cv.borrow().k.clone() {
                                if t > 0 && matches!(&list.borrow()[t - 1].borrow().k, K::SetTemporary { temp: t2, .. } if *t2 == temp) {
                                    list.borrow_mut().remove(t - 1);
                                    t -= 1;
                                }
                            }
                            done = true;
                        }
                    }
                }
                if done {
                    t += 1;
                    continue;
                }
                if let Some(b) = &checked_body {
                    list.borrow_mut().remove(t);
                    let lid = match checked_loop {
                        Some(l) => l,
                        None => {
                            let id = self.act().loop_counter();
                            b.borrow_mut().push(break_item(None, None, id));
                            id
                        }
                    };
                    list.borrow_mut().remove(t - 1);
                    t -= 1;
                    if is_set_type_action(&eobj) {
                        list.borrow_mut().insert(t, eobj.clone());
                        t += 1;
                        let o = set_type_object(&eobj);
                        if let K::AEnumerate { obj } = &mut enumerate.borrow_mut().k {
                            *obj = o.clone();
                        }
                        eobj = o;
                    }
                    let fi = amk(K::AForIn { loop_id: lid, var: cl.clone(), obj: eobj, cmds: b.clone(), label_used: false }, None, None, prec::NOPRECEDENCE, None);
                    list.borrow_mut().insert(t, fi);
                    if t + 1 < list.borrow().len() && matches!(list.borrow()[t + 1].borrow().k, K::AEnumerated) {
                        list.borrow_mut().remove(t + 1);
                    }
                }
            }
            t += 1;
        }
    }

    /// `ActionGraph.finalProcessAfter`.
    pub fn as2_final_process_after(&self, list: &List, level: i32) {
        self.final_process_after_generic(list, level);
        let mut used: HashSet<String> = HashSet::new();
        collect_used_labels(list, &mut used);
        let removed = self.act().removed_goto_labels.borrow().clone();
        let mut i = list.borrow().len();
        while i > 0 {
            i -= 1;
            let drop_it = matches!(&list.borrow()[i].borrow().k, K::Label { name } if removed.contains(name) && !used.contains(name));
            if drop_it {
                list.borrow_mut().remove(i);
            }
        }
    }

    /// `ActionGraph.prepareSecondPass`.
    pub fn as2_prepare_second_pass(&mut self, ld: &Ld, pc: &Pc, list: &List) -> ASecondPass {
        let mut spd = ASecondPass::default();
        let mut processed: Vec<(i32, i32)> = Vec::new();
        self.check_second_pass_switches(ld, pc, &mut spd, &mut processed, list);
        spd
    }

    fn pkey(&self, p: Option<P>) -> (i32, i32) {
        match p {
            Some(p) => (self.parts[p].start, self.parts[p].end),
            None => (-2, -2),
        }
    }

    fn check_second_pass_switches(&mut self, ld: &Ld, pc: &Pc, spd: &mut ASecondPass, processed: &mut Vec<(i32, i32)>, list: &List) {
        let n = list.borrow().len();
        for i in 0..n {
            let item = list.borrow()[i].clone();
            let K::DoWhile { loop_id, cmds, expr, .. } = item.borrow().k.clone() else { continue };
            let e = expr.borrow_mut().pop().unwrap();
            let ev: Vec<It> = expr.borrow().clone();
            cmds.borrow_mut().extend(ev);
            let ifi = if_item(None, None, invert(&e, e.borrow().src), new_list(vec![break_item(None, None, loop_id)]), empty_list());
            let dp = self.new_part(i32::MAX, processed.len() as i32);
            if let K::If { decision_part, .. } = &mut ifi.borrow_mut().k {
                *decision_part = Some(dp);
            }
            cmds.borrow_mut().push(ifi);
            let uni = mk(K::While { loop_id, expr: new_list(vec![true_item(None, None)]), cmds, label_used: false, universal: true }, None, None, prec::NOPRECEDENCE, None);
            list.borrow_mut()[i] = uni;
        }
        let items: Vec<It> = list.borrow().clone();
        for item in &items {
            if let K::If { expr, decision_part, on_true_part, on_false_part, .. } = item.borrow().k.clone() {
                let dkey = self.pkey(decision_part);
                if !processed.contains(&dkey) && matches!(expr.borrow().k, K::Bin { op: Bin::StrictEq | Bin::StrictNeq, .. }) {
                    let mut is_neq = matches!(expr.borrow().k, K::Bin { op: Bin::StrictNeq, .. });
                    let (sl, sr) = bin_sides(&expr);
                    let (reg_id, svar) = match &sl.borrow().k {
                        K::AStoreReg { reg, .. } => (*reg, None),
                        _ => (-1, Some(sl.clone())),
                    };
                    let mut parts = vec![dkey];
                    let mut exprs = vec![sr];
                    let mut on_false = vec![self.pkey(on_true_part)];
                    let mut cases: Vec<Option<P>> = vec![if is_neq { on_true_part } else { on_false_part }];
                    let mut ii2 = item.clone();
                    loop {
                        let (t2, f2) = if_branches(&ii2);
                        let next = first_non_label(if is_neq { &t2 } else { &f2 });
                        let Some(n2) = next.filter(is::if_) else { break };
                        ii2 = n2;
                        let K::If { expr: e2, decision_part: d2, on_true_part: ot2, on_false_part: of2, .. } = ii2.borrow().k.clone() else { break };
                        if !matches!(e2.borrow().k, K::Bin { op: Bin::StrictEq | Bin::StrictNeq, .. }) {
                            break;
                        }
                        is_neq = matches!(e2.borrow().k, K::Bin { op: Bin::StrictNeq, .. });
                        let (l2, r2) = bin_sides(&e2);
                        let ok = if is_direct(&l2) {
                            matches!(&l2.borrow().k, K::ADirect { v: AVal::Register { num, .. }, .. } if *num == reg_id)
                        } else if let Some(sv) = &svar {
                            crate::abc::decomp::eq::eq(&l2, sv)
                        } else {
                            false
                        };
                        if is_direct(&l2) && !matches!(&l2.borrow().k, K::ADirect { v: AVal::Register { .. }, .. }) {
                            break;
                        }
                        if !ok {
                            break;
                        }
                        processed.push(dkey);
                        processed.push(self.pkey(d2));
                        parts.push(self.pkey(d2));
                        on_false.push(self.pkey(ot2));
                        exprs.push(r2);
                        cases.push(if is_neq { ot2 } else { of2 });
                    }
                    if parts.len() > 1 {
                        let case_parts: Vec<P> = cases.iter().filter_map(|c| *c).collect();
                        spd.switch_parts.push(parts);
                        spd.switch_on_false_parts.push(on_false);
                        spd.switch_case_expressions.push(exprs);
                        spd.switch_cases.push(case_parts.clone());
                        let b = self.get_most_common_part(ld, &case_parts, pc);
                        spd.switch_breaks.push(b);
                    }
                }
            }
            if is::block(item) {
                for s in subs(item) {
                    self.check_second_pass_switches(ld, pc, spd, processed, &s);
                }
            }
        }
    }

    /// `Graph.translate` with `ActionGraph.translate`. `Err` carries the second-pass data and
    /// the first-pass tree.
    pub fn as2_translate(&mut self, ld: &mut Ld) -> Result<List, (ASecondPass, List)> {
        let mut all_parts = PartSet::new();
        for h in self.heads.clone() {
            self.populate_parts(h, &mut all_parts);
        }
        let stack = st(Stack::new(true, ld.max_temp.clone()));
        let mut loops: Vec<usize> = Vec::new();
        let spd = self.act().second_pass.clone();
        let (sc, sb) = match &spd {
            Some(sp) => (sp.switch_cases.clone(), sp.switch_breaks.clone()),
            None => (Vec::new(), Vec::new()),
        };
        let heads = self.heads.clone();
        {
            let mut lc = crate::abc::decomp::loops::LoopCtx { parts: &mut self.parts, arena: &mut self.arena };
            crate::abc::decomp::loops::detect_loops(&mut lc, &heads, &mut loops, &[], &all_parts, &sc, &sb, spd.is_some());
        }
        ld.in_get_loops = false;
        self.do_not_process_ifs = spd.is_none();
        self.act().loop_count.set(loops.len() as i64);
        let mut pc = Pc { has_empty_stack_pops: false, found_gotos: Vec::new(), part_codes: Vec::new(), part_code_pos: Vec::new(), all_parts, loops, throw_states: Vec::new(), static_operation: 0 };
        let mut visited: Vec<P> = Vec::new();
        let ret = self.print_graph(&mut pc, &mut visited, ld, &stack, None, Some(heads[0]), None, None, 0);
        if spd.is_none() {
            let data = self.as2_prepare_second_pass(ld, &pc, &ret);
            return Err((data, ret));
        }
        self.process_gotos(&mut pc, &ret);
        self.process_ifs(&ret);
        self.propagate_breaks(&ret);
        self.make_all_commands(&ret, &stack);
        if !pc.has_empty_stack_pops || !contains_pop_items(&ret) {
            promote_push_items(&ret);
        }
        let fd = FinalData { temporary_registers: Default::default(), register_usage: Default::default(), bottom_set_locals: Default::default() };
        self.final_process_all(None, &ret, 0, &fd);
        handle_set_temporary_declarations(&ret);
        let act = self.act();
        if act.inside_do_init && !act.inside_function {
            super::class::check_class(&ret, &act.vars, &act.uninit);
        }
        let defined: HashSet<i32> = act.reg_names.borrow().keys().copied().collect();
        make_define_registers_up(&ret, &mut defined.clone());
        Ok(ret)
    }
}

fn first_non_label(l: &List) -> Option<It> {
    l.borrow().iter().find(|x| !matches!(x.borrow().k, K::Label { .. })).cloned()
}

fn collect_used_labels(list: &List, used: &mut HashSet<String>) {
    for item in list.borrow().iter() {
        if let K::Goto { name, .. } = &item.borrow().k {
            if !name.is_empty() {
                used.insert(name.clone());
            }
        }
        if is::block(item) {
            for s in subs(item) {
                collect_used_labels(&s, used);
            }
        }
    }
}

/// `ActionGraph.makeDefineRegistersUp`: `var` on the first store of each register.
fn make_define_registers_up(list: &List, defined: &mut HashSet<i32>) {
    let mut i = 0;
    while i < list.borrow().len() {
        let ti = list.borrow()[i].clone();
        let mut ri = i;
        if matches!(ti.borrow().k, K::ATempReg { .. }) {
            i += 1;
            continue;
        }
        if let K::AStoreReg { reg, define, .. } = &mut ti.borrow_mut().k {
            *define = !defined.contains(reg);
            defined.insert(*reg);
        }
        let mut seen: Vec<*const std::cell::RefCell<Item>> = Vec::new();
        let for_in_var = match &ti.borrow().k {
            K::AForIn { var, .. } => Some(var.clone()),
            _ => None,
        };
        let mut inserts: Vec<It> = Vec::new();
        fn visit(it: &It, ti: &It, for_in_var: &Option<It>, defined: &mut HashSet<i32>, seen: &mut Vec<*const std::cell::RefCell<Item>>, inserts: &mut Vec<It>) {
            for c in children(it, true) {
                let p = Rc::as_ptr(&c);
                if seen.contains(&p) {
                    continue;
                }
                seen.push(p);
                if matches!(c.borrow().k, K::ATempReg { .. }) {
                    continue;
                }
                if for_in_var.as_ref().is_some_and(|v| same(v, &c)) {
                    continue;
                }
                let sr = match &c.borrow().k {
                    K::AStoreReg { reg, name, .. } => Some((*reg, name.clone())),
                    _ => None,
                };
                if let Some((reg, name)) = sr {
                    let def = !defined.contains(&reg);
                    defined.insert(reg);
                    if let K::AStoreReg { define, .. } = &mut c.borrow_mut().k {
                        *define = def;
                    }
                    if def && !same(&c, ti) {
                        inserts.push(amk(
                            K::AStoreReg { reg, name, define: true, temporary: false, compound_value: None, compound_op: None },
                            None,
                            None,
                            prec::ASSIGNMENT,
                            Some(direct(AVal::Undefined)),
                        ));
                        if let K::AStoreReg { define, .. } = &mut c.borrow_mut().k {
                            *define = false;
                        }
                    }
                }
                if matches!(c.borrow().k, K::AFunction(_)) {
                    continue;
                }
                visit(&c, ti, for_in_var, defined, seen, inserts);
            }
        }
        visit(&ti, &ti, &for_in_var, defined, &mut seen, &mut inserts);
        for ins in inserts {
            list.borrow_mut().insert(ri, ins);
            ri += 1;
        }
        if is::block(&ti) {
            let ss = subs(&ti);
            for s in &ss {
                make_define_registers_up(s, defined);
            }
            for s in &ss {
                let mut j = 0;
                while j < s.borrow().len() {
                    let item = s.borrow()[j].clone();
                    let sr = match &item.borrow().k {
                        K::AStoreReg { reg, name, define: true, .. } => Some((*reg, name.clone())),
                        _ => None,
                    };
                    if let Some((reg, name)) = sr {
                        list.borrow_mut().insert(
                            ri,
                            amk(K::AStoreReg { reg, name, define: true, temporary: false, compound_value: None, compound_op: None }, None, None, prec::ASSIGNMENT, Some(direct(AVal::Undefined))),
                        );
                        if let K::AStoreReg { define, .. } = &mut item.borrow_mut().k {
                            *define = false;
                        }
                        let undef = item.borrow().value.as_ref().is_some_and(|v| matches!(v.borrow().k, K::ADirect { v: AVal::Undefined, .. }));
                        if undef {
                            s.borrow_mut().remove(j);
                            ri += 1;
                            continue;
                        }
                        ri += 1;
                    }
                    j += 1;
                }
            }
        }
        i = ri + 1;
    }
}
