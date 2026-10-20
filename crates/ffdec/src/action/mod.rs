//! ActionScript 1/2 bytecode (SWF actions), read like FFDec's `ActionListReader`.

pub mod asm;
pub mod decomp;
pub mod edit;
pub mod pcode;
pub mod scripts;
pub mod write;

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

/// Text encoding of SWF strings: UTF-8 from SWF 6, FFDec's default ANSI charset before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charset {
    Utf8,
    Windows1252,
}

impl Charset {
    pub fn for_version(version: u8) -> Self {
        if version >= 6 { Charset::Utf8 } else { Charset::Windows1252 }
    }

    pub fn decode(self, b: &[u8]) -> String {
        match self {
            Charset::Utf8 => utf8_decode(b),
            Charset::Windows1252 => b.iter().map(|&c| cp1252(c)).collect(),
        }
    }

    pub fn byte_len(self, s: &str) -> usize {
        match self {
            Charset::Utf8 => utf8_bytes(s).len(),
            Charset::Windows1252 => s.encode_utf16().count(),
        }
    }
}

const CP1252_HIGH: [u16; 32] = [
    0x20ac, 0xfffd, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160, 0x2039, 0x0152, 0xfffd, 0x017d, 0xfffd,
    0xfffd, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0x02dc, 0x2122, 0x0161, 0x203a, 0x0153, 0xfffd, 0x017e, 0x0178,
];

fn cp1252(c: u8) -> char {
    if (0x80..0xa0).contains(&c) {
        char::from_u32(CP1252_HIGH[(c - 0x80) as usize] as u32).unwrap_or('\u{fffd}')
    } else {
        c as char
    }
}

/// FFDec's `Utf8Helper.decode`: invalid bytes become `{invalid_utf8=N}`.
pub fn utf8_decode(data: &[u8]) -> String {
    let esc = |v: u8, out: &mut Vec<u8>| out.extend_from_slice(format!("{{invalid_utf8={v}}}").as_bytes());
    let mut out = Vec::with_capacity(data.len());
    let len = data.len();
    let mut i = 0;
    while i < len {
        let v = data[i];
        let mut next = 0;
        if v & 0x80 == 0 {
            out.push(v);
        } else if v & 0xc0 == 0x80 {
            esc(v, &mut out);
        } else if v & 0xe0 == 0xc0 {
            next = 1;
        } else if v & 0xf0 == 0xe0 {
            next = 2;
        } else if v & 0xf8 == 0xf0 {
            next = 3;
        } else {
            esc(v, &mut out);
        }
        if next > 0 {
            if i + next >= len {
                esc(v, &mut out);
                i += 1;
                continue;
            }
            let mut valid = true;
            for j in 0..next {
                let v2 = data[i + 1 + j];
                if v2 & 0xc0 != 0x80 {
                    if v2 & 0x80 == 0 {
                        next = j;
                    }
                    valid = false;
                    break;
                }
            }
            if valid {
                out.extend_from_slice(&data[i..=i + next]);
            } else {
                esc(v, &mut out);
                for j in 0..next {
                    esc(data[i + 1 + j], &mut out);
                }
            }
            i += next;
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// FFDec's `Utf8Helper.getBytes`: `{invalid_utf8=N}` stands for the raw byte N.
pub fn utf8_bytes(s: &str) -> Vec<u8> {
    if !s.contains("invalid_utf8") {
        return s.as_bytes().to_vec();
    }
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        if c == '{' {
            let tail = &rest[1..];
            let body = tail.strip_prefix("invalid_utf8=").or_else(|| tail.strip_prefix("invalid_utf8:"));
            if let Some(body) = body {
                let digits = body.bytes().take_while(u8::is_ascii_digit).count();
                if digits > 0 && body[digits..].starts_with('}') {
                    let v: u64 = body[..digits].parse().unwrap_or(0);
                    out.push(v as u8);
                    rest = &body[digits + 1..];
                    continue;
                }
            }
        }
        let mut buf = [0; 4];
        out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        rest = &rest[c.len_utf8()..];
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Float(f32),
    Null,
    Undefined,
    Register(u8),
    Bool(bool),
    Double(f64),
    Int(i64),
    Constant(u16),
}

#[derive(Debug, Clone)]
pub enum Op {
    Simple,
    End,
    GotoFrame(u16),
    GetUrl { url: String, target: String },
    WaitForFrame { frame: u16, skip: usize },
    SetTarget(String),
    GoToLabel(String),
    Push { values: Vec<Value>, pool: Option<Arc<Vec<String>>> },
    Jump { offset: i32, deobfuscate: bool },
    If { offset: i32 },
    GetUrl2 { load_variables: bool, load_target: bool, method: u8 },
    GotoFrame2 { scene_bias_flag: bool, play: bool, scene_bias: u16 },
    WaitForFrame2 { skip: usize },
    ConstantPool(Vec<String>),
    DefineFunction { name: String, params: Vec<String>, code_size: u32 },
    With { code_size: u32 },
    StoreRegister(u8),
    DefineFunction2(Box<DefineFunction2>),
    Try(Box<Try>),
    StrictMode(u8),
    Unknown(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct DefineFunction2 {
    pub name: String,
    pub params: Vec<(u8, String)>,
    pub register_count: u8,
    pub preload_parent: bool,
    pub preload_root: bool,
    pub suppress_super: bool,
    pub preload_super: bool,
    pub suppress_arguments: bool,
    pub preload_arguments: bool,
    pub suppress_this: bool,
    pub preload_this: bool,
    pub preload_global: bool,
    pub code_size: u32,
}

#[derive(Debug, Clone)]
pub struct Try {
    pub catch_in_register: bool,
    pub finally_flag: bool,
    pub catch_flag: bool,
    pub catch_name: Option<String>,
    pub catch_register: u8,
    pub try_size: u32,
    pub catch_size: u32,
    pub finally_size: u32,
}

#[derive(Debug, Clone)]
pub struct Action {
    pub code: u8,
    pub length: usize,
    pub address: u64,
    pub file_offset: i64,
    pub ignored: bool,
    pub op: Op,
}

/// FFDec's p-code name of an action without operands, by action code.
pub fn simple_name(code: u8) -> Option<&'static str> {
    Some(match code {
        0x04 => "NextFrame",
        0x05 => "PrevFrame",
        0x06 => "Play",
        0x07 => "Stop",
        0x08 => "ToggleQuality",
        0x09 => "StopSounds",
        0x0a => "Add",
        0x0b => "Subtract",
        0x0c => "Multiply",
        0x0d => "Divide",
        0x0e => "Equals",
        0x0f => "Less",
        0x10 => "And",
        0x11 => "Or",
        0x12 => "Not",
        0x13 => "StringEquals",
        0x14 => "StringLength",
        0x15 => "StringExtract",
        0x17 => "Pop",
        0x18 => "ToInteger",
        0x1c => "GetVariable",
        0x1d => "SetVariable",
        0x20 => "SetTarget2",
        0x21 => "StringAdd",
        0x22 => "GetProperty",
        0x23 => "SetProperty",
        0x24 => "CloneSprite",
        0x25 => "RemoveSprite",
        0x26 => "Trace",
        0x27 => "StartDrag",
        0x28 => "EndDrag",
        0x29 => "StringLess",
        0x2a => "Throw",
        0x2b => "CastOp",
        0x2c => "ImplementsOp",
        0x2d => "FSCommand2",
        0x30 => "RandomNumber",
        0x31 => "MBStringLength",
        0x32 => "CharToAscii",
        0x33 => "AsciiToChar",
        0x34 => "GetTime",
        0x35 => "MBStringExtract",
        0x36 => "MBCharToAscii",
        0x37 => "MBAsciiToChar",
        0x3a => "Delete",
        0x3b => "Delete2",
        0x3c => "DefineLocal",
        0x3d => "CallFunction",
        0x3e => "Return",
        0x3f => "Modulo",
        0x40 => "NewObject",
        0x41 => "DefineLocal2",
        0x42 => "InitArray",
        0x43 => "InitObject",
        0x44 => "TypeOf",
        0x45 => "TargetPath",
        0x46 => "Enumerate",
        0x47 => "Add2",
        0x48 => "Less2",
        0x49 => "Equals2",
        0x4a => "ToNumber",
        0x4b => "ToString",
        0x4c => "PushDuplicate",
        0x4d => "StackSwap",
        0x4e => "GetMember",
        0x4f => "SetMember",
        0x50 => "Increment",
        0x51 => "Decrement",
        0x52 => "CallMethod",
        0x53 => "NewMethod",
        0x54 => "InstanceOf",
        0x55 => "Enumerate2",
        0x60 => "BitAnd",
        0x61 => "BitOr",
        0x62 => "BitXor",
        0x63 => "BitLShift",
        0x64 => "BitRShift",
        0x65 => "BitURShift",
        0x66 => "StrictEquals",
        0x67 => "Greater",
        0x68 => "StringGreater",
        0x69 => "Extends",
        0x9e => "Call",
        _ => return None,
    })
}

/// A byte reader over the whole action buffer that, like FFDec's stream, may read past an
/// action's declared length.
struct In<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
    charset: Charset,
}

impl<'a> In<'a> {
    fn u8(&mut self) -> Option<u8> {
        self.bit = 0;
        let v = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(v)
    }

    fn u16(&mut self) -> Option<u16> {
        Some(self.u8()? as u16 | (self.u8()? as u16) << 8)
    }

    fn i16(&mut self) -> Option<i16> {
        self.u16().map(|v| v as i16)
    }

    fn i32(&mut self) -> Option<i32> {
        let b = self.bytes(4)?;
        Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        self.bit = 0;
        let b = self.data.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(b)
    }

    fn ub(&mut self, n: u32) -> Option<u32> {
        let mut v = 0;
        for _ in 0..n {
            if self.bit == 0 {
                self.data.get(self.pos)?;
            }
            let byte = self.data[self.pos];
            let b = (byte >> (7 - self.bit)) & 1;
            v = (v << 1) | b as u32;
            self.bit += 1;
            if self.bit == 8 {
                self.bit = 0;
                self.pos += 1;
            }
        }
        Some(v)
    }

    fn flag(&mut self) -> Option<bool> {
        self.ub(1).map(|v| v == 1)
    }

    fn string(&mut self) -> Option<String> {
        self.bit = 0;
        let start = self.pos;
        let end = start + self.data.get(start..)?.iter().position(|&b| b == 0)?;
        self.pos = end + 1;
        Some(self.charset.decode(&self.data[start..end]))
    }

    fn f32(&mut self) -> Option<f32> {
        let b = self.bytes(4)?;
        Some(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// SWF DOUBLE: two little-endian 32-bit halves, high word first.
    fn f64(&mut self) -> Option<f64> {
        let b = self.bytes(8)?;
        Some(f64::from_le_bytes([b[4], b[5], b[6], b[7], b[0], b[1], b[2], b[3]]))
    }
}

fn read_push_value(r: &mut In) -> Option<Option<Value>> {
    Some(Some(match r.u8()? {
        0 => Value::Str(r.string()?),
        1 => Value::Float(r.f32()?),
        2 => Value::Null,
        3 => Value::Undefined,
        4 => Value::Register(r.u8()?),
        5 => Value::Bool(r.u8()? != 0),
        6 => Value::Double(r.f64()?),
        7 => Value::Int(r.i32()? as i64),
        8 => Value::Constant(r.u8()? as u16),
        9 => Value::Constant(r.u16()?),
        _ => return Some(None),
    }))
}

/// FFDec's `SWFInputStream.readAction`; `None` at the end of the data.
fn read_action(data: &[u8], pos: usize, charset: Charset) -> Option<Action> {
    let mut r = In { data, pos, bit: 0, charset };
    let code = r.u8()?;
    if code == 0 {
        return Some(Action { code: 0, length: 0, address: 0, file_offset: -1, ignored: true, op: Op::End });
    }
    let length = if code >= 0x80 { r.u16()? as usize } else { 0 };
    let op = match code {
        0x81 => Op::GotoFrame(r.u16()?),
        0x83 => Op::GetUrl { url: r.string()?, target: r.string()? },
        0x8a => {
            let frame = r.u16()?;
            Op::WaitForFrame { frame, skip: r.u8()? as usize }
        }
        0x8b => Op::SetTarget(r.string()?),
        0x8c => Op::GoToLabel(r.string()?),
        0x89 => Op::StrictMode(r.u8()?),
        0x96 => {
            let end = (r.pos + length).min(data.len());
            let mut lim = In { data: &data[..end], pos: r.pos, bit: 0, charset };
            let mut values = Vec::new();
            while lim.pos < end {
                match read_push_value(&mut lim) {
                    Some(Some(v)) => values.push(v),
                    Some(None) => {}
                    None => break,
                }
            }
            Op::Push { values, pool: None }
        }
        0x99 => Op::Jump { offset: r.i16()? as i32, deobfuscate: false },
        0x9d => Op::If { offset: r.i16()? as i32 },
        0x9a => {
            let load_variables = r.flag()?;
            let load_target = r.flag()?;
            r.ub(4)?;
            Op::GetUrl2 { load_variables, load_target, method: r.ub(2)? as u8 }
        }
        0x9f => {
            r.ub(6)?;
            let scene_bias_flag = r.flag()?;
            let play = r.flag()?;
            let scene_bias = if scene_bias_flag { r.u16()? } else { 0 };
            Op::GotoFrame2 { scene_bias_flag, play, scene_bias }
        }
        0x8d => Op::WaitForFrame2 { skip: r.u8()? as usize },
        0x88 => {
            let n = r.u16()?;
            let mut pool = Vec::with_capacity(n as usize);
            for _ in 0..n {
                pool.push(r.string()?);
            }
            Op::ConstantPool(pool)
        }
        0x9b => {
            let name = r.string()?;
            let n = r.u16()?;
            let mut params = Vec::new();
            for _ in 0..n {
                params.push(r.string()?);
            }
            Op::DefineFunction { name, params, code_size: r.u16()? as u32 }
        }
        0x94 => Op::With { code_size: r.u16()? as u32 },
        0x87 => Op::StoreRegister(r.u8()?),
        0x8e => {
            let name = r.string()?;
            let n = r.u16()?;
            let register_count = r.u8()?;
            let preload_parent = r.flag()?;
            let preload_root = r.flag()?;
            let suppress_super = r.flag()?;
            let preload_super = r.flag()?;
            let suppress_arguments = r.flag()?;
            let preload_arguments = r.flag()?;
            let suppress_this = r.flag()?;
            let preload_this = r.flag()?;
            r.ub(7)?;
            let preload_global = r.flag()?;
            let mut params = Vec::new();
            for _ in 0..n {
                let reg = r.u8()?;
                params.push((reg, r.string()?));
            }
            let code_size = r.u16()? as u32;
            Op::DefineFunction2(Box::new(DefineFunction2 {
                name,
                params,
                register_count,
                preload_parent,
                preload_root,
                suppress_super,
                preload_super,
                suppress_arguments,
                preload_arguments,
                suppress_this,
                preload_this,
                preload_global,
                code_size,
            }))
        }
        0x8f => {
            r.ub(5)?;
            let catch_in_register = r.flag()?;
            let finally_flag = r.flag()?;
            let catch_flag = r.flag()?;
            let try_size = r.u16()? as u32;
            let catch_size = r.u16()? as u32;
            let finally_size = r.u16()? as u32;
            let (catch_name, catch_register) = if catch_in_register { (None, r.u8()?) } else { (Some(r.string()?), 0) };
            Op::Try(Box::new(Try { catch_in_register, finally_flag, catch_flag, catch_name, catch_register, try_size, catch_size, finally_size }))
        }
        c if simple_name(c).is_some() => Op::Simple,
        _ => Op::Unknown(r.bytes(length)?.to_vec()),
    };
    Some(Action { code, length, address: 0, file_offset: -1, ignored: false, op })
}

impl Action {
    pub fn new(code: u8, op: Op) -> Self {
        let mut a = Action { code, length: 0, address: 0, file_offset: -1, ignored: matches!(op, Op::End), op };
        a.length = a.content_len(Charset::Utf8);
        a
    }

    fn deobfuscate_jump(address: u64, offset: i32) -> Self {
        Action { code: 0x99, length: 2, address, file_offset: -1, ignored: false, op: Op::Jump { offset, deobfuscate: true } }
    }

    pub fn total_len(&self) -> u64 {
        self.length as u64 + 1 + if self.code >= 0x80 { 2 } else { 0 }
    }

    /// FFDec's `getContentBytesLength`.
    pub fn content_len(&self, cs: Charset) -> usize {
        let utf8 = |s: &str| utf8_bytes(s).len();
        match &self.op {
            Op::Simple | Op::End => 0,
            Op::GotoFrame(_) | Op::Jump { .. } | Op::If { .. } | Op::With { .. } => 2,
            Op::GetUrl { url, target } => cs.byte_len(url) + cs.byte_len(target) + 2,
            Op::WaitForFrame { .. } => 3,
            Op::SetTarget(s) | Op::GoToLabel(s) => cs.byte_len(s) + 1,
            Op::Push { values, .. } => values.iter().map(|v| push_value_len(v, cs)).sum(),
            Op::GetUrl2 { .. } | Op::WaitForFrame2 { .. } | Op::StoreRegister(_) | Op::StrictMode(_) => 1,
            Op::GotoFrame2 { scene_bias_flag, .. } => if *scene_bias_flag { 3 } else { 1 },
            Op::ConstantPool(pool) => 2 + pool.iter().map(|s| cs.byte_len(s) + 1).sum::<usize>(),
            Op::DefineFunction { name, params, .. } => utf8(name) + 5 + params.iter().map(|s| utf8(s) + 1).sum::<usize>(),
            Op::DefineFunction2(f) => utf8(&f.name) + 8 + f.params.iter().map(|(_, s)| utf8(s) + 2).sum::<usize>(),
            Op::Try(t) => 8 + if t.catch_in_register { 0 } else { utf8(t.catch_name.as_deref().unwrap_or("")) },
            Op::Unknown(d) => d.len(),
        }
    }

    pub fn bytes_len(&self, cs: Charset) -> u64 {
        self.content_len(cs) as u64 + if self.code >= 0x80 { 3 } else { 1 }
    }

    fn update_length(&mut self, cs: Charset) {
        self.length = self.content_len(cs);
    }

    pub fn is_exit(&self) -> bool {
        matches!(self.op, Op::End) || (matches!(self.op, Op::Simple) && matches!(self.code, 0x3e | 0x2a))
    }

    pub fn is_jump(&self) -> bool {
        matches!(self.op, Op::Jump { .. })
    }

    /// `If`/`Jump` destination address.
    pub fn target(&self) -> Option<u64> {
        match self.op {
            Op::Jump { offset, .. } | Op::If { offset } => Some((self.address as i64 + 5 + offset as i64) as u64),
            _ => None,
        }
    }

    fn set_offset(&mut self, v: i32) {
        if let Op::Jump { offset, .. } | Op::If { offset } = &mut self.op {
            *offset = v;
        }
    }

    /// Body sizes of DefineFunction, DefineFunction2, With and Try blocks.
    pub fn container_sizes(&self) -> Option<Vec<u64>> {
        Some(match &self.op {
            Op::DefineFunction { code_size, .. } | Op::With { code_size } => vec![*code_size as u64],
            Op::DefineFunction2(f) => vec![f.code_size as u64],
            Op::Try(t) => {
                let mut v = vec![t.try_size as u64];
                if t.catch_flag {
                    v.push(t.catch_size as u64);
                }
                if t.finally_flag {
                    v.push(t.finally_size as u64);
                }
                v
            }
            _ => return None,
        })
    }

    fn set_container_size(&mut self, index: usize, size: u64) {
        let size = size as u32;
        match &mut self.op {
            Op::DefineFunction { code_size, .. } | Op::With { code_size } => *code_size = size,
            Op::DefineFunction2(f) => f.code_size = size,
            Op::Try(t) => {
                let mut pos = 0;
                if index == 0 {
                    t.try_size = size;
                }
                if t.catch_flag {
                    pos += 1;
                    if pos == index {
                        t.catch_size = size;
                    }
                }
                if t.finally_flag {
                    pos += 1;
                    if pos == index {
                        t.finally_size = size;
                    }
                }
            }
            _ => {}
        }
    }

    pub fn container_name(&self) -> Option<&'static str> {
        matches!(self.op, Op::DefineFunction { .. } | Op::DefineFunction2(_)).then_some("function")
    }

    pub fn store_size(&self) -> Option<usize> {
        match self.op {
            Op::WaitForFrame { skip, .. } | Op::WaitForFrame2 { skip } => Some(skip),
            _ => None,
        }
    }

    fn set_store_size(&mut self, n: usize) {
        if let Op::WaitForFrame { skip, .. } | Op::WaitForFrame2 { skip } = &mut self.op {
            *skip = n;
        }
    }
}

fn push_value_len(v: &Value, cs: Charset) -> usize {
    match v {
        Value::Str(s) => cs.byte_len(s) + 2,
        Value::Float(_) => 5,
        Value::Null | Value::Undefined => 1,
        Value::Register(_) | Value::Bool(_) => 2,
        Value::Int(l) if *l < -0x8000_0000 || *l > 0x7fff_ffff => 9,
        Value::Double(_) => 9,
        Value::Int(_) => 5,
        Value::Constant(i) => if *i < 256 { 2 } else { 3 },
    }
}

/// A decoded action list (FFDec's `ActionList`), addressed from 0.
#[derive(Debug, Clone)]
pub struct ActionList {
    pub actions: Vec<Action>,
    pub charset: Charset,
    pub version: u8,
}

impl ActionList {
    pub fn index_of_address(&self, addr: u64) -> Option<usize> {
        self.actions.binary_search_by_key(&addr, |a| a.address).ok()
    }
}

/// Reads the actions of an action record (`data[start..end]`), following jumps from `start`
/// like FFDec, then re-addresses them from 0 with deobfuscation jumps where code is out of order.
pub fn read_action_list(data: &[u8], start: usize, end: usize, version: u8) -> ActionList {
    let charset = Charset::for_version(version);
    let mut map: BTreeMap<u64, Action> = BTreeMap::new();
    let mut next: HashMap<u64, u64> = HashMap::new();
    let mut visited = Vec::new();
    let entry = read_at(data, charset, &mut map, &mut next, start as u64, end as u64, None, &mut visited);
    let mut list = ActionList { actions: Vec::new(), charset, version };
    if map.is_empty() {
        return list;
    }
    let last = map.values().next_back().unwrap();
    if !matches!(last.op, Op::End) {
        let addr = next[&last.address];
        let mut e = Action::new(0, Op::End);
        e.address = addr;
        map.insert(addr, e);
        next.insert(addr, addr + 1);
    }
    let first_addr = *map.keys().next().unwrap();
    list.actions = map.into_values().collect();
    fix_action_list(&mut list, Some(&next));
    if let Some(entry) = entry.filter(|&e| e != first_addr) {
        // Code that does not start at its first byte gets a leading jump to the entry point.
        let jump = Action::deobfuscate_jump(0, 0);
        let shift = jump.total_len();
        for a in &mut list.actions {
            a.address += shift;
        }
        let target = list.actions.iter().find(|a| a.file_offset == entry as i64).map_or(shift, |a| a.address);
        let mut jump = jump;
        jump.set_offset((target - shift) as i32);
        list.actions.insert(0, jump);
    }
    list
}

#[allow(clippy::too_many_arguments)]
fn read_at(
    data: &[u8],
    charset: Charset,
    map: &mut BTreeMap<u64, Action>,
    next: &mut HashMap<u64, u64>,
    ip: u64,
    end_ip: u64,
    mut pool: Option<Arc<Vec<String>>>,
    visited: &mut Vec<u64>,
) -> Option<u64> {
    let mut entry = None;
    if visited.contains(&ip) {
        return None;
    }
    visited.push(ip);
    let mut queue = VecDeque::from([ip]);
    while let Some(start) = queue.pop_front() {
        let mut ip = start;
        while end_ip > ip {
            let Some(mut a) = read_action(data, ip as usize, charset) else { break };
            a.file_offset = ip as i64;
            let mut len = a.total_len();
            if entry.is_none() {
                entry = Some(ip);
            }
            if map.contains_key(&ip) {
                break;
            }
            next.insert(ip, ip + len);
            a.address = ip;
            let exit = a.is_exit();
            let mut stop = false;
            match &mut a.op {
                Op::Push { pool: p, .. } => {
                    if pool.is_some() {
                        *p = pool.clone();
                    }
                }
                Op::ConstantPool(c) => pool = Some(Arc::new(c.clone())),
                Op::If { offset } => {
                    let n = ip as i64 + len as i64 + *offset as i64;
                    if n >= 0 {
                        queue.push_back(n as u64);
                    }
                }
                Op::Jump { offset, .. } => {
                    let n = ip as i64 + len as i64 + *offset as i64;
                    if n >= 0 {
                        queue.push_back(n as u64);
                    }
                    stop = true;
                }
                _ => {}
            }
            let sizes = a.container_sizes();
            map.insert(ip, a);
            if stop {
                break;
            }
            if let Some(sizes) = sizes {
                for size in sizes {
                    if size != 0 {
                        let ip2 = ip + len;
                        read_at(data, charset, map, next, ip2, ip2 + size, pool.clone(), visited);
                        len += size;
                    }
                }
            }
            ip += len;
            if exit {
                break;
            }
        }
    }
    entry
}

fn near_address(actions: &[Action], address: u64) -> Option<usize> {
    match actions.binary_search_by_key(&address, |a| a.address) {
        Ok(i) => Some(i),
        Err(i) => i.checked_sub(1),
    }
}

/// FFDec's `ActionListReader.fixActionList`. Jump targets, stores and container ends are kept as
/// indices into the list while addresses and lengths are recomputed.
pub fn fix_action_list(list: &mut ActionList, next: Option<&HashMap<u64, u64>>) {
    let cs = list.charset;
    let input = std::mem::take(&mut list.actions);
    let mut container_ends: Vec<(usize, Vec<Option<usize>>)> = Vec::new();
    for (i, a) in input.iter().enumerate() {
        if let Some(sizes) = a.container_sizes() {
            let mut end = a.address + a.bytes_len(cs);
            let mut lasts = Vec::new();
            for s in sizes {
                end += s;
                lasts.push(end.checked_sub(1).and_then(|e| near_address(&input, e)));
            }
            container_ends.push((i, lasts));
        }
    }
    // Ids are positions in `input`; inserted jumps get ids from input.len() upwards.
    let mut ids: Vec<usize> = Vec::new();
    let mut ret: Vec<Action> = Vec::new();
    let n = input.len();
    let mut extra = n;
    for (i, a) in input.iter().enumerate() {
        ret.push(a.clone());
        ids.push(i);
        if let (Some(next), Some(b)) = (next, input.get(i + 1)) {
            let want = next[&a.address];
            if b.address != want && !a.is_exit() && !a.is_jump() {
                let mut j = Action::deobfuscate_jump(a.address, 0);
                j.set_offset((want as i64 - a.address as i64 - j.total_len() as i64) as i32);
                ret.push(j);
                ids.push(extra);
                extra += 1;
            }
        }
    }
    // jumps: position in ret -> target position in ret
    let by_addr = |list: &[Action]| {
        let mut m: HashMap<u64, usize> = HashMap::new();
        for (i, a) in list.iter().enumerate() {
            m.entry(a.address).or_insert(i);
        }
        m
    };
    let amap = by_addr(&ret);
    let mut jumps: HashMap<usize, Option<usize>> = HashMap::new();
    for (i, a) in ret.iter().enumerate() {
        if let Some(t) = a.target() {
            jumps.insert(i, amap.get(&t).copied());
        } else if let Some(store) = a.store_size() {
            let mut cur = Some(i);
            for _ in 0..=store {
                let Some(c) = cur else { break };
                let addr = ret[c].address + ret[c].total_len();
                cur = amap.get(&addr).copied();
            }
            jumps.insert(i, cur);
        }
    }
    let pos_of_input: HashMap<usize, usize> = ids.iter().enumerate().filter(|(_, &id)| id < n).map(|(p, &id)| (id, p)).collect();
    let mut containers: Vec<(usize, Vec<Option<usize>>)> =
        container_ends.into_iter().map(|(i, lasts)| (pos_of_input[&i], lasts.into_iter().map(|l| l.map(|l| pos_of_input[&l])).collect())).collect();

    for a in ret.iter_mut() {
        a.update_length(cs);
    }
    let mut address = 0;
    let count = ret.len();
    for (i, a) in ret.iter_mut().enumerate() {
        a.address = address;
        let len = if i != count - 1 && matches!(a.op, Op::End) { 5 } else { a.total_len() };
        address += len;
    }
    let end_address = ret[count - 1].address;
    for i in 0..count {
        if i != count - 1 && matches!(ret[i].op, Op::End) {
            let addr = ret[i].address;
            let fo = ret[i].file_offset;
            let mut j = Action::deobfuscate_jump(addr, 0);
            j.set_offset((end_address as i64 - addr as i64 - j.total_len() as i64) as i32);
            j.file_offset = fo;
            ret[i] = j;
        } else if ret[i].target().is_some() {
            let base = ret[i].address as i64 + ret[i].total_len() as i64;
            let to = match jumps.get(&i).copied().flatten() {
                Some(t) => ret[t].address as i64,
                None => end_address as i64,
            };
            ret[i].set_offset((to - base) as i32);
        }
    }
    let amap = by_addr(&ret);
    for i in 0..count {
        if ret[i].store_size().is_some() {
            let stop = jumps.get(&i).copied().flatten();
            let mut cur = i;
            let mut n = 0;
            loop {
                let addr = ret[cur].address + ret[cur].total_len();
                match amap.get(&addr) {
                    Some(&x) if Some(x) != stop => {
                        n += 1;
                        cur = x;
                    }
                    _ => break,
                }
            }
            ret[i].set_store_size(n);
        }
    }
    for (pos, lasts) in containers.iter_mut() {
        let mut start = ret[*pos].address + ret[*pos].bytes_len(cs);
        for (j, last) in lasts.iter().enumerate() {
            let Some(l) = *last else { continue };
            let size = (ret[l].address + ret[l].total_len()).saturating_sub(start);
            ret[*pos].set_container_size(j, size);
            start += size;
        }
    }
    list.actions = ret;
}

/// The action bytes of a DoAction/DoInitAction tag body.
pub fn tag_actions(code: u16, body: &[u8]) -> Option<&[u8]> {
    match code {
        crate::tags::code::DO_ACTION => Some(body),
        crate::tags::code::DO_INIT_ACTION => body.get(2..),
        _ => None,
    }
}
