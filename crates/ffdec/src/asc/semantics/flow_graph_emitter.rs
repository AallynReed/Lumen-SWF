//! Port of `macromedia/asc/semantics/FlowGraphEmitter.java`: the emitter `FlowAnalyzer` drives to build each method's
//! basic blocks and reaching-definition bits.

#![allow(non_upper_case_globals)]

use std::io::Write;

use crate::asc::parser::node::{NodeRef, MAX_DEF_BITS};
use crate::asc::parser::tokens::GET_TOKEN;
use crate::asc::semantics::emitter::{Emitter, EmitterBase};
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::q_name::QName;
use crate::asc::semantics::type_info::TypeInfo;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::util::bit_set::BitSet;
use crate::asc::util::block::BlockRef;
use crate::asc::util::byte_list::ByteList;
use crate::asc::util::context::ContextRef;
use crate::asc::util::int_list::IntList;

pub const scopes_register: i32 = 0;
pub const obj_register: i32 = 1;
pub const args_register: i32 = 2;

pub struct FlowGraphEmitter {
    base: EmitterBase,
    has_leader: bool,
    blocks: Vec<BlockRef>,
    defs: Vec<Option<NodeRef>>,
    cur_block: i32,
    def_count: i32,
    show_blocks: bool,
    within_try_block: i32,
    cx: ContextRef,

    pub max_temp_count: i32,
    pub cur_temp_count: i32,
    pub max_stack: i32,
    pub cur_stack: i32,
    pub max_params: u16,
    pub scriptname: String,

    pub if_addrs: IntList,
    pub else_addrs: IntList,
    pub loopbegin_addrs: IntList,
    pub break_addrs: Vec<IntList>,
    pub continue_addrs: Vec<IntList>,
    pub switchbegin_addrs: IntList,
    pub case_addrs: Vec<IntList>,
    pub default_addrs: IntList,
    pub seen_default_case: IntList,
    pub current_with_count: i32,
    pub with_count: IntList,

    pub try_addrs: IntList,
    pub catch_addrs: Vec<IntList>,

    pub stack_depth_stack: IntList,

    pub show_instructions: bool,
    pub show_linenums: bool,
    pub show_stacknames: bool,
}

/// `IntList.last()`.
fn last(l: &IntList) -> i32 {
    l.last().copied().unwrap_or(0)
}

/// `IntList.removeLast()`.
fn remove_last(l: &mut IntList) -> i32 {
    l.pop().unwrap_or(0)
}

/// `IntList.pop_back()`.
fn pop_back(l: &mut IntList) -> i32 {
    l.pop().expect("ArrayIndexOutOfBoundsException")
}

fn print_bits(bits: &Option<BitSet>) {
    let mut n = BitSet::next_set_bit(bits, 0);
    while n >= 0 {
        print!(" d{}", n);
        n = BitSet::next_set_bit(bits, n + 1);
    }
}

impl FlowGraphEmitter {
    pub fn enter_block(&mut self) {
        if self.has_leader {
            self.has_leader = false;
            let b = self.cx.borrow().new_block();
            self.blocks.push(b);
            self.cur_block += 1;
        }
    }

    pub fn new_def(&mut self, node: &NodeRef) -> Option<BitSet> {
        let mut def_mask = None;

        if self.def_count >= MAX_DEF_BITS {
            for block in &self.blocks {
                block.borrow_mut().def_bits = None;
            }
        } else {
            def_mask = BitSet::set_index(None, self.def_count, true);
            if self.defs.len() as i32 <= self.def_count {
                self.defs.resize(self.def_count as usize + 1, None);
            }
            self.defs[self.def_count as usize] = Some(node.clone());
            let mut block = self.blocks[self.cur_block as usize].borrow_mut();
            block.def_bits = BitSet::set_index(block.def_bits.take(), self.def_count, true);
        }
        self.def_count += 1;

        def_mask
    }

    pub fn get_defs(&self, def_bits: &Option<BitSet>) -> Vec<Option<NodeRef>> {
        let mut defs = Vec::new();

        let mut i = BitSet::next_set_bit(def_bits, 0);
        while i >= 0 {
            defs.push(self.defs[i as usize].clone());
            i = BitSet::next_set_bit(def_bits, i + 1);
        }
        defs
    }

    /// `GetBlock()`, the index of the current block.
    pub fn get_block_index(&mut self) -> i32 {
        if !self.has_leader {
            self.has_leader = true;
        }
        self.cur_block
    }

    /// `getBlock()`.
    pub fn get_block(&self) -> BlockRef {
        self.blocks[self.cur_block as usize].clone()
    }

    pub fn add_stmt_to_block(&mut self, stmt: &str) {
        if !self.has_leader {
            self.has_leader = true;
            if self.show_blocks {
                self.blocks[self.cur_block as usize].borrow_mut().stmts = Some(format!("\n    {}", stmt));
            }
        }
    }

    pub fn enter_terminal_block(&mut self) {
        self.enter_block();
        self.blocks[self.cur_block as usize].borrow_mut().is_terminal = true;
        self.add_stmt_to_block("terminal block for return");
        self.enter_block();
    }

    pub fn block_is_terminal(&self, b: i32) -> bool {
        self.blocks[b as usize].borrow().is_terminal
    }

    pub fn add_edge(&mut self, b1: i32, b2: i32) {
        if self.blocks[b1 as usize].borrow().is_terminal {
            return;
        }

        {
            let mut block = self.blocks[b1 as usize].borrow_mut();
            let succs = &mut block.succs;
            if !succs.contains(&b2) {
                succs.push(b2);
            }
        }

        let mut block = self.blocks[b2 as usize].borrow_mut();
        let preds = &mut block.preds;
        if !preds.contains(&b1) {
            preds.push(b1);
        }
    }

    /// `GetBlocks()`: the live block list, reused by the next method.
    pub fn get_blocks(&self) -> &[BlockRef] {
        &self.blocks
    }

    pub fn init_graph(&mut self) {
        self.blocks.clear();
        self.cur_block = 0;
        self.has_leader = true;
        let b = self.cx.borrow().new_block();
        self.blocks.push(b);
    }

    pub fn calc_gen_and_kill(&mut self, cx: &ContextRef) {
        let mut gen_bits: Option<BitSet> = None;
        let mut kill_bits: Option<BitSet> = None;

        for block in &self.blocks {
            let defbits = block.borrow().def_bits.clone();

            let mut n = BitSet::next_set_bit(&defbits, 0);
            while n >= 0 {
                if let Some(node) = &self.defs[n as usize] {
                    let r = node.borrow_mut().get_ref(cx).expect("NullPointerException");
                    let slot = r.get_slot_kind(&cx.borrow(), GET_TOKEN);
                    if let Some(slot) = slot {
                        gen_bits = BitSet::set_index(None, n, true);
                        if slot.get_def_bits().is_some() {
                            kill_bits = BitSet::xor(slot.get_def_bits(), gen_bits.clone());
                        } else {
                            kill_bits = gen_bits.clone();
                        }
                    }
                }
                let mut b = block.borrow_mut();
                b.gen_bits = BitSet::reset_set(b.gen_bits.take(), kill_bits.clone(), gen_bits.clone());
                b.kill_bits = BitSet::reset_set(b.kill_bits.take(), gen_bits.clone(), kill_bits.clone());
                n = BitSet::next_set_bit(&defbits, n + 1);
            }
        }
    }

    pub fn calc_in_and_out(&mut self, _cx: &ContextRef) {
        for block in &self.blocks {
            let mut b = block.borrow_mut();
            b.in_bits = None;
            b.out_bits = BitSet::copy(&b.gen_bits);
        }

        loop {
            let mut change = false;

            for block in &self.blocks {
                let preds = block.borrow().preds.clone();
                let mut terminal_preds = 0;
                for &p in &preds {
                    let pred = self.blocks[p as usize].borrow();
                    let pred_out = pred.out_bits.clone();
                    let pred_has_no_preds = pred.preds.is_empty();
                    drop(pred);
                    let mut b = block.borrow_mut();
                    b.in_bits = BitSet::set(b.in_bits.take(), pred_out);
                    if p != 0 && pred_has_no_preds {
                        terminal_preds += 1;
                    }
                }
                let mut b = block.borrow_mut();
                if terminal_preds == b.preds.len() && !b.is_terminal {
                    b.is_terminal = true;
                    b.preds.clear();
                    change = true;
                }

                let mut out_bits = BitSet::copy(&b.in_bits);
                out_bits = BitSet::reset_set(out_bits, b.kill_bits.clone(), b.gen_bits.clone());
                if !BitSet::equals(&b.out_bits, &out_bits) {
                    change = true;
                    b.out_bits = out_bits;
                }
            }

            if !change {
                break;
            }
        }
    }

    pub fn print_blocks(&self, _cx: &ContextRef) {
        for (i, block) in self.blocks.iter().enumerate() {
            let block = block.borrow();

            print!("\nB{}: Pred {{", i);
            for p in &block.preds {
                print!(" B{}", p);
            }

            print!(" }} Succ {{");
            for s in &block.succs {
                print!(" B{}", s);
            }

            print!(" }} Def {{");
            print_bits(&block.def_bits);

            print!(" }} Gen {{");
            print_bits(&block.gen_bits);

            print!(" }} Kill {{");
            print_bits(&block.kill_bits);

            print!(" }} In {{");
            print_bits(&block.in_bits);

            print!(" }} Out {{");
            print_bits(&block.out_bits);
            print!(" }}");

            print!("{}", block.stmts.as_deref().unwrap_or("null"));
        }
    }

    pub fn print_flow_graph(&self, _cx: &ContextRef, scriptname: &str, methname: &str) {
        let mut out = String::new();
        out.push_str("\ndigraph cfg {");
        out.push_str("\nratio=fill");
        out.push_str("\nranksep=.25");
        out.push_str("\nnodesep=.2");
        out.push_str("\nrankdir=LR");
        out.push_str("\nedge [arrowsize=.7,labeldistance=1.0,labelangle=-45,labelfontsize=9]");
        out.push_str("\nnode [fontsize=9,shape=box,width=.2,height=.2]");

        for (i, block) in self.blocks.iter().enumerate() {
            out.push_str(&format!("\nB{}[label=\"B{}\"]", i, i));

            for s in &block.borrow().succs {
                out.push_str(&format!("\nB{} . B{}[weight=2]", i, s));
            }
        }
        out.push_str("\n}");
        let path = format!("{}.{}.dot", scriptname, methname);
        if let Err(ex) = std::fs::File::create(&path).and_then(|mut f| f.write_all(out.as_bytes())) {
            eprintln!("{}", ex);
        }
    }

    /// `FinishClass(cx, name, basename, is_dynamic)`, which does not override `Emitter.FinishClass`.
    pub fn finish_class_is_dynamic(&mut self, _cx: &ContextRef, _name: &QName, _basename: Option<&QName>, _is_dynamic: bool) {}

    pub fn make_dispatch_method(&mut self, _cx: &ContextRef) {}

    pub fn stack(&mut self, size: i32) {
        self.cur_stack += size;
        if self.cur_stack > self.max_stack {
            self.max_stack = self.cur_stack;
        }
    }

    pub fn save_stack_depth(&mut self) {
        self.stack_depth_stack.push(self.cur_stack);
    }

    pub fn restore_stack_depth(&mut self) {
        self.cur_stack = remove_last(&mut self.stack_depth_stack);
    }

    fn print_line_number(&mut self) {
        if self.show_linenums {
            let (mut name, mut ln, mut col) = (None, 0, 0);
            self.get_origin_and_position(&mut name, &mut ln, &mut col);
            print!("\n[Ln {}]", ln);
        }
    }

    pub fn new(cx: &ContextRef, scriptname: &str, show_blocks: bool) -> FlowGraphEmitter {
        let mut e = FlowGraphEmitter {
            base: EmitterBase::new(None),
            has_leader: true,
            blocks: Vec::new(),
            defs: Vec::new(),
            cur_block: 0,
            def_count: 0,
            show_blocks,
            within_try_block: 0,
            cx: cx.clone(),
            max_temp_count: 0,
            cur_temp_count: 0,
            max_stack: 0,
            cur_stack: 0,
            max_params: 0,
            scriptname: scriptname.to_string(),
            if_addrs: IntList::new(),
            else_addrs: IntList::new(),
            loopbegin_addrs: IntList::new(),
            break_addrs: Vec::new(),
            continue_addrs: Vec::new(),
            switchbegin_addrs: IntList::new(),
            case_addrs: Vec::new(),
            default_addrs: IntList::new(),
            seen_default_case: IntList::new(),
            current_with_count: 0,
            with_count: IntList::new(),
            try_addrs: IntList::new(),
            catch_addrs: Vec::new(),
            stack_depth_stack: IntList::new(),
            show_instructions: false,
            show_linenums: false,
            show_stacknames: false,
        };
        e.init_graph();
        e.enter_block();
        e
    }
}

impl Emitter for FlowGraphEmitter {
    fn emitter_base(&self) -> &EmitterBase {
        &self.base
    }

    fn emitter_base_mut(&mut self) -> &mut EmitterBase {
        &mut self.base
    }

    fn start_class(&mut self, _scriptname: &str) {}

    fn start_program(&mut self, _scriptname: &str) {}

    fn finish_program(&mut self, _cx: &ContextRef, _name: &str, _unused: i32) {}

    fn start_method(&mut self, name: &str, param_count: i32, local_count: i32, _temp_count: i32, _needs_activation: bool, _needs_arguments: i32) {
        if self.show_instructions {
            print!("\n// ++StartMethod {}, {}, {}", name, param_count, local_count);
        }
        self.init_graph();
        self.enter_block();
        self.add_edge(self.cur_block - 1, self.cur_block);
        self.max_temp_count = 0;
        self.cur_temp_count = 0;
    }

    fn finish_method(
        &mut self,
        cx: &ContextRef,
        name: &str,
        _type_: Option<&TypeInfo>,
        _types: Option<&[TypeInfo]>,
        _activation: Option<&ObjectValue>,
        _needs_arguments: i32,
        _scope_depth: i32,
        _debug_name: &str,
        _is_native: bool,
        _is_interface: bool,
        _arg_names: Option<&[String]>,
    ) -> i32 {
        if self.show_instructions {
            print!("\n// --FinishMethod {}", name);
        }

        if !self.has_leader {
            self.blocks.pop();
            self.cur_block -= 1;
        }

        self.calc_gen_and_kill(cx);
        self.calc_in_and_out(cx);
        if self.show_blocks {
            print!("\n{}:", name);
            self.print_blocks(cx);
        }
        0
    }

    fn allocate_temp(&mut self) -> i32 {
        let temp = self.cur_temp_count;
        self.cur_temp_count += 1;
        if self.cur_temp_count > self.max_temp_count {
            self.max_temp_count = self.cur_temp_count;
        }
        temp - 1
    }

    fn free_temp(&mut self, _t: i32) {
        self.cur_temp_count -= 1;
    }

    fn get_temp_count(&mut self) -> i32 {
        self.max_temp_count
    }

    fn get_ip(&mut self) -> i32 {
        0
    }

    fn break_(&mut self, loop_index: i32) {
        self.print_line_number();
        if self.show_instructions {
            print!(" Break target {}", loop_index);
        }

        if loop_index >= 0 && (loop_index as usize) < self.break_addrs.len() {
            self.add_stmt_to_block("Break");
            let b = self.cur_block;
            self.break_addrs[loop_index as usize].push(b);
        }

        self.enter_block();
    }

    fn continue_(&mut self, loop_index: i32) {
        self.print_line_number();
        if self.show_instructions {
            print!(" Continue {}", loop_index);
        }

        if loop_index >= 0 && (loop_index as usize) < self.continue_addrs.len() {
            self.add_stmt_to_block("Continue");
            let b = self.cur_block;
            self.continue_addrs[loop_index as usize].push(b);
        }

        self.enter_block();
    }

    fn case_label(&mut self, is_default: bool) {
        self.print_line_number();
        if self.show_instructions {
            if is_default {
                print!(" CaseLabel default");
            } else {
                print!(" CaseLabel");
            }
        }

        self.enter_block();

        if !self.case_addrs.last().expect("NullPointerException").is_empty() {
            self.add_edge(self.cur_block - 1, self.cur_block);
        }

        let cur = self.cur_block;
        if is_default {
            if last(&self.seen_default_case) == 0 {
                self.default_addrs.push(cur);
                self.case_addrs.last_mut().unwrap().push(cur);
                *self.seen_default_case.last_mut().expect("ArrayIndexOutOfBoundsException") = 1;
            }
        } else {
            self.case_addrs.last_mut().expect("NullPointerException").push(cur);
        }
    }

    fn try_(&mut self, _has_finally: bool) {
        self.print_line_number();
        if self.show_instructions {
            print!(" Try");
        }

        self.within_try_block += 1;

        self.enter_block();
        self.add_edge(self.cur_block - 1, self.cur_block);

        self.try_addrs.push(self.cur_block);
        self.catch_addrs.push(IntList::new());
    }

    fn catch_clauses_begin(&mut self) {
        self.print_line_number();
        if self.show_instructions {
            print!(" CatchClausesBegin");
        }
    }

    fn catch(&mut self, _type_: Option<&TypeValue>, _name: &QName) {
        self.enter_block();

        let cur = self.cur_block;
        self.catch_addrs.last_mut().expect("NullPointerException").push(cur);
    }

    fn catch_clauses_end(&mut self) {
        self.print_line_number();
        if self.show_instructions {
            print!(" CatchClausesEnd");
        }

        if self.within_try_block > 0 {
            self.within_try_block -= 1;
        }

        self.enter_block();

        let catch_list = self.catch_addrs.last().expect("NullPointerException").clone();
        let try_start = last(&self.try_addrs);
        let try_end = *catch_list.first().expect("ArrayIndexOutOfBoundsException");
        for i in try_start..try_end {
            for &c in &catch_list {
                self.add_edge(i, c);
            }
        }

        for i in try_start..self.cur_block {
            self.add_edge(i, self.cur_block);
        }
    }

    fn finally_clause_end(&mut self) {
        self.print_line_number();
        if self.show_instructions {
            print!(" FinallyClauseEnd");
        }

        pop_back(&mut self.try_addrs);
        self.catch_addrs.pop();
    }

    fn throw(&mut self) {
        self.print_line_number();
        if self.show_instructions {
            print!(" Throw");
        }

        if self.within_try_block == 0 {
            self.add_stmt_to_block("Throw");
            self.enter_terminal_block();
        }
    }

    fn switch_table(&mut self) {
        self.print_line_number();
        if self.show_instructions {
            print!(" SwitchTable");
        }

        self.add_stmt_to_block("SwitchTable");

        let default_addr = last(&self.default_addrs);
        remove_last(&mut self.default_addrs);
        self.add_edge(self.cur_block, default_addr);

        let case_addr = self.case_addrs.pop().expect("NullPointerException");
        for &a in &case_addr {
            self.add_edge(self.cur_block, a);
        }

        self.enter_block();
    }

    fn if_(&mut self, _kind: i32) {
        self.print_line_number();
        if self.show_instructions {
            print!(" If");
        }
        self.add_stmt_to_block("If");

        let b = self.get_block_index();
        self.if_addrs.push(b);

        self.save_stack_depth();

        self.enter_block();
        self.add_edge(self.cur_block - 1, self.cur_block);
    }

    fn patch_if(&mut self, _target: i32) {
        self.print_line_number();
        if self.show_instructions {
            print!(" PatchIf");
        }

        self.restore_stack_depth();

        let if_index = last(&self.if_addrs);
        remove_last(&mut self.if_addrs);

        self.enter_block();
        self.add_edge(if_index, self.cur_block);
    }

    fn else_(&mut self) {
        self.print_line_number();
        if self.show_instructions {
            print!(" Else");
        }

        let b = self.get_block_index();
        self.else_addrs.push(b);

        self.enter_block();
    }

    fn patch_break(&mut self, loop_index: i32) {
        self.print_line_number();
        if self.show_instructions {
            print!(" PatchBreak {}", loop_index);
        }

        let mut break_addr = self.break_addrs.pop().expect("NullPointerException");
        while let Some(&break_index) = break_addr.last() {
            self.add_edge(break_index, self.cur_block);
            break_addr.pop();
        }
    }

    fn patch_continue(&mut self, loop_index: i32) {
        self.print_line_number();
        if self.show_instructions {
            print!(" PatchContinue {}", loop_index);
        }

        let mut continue_addr = self.continue_addrs.pop().expect("NullPointerException");
        while let Some(&continue_index) = continue_addr.last() {
            self.add_edge(continue_index, self.cur_block);
            continue_addr.pop();
        }
    }

    fn patch_else(&mut self, _target: i32) {
        self.print_line_number();
        if self.show_instructions {
            print!(" PatchElse");
        }

        let else_index = last(&self.else_addrs);
        remove_last(&mut self.else_addrs);

        self.enter_block();
        self.add_edge(else_index, self.cur_block);
        self.add_edge(self.cur_block - 1, self.cur_block);
    }

    fn return_(&mut self, _type_id: i32) {
        self.print_line_number();
        if self.show_instructions {
            print!(" Return");
        }

        self.add_stmt_to_block("Return");
        if self.within_try_block == 0 {
            self.enter_terminal_block();
        }
    }

    fn label_statement_begin(&mut self) {
        self.break_addrs.push(IntList::new());
    }

    fn label_statement_end(&mut self, loop_index: i32) {
        self.patch_break(loop_index);
    }

    fn loop_begin(&mut self) {
        self.print_line_number();
        if self.show_instructions {
            print!(" LoopBegin");
        }

        self.add_stmt_to_block("LoopBegin");

        self.break_addrs.push(IntList::new());
        self.continue_addrs.push(IntList::new());
        self.loopbegin_addrs.push(self.cur_block + 1);
        self.with_count.push(self.current_with_count);

        self.enter_block();
    }

    fn loop_end(&mut self, _kind: i32) {
        self.print_line_number();
        if self.show_instructions {
            print!(" LoopEnd");
        }

        self.add_edge(self.cur_block, last(&self.loopbegin_addrs));
        self.enter_block();
        self.add_edge(self.cur_block - 1, self.cur_block);
        remove_last(&mut self.loopbegin_addrs);
        pop_back(&mut self.with_count);
    }

    fn patch_loop_begin(&mut self, target: i32) {
        self.print_line_number();
        if self.show_instructions {
            print!(" PatchLoopBegin {}", target);
        }

        self.enter_block();
        self.add_edge(self.cur_block - 1, self.cur_block);
        self.add_edge(last(&self.loopbegin_addrs) - 1, self.cur_block);
    }

    fn patch_switch_begin(&mut self, addr: i32) {
        self.print_line_number();
        if self.show_instructions {
            print!(" PatchSwitchBegin {}", addr);
        }

        let switchbegin_index = last(&self.switchbegin_addrs);
        remove_last(&mut self.seen_default_case);
        remove_last(&mut self.switchbegin_addrs);

        self.enter_block();
        self.add_edge(switchbegin_index, self.cur_block);
    }

    fn switch_begin(&mut self) {
        self.print_line_number();
        if self.show_instructions {
            print!(" SwitchBegin");
        }

        self.add_stmt_to_block("SwitchBegin");

        self.seen_default_case.push(0);
        self.case_addrs.push(IntList::new());
        self.break_addrs.push(IntList::new());
        self.continue_addrs.push(IntList::new());

        let b = self.get_block_index();
        self.switchbegin_addrs.push(b);
        self.enter_block();
    }

    fn emit(&mut self, _bytes: &mut ByteList) {}
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::asc::util::context::{Context, ContextStatics};

    fn dump(e: &FlowGraphEmitter, label: &str) -> String {
        let mut s = format!("{}:", label);
        for (i, b) in e.get_blocks().iter().enumerate() {
            let b = b.borrow();
            let join = |l: &IntList| l.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
            s += &format!(" B{}{}[{}|{}]", i, if b.is_terminal { "t" } else { "" }, join(&b.preds), join(&b.succs));
        }
        s
    }

    /// Expectations from driving Java's `FlowGraphEmitter` with the same calls.
    #[test]
    fn builds_blocks_like_java() {
        let cx = Rc::new(RefCell::new(Context::new(Rc::new(RefCell::new(ContextStatics::new())))));
        let mut e = FlowGraphEmitter::new(&cx, "t", false);
        let q = QName::new(None, "e");
        let finish = |e: &mut FlowGraphEmitter, name: &str| {
            e.finish_method(&cx, name, None, None, None, 0, 0, "", false, false, None);
        };

        e.start_method("m", 0, 0, 0, false, 0);
        e.if_(0);
        e.else_();
        e.patch_if(0);
        e.patch_else(0);
        e.return_(0);
        finish(&mut e, "m");
        assert_eq!(dump(&e, "ifelse"), "ifelse: B0t[|1] B1[0|2,3] B2[1|3] B3[1,2|] B4t[|]");

        e.start_method("m2", 0, 0, 0, false, 0);
        e.loop_begin();
        e.if_(0);
        e.break_(0);
        e.patch_if(0);
        e.continue_(0);
        e.patch_continue(0);
        e.patch_loop_begin(0);
        e.loop_end(0);
        e.patch_break(0);
        e.return_(0);
        finish(&mut e, "m2");
        assert_eq!(dump(&e, "loop"), "loop: B0t[|1] B1[0|5] B2[5|3,4] B3[2|5] B4[2|5] B5[4,1,3|2] B6t[|]");

        e.start_method("m3", 0, 0, 0, false, 0);
        e.switch_begin();
        e.case_label(false);
        e.break_(0);
        e.case_label(false);
        e.case_label(true);
        e.throw();
        e.patch_switch_begin(0);
        e.switch_table();
        e.patch_break(0);
        e.try_(false);
        e.throw();
        e.return_(0);
        e.catch(None, &q);
        e.catch(None, &q);
        e.catch_clauses_end();
        e.finally_clause_end();
        e.label_statement_begin();
        e.break_(0);
        e.label_statement_end(0);
        e.return_(0);
        let temps = (e.allocate_temp(), e.allocate_temp(), e.get_temp_count());
        assert_eq!(temps, (-1, 0, 2));
        finish(&mut e, "m3");
        assert_eq!(
            dump(&e, "switch"),
            "switch: B0t[|1] B1[0|5] B2[5|3,6] B3[2,5|] B4t[|] B5[1|3,2,6] B6[2,5|7] B7[6|8] B8[7|] B9t[|]"
        );
    }
}
