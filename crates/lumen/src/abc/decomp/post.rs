//! Statement-level clean-up passes on the structured tree (generic `Graph` part).

use super::graph::*;
use super::item::*;
use super::props::*;
use super::structure::{br_id, cont_id, fix_switch_end, if_branches, if_expr};

fn set_if(it: &It, expr: Option<It>, on_true: Option<List>, on_false: Option<List>) {
    if let K::If { expr: e, on_true: t, on_false: f, .. } = &mut it.borrow_mut().k {
        if let Some(x) = expr {
            *e = x;
        }
        if let Some(x) = on_true {
            *t = x;
        }
        if let Some(x) = on_false {
            *f = x;
        }
    }
}

fn last(l: &List) -> Option<It> {
    l.borrow().last().cloned()
}

fn is_brk_cont_exit(x: &It) -> bool {
    is::break_(x) || is::continue_(x) || is::exit(x)
}

/// Loop id of a loop item.
pub fn lid(it: &It) -> i64 {
    loop_id(it).expect("loop item")
}

/// `getBaseBodyCommands` of loops with a base body.
pub fn base_body(it: &It) -> Option<List> {
    match &it.borrow().k {
        K::While { cmds, .. } | K::DoWhile { cmds, .. } | K::For { cmds, .. } | K::ForIn { cmds, .. } | K::ForEachIn { cmds, .. } => Some(cmds.clone()),
        _ => None,
    }
}

fn is_universal(it: &It) -> bool {
    matches!(it.borrow().k, K::While { universal: true, .. })
}

fn switch_parts(it: &It) -> (List, Vec<List>, Vec<i32>, i64) {
    match &it.borrow().k {
        K::Switch { case_values, case_cmds, mapping, loop_id, .. } => (case_values.clone(), case_cmds.clone(), mapping.clone(), *loop_id),
        _ => panic!("not a switch"),
    }
}

impl<'t, 'c, 'a> G<'t, 'c, 'a> {
    /// `checkContinueAtTheEnd`.
    pub fn check_continue_at_the_end(&self, commands: &List, loop_id: i64) {
        let mut c = commands.borrow_mut();
        if c.is_empty() {
            return;
        }
        let mut i = c.len() as i64 - 1;
        while i >= 0 {
            let x = &c[i as usize];
            if is::continue_(x) || is::break_(x) {
                i -= 1;
                continue;
            }
            break;
        }
        if i < c.len() as i64 - 1 {
            let mut k = (i + 2) as usize;
            while k < c.len() {
                c.remove(k);
                k += 1;
            }
        }
        if c.last().is_some_and(|x| cont_id(x) == Some(loop_id)) {
            c.pop();
        }
    }

    /// `processIfs`.
    pub fn process_ifs(&self, list: &List) {
        if self.do_not_process_ifs {
            return;
        }
        let mut i = 0;
        while i < list.borrow().len() {
            let item = list.borrow()[i].clone();
            if is::loop_(&item) && is::block(&item) {
                let id = lid(&item);
                for s in subs(&item) {
                    self.process_ifs(&s);
                    self.check_continue_at_the_end(&s, id);
                }
            } else if is::block(&item) {
                for s in subs(&item) {
                    self.process_ifs(&s);
                }
            }
            if is::if_(&item) {
                let (mut on_true, mut on_false) = if_branches(&item);
                if !on_true.borrow().is_empty() && !on_false.borrow().is_empty() {
                    if let (Some(a), Some(b)) = (last(&on_true).and_then(|x| cont_id(&x)), last(&on_false).and_then(|x| cont_id(&x))) {
                        if a == b {
                            on_true.borrow_mut().pop();
                            let c = on_false.borrow_mut().pop().unwrap();
                            list.borrow_mut().insert(i + 1, c);
                        }
                    }
                }
                let n = list.borrow().len();
                if i < n - 1 {
                    let next = list.borrow()[i + 1].clone();
                    if is::continue_(&next) && !on_true.borrow().is_empty() && on_false.borrow().is_empty() {
                        if last(&on_true).and_then(|x| cont_id(&x)) == cont_id(&next) {
                            on_true.borrow_mut().pop();
                        }
                    }
                }
                if !on_true.borrow().is_empty() && !on_false.borrow().is_empty() {
                    let lf = last(&on_false).unwrap();
                    if (is::exit(&lf) || is::break_(&lf)) && last(&on_true).is_some_and(|x| is::continue_(&x)) {
                        let c = on_true.borrow_mut().pop().unwrap();
                        list.borrow_mut().insert(i + 1, c);
                    }
                }
                if !on_true.borrow().is_empty() && !on_false.borrow().is_empty() {
                    let lt = last(&on_true).unwrap();
                    if is_brk_cont_exit(&lt) {
                        let fv: Vec<It> = on_false.borrow().clone();
                        let mut l = list.borrow_mut();
                        for (k, x) in fv.into_iter().enumerate() {
                            l.insert(i + 1 + k, x);
                        }
                        drop(l);
                        on_false.borrow_mut().clear();
                    }
                }
                if let Some(lf) = last(&on_false) {
                    let lt_ok = match last(&on_true) {
                        None => true,
                        Some(lt) => !is_brk_cont_exit(&lt),
                    };
                    if is_brk_cont_exit(&lf) && !is::script_end(&lf) && lt_ok {
                        let e = invert(&if_expr(&item), None);
                        let new_false = empty_list();
                        set_if(&item, Some(e), Some(on_false.clone()), Some(new_false.clone()));
                        let tv: Vec<It> = on_true.borrow().clone();
                        let mut l = list.borrow_mut();
                        for (k, x) in tv.into_iter().enumerate() {
                            l.insert(i + 1 + k, x);
                        }
                        drop(l);
                        on_true = on_false;
                        on_false = new_false;
                    }
                }
                let n = list.borrow().len();
                if i < n - 1 {
                    let next = list.borrow()[i + 1].clone();
                    if is::break_(&next) && on_false.borrow().is_empty() && last(&on_true).is_some_and(|x| is::continue_(&x)) {
                        let e = invert(&if_expr(&item), None);
                        set_if(&item, Some(e), None, None);
                        let tv: Vec<It> = on_true.borrow().clone();
                        {
                            let mut l = list.borrow_mut();
                            for (k, x) in tv.into_iter().enumerate() {
                                l.insert(i + 2 + k, x);
                            }
                        }
                        on_true.borrow_mut().clear();
                        let b = list.borrow_mut().remove(i + 1);
                        on_true.borrow_mut().push(b);
                    }
                }
                let n = list.borrow().len();
                if i < n - 1 {
                    let lastl = list.borrow().last().cloned().unwrap();
                    if (is::exit(&lastl) || is::break_(&lastl)) && on_false.borrow().is_empty() && last(&on_true).is_some_and(|x| is::continue_(&x)) {
                        let e = invert(&if_expr(&item), None);
                        set_if(&item, Some(e), None, None);
                        let mut items: Vec<It> = Vec::new();
                        let n = list.borrow().len();
                        for _ in i..n - 1 {
                            items.push(list.borrow_mut().remove(i + 1));
                        }
                        let tv: Vec<It> = on_true.borrow().clone();
                        {
                            let mut l = list.borrow_mut();
                            for (k, x) in tv.into_iter().enumerate() {
                                l.insert(i + 1 + k, x);
                            }
                        }
                        on_true.borrow_mut().clear();
                        on_true.borrow_mut().extend(items);
                    }
                }
                if i + 1 < list.borrow().len() {
                    let next = list.borrow()[i + 1].clone();
                    if on_false.borrow().is_empty() && !on_true.borrow().is_empty() {
                        let lt = last(&on_true).unwrap();
                        if is::continue_(&lt) && is::continue_(&next) && cont_id(&lt) == cont_id(&next) {
                            on_true.borrow_mut().pop();
                        }
                        if let Some(lt) = last(&on_true) {
                            if is::break_(&lt) && is::break_(&next) && br_id(&lt) == br_id(&next) {
                                on_true.borrow_mut().pop();
                            }
                        }
                    }
                }
            }
            i += 1;
        }
    }

    /// `processSubBlk`: whether every branch of a block ends in a push; replaces them with `replacement`.
    pub fn process_sub_blk(&self, b: &It, replacement: Option<&It>) -> bool {
        let mut all_push = true;
        let mut at_least_one = false;
        for sub in subs(b) {
            if sub.borrow().is_empty() {
                continue;
            }
            let mut last_pos = sub.borrow().len() - 1;
            let mut lastv = sub.borrow()[last_pos].clone();
            let mut br: Option<It> = None;
            if is::break_(&lastv) && sub.borrow().len() >= 2 {
                br = Some(lastv.clone());
                last_pos -= 1;
                lastv = sub.borrow()[last_pos].clone();
            }
            if is::block(&lastv) {
                if !self.process_sub_blk(&lastv, replacement) {
                    all_push = false;
                } else {
                    at_least_one = true;
                }
            } else if is::push(&lastv) {
                if let Some(r) = replacement {
                    let e2 = clone_item(r);
                    e2.borrow_mut().value = lastv.borrow().value.clone();
                    sub.borrow_mut()[last_pos] = e2;
                    if br.is_some() {
                        sub.borrow_mut().pop();
                    }
                }
                at_least_one = true;
            } else if !is::exit(&lastv) {
                all_push = false;
            }
        }
        all_push && at_least_one
    }

    /// `processSwitches(list, lastLoopId)`.
    pub fn process_switches(&self, list: &List, last_loop_id: i64) {
        let mut i = 0;
        'loopi: while i < list.borrow().len() {
            let item = list.borrow()[i].clone();
            if matches!(item.borrow().k, K::Switch { .. }) {
                let (_, case_cmds, _, sid) = switch_parts(&item);
                let mut break_count = 0;
                for it in all_sub_items(&item) {
                    if br_id(&it) == Some(sid) {
                        break_count += 1;
                        if break_count > 2 {
                            i += 1;
                            continue 'loopi;
                        }
                    }
                }
                if let Some(lc) = case_cmds.last() {
                    if lc.borrow().is_empty() && break_count > 0 {
                        i += 1;
                        continue 'loopi;
                    }
                    if break_count > 0 {
                        let l = last(lc).unwrap();
                        if !is::continue_(&l) && !is::exit(&l) {
                            i += 1;
                            continue 'loopi;
                        }
                    }
                }
                let mut break_case: i64 = -1;
                for (c, cmds) in case_cmds.iter().enumerate() {
                    if let Some(l) = last(cmds) {
                        if is::break_(&l) && cmds.borrow().len() == 1 && br_id(&l) == Some(sid) {
                            break_case = c as i64;
                            break;
                        }
                    }
                    if c == case_cmds.len() - 1 {
                        match last(cmds) {
                            None => {
                                break_case = c as i64;
                                break;
                            }
                            Some(l) if !is::continue_(&l) && !is::exit(&l) => {
                                break_case = c as i64;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                if break_count == 2 {
                    if break_case <= 0 {
                        i += 1;
                        continue 'loopi;
                    }
                    let prev = case_cmds[(break_case - 1) as usize].clone();
                    match last(&prev) {
                        Some(l) if br_id(&l) == Some(sid) => {
                            prev.borrow_mut().pop();
                        }
                        _ => {
                            i += 1;
                            continue 'loopi;
                        }
                    }
                }
                let has_continues = case_cmds.iter().any(|c| last(c).and_then(|l| cont_id(&l)) == Some(last_loop_id));
                if has_continues {
                    for c in &case_cmds {
                        if last(c).and_then(|l| cont_id(&l)) == Some(last_loop_id) {
                            let n = c.borrow().len();
                            c.borrow_mut()[n - 1] = break_item(None, None, sid);
                        }
                    }
                }
                if has_continues && break_case > -1 && i + 1 < list.borrow().len() {
                    let continue_on_end = list.borrow().last().is_some_and(is::continue_);
                    let size = list.borrow().len();
                    let mut to_add: Vec<It> = Vec::new();
                    for _ in i + 1..size - if continue_on_end { 1 } else { 0 } {
                        to_add.push(list.borrow_mut().remove(i + 1));
                    }
                    let target = case_cmds[break_case as usize].clone();
                    if last(&target).is_some_and(|x| is::break_(&x)) {
                        target.borrow_mut().pop();
                    }
                    let add_break = match to_add.last() {
                        None => true,
                        Some(l) => !(is::exit(l) || is::break_(l)),
                    };
                    target.borrow_mut().extend(to_add);
                    if add_break {
                        target.borrow_mut().push(break_item(None, None, sid));
                    }
                }
                fix_switch_end(&item);
            } else if is::if_(&item) {
                let (t, f) = if_branches(&item);
                self.process_switches(&t, last_loop_id);
                self.process_switches(&f, last_loop_id);
            }
            i += 1;
        }
    }

    /// AVM2Graph's `processOther`: trailing continues inside a final try.
    pub fn process_other(&self, list: &List, last_loop_id: i64) {
        if self.as2() {
            return;
        }
        let n = list.borrow().len();
        if n == 0 {
            return;
        }
        let mut pos = n - 1;
        let lastv = list.borrow()[pos].clone();
        if is::continue_(&lastv) {
            if cont_id(&lastv) != Some(last_loop_id) {
                return;
            }
            if n < 2 {
                return;
            }
            pos -= 1;
        }
        let t = list.borrow()[pos].clone();
        let tk = t.borrow().k.clone();
        if let K::Try { try_cmds, catch_cmds, finally, .. } = &tk {
            let strip = |l: &List| {
                if last(l).and_then(|x| cont_id(&x)) == Some(last_loop_id) {
                    l.borrow_mut().pop();
                }
            };
            for cc in catch_cmds {
                strip(cc);
            }
            strip(try_cmds);
            if let Some(f) = finally {
                strip(f);
            }
        }
    }

    /// `propagateBreaks`.
    pub fn propagate_breaks(&self, list: &List) {
        let mut i = 0;
        while i < list.borrow().len() {
            let item = list.borrow()[i].clone();
            if is::block(&item) {
                for s in subs(&item) {
                    self.propagate_breaks(&s);
                }
            }
            if matches!(item.borrow().k, K::Switch { .. }) {
                self.propagate_switch(&item);
            }
            if is::loop_(&item) {
                if let Some(body) = base_body(&item) {
                    let li_id = lid(&item);
                    if !body.borrow().is_empty() {
                        let mut todos: Vec<List> = vec![body.clone()];
                        let mut j = 0;
                        while j < todos.len() {
                            let current = todos[j].clone();
                            j += 1;
                            let Some(lc) = last(&current) else { continue };
                            if is::loop_(&lc) {
                                let inner_id = lid(&lc);
                                change_continue_to_break_blk(&lc, li_id, inner_id);
                                if is_universal(&lc) {
                                    let lcmds = base_body(&lc).unwrap();
                                    if let Some(ifi) = last(&lcmds).filter(|x| is::if_(x)) {
                                        let (t, f) = if_branches(&ifi);
                                        let mut inverted = false;
                                        let mut found = false;
                                        if f.borrow().is_empty() && t.borrow().len() == 1 && br_id(&t.borrow()[0]) == Some(inner_id) {
                                            found = true;
                                            inverted = true;
                                        } else if t.borrow().is_empty() && f.borrow().len() == 1 && br_id(&f.borrow()[0]) == Some(inner_id) {
                                            found = true;
                                        }
                                        if found {
                                            lcmds.borrow_mut().pop();
                                            let mut e = if_expr(&ifi);
                                            if inverted {
                                                e = invert(&e, None);
                                            }
                                            let (src, ls) = (lc.borrow().src, lc.borrow().line_start);
                                            let dw = mk(K::DoWhile { loop_id: inner_id, cmds: lcmds.clone(), expr: new_list(vec![e]), label_used: false }, src, ls, prec::NOPRECEDENCE, None);
                                            let n = current.borrow().len();
                                            current.borrow_mut()[n - 1] = dw;
                                        }
                                    }
                                }
                            } else if is::block(&lc) {
                                let mut nt = subs(&lc);
                                if !nt.is_empty() && matches!(lc.borrow().k, K::Switch { .. }) {
                                    nt = vec![nt.last().unwrap().clone()];
                                }
                                todos.extend(nt);
                            }
                        }
                    }
                    if let K::For { first, expr, fin, cmds, .. } = item.borrow().k.clone() {
                        let has_cont = continues(&item).iter().any(|c| cont_id(c) == Some(li_id));
                        if !has_cont {
                            let (src, ls) = (item.borrow().src, item.borrow().line_start);
                            let finv = fin.borrow().clone();
                            cmds.borrow_mut().extend(finv);
                            let wh = mk(K::While { loop_id: li_id, expr: new_list(vec![expr]), cmds: cmds.clone(), label_used: false, universal: false }, src, ls, prec::NOPRECEDENCE, None);
                            list.borrow_mut()[i] = wh;
                            for f in first.borrow().iter() {
                                list.borrow_mut().insert(i, f.clone());
                                i += 1;
                            }
                        }
                    }
                }
            }
            i += 1;
        }
    }

    fn propagate_switch(&self, sw: &It) {
        let (_, case_cmds, _, sid) = switch_parts(sw);
        if case_cmds.is_empty() {
            return;
        }
        let last_case = case_cmds.last().unwrap().clone();
        if last_case.borrow().is_empty() {
            return;
        }
        for (h, com) in case_cmds.iter().enumerate() {
            if com.borrow().is_empty() {
                continue;
            }
            let is_last = h == case_cmds.len() - 1;
            let has_break = last(com).and_then(|x| br_id(&x)) == Some(sid);
            if !is_last && !has_break {
                continue;
            }
            let com2 = copy_list(com);
            if has_break {
                com2.borrow_mut().pop();
            }
            let mut todos: Vec<List> = vec![com2.clone()];
            let mut j = 0;
            while j < todos.len() {
                let current = todos[j].clone();
                j += 1;
                let Some(mut lc) = last(&current) else { continue };
                if is::loop_(&lc) {
                    if matches!(lc.borrow().k, K::Switch { .. }) {
                        let inner = lc.clone();
                        let inner_id = lid(&inner);
                        let ss = subs(&inner);
                        for (k, cc) in ss.iter().enumerate() {
                            if cc.borrow().is_empty() {
                                continue;
                            }
                            lc = last(cc).unwrap();
                            if is::break_(&lc) || is::continue_(&lc) || is::exit(&lc) || k == ss.len() - 1 {
                                change_break_to_break(cc, sid, inner_id);
                            }
                        }
                        fix_switch_end(&inner);
                    } else {
                        let inner_id = lid(&lc);
                        if let Some(bb) = base_body(&lc) {
                            change_break_to_break(&bb, sid, inner_id);
                        }
                        if is_universal(&lc) {
                            let body = base_body(&lc).unwrap();
                            let first0 = body.borrow().first().cloned();
                            if let Some(first) = first0.filter(|x| is::if_(x)) {
                                let (t, f) = if_branches(&first);
                                if f.borrow().is_empty() && t.borrow().len() == 1 && br_id(&t.borrow()[0]) == Some(inner_id) {
                                    let e = invert(&if_expr(&first), None);
                                    body.borrow_mut().remove(0);
                                    let (src, ls) = (lc.borrow().src, lc.borrow().line_start);
                                    let wh = mk(K::While { loop_id: inner_id, expr: new_list(vec![e]), cmds: body.clone(), label_used: false, universal: false }, src, ls, prec::NOPRECEDENCE, None);
                                    if std::rc::Rc::ptr_eq(&current, &com2) {
                                        let n = com.borrow().len();
                                        com.borrow_mut()[n - 1 - if has_break { 1 } else { 0 }] = wh;
                                    } else {
                                        let n = current.borrow().len();
                                        current.borrow_mut()[n - 1] = wh;
                                    }
                                }
                            }
                        }
                    }
                } else if is::block(&lc) {
                    let mut nt = subs(&lc);
                    if !nt.is_empty() && matches!(lc.borrow().k, K::Switch { .. }) {
                        nt = vec![nt.last().unwrap().clone()];
                    }
                    todos.extend(nt);
                }
            }
        }
        let mut todos: Vec<List> = vec![last_case.clone()];
        let mut j = 0;
        while j < todos.len() {
            let current = todos[j].clone();
            j += 1;
            let Some(lc) = last(&current) else { continue };
            if is::break_(&lc) {
                if br_id(&lc) == Some(sid) {
                    current.borrow_mut().pop();
                }
            } else if is::loop_(&lc) {
            } else if is::block(&lc) && !matches!(lc.borrow().k, K::Switch { .. }) {
                todos.extend(subs(&lc));
            }
        }
        if let Some(sw_inner) = last(&last_case).filter(|x| matches!(x.borrow().k, K::Switch { .. })) {
            let inner_id = lid(&sw_inner);
            for br in breaks(&sw_inner) {
                if let K::Break { loop_id, .. } = &mut br.borrow_mut().k {
                    if *loop_id == sid {
                        *loop_id = inner_id;
                    }
                }
            }
        }
    }

    /// `finalProcessAll`.
    pub fn final_process_all(&self, parent: Option<&It>, list: &List, level: i32, fd: &FinalData) {
        if self.do_not_process_ifs {
            return;
        }
        self.final_process(parent, list, level, fd);
        let items: Vec<It> = list.borrow().clone();
        for item in &items {
            if is::block(item) {
                for s in subs(item) {
                    self.final_process_all(Some(item), &s, level + 1, fd);
                }
            }
        }
        self.final_process_after(list, level, fd);
    }

    /// Generic `finalProcessAfter`.
    pub fn final_process_after_generic(&self, list: &List, level: i32) {
        let n = list.borrow().len();
        if n >= 2 {
            let lastv = list.borrow()[n - 1].clone();
            if is::exit(&lastv) && lastv.borrow().value.as_ref().is_some_and(is::pop) {
                let prev = list.borrow()[n - 2].clone();
                if is::block(&prev) && self.process_sub_blk(&prev, Some(&lastv)) {
                    list.borrow_mut().pop();
                }
            }
        }
        restructure_while_true_continue_outer(list);
        if level == 0 {
            eliminate_redundant_labeled_breaks(list, &[], None, &[]);
            remove_trailing_continues(list);
        }
    }

    /// Generic `finalProcess`: `if(true)` unwrapping and line-based for loops.
    pub fn final_process_generic(&self, list: &List) {
        let n = list.borrow().len();
        let mut to_delete = vec![false; n];
        let mut i = 0;
        while i < list.borrow().len() {
            let item = list.borrow()[i].clone();
            if is::if_(&item) && is::true_(&if_expr(&item)) {
                let (t, _) = if_branches(&item);
                list.borrow_mut().remove(i);
                let tv: Vec<It> = t.borrow().clone();
                let mut l = list.borrow_mut();
                for (k, x) in tv.into_iter().enumerate() {
                    l.insert(i + k, x);
                }
                drop(l);
                continue;
            }
            if let K::For { first, .. } = item.borrow().k.clone() {
                let line = self.item_line(&item);
                if line > 0 && i > 0 {
                    let j = i - 1;
                    let prev = list.borrow()[j].clone();
                    if self.item_line(&prev) == line && !is::loop_(&prev) {
                        first.borrow_mut().insert(0, prev);
                        if j < to_delete.len() {
                            to_delete[j] = true;
                        }
                    }
                }
            }
            if let K::While { expr, cmds, loop_id, .. } = item.borrow().k.clone() {
                let line = self.item_line(&item);
                if line > 0 {
                    let mut first_cmds: Vec<It> = Vec::new();
                    let mut fin_cmds: Vec<It> = Vec::new();
                    let mut j = i as i64 - 1;
                    while j >= 0 {
                        let x = list.borrow()[j as usize].clone();
                        if self.item_line(&x) == line && !is::loop_(&x) {
                            first_cmds.insert(0, x);
                            if (j as usize) < to_delete.len() {
                                to_delete[j as usize] = true;
                            }
                        } else {
                            break;
                        }
                        j -= 1;
                    }
                    let mut j = cmds.borrow().len() as i64 - 1;
                    while j >= 0 {
                        let x = cmds.borrow()[j as usize].clone();
                        if self.item_line(&x) == line && !is::loop_(&x) {
                            let r = cmds.borrow_mut().remove(j as usize);
                            fin_cmds.insert(0, r);
                        } else {
                            break;
                        }
                        j -= 1;
                    }
                    if !first_cmds.is_empty() || !fin_cmds.is_empty() {
                        if first_cmds.len() > 1 || fin_cmds.len() > 1 {
                            for k in 0..first_cmds.len() {
                                let idx = i - 1 - k;
                                if idx < to_delete.len() {
                                    to_delete[idx] = false;
                                }
                            }
                            cmds.borrow_mut().extend(fin_cmds);
                        } else if cmds.borrow().is_empty() && first_cmds.is_empty() {
                            cmds.borrow_mut().extend(fin_cmds);
                        } else {
                            let last_expr = expr.borrow_mut().pop().unwrap();
                            let ev: Vec<It> = expr.borrow().clone();
                            first_cmds.extend(ev);
                            let (src, ls) = (item.borrow().src, item.borrow().line_start);
                            let f = mk(K::For { loop_id, first: new_list(first_cmds), expr: last_expr, fin: new_list(fin_cmds), cmds: cmds.clone(), label_used: false }, src, ls, prec::NOPRECEDENCE, None);
                            list.borrow_mut()[i] = f;
                        }
                    }
                }
            }
            i += 1;
        }
        for i in (0..to_delete.len()).rev() {
            if to_delete[i] && i < list.borrow().len() {
                list.borrow_mut().remove(i);
            }
        }
    }

    /// `getLine()` of an item: the debug line of its source instruction.
    pub fn item_line(&self, it: &It) -> i32 {
        match it.borrow().src {
            Some(ip) if ip < self.tr.code.len() => self.tr.code.ins[ip].line,
            _ => 0,
        }
    }
}

/// `FinalProcessLocalData` (with the AVM2 additions).
pub struct FinalData {
    pub temporary_registers: std::cell::RefCell<Vec<i32>>,
    pub register_usage: super::ld::Shared<std::collections::BTreeMap<i32, std::collections::BTreeSet<i32>>>,
    pub bottom_set_locals: super::ld::Shared<Vec<It>>,
}

impl FinalData {
    pub fn usage(&self, ip: i32) -> std::collections::BTreeSet<i32> {
        self.register_usage.borrow().get(&ip).cloned().unwrap_or_default()
    }
}

/// `changeContinueToBreak(Block, ...)`.
pub fn change_continue_to_break_blk(blk: &It, cont_id_: i64, brk_id: i64) {
    for s in subs(blk) {
        change_continue_to_break(&s, cont_id_, brk_id);
    }
    if matches!(blk.borrow().k, K::Switch { .. }) {
        fix_switch_end(blk);
    }
}

pub fn change_continue_to_break(items: &List, cont_id_: i64, brk_id: i64) {
    let n = items.borrow().len();
    for i in 0..n {
        let item = items.borrow()[i].clone();
        if is::block(&item) {
            change_continue_to_break_blk(&item, cont_id_, brk_id);
        }
        if cont_id(&item) == Some(cont_id_) {
            let (src, ls) = (item.borrow().src, item.borrow().line_start);
            items.borrow_mut()[i] = break_item(src, ls, brk_id);
        }
    }
}

pub fn change_break_to_break(items: &List, from: i64, to: i64) {
    let v: Vec<It> = items.borrow().clone();
    for item in v {
        if is::block(&item) {
            for s in subs(&item) {
                change_break_to_break(&s, from, to);
            }
        }
        if let K::Break { loop_id, .. } = &mut item.borrow_mut().k {
            if *loop_id == from {
                *loop_id = to;
            }
        }
    }
}

fn remove_trailing_continues(commands: &List) {
    let v: Vec<It> = commands.borrow().clone();
    for item in v {
        if is::block(&item) {
            for s in subs(&item) {
                remove_trailing_continues(&s);
            }
        }
        if is::loop_(&item) {
            if let Some(body) = base_body(&item) {
                let id = lid(&item);
                if last(&body).and_then(|x| cont_id(&x)) == Some(id) {
                    body.borrow_mut().pop();
                }
            }
        }
    }
}

fn eliminate_redundant_labeled_breaks(commands: &List, end_targets: &[i64], loop_id: Option<i64>, exit_targets: &[i64]) {
    let mut continuation: Vec<i64> = end_targets.to_vec();
    let n = commands.borrow().len();
    for i in (0..n).rev() {
        let item = commands.borrow()[i].clone();
        if let Some(mut b) = br_id(&item) {
            if loop_id.is_some() && exit_targets.contains(&b) {
                b = loop_id.unwrap();
                if let K::Break { loop_id: l, .. } = &mut item.borrow_mut().k {
                    *l = b;
                }
            }
            continuation = vec![b];
            if loop_id == Some(b) {
                for &e in exit_targets {
                    if !continuation.contains(&e) {
                        continuation.push(e);
                    }
                }
            }
            continue;
        }
        if matches!(item.borrow().k, K::Switch { .. }) {
            let (_, case_cmds, _, sid) = switch_parts(&item);
            let switch_exit = continuation.clone();
            for (c, cc) in case_cmds.iter().enumerate() {
                let mut case_end: Vec<i64> = Vec::new();
                if c == case_cmds.len() - 1 {
                    case_end.push(sid);
                    for &e in &switch_exit {
                        if !case_end.contains(&e) {
                            case_end.push(e);
                        }
                    }
                }
                eliminate_redundant_labeled_breaks(cc, &case_end, Some(sid), &switch_exit);
            }
            fix_switch_end(&item);
        } else if is::if_(&item) {
            let (t, f) = if_branches(&item);
            eliminate_redundant_labeled_breaks(&t, &continuation, loop_id, exit_targets);
            eliminate_redundant_labeled_breaks(&f, &continuation, loop_id, exit_targets);
        } else if is::block(&item) {
            let nested = loop_id_of(&item);
            for s in subs(&item) {
                eliminate_redundant_labeled_breaks(&s, &[], nested, &[]);
            }
        }
        continuation = Vec::new();
    }
}

fn loop_id_of(it: &It) -> Option<i64> {
    loop_id(it)
}

fn count_breaks_of_loop(commands: &List, id: i64) -> usize {
    let mut c = 0;
    for ti in commands.borrow().iter() {
        if br_id(ti) == Some(id) {
            c += 1;
        }
        if is::block(ti) {
            for s in subs(ti) {
                c += count_breaks_of_loop(&s, id);
            }
        }
    }
    c
}

fn insert_before_breaks_of_loop(commands: &List, id: i64, to_insert: &[It], clone_items: bool) -> bool {
    let exits = to_insert.last().is_some_and(is::exit);
    let mut i = 0;
    while i < commands.borrow().len() {
        let ti = commands.borrow()[i].clone();
        if br_id(&ti) == Some(id) {
            for (j, ins) in to_insert.iter().enumerate() {
                let x = if clone_items { clone_item(ins) } else { ins.clone() };
                commands.borrow_mut().insert(i + j, x);
            }
            if exits {
                commands.borrow_mut().remove(i + to_insert.len());
                i += to_insert.len().saturating_sub(1);
            } else {
                i += to_insert.len();
            }
            if !clone_items {
                return true;
            }
        } else if is::block(&ti) {
            for s in subs(&ti) {
                let done = insert_before_breaks_of_loop(&s, id, to_insert, clone_items);
                if !clone_items && (done || count_breaks_of_loop(&s, id) > 0) {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

/// `restructureWhileTrueContinueOuter`.
fn restructure_while_true_continue_outer(list: &List) {
    let mut i = 0;
    while i < list.borrow().len() {
        let item = list.borrow()[i].clone();
        let (expr, cmds, wid) = match &item.borrow().k {
            K::While { expr, cmds, loop_id, .. } => (expr.clone(), cmds.clone(), *loop_id),
            _ => {
                i += 1;
                continue;
            }
        };
        if !last(&expr).is_some_and(|x| is::true_(&x)) || !cmds.borrow().first().is_some_and(is::if_) {
            i += 1;
            continue;
        }
        let ifi = cmds.borrow()[0].clone();
        let (t, f) = if_branches(&ifi);
        let mut outer: Option<It> = None;
        let mut invert_cond = false;
        let mut body_from_if: Option<List> = None;
        let mut outer_continuation: Option<Vec<It>> = None;
        let single_cont = |l: &List| l.borrow().len() == 1 && is::continue_(&l.borrow()[0]);
        if f.borrow().is_empty() && single_cont(&t) {
            outer = Some(t.borrow()[0].clone());
            invert_cond = true;
        } else if t.borrow().is_empty() && single_cont(&f) {
            outer = Some(f.borrow()[0].clone());
        } else if single_cont(&t) && !f.borrow().is_empty() {
            outer = Some(t.borrow()[0].clone());
            invert_cond = true;
            body_from_if = Some(f.clone());
        } else if single_cont(&f) && !t.borrow().is_empty() {
            outer = Some(f.borrow()[0].clone());
            body_from_if = Some(t.clone());
        } else if cmds.borrow().len() > 1 && last(&cmds).is_some_and(|x| is::continue_(&x)) {
            let trailing = last(&cmds).unwrap();
            if cont_id(&trailing) != Some(wid) {
                if f.borrow().is_empty() && !t.borrow().is_empty() {
                    outer = Some(trailing.clone());
                    body_from_if = Some(t.clone());
                } else if t.borrow().is_empty() && !f.borrow().is_empty() {
                    outer = Some(trailing.clone());
                    invert_cond = true;
                    body_from_if = Some(f.clone());
                }
                if outer.is_some() {
                    let c = cmds.borrow().clone();
                    outer_continuation = Some(c[1..c.len() - 1].to_vec());
                }
            }
        }
        let Some(outer) = outer else {
            i += 1;
            continue;
        };
        let oid = cont_id(&outer).unwrap();
        if oid == wid {
            i += 1;
            continue;
        }
        let mut after: Vec<It> = list.borrow()[i + 1..].to_vec();
        while after.last().is_some_and(|x| cont_id(x) == Some(oid)) {
            after.pop();
        }
        if outer_continuation.is_none() {
            cmds.borrow_mut().remove(0);
            if let Some(b) = &body_from_if {
                let bv: Vec<It> = b.borrow().clone();
                let mut c = cmds.borrow_mut();
                for (k, x) in bv.into_iter().enumerate() {
                    c.insert(k, x);
                }
            }
        } else {
            let bv: Vec<It> = body_from_if.as_ref().unwrap().borrow().clone();
            cmds.borrow_mut().clear();
            cmds.borrow_mut().extend(bv);
        }
        change_continue_to_break(&cmds, oid, wid);
        let bc = count_breaks_of_loop(&cmds, wid);
        if !after.is_empty() {
            if bc == 0 {
                after.clear();
            } else if bc == 1 {
                insert_before_breaks_of_loop(&cmds, wid, &after, false);
            } else {
                insert_before_breaks_of_loop(&cmds, wid, &after, true);
            }
        }
        let mut e = if_expr(&ifi);
        if invert_cond {
            e = invert(&e, None);
        }
        if let K::While { expr: we, .. } = &mut item.borrow_mut().k {
            *we = new_list(vec![e]);
        }
        list.borrow_mut().truncate(i + 1);
        if let Some(oc) = outer_continuation {
            list.borrow_mut().extend(oc);
        }
        i += 1;
    }
}
