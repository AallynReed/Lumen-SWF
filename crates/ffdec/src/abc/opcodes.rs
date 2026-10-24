//! AVM2 instruction set and bytecode decoding.

use crate::error::{Error, Result};
use crate::io::Reader;

use super::read_u30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    ArgCount,
    CaseBaseoffset,
    ClassIndex,
    DebugType,
    DecimalIndex,
    DispatchId,
    DoubleIndex,
    ExceptionIndex,
    Float4Index,
    FloatIndex,
    IntIndex,
    Linenum,
    LocalRegIndex,
    MethodIndex,
    MultinameIndex,
    NamespaceIndex,
    NumberContext,
    Offset,
    OptCaseOffsets,
    OptS16,
    OptS8,
    OptU30,
    RegisterIndex,
    ScopeIndex,
    SlotIndex,
    StringIndex,
    UintIndex,
}

pub const NO_FP: u8 = 1;
pub const UNKNOWN_OPS: u8 = 2;
pub const DOMAIN_MEMORY: u8 = 4;
pub const DEPRECATED: u8 = 8;

pub static OPS: &[(u8, &str, &[Operand], u8)] = &[
    (0x01, "bkpt", &[], 0),
    (0x02, "nop", &[], 0),
    (0x03, "throw", &[], 0),
    (0x04, "getsuper", &[Operand::MultinameIndex], 0),
    (0x05, "setsuper", &[Operand::MultinameIndex], 0),
    (0x06, "dxns", &[Operand::StringIndex], 0),
    (0x07, "dxnslate", &[], 0),
    (0x08, "kill", &[Operand::LocalRegIndex], 0),
    (0x09, "label", &[], 0),
    (0x0a, "lf32x4", &[], NO_FP | DOMAIN_MEMORY),
    (0x0b, "sf32x4", &[], NO_FP | DOMAIN_MEMORY),
    (0x0c, "ifnlt", &[Operand::Offset], 0),
    (0x0d, "ifnle", &[Operand::Offset], 0),
    (0x0e, "ifngt", &[Operand::Offset], 0),
    (0x0f, "ifnge", &[Operand::Offset], 0),
    (0x10, "jump", &[Operand::Offset], 0),
    (0x11, "iftrue", &[Operand::Offset], 0),
    (0x12, "iffalse", &[Operand::Offset], 0),
    (0x13, "ifeq", &[Operand::Offset], 0),
    (0x14, "ifne", &[Operand::Offset], 0),
    (0x15, "iflt", &[Operand::Offset], 0),
    (0x16, "ifle", &[Operand::Offset], 0),
    (0x17, "ifgt", &[Operand::Offset], 0),
    (0x18, "ifge", &[Operand::Offset], 0),
    (0x19, "ifstricteq", &[Operand::Offset], 0),
    (0x1a, "ifstrictne", &[Operand::Offset], 0),
    (0x1b, "lookupswitch", &[Operand::CaseBaseoffset, Operand::OptCaseOffsets], 0),
    (0x1c, "pushwith", &[], 0),
    (0x1d, "popscope", &[], 0),
    (0x1e, "nextname", &[], 0),
    (0x1f, "hasnext", &[], 0),
    (0x20, "pushnull", &[], 0),
    (0x21, "pushundefined", &[], 0),
    (0x22, "pushconstant", &[Operand::OptU30], NO_FP),
    (0x23, "nextvalue", &[], 0),
    (0x24, "pushbyte", &[Operand::OptS8], 0),
    (0x25, "pushshort", &[Operand::OptS16], 0),
    (0x26, "pushtrue", &[], 0),
    (0x27, "pushfalse", &[], 0),
    (0x28, "pushnan", &[], 0),
    (0x29, "pop", &[], 0),
    (0x2a, "dup", &[], 0),
    (0x2b, "swap", &[], 0),
    (0x2c, "pushstring", &[Operand::StringIndex], 0),
    (0x2d, "pushint", &[Operand::IntIndex], 0),
    (0x2e, "pushuint", &[Operand::UintIndex], 0),
    (0x2f, "pushdouble", &[Operand::DoubleIndex], 0),
    (0x30, "pushscope", &[], 0),
    (0x31, "pushnamespace", &[Operand::NamespaceIndex], 0),
    (0x32, "hasnext2", &[Operand::LocalRegIndex, Operand::LocalRegIndex], 0),
    (0x33, "pushdecimal", &[Operand::DecimalIndex], NO_FP),
    (0x34, "pushdnan", &[], NO_FP),
    (0x35, "li8", &[], DOMAIN_MEMORY),
    (0x36, "li16", &[], DOMAIN_MEMORY),
    (0x37, "li32", &[], DOMAIN_MEMORY),
    (0x38, "lf32", &[], DOMAIN_MEMORY),
    (0x39, "lf64", &[], DOMAIN_MEMORY),
    (0x3a, "si8", &[], DOMAIN_MEMORY),
    (0x3b, "si16", &[], DOMAIN_MEMORY),
    (0x3c, "si32", &[], DOMAIN_MEMORY),
    (0x3d, "sf32", &[], DOMAIN_MEMORY),
    (0x3e, "sf64", &[], DOMAIN_MEMORY),
    (0x40, "newfunction", &[Operand::MethodIndex], 0),
    (0x41, "call", &[Operand::ArgCount], 0),
    (0x42, "construct", &[Operand::ArgCount], 0),
    (0x43, "callmethod", &[Operand::DispatchId, Operand::ArgCount], 0),
    (0x44, "callstatic", &[Operand::MethodIndex, Operand::ArgCount], 0),
    (0x45, "callsuper", &[Operand::MultinameIndex, Operand::ArgCount], 0),
    (0x46, "callproperty", &[Operand::MultinameIndex, Operand::ArgCount], 0),
    (0x47, "returnvoid", &[], 0),
    (0x48, "returnvalue", &[], 0),
    (0x49, "constructsuper", &[Operand::ArgCount], 0),
    (0x4a, "constructprop", &[Operand::MultinameIndex, Operand::ArgCount], 0),
    (0x4b, "callsuperid", &[], NO_FP | UNKNOWN_OPS),
    (0x4c, "callproplex", &[Operand::MultinameIndex, Operand::ArgCount], 0),
    (0x4d, "callinterface", &[Operand::ClassIndex], NO_FP),
    (0x4e, "callsupervoid", &[Operand::MultinameIndex, Operand::ArgCount], 0),
    (0x4f, "callpropvoid", &[Operand::MultinameIndex, Operand::ArgCount], 0),
    (0x50, "sxi1", &[], DOMAIN_MEMORY),
    (0x51, "sxi8", &[], DOMAIN_MEMORY),
    (0x52, "sxi16", &[], DOMAIN_MEMORY),
    (0x53, "applytype", &[Operand::ArgCount], 0),
    (0x54, "pushfloat4", &[Operand::Float4Index], NO_FP),
    (0x55, "newobject", &[Operand::ArgCount], 0),
    (0x56, "newarray", &[Operand::ArgCount], 0),
    (0x57, "newactivation", &[], 0),
    (0x58, "newclass", &[Operand::ClassIndex], 0),
    (0x59, "getdescendants", &[Operand::MultinameIndex], 0),
    (0x5a, "newcatch", &[Operand::ExceptionIndex], 0),
    (0x5b, "deldescendants", &[Operand::MultinameIndex], NO_FP),
    (0x5c, "findpropglobal", &[Operand::MultinameIndex], NO_FP),
    (0x5d, "findpropstrict", &[Operand::MultinameIndex], 0),
    (0x5e, "findproperty", &[Operand::MultinameIndex], 0),
    (0x5f, "finddef", &[Operand::MultinameIndex], 0),
    (0x60, "getlex", &[Operand::MultinameIndex], 0),
    (0x61, "setproperty", &[Operand::MultinameIndex], 0),
    (0x62, "getlocal", &[Operand::LocalRegIndex], 0),
    (0x63, "setlocal", &[Operand::LocalRegIndex], 0),
    (0x64, "getglobalscope", &[], 0),
    (0x65, "getscopeobject", &[Operand::ScopeIndex], 0),
    (0x66, "getproperty", &[Operand::MultinameIndex], 0),
    (0x67, "getouterscope", &[Operand::ScopeIndex], 0),
    (0x68, "initproperty", &[Operand::MultinameIndex], 0),
    (0x69, "setpropertylate", &[], NO_FP | UNKNOWN_OPS),
    (0x6a, "deleteproperty", &[Operand::MultinameIndex], 0),
    (0x6b, "deletepropertylate", &[], NO_FP),
    (0x6c, "getslot", &[Operand::SlotIndex], 0),
    (0x6d, "setslot", &[Operand::SlotIndex], 0),
    (0x6e, "getglobalslot", &[Operand::SlotIndex], 0),
    (0x6f, "setglobalslot", &[Operand::SlotIndex], 0),
    (0x70, "convert_s", &[], 0),
    (0x71, "esc_xelem", &[], 0),
    (0x72, "esc_xattr", &[], 0),
    (0x73, "convert_i", &[], 0),
    (0x74, "convert_u", &[], 0),
    (0x75, "convert_d", &[], 0),
    (0x76, "convert_b", &[], 0),
    (0x77, "convert_o", &[], 0),
    (0x78, "checkfilter", &[], 0),
    (0x79, "convert_f", &[], NO_FP),
    (0x7a, "unplus", &[], NO_FP),
    (0x7b, "convert_f4", &[], NO_FP),
    (0x80, "coerce", &[Operand::MultinameIndex], 0),
    (0x81, "coerce_b", &[], DEPRECATED),
    (0x82, "coerce_a", &[], 0),
    (0x83, "coerce_i", &[], DEPRECATED),
    (0x84, "coerce_d", &[], DEPRECATED),
    (0x85, "coerce_s", &[], 0),
    (0x86, "astype", &[Operand::MultinameIndex], 0),
    (0x87, "astypelate", &[], 0),
    (0x88, "coerce_u", &[], DEPRECATED),
    (0x89, "coerce_o", &[], 0),
    (0x8f, "negate_p", &[Operand::NumberContext], NO_FP),
    (0x90, "negate", &[], 0),
    (0x91, "increment", &[], 0),
    (0x92, "inclocal", &[Operand::LocalRegIndex], 0),
    (0x93, "decrement", &[], 0),
    (0x94, "declocal", &[Operand::LocalRegIndex], 0),
    (0x95, "typeof", &[], 0),
    (0x96, "not", &[], 0),
    (0x97, "bitnot", &[], 0),
    (0x9a, "concat", &[], NO_FP | UNKNOWN_OPS),
    (0x9b, "add_d", &[], NO_FP | UNKNOWN_OPS),
    (0x9c, "increment_p", &[Operand::NumberContext], NO_FP),
    (0x9d, "inclocal_p", &[Operand::NumberContext, Operand::LocalRegIndex], NO_FP),
    (0x9e, "decrement_p", &[Operand::NumberContext], NO_FP),
    (0x9f, "declocal_p", &[Operand::NumberContext, Operand::LocalRegIndex], NO_FP),
    (0xa0, "add", &[], 0),
    (0xa1, "subtract", &[], 0),
    (0xa2, "multiply", &[], 0),
    (0xa3, "divide", &[], 0),
    (0xa4, "modulo", &[], 0),
    (0xa5, "lshift", &[], 0),
    (0xa6, "rshift", &[], 0),
    (0xa7, "urshift", &[], 0),
    (0xa8, "bitand", &[], 0),
    (0xa9, "bitor", &[], 0),
    (0xaa, "bitxor", &[], 0),
    (0xab, "equals", &[], 0),
    (0xac, "strictequals", &[], 0),
    (0xad, "lessthan", &[], 0),
    (0xae, "lessequals", &[], 0),
    (0xaf, "greaterthan", &[], 0),
    (0xb0, "greaterequals", &[], 0),
    (0xb1, "instanceof", &[], 0),
    (0xb2, "istype", &[Operand::MultinameIndex], 0),
    (0xb3, "istypelate", &[], 0),
    (0xb4, "in", &[], 0),
    (0xb5, "add_p", &[Operand::NumberContext], NO_FP),
    (0xb6, "subtract_p", &[Operand::NumberContext], NO_FP),
    (0xb7, "multiply_p", &[Operand::NumberContext], NO_FP),
    (0xb8, "divide_p", &[Operand::NumberContext], NO_FP),
    (0xb9, "modulo_p", &[Operand::NumberContext], NO_FP),
    (0xc0, "increment_i", &[], 0),
    (0xc1, "decrement_i", &[], 0),
    (0xc2, "inclocal_i", &[Operand::LocalRegIndex], 0),
    (0xc3, "declocal_i", &[Operand::LocalRegIndex], 0),
    (0xc4, "negate_i", &[], 0),
    (0xc5, "add_i", &[], 0),
    (0xc6, "subtract_i", &[], 0),
    (0xc7, "multiply_i", &[], 0),
    (0xd0, "getlocal0", &[], 0),
    (0xd1, "getlocal1", &[], 0),
    (0xd2, "getlocal2", &[], 0),
    (0xd3, "getlocal3", &[], 0),
    (0xd4, "setlocal0", &[], 0),
    (0xd5, "setlocal1", &[], 0),
    (0xd6, "setlocal2", &[], 0),
    (0xd7, "setlocal3", &[], 0),
    (0xed, "invalid", &[], NO_FP | UNKNOWN_OPS),
    (0xee, "abs_jump", &[], NO_FP | UNKNOWN_OPS),
    (0xef, "debug", &[Operand::DebugType, Operand::StringIndex, Operand::RegisterIndex, Operand::OptU30], 0),
    (0xf0, "debugline", &[Operand::Linenum], 0),
    (0xf1, "debugfile", &[Operand::StringIndex], 0),
    (0xf2, "bkptline", &[Operand::Linenum], 0),
    (0xf3, "timestamp", &[], 0),
    (0xf5, "verifypass", &[], NO_FP | UNKNOWN_OPS),
    (0xf6, "alloc", &[], NO_FP | UNKNOWN_OPS),
    (0xf7, "mark", &[], NO_FP | UNKNOWN_OPS),
    (0xf8, "wb", &[], NO_FP | UNKNOWN_OPS),
    (0xf9, "prologue", &[], NO_FP | UNKNOWN_OPS),
    (0xfa, "sendenter", &[], NO_FP | UNKNOWN_OPS),
    (0xfb, "doubletoatom", &[], NO_FP | UNKNOWN_OPS),
    (0xfc, "sweep", &[], NO_FP | UNKNOWN_OPS),
    (0xfd, "codegenop", &[], NO_FP | UNKNOWN_OPS),
    (0xfe, "verifyop", &[], NO_FP | UNKNOWN_OPS),
    (0xff, "decode", &[], NO_FP | UNKNOWN_OPS),
];

pub struct OpDef {
    pub code: u8,
    pub name: &'static str,
    pub operands: &'static [Operand],
    pub flags: u8,
}

/// Which optional instruction families an ABC version enables.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Dialect {
    pub float: bool,
    pub decimal: bool,
}

impl Dialect {
    pub fn of(major: u16, minor: u16) -> Dialect {
        Dialect { float: (major, minor) >= (47, 16), decimal: minor >= 17 }
    }
}

/// Opcodes whose meaning depends on the ABC version.
const FLOAT_OPS: &[(u8, &str, &[Operand], u8)] = &[
    (0x22, "pushfloat", &[Operand::FloatIndex], NO_FP),
    (0x79, "convert_f", &[], NO_FP),
    (0x7a, "unplus", &[], NO_FP),
];
const DECIMAL_OPS: &[(u8, &str, &[Operand], u8)] = &[
    (0x79, "convert_m", &[], NO_FP),
    (0x7a, "convert_m_p", &[Operand::NumberContext], NO_FP),
];

pub fn def(code: u8) -> Option<OpDef> {
    def_in(code, Dialect::default())
}

pub fn def_in(code: u8, dialect: Dialect) -> Option<OpDef> {
    let pick = |t: &[(u8, &'static str, &'static [Operand], u8)]| t.iter().find(|o| o.0 == code).copied();
    let found = if dialect.float { pick(FLOAT_OPS) } else { None }
        .or_else(|| if dialect.decimal { pick(DECIMAL_OPS) } else { None })
        .or_else(|| OPS.binary_search_by_key(&code, |o| o.0).ok().map(|i| OPS[i]));
    found.map(|(code, name, operands, flags)| OpDef { code, name, operands, flags })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Instruction {
    pub offset: usize,
    pub len: usize,
    pub code: u8,
    pub name: &'static str,
    pub kinds: &'static [Operand],
    /// Operand values in definition order; lookupswitch has base, case count, then offsets.
    pub operands: Vec<i64>,
}

impl Instruction {

    /// Absolute jump targets: branch offsets are relative to the end of the instruction,
    /// lookupswitch offsets to its start.
    pub fn targets(&self) -> Vec<usize> {
        let mut out = Vec::new();
        for (i, op) in self.kinds.iter().enumerate() {
            match op {
                Operand::Offset => out.push((self.offset as i64 + self.len as i64 + self.operands[i]) as usize),
                Operand::CaseBaseoffset => out.push((self.offset as i64 + self.operands[i]) as usize),
                Operand::OptCaseOffsets => {
                    for v in &self.operands[i + 1..] {
                        out.push((self.offset as i64 + v) as usize);
                    }
                }
                _ => {}
            }
        }
        out
    }
}

fn s24(r: &mut Reader) -> Result<i64> {
    let b = r.bytes(3)?;
    let v = b[0] as i32 | (b[1] as i32) << 8 | (b[2] as i32) << 16;
    Ok(((v << 8) >> 8) as i64)
}

/// Decodes a method body. An unknown opcode ends decoding with an error naming its offset.
pub fn decode(code: &[u8], dialect: Dialect) -> Result<Vec<Instruction>> {
    let mut r = Reader::new(code);
    let mut out = Vec::new();
    while !r.is_empty() {
        let offset = r.pos();
        let op = r.u8()?;
        let d = def_in(op, dialect).ok_or_else(|| Error::Other(format!("unknown AVM2 opcode 0x{op:02x} at 0x{offset:04x}")))?;
        let mut operands = Vec::with_capacity(d.operands.len());
        for o in d.operands {
            match o {
                Operand::Offset | Operand::CaseBaseoffset => operands.push(s24(&mut r)?),
                Operand::OptCaseOffsets => {
                    let n = read_u30(&mut r)?;
                    operands.push(n as i64);
                    for _ in 0..=n {
                        operands.push(s24(&mut r)?);
                    }
                }
                Operand::OptS8 => operands.push(r.i8()? as i64),
                Operand::OptS16 => operands.push(read_u30(&mut r)? as u16 as i16 as i64),
                Operand::DebugType | Operand::RegisterIndex => operands.push(r.u8()? as i64),
                _ => operands.push(read_u30(&mut r)? as i64),
            }
        }
        out.push(Instruction { offset, len: r.pos() - offset, code: op, name: d.name, kinds: d.operands, operands });
    }
    Ok(out)
}

pub const NOP: u8 = 0x02;
pub const JUMP: u8 = 0x10;
pub const LOOKUPSWITCH: u8 = 0x1b;

fn is_exit(op: u8) -> bool {
    matches!(op, 0x03 | 0x47 | 0x48)
}

fn is_branch(op: u8) -> bool {
    matches!(op, 0x0c..=0x1a) && op != JUMP
}

/// Disassembles: follows reachable code from the entry and exception handlers,
/// then decodes unreachable regions without following their jumps, rejects overlapping
/// or oversized lookupswitches, and leaves every byte nothing covers as a one-byte `nop`.
pub fn disassemble(code: &[u8], handlers: &[usize], dialect: Dialect) -> Vec<Instruction> {
    let n = code.len();
    let mut owner: Vec<Option<usize>> = vec![None; n];
    let mut found: Vec<Instruction> = Vec::new();
    let mut addresses: std::collections::VecDeque<usize> = std::collections::VecDeque::from(vec![0]);
    addresses.extend(handlers.iter().copied());
    let mut switches: std::collections::VecDeque<usize> = Default::default();
    let mut unreached: std::collections::VecDeque<usize> = Default::default();

    loop {
        let (mut pos, mut is_switch, follow) = if let Some(a) = addresses.pop_front() {
            (a, false, true)
        } else if let Some(a) = switches.pop_front() {
            (a, true, true)
        } else if let Some(a) = unreached.pop_front() {
            (a, false, false)
        } else {
            break;
        };
        'path: while pos < n {
            if owner[pos].is_some() {
                break;
            }
            let start = pos;
            let op = code[pos];
            let Some(d) = def_in(op, dialect) else { break };
            if op == LOOKUPSWITCH {
                if !is_switch {
                    switches.push_back(start);
                    break;
                }
                is_switch = false;
            }
            let mut r = Reader::new(code);
            let _ = r.skip(start + 1);
            let mut operands = Vec::with_capacity(d.operands.len());
            let ok = (|| -> Result<()> {
                for o in d.operands {
                    match o {
                        Operand::Offset | Operand::CaseBaseoffset => operands.push(s24(&mut r)?),
                        Operand::OptCaseOffsets => {
                            let count = read_u30(&mut r)?;
                            let end = r.pos() + 3 * (count + 1);
                            if end > n || (start..end).any(|a| owner[a].is_some()) {
                                return Err(Error::Other("invalid lookupswitch".into()));
                            }
                            operands.push(count as i64);
                            for _ in 0..=count {
                                operands.push(s24(&mut r)?);
                            }
                        }
                        Operand::OptS8 => operands.push(r.i8()? as i64),
                        Operand::OptS16 => operands.push(read_u30(&mut r)? as u16 as i16 as i64),
                        Operand::DebugType | Operand::RegisterIndex => operands.push(r.u8()? as i64),
                        _ => operands.push(read_u30(&mut r)? as i64),
                    }
                }
                Ok(())
            })();
            if ok.is_err() {
                break;
            }
            let end = r.pos();
            if (start..end).any(|a| owner[a].is_some()) {
                break;
            }
            let mut ins = Instruction { offset: start, len: end - start, code: op, name: d.name, kinds: d.operands, operands };
            let mut stop = false;
            if is_branch(op) {
                if follow {
                    addresses.push_back((end as i64 + ins.operands[0]) as usize);
                } else {
                    ins.operands[0] = 0;
                }
            } else if op == JUMP {
                if follow {
                    addresses.push_back((end as i64 + ins.operands[0]) as usize);
                    unreached.push_back(end);
                    stop = true;
                } else {
                    ins.operands[0] = 0;
                }
            } else if is_exit(op) && follow {
                unreached.push_back(end);
                stop = true;
            } else if op == LOOKUPSWITCH {
                if follow {
                    addresses.push_back((start as i64 + ins.operands[0]) as usize);
                    for v in &ins.operands[2..] {
                        addresses.push_back((start as i64 + v) as usize);
                    }
                    unreached.push_back(end);
                    stop = true;
                } else {
                    let len = (end - start) as i64;
                    ins.operands[0] = len;
                    for v in ins.operands[2..].iter_mut() {
                        *v = len;
                    }
                }
            }
            let idx = found.len();
            for slot in &mut owner[start..end] {
                *slot = Some(idx);
            }
            found.push(ins);
            if stop {
                break 'path;
            }
            pos = end;
        }
    }

    let mut out = Vec::with_capacity(found.len());
    let mut i = 0;
    while i < n {
        match owner[i] {
            Some(idx) => {
                let ins = found[idx].clone();
                i += ins.len;
                out.push(ins);
            }
            None => {
                out.push(Instruction { offset: i, len: 1, code: NOP, name: "nop", kinds: &[], operands: Vec::new() });
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreachable_bytes_become_nops() {
        // returnvoid, then an undefined opcode: the junk byte stays a nop.
        let ins = disassemble(&[0x47, 0x7c], &[], Dialect::default());
        assert_eq!(ins.iter().map(|i| i.code).collect::<Vec<_>>(), vec![0x47, NOP]);
    }

    #[test]
    fn table_is_sorted() {
        assert!(OPS.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(def(0x24).unwrap().name, "pushbyte");
        assert_eq!(def(0x4c).unwrap().name, "callproplex");
        assert_eq!(def_in(0x22, Dialect { float: true, decimal: false }).unwrap().name, "pushfloat");
        assert_eq!(def_in(0x79, Dialect { float: false, decimal: true }).unwrap().name, "convert_m");
    }

    #[test]
    fn decodes_branches_and_switch() {
        // pushbyte -1; jump +0; lookupswitch base 0 with one case.
        let code = [0x24, 0xff, 0x10, 0, 0, 0, 0x1b, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        let ins = decode(&code, Dialect::default()).unwrap();
        assert_eq!(ins.len(), 3);
        assert_eq!(ins[0].operands, vec![-1]);
        assert_eq!(ins[1].targets(), vec![6]);
        assert_eq!(ins[2].targets(), vec![6, 6, 6]);
    }
}
