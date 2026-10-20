//! AS1/2 actions to items: `Action.actionsPartToTree` and each action's `translate`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::abc::decomp::code::{op, Code, Ins};
use crate::abc::decomp::graph::St;
use crate::abc::decomp::item::*;
use crate::abc::decomp::ld::Ld;
use crate::abc::decomp::props::{has_side_effect, invert, not_coerced_no_dup, through_duplicate};

use super::super::{Action, Op, Value};
use super::items::*;

/// FFDec's `variables`/`functions` maps (`LinkedHashMap<String, GraphTargetItem>`).
#[derive(Debug, Clone, Default)]
pub struct JMap(pub Vec<(String, Option<It>)>);

impl JMap {
    pub fn get(&self, k: &str) -> Option<It> {
        self.0.iter().find(|(x, _)| x == k).and_then(|(_, v)| v.clone())
    }

    pub fn contains(&self, k: &str) -> bool {
        self.0.iter().any(|(x, _)| x == k)
    }

    pub fn put(&mut self, k: &str, v: Option<It>) {
        match self.0.iter_mut().find(|(x, _)| x == k) {
            Some(e) => e.1 = v,
            None => self.0.push((k.to_string(), v)),
        }
    }

    pub fn remove(&mut self, k: &str) {
        self.0.retain(|(x, _)| x != k);
    }
}

pub type Vars = Rc<RefCell<JMap>>;

/// `ActionSecondPassData`: switch chains found by the first pass, as part start/end pairs.
#[derive(Debug, Clone, Default)]
pub struct ASecondPass {
    pub switch_cases: Vec<Vec<usize>>,
    pub switch_breaks: Vec<Option<usize>>,
    pub switch_parts: Vec<Vec<(i32, i32)>>,
    pub switch_on_false_parts: Vec<Vec<(i32, i32)>>,
    pub switch_case_expressions: Vec<Vec<It>>,
}

/// Everything the action translator needs for one graph (`ActionGraphSource` and `ActionLocalData`).
pub struct ActTr {
    pub actions: Rc<Vec<Action>>,
    pub len: usize,
    pub version: u8,
    pub path: String,
    pub inside_do_init: bool,
    pub inside_function: bool,
    pub reg_names: Rc<RefCell<HashMap<i32, String>>>,
    pub vars: Vars,
    pub funcs: Vars,
    pub second_pass: Option<Rc<ASecondPass>>,
    /// `labelsOfRemovedGotos`.
    pub removed_goto_labels: RefCell<std::collections::HashSet<String>>,
    /// `localData.loops.size()` while final processing.
    pub loop_count: std::cell::Cell<i64>,
    /// Fields to add to detected classes (`uninitializedClassTraits`).
    pub uninit: Rc<super::uninit::Uninitialized>,
}

impl ActTr {
    pub fn new(actions: Rc<Vec<Action>>, len: usize, version: u8, path: String, inside_do_init: bool, inside_function: bool, reg_names: HashMap<i32, String>, vars: Vars, funcs: Vars) -> ActTr {
        ActTr {
            actions,
            len,
            version,
            path,
            inside_do_init,
            inside_function,
            reg_names: Rc::new(RefCell::new(reg_names)),
            vars,
            funcs,
            second_pass: None,
            removed_goto_labels: Default::default(),
            loop_count: Default::default(),
            uninit: Default::default(),
        }
    }

    pub fn loop_counter(&self) -> i64 {
        self.loop_count.get()
    }
}

/// Where a part translation stopped past its end (`GraphPartChangeException`).
pub type PartChange = Option<i32>;

impl ActTr {
    pub fn action(&self, ip: usize) -> &Action {
        &self.actions[ip]
    }

    /// `Action.adr2ip` within the graph's list.
    pub fn adr2ip(&self, addr: u64) -> i32 {
        let list = &self.actions[..self.len];
        if let Some(i) = list.iter().position(|a| a.address == addr) {
            return i as i32;
        }
        if let Some(l) = list.last() {
            if addr == l.address + l.total_len() {
                return list.len() as i32;
            }
        }
        -1
    }

    fn ip_after_jumps(&self, code: &Code, mut nip: usize) -> usize {
        while nip < self.len && matches!(self.actions[nip].op, Op::Jump { .. }) {
            let t = self.actions[nip].target().unwrap();
            let nip2 = code.adr2pos(t as i64).unwrap_or(usize::MAX);
            if nip2 == nip || nip2 == usize::MAX {
                break;
            }
            nip = nip2;
        }
        nip
    }

    /// `ActionGraph.checkIp`: skips `Push null; Equals; Not; If` that jumps back to itself.
    pub fn check_ip(&self, code: &Code, ip: i32) -> i32 {
        if ip < 0 || ip as usize >= self.len {
            return ip;
        }
        let old = ip;
        let mut ip = ip as usize;
        let a = &self.actions[ip];
        let single_null = matches!(&a.op, Op::Push { values, .. } if values.len() == 1 && values[0] == Value::Null);
        if single_null && ip + 1 < self.len {
            let mut nip = self.ip_after_jumps(code, ip + 1);
            if nip < self.len && matches!(self.actions[nip].code, 0x0e | 0x49) && matches!(self.actions[nip].op, Op::Simple) {
                nip = self.ip_after_jumps(code, nip + 1);
                if nip < self.len && self.actions[nip].code == 0x12 {
                    nip = self.ip_after_jumps(code, nip + 1);
                    if nip < self.len && matches!(self.actions[nip].op, Op::If { .. }) {
                        let t = self.actions[nip].target().unwrap();
                        let jip = code.adr2pos(t as i64).map(|j| self.ip_after_jumps(code, j));
                        if jip == Some(ip) {
                            ip = nip + 1;
                        }
                    }
                }
            }
        }
        if old as usize != ip {
            if ip == self.len {
                return ip as i32;
            }
            return self.check_ip(code, ip as i32);
        }
        ip as i32
    }

    /// The action list as graph instructions.
    pub fn code(&self, start_ip: usize) -> Code {
        let cs = self.actions.first().map_or(super::super::Charset::Utf8, |_| super::super::Charset::for_version(self.version));
        let list = &self.actions[..self.len];
        let end_addr = list.last().map_or(0, |l| (l.address + l.total_len()) as i64);
        // `ActionGraphSource.adr2pos`: unknown addresses past the last action are the end.
        let adr2pos = |adr: u64| -> i64 {
            if adr == 0 {
                return 0;
            }
            match list.iter().position(|a| a.address == adr) {
                Some(p) => p as i64,
                None if list.last().is_some_and(|l| adr > l.address) => list.len() as i64,
                None => -1,
            }
        };
        let pos_addr = |p: i64| -> i64 { if p as usize >= list.len() { end_addr } else { list[p as usize].address as i64 } };
        let ins = list
            .iter()
            .map(|a| {
                let mut i = Ins { addr: a.address as i64, len: a.total_len() as i64, code: 0, name: "", kinds: &[], ops: vec![], ignored: a.ignored, line: 0, file: None, bodies: vec![], container_end: None, define_function: false };
                match &a.op {
                    Op::Jump { .. } | Op::If { .. } => {
                        i.code = if matches!(a.op, Op::Jump { .. }) { op::JUMP } else { op::IFTRUE };
                        let mut t = adr2pos(a.target().unwrap());
                        if t == -1 {
                            t = adr2pos(a.address + a.total_len());
                        }
                        let target = if t < 0 { a.address as i64 + a.total_len() as i64 } else { pos_addr(t) };
                        i.ops = vec![target - a.address as i64 - 4];
                    }
                    _ if a.is_exit() => i.code = op::RETURNVOID,
                    _ => {}
                }
                if let Some(sizes) = a.container_sizes() {
                    let mut addr = (a.address + a.bytes_len(cs)) as i64;
                    for s in sizes {
                        i.bodies.push((addr, addr + s as i64));
                        addr += s as i64;
                    }
                    i.container_end = Some(addr);
                    i.define_function = matches!(a.op, Op::DefineFunction { .. } | Op::DefineFunction2(_));
                }
                i
            })
            .collect();
        Code { ins, exceptions: vec![], start_ip: start_ip as i32 }
    }

    /// `Action.actionsPartToTree` over `start..=end`.
    pub fn translate_range(&self, g: &crate::abc::decomp::graph::G, output: &List, ld: &mut Ld, stack: &St, start: usize, end: usize) -> PartChange {
        let cs = super::super::Charset::for_version(self.version);
        let mut ip = start;
        while ip <= end {
            if ip >= self.len {
                output.borrow_mut().push(script_end());
                break;
            }
            let a = &self.actions[ip];
            if a.ignored {
                ip += 1;
                continue;
            }
            let stack_empty = stack.borrow().items.iter().all(|i| matches!(i.borrow().k, K::AFunction(_)));
            if stack_empty {
                ld.line_start = Some(ip);
            }
            if let Some(sizes) = a.container_sizes() {
                let mut end_addr = a.address + a.bytes_len(cs);
                let name = a.container_name();
                let mut outs: Vec<List> = Vec::new();
                let vars2 = Rc::new(RefCell::new(prepare_variables(a, &self.vars.borrow())));
                for size in sizes {
                    if size == 0 {
                        outs.push(empty_list());
                        continue;
                    }
                    let mut reg_names = container_reg_names(a);
                    if matches!(a.op, Op::With { .. } | Op::Try(_)) {
                        for (k, v) in self.reg_names.borrow().iter() {
                            reg_names.entry(*k).or_insert_with(|| v.clone());
                        }
                    }
                    let start_ip = self.adr2ip(end_addr);
                    let sub_len = self.adr2ip(end_addr + size);
                    let path = match name {
                        Some(n) => format!("{}/{n}", self.path),
                        None => self.path.clone(),
                    };
                    let sub = ActTr::new(self.actions.clone(), sub_len.max(0) as usize, self.version, path, self.inside_do_init, true, reg_names, vars2.clone(), self.funcs.clone());
                    let out = super::translate_via_graph(sub, start_ip.max(0) as usize).unwrap_or_else(|e| new_list(vec![decompilation_error(&e)]));
                    outs.push(out);
                    end_addr += size;
                }
                self.translate_container(a, ip, outs, ld, output, stack);
                ip = self.adr2ip(end_addr).max(0) as usize;
                continue;
            }
            let nip = g.check_ip(ip as i32) as usize;
            let mut ip_t = ip;
            if nip != ip {
                ip = nip;
                if ip > end {
                    break;
                }
                ip_t = ip;
            }
            if let Some(n) = self.actions[ip_t].store_size() {
                let skipped: Vec<Action> = self.actions[ip_t + 1..(ip_t + 1 + n).min(self.len)].to_vec();
                self.translate_store(ip_t, skipped, ld, output, stack);
                ip = ip_t + n;
            } else {
                let mut x = X { tr: self, ld: &mut *ld, out: output, st: stack, ip: ip_t };
                x.translate();
            }
            let code = self.actions[ip_t].code;
            if matches!(code, 0x8b | 0x20) && !stack.borrow().is_empty() {
                stack.borrow_mut().finish_block(output);
                let last = output.borrow_mut().pop();
                g.make_all_commands(output, stack);
                if let Some(l) = last {
                    output.borrow_mut().push(l);
                }
            }
            ip += 1;
        }
        if ip > end + 1 {
            return Some(ip as i32);
        }
        None
    }

    fn translate_store(&self, ip: usize, skipped: Vec<Action>, ld: &mut Ld, _output: &List, stack: &St) {
        let a = &self.actions[ip];
        let src = Some(ip);
        let ls = ld.line_start;
        let frame = match &a.op {
            Op::WaitForFrame { frame, .. } => direct(AVal::Int(*frame as i64)),
            _ => stack.borrow_mut().pop(),
        };
        let n = skipped.len();
        let mut sub = ActTr::new(Rc::new(skipped), n, self.version, self.path.clone(), self.inside_do_init, true, HashMap::new(), self.vars.clone(), self.funcs.clone());
        sub.reg_names = self.reg_names.clone();
        let body = super::translate_via_graph(sub, 0).unwrap_or_else(|e| new_list(vec![decompilation_error(&e)]));
        let item = amk(K::AIfFrameLoaded { frame, actions: body }, src, ls, prec::NOPRECEDENCE, None);
        stack.borrow_mut().add_to_output(item);
    }

    fn translate_container(&self, a: &Action, ip: usize, contents: Vec<List>, ld: &mut Ld, output: &List, stack: &St) {
        let src = Some(ip);
        match &a.op {
            Op::DefineFunction { name, params, .. } => {
                let f = AFunction {
                    name: name.clone(),
                    params: params.clone(),
                    reg_names: vec![],
                    actions: contents[0].clone(),
                    calculated_name: None,
                    getter: false,
                    setter: false,
                    reg_start: 1,
                    param_registers: None,
                };
                let fi = amk(K::AFunction(Box::new(f)), src, src, prec::PRIMARY, None);
                stack.borrow_mut().push(fi.clone());
                self.funcs.borrow_mut().put(name, Some(fi));
            }
            Op::DefineFunction2(d) => {
                let f = AFunction {
                    name: d.name.clone(),
                    params: d.params.iter().map(|(_, n)| n.clone()).collect(),
                    reg_names: df2_reg_names(d),
                    actions: contents[0].clone(),
                    calculated_name: None,
                    getter: false,
                    setter: false,
                    reg_start: df2_first_register(d),
                    param_registers: Some(d.params.iter().map(|(r, _)| *r as i32).collect()),
                };
                let fi = amk(K::AFunction(Box::new(f)), src, src, prec::PRIMARY, None);
                self.funcs.borrow_mut().put(&d.name, Some(fi.clone()));
                stack.borrow_mut().push(fi);
            }
            Op::With { .. } => {
                let scope = stack.borrow_mut().pop();
                let w = amk(K::AWith { scope, items: contents[0].clone() }, src, src, prec::NOPRECEDENCE, None);
                stack.borrow_mut().add_to_output(w);
            }
            Op::Try(t) => {
                let item = try_item(t, src, &contents);
                stack.borrow_mut().add_to_output(item);
            }
            _ => {}
        }
        let _ = (ld, output);
    }
}

pub fn decompilation_error(kind: &str) -> It {
    mk(
        K::Comment(vec![
            Some(String::new()),
            Some(" * Decompilation error".into()),
            Some(" * Code may be obfuscated".into()),
            Some(format!(" * Error type: {kind}")),
            Some(String::new()),
        ]),
        None,
        None,
        prec::NOPRECEDENCE,
        None,
    )
}

fn prepare_variables(a: &Action, vars: &JMap) -> JMap {
    let mut v = vars.clone();
    if matches!(a.op, Op::DefineFunction { .. } | Op::DefineFunction2(_)) {
        v.0.retain(|(k, _)| !(k.starts_with("__register") && k[10..].parse::<u32>().is_ok_and(|r| r < 256)));
    }
    v
}

fn df2_first_register(d: &super::super::DefineFunction2) -> i32 {
    1 + [d.preload_this, d.preload_arguments, d.preload_super, d.preload_root, d.preload_parent, d.preload_global].iter().filter(|&&b| b).count() as i32
}

/// `ActionDefineFunction2.getRegNames`.
pub fn df2_reg_names(d: &super::super::DefineFunction2) -> Vec<(i32, String)> {
    let mut m: Vec<(i32, String)> = Vec::new();
    let mut put = |r: i32, n: &str| match m.iter_mut().find(|(k, _)| *k == r) {
        Some(e) => e.1 = n.to_string(),
        None => m.push((r, n.to_string())),
    };
    for (reg, name) in &d.params {
        if *reg != 0 {
            put(*reg as i32, name);
        }
    }
    let mut pos = 1;
    for (flag, name) in [(d.preload_this, "this"), (d.preload_arguments, "arguments"), (d.preload_super, "super"), (d.preload_root, "_root"), (d.preload_parent, "_parent"), (d.preload_global, "_global")] {
        if flag {
            put(pos, name);
            pos += 1;
        }
    }
    m
}

fn container_reg_names(a: &Action) -> HashMap<i32, String> {
    match &a.op {
        Op::DefineFunction2(d) => df2_reg_names(d).into_iter().collect(),
        _ => HashMap::new(),
    }
}

/// `ActionTry.translateContainer`: catch clauses with typed exceptions.
fn try_item(t: &super::super::Try, src: Option<usize>, contents: &[List]) -> It {
    let try_cmds = contents[0].clone();
    let names = empty_list();
    let mut types: Vec<Option<It>> = Vec::new();
    let mut catch_cmds: Vec<List> = Vec::new();
    let mut p = 0;
    if t.catch_flag {
        p += 1;
        let mut body = contents[p].clone();
        if t.catch_in_register {
            if body.borrow().len() >= 2 {
                let mut pos = 0;
                loop {
                    let Some(sti) = body.borrow().get(pos).cloned() else { break };
                    let K::SetTemporary { temp: sti_temp, .. } = sti.borrow().k.clone() else { break };
                    if pos + 1 >= body.borrow().len() {
                        break;
                    }
                    let pi = body.borrow()[pos + 1].clone();
                    if !is::push(&pi) {
                        break;
                    }
                    let piv = pi.borrow().val();
                    let K::DuplicateSource { temp: ds_temp, .. } = piv.borrow().k.clone() else { break };
                    if ds_temp != sti_temp {
                        break;
                    }
                    let stv = sti.borrow().val();
                    let K::ACastOp { ctor, obj } = stv.borrow().k.clone() else { break };
                    let reg = match &obj.borrow().k {
                        K::ADirect { v: AVal::Register { num, .. }, .. } => *num,
                        K::ATempReg { reg, .. } => *reg,
                        _ => break,
                    };
                    if reg != t.catch_register as i32 {
                        break;
                    }
                    types.push(Some(ctor));
                    if pos + 2 >= body.borrow().len() {
                        break;
                    }
                    let ifi = body.borrow()[pos + 2].clone();
                    let K::If { on_true, on_false, .. } = ifi.borrow().k.clone() else { break };
                    let mut on_false_l = on_false.clone();
                    let mut on_false_pos = 0;
                    let last_exit = on_true.borrow().last().is_some_and(|l| is::exit(l) || is::break_(l) || is::continue_(l));
                    if on_false.borrow().is_empty() && !on_true.borrow().is_empty() && last_exit {
                        on_false_l = body.clone();
                        on_false_pos = pos + 3;
                    }
                    let first_true = on_true.borrow().first().cloned();
                    if let Some(dl) = first_true.filter(|f| matches!(f.borrow().k, K::ADefineLocal { .. })) {
                        let K::ADefineLocal { name } = dl.borrow().k.clone() else { unreachable!() };
                        names.borrow_mut().push(name);
                        let cb = copy_list(&on_true);
                        cb.borrow_mut().remove(0);
                        catch_cmds.push(cb);
                        if on_false_l.borrow().len() <= on_false_pos {
                            break;
                        }
                        if !is::pop(&on_false_l.borrow()[on_false_pos]) {
                            break;
                        }
                        pos = on_false_pos + 1;
                        body = on_false_l;
                    } else if on_false_l.borrow().len() > on_false_pos && matches!(on_false_l.borrow()[on_false_pos].borrow().k, K::ADefineLocal { .. }) {
                        let dl = on_false_l.borrow()[on_false_pos].clone();
                        let K::ADefineLocal { name } = dl.borrow().k.clone() else { unreachable!() };
                        names.borrow_mut().push(name);
                        let cb: Vec<It> = on_false_l.borrow()[on_false_pos + 1..].to_vec();
                        catch_cmds.push(new_list(cb));
                        if on_true.borrow().first().is_some_and(is::pop) {
                            pos = 1;
                            body = on_true.clone();
                        }
                    } else {
                        break;
                    }
                }
                let at = body.borrow().get(pos).cloned();
                if let Some(dl) = at.filter(|d| matches!(d.borrow().k, K::ADefineLocal { .. })) {
                    let K::ADefineLocal { name } = dl.borrow().k.clone() else { unreachable!() };
                    names.borrow_mut().push(name);
                    types.push(None);
                    let cb = copy_list(&body);
                    {
                        let mut b = cb.borrow_mut();
                        if !b.is_empty() {
                            b.remove(0);
                        }
                        if !b.is_empty() {
                            b.remove(0);
                        }
                    }
                    catch_cmds.push(cb);
                }
            }
        } else {
            names.borrow_mut().push(direct_at(src, src, -1, AVal::Str(t.catch_name.clone().unwrap_or_default())));
            types.push(None);
            catch_cmds.push(body);
        }
    }
    let finally = if t.finally_flag {
        p += 1;
        contents[p].clone()
    } else {
        empty_list()
    };
    amk(K::ATry { try_cmds, names, types, catch_cmds, finally }, None, None, prec::NOPRECEDENCE, None)
}

struct X<'x> {
    tr: &'x ActTr,
    ld: &'x mut Ld,
    out: &'x List,
    st: &'x St,
    ip: usize,
}

impl<'x> X<'x> {
    fn a(&self) -> &'x Action {
        &self.tr.actions[self.ip]
    }

    fn src(&self) -> Option<usize> {
        Some(self.ip)
    }

    fn ls(&self) -> Option<usize> {
        self.ld.line_start
    }

    fn mk(&self, k: K, p: u8, v: Option<It>) -> It {
        amk(k, self.src(), self.ls(), p, v)
    }

    fn pop(&self) -> It {
        self.st.borrow_mut().pop()
    }

    fn push(&self, it: It) {
        self.st.borrow_mut().push(it);
    }

    fn peek(&self) -> Option<It> {
        let s = self.st.borrow();
        s.items.last().cloned()
    }

    fn add_out(&self, it: It) {
        self.st.borrow_mut().add_to_output(it);
    }

    fn finish(&self) {
        self.st.borrow_mut().finish_block(self.out);
    }

    fn move_to(&self) {
        self.st.borrow_mut().move_to_stack(self.out);
    }

    fn allow_swap(&self) {
        self.st.borrow_mut().allow_swap(self.out);
    }

    /// `Action.popLong`.
    fn pop_long(&self) -> i64 {
        let it = self.pop();
        if is_direct(&it) {
            return as_long(&it);
        }
        0
    }

    fn bin(&self, op: Bin) {
        let a = self.pop();
        let b = self.pop();
        self.push(abin(op, self.src(), self.ls(), b, a));
    }

    fn fn1(&self, name: &'static str) {
        let a = self.pop();
        self.push(afn_pure(name, self.src(), self.ls(), vec![a]));
    }

    fn stmt(&self, name: &'static str) {
        self.add_out(afn(name, self.src(), self.ls(), vec![], false));
    }

    fn name_str(&self, name: &It) -> String {
        if is_direct(name) {
            return direct_string(name).unwrap_or_else(|| crate::action::decomp::print::number(result_as_number(name).unwrap_or(0.0)));
        }
        compile_time_string(name).unwrap_or_else(|| "null".into())
    }

    fn translate(&mut self) {
        let a = self.a();
        match &a.op {
            Op::Simple => self.simple(a.code),
            Op::End | Op::ConstantPool(_) | Op::Unknown(_) => {}
            Op::GotoFrame(f) => self.add_out(self.mk(K::AGotoFrame(*f as i32), prec::PRIMARY, None)),
            Op::GetUrl { url, target } => self.get_url(url, target),
            Op::SetTarget(t) => self.add_out(self.mk(K::ASetTarget(t.clone()), prec::PRIMARY, None)),
            Op::GoToLabel(l) => self.add_out(afn_with("gotoAndStop", self.src(), self.ls(), vec![dstr(l)], ",", String::new(), false)),
            Op::Push { values, pool } => self.push_values(values, pool.as_deref().map(|p| p.as_slice())),
            Op::Jump { .. } | Op::If { .. } => {}
            Op::GetUrl2 { load_variables, load_target, method } => self.get_url2(*load_variables, *load_target, *method as i32),
            Op::GotoFrame2 { scene_bias_flag, play, scene_bias } => {
                let frame = self.pop();
                let mut args = Vec::new();
                if *scene_bias_flag {
                    args.push(direct(AVal::Int(*scene_bias as i64)));
                }
                args.push(frame);
                let name = if *play { "gotoAndPlay" } else { "gotoAndStop" };
                self.add_out(afn_with(name, self.src(), self.ls(), args, ", ", String::new(), false));
            }
            Op::StoreRegister(r) => self.store_register(*r as i32),
            Op::StrictMode(m) => {
                if *m == 1 {
                    self.out.borrow_mut().push(self.mk(K::AStrict(*m as i32), prec::PRIMARY, None));
                }
            }
            _ => {}
        }
    }

    fn simple(&mut self, code: u8) {
        let (src, ls) = (self.src(), self.ls());
        match code {
            0x04 => self.stmt("nextFrame"),
            0x05 => self.stmt("prevFrame"),
            0x06 => {
                self.finish();
                let last = self.out.borrow().last().cloned();
                if let Some(l) = last.filter(|l| matches!(l.borrow().k, K::AGotoFrame(_))) {
                    self.out.borrow_mut().pop();
                    let K::AGotoFrame(f) = l.borrow().k.clone() else { unreachable!() };
                    self.move_to();
                    self.add_out(afn_with("gotoAndPlay", src, ls, vec![direct(AVal::Int(f as i64 + 1))], ", ", String::new(), false));
                } else {
                    self.move_to();
                    self.stmt("play");
                }
            }
            0x07 => self.stmt("stop"),
            0x08 => self.stmt("toggleHighQuality"),
            0x09 => self.stmt("stopAllSounds"),
            0x0a | 0x47 => self.bin(Bin::Add),
            0x0b => self.bin(Bin::Sub),
            0x0c => self.bin(Bin::Mul),
            0x0d => self.bin(Bin::Div),
            0x0e | 0x49 => self.bin(Bin::Eq),
            0x0f | 0x48 => self.bin(Bin::Lt),
            0x10 => self.bin(Bin::AAnd),
            0x11 => self.bin(Bin::AOr),
            0x12 => {
                let a = self.pop();
                self.push(invert(&a, src));
            }
            0x13 => self.bin(Bin::StrEq),
            0x14 => self.fn1("length"),
            0x15 | 0x35 => {
                let count = self.pop();
                let index = self.pop();
                let value = self.pop();
                self.push(afn_pure(if code == 0x15 { "substring" } else { "mbsubstring" }, src, ls, vec![value, index, count]));
            }
            0x17 => {
                let v = self.pop();
                if matches!(v.borrow().k, K::Any) {
                    return;
                }
                self.add_out(v);
            }
            0x18 => self.fn1("int"),
            0x1c => {
                let name = self.pop();
                let ns = self.name_str(&name);
                let computed = self.tr.vars.borrow().get(&ns);
                if direct_string(&name).as_deref() == Some("/:$version") {
                    self.push(afn_pure("getVersion", src, ls, vec![]));
                } else if !is_direct(&name) && !matches!(name.borrow().k, K::AGetVar { .. }) {
                    self.push(self.mk(K::AEval, prec::PRIMARY, Some(name)));
                } else {
                    self.push(self.mk(K::AGetVar { name, print_obf: false, computed }, prec::PRIMARY, None));
                }
            }
            0x1d => self.set_variable(),
            0x20 => {
                let t = self.pop();
                self.add_out(self.mk(K::ASetTarget2 { target: t }, prec::PRIMARY, None));
            }
            0x21 => self.bin(Bin::StrAdd),
            0x22 => {
                let index = self.pop();
                let target = self.pop();
                let i = property_index(&index);
                self.push(self.mk(K::AGetProp { target, index: i, use_fn: true }, prec::PRIMARY, None));
            }
            0x23 => self.set_property(),
            0x24 => {
                let depth = self.pop();
                let source = self.pop();
                let target = self.pop();
                self.add_out(afn("duplicateMovieClip", src, ls, vec![target, source, depth], false));
            }
            0x25 => {
                let t = self.pop();
                self.add_out(afn("removeMovieClip", src, ls, vec![t], false));
            }
            0x26 => {
                let v = self.pop();
                self.add_out(afn("trace", src, ls, vec![v], false));
            }
            0x27 => self.start_drag(),
            0x28 => self.stmt("stopDrag"),
            0x29 => self.bin(Bin::StrLt),
            0x2a => {
                let v = self.pop();
                self.add_out(self.mk(K::Throw, prec::PRIMARY, Some(v)));
            }
            0x2b => {
                let obj = self.pop();
                let ctor = self.pop();
                self.push(self.mk(K::ACastOp { ctor, obj }, prec::PRIMARY, None));
            }
            0x2c => {
                let sub = self.pop();
                let n = self.pop_long();
                let supers: Vec<It> = (0..n).map(|_| self.pop()).collect();
                self.add_out(self.mk(K::AImplements { sub, supers: new_list(supers) }, prec::PRIMARY, None));
            }
            0x2d => {
                let n = self.pop_long();
                let args: Vec<It> = (0..n).map(|_| self.pop()).collect();
                self.push(afn("fscommand2", src, ls, args, true));
            }
            0x30 => self.fn1("random"),
            0x31 => self.fn1("mblength"),
            0x32 => self.fn1("ord"),
            0x33 => self.fn1("chr"),
            0x34 => self.push(afn_pure("getTimer", src, ls, vec![])),
            0x36 => self.fn1("mbord"),
            0x37 => self.fn1("mbchr"),
            0x3a => {
                let prop = self.pop();
                let obj = self.pop();
                self.push(self.mk(K::ADelete { obj: Some(obj), prop }, prec::PRIMARY, None));
            }
            0x3b => {
                let prop = self.pop();
                self.push(self.mk(K::ADelete { obj: None, prop }, prec::PRIMARY, None));
            }
            0x3c => {
                self.allow_swap();
                let value = self.pop();
                let name = self.pop();
                let ns = self.name_str(&name);
                self.tr.vars.borrow_mut().put(&ns, Some(value.clone()));
                self.add_out(self.mk(K::ADefineLocal { name }, prec::PRIMARY, Some(value)));
            }
            0x3d => {
                let name = self.pop();
                let n = self.pop_long();
                let args: Vec<It> = (0..n).map(|_| self.pop()).collect();
                let calculated = self.tr.funcs.borrow().get(&no_quotes_plain(&name));
                self.push(self.mk(K::ACallFunction { name, args: new_list(args), calculated }, prec::PRIMARY, None));
            }
            0x3e => {
                let v = self.pop();
                self.add_out(self.mk(K::AReturn, prec::PRIMARY, Some(v)));
            }
            0x3f => self.bin(Bin::Mod),
            0x40 => {
                let name = self.pop();
                let n = self.pop_long();
                let args: Vec<It> = (0..n).map(|_| self.pop()).collect();
                self.push(self.mk(K::ANewObject { name, args: new_list(args) }, prec::PRIMARY, None));
            }
            0x41 => {
                let name = self.pop();
                self.add_out(self.mk(K::ADefineLocal { name }, prec::PRIMARY, None));
            }
            0x42 => {
                let n = self.pop_long();
                let args: Vec<It> = (0..n).map(|_| self.pop()).collect();
                self.push(self.mk(K::AInitArray { values: new_list(args) }, prec::PRIMARY, None));
            }
            0x43 => {
                let n = self.pop_long();
                let (mut values, mut names) = (Vec::new(), Vec::new());
                for _ in 0..n {
                    values.push(self.pop());
                    names.push(self.pop());
                }
                self.push(self.mk(K::AInitObject { names: new_list(names), values: new_list(values) }, prec::PRIMARY, None));
            }
            0x44 => {
                let v = self.pop();
                self.push(self.mk(K::ATypeOf, prec::UNARY, Some(v)));
            }
            0x45 => {
                let v = self.pop();
                self.push(afn("targetPath", src, ls, vec![v], true));
            }
            0x46 | 0x55 => {
                let obj = self.pop();
                self.push(self.mk(K::AEnumerated, prec::PRIMARY, None));
                self.add_out(self.mk(K::AEnumerate { obj }, prec::PRIMARY, None));
            }
            0x4a => self.fn1("Number"),
            0x4b => self.fn1("String"),
            0x4c => self.dup(),
            0x4d => self.swap(),
            0x4e => {
                let member = self.pop();
                let obj = self.pop();
                self.push(self.mk(K::AGetMember { obj, member, print_obf: false, getter: false }, prec::PRIMARY, None));
            }
            0x4f => {
                self.finish();
                self.allow_swap();
                self.move_to();
                let value = through_duplicate(&self.pop());
                let member = self.pop();
                let obj = self.pop();
                self.handle_set_member(false, value, member, obj);
            }
            0x50 => {
                let a = self.pop();
                self.push(self.mk(K::AInc { obj: a }, prec::ADDITIVE, None));
            }
            0x51 => {
                let a = self.pop();
                self.push(self.mk(K::ADec { obj: a }, prec::ADDITIVE, None));
            }
            0x52 => self.call_method(),
            0x53 => {
                let name = self.pop();
                let obj = self.pop();
                let n = self.pop_long();
                let args: Vec<It> = (0..n).map(|_| self.pop()).collect();
                self.push(self.mk(K::ANewMethod { obj, name, args: new_list(args) }, prec::PRIMARY, None));
            }
            0x54 => self.bin(Bin::InstanceOf),
            0x60 => self.bin(Bin::BitAnd),
            0x61 => self.bin(Bin::BitOr),
            0x62 => self.bin(Bin::BitXor),
            0x63 => self.bin(Bin::LShift),
            0x64 => self.bin(Bin::RShift),
            0x65 => self.bin(Bin::URShift),
            0x66 => self.bin(Bin::StrictEq),
            0x67 => self.bin(Bin::Gt),
            0x68 => self.bin(Bin::StrGt),
            0x69 => {
                let sup = self.pop();
                let sub = self.pop();
                self.add_out(self.mk(K::AExtends { sub, sup }, prec::PRIMARY, None));
            }
            0x9e => {
                let v = self.pop();
                self.add_out(afn("call", src, ls, vec![v], false));
            }
            _ => {}
        }
    }

    fn push_values(&mut self, values: &[Value], pool: Option<&[String]>) {
        for (pos, v) in values.iter().enumerate() {
            let pos = pos as i32;
            let (src, ls) = (self.src(), self.ls());
            let to_push = match v {
                Value::Constant(i) => match pool.and_then(|p| p.get(*i as usize)) {
                    Some(s) => direct_at(src, ls, pos, AVal::Str(s.clone())),
                    None => amk(K::AUnresolved(*i as i32), src, ls, prec::PRIMARY, None),
                },
                Value::Bool(true) => true_item(src, ls),
                Value::Bool(false) => false_item(src, ls),
                Value::Str(s) => direct_at(src, ls, pos, AVal::Str(s.clone())),
                Value::Float(f) => direct_at(src, ls, pos, AVal::Float(*f)),
                Value::Double(d) => direct_at(src, ls, pos, AVal::Double(*d)),
                Value::Int(i) => direct_at(src, ls, pos, AVal::Int(*i)),
                Value::Null => direct_at(src, ls, pos, AVal::Null),
                Value::Undefined => direct_at(src, ls, pos, AVal::Undefined),
                Value::Register(r) => {
                    let num = *r as i32;
                    let name = self.tr.reg_names.borrow().get(&num).cloned();
                    let computed = self.tr.vars.borrow().get(&format!("__register{num}"));
                    let temp = computed.as_ref().and_then(|c| match &c.borrow().k {
                        K::ATempReg { used, .. } => Some((used.clone(), c.borrow().val())),
                        _ => None,
                    });
                    if let Some((used, tv)) = temp {
                        used.set(true);
                        self.finish();
                        let c = computed.clone().unwrap();
                        let idx = self.out.borrow().iter().position(|x| matches!(&x.borrow().k, K::ATempRegMark { reg } if same(reg, &c)));
                        if let Some(i) = idx {
                            self.out.borrow_mut().remove(i);
                        }
                        self.move_to();
                        temp_reg(num, tv)
                    } else {
                        let d = direct_at(src, ls, pos, AVal::Register { num, name });
                        if let K::ADirect { computed: cv, .. } = &mut d.borrow_mut().k {
                            *cv = computed;
                        }
                        d
                    }
                }
            };
            to_push.borrow_mut().pos = pos;
            self.push(to_push);
        }
    }

    fn dup(&mut self) {
        let v = self.pop();
        if is::simple_value(&v) {
            self.push(v.clone());
            self.push(v);
            return;
        }
        let temp = match temp_index(&v) {
            Some(t) => {
                self.push(v.clone());
                t
            }
            None => {
                let t = self.ld.max_temp.get() + 1;
                self.ld.max_temp.set(t);
                self.finish();
                self.add_out(self.mk(K::SetTemporary { temp: t, declaration: false, ref_count: 2, suffix: "dup".into() }, prec::ASSIGNMENT, Some(v.clone())));
                self.finish();
                self.push(self.mk(K::DuplicateSource { temp: t, declaration: false }, prec::PRIMARY, Some(v.clone())));
                t
            }
        };
        self.push(self.mk(K::Duplicate { temp }, prec::PRIMARY, Some(v)));
    }

    fn swap(&mut self) {
        let o1 = self.pop();
        let o2 = self.pop();
        let simple = |o: &It| is::simple_value(o) && !is::duplicate(o);
        if (simple(&o1) && simple(&o2)) || matches!(o2.borrow().k, K::Temporary { .. } | K::Duplicate { .. } | K::DuplicateSource { .. }) {
            self.push(o1);
            self.push(o2);
            return;
        }
        let temp = self.ld.max_temp.get() + 1;
        self.ld.max_temp.set(temp);
        self.finish();
        self.add_out(self.mk(K::SetTemporary { temp, declaration: false, ref_count: 1, suffix: "swap".into() }, prec::ASSIGNMENT, Some(o2.clone())));
        self.push(o1);
        self.push(self.mk(K::Temporary { temp }, prec::PRIMARY, Some(o2)));
        self.finish();
    }

    fn get_url(&mut self, url: &str, target: &str) {
        let (src, ls) = (self.src(), self.ls());
        if let Some(cmd) = url.strip_prefix("FSCommand:") {
            let mut args = vec![dstr(cmd)];
            if !target.is_empty() {
                args.push(dstr(target));
            }
            self.add_out(afn("fscommand", src, ls, args, false));
            return;
        }
        if let Some(num) = target.strip_prefix("_level").and_then(|n| n.parse::<i32>().ok()) {
            if url.is_empty() {
                self.add_out(afn("unloadMovieNum", src, ls, vec![direct(AVal::Int(num as i64))], false));
            } else {
                self.add_out(afn_with("loadMovieNum", src, ls, vec![dstr(url), direct(AVal::Int(num as i64))], ",", ",\"GET\"".into(), false));
            }
            return;
        }
        if url.is_empty() {
            self.add_out(afn("unloadMovie", src, ls, vec![dstr(target)], false));
        } else {
            let mut args = vec![dstr(url)];
            if !target.is_empty() {
                args.push(dstr(target));
            }
            self.add_out(afn_with("getURL", src, ls, args, ", ", String::new(), false));
        }
    }

    fn get_url2(&mut self, load_variables: bool, load_target: bool, method: i32) {
        let (src, ls) = (self.src(), self.ls());
        let target = self.pop();
        let mut url = self.pop();
        let method_str = match method {
            1 => ",\"GET\"".to_string(),
            2 => ",\"POST\"".to_string(),
            _ => String::new(),
        };
        let mut num: Option<It> = None;
        if let Some(t) = compile_time_string(&target) {
            if let Some(n) = t.strip_prefix("_level").and_then(|n| n.parse::<i64>().ok()) {
                num = Some(direct(AVal::Int(n)));
            }
        }
        if num.is_none() {
            if let K::Bin { op: Bin::StrAdd, l, r } = &target.borrow().k {
                if compile_time_string(l).as_deref() == Some("_level") {
                    num = Some(r.clone());
                }
            }
        }
        let call = |name: &'static str, args: Vec<It>, suffix: String| afn_with(name, src, ls, args, ",", suffix, false);
        if load_variables {
            match num {
                Some(n) => self.add_out(call("loadVariablesNum", vec![url, n], method_str)),
                None => self.add_out(call("loadVariables", vec![url, target], method_str)),
            }
            return;
        }
        if load_target {
            if direct_string(&url).is_some_and(|s| s.is_empty()) {
                self.add_out(call("unloadMovie", vec![target], String::new()));
            } else {
                self.add_out(call("loadMovie", vec![url, target], method_str));
            }
            return;
        }
        let mut print_type: Option<It> = None;
        let (mut do_print, mut do_print_bmp, mut do_fs, mut do_unload) = (false, false, false, false);
        if let Some(u) = compile_time_string(&url) {
            if u.is_empty() {
                do_unload = true;
            } else if let Some(r) = u.strip_prefix("print:#") {
                print_type = Some(dstr(r));
                do_print = true;
            } else if let Some(r) = u.strip_prefix("printasbitmap:#") {
                print_type = Some(dstr(r));
                do_print_bmp = true;
            } else if let Some(r) = u.strip_prefix("FSCommand:") {
                url = dstr(r);
                do_fs = true;
            } else if u == "print:" {
                print_type = Some(dstr("bmovie"));
                do_print = true;
            } else if u == "printasbitmap:" {
                print_type = Some(dstr("bmovie"));
                do_print_bmp = true;
            }
        } else if let K::Bin { op: Bin::StrAdd, l, r } = { let uk = url.borrow().k.clone(); uk } {
            match compile_time_string(&l).as_deref() {
                Some("print:#") => {
                    print_type = Some(r.clone());
                    do_print = true;
                }
                Some("printasbitmap:#") => {
                    print_type = Some(r.clone());
                    do_print_bmp = true;
                }
                Some("FSCommand:") => {
                    url = r.clone();
                    do_fs = true;
                }
                _ => {}
            }
        }
        if let Some(n) = num {
            if do_unload {
                self.add_out(call("unloadMovieNum", vec![n], String::new()));
            } else if do_print {
                self.add_out(call("printNum", vec![n, print_type.unwrap()], String::new()));
            } else if do_print_bmp {
                self.add_out(call("printAsBitmapNum", vec![n, print_type.unwrap()], String::new()));
            } else {
                self.add_out(call("loadMovieNum", vec![url, n], method_str));
            }
        } else if do_print {
            self.add_out(call("print", vec![target, print_type.unwrap()], String::new()));
        } else if do_print_bmp {
            self.add_out(call("printAsBitmap", vec![target, print_type.unwrap()], String::new()));
        } else if do_fs {
            self.add_out(call("fscommand", vec![url, target], String::new()));
        } else {
            self.add_out(call("getURL", vec![url, target], method_str));
        }
    }

    fn start_drag(&mut self) {
        let target = self.pop();
        let lock = self.pop();
        let constrain = self.pop();
        let literal = is_direct(&constrain) || matches!(constrain.borrow().k, K::True | K::False);
        let has = !(literal && result_as_number(&constrain) == Some(0.0));
        let mut args = vec![target, lock];
        if has {
            let y2 = self.pop();
            let x2 = self.pop();
            let y1 = self.pop();
            let x1 = self.pop();
            args.extend([x1, y1, x2, y2]);
        }
        self.add_out(afn("startDrag", self.src(), self.ls(), args, false));
    }

    fn call_method(&mut self) {
        let name = self.pop();
        let obj = self.pop();
        let n = self.pop_long();
        let args: Vec<It> = (0..n).map(|_| self.pop()).collect();
        if let Some(m) = direct_string(&name) {
            if let Some(var) = m.strip_prefix("__get__").filter(|_| args.is_empty()) {
                self.push(self.mk(K::AGetMember { obj, member: dstr(var), print_obf: false, getter: true }, prec::PRIMARY, None));
                return;
            }
            if let Some(var) = m.strip_prefix("__set__").filter(|_| args.len() == 1) {
                let v = args[0].clone();
                self.handle_set_member(true, v, dstr(var), obj);
                return;
            }
        }
        self.push(self.mk(K::ACallMethod { obj, name, args: new_list(args) }, prec::PRIMARY, None));
    }

    fn set_variable(&mut self) {
        let value = through_duplicate(&self.pop());
        let name = self.pop();
        let ns = self.name_str(&name);
        self.tr.vars.borrow_mut().put(&ns, Some(value.clone()));
        let vk = value.borrow().k.clone();
        if let K::AInc { obj } | K::ADec { obj } = &vk {
            if self.peek().is_some_and(|p| crate::abc::decomp::eq::eq(&p, obj)) {
                self.pop();
                if let K::AGetVar { print_obf, .. } = &mut obj.borrow_mut().k {
                    *print_obf = true;
                }
                let k = if matches!(vk, K::AInc { .. }) { K::APostInc { obj: obj.clone() } } else { K::APostDec { obj: obj.clone() } };
                self.push(self.mk(k, prec::POSTFIX, None));
                return;
            }
        }
        if let K::AInc { obj } | K::ADec { obj } = &vk {
            let inc = matches!(vk, K::AInc { .. });
            let mk_post = |x: &Self, o: It| x.mk(if inc { K::APostInc { obj: o } } else { K::APostDec { obj: o } }, prec::POSTFIX, None);
            let ok = matches!(&obj.borrow().k, K::AGetVar { name: n2, .. } if crate::abc::decomp::eq::eq(n2, &name));
            if ok {
                if let K::AGetVar { print_obf, .. } = &mut obj.borrow_mut().k {
                    *print_obf = true;
                }
                self.add_out(mk_post(self, obj.clone()));
                return;
            }
            let eval_dup = matches!(obj.borrow().k, K::AEval) && obj.borrow().value.as_ref().is_some_and(|v| is::duplicate(v) && v.borrow().value.as_ref().is_some_and(|vv| same(vv, &name)));
            if eval_dup {
                let gv = amk(K::AGetVar { name: name.clone(), print_obf: false, computed: None }, None, None, prec::PRIMARY, None);
                self.add_out(mk_post(self, gv));
                return;
            }
        }
        let set_var = self.mk(K::ASetVar { name: name.clone(), compound_value: None, compound_op: None }, prec::ASSIGNMENT, Some(value.clone()));
        let mut ret = set_var.clone();
        let mut inside = not_coerced_no_dup(&value);
        if matches!(inside.borrow().k, K::AStoreReg { .. }) {
            let v = inside.borrow().val();
            inside = v;
        }
        if let Some((op, l, r)) = compoundable(&inside) {
            if !has_side_effect(&name) {
                let matched = match &l.borrow().k {
                    K::AGetVar { name: n2, .. } => crate::abc::decomp::eq::eq(&name, n2),
                    K::AEval => l.borrow().value.as_ref().is_some_and(|v| is::duplicate(v) && v.borrow().value.as_ref().is_some_and(|vv| same(vv, &name))),
                    _ => false,
                };
                if matched {
                    if let K::ASetVar { compound_value, compound_op, .. } = &mut set_var.borrow_mut().k {
                        *compound_value = Some(r.clone());
                        *compound_op = Some(op.op().to_string());
                    }
                }
            }
        }
        if let Some(r) = self.define_store(&value, &ret, |v| match &v.borrow().k {
            K::AGetVar { name: n2, .. } => crate::abc::decomp::eq::eq(n2, &name),
            _ => false,
        }) {
            ret = r;
            let _ = ret;
            return;
        }
        self.add_out(ret);
    }

    /// The `StoreRegister` with `define` inside a set: turns the set into `++x`/`x = ` with a
    /// temporary register. Returns `Some` when it handled the output.
    fn define_store(&mut self, value: &It, set: &It, same_target: impl Fn(&It) -> bool) -> Option<It> {
        let (reg, define) = match &value.borrow().k {
            K::AStoreReg { reg, define, .. } => (*reg, *define),
            _ => return None,
        };
        if !define {
            return None;
        }
        let sr = value.clone();
        let inner = sr.borrow().val();
        set.borrow_mut().value = Some(inner.clone());
        let mut ret = set.clone();
        let ik = inner.borrow().k.clone();
        match &ik {
            K::AInc { obj } | K::ADec { obj } => {
                if same_target(obj) {
                    if let K::AGetVar { print_obf, .. } = &mut obj.borrow_mut().k {
                        *print_obf = true;
                    }
                    if let K::AGetProp { use_fn, .. } = &mut obj.borrow_mut().k {
                        *use_fn = false;
                    }
                    let op = if matches!(ik, K::AInc { .. }) { Un::PreIncrement } else { Un::PreDecrement };
                    ret = self.mk(K::Un { op, ty: None }, prec::UNARY, Some(obj.clone()));
                }
            }
            _ => {
                if let K::AStoreReg { temporary, .. } = &mut sr.borrow_mut().k {
                    *temporary = true;
                }
                set.borrow_mut().value = Some(sr.clone());
            }
        }
        let tr = temp_reg(reg, ret);
        self.tr.vars.borrow_mut().put(&format!("__register{reg}"), Some(tr.clone()));
        self.add_out(amk(K::ATempRegMark { reg: tr }, None, None, prec::NOPRECEDENCE, None));
        Some(set.clone())
    }

    fn set_property(&mut self) {
        let value = through_duplicate(&self.pop());
        let index = through_duplicate(&self.pop());
        let target = through_duplicate(&self.pop());
        let idx = property_index(&index);
        let vk = through_duplicate(&value).borrow().k.clone();
        if let K::AInc { obj } | K::ADec { obj } = &vk {
            let inc = matches!(vk, K::AInc { .. });
            if self.peek().is_some_and(|p| crate::abc::decomp::eq::eq(&p, obj)) {
                self.pop();
                if let K::AGetProp { use_fn, .. } = &mut obj.borrow_mut().k {
                    *use_fn = false;
                }
                self.push(self.mk(if inc { K::APostInc { obj: obj.clone() } } else { K::APostDec { obj: obj.clone() } }, prec::POSTFIX, None));
                return;
            }
            let gp = match &obj.borrow().k {
                K::AGetProp { target: t2, index: i2, .. } => crate::abc::decomp::eq::eq(t2, &target) && *i2 == idx,
                _ => false,
            };
            if gp {
                self.add_out(self.mk(if inc { K::APostInc { obj: obj.clone() } } else { K::APostDec { obj: obj.clone() } }, prec::POSTFIX, None));
                return;
            }
        }
        let ret = self.mk(K::ASetProp { target: target.clone(), index: idx }, prec::ASSIGNMENT, Some(value.clone()));
        let target2 = target.clone();
        if self
            .define_store(&value, &ret, |v| matches!(&v.borrow().k, K::AGetProp { target: t2, index: i2, .. } if crate::abc::decomp::eq::eq(t2, &target2) && *i2 == idx))
            .is_some()
        {
            return;
        }
        self.add_out(ret);
    }

    fn store_register(&mut self, reg: i32) {
        let value = self.pop();
        let name = self.tr.reg_names.borrow().get(&reg).cloned();
        let key = format!("__register{reg}");
        let is_temp = self.tr.vars.borrow().get(&key).is_some_and(|v| matches!(v.borrow().k, K::ATempReg { .. }));
        if is_temp {
            self.tr.vars.borrow_mut().remove(&key);
        }
        let mut define = !self.tr.vars.borrow().contains(&key);
        if name.is_some() {
            define = false;
        }
        let mut value = value;
        if matches!(value.borrow().k, K::ATempReg { .. }) {
            let v = value.borrow().val();
            value = v;
        }
        self.tr.vars.borrow_mut().put(&key, Some(value.clone()));
        let vk = value.borrow().k.clone();
        match &vk {
            K::ADirect { v: AVal::Register { num, .. }, .. } if *num == reg => {
                self.push(value);
                return;
            }
            K::AStoreReg { reg: r2, .. } if *r2 == reg => {
                self.push(value);
                return;
            }
            K::AInc { obj } | K::ADec { obj } => {
                if self.peek().is_some_and(|p| crate::abc::decomp::eq::eq(&p, obj)) {
                    self.pop();
                    let k = if matches!(vk, K::AInc { .. }) { K::APostInc { obj: obj.clone() } } else { K::APostDec { obj: obj.clone() } };
                    self.push(self.mk(k, prec::POSTFIX, None));
                    self.push(bare(K::Any, prec::PRIMARY));
                    return;
                }
            }
            K::AEnumerated => {
                self.tr.vars.borrow_mut().remove(&key);
            }
            _ => {}
        }
        let ret = self.mk(K::AStoreReg { reg, name, define, temporary: false, compound_value: None, compound_op: None }, prec::ASSIGNMENT, Some(value.clone()));
        if let Some((op, l, r)) = compoundable(&not_coerced_no_dup(&value)) {
            if matches!(&l.borrow().k, K::ADirect { v: AVal::Register { num, .. }, .. } if *num == reg) {
                if let K::AStoreReg { compound_value, compound_op, .. } = &mut ret.borrow_mut().k {
                    *compound_value = Some(r.clone());
                    *compound_op = Some(op.op().to_string());
                }
            }
        }
        self.push(ret);
    }

    /// `ActionSetMember.handleSetMember`.
    fn handle_set_member(&mut self, setter: bool, value: It, member: It, obj: It) {
        self.handle_set_member_inner(setter, value, member, obj);
        if setter {
            self.finish();
            let last = self.out.borrow_mut().pop();
            if let Some(l) = last {
                self.push(l);
                self.move_to();
            }
        }
    }

    fn handle_set_member_inner(&mut self, setter: bool, value: It, member: It, obj: It) {
        let vk = value.borrow().k.clone();
        if let K::AInc { obj: o } | K::ADec { obj: o } = &vk {
            if self.peek().is_some_and(|p| crate::abc::decomp::eq::eq(&p, o)) {
                self.pop();
                let k = if matches!(vk, K::AInc { .. }) { K::APostInc { obj: o.clone() } } else { K::APostDec { obj: o.clone() } };
                self.push(self.mk(k, prec::POSTFIX, None));
                if setter {
                    self.push(bare(K::Any, prec::PRIMARY));
                }
                return;
            }
        }
        if let K::AInc { obj: o } | K::ADec { obj: o } = &vk {
            let inc = matches!(vk, K::AInc { .. });
            let gm = match &o.borrow().k {
                K::AGetMember { obj: go, member: gmn, .. } => Some((go.clone(), gmn.clone())),
                _ => None,
            };
            if let Some((go, gmn)) = gm {
                if crate::abc::decomp::eq::eq(&through_duplicate(&go), &through_duplicate(&obj)) && crate::abc::decomp::eq::eq(&gmn, &member) {
                    let td = through_duplicate(&go);
                    if let K::AGetMember { obj: oo, .. } = &mut o.borrow_mut().k {
                        *oo = td.clone();
                    }
                    self.cleanup_temp(&td, &obj);
                    let target = through_duplicate(o);
                    let k = if setter {
                        K::Un { op: if inc { Un::PreIncrement } else { Un::PreDecrement }, ty: None }
                    } else if inc {
                        K::APostInc { obj: target.clone() }
                    } else {
                        K::APostDec { obj: target.clone() }
                    };
                    let item = if setter { self.mk(k, prec::UNARY, Some(target)) } else { self.mk(k, prec::POSTFIX, None) };
                    self.add_out(item);
                    return;
                }
            }
        }
        let set_mem = self.mk(K::ASetMember { obj: obj.clone(), member: member.clone(), compound_value: None, compound_op: None, setter }, prec::ASSIGNMENT, Some(value.clone()));
        let mut inside = not_coerced_no_dup(&value);
        if matches!(inside.borrow().k, K::AStoreReg { .. }) {
            let v = inside.borrow().val();
            inside = v;
        }
        if let Some((op, l, r)) = compoundable(&inside) {
            if !has_side_effect(&obj) && !has_side_effect(&member) {
                let gm = match &l.borrow().k {
                    K::AGetMember { obj: go, member: gmn, .. } => Some((go.clone(), gmn.clone())),
                    _ => None,
                };
                if let Some((go, gmn)) = gm {
                    if crate::abc::decomp::eq::eq(&through_duplicate(&obj), &through_duplicate(&go)) && crate::abc::decomp::eq::eq(&member, &gmn) {
                        self.cleanup_temp(&obj, &go);
                        if let K::ASetMember { compound_value, compound_op, obj: so, .. } = &mut set_mem.borrow_mut().k {
                            *compound_value = Some(r.clone());
                            *compound_op = Some(op.op().to_string());
                            *so = through_duplicate(&obj);
                        }
                    }
                }
            }
        }
        let obj2 = set_mem.clone();
        let target = set_type_object(&obj2);
        if self.define_store(&value, &set_mem, |v| matches!(v.borrow().k, K::AGetMember { .. }) && crate::abc::decomp::eq::eq(v, &target)).is_some() {
            return;
        }
        self.add_out(set_mem);
    }

    fn cleanup_temp(&mut self, d1: &It, d2: &It) {
        let i1 = temp_index(d1);
        let i2 = temp_index(d2);
        if let (Some(a), Some(b)) = (i1, i2) {
            if a != b {
                return;
            }
        }
        let index = i1.or(i2).unwrap_or(-1);
        self.finish();
        let last = self.out.borrow().last().cloned();
        if let Some(l) = last {
            if matches!(&l.borrow().k, K::SetTemporary { temp, .. } if *temp == index) {
                self.out.borrow_mut().pop();
            }
        }
    }
}

fn temp_index(it: &It) -> Option<i32> {
    match &it.borrow().k {
        K::Duplicate { temp } | K::DuplicateSource { temp, .. } | K::SetTemporary { temp, .. } | K::Temporary { temp } => Some(*temp),
        _ => None,
    }
}

fn temp_reg(reg: i32, value: It) -> It {
    let (src, ls, p) = {
        let b = value.borrow();
        (b.src, b.line_start, b.prec)
    };
    amk(K::ATempReg { reg, used: Rc::new(std::cell::Cell::new(false)) }, src, ls, p, Some(value))
}

/// `CompoundableBinaryOpAs12`: operator, left and right of a compoundable AS1/2 binary op.
fn compoundable(it: &It) -> Option<(Bin, It, It)> {
    let b = it.borrow();
    if !b.as2 {
        return None;
    }
    match &b.k {
        K::Bin { op, l, r } if matches!(op, Bin::Add | Bin::Sub | Bin::Mul | Bin::Div | Bin::Mod | Bin::BitAnd | Bin::BitOr | Bin::BitXor | Bin::LShift | Bin::RShift | Bin::URShift) => {
            Some((*op, l.clone(), r.clone()))
        }
        _ => None,
    }
}

fn property_index(index: &It) -> i32 {
    match &index.borrow().k {
        K::ADirect { v: AVal::Int(i), .. } => *i as i32,
        K::ADirect { v: AVal::Double(d), .. } => d.round() as i32,
        K::ADirect { v: AVal::Float(f), .. } => f.round() as i32,
        K::ADirect { v: AVal::Str(s), .. } => s.parse().unwrap_or(0),
        _ => 0,
    }
}

/// `toStringNoQuotes(LocalData.empty)` of a function name.
fn no_quotes_plain(it: &It) -> String {
    match &it.borrow().k {
        K::ADirect { v: AVal::Str(s), .. } => s.clone(),
        K::ADirect { v: AVal::Int(i), .. } => i.to_string(),
        K::ADirect { v: AVal::Double(d), .. } => crate::action::decomp::print::number(*d),
        _ => String::new(),
    }
}
