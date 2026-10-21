//! Stack, scope and register needs of a method body, from FFDec's AVM2Code.getStats.

use std::collections::VecDeque;

use super::opcodes::{Instruction, Operand};
use super::{Abc, Multiname};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stats {
    pub max_stack: usize,
    /// Absolute: the initial scope depth plus what the code pushes.
    pub max_scope: usize,
    pub max_regs: usize,
    pub has_activation: bool,
    pub has_set_dxns: bool,
}

/// Why a body cannot pass the player's verifier, at an instruction index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    Underflow(usize),
    ScopeUnderflow(usize),
}

fn runtime_args(abc: &Abc, index: i64) -> i64 {
    match abc.constants.multinames.get(index as usize) {
        Some(Multiname::RtqName { .. }) | Some(Multiname::MultinameL { .. }) => 1,
        Some(Multiname::RtqNameL { .. }) => 2,
        _ => 0,
    }
}

/// Pops and pushes on the operand stack, and the scope stack change; None where FFDec has none.
pub(crate) fn deltas(i: &Instruction, abc: &Abc) -> Option<(i64, i64, i64)> {
    let o = &i.operands;
    let rt = || runtime_args(abc, o[0]);
    Some(match i.code {
        0x01 | 0x02 | 0x06 | 0x08 | 0x09 | 0x10 | 0x47 | 0x92 | 0x94 | 0x9d | 0x9f | 0xc2 | 0xc3 | 0xef..=0xf3 => (0, 0, 0),
        0x03 | 0x07 | 0x11 | 0x12 | 0x1b | 0x29 | 0x48 | 0x6f | 0xd4..=0xd7 | 0x63 => (1, 0, 0),
        0x0c..=0x0f | 0x13..=0x1a | 0x3a..=0x3e | 0x0b | 0x6d => (2, 0, 0),
        0x1c | 0x30 => (1, 0, 1),
        0x1d => (0, 0, -1),
        0x1e | 0x1f | 0x23 | 0x87 | 0x9a | 0x9b | 0xa0..=0xb1 | 0xb3..=0xb9 | 0xc5..=0xc7 => (2, 1, 0),
        0x20 | 0x21 | 0x24..=0x28 | 0x2c..=0x2f | 0x31..=0x34 | 0x40 | 0x54 | 0x57 | 0x5a | 0x60 | 0x62 | 0x64 | 0x65 | 0x67 | 0x6e | 0xd0..=0xd3 => (0, 1, 0),
        0x22 if i.name == "pushfloat" => (0, 1, 0),
        0x0a | 0x35..=0x39 | 0x50..=0x52 | 0x58 | 0x6c | 0x70..=0x7b | 0x80..=0x86 | 0x88..=0x91 | 0x93 | 0x95..=0x97 | 0x9c | 0x9e | 0xb2 | 0xc0 | 0xc1 | 0xc4 => (1, 1, 0),
        0x2a => (1, 2, 0),
        0x2b => (2, 2, 0),
        0x04 | 0x59 | 0x66 | 0x6a => (1 + rt(), 1, 0),
        0x05 | 0x61 | 0x68 => (2 + rt(), 0, 0),
        0x5d | 0x5e | 0x5f => (rt(), 1, 0),
        0x41 => (o[0] + 2, 1, 0),
        0x42 | 0x53 => (o[0] + 1, 1, 0),
        0x43 | 0x44 => (o[1] + 1, 1, 0),
        0x45 | 0x46 | 0x4a | 0x4c => (o[1] + 1 + rt(), 1, 0),
        0x4e | 0x4f => (o[1] + 1 + rt(), 0, 0),
        0x49 => (o[0] + 1, 0, 0),
        0x55 => (o[0] * 2, 1, 0),
        0x56 => (o[0], 1, 0),
        _ => return None,
    })
}

fn registers(i: &Instruction) -> impl Iterator<Item = usize> + '_ {
    let fixed = match i.code {
        0xd0..=0xd3 => Some((i.code - 0xd0) as usize),
        0xd4..=0xd7 => Some((i.code - 0xd4) as usize),
        _ => None,
    };
    let ops = i.kinds.iter().zip(&i.operands).filter(|(k, _)| **k == Operand::LocalRegIndex).map(|(_, v)| *v as usize);
    fixed.into_iter().chain(ops)
}

/// Walks every path from the entry and from each exception handler. None when the body has
/// an instruction or jump FFDec cannot account for.
pub fn stats(abc: &Abc, code: &[Instruction], handlers: &[usize], init_scope: usize) -> Option<(Stats, Option<Problem>)> {
    let pos = |at: usize| code.binary_search_by_key(&at, |i| i.offset).ok();
    let mut seen = vec![false; code.len()];
    let mut st = Stats { max_scope: init_scope, ..Default::default() };
    let mut problem = None;
    let mut q = VecDeque::from([(0usize, 0i64, init_scope as i64)]);
    for &h in handlers {
        q.push_back((pos(h)?, 1, init_scope as i64));
    }
    while let Some((mut p, mut stack, mut scope)) = q.pop_front() {
        while p < code.len() {
            if seen[p] {
                break;
            }
            seen[p] = true;
            let i = &code[p];
            let (pop, push, sd) = deltas(i, abc)?;
            if stack < pop && problem.is_none() {
                problem = Some(Problem::Underflow(p));
            }
            stack = (stack - pop).max(0) + push;
            scope += sd;
            if scope < 0 && problem.is_none() {
                problem = Some(Problem::ScopeUnderflow(p));
            }
            st.max_stack = st.max_stack.max(stack as usize);
            st.max_scope = st.max_scope.max(scope.max(0) as usize);
            st.has_set_dxns |= matches!(i.code, 0x06 | 0x07);
            st.has_activation |= i.code == 0x57;
            for r in registers(i) {
                st.max_regs = st.max_regs.max(r + 1);
            }
            match i.code {
                0x03 | 0x47 | 0x48 => break,
                0x10 => {
                    p = pos(i.targets()[0])?;
                    continue;
                }
                0x0c..=0x0f | 0x11..=0x1a => q.push_back((pos(i.targets()[0])?, stack, scope)),
                0x1b => {
                    for t in i.targets() {
                        q.push_back((pos(t)?, stack, scope));
                    }
                    break;
                }
                _ => {}
            }
            p += 1;
        }
    }
    Some((st, problem))
}

/// Raises a body's declared limits and method flags to what its code needs. Limits are never
/// lowered: the player only rejects ones that are too small.
pub fn fit_body(abc: &mut Abc, body: usize) -> Result<(), (usize, String)> {
    let b = &abc.bodies[body];
    let Ok(code) = super::opcodes::decode(&b.code, super::opcodes::Dialect::of(abc.major, abc.minor)) else { return Ok(()) };
    let handlers: Vec<usize> = b.exceptions.iter().map(|e| e.target).collect();
    let Some((st, problem)) = stats(abc, &code, &handlers, b.init_scope_depth) else { return Ok(()) };
    if let Some(p) = problem {
        let (at, what) = match p {
            Problem::Underflow(i) => (i, "pops more values than the stack holds"),
            Problem::ScopeUnderflow(i) => (i, "pops more scopes than the scope stack holds"),
        };
        return Err((at, format!("{} {what}", code[at].name)));
    }
    let info = &mut abc.methods[abc.bodies[body].method];
    let min_regs = info.param_types.len() + 1 + usize::from(info.flags & super::method_flags::NEED_REST != 0);
    if st.has_activation {
        info.flags |= super::method_flags::NEED_ACTIVATION;
    }
    if st.has_set_dxns {
        info.flags |= super::method_flags::SET_DXNS;
    }
    let b = &mut abc.bodies[body];
    b.max_stack = b.max_stack.max(st.max_stack);
    b.max_scope_depth = b.max_scope_depth.max(st.max_scope);
    b.max_regs = b.max_regs.max(st.max_regs).max(min_regs);
    Ok(())
}
