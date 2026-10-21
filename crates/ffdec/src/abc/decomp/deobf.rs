//! FFDec's automatic AS3 deobfuscation (`AVM2Code.removeTraps`): the GetSet, SimpleOld,
//! RegistersOld, Jumps and ZeroJumpsNullPushes passes over a method's instruction list.
//!
//! The passes run the decompiler's own translator symbolically, as FFDec does, and fold the
//! branches whose condition evaluates statically.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use super::code::{op, Code, Ins, Lit};
use super::ctx::Ctx;
use super::ecma::result_as_bool;
use super::item::*;
use super::ld::Ld;
use super::stack::Stack;
use super::translate::Tr;
use crate::abc::{method_flags, Abc, MethodBody, TraitData};

const EXECUTION_LIMIT: usize = 30000;

/// Why a pass stopped: FFDec throws here, and the caller falls back to the raw body.
struct Abort;

type R<T> = Result<T, Abort>;

type St = Rc<RefCell<Stack>>;

fn new_stack(path: bool) -> St {
    let mut s = Stack::new(path, Rc::new(Cell::new(0)));
    s.no_push_to_output = true;
    Rc::new(RefCell::new(s))
}

fn deob_pop() -> Ins {
    Ins::make(op::POP, Some("ffdec_deobfuscatepop"), Vec::new(), None)
}

fn is_pop(i: &Ins) -> bool {
    i.code == op::POP
}

/// `getNotCoerced`.
fn not_coerced(it: &It) -> It {
    let b = it.borrow();
    match &b.k {
        K::Coerce { .. } | K::Convert { .. } => not_coerced(b.value.as_ref().expect("value")),
        _ => it.clone(),
    }
}

/// Instructions `executeInstructions` may run through.
fn executable(name: &str) -> bool {
    matches!(
        name,
        "pushbyte" | "pushshort" | "pushint" | "pushdouble" | "pushstring" | "pushnull" | "pushundefined" | "pushfalse" | "pushtrue" | "dup" | "swap" | "add" | "add_i" | "subtract" | "subtract_i" | "modulo"
            | "multiply" | "multiply_i" | "divide" | "bitand" | "bitxor" | "bitor" | "lshift" | "rshift" | "urshift" | "equals" | "not" | "negate" | "negate_i" | "increment" | "increment_i" | "decrement"
            | "decrement_i" | "jump" | "iftrue" | "iffalse" | "ifeq" | "ifne" | "iflt" | "ifle" | "ifgt" | "ifge" | "ifstricteq" | "ifstrictne" | "ifnlt" | "ifnle" | "ifngt" | "ifnge" | "lessequals"
            | "greaterequals" | "greaterthan" | "lessthan" | "bitnot" | "strictequals" | "pop" | "ffdec_deobfuscatepop" | "getlocal" | "getlocal0" | "getlocal1" | "getlocal2" | "getlocal3"
            | "newfunction" | "newarray" | "newobject" | "getproperty" | "coerce" | "coerce_a" | "coerce_b" | "coerce_d" | "coerce_i" | "coerce_o" | "coerce_s" | "coerce_u" | "convert_b" | "convert_d"
            | "convert_i" | "convert_o" | "convert_s" | "convert_u" | "convert_f" | "convert_f4" | "convert_m" | "convert_m_p" | "construct" | "call" | "typeof" | "debugline" | "debugfile" | "debug" | "nop"
    )
}

fn simple_push(i: &Ins) -> bool {
    matches!(i.name, "pushbyte" | "pushdouble" | "pushfalse" | "pushint" | "pushnan" | "pushnull" | "pushshort" | "pushstring" | "pushtrue" | "pushuint" | "pushundefined")
}

struct Deob<'c, 'a> {
    tr: Tr<'c, 'a>,
    reserved: usize,
    max_regs: usize,
    /// Constants new pushes add to the pool, as `getIntId(..., true)` would.
    extra_ints: Vec<i32>,
    extra_doubles: Vec<f64>,
    extra_strings: Vec<String>,
}

impl<'c, 'a> Deob<'c, 'a> {
    fn code(&self) -> &Code {
        &self.tr.code
    }

    fn code_mut(&mut self) -> &mut Code {
        &mut self.tr.code
    }

    fn pops(&self, i: &Ins) -> usize {
        self.tr.stack_effect(i).0.max(0) as usize
    }

    fn new_ld(&self) -> Ld {
        let ld = Ld::new(Rc::new(Cell::new(0)));
        ld.local_regs.borrow_mut().clear();
        ld
    }

    /// `initLocalRegs`: parameters are not compile time, other registers start undefined.
    fn init_regs(&self, ld: &Ld) {
        let mut regs = ld.local_regs.borrow_mut();
        regs.clear();
        for i in 0..self.reserved.max(self.max_regs) {
            regs.insert(i as i32, if i < self.reserved { mk(K::Unparsed(String::new()), None, None, prec::PRIMARY, None) } else { undefined(None, None) });
        }
    }

    fn reset(&self, ld: &mut Ld) {
        ld.scope_stack.borrow_mut().clear();
        ld.local_scope_stack.borrow_mut().clear();
        ld.local_reg_assignment_ips.borrow_mut().clear();
        self.init_regs(ld);
    }

    /// Translates `ins` at `ip`, standing in for the instruction there.
    fn translate(&mut self, ld: &mut Ld, out: &List, st: &St, ip: usize, ins: Option<&Ins>) -> R<()> {
        let saved = ins.map(|i| std::mem::replace(&mut self.tr.code.ins[ip], i.clone()));
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.tr.translate_one(ld, out, st, ip)));
        if let Some(s) = saved {
            self.tr.code.ins[ip] = s;
        }
        r.map_err(|e| if super::method::is_timeout(&*e) { std::panic::resume_unwind(e) } else { Abort })
    }

    fn pool_index<T: PartialEq + Clone>(pool: &[T], extra: &mut Vec<T>, v: &T) -> i64 {
        if let Some(p) = pool.iter().skip(1).position(|x| x == v) {
            return p as i64 + 1;
        }
        let base = pool.len().max(1);
        if let Some(p) = extra.iter().position(|x| x == v) {
            return (base + p) as i64;
        }
        extra.push(v.clone());
        (base + extra.len() - 1) as i64
    }

    /// `makePush`.
    fn make_push(&mut self, it: &It) -> Option<Ins> {
        let k = it.borrow().k.clone();
        let c = &self.tr.ctx.abc.constants;
        Some(match k {
            K::IntegerValueA { v, .. } => {
                if (-128..=127).contains(&v) {
                    Ins::make(0x24, None, vec![v as i64], Some(Lit::Int(v)))
                } else if (-32768..=32767).contains(&v) {
                    Ins::make(0x25, None, vec![(v & 0xffff) as i64], Some(Lit::Int(v)))
                } else {
                    let i = Self::pool_index(&c.ints, &mut self.extra_ints, &v);
                    Ins::make(0x2d, None, vec![i], Some(Lit::Int(v)))
                }
            }
            K::DoubleValue(d) => {
                let i = match c.doubles.iter().skip(1).position(|x| x.to_bits() == d.to_bits()) {
                    Some(p) => p as i64 + 1,
                    None => Self::pool_index(&c.doubles, &mut self.extra_doubles, &d),
                };
                Ins::make(0x2f, None, vec![i], Some(Lit::Double(d)))
            }
            K::Str(s) => {
                let i = Self::pool_index(&c.strings, &mut self.extra_strings, &s);
                Ins::make(0x2c, None, vec![i], Some(Lit::Str(s)))
            }
            K::True => Ins::make(0x26, None, Vec::new(), None),
            K::False => Ins::make(0x27, None, Vec::new(), None),
            K::Null => Ins::make(0x20, None, Vec::new(), None),
            K::Undefined => Ins::make(0x21, None, Vec::new(), None),
            _ => return None,
        })
    }

    // ---- ZeroJumpsNullPushes ----

    fn remove_zero_jumps(&mut self) -> i32 {
        let mut min = -1;
        let mut i = 0;
        while i < self.code().len() {
            let ins = &self.code().ins[i];
            if ins.code == op::JUMP && ins.ops[0] == 0 {
                if min == -1 {
                    min = i as i32;
                }
                self.code_mut().remove_instruction(i);
                continue;
            }
            i += 1;
        }
        min
    }

    fn remove_null_pushes(&mut self) {
        let mut offsets = self.code().important_offsets(true);
        let mut i: i64 = 1;
        while (i as usize) < self.code().len() {
            let u = i as usize;
            let (ins1, ins2) = (self.code().ins[u - 1].clone(), self.code().ins[u].clone());
            if is_pop(&ins2) && !offsets.contains(&ins2.addr) && simple_push(&ins1) {
                self.code_mut().remove_instruction(u - 1);
                self.code_mut().remove_instruction(u - 1);
                i -= 2;
                offsets = self.code().important_offsets(true);
            } else if u >= 2 {
                let ins0 = self.code().ins[u - 2].clone();
                if is_pop(&ins2) && ins1.code == 0x96 && !offsets.contains(&ins2.addr) && !offsets.contains(&ins1.addr) && simple_push(&ins0) {
                    for _ in 0..3 {
                        self.code_mut().remove_instruction(u - 2);
                    }
                    i -= 3;
                    offsets = self.code().important_offsets(true);
                }
            }
            i += 1;
        }
    }

    // ---- GetSet ----

    fn get_set(&mut self) -> R<()> {
        self.code_mut().remove_dead_code();
        if self.code().is_empty() {
            return Ok(());
        }
        let mut ld = self.new_ld();
        let mut i = 0;
        while i < self.code().len() {
            self.reset(&mut ld);
            self.get_set_from(&mut ld, i)?;
            i += 1;
        }
        Ok(())
    }

    fn get_set_from(&mut self, ld: &mut Ld, mut idx: usize) -> R<()> {
        let out = empty_list();
        let st = new_stack(false);
        let mut processed = 0;
        loop {
            if idx >= self.code().len() || processed > EXECUTION_LIMIT {
                break;
            }
            let ins = self.code().ins[idx].clone();
            if let Some(reg) = ins.set_local_reg() {
                let top_reg = {
                    let mut s = st.borrow_mut();
                    if s.is_empty() {
                        None
                    } else {
                        match &s.peek().borrow().k {
                            K::LocalReg { reg, .. } => Some(*reg),
                            _ => None,
                        }
                    }
                };
                if top_reg == Some(reg) {
                    st.borrow_mut().pop();
                    self.code_mut().replace_instruction(idx, deob_pop());
                    idx += 1;
                    continue;
                }
            }
            if ins.code == op::NEWFUNCTION {
                if self.code().ins.get(idx + 1).is_some_and(is_pop) {
                    self.code_mut().remove_instruction(idx + 1);
                    self.code_mut().remove_instruction(idx);
                    continue;
                }
            } else {
                if st.borrow().len() < self.pops(&ins) {
                    return Ok(());
                }
                self.translate(ld, &out, &st, idx, None)?;
            }
            if !(ins.set_local_reg().is_some() || ins.code == op::KILL || ins.get_local_reg().is_some()) {
                break;
            }
            idx += 1;
            processed += 1;
        }
        Ok(())
    }

    // ---- SimpleOld ----

    fn flush(&mut self, pending: &mut BTreeMap<usize, Ins>) {
        if pending.is_empty() {
            return;
        }
        let mut addrs = Vec::new();
        let mut deltas = vec![0i64];
        for (&k, v) in pending.iter() {
            let o = &self.code().ins[k];
            addrs.push(o.addr);
            deltas.push(deltas.last().unwrap() + v.len - o.len);
        }
        let delta = |a: i64| deltas[addrs.partition_point(|&x| x < a)];
        self.code_mut().update_offsets(&|a| a + delta(a), &|j, t, o| o + delta(t) - delta(j));
        for (k, mut v) in std::mem::take(pending) {
            v.addr = self.code().ins[k].addr;
            self.code_mut().ins[k] = v;
        }
    }

    fn put(&self, pending: &mut BTreeMap<usize, Ins>, idx: usize, mut ins: Ins) {
        ins.addr = self.code().ins[idx].addr;
        pending.insert(idx, ins);
    }

    /// `removeObfuscationIfs`.
    fn remove_obfuscation_ifs(&mut self, inline: &[u32]) -> R<()> {
        if self.code().is_empty() {
            return Ok(());
        }
        let mut static_regs: HashMap<i32, It> = HashMap::new();
        for id in inline {
            if let Some(reg) = self.code().index_of(*id).and_then(|p| self.code().ins[p].get_local_reg()) {
                static_regs.insert(reg, undefined(None, None));
            }
        }
        let mut jump_targets = Vec::new();
        for ins in &self.code().ins {
            if ins.code == op::JUMP {
                jump_targets.push(self.code().adr2pos(ins.target_address()).map_or(-1, |p| p as i64));
            }
        }
        let mut ld = self.new_ld();
        let mut pending: BTreeMap<usize, Ins> = BTreeMap::new();
        let mut important = Some(self.code().important_offsets(self.tr.is_static));
        let mut i = 0;
        while i < self.code().len() {
            if self.pops(&self.code().ins[i].clone()) > 0 {
                i += 1;
                continue;
            }
            self.reset(&mut ld);
            let mut min = -1i64;
            if self.execute(&mut ld, &mut pending, &mut important, &mut static_regs, i, inline, &jump_targets, &mut min)? && min < i as i64 + 1 {
                i = min.max(0) as usize;
                continue;
            }
            i += 1;
        }
        self.flush(&mut pending);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn execute(&mut self, ld: &mut Ld, pending: &mut BTreeMap<usize, Ins>, important: &mut Option<HashSet<i64>>, static_regs: &mut HashMap<i32, It>, mut idx: usize, inline: &[u32], jump_targets: &[i64], min_ref: &mut i64) -> R<bool> {
        let out = empty_list();
        let st = new_stack(false);
        let mut processed = 0;
        let mut min = i64::MAX;
        loop {
            if idx >= self.code().len() || processed > EXECUTION_LIMIT {
                break;
            }
            let mut ins = pending.get(&idx).cloned().unwrap_or_else(|| self.code().ins[idx].clone());
            if ins.is_if_type() {
                self.flush(pending);
                ins = self.code().ins[idx].clone();
            }
            if st.borrow().len() < self.pops(&ins) {
                return Ok(false);
            }
            if is_pop(&ins) && !st.borrow().is_empty() {
                let top = st.borrow_mut().peek();
                let fsrc = match &top.borrow().k {
                    K::NewFunction { .. } => Some(top.borrow().src),
                    _ => None,
                };
                if let Some(src) = fsrc {
                    self.flush(pending);
                    let next_id = self.code().ins.get(idx + 1).map(|i| i.id);
                    if let Some(f) = src.filter(|&f| self.code().ins.get(f).is_some_and(|i| i.code == op::NEWFUNCTION)) {
                        self.code_mut().remove_instruction(f);
                        min = min.min(f as i64);
                    }
                    let n = self.code().index_of(ins.id).ok_or(Abort)?;
                    self.code_mut().remove_instruction(n);
                    min = min.min(n as i64);
                    idx = match next_id {
                        None => self.code().len(),
                        Some(id) => self.code().index_of(id).unwrap_or(self.code().len()),
                    };
                    *important = None;
                    continue;
                }
            }
            self.translate(ld, &out, &st, idx, pending.get(&idx).cloned().as_ref())?;
            if inline.contains(&ins.id) {
                if let Some(reg) = ins.set_local_reg() {
                    let prev_dup = idx >= 1 && self.code().ins[idx - 1].code == op::DUP;
                    let jt = |p: i64| jump_targets.contains(&p);
                    if (prev_dup && !jt(idx as i64 - 2)) || !jt(idx as i64 - 1) {
                        let v = ld.local_regs.borrow().get(&reg).cloned().ok_or(Abort)?;
                        static_regs.insert(reg, not_coerced(&v));
                        self.put(pending, idx, deob_pop());
                        min = min.min(idx as i64);
                        *important = None;
                    }
                }
            }
            if let Some(reg) = ins.get_local_reg() {
                if let Some(v) = static_regs.get(&reg).cloned() {
                    if st.borrow().is_empty() {
                        return Ok(false);
                    }
                    st.borrow_mut().pop();
                    let Some(push) = self.make_push(&v) else { return Ok(false) };
                    self.put(pending, idx, push.clone());
                    min = min.min(idx as i64);
                    st.borrow_mut().push(v);
                    ins = push;
                    *important = None;
                }
            }
            let mut ok = executable(ins.name);
            if ins.code == 0x66 {
                ok = false;
                let top = st.borrow_mut().peek();
                let empty_array = match &top.borrow().k {
                    K::GetProperty { obj, .. } => matches!(&obj.borrow().k, K::NewArray { values } if values.borrow().is_empty()),
                    _ => return Err(Abort),
                };
                if empty_array {
                    st.borrow_mut().pop();
                    st.borrow_mut().push(undefined(None, None));
                    ok = true;
                }
            }
            if !ok {
                break;
            }
            if let Some(reg) = ins.get_local_reg() {
                if let Some(v) = static_regs.get(&reg).cloned() {
                    if st.borrow().is_empty() {
                        return Ok(false);
                    }
                    st.borrow_mut().pop();
                    st.borrow_mut().push(v);
                } else if reg > 0 {
                    break;
                }
            }
            if is_pop(&ins) {
                self.put(pending, idx, deob_pop());
                min = min.min(idx as i64);
                idx += 1;
            } else if ins.code == op::JUMP {
                idx = self.code().adr2pos(ins.target_address()).ok_or(Abort)?;
            } else if ins.is_if_type() {
                let if_addr = self.code().pos2adr(idx);
                if important.is_none() {
                    *important = Some(self.code().important_offsets(false));
                }
                if important.as_ref().unwrap().contains(&if_addr) {
                    return Ok(false);
                }
                if st.borrow().is_empty() {
                    return Ok(false);
                }
                let top = st.borrow_mut().pop();
                let res = result_as_bool(&top);
                let nidx = self.code().adr2pos(ins.target_address()).ok_or(Abort)?;
                let tar_id = self.code().ins.get(nidx).ok_or(Abort)?.id;
                let count = self.pops(&ins);
                if res {
                    self.code_mut().replace_instruction(idx, Ins::make(op::JUMP, None, vec![0], None));
                    min = min.min(idx as i64);
                    let tp = self.code().index_of(tar_id).ok_or(Abort)?;
                    let ta = self.code().ins[tp].addr;
                    let j = &mut self.code_mut().ins[idx];
                    j.ops[0] = ta - j.addr - j.len;
                    for _ in 0..count {
                        self.code_mut().insert_instruction(idx, deob_pop(), true);
                    }
                } else {
                    min = min.min(idx as i64);
                    self.code_mut().replace_instruction(idx, deob_pop());
                    for _ in 1..count {
                        self.code_mut().insert_instruction(idx, deob_pop(), true);
                    }
                }
                let m2 = self.code_mut().remove_dead_code() as i64;
                if m2 != -1 && m2 < min {
                    min = m2;
                }
                let m3 = self.remove_zero_jumps() as i64;
                if m3 != -1 && m3 < min {
                    min = m3;
                }
                *important = None;
                *min_ref = min;
                return Ok(true);
            } else {
                idx += 1;
            }
            processed += 1;
        }
        Ok(false)
    }

    fn simple(&mut self) -> R<()> {
        self.code_mut().remove_dead_code();
        self.remove_obfuscation_ifs(&[])?;
        self.remove_zero_jumps();
        self.remove_null_pushes();
        Ok(())
    }

    // ---- RegistersOld ----

    fn repeated_assignments(&self) -> HashSet<i32> {
        let mut assigned = HashSet::new();
        let mut repeated = HashSet::new();
        if !self.code().exceptions.is_empty() {
            return repeated;
        }
        for ins in &self.code().ins {
            if ins.is_branch() || ins.code == op::JUMP || ins.code == op::NEWFUNCTION {
                return HashSet::new();
            }
            if let Some(r) = ins.set_local_reg() {
                if !assigned.insert(r) {
                    repeated.insert(r);
                }
            }
        }
        repeated
    }

    fn registers(&mut self) -> R<()> {
        self.code_mut().remove_dead_code();
        let mut ignored: HashSet<i32> = (0..self.reserved as i32).collect();
        ignored.extend(self.repeated_assignments());
        let mut listed_regs: Vec<i32> = Vec::new();
        let mut listed_bodies: Vec<Code> = Vec::new();
        let mut ignored_gets: HashSet<i32> = HashSet::new();
        loop {
            let before = self.code().clone();
            let Some((reg, assign)) = self.first_register_setter(&ignored, &ignored_gets)? else { break };
            if let Some(li) = listed_regs.iter().position(|&r| r == reg) {
                self.tr.code = listed_bodies[li].clone();
                ignored.insert(reg);
                for i in (li..listed_regs.len()).rev() {
                    let r = listed_regs.remove(i);
                    listed_bodies.remove(i);
                    ignored_gets.remove(&r);
                }
                continue;
            }
            let (is_set, is_get) = (assign.set_local_reg().is_some(), assign.get_local_reg().is_some());
            if is_set || is_get {
                self.remove_obfuscation_ifs(&[assign.id])?;
            }
            if is_get {
                ignored_gets.insert(reg);
            }
            listed_regs.push(reg);
            listed_bodies.push(before);
        }
        self.code_mut().remove_dead_code();
        Ok(())
    }

    fn first_register_setter(&mut self, ignored: &HashSet<i32>, ignored_gets: &HashSet<i32>) -> R<Option<(i32, Ins)>> {
        if self.code().is_empty() {
            return Ok(None);
        }
        let n = self.code().len();
        let mut visited = vec![false; n];
        let mut starts: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
        for (e, ex) in self.code().exceptions.iter().enumerate() {
            let s = self.code().adr2pos_nearest(ex.start as i64).ok_or(Abort)?;
            let t = self.code().adr2pos(ex.target as i64).ok_or(Abort)?;
            starts.entry(s).or_default().push((e, t));
        }
        let out = empty_list();
        let mut ld = self.new_ld();
        self.init_regs(&ld);
        ld.local_regs.borrow_mut().insert(0, null(None, None));
        let mut to_visit: Vec<(usize, St)> = vec![(0, new_stack(true))];
        'outer: while !to_visit.is_empty() {
            let (mut idx, st) = to_visit.remove(0);
            loop {
                if idx >= self.code().len() || visited[idx] {
                    break;
                }
                visited[idx] = true;
                if let Some(v) = starts.get(&idx) {
                    for &(e, t) in v {
                        let s2 = Rc::new(RefCell::new(st.borrow().duplicate()));
                        s2.borrow_mut().push(mk(K::ExceptionItem(self.code().exceptions[e].clone()), None, None, prec::PRIMARY, None));
                        to_visit.push((t, s2));
                    }
                }
                let ins = self.code().ins[idx].clone();
                if st.borrow().len() < self.pops(&ins) {
                    continue 'outer;
                }
                if matches!(ins.code, op::RETURNVALUE | op::THROW | op::RETURNVOID) {
                    break;
                }
                self.translate(&mut ld, &out, &st, idx, None)?;
                if let Some(r) = ins.set_local_reg() {
                    if !ignored.contains(&r) {
                        return Ok(Some((r, ins)));
                    }
                } else if let Some(r) = ins.get_local_reg() {
                    if !ignored.contains(&r) && !ignored_gets.contains(&r) {
                        return Ok(Some((r, ins)));
                    }
                } else if ins.code != op::KILL && ins.code != op::DEBUG {
                    for (k, o) in ins.kinds.iter().zip(&ins.ops) {
                        if *k == crate::abc::opcodes::Operand::LocalRegIndex && !ignored.contains(&(*o as i32)) {
                            return Ok(Some((*o as i32, ins)));
                        }
                    }
                }
                idx += 1;
                if ins.code == op::JUMP {
                    idx = self.code().adr2pos(ins.target_address()).ok_or(Abort)?;
                }
                if ins.is_branch() {
                    let b = self.code().branches(self.code().index_of(ins.id).ok_or(Abort)?);
                    idx = *b.first().ok_or(Abort)? as usize;
                    for &x in &b[1..] {
                        let x = x as usize;
                        if visited.get(x).copied().unwrap_or(true) {
                            continue;
                        }
                        to_visit.push((x, Rc::new(RefCell::new(st.borrow().duplicate()))));
                    }
                }
            }
        }
        Ok(None)
    }

    // ---- Jumps ----

    fn jumps(&mut self) {
        let starts: Vec<usize> = self.code().exceptions.iter().filter_map(|e| self.code().adr2pos_nearest(e.start as i64)).collect();
        loop {
            let mut found = false;
            let refs = self.code().visit_refs();
            for i in 0..self.code().len() {
                let ins = self.code().ins[i].clone();
                if ins.code != op::JUMP {
                    continue;
                }
                let target = ins.target_address();
                let mismatch = |src: i64, code: &Code| code.exceptions.iter().any(|e| (src >= e.start as i64 && src < e.end as i64) != (target >= e.start as i64 && target < e.end as i64));
                if mismatch(ins.addr, self.code()) || starts.contains(&i) {
                    continue;
                }
                for &r in refs.get(i).map(Vec::as_slice).unwrap_or(&[]) {
                    if r < 0 {
                        continue;
                    }
                    let r = r as usize;
                    let src = self.code().ins[r].clone();
                    if src.code == op::JUMP || (src.is_if_type() && r + 1 != i) {
                        if mismatch(src.addr, self.code()) {
                            continue;
                        }
                        let new = target - (src.addr + src.len);
                        if self.code().ins[r].ops[0] != new {
                            self.code_mut().ins[r].ops[0] = new;
                            found = true;
                        }
                    }
                }
            }
            self.code_mut().remove_dead_code();
            if !found {
                break;
            }
        }
    }

    fn remove_traps(&mut self) -> R<()> {
        self.get_set()?;
        self.simple()?;
        self.registers()?;
        self.jumps();
        self.remove_zero_jumps();
        self.remove_null_pushes();
        Ok(())
    }
}

/// `MethodBody.getLocalReservedCount`.
fn reserved_count(ctx: &Ctx, method: usize) -> usize {
    let mi = &ctx.abc.methods[method];
    let mut n = mi.param_types.len() + 1;
    if mi.flags & method_flags::NEED_ARGUMENTS != 0 {
        n += 1;
    }
    if mi.flags & method_flags::NEED_REST != 0 {
        n += 1;
    }
    n
}

/// Constants new pushes need appended to the pool, in index order.
#[derive(Default)]
struct Extras {
    ints: Vec<i32>,
    doubles: Vec<f64>,
    strings: Vec<String>,
}

fn run(ctx: &Ctx, body: &MethodBody, code: Code, method: usize, is_static: bool, script_index: i32, class_index: i32) -> Option<(Code, Extras)> {
    let tr = Tr {
        ctx,
        body,
        code,
        method,
        is_static,
        script_index,
        class_index,
        local_reg_names: HashMap::new(),
        fqn: Vec::new(),
        swf_version: ctx.swf_version,
        call_stack: vec![method],
        path: String::new(),
    };
    let mut d = Deob { tr, reserved: reserved_count(ctx, method), max_regs: body.max_regs, extra_ints: Vec::new(), extra_doubles: Vec::new(), extra_strings: Vec::new() };
    let r = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| d.remove_traps())) {
        Ok(r) => r,
        Err(e) if super::method::is_timeout(&*e) => std::panic::resume_unwind(e),
        Err(_) => Err(Abort),
    };
    r.ok()?;
    let extras = Extras { ints: d.extra_ints, doubles: d.extra_doubles, strings: d.extra_strings };
    Some((d.tr.code, extras))
}

/// `convertMethodBody(true, ...)`: the fixed code with FFDec's traps removed, or None when a
/// pass fails, which makes FFDec fall back to the plain body.
pub fn deobfuscate(ctx: &Ctx, body: &MethodBody, code: Code, method: usize, is_static: bool, script_index: i32, class_index: i32) -> Option<Code> {
    let (mut code, _) = run(ctx, body, code, method, is_static, script_index, class_index)?;
    code.remove_labels_and_debug_line();
    Some(code)
}

/// Stores `code` as the method's body, adding the constants its new pushes use.
fn store(abc: &mut Abc, bi: usize, code: &Code, extras: Extras) {
    let c = &mut abc.constants;
    if c.ints.is_empty() {
        c.ints.push(0);
    }
    if c.doubles.is_empty() {
        c.doubles.push(0.0);
    }
    if c.strings.is_empty() {
        c.strings.push(String::new());
    }
    c.ints.extend(extras.ints);
    c.doubles.extend(extras.doubles);
    c.strings.extend(extras.strings);
    let mut bytes = Vec::new();
    for x in &code.ins {
        let (c, name) = if x.name == "ffdec_deobfuscatepop" { (op::POP, "pop") } else { (x.code, x.name) };
        bytes.extend(crate::abc::asm::encode(&crate::abc::opcodes::Instruction { offset: x.addr as usize, len: x.len as usize, code: c, name, kinds: x.kinds, operands: x.ops.clone() }));
    }
    let body = &mut abc.bodies[bi];
    body.code = bytes;
    body.exceptions = code.exceptions.clone();
}

/// `MethodBody.removeTraps`, the permanent form: rewrites the body in place. False when a pass
/// fails; the body is then left as it was.
pub fn remove_traps(abc: &mut Abc, method: usize, is_static: bool, script_index: i32, class_index: i32) -> bool {
    let Some(bi) = abc.body_of.get(method).copied().flatten() else { return false };
    let r = {
        let ctx = Ctx::new(abc, super::ctx::Config::default());
        let body = &abc.bodies[bi];
        run(&ctx, body, Code::new(abc, body), method, is_static, script_index, class_index)
    };
    let Some((code, extras)) = r else { return false };
    store(abc, bi, &code, extras);
    true
}

/// `MethodBody.removeDeadCode`.
pub fn remove_dead_code(abc: &mut Abc, method: usize) {
    let Some(bi) = abc.body_of.get(method).copied().flatten() else { return };
    let mut code = Code::new(abc, &abc.bodies[bi]);
    code.remove_dead_code();
    store(abc, bi, &code, Extras::default());
}

/// `removeLabelsAndDebugLine` on a stored body.
pub fn remove_labels_and_debug_line(abc: &mut Abc, method: usize) {
    let Some(bi) = abc.body_of.get(method).copied().flatten() else { return };
    let mut code = Code::new(abc, &abc.bodies[bi]);
    code.remove_labels_and_debug_line();
    store(abc, bi, &code, Extras::default());
}

fn traits_remove_traps(abc: &mut Abc, traits: &[crate::abc::Trait], script: i32, class: i32, is_static: bool) -> usize {
    let mut n = 0;
    for t in traits {
        match t.data {
            TraitData::Method { method, .. } | TraitData::Getter { method, .. } | TraitData::Setter { method, .. } | TraitData::Function { method, .. } => n += remove_traps(abc, method, is_static, script, class) as usize,
            TraitData::Class { class: c, .. } => {
                let (iinit, cinit) = (abc.instances[c].iinit, abc.classes[c].cinit);
                n += remove_traps(abc, iinit, false, script, c as i32) as usize;
                n += remove_traps(abc, cinit, true, script, c as i32) as usize;
                let it = abc.instances[c].traits.clone();
                n += traits_remove_traps(abc, &it, script, c as i32, false);
                let st = abc.classes[c].traits.clone();
                n += traits_remove_traps(abc, &st, script, c as i32, true);
            }
            _ => {}
        }
    }
    n
}

/// `ABC.removeTraps`: every trait method of every script, as FFDec's "Remove traps" does.
pub fn remove_traps_all(abc: &mut Abc) -> usize {
    let mut n = 0;
    for s in 0..abc.scripts.len() {
        let traits = abc.scripts[s].traits.clone();
        n += traits_remove_traps(abc, &traits, s as i32, -1, true);
    }
    n
}

/// `ABC.removeDeadCode`.
pub fn remove_dead_code_all(abc: &mut Abc) {
    for bi in 0..abc.bodies.len() {
        let m = abc.bodies[bi].method;
        remove_dead_code(abc, m);
    }
}
