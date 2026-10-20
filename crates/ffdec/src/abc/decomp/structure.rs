//! FFDec's `Graph.printGraph`, `handleLoop` and `handleSwitch`: turning parts into statements.

use super::eq;
use super::graph::*;
use super::item::*;
use super::item::PartId as P;
use super::ld::Ld;
use super::parts::*;
use super::props::*;
use super::stack::Stack;

struct LoopLocal {
    part: Option<P>,
    loop_item: It,
    li: It,
    current_loop: usize,
    loop_type_found: bool,
    do_while_candidate: bool,
    precontinue: List,
    stop: Stops,
    ret: List,
    s_pre_loop: St,
}

fn last_is(l: &List, f: fn(&It) -> bool) -> bool {
    l.borrow().last().is_some_and(f)
}

fn loop_id_of_last(l: &List, kind: fn(&It) -> bool) -> Option<i64> {
    let x = l.borrow().last()?.clone();
    if !kind(&x) {
        return None;
    }
    let r = match &x.borrow().k {
        K::Break { loop_id, .. } | K::Continue { loop_id, .. } => Some(*loop_id),
        _ => None,
    };
    r
}

pub fn br_id(it: &It) -> Option<i64> {
    match &it.borrow().k {
        K::Break { loop_id, .. } => Some(*loop_id),
        _ => None,
    }
}

pub fn cont_id(it: &It) -> Option<i64> {
    match &it.borrow().k {
        K::Continue { loop_id, .. } => Some(*loop_id),
        _ => None,
    }
}

/// `(onTrue, onFalse)` of an if item.
pub fn if_branches(it: &It) -> (List, List) {
    match &it.borrow().k {
        K::If { on_true, on_false, .. } => (on_true.clone(), on_false.clone()),
        _ => panic!("not an if"),
    }
}

pub fn if_expr(it: &It) -> It {
    match &it.borrow().k {
        K::If { expr, .. } => expr.clone(),
        _ => panic!("not an if"),
    }
}

fn single_break_of(l: &List, id: i64) -> bool {
    let b = l.borrow();
    b.len() == 1 && br_id(&b[0]) == Some(id)
}

fn single_continue_of(l: &List, id: i64) -> bool {
    let b = l.borrow();
    b.len() == 1 && cont_id(&b[0]) == Some(id)
}

fn loop_commands(it: &It) -> List {
    match &it.borrow().k {
        K::While { cmds, .. } => cmds.clone(),
        _ => panic!("not a while"),
    }
}

impl<'t, 'c, 'a> G<'t, 'c, 'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn print_graph(&mut self, pc: &mut Pc, visited: &mut Vec<P>, ld: &mut Ld, stack: &St, parent: Option<P>, part: Option<P>, stop: Option<Stops>, ret: Option<List>, recursion: usize) -> List {
        let ret = ret.unwrap_or_else(empty_list);
        let original_ret = ret.clone();
        let mut ret = ret;
        let mut stack = stack.clone();
        let mut parent = parent;
        let mut part = part;
        let mut stop = stop.unwrap_or_default();
        let mut loop_stack: Vec<LoopLocal> = Vec::new();
        let mut next_one: Option<P> = None;
        let mut is_loop = false;
        let mut current_loop: Option<usize> = None;
        let mut loop_item: Option<It> = None;
        let mut s_pre_loop: Option<St> = None;
        let mut li: Option<It> = None;
        let mut loop_type_found = false;
        let mut do_while_candidate = false;
        let mut precontinue: List = empty_list();
        'main: loop {
            if recursion > pc.all_parts.len() + 1 {
                panic!("printGraph max recursion level reached.");
            }
            let Some(p0) = part else { return original_ret };
            let Some(p) = self.check_part_with_output(Some(&ret), &stack, ld, parent, p0) else { return original_ret };
            part = Some(p);
            precontinue = empty_list();
            let can_handle = self.can_handle_loop(ld, p, pc);
            let mut ignored_loop: Option<usize> = None;
            for &l in &pc.loops.clone() {
                if self.arena[l].cont == Some(p) && self.arena[l].phase == 0 {
                    if can_handle {
                        current_loop = Some(l);
                        self.arena[l].phase = 1;
                        is_loop = true;
                    } else {
                        ignored_loop = Some(l);
                    }
                    break;
                }
            }
            if is_loop {
                self.make_all_commands(&ret, &stack);
            }
            for idx in (0..pc.loops.len()).rev() {
                let l = pc.loops[idx];
                if Some(l) == ignored_loop || Some(l) == current_loop {
                    continue;
                }
                if self.arena[l].phase == 4 {
                    self.arena[l].phase = 1;
                    continue;
                }
                if self.arena[l].phase != 1 {
                    continue;
                }
                let lid = self.arena[l].id;
                if self.arena[l].brk == Some(p) {
                    if let Some(cl) = current_loop {
                        self.arena[cl].phase = 0;
                    }
                    self.make_all_commands(&ret, &stack);
                    let br = break_item(None, ld.line_start, lid);
                    if self.parts[p].start >= self.code_len() - 1 {
                        if let K::Break { is_script_end, .. } = &mut br.borrow_mut().k {
                            *is_script_end = true;
                        }
                    }
                    ret.borrow_mut().push(br);
                    break 'main;
                }
                if self.arena[l].pre_cont == Some(p) || self.arena[l].cont == Some(p) {
                    if let Some(cl) = current_loop {
                        self.arena[cl].phase = 0;
                    }
                    self.make_all_commands(&ret, &stack);
                    ret.borrow_mut().push(continue_item(None, ld.line_start, lid));
                    break 'main;
                }
            }
            if stop.iter().any(|(s, _)| self.peq(*s, p)) {
                let mut real = false;
                for (s, k) in stop.iter().rev() {
                    if *k == StopKind::Other && *s == p {
                        real = true;
                        break;
                    }
                    if *k == StopKind::BlockClose {
                        if *s == p {
                            real = true;
                        }
                        break;
                    }
                }
                if real {
                    if let Some(cl) = current_loop {
                        self.arena[cl].phase = 0;
                    }
                    break 'main;
                }
            }
            if self.code_len() <= self.parts[p].start {
                if !last_is(&ret, is::exit) {
                    stack.borrow_mut().set_connected_output(Some(ret.clone()));
                    stack.borrow_mut().add_to_output(script_end());
                }
                break 'main;
            }
            if self.can_handle_visited(ld, p) {
                if self.pcontains(visited, p) {
                    let mut label = format!("addr{:04x}", self.tr.code.pos2adr(self.parts[p].start as usize));
                    let first_code = pc.part_codes.iter().find(|(x, _)| self.peq(*x, p)).map(|(_, l)| l.clone()).expect("part code");
                    let mut pos = pc.part_code_pos.iter().find(|(x, _)| self.peq(*x, p)).map(|(_, v)| *v).unwrap_or(0);
                    if pos > first_code.borrow().len() {
                        pos = first_code.borrow().len();
                    }
                    let existing = first_code.borrow().get(pos).and_then(|x| match &x.borrow().k {
                        K::Label { name } => Some(name.clone()),
                        _ => None,
                    });
                    match existing {
                        Some(n) => label = n,
                        None => first_code.borrow_mut().insert(pos, mk(K::Label { name: label.clone() }, None, ld.line_start, prec::PRIMARY, None)),
                    }
                    let g = mk(K::Goto { name: label, target: None }, None, ld.line_start, prec::PRIMARY, None);
                    ret.borrow_mut().push(g.clone());
                    pc.found_gotos.push(g);
                    ld.gotos_used.set(true);
                    break 'main;
                } else {
                    visited.push(p);
                    pc.part_codes.push((p, ret.clone()));
                    let len = ret.borrow().len();
                    pc.part_code_pos.push((p, len));
                }
            }
            let mut current_ret = ret.clone();
            s_pre_loop = Some(stack.clone());
            if is_loop {
                let cl = current_loop.unwrap();
                let ns = st_clone(&stack);
                ns.borrow_mut().clear();
                stack = ns;
                let lit = mk(K::While { loop_id: self.arena[cl].id, expr: new_list(vec![true_item(None, ld.line_start)]), cmds: empty_list(), label_used: false, universal: true }, None, ld.line_start, prec::NOPRECEDENCE, None);
                current_ret.borrow_mut().push(lit.clone());
                current_ret = loop_commands(&lit);
                loop_item = Some(lit.clone());
                li = Some(lit);
                if let Some(backup) = self.arena[cl].pre_cont {
                    self.arena[cl].pre_cont = None;
                    let mut stop2 = stop.clone();
                    stop2.push((self.arena[cl].cont.unwrap(), StopKind::Other));
                    let mut sub_visited: Vec<P> = Vec::new();
                    let phases: Vec<i32> = pc.loops.iter().map(|&l| self.arena[l].phase).collect();
                    let fresh = st(Stack::new(true, ld.max_temp.clone()));
                    precontinue = self.print_graph(pc, &mut sub_visited, ld, &fresh, None, Some(backup), Some(stop2), None, recursion + 1);
                    self.arena[cl].pre_cont = Some(backup);
                    self.check_continue_at_the_end(&precontinue, self.arena[cl].id);
                    let cid = self.arena[cl].id;
                    if let Some(last) = last_of(&precontinue) {
                        if is::if_(&last) {
                            let (t, f) = if_branches(&last);
                            if (single_break_of(&t, cid) && single_continue_of(&f, cid)) || (single_continue_of(&t, cid) && single_break_of(&f, cid)) {
                                do_while_candidate = true;
                            }
                        }
                    }
                    if !do_while_candidate {
                        if precontinue.borrow().iter().any(is::block) {
                            self.arena[cl].pre_cont = None;
                        }
                        if self.arena[cl].pre_cont.is_none() {
                            precontinue.borrow_mut().clear();
                            for (i, ph) in phases.iter().enumerate() {
                                let l = pc.loops[i];
                                self.arena[l].phase = *ph;
                            }
                        }
                    }
                }
            }
            let mut parse_next = true;
            let output = empty_list();
            stack.borrow_mut().set_connected_output(Some(current_ret.clone()));
            if self.check_part_output(pc, &current_ret, visited, ld, &stack, parent, p, &stop, current_loop, recursion) {
                parse_next = false;
            } else {
                stack.borrow_mut().set_connected_output(Some(output.clone()));
                let (s, e) = (self.parts[p].start as usize, self.parts[p].end as usize);
                self.tr.translate_range(&output, ld, &stack, s, e);
                if stack.borrow().empty_pop_count > 0 {
                    pc.has_empty_stack_pops = true;
                }
                if self.parts[p].end >= self.code_len() - 1 && self.next_parts(p).is_empty() && !last_is(&output, is::exit) {
                    stack.borrow_mut().add_to_output(script_end());
                }
            }
            if parse_next {
                match self.check(pc, &current_ret, visited, ld, &stack, parent, p, &stop, &output) {
                    Some(rc) => {
                        let v = rc.borrow().clone();
                        current_ret.borrow_mut().extend(v);
                        parse_next = false;
                    }
                    None => {
                        let v = output.borrow().clone();
                        current_ret.borrow_mut().extend(v);
                    }
                }
            }
            if parse_next {
                let nps = self.next_parts(p);
                if nps.len() > 2 || self.part_is_switch(p) {
                    self.switch_part(pc, visited, ld, &stack, p, &stop, &current_ret, &ret, &mut next_one);
                }
                let nps = self.next_parts(p);
                if next_one.is_none() && nps.len() == 2 && !self.part_is_switch(p) {
                    let expr = self.get_if_expression(&stack, &current_ret);
                    self.if_part(pc, visited, ld, &mut stack, p, &stop, &mut current_ret, &mut ret, expr, &mut next_one, &mut is_loop, &mut current_loop, &mut loop_item, &mut li, &mut loop_type_found, &mut do_while_candidate, &mut precontinue, &mut s_pre_loop, recursion);
                }
                if next_one.is_none() && self.next_parts(p).len() == 1 {
                    next_one = Some(self.next_parts(p)[0]);
                }
                if self.next_parts(p).is_empty() {
                    self.make_all_commands(&current_ret, &stack);
                }
            }
            if let Some(n) = next_one {
                if is_loop {
                    loop_stack.push(LoopLocal {
                        part: Some(p),
                        loop_item: loop_item.clone().unwrap(),
                        li: li.clone().unwrap(),
                        current_loop: current_loop.unwrap(),
                        loop_type_found,
                        do_while_candidate,
                        precontinue: precontinue.clone(),
                        stop: stop.clone(),
                        ret: ret.clone(),
                        s_pre_loop: s_pre_loop.clone().unwrap(),
                    });
                }
                parent = Some(p);
                part = Some(n);
                next_one = None;
                is_loop = false;
                li = None;
                current_loop = None;
                loop_type_found = false;
                do_while_candidate = false;
                ret = current_ret;
                s_pre_loop = None;
                continue;
            }
            if is_loop {
                loop_stack.push(LoopLocal {
                    part: Some(p),
                    loop_item: loop_item.clone().unwrap(),
                    li: li.clone().unwrap(),
                    current_loop: current_loop.unwrap(),
                    loop_type_found,
                    do_while_candidate,
                    precontinue: precontinue.clone(),
                    stop: stop.clone(),
                    ret: ret.clone(),
                    s_pre_loop: s_pre_loop.clone().unwrap(),
                });
            }
            break;
        }
        let _ = &mut stop;
        while let Some(mut ll) = loop_stack.pop() {
            self.handle_loop(pc, &mut ll, visited, ld, recursion);
        }
        original_ret
    }

    #[allow(clippy::too_many_arguments)]
    fn switch_part(&mut self, pc: &mut Pc, visited: &mut Vec<P>, ld: &mut Ld, stack: &St, p: P, stop: &Stops, current_ret: &List, ret: &List, next_one: &mut Option<P>) {
        let original_switched = stack.borrow_mut().pop();
        self.make_all_commands(current_ret, stack);
        let mut switched = original_switched.clone();
        if is::pop(&switched) {
            if let Some(last) = last_of(current_ret) {
                if is::if_(&last) {
                    switched = last;
                }
            }
        }
        let mut case_values: Vec<It> = Vec::new();
        let mut case_left: std::collections::BTreeMap<i32, It> = Default::default();
        let mut case_right: std::collections::BTreeMap<i32, It> = Default::default();
        let mut comma: Vec<It> = Vec::new();
        let mut case_comma: std::collections::BTreeMap<i32, Vec<It>> = Default::default();
        let mut it = switched.clone();
        let mut default_branch = 0;
        let mut has_expr = false;
        let int_value = |x: &It| -> Option<i32> {
            match &x.borrow().k {
                K::IntegerValue(v) | K::IntegerValueA { v, .. } => Some(*v),
                _ => None,
            }
        };
        let eq_type = |x: &It| matches!(x.borrow().k, K::Bin { op: Bin::Eq | Bin::StrictEq, .. });
        let neq_type = |x: &It| matches!(x.borrow().k, K::Bin { op: Bin::Neq | Bin::StrictNeq, .. });
        let sides = |x: &It| match &x.borrow().k {
            K::Bin { l, r, .. } => (l.clone(), r.clone()),
            _ => unreachable!(),
        };
        loop {
            if is::if_(&it) {
                let expr = if_expr(&it);
                let (t, f) = if_branches(&it);
                if eq_type(&expr) || neq_type(&expr) {
                    let (ot, of) = if neq_type(&expr) { (f, t) } else { (t, f) };
                    let tl = ot.borrow().last().cloned();
                    let pushed_int = tl.filter(is::push).and_then(|x| int_value(&x.borrow().val()));
                    if !of.borrow().is_empty() && pushed_int.is_some() {
                        let cpos = pushed_int.unwrap();
                        case_comma.insert(cpos, std::mem::take(&mut comma));
                        let (l, r) = sides(&expr);
                        case_left.insert(cpos, l);
                        case_right.insert(cpos, r);
                        let ofv = of.borrow().clone();
                        comma = ofv[..ofv.len() - 1].to_vec();
                        it = ofv.last().unwrap().clone();
                        if is::push(&it) {
                            let v = it.borrow().val();
                            it = v;
                        }
                    } else {
                        break;
                    }
                } else if is::false_(&expr) && !f.borrow().is_empty() {
                    it = last_of(&f).unwrap();
                } else if is::true_(&expr) && !t.borrow().is_empty() {
                    it = last_of(&t).unwrap();
                } else {
                    break;
                }
            } else if is::ternar(&it) {
                let (expr, t, f) = match &it.borrow().k {
                    K::Ternar { expr, on_true, on_false } => (expr.clone(), on_true.clone(), on_false.clone()),
                    _ => unreachable!(),
                };
                if eq_type(&expr) || neq_type(&expr) {
                    let (ot, of) = if neq_type(&expr) { (f, t) } else { (t, f) };
                    if let Some(cpos) = int_value(&ot) {
                        let (l, r) = sides(&expr);
                        case_left.insert(cpos, l);
                        case_right.insert(cpos, r);
                        case_comma.insert(cpos, std::mem::take(&mut comma));
                        it = of;
                        let cmds = match &it.borrow().k {
                            K::Comma { cmds } => Some(cmds.borrow().clone()),
                            _ => None,
                        };
                        if let Some(c) = cmds {
                            comma = c[..c.len() - 1].to_vec();
                            it = c.last().unwrap().clone();
                        }
                    } else {
                        break;
                    }
                } else if is::false_(&expr) {
                    it = f;
                } else if is::true_(&expr) {
                    it = t;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        if !same(&switched, &original_switched) && !case_right.is_empty() {
            current_ret.borrow_mut().pop();
        } else {
            switched = original_switched.clone();
        }
        if let Some(v) = int_value(&it) {
            if int_value(&switched).is_none() {
                default_branch = v;
            }
        }
        let mut case_exprs: std::collections::BTreeMap<i32, It> = Default::default();
        let mut other_sides: Vec<It> = Vec::new();
        if !case_right.is_empty() {
            let first = case_right.values().next().unwrap().clone();
            if case_right.values().all(|c| eq::eq(c, &first)) {
                case_exprs = case_left.clone();
                other_sides = case_right.values().cloned().collect();
                switched = first;
                has_expr = true;
            } else {
                let first = case_left.values().next().unwrap().clone();
                if case_left.values().all(|c| eq::eq(c, &first)) {
                    case_exprs = case_right.clone();
                    other_sides = case_left.values().cloned().collect();
                    switched = first;
                    has_expr = true;
                }
            }
        }
        let nps = self.next_parts(p);
        let default_part = if has_expr { nps[(1 + default_branch) as usize] } else { nps[0] };
        let mut case_body_parts: Vec<P> = Vec::new();
        let mut add_default_values: Vec<It> = Vec::new();
        let mut add_default_pos = -1;
        let mut pos = 0;
        for i in 1..nps.len() {
            if !has_expr {
                if nps[i] == default_part {
                    add_default_pos = case_values.len() as i32;
                    add_default_values.push(mk(K::IntegerValue(pos), None, ld.line_start, prec::PRIMARY, None));
                    pos += 1;
                    continue;
                }
                case_values.push(mk(K::IntegerValue(pos), None, ld.line_start, prec::PRIMARY, None));
            } else if let Some(e) = case_exprs.get(&pos) {
                let mut e = e.clone();
                let cc = case_comma.get(&pos).cloned().unwrap_or_default();
                if !cc.is_empty() {
                    let mut v = cc;
                    v.push(e.clone());
                    let ls = e.borrow().line_start;
                    e = mk(K::Comma { cmds: new_list(v) }, None, ls, prec::COMMA, None);
                }
                case_values.push(e);
            } else {
                pos += 1;
                continue;
            }
            case_body_parts.push(nps[i]);
            pos += 1;
        }
        self.make_all_commands(current_ret, stack);
        let src = original_switched.borrow().src;
        let (sw, next, ti) = self.handle_switch(pc, visited, ld, stack, stop, switched, src, case_values, default_part, case_body_parts);
        if let K::Switch { add_default_pos: adp, add_default_values: adv, .. } = &mut sw.borrow_mut().k {
            *adp = add_default_pos;
            *adv = new_list(add_default_values);
        }
        self.check_switch(ld, &sw, &other_sides, current_ret);
        current_ret.borrow_mut().push(sw);
        if next.is_some() {
            match ti {
                Some(t) => ret.borrow_mut().push(t),
                None => *next_one = next,
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn if_part(
        &mut self,
        pc: &mut Pc,
        visited: &mut Vec<P>,
        ld: &mut Ld,
        stack: &mut St,
        p: P,
        stop: &Stops,
        current_ret: &mut List,
        ret: &mut List,
        expr: It,
        next_one: &mut Option<P>,
        is_loop: &mut bool,
        current_loop: &mut Option<usize>,
        loop_item: &mut Option<It>,
        li: &mut Option<It>,
        loop_type_found: &mut bool,
        do_while_candidate: &mut bool,
        precontinue: &mut List,
        s_pre_loop: &mut Option<St>,
        recursion: usize,
    ) {
        let mut expr = expr;
        let nps = self.next_parts(p);
        let is_empty = nps[0] == nps[1];
        let mut next = self.get_common_part(ld, Some(p), &nps, pc);
        if ld.second_pass.is_some() {
            if let Some(nx) = next {
                self.detect_always_break(pc, visited, ld, p, nx, current_ret, ret, is_loop, current_loop, loop_item, li, loop_type_found, do_while_candidate, precontinue, s_pre_loop, stop, recursion);
            }
        }
        let true_stack = st_clone(stack);
        let false_stack = st_clone(stack);
        true_stack.borrow_mut().clear();
        false_stack.borrow_mut().clear();
        if is_empty {
            next = Some(nps[0]);
        }
        let mut has_on_true = Some(nps[1]) != next;
        let mut has_on_false = Some(nps[0]) != next;
        if self.parts[nps[1]].start >= self.code_len() || self.parts[nps[0]].start >= self.code_len() {
            next = None;
            has_on_true = true;
            has_on_false = true;
        }
        let mut stop2 = stop.clone();
        if !is_empty {
            if let Some(nx) = next {
                stop2.push((nx, StopKind::BlockClose));
            }
        }
        let mut on_true = empty_list();
        if !is_empty && has_on_true {
            let mut bld = self.prepare_branch_local_data(ld);
            on_true = self.print_graph(pc, visited, &mut bld, &true_stack, Some(p), Some(nps[1]), Some(stop2.clone()), None, recursion + 1);
        }
        let mut on_false = empty_list();
        if !is_empty && has_on_false {
            let mut bld = self.prepare_branch_local_data(ld);
            on_false = self.print_graph(pc, visited, &mut bld, &false_stack, Some(p), Some(nps[0]), Some(stop2.clone()), None, recursion + 1);
        }
        self.make_all_commands(&on_true, &true_stack);
        self.make_all_commands(&on_false, &false_stack);
        let mut add_after_if: Option<It> = None;
        if let (Some(ct), Some(cf)) = (loop_id_of_last(&on_true, is::continue_), loop_id_of_last(&on_false, is::continue_)) {
            if ct == cf {
                let c = pop_last(&on_true).unwrap();
                pop_last(&on_false);
                add_after_if = Some(c);
            }
        }
        let t_push = last_is(&on_true, is::push);
        let f_push = last_is(&on_false, is::push);
        let commaisable = |l: &List| !l.borrow().iter().any(is::block);
        if !on_true.borrow().is_empty() && !on_false.borrow().is_empty() && t_push && f_push && commaisable(&on_true) && commaisable(&on_false) {
            let branch = |l: &List| -> It {
                let n = l.borrow().len();
                if n > 1 {
                    let v = l.borrow()[n - 1].borrow().val();
                    l.borrow_mut()[n - 1] = v;
                    mk(K::Comma { cmds: l.clone() }, None, None, prec::COMMA, None)
                } else {
                    let f = l.borrow()[0].clone();
                    let v = f.borrow().val();
                    v
                }
            };
            let tt = branch(&on_true);
            let tf = branch(&on_false);
            let top = ternar(None, ld.line_start, invert(&expr, None), tt, tf);
            let h = self.handle_ternar(top);
            stack.borrow_mut().push(h);
        } else {
            let mut is_if = true;
            let mut filtered_true = on_true.clone();
            if on_true.borrow().is_empty() && !on_false.borrow().is_empty() {
                expr = invert(&expr, None);
                std::mem::swap(&mut on_true, &mut on_false);
                filtered_true = on_true.clone();
            }
            let ft: Vec<It> = filtered_true.borrow().clone();
            let pop_shape = (ft.len() == 1 && is::pop(&ft[0])) || (ft.len() >= 2 && is::pop(&ft[0]) && is::push(ft.last().unwrap()));
            if !stack.borrow().is_empty() && pop_shape {
                if ft.len() > 1 {
                    let right = ft.last().unwrap().borrow().val();
                    let prev_expr = stack.borrow_mut().pop();
                    let left = not_coerced_no_dup(&expr);
                    let inverted_left = if is::not(&left) { left.borrow().val() } else { invert(&left, None) };
                    let prev_expr = through_duplicate(&prev_expr);
                    let ls = ld.line_start;
                    let drop_set_temp = |cr: &List, d: &It, stack: &St| {
                        let K::Duplicate { temp } = d.borrow().k else { return };
                        let last = last_of(cr);
                        if let Some(st_) = last {
                            if let K::SetTemporary { temp: t2, .. } = st_.borrow().k {
                                if t2 == temp {
                                    cr.borrow_mut().pop();
                                    stack.borrow_mut().move_to_stack(cr);
                                }
                            }
                        }
                    };
                    if is::duplicate(&left) || same(&not_coerced(&left), &prev_expr) {
                        if is::duplicate(&left) && last_is(current_ret, is::set_temporary) {
                            drop_set_temp(current_ret, &left, stack);
                        }
                        is_if = false;
                        let v = if is::false_(&not_coerced_no_dup(&prev_expr)) {
                            right
                        } else if is::false_(&not_coerced_no_dup(&right)) {
                            prev_expr
                        } else {
                            or_item(None, ls, prev_expr, right)
                        };
                        stack.borrow_mut().push(v);
                    } else if is::duplicate(&not_coerced_no_dup(&inverted_left)) || same(&not_coerced(&inverted_left), &prev_expr) {
                        let li_ = not_coerced_no_dup(&invert(&left, None));
                        if is::duplicate(&li_) && last_is(current_ret, is::set_temporary) {
                            drop_set_temp(current_ret, &li_, stack);
                        }
                        is_if = false;
                        let v = if is::true_(&not_coerced_no_dup(&prev_expr)) {
                            right
                        } else if is::true_(&not_coerced_no_dup(&right)) {
                            prev_expr
                        } else {
                            and_item(None, ls, prev_expr, right)
                        };
                        stack.borrow_mut().push(v);
                    } else if is::false_(&prev_expr) {
                        is_if = false;
                        let left = invert(&left, None);
                        let v = if is::true_(&not_coerced_no_dup(&left)) {
                            right
                        } else if is::true_(&not_coerced_no_dup(&right)) {
                            left
                        } else {
                            and_item(None, ls, left, right)
                        };
                        stack.borrow_mut().push(v);
                    } else if is::true_(&prev_expr) {
                        is_if = false;
                        let v = if is::false_(&not_coerced_no_dup(&left)) {
                            right
                        } else if is::false_(&not_coerced_no_dup(&right)) {
                            left
                        } else {
                            or_item(None, ls, left, right)
                        };
                        stack.borrow_mut().push(v);
                    } else {
                        stack.borrow_mut().push(prev_expr);
                    }
                } else {
                    is_if = false;
                }
            }
            if is_if {
                self.make_all_commands(current_ret, stack);
                let b = if_item(None, ld.line_start, invert(&expr, None), on_true.clone(), on_false.clone());
                if let K::If { decision_part, on_true_part, on_false_part, .. } = &mut b.borrow_mut().k {
                    *decision_part = Some(p);
                    *on_true_part = Some(nps[0]);
                    *on_false_part = Some(nps[1]);
                }
                current_ret.borrow_mut().push(b.clone());
                if self.process_sub_blk(&b, None) {
                    stack.borrow_mut().push(mk(K::Pop, None, ld.line_start, prec::PRIMARY, None));
                }
            }
        }
        if let Some(a) = add_after_if {
            current_ret.borrow_mut().push(a);
        }
        if next.is_some() {
            *next_one = next;
        }
    }

    /// The "detect forward jumps (breaks) in always-break loops" block of `printGraph`.
    #[allow(clippy::too_many_arguments)]
    fn detect_always_break(
        &mut self,
        pc: &mut Pc,
        visited: &mut Vec<P>,
        ld: &mut Ld,
        part: P,
        next: P,
        current_ret: &mut List,
        ret: &mut List,
        is_loop: &mut bool,
        current_loop: &mut Option<usize>,
        loop_item: &mut Option<It>,
        li: &mut Option<It>,
        loop_type_found: &mut bool,
        do_while_candidate: &mut bool,
        precontinue: &mut List,
        s_pre_loop: &mut Option<St>,
        stop: &Stops,
        recursion: usize,
    ) {
        let mut ig: Vec<P> = Vec::new();
        let mut back_edges: Vec<Edge> = Vec::new();
        for &l in &pc.loops {
            let lp = &self.arena[l];
            if let Some(cont) = lp.cont {
                for be in lp.back_edges.iter() {
                    edges_insert(&self.parts, &mut back_edges, Edge { from: Some(be), to: cont });
                }
            }
            if lp.phase == 1 {
                if let Some(c) = lp.cont {
                    if !self.pcontains(&ig, c) {
                        ig.push(c);
                    }
                }
                if let Some(b) = lp.brk {
                    if !self.pcontains(&ig, b) {
                        ig.push(b);
                    }
                }
            }
        }
        let mut s: std::collections::VecDeque<P> = std::collections::VecDeque::from([next]);
        let mut v: Vec<P> = Vec::new();
        let mut reachable: Option<Vec<P>> = None;
        'loops: while let Some(p) = s.pop_front() {
            if !self.pcontains(&v, p) {
                v.push(p);
            }
            if p == part {
                continue;
            }
            for r in self.parts[p].refs.clone() {
                if edges_contain(&self.parts, &back_edges, &Edge { from: Some(r), to: p }) {
                    continue;
                }
                if reachable.is_none() {
                    reachable = Some(self.reachable_bits(ld, part, pc, true));
                }
                if !reachable.as_ref().unwrap().contains(&r) {
                    continue;
                }
                if r == part || self.pcontains(&ig, r) || self.pcontains(&v, r) {
                    continue;
                }
                for n in self.next_parts(r) {
                    if self.pcontains(&v, n) {
                        continue;
                    }
                    if !self.leads_to(ld, n, next, pc, true) {
                        let n2 = self.get_common_part(ld, Some(r), &[next, n], pc);
                        if let Some(n2) = n2 {
                            if self.parts[n2].start < self.code_len() {
                                let mut el = Loop::new(pc.loops.len() as i64, Some(part), Some(n2));
                                el.phase = 1;
                                self.arena.push(el);
                                let eli = self.arena.len() - 1;
                                pc.loops.push(eli);
                                reachable = None;
                                if !self.pcontains(&ig, n2) {
                                    ig.push(n2);
                                }
                                let commands = empty_list();
                                let new_loop = mk(K::While { loop_id: self.arena[eli].id, expr: new_list(vec![true_item(None, None)]), cmds: commands.clone(), label_used: false, universal: true }, None, None, prec::NOPRECEDENCE, None);
                                current_ret.borrow_mut().push(new_loop.clone());
                                if let Some(cl) = *current_loop {
                                    let mut ll = LoopLocal {
                                        part: None,
                                        loop_item: loop_item.clone().unwrap(),
                                        li: li.clone().unwrap(),
                                        current_loop: cl,
                                        loop_type_found: *loop_type_found,
                                        do_while_candidate: *do_while_candidate,
                                        precontinue: precontinue.clone(),
                                        stop: stop.clone(),
                                        ret: ret.clone(),
                                        s_pre_loop: s_pre_loop.clone().unwrap(),
                                    };
                                    let _ = *is_loop;
                                    self.handle_loop(pc, &mut ll, visited, ld, recursion);
                                    self.arena[cl].phase = 1;
                                }
                                *loop_item = Some(new_loop.clone());
                                *li = Some(new_loop);
                                *loop_type_found = true;
                                *do_while_candidate = false;
                                *precontinue = empty_list();
                                *s_pre_loop = Some(st(Stack::new(true, ld.max_temp.clone())));
                                *current_loop = Some(eli);
                                *ret = current_ret.clone();
                                *current_ret = commands;
                                *is_loop = true;
                                if !self.pcontains(&ig, next) {
                                    s.clear();
                                    s.push_back(next);
                                    v.clear();
                                    continue 'loops;
                                }
                            }
                        }
                    }
                }
                s.push_back(r);
            }
        }
    }

    fn handle_loop(&mut self, pc: &mut Pc, ll: &mut LoopLocal, visited: &mut Vec<P>, ld: &mut Ld, recursion: usize) {
        let cl = ll.current_loop;
        let cid = self.arena[cl].id;
        let cmds = loop_commands(&ll.loop_item);
        self.process_ifs(&cmds);
        self.process_switches(&cmds, cid);
        self.process_other(&cmds, cid);
        self.check_continue_at_the_end(&cmds, cid);
        if !ll.loop_type_found && !cmds.borrow().is_empty() {
            let mut cc1: Vec<List> = Vec::new();
            get_continues_commands(&cmds, &mut cc1, cid);
            if !cc1.is_empty() && ll.do_while_candidate {
                let index = ll.ret.borrow().iter().position(|x| same(x, &ll.loop_item)).unwrap();
                ll.ret.borrow_mut().remove(index);
                let ifi = pop_last(&ll.precontinue).unwrap();
                let mut expr_list: Vec<It> = ll.precontinue.borrow().clone();
                let (t, f) = if_branches(&ifi);
                let inv = single_break_of(&t, cid) && single_continue_of(&f, cid);
                let mut expr = if_expr(&ifi);
                if inv {
                    expr = invert(&expr, None);
                }
                expr_list.push(expr.clone());
                let ls = expr.borrow().line_start;
                let dw = mk(K::DoWhile { loop_id: cid, cmds: cmds.clone(), expr: new_list(expr_list), label_used: false }, None, ls, prec::NOPRECEDENCE, None);
                ll.ret.borrow_mut().insert(index, dw.clone());
                ll.li = dw;
                ll.loop_type_found = true;
            }
        }
        if !ll.loop_type_found && !cmds.borrow().is_empty() {
            let first = cmds.borrow()[0].clone();
            if is::if_(&first) {
                let (t, f) = if_branches(&first);
                let mut body: Option<List> = None;
                let mut inverted = false;
                let mut breakpos2 = false;
                let mut add_break: Option<It> = None;
                if t.borrow().len() == 1 && is::break_(&t.borrow()[0]) {
                    if br_id(&t.borrow()[0]) == Some(cid) {
                        body = Some(f.clone());
                        inverted = true;
                    }
                } else if f.borrow().len() == 1 && is::break_(&f.borrow()[0]) {
                    if br_id(&f.borrow()[0]) == Some(cid) {
                        body = Some(t.clone());
                    }
                } else if cmds.borrow().len() == 2 && is::break_(&cmds.borrow()[1]) && f.borrow().is_empty() && !t.borrow().is_empty() {
                    let bi = cmds.borrow()[1].clone();
                    let b = if inverted { f.clone() } else { t.clone() };
                    breakpos2 = true;
                    if br_id(&bi) != Some(cid) {
                        add_break = Some(bi);
                    } else {
                        let last = last_of(&b).unwrap();
                        if !is::continue_(&last) && !is::break_(&last) && !is::exit(&last) {
                            b.borrow_mut().push(bi);
                        }
                    }
                    body = Some(b);
                }
                if let Some(body) = body {
                    let index = ll.ret.borrow().iter().position(|x| same(x, &ll.loop_item)).unwrap();
                    ll.ret.borrow_mut().remove(index);
                    let mut expr = if_expr(&first);
                    if inverted {
                        if matches!(&expr.borrow().k, K::Bin { op, .. } if op.logical()) || is::true_(&expr) || is::false_(&expr) || is::not(&expr) {
                            expr = invert(&expr, None);
                        } else {
                            let ls = expr.borrow().line_start;
                            expr = not_item(None, ls, expr.clone());
                        }
                    }
                    let expr_list = vec![expr.clone()];
                    let commands = new_list(body.borrow().clone());
                    cmds.borrow_mut().remove(0);
                    if breakpos2 {
                        cmds.borrow_mut().remove(0);
                    }
                    let rest = cmds.borrow().clone();
                    commands.borrow_mut().extend(rest);
                    self.check_continue_at_the_end(&commands, cid);
                    let final_comm = empty_list();
                    if !ll.precontinue.borrow().is_empty() {
                        let mut cc: Vec<List> = Vec::new();
                        get_continues_commands(&commands, &mut cc, cid);
                        if cc.is_empty() {
                            let pv = ll.precontinue.borrow().clone();
                            commands.borrow_mut().extend(pv);
                            ll.precontinue = empty_list();
                        } else if !commands.borrow().is_empty() && cc.len() == 1 {
                            let last = last_of(&commands).unwrap();
                            if is::break_(&last) || is::continue_(&last) || is::exit(&last) {
                                let pv = ll.precontinue.borrow().clone();
                                let at = cc[0].borrow().len() - 1;
                                let mut target = cc[0].borrow_mut();
                                for (k, x) in pv.into_iter().enumerate() {
                                    target.insert(at + k, x);
                                }
                                drop(target);
                                ll.precontinue = empty_list();
                            }
                        }
                        let pv = ll.precontinue.borrow().clone();
                        final_comm.borrow_mut().extend(pv);
                    }
                    let (src, ls) = (expr.borrow().src, expr.borrow().line_start);
                    let new_li = if !final_comm.borrow().is_empty() {
                        mk(K::For { loop_id: cid, first: empty_list(), expr: expr_list.last().unwrap().clone(), fin: final_comm, cmds: commands, label_used: false }, src, ls, prec::NOPRECEDENCE, None)
                    } else {
                        mk(K::While { loop_id: cid, expr: new_list(expr_list), cmds: commands, label_used: false, universal: false }, src, ls, prec::NOPRECEDENCE, None)
                    };
                    ll.ret.borrow_mut().insert(index, new_li.clone());
                    ll.li = new_li;
                    if let Some(b) = add_break {
                        ll.ret.borrow_mut().insert(index + 1, b);
                    }
                    ll.loop_type_found = true;
                }
            }
        }
        if !ll.loop_type_found && !ll.precontinue.borrow().is_empty() {
            let pv = ll.precontinue.borrow().clone();
            cmds.borrow_mut().extend(pv);
        }
        if !ll.loop_type_found && !cmds.borrow().is_empty() {
            let last = last_of(&cmds).unwrap();
            if is::if_(&last) {
                let (t, f) = if_branches(&last);
                let mut body: Option<List> = None;
                let mut inverted = false;
                if t.borrow().len() == 1 && is::break_(&t.borrow()[0]) {
                    if br_id(&t.borrow()[0]) == Some(cid) {
                        body = Some(f.clone());
                        inverted = true;
                    }
                } else if f.borrow().len() == 1 && is::break_(&f.borrow()[0]) && br_id(&f.borrow()[0]) == Some(cid) {
                    body = Some(t.clone());
                }
                if let Some(body) = body {
                    let found = ll.ret.borrow().iter().position(|x| same(x, &ll.loop_item));
                    if let Some(index) = found {
                        ll.ret.borrow_mut().remove(index);
                        let mut expr = if_expr(&last);
                        if inverted {
                            expr = invert(&expr, None);
                        }
                        self.check_continue_at_the_end(&body, cid);
                        if !body.borrow().is_empty() {
                            ll.ret.borrow_mut().insert(index, ll.loop_item.clone());
                        } else {
                            cmds.borrow_mut().pop();
                            let commands = new_list(cmds.borrow().clone());
                            let bv = body.borrow().clone();
                            commands.borrow_mut().extend(bv);
                            self.check_continue_at_the_end(&commands, cid);
                            let ls = expr.borrow().line_start;
                            let dw = mk(K::DoWhile { loop_id: cid, cmds: commands, expr: new_list(vec![expr]), label_used: false }, None, ls, prec::NOPRECEDENCE, None);
                            ll.ret.borrow_mut().insert(index, dw.clone());
                            ll.li = dw;
                        }
                        ll.loop_type_found = true;
                    }
                }
            }
        }
        if !ll.loop_type_found {
            self.check_continue_at_the_end(&cmds, cid);
        }
        self.arena[cl].phase = 2;
        let replaced = self.check_loop_item(&ll.ret, &ll.li, ld, &ll.s_pre_loop);
        let differs = match &replaced {
            Some(r) => !same(r, &ll.li),
            None => true,
        };
        if differs {
            let index = ll.ret.borrow().iter().position(|x| same(x, &ll.li));
            if let Some(index) = index {
                ll.ret.borrow_mut().remove(index);
                if let Some(r) = replaced {
                    ll.ret.borrow_mut().insert(index, r);
                }
            }
        }
        if let Some(b) = self.arena[cl].brk {
            let stop = ll.stop.clone();
            self.print_graph(pc, visited, ld, &ll.s_pre_loop.clone(), ll.part, Some(b), Some(stop), Some(ll.ret.clone()), recursion + 1);
        }
    }

    /// `handleSwitch`: returns the switch, the part after it and an optional loop exit item.
    #[allow(clippy::too_many_arguments)]
    pub fn handle_switch(&mut self, pc: &mut Pc, visited: &mut Vec<P>, ld: &mut Ld, stack: &St, stop: &Stops, switched: It, src: Option<usize>, case_values: Vec<It>, default_part: P, case_body_parts: Vec<P>) -> (It, Option<P>, Option<It>) {
        let mut case_values = case_values;
        let mut bodies_parts = case_body_parts;
        let mut has_default = false;
        let default_item = || mk(K::Default, None, None, prec::NOPRECEDENCE, None);
        for i in (0..bodies_parts.len()).rev() {
            if bodies_parts[i] == default_part {
                case_values.insert(i + 1, default_item());
                bodies_parts.insert(i + 1, default_part);
                has_default = true;
                break;
            }
        }
        if !has_default {
            for i in (0..bodies_parts.len()).rev() {
                if self.leads_to(ld, bodies_parts[i], default_part, pc, false) {
                    case_values.insert(i + 1, default_item());
                    bodies_parts.insert(i + 1, default_part);
                    has_default = true;
                    break;
                }
            }
        }
        if !has_default {
            for i in 0..bodies_parts.len() {
                if self.leads_to(ld, default_part, bodies_parts[i], pc, false) {
                    case_values.insert(i, default_item());
                    bodies_parts.insert(i, default_part);
                    has_default = true;
                    break;
                }
            }
        }
        if !has_default {
            case_values.push(default_item());
            bodies_parts.push(default_part);
        }
        let break_part = self.get_most_common_part(ld, &bodies_parts, pc);
        let mut cl = Loop::new(pc.loops.len() as i64, None, break_part);
        cl.phase = 1;
        self.arena.push(cl);
        let cli = self.arena.len() - 1;
        pc.loops.push(cli);
        let cid = self.arena[cli].id;
        let mut bodies: Vec<P> = Vec::new();
        for &b in &bodies_parts {
            if !self.pcontains(&bodies, b) {
                bodies.push(b);
            }
        }
        let mut i = 0;
        'li: while i < bodies.len() {
            let b = bodies[i];
            let mut j = i + 1;
            while j < bodies.len() {
                let b2 = bodies[j];
                if self.leads_to(ld, b2, b, pc, false) {
                    if self.leads_to(ld, b, b2, pc, false) {
                        j += 1;
                        continue;
                    }
                    bodies.remove(j);
                    bodies.insert(i, b2);
                    continue 'li;
                } else if j > i + 1 && self.leads_to(ld, b, b2, pc, false) {
                    bodies.remove(j);
                    bodies.insert(i + 1, b2);
                    i += 1;
                    continue 'li;
                }
                j += 1;
            }
            i += 1;
        }
        let mapping: Vec<i32> = bodies_parts.iter().map(|&b| pindex_of(&self.parts, &bodies, b).map_or(-1, |x| x as i32)).collect();
        let mut case_commands: Vec<List> = Vec::new();
        for i in 0..bodies.len() {
            let mut will_have_break = false;
            if i < bodies.len() - 1 && !self.leads_to(ld, bodies[i], bodies[i + 1], pc, false) {
                will_have_break = true;
            }
            let mut stop2 = stop.clone();
            for &b in &bodies {
                if b != bodies[i] {
                    stop2.push((b, StopKind::Other));
                }
            }
            if let Some(bp) = break_part {
                stop2.push((bp, StopKind::Other));
            }
            let sub = st_clone(stack);
            let cmds = self.print_graph(pc, visited, ld, &sub, None, Some(bodies[i]), Some(stop2), None, 0);
            if will_have_break {
                if let Some(last) = last_of(&cmds) {
                    if !is::continue_(&last) && !is::break_(&last) && !matches!(last.borrow().k, K::Goto { .. }) && !is::exit(&last) {
                        cmds.borrow_mut().push(break_item(None, ld.line_start, cid));
                    }
                }
            }
            sub.borrow_mut().finish_block(&cmds);
            case_commands.push(cmds);
        }
        for i in 0..case_commands.len() {
            if single_break_of(&case_commands[i], cid) {
                for j in i + 1..case_commands.len() {
                    if single_break_of(&case_commands[j], cid) {
                        case_commands[j - 1].borrow_mut().remove(0);
                    } else {
                        break;
                    }
                }
            }
        }
        let switch_cases: Vec<P> = mapping.iter().map(|&m| bodies[m as usize]).collect();
        ld.switch_cases.borrow_mut().push(switch_cases);
        ld.switch_breaks.borrow_mut().push(break_part);
        self.arena[cli].phase = 2;
        let ti = self.check_loop_part(break_part, stop, pc);
        let sw = mk(K::Switch { loop_id: cid, obj: switched, case_values: new_list(case_values), case_cmds: case_commands, mapping, add_default_pos: -1, add_default_values: empty_list(), label_used: false }, None, src, prec::NOPRECEDENCE, None);
        fix_switch_end(&sw);
        (sw, break_part, ti)
    }
}

/// `getContinuesCommands`.
pub fn get_continues_commands(commands: &List, result: &mut Vec<List>, loop_id: i64) {
    for ti in commands.borrow().iter() {
        if cont_id(ti) == Some(loop_id) {
            result.push(commands.clone());
        }
        if is::block(ti) {
            for s in subs(ti) {
                get_continues_commands(&s, result, loop_id);
            }
        }
    }
}

/// `fixSwitchEnd`.
pub fn fix_switch_end(sw: &It) {
    let mut b = sw.borrow_mut();
    let K::Switch { case_values, case_cmds, mapping, loop_id, .. } = &mut b.k else { return };
    let loop_id = *loop_id;
    let n = case_values.borrow().len();
    for j in 0..n {
        if !matches!(case_values.borrow()[j].borrow().k, K::Default) {
            continue;
        }
        let m = mapping[j] as usize;
        let cc = case_cmds[m].clone();
        if cc.borrow().len() == 1 && br_id(&cc.borrow()[0]) == Some(loop_id) {
            case_values.borrow_mut().remove(j);
            mapping.remove(j);
            let has_other = mapping.iter().any(|&x| x == m as i32);
            if !has_other {
                case_cmds.remove(m);
                for x in mapping.iter_mut() {
                    if *x > m as i32 {
                        *x -= 1;
                    }
                }
                if j > 0 {
                    let mp = mapping[j - 1] as usize;
                    let last = case_cmds[mp].borrow().last().cloned();
                    let ok = match &last {
                        None => true,
                        Some(l) => !(is::break_(l) || is::continue_(l) || is::exit(l)),
                    };
                    if ok {
                        case_cmds[mp].borrow_mut().push(break_item(None, None, loop_id));
                    }
                }
            }
            break;
        }
    }
    if let Some(lastc) = case_cmds.last().cloned() {
        if lastc.borrow().last().is_some_and(|x| br_id(x) == Some(loop_id)) {
            lastc.borrow_mut().pop();
        }
        if lastc.borrow().is_empty() {
            let last_index = case_cmds.len() as i32 - 1;
            let mut default_index: Option<usize> = None;
            for i in 0..mapping.len() {
                if mapping[i] == last_index && matches!(case_values.borrow()[i].borrow().k, K::Default) {
                    default_index = Some(i);
                    break;
                }
            }
            if let Some(di) = default_index {
                let cnt2 = mapping.iter().filter(|&&x| x == last_index).count();
                case_values.borrow_mut().remove(di);
                mapping.remove(di);
                if cnt2 == 1 {
                    case_cmds.pop();
                }
            }
        }
    }
    if let Some(lastc) = case_cmds.last().cloned() {
        if lastc.borrow().last().is_some_and(|x| br_id(x) == Some(loop_id)) {
            lastc.borrow_mut().pop();
        }
    }
}
