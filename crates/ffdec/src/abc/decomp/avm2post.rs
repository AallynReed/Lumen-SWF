//! AVM2Graph's loop and final-process passes, and the `translate` driver.

use std::rc::Rc;

use super::avm2graph::*;
use super::graph::*;
use super::item::*;
use super::item::PartId as P;
use super::ld::{Ld, SecondPass};
use super::parts::*;
use super::post::FinalData;
use super::props::*;
use super::stack::Stack;
use super::structure::{br_id, fix_switch_end, if_branches, if_expr};

fn last(l: &List) -> Option<It> {
    l.borrow().last().cloned()
}

fn int_value(it: &It) -> Option<i32> {
    match &it.borrow().k {
        K::IntegerValue(v) | K::IntegerValueA { v, .. } => Some(*v),
        _ => None,
    }
}

fn next_parts(it: &It) -> Option<(It, It)> {
    match &it.borrow().k {
        K::NextValue { index, obj, .. } | K::NextName { index, obj, .. } => Some((index.clone(), obj.clone())),
        _ => None,
    }
}

impl<'t, 'c, 'a> G<'t, 'c, 'a> {
    /// AVM2Graph's `checkLoop(output, loopItem, ...)`: for-in, for-each and XML filters.
    pub fn check_loop_item(&self, output: &List, li: &It, ld: &mut Ld, stack: &St) -> Option<It> {
        if self.do_not_process_ifs {
            return Some(li.clone());
        }
        let (wexpr, wcmds, wid) = match &li.borrow().k {
            K::While { expr, cmds, loop_id, .. } => (expr.clone(), cmds.clone(), *loop_id),
            _ => return Some(li.clone()),
        };
        let Some(hn) = last(&wexpr).filter(|x| matches!(x.borrow().k, K::HasNext { .. })) else { return Some(li.clone()) };
        let (hindex, hobj) = match &hn.borrow().k {
            K::HasNext { index, obj } => (index.clone(), obj.clone()),
            _ => unreachable!(),
        };
        let (wsrc, wls) = (li.borrow().src, li.borrow().line_start);
        let (hsrc, hls) = (hn.borrow().src, hn.borrow().line_start);
        let filtered = matches!(not_coerced(&through_register(&not_coerced(&hobj))).borrow().k, K::FilteredCheck { .. });
        if filtered {
            let n = output.borrow().len();
            let mut pushnum = 0;
            let mut i = n as i64 - 2;
            while i >= 0 && is::push(&output.borrow()[i as usize]) {
                pushnum += 1;
                i -= 1;
            }
            let rem = n - 1 - pushnum;
            for _ in 0..pushnum {
                let p = output.borrow_mut().remove(rem);
                stack.borrow_mut().push(p.borrow().val());
            }
            if wcmds.borrow().len() >= 3 {
                let cmds: Vec<It> = wcmds.borrow().clone();
                let mut pos = 0;
                let mut kill: Vec<i32> = Vec::new();
                let add_kill = |kill: &mut Vec<i32>, v: &It| {
                    if let Some((ix, ob)) = next_parts(v) {
                        if matches!(v.borrow().k, K::NextValue { .. }) {
                            if let Some(r) = local_reg(&ix) {
                                kill.push(r);
                            }
                            if let Some(r) = local_reg(&ob) {
                                kill.push(r);
                            }
                        }
                    }
                };
                while pos < cmds.len() && is::set_local(&cmds[pos]) {
                    add_kill(&mut kill, &value(&cmds[pos]));
                    pos += 1;
                }
                let ft = cmds[pos].clone();
                let ftk = ft.borrow().k.clone();
                if let K::With { scope, .. } = &ftk {
                    if is::set_local(scope) {
                        add_kill(&mut kill, &value(scope));
                    }
                    pos += 1;
                    let mut with_cmds: Vec<It> = Vec::new();
                    while pos < cmds.len() && !matches!(cmds[pos].borrow().k, K::WithEnd { .. }) {
                        with_cmds.push(cmds[pos].clone());
                        pos += 1;
                    }
                    let mut get_local_obj_ip: i32 = -1;
                    let mut reg_index: i32 = -1;
                    let expr: It;
                    if !with_cmds.is_empty() {
                        let wl = with_cmds.last().unwrap().clone();
                        if is::if_(&wl) {
                            let (t, _) = if_branches(&wl);
                            if let Some(f0) = t.borrow().first().cloned() {
                                if let K::SetProperty { obj, .. } = &f0.borrow().k {
                                    if let Some(r) = local_reg(obj) {
                                        get_local_obj_ip = obj.borrow().src.map_or(-1, |x| x as i32);
                                        reg_index = r;
                                    }
                                }
                            }
                            let mut e = not_coerced(&if_expr(&wl));
                            if with_cmds.len() > 1 {
                                with_cmds.pop();
                                with_cmds.push(e);
                                e = mk(K::Comma { cmds: new_list(with_cmds) }, None, ld.line_start, prec::COMMA, None);
                            }
                            expr = e;
                        } else {
                            with_cmds.push(false_item(None, ld.line_start));
                            expr = mk(K::Comma { cmds: new_list(with_cmds) }, None, ld.line_start, prec::COMMA, None);
                        }
                    } else {
                        expr = false_item(None, ld.line_start);
                    }
                    let fc = not_coerced(&through_register(&hobj));
                    let fobj = match &fc.borrow().k {
                        K::FilteredCheck { obj } => obj.clone(),
                        _ => unreachable!(),
                    };
                    let filter = mk(K::Filter { expr, coll: fobj }, None, None, prec::NOPRECEDENCE, None);
                    if reg_index == -1 {
                        let n = output.borrow().len();
                        let mut i = n as i64 - 2;
                        while i >= 0 {
                            let x = output.borrow()[i as usize].clone();
                            if is::set_local(&x) {
                                let v = value(&x);
                                let mut is_xml_list = false;
                                if let K::Construct { obj, .. } = &v.borrow().k {
                                    if let K::GetLex { mn, .. } = &obj.borrow().k {
                                        is_xml_list = self.tr.ctx.names().name(*mn, &self.tr.fqn, true, true) == "XMLList";
                                    }
                                    if let K::GetProperty { obj: go, prop, .. } = &obj.borrow().k {
                                        if let K::FindProperty { prop: fp } = &go.borrow().k {
                                            is_xml_list = self.is_top_level(fp, "XMLList") && self.is_top_level(prop, "XMLList");
                                        }
                                    }
                                }
                                if is_xml_list {
                                    reg_index = set_local_reg(&x).unwrap();
                                }
                            } else if is::push(&x) {
                            } else {
                                break;
                            }
                            i -= 1;
                        }
                    }
                    kill.push(reg_index);
                    if let Some(r) = local_reg(&hobj) {
                        kill.push(r);
                    }
                    let mut set_local_ip = -1;
                    let n = output.borrow().len();
                    let mut i = n as i64 - 2;
                    while i >= 0 {
                        let x = output.borrow()[i as usize].clone();
                        if is::set_local(&x) {
                            if set_local_reg(&x) == Some(reg_index) {
                                set_local_ip = x.borrow().src.map_or(-1, |s| s as i32);
                                break;
                            }
                        } else if !is::push(&x) {
                            break;
                        }
                        i -= 1;
                    }
                    let mut usages = std::collections::BTreeSet::new();
                    if set_local_ip > -1 {
                        usages = ld.set_local_usages_of(set_local_ip);
                        usages.remove(&get_local_obj_ip);
                    }
                    let n = output.borrow().len();
                    let mut i = n as i64 - 2;
                    while i >= 0 && !kill.is_empty() {
                        let x = output.borrow()[i as usize].clone();
                        if is::set_local(&x) {
                            let r = set_local_reg(&x).unwrap();
                            if kill.contains(&r) {
                                output.borrow_mut().remove(i as usize);
                            }
                            if r == reg_index {
                                x.borrow_mut().value = Some(filter.clone());
                            }
                        } else if !is::push(&x) {
                            break;
                        }
                        i -= 1;
                    }
                    let n = output.borrow().len();
                    let mut first_push: i64 = -1;
                    let mut i = n as i64 - 2;
                    while i >= 0 && is::push(&output.borrow()[i as usize]) {
                        first_push = i;
                        i -= 1;
                    }
                    if first_push > -1 {
                        let max = output.borrow().len() - 2;
                        for _ in first_push as usize..=max {
                            let p = output.borrow_mut().remove(first_push as usize);
                            stack.borrow_mut().push(p.borrow().val());
                        }
                    }
                    if usages.is_empty() {
                        output.borrow_mut().push(filter);
                    } else {
                        ld.local_regs.borrow_mut().insert(reg_index, filter);
                    }
                    return None;
                }
            }
        } else if !wcmds.borrow().is_empty() {
            let first = wcmds.borrow()[0].clone();
            if is_set_type(&first) {
                wcmds.borrow_mut().remove(0);
                let gti = not_coerced(&value(&first));
                let var = set_type_object(&first);
                let mut collection = hobj.clone();
                let hn_ip = hsrc.map_or(-1, |x| x as i32);
                let usage_of = |reg_item: &It| reg_item.borrow().src.map_or(-1, |x| x as i32);
                if let Some(obj_reg) = local_reg(&hobj) {
                    let n = output.borrow().len();
                    let mut i = n as i64 - 2;
                    while i >= 0 {
                        let x = output.borrow()[i as usize].clone();
                        if !is::set_local(&x) {
                            break;
                        }
                        if set_local_reg(&x) == Some(obj_reg) {
                            let ip = x.borrow().src.map_or(-1, |s| s as i32);
                            let mut u = ld.set_local_usages_of(ip);
                            u.remove(&hn_ip);
                            if let Some((_, ob)) = next_parts(&gti) {
                                if local_reg(&ob) == Some(obj_reg) {
                                    u.remove(&usage_of(&ob));
                                }
                            }
                            if u.is_empty() {
                                output.borrow_mut().remove(i as usize);
                                collection = value(&x);
                            }
                        }
                        i -= 1;
                    }
                }
                if let Some(idx_reg) = local_reg(&hindex) {
                    let n = output.borrow().len();
                    let mut i = n as i64 - 2;
                    while i >= 0 {
                        let x = output.borrow()[i as usize].clone();
                        if !is::set_local(&x) {
                            break;
                        }
                        if set_local_reg(&x) == Some(idx_reg) {
                            let ip = x.borrow().src.map_or(-1, |s| s as i32);
                            let mut u = ld.set_local_usages_of(ip);
                            u.remove(&hn_ip);
                            if let Some((ix, _)) = next_parts(&gti) {
                                if local_reg(&ix) == Some(idx_reg) {
                                    u.remove(&usage_of(&ix));
                                }
                            }
                            if u.is_empty() {
                                output.borrow_mut().remove(i as usize);
                            }
                        }
                        i -= 1;
                    }
                }
                let infor = mk(K::InFor { object: var, collection }, hsrc, hls, prec::NOPRECEDENCE, None);
                let gk = gti.borrow().k.clone();
                match &gk {
                    K::NextValue { .. } => return Some(mk(K::ForEachIn { loop_id: wid, expr: infor, cmds: wcmds.clone(), label_used: false }, wsrc, wls, prec::NOPRECEDENCE, None)),
                    K::NextName { .. } => return Some(mk(K::ForIn { loop_id: wid, expr: infor, cmds: wcmds.clone(), label_used: false }, wsrc, wls, prec::NOPRECEDENCE, None)),
                    _ => {}
                }
            } else {
                let mut kind = 0;
                let mut handled = false;
                let mut check = |item: &It| {
                    if matches!(item.borrow().k, K::NextName { .. } | K::NextValue { .. }) {
                        if handled {
                            return;
                        }
                        handled = true;
                        let is_value = matches!(item.borrow().k, K::NextValue { .. });
                        if let K::NextName { local_reg, .. } | K::NextValue { local_reg, .. } = &mut item.borrow_mut().k {
                            *local_reg = Some(hindex.clone());
                        }
                        kind = if is_value { 1 } else { 2 };
                    }
                };
                let done = std::cell::Cell::new(false);
                visit_recursively(&first, &mut |i| {
                    if done.get() {
                        return false;
                    }
                    if matches!(i.borrow().k, K::NextName { .. } | K::NextValue { .. }) {
                        done.set(true);
                        check(i);
                        return false;
                    }
                    true
                });
                let infor = || mk(K::InFor { object: hindex.clone(), collection: hobj.clone() }, hsrc, hls, prec::NOPRECEDENCE, None);
                if kind == 1 {
                    return Some(mk(K::ForEachIn { loop_id: wid, expr: infor(), cmds: wcmds.clone(), label_used: false }, wsrc, wls, prec::NOPRECEDENCE, None));
                }
                if kind == 2 {
                    return Some(mk(K::ForIn { loop_id: wid, expr: infor(), cmds: wcmds.clone(), label_used: false }, wsrc, wls, prec::NOPRECEDENCE, None));
                }
            }
        }
        Some(li.clone())
    }

    /// `FullMultinameAVM2Item.isTopLevel`.
    fn is_top_level(&self, fm: &It, tname: &str) -> bool {
        let K::FullMultiname { mn, name, ns, .. } = &fm.borrow().k else { return false };
        let names = self.tr.ctx.names();
        let cname = match name {
            Some(_) => return false,
            None => names.name(*mn, &self.tr.fqn, true, true),
        };
        if ns.is_some() {
            return false;
        }
        let cns = match names.mn(*mn) {
            Some(crate::abc::Multiname::QName { ns, .. }) if *ns != 0 && self.tr.ctx.abc.constants.namespaces.get(*ns).is_some_and(|n| n.name != 0) => names.ns_name(*ns).printable(&self.tr.ctx.ids),
            _ => String::new(),
        };
        cname == tname && cns.is_empty()
    }

    /// AVM2Graph's `finalProcessAfter`.
    pub fn final_process_after(&self, list: &List, level: i32, fd: &FinalData) {
        self.final_process_after_generic(list, level);
        let mut i = 0;
        while i < list.borrow().len() {
            let x = list.borrow()[i].clone();
            if is_set_type(&x) && matches!(through_duplicate(&value(&x)).borrow().k, K::ExceptionItem(_)) {
                let mut remove = matches!(x.borrow().k, K::SetSlot { .. });
                if is::set_local(&x) {
                    let ip = x.borrow().src.map_or(-1, |s| s as i32);
                    if fd.usage(ip).is_empty() {
                        remove = true;
                    }
                }
                if remove {
                    list.borrow_mut().remove(i);
                    continue;
                }
            }
            i += 1;
        }
    }

    /// AVM2Graph's `finalProcess`.
    pub fn final_process(&self, parent: Option<&It>, list: &List, level: i32, fd: &FinalData) {
        if self.do_not_process_ifs {
            return;
        }
        self.merge_double_switches(list);
        if level == 0 && last(list).is_some_and(|x| matches!(x.borrow().k, K::ReturnVoid)) {
            list.borrow_mut().pop();
        }
        if level == 0 {
            self.remove_self_assignments(list);
        }
        self.for_in_from_ifs(list);
        let mut i = 0;
        while i < list.borrow().len() {
            let x = list.borrow()[i].clone();
            let xk = x.borrow().k.clone();
            if let K::With { scope, .. } = &xk {
                if is::set_local(scope) {
                    let ip = scope.borrow().src.map_or(-1, |s| s as i32);
                    if fd.usage(ip).is_empty() {
                        let n = list.borrow().len();
                        for j in i + 1..n {
                            let y = list.borrow()[j].clone();
                            let matches = matches!(&y.borrow().k, K::WithEnd { scope: s2 } if same(s2, scope));
                            if matches {
                                let v = value(scope);
                                if let K::With { scope: s, .. } = &mut x.borrow_mut().k {
                                    *s = v.clone();
                                }
                                if let K::WithEnd { scope: s } = &mut y.borrow_mut().k {
                                    *s = v;
                                }
                            }
                        }
                    }
                }
            }
            if let Some(reg) = set_local_reg(&x) {
                let ip = x.borrow().src.map_or(-1, |s| s as i32);
                let usages = fd.usage(ip);
                let v = value(&x);
                if let K::ExceptionItem(e) = &not_coerced(&v).borrow().k {
                    if is_finally(e) {
                        list.borrow_mut().remove(i);
                        fd.temporary_registers.borrow_mut().push(reg);
                        continue;
                    }
                }
                let n = list.borrow().len();
                let returns_reg = |y: &It| matches!(y.borrow().k, K::ReturnValue | K::Throw) && local_reg(&value(y)) == Some(reg);
                if i + 1 < n {
                    let y = list.borrow()[i + 1].clone();
                    if returns_reg(&y) {
                        y.borrow_mut().value = Some(v.clone());
                        list.borrow_mut().remove(i);
                        continue;
                    }
                }
                let mut num = 0;
                while i + 1 + num < n && int_value(&list.borrow()[i + 1 + num]).is_some() && is::integer_a(&list.borrow()[i + 1 + num]) {
                    num += 1;
                }
                if num > 0 {
                    if i + 1 + num < n {
                        let y = list.borrow()[i + 1 + num].clone();
                        if returns_reg(&y) {
                            y.borrow_mut().value = Some(v.clone());
                            for _ in 0..num {
                                list.borrow_mut().remove(i + 1);
                            }
                            list.borrow_mut().remove(i);
                            continue;
                        }
                    } else if i + num < n && usages.is_empty() {
                        for _ in 0..num {
                            list.borrow_mut().remove(i + 1);
                        }
                        list.borrow_mut().remove(i);
                        continue;
                    }
                }
            }
            let x = list.borrow()[i].clone();
            if is::continue_(&x) || is::break_(&x) || matches!(x.borrow().k, K::ReturnVoid) {
                let mut j = i as i64 - 1;
                while j >= 0 && is::integer_a(&list.borrow()[j as usize]) {
                    list.borrow_mut().remove(j as usize);
                    i -= 1;
                    j -= 1;
                }
            }
            i += 1;
        }
        self.move_bottom_set_locals(parent, list, fd);
        self.final_process_generic(list);
    }

    fn merge_double_switches(&self, list: &List) {
        let mut i = 1;
        'loopi: while i < list.borrow().len() {
            let item = list.borrow()[i].clone();
            let prev = list.borrow()[i - 1].clone();
            let both = matches!(item.borrow().k, K::Switch { .. }) && matches!(prev.borrow().k, K::Switch { .. });
            if !both || !matches!(&item.borrow().k, K::Switch { obj, .. } if is::pop(obj)) {
                i += 1;
                continue;
            }
            let (pvals, pcmds, pmap, pid, pobj) = match &prev.borrow().k {
                K::Switch { case_values, case_cmds, mapping, loop_id, obj, .. } => (case_values.borrow().clone(), case_cmds.clone(), mapping.clone(), *loop_id, obj.clone()),
                _ => unreachable!(),
            };
            {
                let b = item.borrow();
                let K::Switch { case_values, .. } = &b.k else { unreachable!() };
                for v in case_values.borrow().iter() {
                    if !matches!(v.borrow().k, K::Default) && int_value(v).is_none() {
                        i += 1;
                        continue 'loopi;
                    }
                }
            }
            let mut exprs: Vec<(i32, It)> = Vec::new();
            for (j, cc) in pcmds.iter().enumerate() {
                let Some(vi) = pmap.iter().position(|&m| m == j as i32) else {
                    i += 1;
                    continue 'loopi;
                };
                let cur = pvals[vi].clone();
                let c: Vec<It> = cc.borrow().clone();
                if c.len() > 2 {
                    i += 1;
                    continue 'loopi;
                }
                let mut delta = 0;
                if matches!(cur.borrow().k, K::Default) && pcmds.len() >= 2 {
                    if let Some(f) = c.first() {
                        if is::if_(f) && is::false_(&if_expr(f)) {
                            delta += 1;
                        }
                    }
                }
                if j == pcmds.len() - 1 && c.len() == delta + 1 {
                } else if c.len() != 2 + delta {
                    i += 1;
                    continue 'loopi;
                } else if br_id(&c[delta + 1]) != Some(pid) {
                    i += 1;
                    continue 'loopi;
                }
                let Some(pushed) = c.get(delta).filter(|x| is::push(x)).and_then(|x| int_value(&x.borrow().val())) else {
                    i += 1;
                    continue 'loopi;
                };
                match exprs.iter_mut().find(|(k, _)| *k == pushed) {
                    Some(e) => e.1 = cur,
                    None => exprs.push((pushed, cur)),
                }
            }
            let mut b = item.borrow_mut();
            let K::Switch { case_values, case_cmds, mapping, add_default_pos, add_default_values, obj, loop_id, .. } = &mut b.k else { unreachable!() };
            let sid = *loop_id;
            let mut default_index: i64 = -1;
            for (k, v) in case_values.borrow().iter().enumerate() {
                if matches!(v.borrow().k, K::Default) {
                    default_index = k as i64;
                }
            }
            let adv = add_default_values.borrow().clone();
            if default_index > -1 && !adv.is_empty() && *add_default_pos > -1 {
                let adp = *add_default_pos as usize;
                let mut default_body = mapping[default_index as usize] as usize;
                if adp != default_index as usize {
                    let previous = mapping[adp] as usize;
                    let body = case_cmds.remove(default_body);
                    case_cmds.insert(previous, body.clone());
                    for m in mapping.iter_mut() {
                        if *m >= previous as i32 {
                            *m += 1;
                        }
                    }
                    default_body = previous;
                    if let Some(l) = last(&body) {
                        if !is::exit(&l) && !is::break_(&l) && !is::continue_(&l) {
                            body.borrow_mut().push(break_item(None, None, sid));
                        }
                    }
                }
                case_values.borrow_mut().remove(default_index as usize);
                mapping.remove(default_index as usize);
                case_values.borrow_mut().insert(adp, bare(K::Default, prec::NOPRECEDENCE));
                mapping.insert(adp, default_body as i32);
                for m in (0..adv.len()).rev() {
                    case_values.borrow_mut().insert(adp, adv[m].clone());
                    mapping.insert(adp, default_body as i32);
                }
            }
            let this_exprs: Vec<i32> = case_values.borrow().iter().filter_map(int_value).collect();
            let mut no_case: Vec<i32> = exprs.iter().map(|(k, _)| *k).filter(|k| !this_exprs.contains(k)).collect();
            no_case.sort();
            let mut k = 0;
            while k < case_values.borrow().len() {
                let v = case_values.borrow()[k].clone();
                if matches!(v.borrow().k, K::Default) {
                    let m = mapping[k];
                    case_values.borrow_mut().remove(k);
                    mapping.remove(k);
                    for e in &no_case {
                        let ex = exprs.iter().find(|(x, _)| x == e).unwrap().1.clone();
                        case_values.borrow_mut().insert(k, ex);
                        mapping.insert(k, m);
                        k += 1;
                    }
                    if !mapping.contains(&m) {
                        case_cmds.remove(m as usize);
                        for x in mapping.iter_mut() {
                            if *x > m {
                                *x -= 1;
                            }
                        }
                    }
                    continue;
                }
                let cv = int_value(&v).unwrap();
                match exprs.iter().find(|(x, _)| *x == cv) {
                    None => {
                        let mi = mapping[k];
                        case_values.borrow_mut().remove(k);
                        mapping.remove(k);
                        if !mapping.contains(&mi) {
                            case_cmds.remove(mi as usize);
                            for x in mapping.iter_mut() {
                                if *x > mi {
                                    *x -= 1;
                                }
                            }
                        }
                        continue;
                    }
                    Some((_, e)) => case_values.borrow_mut()[k] = e.clone(),
                }
                k += 1;
            }
            *obj = pobj;
            drop(b);
            list.borrow_mut().remove(i - 1);
            fix_switch_end(&item);
        }
    }

    fn remove_self_assignments(&self, list: &List) {
        let reg_names = &self.tr.local_reg_names;
        let mut i = 0;
        while i < list.borrow().len() {
            let x = list.borrow()[i].clone();
            let mut remove = false;
            if let K::SetProperty { obj, prop, .. } = &x.borrow().k {
                if is::find_property(obj) {
                    if let K::FullMultiname { resolved, .. } = &prop.borrow().k {
                        if let Some(r) = local_reg(x.borrow().value.as_ref().unwrap()) {
                            let name = match reg_names.get(&r) {
                                Some(n) => self.tr.ctx.ids.print(n),
                                None if r == 0 => "this".into(),
                                None => self.tr.ctx.cfg.reg_name(r),
                            };
                            if resolved.as_deref() == Some(name.as_str()) {
                                remove = true;
                            }
                        }
                    }
                }
            }
            if let K::SetSlot { slot_obj, slot_name, .. } = &x.borrow().k {
                if matches!(slot_obj.borrow().k, K::NewActivation) {
                    let sn = slot_name.map(|n| self.tr.ctx.names().name(n, &[], true, true)).unwrap_or_default();
                    if let Some(r) = local_reg(&not_coerced_no_dup(x.borrow().value.as_ref().unwrap())) {
                        if reg_names.get(&r) == Some(&sn) {
                            remove = true;
                        }
                    }
                }
            }
            if remove {
                list.borrow_mut().remove(i);
                continue;
            }
            i += 1;
        }
    }

    fn for_in_from_ifs(&self, list: &List) {
        let mut i = 0;
        while i < list.borrow().len() {
            let x = list.borrow()[i].clone();
            if !is::if_(&x) {
                i += 1;
                continue;
            }
            let e = if_expr(&x);
            let negated = is::not(&e) && matches!(e.borrow().val().borrow().k, K::HasNext { .. });
            if !matches!(e.borrow().k, K::HasNext { .. }) && !negated {
                i += 1;
                continue;
            }
            let (t, f) = if_branches(&x);
            let hn = if negated { e.borrow().val() } else { e.clone() };
            let hobj = match &hn.borrow().k {
                K::HasNext { obj, .. } => obj.clone(),
                _ => unreachable!(),
            };
            let (body, next_body) = if negated {
                let b = new_list(f.borrow().clone());
                while list.borrow().len() > i + 1 {
                    let y = list.borrow_mut().remove(i + 1);
                    b.borrow_mut().push(y);
                }
                (b, t.clone())
            } else {
                (t.clone(), f.clone())
            };
            let first_set = body.borrow().first().cloned().filter(is_set_type);
            if let Some(first) = first_set {
                let gti = not_coerced(&value(&first));
                let is_value = matches!(gti.borrow().k, K::NextValue { .. });
                let is_name = matches!(gti.borrow().k, K::NextName { .. });
                if is_value || is_name {
                    body.borrow_mut().remove(0);
                    let infor = mk(K::InFor { object: set_type_object(&first), collection: hobj }, None, None, prec::NOPRECEDENCE, None);
                    let (src, ls) = (x.borrow().src, x.borrow().line_start);
                    let k = if is_value { K::ForEachIn { loop_id: 0, expr: infor, cmds: body.clone(), label_used: false } } else { K::ForIn { loop_id: 0, expr: infor, cmds: body.clone(), label_used: false } };
                    let repl = mk(k, src, ls, prec::NOPRECEDENCE, None);
                    list.borrow_mut()[i] = repl;
                    let nb: Vec<It> = next_body.borrow().clone();
                    for (k, y) in nb.into_iter().enumerate() {
                        list.borrow_mut().insert(i + 1 + k, y);
                    }
                }
            }
            i += 1;
        }
    }

    fn move_bottom_set_locals(&self, parent: Option<&It>, list: &List, fd: &FinalData) {
        if fd.bottom_set_locals.borrow().is_empty() {
            return;
        }
        let in_bottom = |x: &It| fd.bottom_set_locals.borrow().iter().any(|b| same(b, x));
        let mut modified = false;
        let mut i = 0;
        while i < list.borrow().len() {
            let item = list.borrow()[i].clone();
            if is::loop_(&item) || matches!(item.borrow().k, K::WithEnd { .. }) {
                i += 1;
                continue;
            }
            let mut ignored: Vec<It> = Vec::new();
            visit_recursively_no_block(&item, &mut |sub, _| {
                if is::and(sub) || is::or(sub) {
                    let r = match &sub.borrow().k {
                        K::Bin { r, .. } => r.clone(),
                        _ => unreachable!(),
                    };
                    let mut check = |y: &It| {
                        if is::set_local(y) && in_bottom(y) {
                            ignored.push(y.clone());
                        }
                    };
                    check(&r);
                    visit_recursively_no_block(&r, &mut |y, _| check(y));
                }
            });
            let mut found: Vec<It> = Vec::new();
            visit_recursively_no_block(&item, &mut |sub, parents| {
                if !is::set_local(sub) || !in_bottom(sub) || ignored.iter().any(|x| same(x, sub)) {
                    return;
                }
                let mut s = parents.len() as i64 - 1;
                if s >= 0 && matches!(parents[s as usize].borrow().k, K::Coerce { .. } | K::Convert { .. }) {
                    s -= 1;
                }
                if s >= 0 {
                    let parent = &parents[s as usize];
                    let mut mv = true;
                    if is_set_type(parent) && same(&not_coerced(&value(parent)), sub) {
                        mv = false;
                    }
                    if mv {
                        found.insert(0, sub.clone());
                    }
                }
            });
            for sl in found {
                list.borrow_mut().insert(i, clone_item(&sl));
                if let K::SetLocal { hide_value, .. } = &mut sl.borrow_mut().k {
                    *hide_value = true;
                }
                i += 1;
                modified = true;
                fd.bottom_set_locals.borrow_mut().retain(|x| !same(x, &sl));
            }
            i += 1;
        }
        if modified {
            if let Some(p) = parent {
                let wi = match &p.borrow().k {
                    K::While { expr, cmds, loop_id, .. } if Rc::ptr_eq(expr, list) => Some((cmds.clone(), *loop_id)),
                    _ => None,
                };
                if let Some((cmds, wid)) = wi {
                    if !list.borrow().is_empty() {
                        let last_expr = list.borrow_mut().pop().unwrap();
                        let ifi = if_item(None, None, invert(&last_expr, None), new_list(vec![break_item(None, None, wid)]), empty_list());
                        list.borrow_mut().push(ifi);
                        let lv: Vec<It> = list.borrow().clone();
                        for (k, y) in lv.into_iter().enumerate() {
                            cmds.borrow_mut().insert(k, y);
                        }
                        list.borrow_mut().clear();
                        list.borrow_mut().push(true_item(None, None));
                    }
                }
            }
        }
    }

    /// `Graph.translate` for one pass. `Err` carries the second-pass data.
    pub fn translate_pass(&mut self, ld: &mut Ld, static_operation: i32) -> Result<List, SecondPass> {
        let mut all_parts = PartSet::new();
        for h in self.heads.clone() {
            self.populate_parts(h, &mut all_parts);
        }
        let stack = st(Stack::new(true, ld.max_temp.clone()));
        *ld.code_stats.borrow_mut() = Some(Rc::new(self.tr.stats()));
        let mut throw_states = self.get_throw_states(ld, &mut all_parts);
        self.before_get_loops(ld, &all_parts, &mut throw_states);
        let mut loops: Vec<usize> = Vec::new();
        let (sc, sb) = match &ld.second_pass {
            Some(sp) => (sp.switch_cases.clone(), sp.switch_breaks.clone()),
            None => (Vec::new(), Vec::new()),
        };
        let heads = self.heads.clone();
        {
            let mut lc = super::loops::LoopCtx { parts: &mut self.parts, arena: &mut self.arena };
            super::loops::detect_loops(&mut lc, &heads, &mut loops, &throw_states, &all_parts, &sc, &sb, ld.second_pass.is_some());
        }
        ld.in_get_loops = false;
        self.do_not_process_ifs = ld.second_pass.is_none();
        let mut pc = Pc { has_empty_stack_pops: false, found_gotos: Vec::new(), part_codes: Vec::new(), part_code_pos: Vec::new(), all_parts, loops, throw_states, static_operation };
        let mut visited: Vec<P> = Vec::new();
        let ret = self.print_graph(&mut pc, &mut visited, ld, &stack, None, Some(heads[0]), None, None, 0);
        if ld.second_pass.is_none() {
            return Err(SecondPass { switch_cases: ld.switch_cases.borrow().clone(), switch_breaks: ld.switch_breaks.borrow().clone() });
        }
        self.process_gotos(&mut pc, &ret);
        self.process_ifs(&ret);
        self.propagate_breaks(&ret);
        self.make_all_commands(&ret, &stack);
        if !pc.has_empty_stack_pops || !contains_pop_items(&ret) {
            promote_push_items(&ret);
        }
        let fd = FinalData { temporary_registers: Default::default(), register_usage: ld.set_local_usages.clone(), bottom_set_locals: ld.bottom_set_locals.clone() };
        self.final_process_all(None, &ret, 0, &fd);
        handle_set_temporary_declarations(&ret);
        Ok(ret)
    }

    fn process_gotos(&self, pc: &mut Pc, ret: &List) {
        let mut processed: Vec<List> = Vec::new();
        process_if_gotos2(&mut processed, ret);
        process_if_gotos(&mut pc.found_gotos, ret);
        process_script_end(ret);
        let mut usages: std::collections::HashMap<String, (usize, It)> = Default::default();
        for g in &pc.found_gotos {
            if let K::Goto { name, .. } = &g.borrow().k {
                let e = usages.entry(name.clone()).or_insert((0, g.clone()));
                e.0 += 1;
                e.1 = g.clone();
            }
        }
        for (_, (n, g)) in usages {
            if n == 1 {
                if let K::Goto { name, .. } = &mut g.borrow_mut().k {
                    name.clear();
                }
            }
        }
        expand_gotos(ret);
    }
}

fn goto_name(it: &It) -> Option<String> {
    match &it.borrow().k {
        K::Goto { name, .. } => Some(name.clone()),
        _ => None,
    }
}

fn process_if_gotos2(processed: &mut Vec<List>, list: &List) {
    let mut i = 0;
    while i < list.borrow().len() {
        let item = list.borrow()[i].clone();
        if is::block(&item) {
            for s in subs(&item) {
                process_if_gotos2(processed, &s);
            }
        }
        if let Some(gname) = goto_name(&item) {
            'blk: for blk in processed.iter() {
                let n = blk.borrow().len();
                for j in 0..n {
                    let is_label = matches!(&blk.borrow()[j].borrow().k, K::Label { name } if *name == gname);
                    if is_label {
                        if blk.borrow().last().is_some_and(is::exit) {
                            let siz = blk.borrow().len();
                            for k in 0..siz - j {
                                let y = blk.borrow_mut().remove(j);
                                list.borrow_mut().insert(i + 1 + k, y);
                            }
                            let g = list.borrow_mut().remove(i);
                            blk.borrow_mut().insert(j, g);
                        }
                        break 'blk;
                    }
                }
            }
        }
        i += 1;
    }
    processed.push(list.clone());
}

fn process_if_gotos(all: &mut Vec<It>, list: &List) {
    let mut i = 0;
    while i < list.borrow().len() {
        let item = list.borrow()[i].clone();
        if is::block(&item) {
            for s in subs(&item) {
                process_if_gotos(all, &s);
            }
        }
        if is::if_(&item) {
            let (t, f) = if_branches(&item);
            let lt = last(&t).and_then(|x| goto_name(&x).map(|n| (x, n)));
            let lf = last(&f).and_then(|x| goto_name(&x).map(|n| (x, n)));
            if lt.is_some() && lf.is_some() {
                list.borrow_mut().truncate(i + 1);
                let ((gt, nt), (gf, nf)) = (lt.unwrap(), lf.unwrap());
                if nt == nf && !nt.is_empty() {
                    let has_target = matches!(&gt.borrow().k, K::Goto { target: Some(_), .. });
                    let (merged, removed) = if has_target { (gt, gf) } else { (gf, gt) };
                    t.borrow_mut().pop();
                    f.borrow_mut().pop();
                    list.borrow_mut().insert(i + 1, merged);
                    all.retain(|x| !same(x, &removed));
                }
            }
            if f.borrow().is_empty() {
                if let Some((_, n1)) = last(&t).and_then(|x| goto_name(&x).map(|n| (x, n))) {
                    if let Some(n2) = list.borrow().get(i + 1).and_then(goto_name) {
                        if n1 == n2 {
                            t.borrow_mut().pop();
                        }
                    }
                }
            }
        }
        i += 1;
    }
}

fn process_script_end(ret: &List) {
    let Some(l) = last(ret) else { return };
    if is::script_end(&l) {
        ret.borrow_mut().pop();
        process_script_end(ret);
        return;
    }
    if is::block(&l) && !is::loop_(&l) {
        for s in subs(&l) {
            process_script_end(&s);
        }
    }
}

fn expand_gotos(list: &List) {
    if let Some(l) = last(list) {
        if let K::Goto { name, target: Some(t) } = &l.borrow().k.clone() {
            list.borrow_mut().pop();
            if !name.is_empty() {
                let ls = l.borrow().line_start;
                list.borrow_mut().push(mk(K::Label { name: name.clone() }, None, ls, prec::PRIMARY, None));
            }
            let tv: Vec<It> = t.borrow().clone();
            list.borrow_mut().extend(tv);
        }
    }
    let items: Vec<It> = list.borrow().clone();
    for item in items {
        if is::block(&item) {
            for s in subs(&item) {
                expand_gotos(&s);
            }
        }
    }
}

fn contains_pop_items(list: &List) -> bool {
    for item in list.borrow().iter() {
        if is::pop(item) {
            return true;
        }
        let mut found = false;
        visit_recursively(item, &mut |i| {
            if is::pop(i) {
                found = true;
                return false;
            }
            true
        });
        if found {
            return true;
        }
    }
    false
}

fn promote_push_items(list: &List) {
    let items: Vec<It> = list.borrow().clone();
    for ti in items {
        if is::push(&ti) {
            let mut b = ti.borrow_mut();
            if let K::Push { as_comment } = &mut b.k {
                *as_comment = true;
            }
            b.comment = "unpopped".into();
        }
        if is::block(&ti) {
            for s in subs(&ti) {
                promote_push_items(&s);
            }
        }
    }
}

fn handle_set_temporary_declarations(items: &List) {
    let mut i = 0;
    while i < items.borrow().len() {
        let item = items.borrow()[i].clone();
        if let K::SetTemporary { declaration, .. } = &mut item.borrow_mut().k {
            *declaration = true;
        }
        let mut decls: Vec<It> = Vec::new();
        visit_recursively_no_block(&item, &mut |sub, _| {
            if let K::SetTemporary { temp, suffix, ref_count, .. } = &sub.borrow().k {
                decls.push(mk(K::SetTemporary { temp: *temp, declaration: true, ref_count: *ref_count, suffix: suffix.clone() }, None, None, prec::ASSIGNMENT, None));
            }
        });
        for d in decls {
            items.borrow_mut().insert(i, d);
            i += 1;
        }
        if is::block(&item) {
            for s in subs(&item) {
                handle_set_temporary_declarations(&s);
            }
        }
        i += 1;
    }
}
