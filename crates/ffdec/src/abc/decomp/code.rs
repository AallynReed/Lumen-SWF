//! A method body's instruction list prepared for decompilation: FFDec's `AVM2Code` plus
//! the `GraphSource` view the graph works on.

use std::collections::{HashMap, VecDeque};

use crate::abc::opcodes::{disassemble, Dialect, Instruction, Operand};
use crate::abc::{Abc, Exception, MethodBody};

#[allow(dead_code)]
pub mod op {
    pub const NOP: u8 = 0x02;
    pub const THROW: u8 = 0x03;
    pub const GETSUPER: u8 = 0x04;
    pub const SETSUPER: u8 = 0x05;
    pub const DXNS: u8 = 0x06;
    pub const DXNSLATE: u8 = 0x07;
    pub const KILL: u8 = 0x08;
    pub const LABEL: u8 = 0x09;
    pub const IFNLT: u8 = 0x0c;
    pub const IFNLE: u8 = 0x0d;
    pub const IFNGT: u8 = 0x0e;
    pub const IFNGE: u8 = 0x0f;
    pub const JUMP: u8 = 0x10;
    pub const IFTRUE: u8 = 0x11;
    pub const IFFALSE: u8 = 0x12;
    pub const IFEQ: u8 = 0x13;
    pub const IFNE: u8 = 0x14;
    pub const IFLT: u8 = 0x15;
    pub const IFLE: u8 = 0x16;
    pub const IFGT: u8 = 0x17;
    pub const IFGE: u8 = 0x18;
    pub const IFSTRICTEQ: u8 = 0x19;
    pub const IFSTRICTNE: u8 = 0x1a;
    pub const LOOKUPSWITCH: u8 = 0x1b;
    pub const PUSHWITH: u8 = 0x1c;
    pub const POPSCOPE: u8 = 0x1d;
    pub const NEXTNAME: u8 = 0x1e;
    pub const HASNEXT: u8 = 0x1f;
    pub const PUSHNULL: u8 = 0x20;
    pub const PUSHUNDEFINED: u8 = 0x21;
    pub const NEXTVALUE: u8 = 0x23;
    pub const PUSHBYTE: u8 = 0x24;
    pub const PUSHSHORT: u8 = 0x25;
    pub const PUSHTRUE: u8 = 0x26;
    pub const PUSHFALSE: u8 = 0x27;
    pub const PUSHNAN: u8 = 0x28;
    pub const POP: u8 = 0x29;
    pub const DUP: u8 = 0x2a;
    pub const SWAP: u8 = 0x2b;
    pub const PUSHSTRING: u8 = 0x2c;
    pub const PUSHINT: u8 = 0x2d;
    pub const PUSHUINT: u8 = 0x2e;
    pub const PUSHDOUBLE: u8 = 0x2f;
    pub const PUSHSCOPE: u8 = 0x30;
    pub const PUSHNAMESPACE: u8 = 0x31;
    pub const HASNEXT2: u8 = 0x32;
    pub const NEWFUNCTION: u8 = 0x40;
    pub const CALL: u8 = 0x41;
    pub const CONSTRUCT: u8 = 0x42;
    pub const CALLMETHOD: u8 = 0x43;
    pub const CALLSTATIC: u8 = 0x44;
    pub const CALLSUPER: u8 = 0x45;
    pub const CALLPROPERTY: u8 = 0x46;
    pub const RETURNVOID: u8 = 0x47;
    pub const RETURNVALUE: u8 = 0x48;
    pub const CONSTRUCTSUPER: u8 = 0x49;
    pub const CONSTRUCTPROP: u8 = 0x4a;
    pub const CALLPROPLEX: u8 = 0x4c;
    pub const CALLSUPERVOID: u8 = 0x4e;
    pub const CALLPROPVOID: u8 = 0x4f;
    pub const NEWOBJECT: u8 = 0x55;
    pub const NEWARRAY: u8 = 0x56;
    pub const NEWACTIVATION: u8 = 0x57;
    pub const NEWCLASS: u8 = 0x58;
    pub const NEWCATCH: u8 = 0x5a;
    pub const FINDPROPSTRICT: u8 = 0x5d;
    pub const FINDPROPERTY: u8 = 0x5e;
    pub const FINDDEF: u8 = 0x5f;
    pub const GETLEX: u8 = 0x60;
    pub const SETPROPERTY: u8 = 0x61;
    pub const GETLOCAL: u8 = 0x62;
    pub const SETLOCAL: u8 = 0x63;
    pub const GETGLOBALSCOPE: u8 = 0x64;
    pub const GETSCOPEOBJECT: u8 = 0x65;
    pub const GETPROPERTY: u8 = 0x66;
    pub const INITPROPERTY: u8 = 0x68;
    pub const GETSLOT: u8 = 0x6c;
    pub const SETSLOT: u8 = 0x6d;
    pub const COERCE_A: u8 = 0x82;
    pub const CONVERT_I: u8 = 0x73;
    pub const INCLOCAL: u8 = 0x92;
    pub const DECLOCAL: u8 = 0x94;
    pub const INCLOCAL_P: u8 = 0x9d;
    pub const DECLOCAL_P: u8 = 0x9f;
    pub const INCLOCAL_I: u8 = 0xc2;
    pub const DECLOCAL_I: u8 = 0xc3;
    pub const GETLOCAL0: u8 = 0xd0;
    pub const GETLOCAL3: u8 = 0xd3;
    pub const SETLOCAL0: u8 = 0xd4;
    pub const SETLOCAL3: u8 = 0xd7;
    pub const DEBUG: u8 = 0xef;
    pub const DEBUGLINE: u8 = 0xf0;
    pub const DEBUGFILE: u8 = 0xf1;
}

#[derive(Debug, Clone)]
pub struct Ins {
    pub addr: i64,
    pub len: i64,
    pub code: u8,
    pub name: &'static str,
    pub kinds: &'static [Operand],
    pub ops: Vec<i64>,
    pub ignored: bool,
    pub line: i32,
    pub file: Option<String>,
    /// AS1/2 container actions (functions, `with`, `try`): body address ranges.
    pub bodies: Vec<(i64, i64)>,
    /// Address after a container's bodies.
    pub container_end: Option<i64>,
    pub define_function: bool,
}

impl Ins {
    fn from(i: Instruction) -> Ins {
        Ins { addr: i.offset as i64, len: i.len as i64, code: i.code, name: i.name, kinds: i.kinds, ops: i.operands, ignored: false, line: 0, file: None, bodies: vec![], container_end: None, define_function: false }
    }

    pub fn op(&self, i: usize) -> i32 {
        self.ops.get(i).copied().unwrap_or(0) as i32
    }

    pub fn is_jump(&self) -> bool {
        self.code == op::JUMP
    }

    /// `IfTypeIns`, which includes `jump`.
    pub fn is_if_type(&self) -> bool {
        (op::IFNLT..=op::IFSTRICTNE).contains(&self.code)
    }

    pub fn is_branch(&self) -> bool {
        self.is_if_type() || self.code == op::LOOKUPSWITCH
    }

    pub fn is_exit(&self) -> bool {
        matches!(self.code, op::RETURNVALUE | op::RETURNVOID | op::THROW)
    }

    pub fn target_address(&self) -> i64 {
        self.addr + 4 + self.ops[0]
    }

    /// `getOffsets`: every jump target address.
    pub fn offsets(&self) -> Vec<i64> {
        let mut out = Vec::new();
        for (i, k) in self.kinds.iter().enumerate() {
            match k {
                Operand::Offset => out.push(self.addr + self.ops[i] + self.len),
                Operand::CaseBaseoffset => out.push(self.addr + self.ops[i]),
                Operand::OptCaseOffsets => out.extend(self.ops[i + 1..].iter().map(|o| self.addr + o)),
                _ => {}
            }
        }
        out
    }

    /// `getRegisterId` of the getlocal/setlocal families, if this is one.
    pub fn get_local_reg(&self) -> Option<i32> {
        match self.code {
            op::GETLOCAL => Some(self.op(0)),
            c @ op::GETLOCAL0..=op::GETLOCAL3 => Some((c - op::GETLOCAL0) as i32),
            _ => None,
        }
    }

    pub fn set_local_reg(&self) -> Option<i32> {
        match self.code {
            op::SETLOCAL => Some(self.op(0)),
            c @ op::SETLOCAL0..=op::SETLOCAL3 => Some((c - op::SETLOCAL0) as i32),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Code {
    pub ins: Vec<Ins>,
    pub exceptions: Vec<Exception>,
    /// `GraphSource.startIp`: where the graph starts (AS1/2 function bodies).
    pub start_ip: i32,
}

impl Code {
    pub fn new(abc: &Abc, body: &MethodBody) -> Code {
        let handlers: Vec<usize> = body.exceptions.iter().map(|e| e.target).collect();
        let dialect = Dialect::of(abc.major, abc.minor);
        let ins = disassemble(&body.code, &handlers, dialect).into_iter().map(Ins::from).collect();
        Code { ins, exceptions: body.exceptions.clone(), start_ip: 0 }
    }

    pub fn len(&self) -> usize {
        self.ins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ins.is_empty()
    }

    pub fn end_offset(&self) -> i64 {
        match self.ins.last() {
            Some(i) => i.addr + i.len,
            None => 0,
        }
    }

    fn adr2pos_raw(&self, addr: i64) -> Result<usize, usize> {
        match self.ins.binary_search_by_key(&addr, |i| i.addr) {
            Ok(p) => Ok(p),
            Err(_) if addr == self.end_offset() => Ok(self.ins.len()),
            Err(p) => Err(p),
        }
    }

    /// Exact position of an address; the end of code maps to `len()`.
    pub fn adr2pos(&self, addr: i64) -> Option<usize> {
        self.adr2pos_raw(addr).ok()
    }

    /// `adr2pos(addr, true)`: addresses inside an instruction round up to the next one.
    pub fn adr2pos_nearest(&self, addr: i64) -> Option<usize> {
        match self.adr2pos_raw(addr) {
            Ok(p) => Some(p),
            Err(p) if addr < self.end_offset() => Some(p),
            Err(_) => None,
        }
    }

    pub fn pos2adr(&self, pos: usize) -> i64 {
        if pos == self.ins.len() {
            return self.end_offset();
        }
        self.ins[pos].addr
    }

    /// `getBranches`: successor positions of a branch or jump.
    pub fn branches(&self, pos: usize) -> Vec<i32> {
        let i = &self.ins[pos];
        let p = |a: i64| self.adr2pos(a).map_or(-1, |x| x as i32);
        let mut out = Vec::new();
        if i.is_if_type() {
            out.push(p(i.target_address()));
            if !i.is_jump() {
                out.push(p(i.addr + i.len));
            }
        }
        if i.code == op::LOOKUPSWITCH {
            out.push(p(i.addr + i.ops[0]));
            for k in 2..i.ops.len() {
                out.push(p(i.addr + i.ops[k]));
            }
        }
        out
    }

    /// `AVM2Code.visitCode(body)`: for every position, the positions it is reached from.
    pub fn visit_refs(&self) -> Vec<Vec<i32>> {
        let mut refs: Vec<Vec<i32>> = vec![Vec::new(); self.ins.len()];
        self.visit_from(0, 0, &mut refs);
        for e in &self.exceptions {
            if let Some(t) = self.adr2pos(e.target as i64) {
                self.visit_from(t, -1, &mut refs);
            }
        }
        refs
    }

    fn visit_from(&self, ip: usize, last: i32, refs: &mut [Vec<i32>]) {
        let mut q: VecDeque<(usize, i32)> = VecDeque::from([(ip, last)]);
        while let Some((mut ip, mut last)) = q.pop_front() {
            while ip < self.ins.len() {
                refs[ip].push(last);
                last = ip as i32;
                if refs[ip].len() > 1 {
                    break;
                }
                let ins = &self.ins[ip];
                if ins.is_exit() {
                    break;
                }
                if ins.code == op::LOOKUPSWITCH {
                    let base = self.pos2adr(ip);
                    let targets: Option<Vec<usize>> = (2..ins.ops.len()).map(|k| self.adr2pos(base + ins.ops[k])).collect();
                    if let (Some(ts), Some(d)) = (targets, self.adr2pos(base + ins.ops[0])) {
                        for t in ts {
                            q.push_back((t, ip as i32));
                        }
                        ip = d;
                        continue;
                    }
                }
                if ins.is_jump() {
                    if let Some(t) = self.adr2pos(ins.target_address()) {
                        ip = t;
                        continue;
                    }
                } else if ins.is_if_type() {
                    if let Some(t) = self.adr2pos(ins.target_address()) {
                        q.push_back((t, ip as i32));
                    }
                }
                ip += 1;
            }
        }
    }

    /// `GraphSource.visitCode(alternateEntries)`.
    pub fn graph_refs(&self, alternate: &[i32]) -> Vec<Vec<i32>> {
        let n = self.ins.len();
        let mut refs: Vec<Vec<i32>> = vec![Vec::new(); n];
        let mut q: VecDeque<(i32, i32, i32)> = VecDeque::from([(self.start_ip, 0, -1)]);
        for (k, &e) in alternate.iter().enumerate() {
            q.push_back((e, -(k as i32 + 1), -1));
        }
        while let Some((mut ip, mut last, end)) = q.pop_front() {
            while ip >= 0 && (ip as usize) < n && (end == -1 || ip < end) {
                let u = ip as usize;
                refs[u].push(last);
                last = ip;
                if refs[u].len() > 1 {
                    break;
                }
                let ins = &self.ins[u];
                if ins.ignored {
                    ip += 1;
                    continue;
                }
                if ins.is_exit() {
                    break;
                }
                if let Some(cend) = ins.container_end {
                    let p = |a: i64| self.adr2pos(a).map_or(-1, |x| x as i32);
                    for &(s, e) in &ins.bodies {
                        if e > s {
                            q.push_back((p(s), ip, p(e)));
                        }
                    }
                    ip = p(cend);
                    continue;
                }
                if ins.is_branch() || ins.is_jump() {
                    for b in self.branches(u) {
                        if b >= 0 {
                            q.push_back((b, ip, end));
                        }
                    }
                    break;
                }
                ip += 1;
            }
        }
        refs
    }

    /// `calculateDebugFileLine`: the source line each instruction belongs to.
    pub fn calculate_debug_file_line(&mut self, abc: &Abc) {
        struct W {
            file: Option<String>,
            line: i32,
            pos: usize,
            seen: std::rc::Rc<std::cell::RefCell<Vec<bool>>>,
        }
        let n = self.ins.len();
        let mut seen_methods: Vec<usize> = Vec::new();
        let mut q: VecDeque<W> = VecDeque::new();
        q.push_back(W { file: None, line: 0, pos: 0, seen: std::rc::Rc::new(std::cell::RefCell::new(vec![false; n])) });
        while let Some(w) = q.pop_front() {
            let (mut file, mut line, mut pos, seen) = (w.file, w.line, w.pos, w.seen);
            while pos < n {
                if seen.borrow()[pos] {
                    break;
                }
                seen.borrow_mut()[pos] = true;
                let code = self.ins[pos].code;
                if code == op::DEBUGFILE {
                    file = Some(abc.str(self.ins[pos].op(0) as usize).to_string());
                }
                if code == op::DEBUGLINE {
                    line = self.ins[pos].op(0);
                }
                self.ins[pos].file = file.clone();
                self.ins[pos].line = line;
                if code == op::NEWFUNCTION && pos + 1 < n && self.ins[pos + 1].code != op::POP {
                    let m = self.ins[pos].op(0) as usize;
                    if !seen_methods.contains(&m) && abc.body_of.get(m).copied().flatten().is_some() {
                        seen_methods.push(m);
                        q.push_back(W { file: file.clone(), line, pos: 0, seen: std::rc::Rc::new(std::cell::RefCell::new(vec![false; n])) });
                    }
                }
                if code == op::RETURNVALUE || code == op::RETURNVOID {
                    break;
                }
                let ins = &self.ins[pos];
                if ins.is_jump() {
                    match self.adr2pos(ins.target_address()) {
                        Some(t) => {
                            pos = t;
                            continue;
                        }
                        None => break,
                    }
                } else if ins.is_if_type() {
                    match self.adr2pos(ins.target_address()) {
                        Some(t) => q.push_back(W { file: file.clone(), line, pos: t, seen: seen.clone() }),
                        None => break,
                    }
                }
                if ins.code == op::LOOKUPSWITCH {
                    let base = self.pos2adr(pos);
                    let mut bad = false;
                    for k in 0..ins.ops.len() {
                        if k == 1 {
                            continue;
                        }
                        match self.adr2pos(base + ins.ops[k]) {
                            Some(t) => q.push_back(W { file: file.clone(), line, pos: t, seen: seen.clone() }),
                            None => {
                                bad = true;
                                break;
                            }
                        }
                    }
                    let _ = bad;
                }
                pos += 1;
            }
        }
    }

    /// `AVM2DeobfuscatorPushFalseIfFalse`: `pushfalse; iffalse` becomes `nop; jump`.
    pub fn push_false_if_false(&mut self) {
        let refs = self.visit_refs();
        for ip in 1..self.ins.len() {
            if self.ins[ip].code != op::IFFALSE || self.ins[ip - 1].code != op::PUSHFALSE {
                continue;
            }
            if refs[ip].len() > 1 {
                continue;
            }
            self.ins[ip - 1].code = op::NOP;
            self.ins[ip - 1].name = "nop";
            self.ins[ip].code = op::JUMP;
            self.ins[ip].name = "jump";
        }
    }

    /// `fixJumps`: instructions jumping outside the code are dropped.
    pub fn fix_jumps(&mut self) {
        if self.ins.is_empty() {
            return;
        }
        let end = self.end_offset();
        let mut bad: Vec<usize> = Vec::new();
        for (i, ins) in self.ins.iter().enumerate() {
            if ins.is_branch() {
                for t in ins.offsets() {
                    if t > end || t < 0 || self.adr2pos(t).is_none() {
                        bad.push(i);
                        break;
                    }
                }
            }
        }
        for &i in bad.iter().rev() {
            self.remove_instruction(i);
        }
    }

    /// `removeInstruction`: drops an instruction and shifts every offset past it.
    pub fn remove_instruction(&mut self, pos: usize) {
        let rem = self.ins[pos].addr;
        let count = self.ins[pos].len;
        let upd = |ins_addr: i64, target: i64, ofs: i64| -> i64 {
            if target > rem && ins_addr < rem {
                ofs - count
            } else if target <= rem && ins_addr > rem {
                ofs + count
            } else {
                ofs
            }
        };
        for i in 0..self.ins.len() {
            let ins = self.ins[i].clone();
            if ins.code == op::LOOKUPSWITCH {
                let new0 = upd(ins.addr, ins.addr + ins.ops[0], ins.ops[0]);
                self.ins[i].ops[0] = new0;
                for k in 2..ins.ops.len() {
                    self.ins[i].ops[k] = upd(ins.addr, ins.addr + ins.ops[k], ins.ops[k]);
                }
            } else if ins.is_if_type() {
                self.ins[i].ops[0] = upd(ins.addr, ins.target_address(), ins.ops[0]);
            }
            if ins.addr > rem {
                self.ins[i].addr -= count;
            }
        }
        for e in self.exceptions.iter_mut() {
            e.start = upd(-1, e.start as i64, e.start as i64) as usize;
            e.end = upd(-1, e.end as i64, e.end as i64) as usize;
            e.target = upd(-1, e.target as i64, e.target as i64) as usize;
        }
        self.ins.remove(pos);
    }

    /// `getLocalRegNamesFromDebug`.
    pub fn local_reg_names_from_debug(&self, abc: &Abc, max_regs: usize, reg_format: &dyn Fn(i32) -> String) -> HashMap<i32, String> {
        let reserved: Vec<String> = (0..max_regs as i32).map(reg_format).collect();
        let mut by_index: HashMap<i32, String> = HashMap::new();
        let mut by_name: HashMap<String, i32> = HashMap::new();
        for ins in &self.ins {
            if ins.code == op::DEBUG && ins.op(0) == 1 {
                let v = abc.str(ins.op(1) as usize).to_string();
                if reserved.contains(&v) {
                    continue;
                }
                let reg = ins.op(2) + 1;
                if let Some(&e) = by_name.get(&v) {
                    if e != reg {
                        return HashMap::new();
                    }
                }
                by_name.insert(v.clone(), reg);
                by_index.insert(reg, v);
            }
        }
        by_index
    }
}
