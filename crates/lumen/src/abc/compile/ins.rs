//! Instructions as the generator builds them: shared so later passes can patch offsets.

use std::cell::RefCell;
use std::rc::Rc;

use crate::abc::opcodes::{def, Operand};
use crate::abc::write::write_u32v;

/// Placeholders that become jumps once their targets are known, and exception marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pseudo {
    None,
    Break(i64),
    Continue(i64),
    Finally(i64),
    Mark { exception: usize, kind: u8 },
}

pub const MARK_START: u8 = 0;
pub const MARK_END: u8 = 1;
pub const MARK_TARGET: u8 = 2;
pub const MARK_FINALLY_PART: u8 = 3;

#[derive(Debug, Clone)]
pub struct Ins {
    pub code: u8,
    pub ops: Vec<i64>,
    pub pseudo: Pseudo,
}

pub type I = Rc<RefCell<Ins>>;
pub type Code = Vec<I>;

pub fn ins(code: u8, ops: &[i64]) -> I {
    Rc::new(RefCell::new(Ins { code, ops: ops.to_vec(), pseudo: Pseudo::None }))
}

pub fn pseudo(p: Pseudo) -> I {
    let code = if matches!(p, Pseudo::Mark { .. }) { 0 } else { op::JUMP };
    Rc::new(RefCell::new(Ins { code, ops: vec![0], pseudo: p }))
}

pub fn mark(exception: usize, kind: u8) -> I {
    pseudo(Pseudo::Mark { exception, kind })
}

fn u30_len(v: i64) -> usize {
    let mut w = crate::io::Writer::new();
    write_u32v(&mut w, v as i32 as u32);
    w.data.len()
}

impl Ins {
    pub fn is_mark(&self) -> bool {
        matches!(self.pseudo, Pseudo::Mark { .. })
    }

    pub fn len(&self) -> usize {
        if self.is_mark() {
            return 0;
        }
        let Some(d) = def(self.code) else { return 1 };
        let mut n = 1;
        for (i, k) in d.operands.iter().enumerate() {
            let v = self.ops.get(i).copied().unwrap_or(0);
            match k {
                Operand::Offset | Operand::CaseBaseoffset => n += 3,
                Operand::OptS8 | Operand::DebugType | Operand::RegisterIndex => n += 1,
                Operand::OptCaseOffsets => {
                    n += u30_len(v) + 3 * self.ops.len().saturating_sub(i + 1);
                    break;
                }
                _ => n += u30_len(v),
            }
        }
        n
    }

    pub fn bytes(&self, out: &mut Vec<u8>) {
        if self.is_mark() {
            return;
        }
        let Some(d) = def(self.code) else {
            out.push(self.code);
            return;
        };
        let i = crate::abc::opcodes::Instruction {
            offset: 0,
            len: 0,
            code: self.code,
            name: d.name,
            kinds: d.operands,
            operands: self.ops.clone(),
        };
        out.extend(crate::abc::asm::encode(&i));
    }
}

pub fn code_len(code: &[I]) -> i64 {
    code.iter().map(|i| i.borrow().len() as i64).sum()
}

pub fn code_bytes(code: &[I]) -> Vec<u8> {
    let mut out = Vec::new();
    for i in code {
        i.borrow().bytes(&mut out);
    }
    out
}

/// `AVM2Instructions` codes the generator emits.
pub mod op {
    pub const BKPT: u8 = 0x01;
    pub const NOP: u8 = 0x02;
    pub const THROW: u8 = 0x03;
    pub const GET_SUPER: u8 = 0x04;
    pub const SET_SUPER: u8 = 0x05;
    pub const DXNS: u8 = 0x06;
    pub const DXNS_LATE: u8 = 0x07;
    pub const KILL: u8 = 0x08;
    pub const LABEL: u8 = 0x09;
    pub const IF_NLT: u8 = 0x0c;
    pub const IF_NLE: u8 = 0x0d;
    pub const IF_NGT: u8 = 0x0e;
    pub const IF_NGE: u8 = 0x0f;
    pub const JUMP: u8 = 0x10;
    pub const IF_TRUE: u8 = 0x11;
    pub const IF_FALSE: u8 = 0x12;
    pub const IF_EQ: u8 = 0x13;
    pub const IF_NE: u8 = 0x14;
    pub const IF_LT: u8 = 0x15;
    pub const IF_LE: u8 = 0x16;
    pub const IF_GT: u8 = 0x17;
    pub const IF_GE: u8 = 0x18;
    pub const IF_STRICT_EQ: u8 = 0x19;
    pub const IF_STRICT_NE: u8 = 0x1a;
    pub const LOOKUP_SWITCH: u8 = 0x1b;
    pub const PUSH_WITH: u8 = 0x1c;
    pub const POP_SCOPE: u8 = 0x1d;
    pub const NEXT_NAME: u8 = 0x1e;
    pub const HAS_NEXT: u8 = 0x1f;
    pub const PUSH_NULL: u8 = 0x20;
    pub const PUSH_UNDEFINED: u8 = 0x21;
    pub const PUSH_FLOAT: u8 = 0x22;
    pub const NEXT_VALUE: u8 = 0x23;
    pub const PUSH_BYTE: u8 = 0x24;
    pub const PUSH_SHORT: u8 = 0x25;
    pub const PUSH_TRUE: u8 = 0x26;
    pub const PUSH_FALSE: u8 = 0x27;
    pub const PUSH_NAN: u8 = 0x28;
    pub const POP: u8 = 0x29;
    pub const DUP: u8 = 0x2a;
    pub const SWAP: u8 = 0x2b;
    pub const PUSH_STRING: u8 = 0x2c;
    pub const PUSH_INT: u8 = 0x2d;
    pub const PUSH_UINT: u8 = 0x2e;
    pub const PUSH_DOUBLE: u8 = 0x2f;
    pub const PUSH_SCOPE: u8 = 0x30;
    pub const PUSH_NAMESPACE: u8 = 0x31;
    pub const HAS_NEXT2: u8 = 0x32;
    pub const PUSH_DECIMAL: u8 = 0x33;
    pub const PUSH_DNAN: u8 = 0x34;
    pub const LI8: u8 = 0x35;
    pub const LI16: u8 = 0x36;
    pub const LI32: u8 = 0x37;
    pub const LF32: u8 = 0x38;
    pub const LF64: u8 = 0x39;
    pub const SI8: u8 = 0x3a;
    pub const SI16: u8 = 0x3b;
    pub const SI32: u8 = 0x3c;
    pub const SF32: u8 = 0x3d;
    pub const SF64: u8 = 0x3e;
    pub const NEW_FUNCTION: u8 = 0x40;
    pub const CALL: u8 = 0x41;
    pub const CONSTRUCT: u8 = 0x42;
    pub const CALL_SUPER: u8 = 0x45;
    pub const CALL_PROPERTY: u8 = 0x46;
    pub const RETURN_VOID: u8 = 0x47;
    pub const RETURN_VALUE: u8 = 0x48;
    pub const CONSTRUCT_SUPER: u8 = 0x49;
    pub const CONSTRUCT_PROP: u8 = 0x4a;
    pub const CALL_SUPER_VOID: u8 = 0x4e;
    pub const CALL_PROP_VOID: u8 = 0x4f;
    pub const SXI1: u8 = 0x50;
    pub const SXI8: u8 = 0x51;
    pub const SXI16: u8 = 0x52;
    pub const APPLY_TYPE: u8 = 0x53;
    pub const PUSH_FLOAT4: u8 = 0x54;
    pub const NEW_OBJECT: u8 = 0x55;
    pub const NEW_ARRAY: u8 = 0x56;
    pub const NEW_ACTIVATION: u8 = 0x57;
    pub const NEW_CLASS: u8 = 0x58;
    pub const GET_DESCENDANTS: u8 = 0x59;
    pub const NEW_CATCH: u8 = 0x5a;
    pub const FIND_PROPERTY_STRICT: u8 = 0x5d;
    pub const FIND_PROPERTY: u8 = 0x5e;
    pub const GET_LEX: u8 = 0x60;
    pub const SET_PROPERTY: u8 = 0x61;
    pub const GET_LOCAL: u8 = 0x62;
    pub const SET_LOCAL: u8 = 0x63;
    pub const GET_GLOBAL_SCOPE: u8 = 0x64;
    pub const GET_SCOPE_OBJECT: u8 = 0x65;
    pub const GET_PROPERTY: u8 = 0x66;
    pub const INIT_PROPERTY: u8 = 0x68;
    pub const DELETE_PROPERTY: u8 = 0x6a;
    pub const GET_SLOT: u8 = 0x6c;
    pub const SET_SLOT: u8 = 0x6d;
    pub const CONVERT_S: u8 = 0x70;
    pub const ESC_XELEM: u8 = 0x71;
    pub const ESC_XATTR: u8 = 0x72;
    pub const CONVERT_I: u8 = 0x73;
    pub const CONVERT_U: u8 = 0x74;
    pub const CONVERT_D: u8 = 0x75;
    pub const CONVERT_B: u8 = 0x76;
    pub const CONVERT_O: u8 = 0x77;
    pub const CHECK_FILTER: u8 = 0x78;
    pub const CONVERT_F: u8 = 0x79;
    pub const CONVERT_M: u8 = 0x79;
    pub const UN_PLUS: u8 = 0x7a;
    pub const CONVERT_MP: u8 = 0x7a;
    pub const CONVERT_F4: u8 = 0x7b;
    pub const COERCE: u8 = 0x80;
    pub const COERCE_A: u8 = 0x82;
    pub const COERCE_S: u8 = 0x85;
    pub const AS_TYPE_LATE: u8 = 0x87;
    pub const NEGATE: u8 = 0x90;
    pub const INCREMENT: u8 = 0x91;
    pub const INC_LOCAL: u8 = 0x92;
    pub const DECREMENT: u8 = 0x93;
    pub const DEC_LOCAL: u8 = 0x94;
    pub const TYPE_OF: u8 = 0x95;
    pub const NOT: u8 = 0x96;
    pub const BIT_NOT: u8 = 0x97;
    pub const INCREMENT_P: u8 = 0x9c;
    pub const INC_LOCAL_P: u8 = 0x9d;
    pub const DECREMENT_P: u8 = 0x9e;
    pub const DEC_LOCAL_P: u8 = 0x9f;
    pub const ADD: u8 = 0xa0;
    pub const SUBTRACT: u8 = 0xa1;
    pub const MULTIPLY: u8 = 0xa2;
    pub const DIVIDE: u8 = 0xa3;
    pub const MODULO: u8 = 0xa4;
    pub const LSHIFT: u8 = 0xa5;
    pub const RSHIFT: u8 = 0xa6;
    pub const URSHIFT: u8 = 0xa7;
    pub const BIT_AND: u8 = 0xa8;
    pub const BIT_OR: u8 = 0xa9;
    pub const BIT_XOR: u8 = 0xaa;
    pub const EQUALS: u8 = 0xab;
    pub const STRICT_EQUALS: u8 = 0xac;
    pub const LESS_THAN: u8 = 0xad;
    pub const LESS_EQUALS: u8 = 0xae;
    pub const GREATER_THAN: u8 = 0xaf;
    pub const GREATER_EQUALS: u8 = 0xb0;
    pub const INSTANCE_OF: u8 = 0xb1;
    pub const IS_TYPE_LATE: u8 = 0xb3;
    pub const IN: u8 = 0xb4;
    pub const ADD_P: u8 = 0xb5;
    pub const SUBTRACT_P: u8 = 0xb6;
    pub const MULTIPLY_P: u8 = 0xb7;
    pub const DIVIDE_P: u8 = 0xb8;
    pub const MODULO_P: u8 = 0xb9;
    pub const INCREMENT_I: u8 = 0xc0;
    pub const DECREMENT_I: u8 = 0xc1;
    pub const INC_LOCAL_I: u8 = 0xc2;
    pub const DEC_LOCAL_I: u8 = 0xc3;
    pub const NEGATE_I: u8 = 0xc4;
    pub const NEGATE_P: u8 = 0x8f;
    pub const GET_LOCAL0: u8 = 0xd0;
    pub const SET_LOCAL0: u8 = 0xd4;
    pub const DEBUG: u8 = 0xef;
}
