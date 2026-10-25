//! `ActionDefineFunctionPushRegistersCleaner`: drops the register save/restore some compilers
//! wrap around `DefineFunction` bodies, before decompiling.

use super::super::{Action, ActionList, Op, Value};

fn is_simple(a: &Action, code: u8) -> bool {
    matches!(a.op, Op::Simple) && a.code == code
}

fn is_pop(a: &Action) -> bool {
    is_simple(a, 0x17)
}

fn is_return(a: &Action) -> bool {
    is_simple(a, 0x3e)
}

fn store_reg(a: &Action) -> Option<u8> {
    match a.op {
        Op::StoreRegister(r) => Some(r),
        _ => None,
    }
}

/// `cleanDefineFunctionRegistersForDecompilation`: a cleaned copy when there are DefineFunctions.
pub fn clean(list: &ActionList) -> ActionList {
    if !list.actions.iter().any(|a| matches!(a.op, Op::DefineFunction { .. })) {
        return list.clone();
    }
    let mut copy = list.clone();
    let mut i = copy.actions.len();
    while i > 0 {
        i -= 1;
        let a = &copy.actions[i];
        if !matches!(a.op, Op::DefineFunction { .. }) {
            continue;
        }
        let sizes = a.container_sizes().unwrap();
        let end = a.address + a.bytes_len(copy.charset) + sizes[0];
        let Some(last) = copy.index_of_address(end) else { continue };
        let start = i + 1;
        if last >= start {
            clean_push_registers(&mut copy, start, last - start);
        }
    }
    copy
}

fn clean_push_registers(code: &mut ActionList, start: usize, mut count: usize) -> bool {
    if count == 0 {
        return false;
    }
    let mut pushed: Vec<u8> = Vec::new();
    let mut pos = start;
    'regs: while let Op::Push { values, .. } = &code.actions[pos].op {
        for v in values {
            match v {
                Value::Register(r) => pushed.push(*r),
                _ => break 'regs,
            }
        }
        pos += 1;
        if pos >= code.actions.len() {
            return false;
        }
    }
    if pushed.is_empty() {
        return false;
    }
    if clean_try_finally(code, start, count, pos, &pushed) {
        return true;
    }
    if clean_stack_swap_returns(code, start, count, &pushed) {
        return true;
    }
    let mut return_reg: i32 = -1;
    let mut pos = (start + count - 1) as i64;
    let at = |code: &ActionList, p: i64| -> Option<Action> { (p >= 0).then(|| code.actions.get(p as usize).cloned()).flatten() };
    if at(code, pos).is_some_and(|a| is_return(&a)) {
        pos -= 1;
        if pos < start as i64 {
            return false;
        }
        let Some(Op::Push { values, .. }) = at(code, pos).map(|a| a.op) else { return false };
        if values.len() != 1 {
            return false;
        }
        let Value::Register(r) = values[0] else { return false };
        return_reg = r as i32;
        pos -= 1;
        if pos < start as i64 {
            return false;
        }
    }
    for &expected in &pushed {
        if !at(code, pos).is_some_and(|a| is_pop(&a)) {
            return false;
        }
        pos -= 1;
        if pos < start as i64 {
            return false;
        }
        if at(code, pos).and_then(|a| store_reg(&a)) != Some(expected) {
            return false;
        }
        pos -= 1;
        if pos < start as i64 {
            return false;
        }
    }
    let mut jumps_to_return: Vec<usize> = Vec::new();
    let pos_before_finish;
    let mut action_before_finish: Option<Action> = None;
    if return_reg > -1 {
        if !at(code, pos).is_some_and(|a| is_pop(&a)) {
            return false;
        }
        pos -= 1;
        if pos < start as i64 {
            return false;
        }
        if at(code, pos).and_then(|a| store_reg(&a)).map(|r| r as i32) != Some(return_reg) {
            return false;
        }
        let mut with_refs = pos as usize;
        if let Some(Op::Jump { offset: 0, .. }) = at(code, pos - 1).map(|a| a.op) {
            if code.references_for(pos as usize).len() == 1 {
                with_refs = (pos - 1) as usize;
                pos -= 1;
            }
        }
        for r in code.references_for(with_refs) {
            if !matches!(code.actions[r].op, Op::Jump { .. }) {
                return false;
            }
            jumps_to_return.push(r);
        }
        pos -= 1;
        let prev = at(code, pos);
        if !prev.as_ref().is_some_and(|a| matches!(a.op, Op::Jump { .. })) {
            action_before_finish = prev;
        }
        pos_before_finish = pos;
    } else {
        pos_before_finish = pos;
    }
    jumps_to_return.sort_by(|a, b| b.cmp(a));
    jumps_to_return.dedup();
    for &index in &jumps_to_return {
        let mut ret = Action::new(0x3e, Op::Simple);
        ret.file_offset = code.actions[index].file_offset;
        code.remove_action(index);
        code.add_action(index, ret);
    }
    let mut pos_before_finish = pos_before_finish;
    if return_reg > -1 {
        if let Some(b) = &action_before_finish {
            let undefined_push = matches!(&b.op, Op::Push { values, .. } if values.len() == 1 && values[0] == Value::Undefined);
            if undefined_push {
                code.remove_action(pos_before_finish as usize);
                pos_before_finish -= 1;
                count -= 1;
            } else if is_return(b) || matches!(b.op, Op::Jump { .. }) {
            } else {
                pos_before_finish += 1;
                let mut ret = Action::new(0x3e, Op::Simple);
                ret.file_offset = code.actions[pos_before_finish as usize].file_offset;
                code.add_action(pos_before_finish as usize, ret);
                count += 1;
            }
        }
    }
    let remove_start = (pos_before_finish + 1) as usize;
    let remove_count = (start + count).saturating_sub(remove_start);
    code.remove_actions(remove_start, remove_count);
    remove_register_pushes(code, start, pushed.len());
    true
}

fn clean_stack_swap_returns(code: &mut ActionList, start: usize, count: usize, regs: &[u8]) -> bool {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut nested_end: i64 = -1;
    let end = start + count;
    for i in start..end {
        let a = &code.actions[i];
        if (a.address as i64) < nested_end {
            continue;
        }
        if matches!(a.op, Op::DefineFunction { .. } | Op::DefineFunction2(_)) {
            nested_end = (a.address + a.bytes_len(code.charset) + a.container_sizes().unwrap()[0]) as i64;
            continue;
        }
        if is_return(a) {
            let Some(cs) = i.checked_sub(regs.len() * 3) else { return false };
            if !matches_restore(code, cs, start, regs, true) {
                return false;
            }
            ranges.push((cs, regs.len() * 3));
        }
    }
    if ranges.is_empty() {
        return false;
    }
    if !is_return(&code.actions[end - 1]) {
        let Some(cs) = end.checked_sub(regs.len() * 2) else { return false };
        if !matches_restore(code, cs, start, regs, false) {
            return false;
        }
        ranges.push((cs, regs.len() * 2));
    }
    for &(s, n) in ranges.iter().rev() {
        code.remove_actions(s, n);
    }
    remove_register_pushes(code, start, regs.len());
    true
}

fn matches_restore(code: &ActionList, mut pos: usize, start: usize, regs: &[u8], returning: bool) -> bool {
    if pos <= start {
        return false;
    }
    for &r in regs.iter().rev() {
        let get = |p: usize| code.actions.get(p);
        if returning {
            if !get(pos).is_some_and(|a| is_simple(a, 0x4d)) {
                return false;
            }
            pos += 1;
        }
        if get(pos).and_then(store_reg) != Some(r) {
            return false;
        }
        pos += 1;
        if !get(pos).is_some_and(is_pop) {
            return false;
        }
        pos += 1;
    }
    true
}

fn clean_try_finally(code: &mut ActionList, start: usize, count: usize, pos: usize, regs: &[u8]) -> bool {
    let Op::Try(t) = &code.actions[pos].op else { return false };
    let t = t.clone();
    let end = start + count;
    let Some(cleanup) = end.checked_sub(regs.len() * 2) else { return false };
    let last = &code.actions[end - 1];
    let end_addr = last.address + last.total_len();
    let a = &code.actions[pos];
    let sizes = a.container_sizes().unwrap();
    if t.catch_flag || !t.finally_flag || sizes.get(1).copied() != Some((regs.len() * 5) as u64) {
        return false;
    }
    if a.address + a.bytes_len(code.charset) + t.try_size as u64 + sizes[1] != end_addr || !matches_restore(code, cleanup, start, regs, false) {
        return false;
    }
    code.remove_actions(cleanup, regs.len() * 2);
    code.remove_action(pos);
    remove_register_pushes(code, start, regs.len());
    true
}

fn remove_register_pushes(code: &mut ActionList, start: usize, count: usize) {
    let mut pos = start as i64;
    let mut left = count;
    while let Some(Op::Push { values, pool }) = code.actions.get(pos as usize).map(|a| a.op.clone()) {
        let mut kept = Vec::new();
        for v in &values {
            if left > 0 && matches!(v, Value::Register(_)) {
                left -= 1;
            } else {
                kept.push(v.clone());
            }
        }
        if kept.len() != values.len() {
            code.remove_action(pos as usize);
            if !kept.is_empty() {
                let mut p = Action::new(0x96, Op::Push { values: kept, pool });
                p.length = p.content_len(code.charset);
                code.add_action(pos as usize, p);
            } else {
                pos -= 1;
            }
        }
        if left == 0 {
            break;
        }
        pos += 1;
    }
}
