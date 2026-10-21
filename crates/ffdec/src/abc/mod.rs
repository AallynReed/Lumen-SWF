//! ActionScript 3 bytecode (ABC) as found in DoABC/DoABC2 tags, read like FFDec's `ABC`.

pub mod asm;
pub mod compile;
pub mod decomp;
pub mod names;
pub mod opcodes;
pub mod pcode;
pub mod scripts;
pub mod stats;
pub mod usages;
pub mod write;

use crate::error::{Error, Result};
use crate::io::Reader;
use crate::tags::{code::*, Tag};

pub mod ns_kind {
    pub const NAMESPACE: u8 = 0x08;
    pub const PRIVATE: u8 = 0x05;
    pub const PACKAGE: u8 = 0x16;
    pub const PACKAGE_INTERNAL: u8 = 0x17;
    pub const PROTECTED: u8 = 0x18;
    pub const EXPLICIT: u8 = 0x19;
    pub const STATIC_PROTECTED: u8 = 0x1a;
    pub const ALL: [u8; 7] = [NAMESPACE, PRIVATE, PACKAGE, PACKAGE_INTERNAL, PROTECTED, EXPLICIT, STATIC_PROTECTED];
}

pub mod mn_kind {
    pub const QNAME: u8 = 0x07;
    pub const QNAME_A: u8 = 0x0d;
    pub const RTQNAME: u8 = 0x0f;
    pub const RTQNAME_A: u8 = 0x10;
    pub const RTQNAME_L: u8 = 0x11;
    pub const RTQNAME_LA: u8 = 0x12;
    pub const MULTINAME: u8 = 0x09;
    pub const MULTINAME_A: u8 = 0x0e;
    pub const MULTINAME_L: u8 = 0x1b;
    pub const MULTINAME_LA: u8 = 0x1c;
    pub const TYPENAME: u8 = 0x1d;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Namespace {
    pub kind: u8,
    pub name: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Multiname {
    QName { attr: bool, ns: usize, name: usize },
    RtqName { attr: bool, name: usize },
    RtqNameL { attr: bool },
    Multiname { attr: bool, name: usize, ns_set: usize },
    MultinameL { attr: bool, ns_set: usize },
    TypeName { name: usize, params: Vec<usize> },
}

impl Multiname {
    pub fn kind(&self) -> u8 {
        use mn_kind::*;
        match self {
            Multiname::QName { attr, .. } => if *attr { QNAME_A } else { QNAME },
            Multiname::RtqName { attr, .. } => if *attr { RTQNAME_A } else { RTQNAME },
            Multiname::RtqNameL { attr } => if *attr { RTQNAME_LA } else { RTQNAME_L },
            Multiname::Multiname { attr, .. } => if *attr { MULTINAME_A } else { MULTINAME },
            Multiname::MultinameL { attr, .. } => if *attr { MULTINAME_LA } else { MULTINAME_L },
            Multiname::TypeName { .. } => TYPENAME,
        }
    }

    pub fn is_attribute(&self) -> bool {
        matches!(
            self,
            Multiname::QName { attr: true, .. }
                | Multiname::RtqName { attr: true, .. }
                | Multiname::RtqNameL { attr: true }
                | Multiname::Multiname { attr: true, .. }
                | Multiname::MultinameL { attr: true, .. }
        )
    }

    /// Multinames whose name or namespace is taken from the stack at runtime.
    pub fn needs_runtime_name(&self) -> bool {
        matches!(self, Multiname::RtqNameL { .. } | Multiname::MultinameL { .. })
    }

    pub fn needs_runtime_ns(&self) -> bool {
        matches!(self, Multiname::RtqName { .. } | Multiname::RtqNameL { .. })
    }
}

/// Constant pools. Index 0 of every pool is the implicit "none" entry.
#[derive(Debug, Default, Clone)]
pub struct Constants {
    pub ints: Vec<i32>,
    pub uints: Vec<u32>,
    pub doubles: Vec<f64>,
    pub decimals: Vec<[u8; 16]>,
    pub floats: Vec<f32>,
    pub strings: Vec<String>,
    pub namespaces: Vec<Namespace>,
    pub ns_sets: Vec<Vec<usize>>,
    pub multinames: Vec<Multiname>,
}

impl Constants {
    pub fn string(&self, i: usize) -> Option<&str> {
        if i == 0 { None } else { self.strings.get(i).map(String::as_str) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueKind {
    pub index: usize,
    pub kind: u8,
}

pub mod method_flags {
    pub const NEED_ARGUMENTS: u8 = 0x01;
    pub const NEED_ACTIVATION: u8 = 0x02;
    pub const NEED_REST: u8 = 0x04;
    pub const HAS_OPTIONAL: u8 = 0x08;
    pub const IGNORE_REST: u8 = 0x10;
    pub const EXPLICIT: u8 = 0x20;
    pub const SET_DXNS: u8 = 0x40;
    pub const HAS_PARAM_NAMES: u8 = 0x80;
}

#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub param_types: Vec<usize>,
    pub return_type: usize,
    pub name: usize,
    pub flags: u8,
    pub optional: Vec<ValueKind>,
    pub param_names: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Metadata {
    pub name: usize,
    pub keys: Vec<usize>,
    pub values: Vec<usize>,
}

pub mod trait_attr {
    pub const FINAL: u8 = 0x1;
    pub const OVERRIDE: u8 = 0x2;
    pub const METADATA: u8 = 0x4;
}

#[derive(Debug, Clone)]
pub enum TraitData {
    Slot { slot_id: usize, type_name: usize, value: Option<ValueKind> },
    Const { slot_id: usize, type_name: usize, value: Option<ValueKind> },
    Method { disp_id: usize, method: usize },
    Getter { disp_id: usize, method: usize },
    Setter { disp_id: usize, method: usize },
    Class { slot_id: usize, class: usize },
    Function { slot_id: usize, method: usize },
}

#[derive(Debug, Clone)]
pub struct Trait {
    pub name: usize,
    /// Upper four bits of the kind byte: final, override, metadata.
    pub attrs: u8,
    pub data: TraitData,
    pub metadata: Vec<usize>,
}

impl Trait {
    pub fn kind_name(&self) -> &'static str {
        match self.data {
            TraitData::Slot { .. } => "slot",
            TraitData::Const { .. } => "const",
            TraitData::Method { .. } => "method",
            TraitData::Getter { .. } => "getter",
            TraitData::Setter { .. } => "setter",
            TraitData::Class { .. } => "class",
            TraitData::Function { .. } => "function",
        }
    }

    pub fn method(&self) -> Option<usize> {
        match self.data {
            TraitData::Method { method, .. } | TraitData::Getter { method, .. } | TraitData::Setter { method, .. } | TraitData::Function { method, .. } => Some(method),
            _ => None,
        }
    }
}

pub mod instance_flags {
    pub const SEALED: u8 = 0x01;
    pub const FINAL: u8 = 0x02;
    pub const INTERFACE: u8 = 0x04;
    pub const PROTECTED_NS: u8 = 0x08;
    pub const NON_NULLABLE: u8 = 0x10;
}

#[derive(Debug, Clone)]
pub struct Instance {
    pub name: usize,
    pub super_name: usize,
    pub flags: u8,
    pub protected_ns: usize,
    pub interfaces: Vec<usize>,
    pub iinit: usize,
    pub traits: Vec<Trait>,
}

#[derive(Debug, Clone)]
pub struct Class {
    pub cinit: usize,
    pub traits: Vec<Trait>,
}

#[derive(Debug, Clone)]
pub struct Script {
    pub init: usize,
    pub traits: Vec<Trait>,
}

#[derive(Debug, Clone)]
pub struct Exception {
    pub start: usize,
    pub end: usize,
    pub target: usize,
    pub type_name: usize,
    pub var_name: usize,
}

#[derive(Debug, Clone)]
pub struct MethodBody {
    pub method: usize,
    pub max_stack: usize,
    pub max_regs: usize,
    pub init_scope_depth: usize,
    pub max_scope_depth: usize,
    pub code: Vec<u8>,
    pub exceptions: Vec<Exception>,
    pub traits: Vec<Trait>,
}

#[derive(Debug, Clone)]
pub struct Abc {
    pub minor: u16,
    pub major: u16,
    pub constants: Constants,
    pub methods: Vec<MethodInfo>,
    pub metadata: Vec<Metadata>,
    pub instances: Vec<Instance>,
    pub classes: Vec<Class>,
    pub scripts: Vec<Script>,
    pub bodies: Vec<MethodBody>,
    /// Method index → body index.
    pub body_of: Vec<Option<usize>>,
}

/// Variable-length unsigned integer: up to five bytes, seven bits each.
pub fn read_u32v(r: &mut Reader) -> Result<u32> {
    let mut v: u64 = 0;
    for i in 0..5 {
        let b = r.u8()?;
        v += ((b & 0x7f) as u64) << (7 * i);
        if b & 0x80 == 0 {
            break;
        }
    }
    Ok(v as u32)
}

pub fn read_u30(r: &mut Reader) -> Result<usize> {
    Ok((read_u32v(r)? & 0x3fff_ffff) as usize)
}

/// FFDec's `readS32`, including its handling of the fifth byte's sign.
pub fn read_s32v(r: &mut Reader) -> Result<i32> {
    let mut v: i64 = 0;
    let mut shift = 0;
    for _ in 0..5 {
        let b = r.u8()?;
        v += (((b & 0x7f) as i32) << shift) as i64;
        shift += 7;
        if shift == 35 {
            if v >> 31 == 1 {
                v = -(v & 0x7fff_ffff);
            }
            break;
        }
        if b & 0x80 == 0 {
            break;
        }
    }
    Ok(v as i32)
}

fn traits(r: &mut Reader) -> Result<Vec<Trait>> {
    let n = read_u30(r)?;
    let mut out = Vec::with_capacity(n.min(4096));
    for _ in 0..n {
        let name = read_u30(r)?;
        let kind = r.u8()?;
        let attrs = kind >> 4;
        let slot = |r: &mut Reader| -> Result<(usize, usize, Option<ValueKind>)> {
            let slot_id = read_u30(r)?;
            let type_name = read_u30(r)?;
            let vi = read_u30(r)?;
            let value = if vi != 0 { Some(ValueKind { index: vi, kind: r.u8()? }) } else { None };
            Ok((slot_id, type_name, value))
        };
        let data = match kind & 0x0f {
            0 => {
                let (slot_id, type_name, value) = slot(r)?;
                TraitData::Slot { slot_id, type_name, value }
            }
            6 => {
                let (slot_id, type_name, value) = slot(r)?;
                TraitData::Const { slot_id, type_name, value }
            }
            k @ 1..=3 => {
                let disp_id = read_u30(r)?;
                let method = read_u30(r)?;
                match k {
                    1 => TraitData::Method { disp_id, method },
                    2 => TraitData::Getter { disp_id, method },
                    _ => TraitData::Setter { disp_id, method },
                }
            }
            4 => TraitData::Class { slot_id: read_u30(r)?, class: read_u30(r)? },
            5 => TraitData::Function { slot_id: read_u30(r)?, method: read_u30(r)? },
            k => return Err(Error::Other(format!("unknown trait kind {k}"))),
        };
        let mut metadata = Vec::new();
        if attrs & trait_attr::METADATA != 0 {
            let m = read_u30(r)?;
            for _ in 0..m {
                metadata.push(read_u30(r)?);
            }
        }
        out.push(Trait { name, attrs, data, metadata });
    }
    Ok(out)
}

fn pool<T>(r: &mut Reader, zero: T, mut item: impl FnMut(&mut Reader) -> Result<T>) -> Result<Vec<T>> {
    let n = read_u30(r)?;
    let mut v = Vec::with_capacity(n.clamp(1, 1 << 16));
    v.push(zero);
    for _ in 1..n {
        v.push(item(r)?);
    }
    Ok(v)
}

impl Abc {
    /// An ABC with no definitions, for printing items outside AS3.
    pub fn empty() -> Abc {
        Abc { minor: 16, major: 46, constants: Constants::default(), methods: vec![], metadata: vec![], instances: vec![], classes: vec![], scripts: vec![], bodies: vec![], body_of: vec![] }
    }
    pub fn parse(data: &[u8]) -> Result<Abc> {
        let mut r = Reader::new(data);
        let minor = r.u16()?;
        let major = r.u16()?;
        let ver = (major, minor);
        let mut c = Constants {
            ints: pool(&mut r, 0, read_s32v)?,
            uints: pool(&mut r, 0, read_u32v)?,
            doubles: pool(&mut r, f64::NAN, |r| r.f64())?,
            ..Default::default()
        };
        if minor >= 17 {
            c.decimals = pool(&mut r, [0; 16], |r| Ok(r.bytes(16)?.try_into().unwrap()))?;
        }
        if ver >= (47, 16) {
            c.floats = pool(&mut r, f32::NAN, |r| r.f32())?;
        }
        c.strings = pool(&mut r, String::new(), |r| {
            let n = read_u30(r)?;
            Ok(String::from_utf8_lossy(r.bytes(n)?).into_owned())
        })?;
        c.namespaces = pool(&mut r, Namespace { kind: 0, name: 0 }, |r| {
            let kind = r.u8()?;
            let name = if ns_kind::ALL.contains(&kind) { read_u30(r)? } else { 0 };
            Ok(Namespace { kind, name })
        })?;
        c.ns_sets = pool(&mut r, Vec::new(), |r| {
            let n = read_u30(r)?;
            (0..n).map(|_| read_u30(r)).collect()
        })?;
        c.multinames = pool(&mut r, Multiname::QName { attr: false, ns: 0, name: 0 }, |r| {
            use mn_kind::*;
            let kind = r.u8()?;
            Ok(match kind {
                QNAME | QNAME_A => {
                    let ns = read_u30(r)?;
                    Multiname::QName { attr: kind == QNAME_A, ns, name: read_u30(r)? }
                }
                RTQNAME | RTQNAME_A => Multiname::RtqName { attr: kind == RTQNAME_A, name: read_u30(r)? },
                RTQNAME_L | RTQNAME_LA => Multiname::RtqNameL { attr: kind == RTQNAME_LA },
                MULTINAME | MULTINAME_A => {
                    let name = read_u30(r)?;
                    Multiname::Multiname { attr: kind == MULTINAME_A, name, ns_set: read_u30(r)? }
                }
                MULTINAME_L | MULTINAME_LA => Multiname::MultinameL { attr: kind == MULTINAME_LA, ns_set: read_u30(r)? },
                TYPENAME => {
                    let name = read_u30(r)?;
                    let n = read_u30(r)?;
                    Multiname::TypeName { name, params: (0..n).map(|_| read_u30(r)).collect::<Result<_>>()? }
                }
                k => return Err(Error::Other(format!("unknown multiname kind 0x{k:x}"))),
            })
        })?;

        let n = read_u30(&mut r)?;
        let mut methods = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            let pc = read_u30(&mut r)?;
            let return_type = read_u30(&mut r)?;
            let param_types = (0..pc).map(|_| read_u30(&mut r)).collect::<Result<_>>()?;
            let name = read_u30(&mut r)?;
            let flags = r.u8()?;
            let mut optional = Vec::new();
            if flags & method_flags::HAS_OPTIONAL != 0 {
                let oc = read_u30(&mut r)?;
                for _ in 0..oc {
                    let index = read_u30(&mut r)?;
                    optional.push(ValueKind { index, kind: r.u8()? });
                }
            }
            let mut param_names = vec![0; pc];
            if flags & method_flags::HAS_PARAM_NAMES != 0 {
                for p in param_names.iter_mut() {
                    *p = read_u30(&mut r)?;
                }
            }
            methods.push(MethodInfo { param_types, return_type, name, flags, optional, param_names });
        }

        let n = read_u30(&mut r)?;
        let mut metadata = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            let name = read_u30(&mut r)?;
            let vc = read_u30(&mut r)?;
            let keys = (0..vc).map(|_| read_u30(&mut r)).collect::<Result<_>>()?;
            let values = (0..vc).map(|_| read_u30(&mut r)).collect::<Result<_>>()?;
            metadata.push(Metadata { name, keys, values });
        }

        let n = read_u30(&mut r)?;
        let mut instances = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            let name = read_u30(&mut r)?;
            let super_name = read_u30(&mut r)?;
            let flags = r.u8()?;
            let protected_ns = if flags & instance_flags::PROTECTED_NS != 0 { read_u30(&mut r)? } else { 0 };
            let ic = read_u30(&mut r)?;
            let interfaces = (0..ic).map(|_| read_u30(&mut r)).collect::<Result<_>>()?;
            let iinit = read_u30(&mut r)?;
            instances.push(Instance { name, super_name, flags, protected_ns, interfaces, iinit, traits: traits(&mut r)? });
        }
        let mut classes = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            let cinit = read_u30(&mut r)?;
            classes.push(Class { cinit, traits: traits(&mut r)? });
        }
        let n = read_u30(&mut r)?;
        let mut scripts = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            let init = read_u30(&mut r)?;
            scripts.push(Script { init, traits: traits(&mut r)? });
        }

        let has_exception_names = ver > (46, 15);
        let n = read_u30(&mut r)?;
        let mut bodies = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            let body = (|| -> Result<MethodBody> {
                let method = read_u30(&mut r)?;
                let max_stack = read_u30(&mut r)?;
                let max_regs = read_u30(&mut r)?;
                let init_scope_depth = read_u30(&mut r)?;
                let max_scope_depth = read_u30(&mut r)?;
                let len = read_u30(&mut r)?;
                let code = r.bytes(len)?.to_vec();
                let ec = read_u30(&mut r)?;
                let mut exceptions = Vec::with_capacity(ec.min(1024));
                for _ in 0..ec {
                    exceptions.push(Exception {
                        start: read_u30(&mut r)?,
                        end: read_u30(&mut r)?,
                        target: read_u30(&mut r)?,
                        type_name: read_u30(&mut r)?,
                        var_name: if has_exception_names { read_u30(&mut r)? } else { 0 },
                    });
                }
                Ok(MethodBody { method, max_stack, max_regs, init_scope_depth, max_scope_depth, code, exceptions, traits: traits(&mut r)? })
            })();
            match body {
                Ok(b) => bodies.push(b),
                Err(Error::Eof) => break,
                Err(e) => return Err(e),
            }
        }
        let mut body_of = vec![None; methods.len()];
        for (i, b) in bodies.iter().enumerate() {
            if let Some(slot) = body_of.get_mut(b.method) {
                *slot = Some(i);
            }
        }
        Ok(Abc { minor, major, constants: c, methods, metadata, instances, classes, scripts, bodies, body_of })
    }

    pub fn body(&self, method: usize) -> Option<&MethodBody> {
        self.body_of.get(method).copied().flatten().and_then(|i| self.bodies.get(i))
    }
}

/// The ABC bytes of a DoABC or DoABC2 tag, and the DoABC2 name.
pub fn tag_abc(tag: &Tag) -> Option<(&[u8], Option<String>)> {
    match tag.code {
        DO_ABC => Some((&tag.body[..], None)),
        DO_ABC2 => {
            let mut r = Reader::new(&tag.body);
            r.u32().ok()?;
            let name = r.string().ok()?;
            Some((r.rest(), Some(name)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints() {
        let mut r = Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x0f]);
        assert_eq!(read_u32v(&mut r).unwrap(), u32::MAX);
        let mut r = Reader::new(&[0x7f]);
        assert_eq!(read_s32v(&mut r).unwrap(), 127);
        let mut r = Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x0f]);
        assert_eq!(read_s32v(&mut r).unwrap(), -1);
    }
}
