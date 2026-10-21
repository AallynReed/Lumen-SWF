//! AMF0 and AMF3 values, ported from FFDec's `amf` package: reading, writing, FFDec's text
//! form (`Amf0Exporter`/`Amf3Exporter` and their importers) and local shared object (.sol) files.
//!
//! Objects live in an arena so references and cycles survive a round trip. Unlike FFDec, AMF3
//! doubles with integral values print as `5.0` so they stay doubles when read back, AMF0 numbers
//! written as integers are accepted, dates print in UTC, and AMF3 values inside AMF0 are written
//! back with the AVM+ marker.

use std::collections::HashMap;

use crate::abc::names::{ecma_number, escape_as};
use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Undefined,
    Null,
    /// A value FFDec could not read; written as nothing.
    Unknown,
    Bool(bool),
    /// AMF3 integer.
    Int(i64),
    Double(f64),
    Str(String),
    Obj(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Obj {
    Object { class: String, dynamic: bool, sealed: Vec<(String, Value)>, members: Vec<(String, Value)> },
    Array { dense: Vec<Value>, assoc: Vec<(String, Value)> },
    VectorInt { fixed: bool, values: Vec<i64> },
    VectorUint { fixed: bool, values: Vec<i64> },
    VectorDouble { fixed: bool, values: Vec<f64> },
    VectorObject { fixed: bool, type_name: String, values: Vec<Value> },
    Dictionary { weak: bool, entries: Vec<(Value, Value)> },
    ByteArray(Vec<u8>),
    Date(f64),
    Xml(String),
    XmlDoc(String),
    /// AMF0 anonymous object.
    Object0(Vec<(String, Value)>),
    TypedObject0 { class: String, members: Vec<(String, Value)> },
    EcmaArray { dense: Vec<(String, Value)>, assoc: Vec<(String, Value)> },
    StrictArray(Vec<Value>),
    Date0 { millis: f64, timezone: i16 },
    XmlDoc0(String),
}

impl Obj {
    fn children(&self) -> Vec<&Value> {
        fn pairs(v: &[(String, Value)]) -> Vec<&Value> {
            v.iter().map(|p| &p.1).collect()
        }
        match self {
            Obj::Object { sealed, members, .. } => members.iter().chain(sealed).map(|p| &p.1).collect(),
            Obj::Array { dense, assoc } => assoc.iter().map(|p| &p.1).chain(dense).collect(),
            Obj::VectorObject { values, .. } | Obj::StrictArray(values) => values.iter().collect(),
            Obj::Dictionary { entries, .. } => entries.iter().flat_map(|(k, v)| [k, v]).collect(),
            Obj::Object0(m) | Obj::TypedObject0 { members: m, .. } => pairs(m),
            Obj::EcmaArray { dense, assoc } => dense.iter().chain(assoc).map(|p| &p.1).collect(),
            _ => Vec::new(),
        }
    }

    fn values_mut(&mut self) -> Vec<&mut Value> {
        match self {
            Obj::Object { sealed, members, .. } => sealed.iter_mut().chain(members.iter_mut()).map(|p| &mut p.1).collect(),
            Obj::Array { dense, assoc } => dense.iter_mut().chain(assoc.iter_mut().map(|p| &mut p.1)).collect(),
            Obj::VectorObject { values, .. } | Obj::StrictArray(values) => values.iter_mut().collect(),
            Obj::Dictionary { entries, .. } => entries.iter_mut().flat_map(|(k, v)| [k, v]).collect(),
            Obj::Object0(m) | Obj::TypedObject0 { members: m, .. } => m.iter_mut().map(|p| &mut p.1).collect(),
            Obj::EcmaArray { dense, assoc } => dense.iter_mut().chain(assoc.iter_mut()).map(|p| &mut p.1).collect(),
            _ => Vec::new(),
        }
    }

    /// Kinds that AMF0 can only carry inside an AVM+ (AMF3) value.
    fn is_amf3(&self) -> bool {
        !matches!(self, Obj::Object0(_) | Obj::TypedObject0 { .. } | Obj::EcmaArray { .. } | Obj::StrictArray(_) | Obj::Date0 { .. } | Obj::XmlDoc0(_))
    }
}

/// Values and the objects they refer to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Amf {
    pub objs: Vec<Obj>,
}

impl Amf {
    fn add(&mut self, o: Obj) -> usize {
        self.objs.push(o);
        self.objs.len() - 1
    }
}

fn err(m: impl Into<String>) -> Error {
    Error::Other(m.into())
}

struct In<'a> {
    d: &'a [u8],
    p: usize,
}

impl In<'_> {
    fn u8(&mut self) -> Result<u8> {
        let b = *self.d.get(self.p).ok_or(Error::Eof)?;
        self.p += 1;
        Ok(b)
    }
    fn bytes(&mut self, n: usize) -> Result<&[u8]> {
        let s = self.d.get(self.p..self.p.checked_add(n).ok_or(Error::Eof)?).ok_or(Error::Eof)?;
        self.p += n;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_be_bytes(self.bytes(8)?.try_into().unwrap()))
    }
    fn at_end(&self) -> bool {
        self.p >= self.d.len()
    }
    fn u29(&mut self) -> Result<u32> {
        let mut v = 0u32;
        for i in 1..=4 {
            let b = self.u8()? as u32;
            if i == 4 {
                v = (v << 8) | b;
            } else {
                v = (v << 7) | (b & 0x7f);
                if b & 0x80 == 0 {
                    break;
                }
            }
        }
        Ok(v)
    }
    fn utf8(&mut self, n: usize) -> Result<String> {
        Ok(String::from_utf8_lossy(self.bytes(n)?).into_owned())
    }
}

fn sign29(v: u32) -> i64 {
    if v & 0x1000_0000 != 0 { v as i64 - 0x2000_0000 } else { v as i64 }
}

#[derive(Default)]
struct Tables3 {
    strings: Vec<String>,
    objects: Vec<usize>,
    traits: Vec<(String, bool, Vec<String>)>,
}

/// `Amf3InputStream.readValue`.
struct Read3<'a, 'b> {
    i: &'b mut In<'a>,
    t: Tables3,
}

impl Read3<'_, '_> {
    fn string(&mut self) -> Result<String> {
        let u = self.i.u29()?;
        if u & 1 == 1 {
            let s = self.i.utf8((u >> 1) as usize)?;
            if !s.is_empty() {
                self.t.strings.push(s.clone());
            }
            Ok(s)
        } else {
            self.t.strings.get((u >> 1) as usize).cloned().ok_or_else(|| err("bad AMF3 string reference"))
        }
    }

    fn obj_ref(&self, u: u32) -> Result<Value> {
        self.t.objects.get((u >> 1) as usize).map(|&o| Value::Obj(o)).ok_or_else(|| err("bad AMF3 object reference"))
    }

    fn new_obj(&mut self, amf: &mut Amf, o: Obj) -> usize {
        let id = amf.add(o);
        self.t.objects.push(id);
        id
    }

    fn value(&mut self, amf: &mut Amf) -> Result<Value> {
        let marker = self.i.u8()?;
        Ok(match marker {
            0x00 => Value::Undefined,
            0x01 => Value::Null,
            0x02 => Value::Bool(false),
            0x03 => Value::Bool(true),
            0x04 => Value::Int(sign29(self.i.u29()?)),
            0x05 => Value::Double(self.i.f64()?),
            0x06 => Value::Str(self.string()?),
            0x07 | 0x0b => {
                let u = self.i.u29()?;
                if u & 1 == 0 {
                    return self.obj_ref(u);
                }
                let s = self.i.utf8((u >> 1) as usize)?;
                Value::Obj(self.new_obj(amf, if marker == 0x07 { Obj::XmlDoc(s) } else { Obj::Xml(s) }))
            }
            0x08 => {
                let u = self.i.u29()?;
                if u & 1 == 0 {
                    return self.obj_ref(u);
                }
                let d = self.i.f64()?;
                Value::Obj(self.new_obj(amf, Obj::Date(d)))
            }
            0x09 => {
                let u = self.i.u29()?;
                if u & 1 == 0 {
                    return self.obj_ref(u);
                }
                let id = self.new_obj(amf, Obj::Array { dense: Vec::new(), assoc: Vec::new() });
                let mut assoc = Vec::new();
                loop {
                    let k = self.string()?;
                    if k.is_empty() {
                        break;
                    }
                    assoc.push((k, self.value(amf)?));
                }
                let mut dense = Vec::new();
                for _ in 0..u >> 1 {
                    dense.push(self.value(amf)?);
                }
                amf.objs[id] = Obj::Array { dense, assoc };
                Value::Obj(id)
            }
            0x0a => {
                let u = self.i.u29()?;
                if u & 1 == 0 {
                    return self.obj_ref(u);
                }
                let traits = if (u >> 1) & 1 == 1 {
                    if (u >> 2) & 1 == 1 {
                        let class = self.string()?;
                        return Err(err(format!("no serializer for the externalizable class {class}")));
                    }
                    let class = self.string()?;
                    let mut names = Vec::new();
                    for _ in 0..u >> 4 {
                        names.push(self.string()?);
                    }
                    let t = (class, (u >> 3) & 1 == 1, names);
                    self.t.traits.push(t.clone());
                    t
                } else {
                    self.t.traits.get((u >> 2) as usize).cloned().ok_or_else(|| err("bad AMF3 traits reference"))?
                };
                let (class, dynamic, names) = traits;
                let id = self.new_obj(amf, Obj::Object { class: class.clone(), dynamic, sealed: Vec::new(), members: Vec::new() });
                let mut sealed = Vec::new();
                for n in names {
                    sealed.push((n, self.value(amf)?));
                }
                let mut members = Vec::new();
                if dynamic {
                    loop {
                        let k = self.string()?;
                        if k.is_empty() {
                            break;
                        }
                        members.push((k, self.value(amf)?));
                    }
                }
                amf.objs[id] = Obj::Object { class, dynamic, sealed, members };
                Value::Obj(id)
            }
            0x0c => {
                let u = self.i.u29()?;
                if u & 1 == 0 {
                    return self.obj_ref(u);
                }
                let b = self.i.bytes((u >> 1) as usize)?.to_vec();
                Value::Obj(self.new_obj(amf, Obj::ByteArray(b)))
            }
            0x0d..=0x10 => {
                let u = self.i.u29()?;
                if u & 1 == 0 {
                    return self.obj_ref(u);
                }
                let n = (u >> 1) as usize;
                let fixed = self.i.u8()? == 1;
                let o = match marker {
                    0x0d => Obj::VectorInt { fixed, values: (0..n).map(|_| self.i.u32().map(|v| v as i32 as i64)).collect::<Result<_>>()? },
                    0x0e => Obj::VectorUint { fixed, values: (0..n).map(|_| self.i.u32().map(|v| v as i64)).collect::<Result<_>>()? },
                    0x0f => Obj::VectorDouble { fixed, values: (0..n).map(|_| self.i.f64()).collect::<Result<_>>()? },
                    _ => {
                        let type_name = self.string()?;
                        let id = self.new_obj(amf, Obj::VectorObject { fixed, type_name: type_name.clone(), values: Vec::new() });
                        let mut values = Vec::new();
                        for _ in 0..n {
                            values.push(self.value(amf)?);
                        }
                        amf.objs[id] = Obj::VectorObject { fixed, type_name, values };
                        return Ok(Value::Obj(id));
                    }
                };
                Value::Obj(self.new_obj(amf, o))
            }
            0x11 => {
                let u = self.i.u29()?;
                if u & 1 == 0 {
                    return self.obj_ref(u);
                }
                let weak = self.i.u8()? == 1;
                let id = self.new_obj(amf, Obj::Dictionary { weak, entries: Vec::new() });
                let mut entries = Vec::new();
                for _ in 0..u >> 1 {
                    let k = self.value(amf)?;
                    entries.push((k, self.value(amf)?));
                }
                amf.objs[id] = Obj::Dictionary { weak, entries };
                Value::Obj(id)
            }
            m => return Err(err(format!("unsupported AMF3 value type {m:#04x}"))),
        })
    }
}

/// Reads one AMF3 value, such as PlaceObject4's metadata or `ByteArray.writeObject` output.
pub fn read_amf3(data: &[u8]) -> Result<(Amf, Value)> {
    let mut amf = Amf::default();
    let mut i = In { d: data, p: 0 };
    let v = Read3 { i: &mut i, t: Tables3::default() }.value(&mut amf)?;
    Ok((amf, v))
}

#[derive(Default)]
struct Write3 {
    out: Vec<u8>,
    strings: Vec<String>,
    objects: Vec<usize>,
    traits: Vec<(String, bool, Vec<String>)>,
}

impl Write3 {
    fn u29(&mut self, v: i64) {
        let v = (v & 0x3fff_ffff) as u32;
        if v <= 0x7f {
            self.out.push(v as u8);
        } else if v <= 0x3fff {
            self.out.extend([((v >> 7) & 0x7f) as u8 | 0x80, (v & 0x7f) as u8]);
        } else if v <= 0x1f_ffff {
            self.out.extend([((v >> 14) & 0x7f) as u8 | 0x80, ((v >> 7) & 0x7f) as u8 | 0x80, (v & 0x7f) as u8]);
        } else {
            self.out.extend([((v >> 21) & 0x7f) as u8 | 0x80, ((v >> 14) & 0x7f) as u8 | 0x80, ((v >> 7) & 0x7f) as u8 | 0x80, (v & 0xff) as u8]);
        }
    }

    fn string(&mut self, s: &str) {
        match self.strings.iter().position(|x| x == s) {
            Some(i) => self.u29((i as i64) << 1),
            None => {
                if !s.is_empty() {
                    self.strings.push(s.to_string());
                }
                self.u29(((s.len() as i64) << 1) | 1);
                self.out.extend_from_slice(s.as_bytes());
            }
        }
    }

    fn double(&mut self, v: f64) {
        let bits = if v.is_nan() { f64::NAN.to_bits() } else { v.to_bits() };
        self.out.extend_from_slice(&bits.to_be_bytes());
    }

    fn value(&mut self, amf: &Amf, v: &Value) -> Result<()> {
        match v {
            Value::Undefined => self.out.push(0x00),
            Value::Null => self.out.push(0x01),
            Value::Bool(b) => self.out.push(if *b { 0x03 } else { 0x02 }),
            Value::Unknown => {}
            Value::Int(i) => {
                self.out.push(0x04);
                self.u29(*i);
            }
            Value::Double(d) => {
                self.out.push(0x05);
                self.double(*d);
            }
            Value::Str(s) => {
                self.out.push(0x06);
                self.string(s);
            }
            Value::Obj(id) => self.obj(amf, *id)?,
        }
        Ok(())
    }

    fn obj(&mut self, amf: &Amf, id: usize) -> Result<()> {
        let o = amf.objs.get(id).ok_or_else(|| err("bad object id"))?;
        let marker = match o {
            Obj::XmlDoc(_) | Obj::XmlDoc0(_) => 0x07,
            Obj::Date(_) | Obj::Date0 { .. } => 0x08,
            Obj::Array { .. } | Obj::StrictArray(_) | Obj::EcmaArray { .. } => 0x09,
            Obj::Object { .. } | Obj::Object0(_) | Obj::TypedObject0 { .. } => 0x0a,
            Obj::Xml(_) => 0x0b,
            Obj::ByteArray(_) => 0x0c,
            Obj::VectorInt { .. } => 0x0d,
            Obj::VectorUint { .. } => 0x0e,
            Obj::VectorDouble { .. } => 0x0f,
            Obj::VectorObject { .. } => 0x10,
            Obj::Dictionary { .. } => 0x11,
        };
        self.out.push(marker);
        if let Some(i) = self.objects.iter().position(|&x| x == id) {
            self.u29((i as i64) << 1);
            return Ok(());
        }
        self.objects.push(id);
        match o {
            Obj::XmlDoc(s) | Obj::XmlDoc0(s) | Obj::Xml(s) => {
                self.u29(((s.len() as i64) << 1) | 1);
                self.out.extend_from_slice(s.as_bytes());
            }
            Obj::Date(d) | Obj::Date0 { millis: d, .. } => {
                self.u29(1);
                self.double(*d);
            }
            Obj::ByteArray(b) => {
                self.u29(((b.len() as i64) << 1) | 1);
                self.out.extend_from_slice(b);
            }
            Obj::Array { dense, assoc } => self.array(amf, dense, assoc)?,
            Obj::StrictArray(dense) => self.array(amf, dense, &[])?,
            Obj::EcmaArray { dense, assoc } => {
                let all: Vec<(String, Value)> = dense.iter().chain(assoc).cloned().collect();
                self.array(amf, &[], &all)?;
            }
            Obj::Object { class, dynamic, sealed, members } => self.object(amf, class, *dynamic, sealed, members)?,
            Obj::Object0(m) => self.object(amf, "", true, &[], m)?,
            Obj::TypedObject0 { class, members } => self.object(amf, class, true, &[], members)?,
            Obj::VectorInt { fixed, values } | Obj::VectorUint { fixed, values } => {
                self.u29(((values.len() as i64) << 1) | 1);
                self.out.push(*fixed as u8);
                for v in values {
                    self.out.extend_from_slice(&(*v as u32).to_be_bytes());
                }
            }
            Obj::VectorDouble { fixed, values } => {
                self.u29(((values.len() as i64) << 1) | 1);
                self.out.push(*fixed as u8);
                for v in values {
                    self.double(*v);
                }
            }
            Obj::VectorObject { fixed, type_name, values } => {
                self.u29(((values.len() as i64) << 1) | 1);
                self.out.push(*fixed as u8);
                self.string(type_name);
                for v in values {
                    self.value(amf, v)?;
                }
            }
            Obj::Dictionary { weak, entries } => {
                self.u29(((entries.len() as i64) << 1) | 1);
                self.out.push(*weak as u8);
                for (k, v) in entries {
                    self.value(amf, k)?;
                    self.value(amf, v)?;
                }
            }
        }
        Ok(())
    }

    fn array(&mut self, amf: &Amf, dense: &[Value], assoc: &[(String, Value)]) -> Result<()> {
        self.u29(((dense.len() as i64) << 1) | 1);
        for (k, v) in assoc {
            self.string(k);
            self.value(amf, v)?;
        }
        self.string("");
        for v in dense {
            self.value(amf, v)?;
        }
        Ok(())
    }

    fn object(&mut self, amf: &Amf, class: &str, dynamic: bool, sealed: &[(String, Value)], members: &[(String, Value)]) -> Result<()> {
        let t = (class.to_string(), dynamic, sealed.iter().map(|p| p.0.clone()).collect::<Vec<_>>());
        match self.traits.iter().position(|x| *x == t) {
            Some(i) => self.u29(((i as i64) << 2) | 1),
            None => {
                self.u29(((sealed.len() as i64) << 4) | 3 | if dynamic { 8 } else { 0 });
                self.string(class);
                for (k, _) in sealed {
                    self.string(k);
                }
                self.traits.push(t);
            }
        }
        for (_, v) in sealed {
            self.value(amf, v)?;
        }
        if dynamic {
            for (k, v) in members {
                self.string(k);
                self.value(amf, v)?;
            }
            self.string("");
        }
        Ok(())
    }
}

/// `Amf3OutputStream.writeValue`.
pub fn write_amf3(amf: &Amf, v: &Value) -> Result<Vec<u8>> {
    let mut w = Write3::default();
    w.value(amf, v)?;
    Ok(w.out)
}

/// `Amf0InputStream.readValue`, with references resolved while reading.
struct Read0<'a, 'b> {
    i: &'b mut In<'a>,
    complex: Vec<usize>,
}

impl Read0<'_, '_> {
    fn utf8(&mut self) -> Result<String> {
        let n = self.i.u16()? as usize;
        self.i.utf8(n)
    }

    fn members(&mut self, amf: &mut Amf) -> Result<Vec<(String, Value)>> {
        let mut out = Vec::new();
        loop {
            let k = self.utf8()?;
            let v = self.value(amf)?;
            if k.is_empty() {
                return Ok(out);
            }
            out.push((k, v));
        }
    }

    fn complex(&mut self, amf: &mut Amf, o: Obj) -> usize {
        let id = amf.add(o);
        self.complex.push(id);
        id
    }

    fn value(&mut self, amf: &mut Amf) -> Result<Value> {
        let m = self.i.u8()?;
        Ok(match m {
            0x00 => Value::Double(self.i.f64()?),
            0x01 => Value::Bool(self.i.u8()? > 0),
            0x02 => Value::Str(self.utf8()?),
            0x03 => {
                let id = self.complex(amf, Obj::Object0(Vec::new()));
                amf.objs[id] = Obj::Object0(self.members(amf)?);
                Value::Obj(id)
            }
            0x05 => Value::Null,
            0x06 => Value::Undefined,
            0x07 => {
                let r = self.i.u16()? as usize;
                self.complex.get(r).map(|&o| Value::Obj(o)).ok_or_else(|| err("bad AMF0 reference"))?
            }
            0x08 => {
                let n = self.i.u32()?;
                let id = self.complex(amf, Obj::EcmaArray { dense: Vec::new(), assoc: Vec::new() });
                let mut dense = Vec::new();
                for _ in 0..n {
                    let k = self.utf8()?;
                    dense.push((k, self.value(amf)?));
                }
                let assoc = self.members(amf)?;
                amf.objs[id] = Obj::EcmaArray { dense, assoc };
                Value::Obj(id)
            }
            0x09 => Value::Unknown,
            0x0a => {
                let n = self.i.u32()?;
                let id = self.complex(amf, Obj::StrictArray(Vec::new()));
                let mut vals = Vec::new();
                for _ in 0..n {
                    vals.push(self.value(amf)?);
                }
                amf.objs[id] = Obj::StrictArray(vals);
                Value::Obj(id)
            }
            0x0b => {
                let millis = self.i.f64()?;
                let timezone = self.i.u16()? as i16;
                Value::Obj(self.complex(amf, Obj::Date0 { millis, timezone }))
            }
            0x0c => {
                let n = self.i.u32()? as usize;
                Value::Str(self.i.utf8(n)?)
            }
            0x0f => {
                let n = self.i.u32()? as usize;
                let s = self.i.utf8(n)?;
                Value::Obj(self.complex(amf, Obj::XmlDoc0(s)))
            }
            0x10 => {
                let class = self.utf8()?;
                let id = self.complex(amf, Obj::TypedObject0 { class: class.clone(), members: Vec::new() });
                let members = self.members(amf)?;
                amf.objs[id] = Obj::TypedObject0 { class, members };
                Value::Obj(id)
            }
            0x11 => Read3 { i: self.i, t: Tables3::default() }.value(amf)?,
            0x04 => return Err(err("MovieClip is not supported in AMF0")),
            0x0e => return Err(err("RecordSet is not supported in AMF0")),
            m => return Err(err(format!("unsupported AMF0 value type {m:#04x}"))),
        })
    }
}

#[derive(Default)]
struct Write0 {
    out: Vec<u8>,
    complex: Vec<usize>,
}

impl Write0 {
    fn utf8(&mut self, s: &str) {
        self.out.extend_from_slice(&(s.len() as u16).to_be_bytes());
        self.out.extend_from_slice(s.as_bytes());
    }

    fn utf8_long(&mut self, s: &str) {
        self.out.extend_from_slice(&(s.len() as u32).to_be_bytes());
        self.out.extend_from_slice(s.as_bytes());
    }

    fn members(&mut self, amf: &Amf, m: &[(String, Value)]) -> Result<()> {
        for (k, v) in m {
            self.utf8(k);
            self.value(amf, v)?;
        }
        self.utf8("");
        self.out.push(0x09);
        Ok(())
    }

    fn value(&mut self, amf: &Amf, v: &Value) -> Result<()> {
        match v {
            Value::Double(d) => {
                self.out.push(0x00);
                self.out.extend_from_slice(&d.to_be_bytes());
            }
            Value::Bool(b) => self.out.extend([0x01, *b as u8]),
            Value::Str(s) if s.len() > 0xffff => {
                self.out.push(0x0c);
                self.utf8_long(s);
            }
            Value::Str(s) => {
                self.out.push(0x02);
                self.utf8(s);
            }
            Value::Null => self.out.push(0x05),
            Value::Undefined => self.out.push(0x06),
            Value::Unknown => self.out.push(0x09),
            Value::Int(_) => {
                self.out.push(0x11);
                let mut w = Write3::default();
                w.value(amf, v)?;
                self.out.extend(w.out);
            }
            Value::Obj(id) => {
                let o = amf.objs.get(*id).ok_or_else(|| err("bad object id"))?;
                if o.is_amf3() {
                    self.out.push(0x11);
                    let mut w = Write3::default();
                    w.value(amf, v)?;
                    self.out.extend(w.out);
                    return Ok(());
                }
                if let Some(i) = self.complex.iter().position(|x| x == id).filter(|&i| i <= 0xffff) {
                    self.out.push(0x07);
                    self.out.extend_from_slice(&(i as u16).to_be_bytes());
                    return Ok(());
                }
                self.complex.push(*id);
                match o {
                    Obj::Object0(m) => {
                        self.out.push(0x03);
                        self.members(amf, m)?;
                    }
                    Obj::TypedObject0 { class, members } => {
                        self.out.push(0x10);
                        self.utf8(class);
                        self.members(amf, members)?;
                    }
                    Obj::EcmaArray { dense, assoc } => {
                        self.out.push(0x08);
                        self.out.extend_from_slice(&(dense.len() as u32).to_be_bytes());
                        for (k, v) in dense {
                            self.utf8(k);
                            self.value(amf, v)?;
                        }
                        self.members(amf, assoc)?;
                    }
                    Obj::StrictArray(vals) => {
                        self.out.push(0x0a);
                        self.out.extend_from_slice(&(vals.len() as u32).to_be_bytes());
                        for v in vals {
                            self.value(amf, v)?;
                        }
                    }
                    Obj::Date0 { millis, timezone } => {
                        self.out.push(0x0b);
                        self.out.extend_from_slice(&millis.to_be_bytes());
                        self.out.extend_from_slice(&timezone.to_be_bytes());
                    }
                    Obj::XmlDoc0(s) => {
                        self.out.push(0x0f);
                        self.utf8_long(s);
                    }
                    _ => unreachable!(),
                }
            }
        }
        Ok(())
    }
}

/// A local shared object: its name, AMF version and variables.
#[derive(Debug, Clone, PartialEq)]
pub struct Sol {
    pub name: String,
    pub amf_version: u32,
    pub amf: Amf,
    pub vars: Vec<(String, Value)>,
}

impl Sol {
    /// `SolFile` reading its `LsoTag`.
    pub fn parse(data: &[u8]) -> Result<Sol> {
        let mut i = In { d: data, p: 0 };
        while !i.at_end() {
            let h = i.u16()?;
            let mut len = (h & 0x3f) as usize;
            if len == 0x3f {
                len = i.u32()? as usize;
            }
            let body = i.bytes(len)?;
            if h >> 6 == 2 {
                return Sol::lso(body);
            }
        }
        Err(err("not a SOL file"))
    }

    fn lso(body: &[u8]) -> Result<Sol> {
        let mut i = In { d: body, p: 0 };
        if i.bytes(4)? != b"TCSO" {
            return Err(err("not a SOL file: bad signature"));
        }
        i.bytes(6)?;
        let n = i.u16()? as usize;
        let name = i.utf8(n)?;
        let amf_version = i.u32()?;
        let mut amf = Amf::default();
        let mut vars = Vec::new();
        match amf_version {
            0 => {
                let mut r = Read0 { i: &mut i, complex: Vec::new() };
                while !r.i.at_end() {
                    let k = r.utf8()?;
                    let v = r.value(&mut amf)?;
                    vars.push((k, v));
                    r.i.u8()?;
                }
            }
            3 => {
                let mut r = Read3 { i: &mut i, t: Tables3::default() };
                while !r.i.at_end() {
                    let k = r.string()?;
                    let v = r.value(&mut amf)?;
                    vars.push((k, v));
                    r.i.u8()?;
                }
            }
            _ => return Err(err("unsupported AMF version")),
        }
        Ok(Sol { name, amf_version, amf, vars })
    }

    /// `SolFile.writeTo`: one LsoTag with a long header.
    pub fn write(&self) -> Result<Vec<u8>> {
        let mut b = b"TCSO\x00\x04\x00\x00\x00\x00".to_vec();
        b.extend_from_slice(&(self.name.len() as u16).to_be_bytes());
        b.extend_from_slice(self.name.as_bytes());
        b.extend_from_slice(&self.amf_version.to_be_bytes());
        if self.amf_version == 3 {
            let mut w = Write3::default();
            for (k, v) in &self.vars {
                w.string(k);
                w.value(&self.amf, v)?;
                w.out.push(0);
            }
            b.extend(w.out);
        } else {
            let mut w = Write0::default();
            for (k, v) in &self.vars {
                w.utf8(k);
                w.value(&self.amf, v)?;
                w.out.push(0);
            }
            b.extend(w.out);
        }
        let mut out = ((2u16 << 6) | 0x3f).to_be_bytes().to_vec();
        out.extend_from_slice(&(b.len() as u32).to_be_bytes());
        out.extend(b);
        Ok(out)
    }

    /// The variables as FFDec's text (`amfMapToString`).
    pub fn to_text(&self) -> String {
        map_to_string(&self.amf, &self.vars, self.amf_version == 0)
    }

    /// Replaces the variables with ones parsed from FFDec's text.
    pub fn set_text(&mut self, text: &str) -> std::result::Result<(), ParseError> {
        let (amf, vars) = parse_map(text, self.amf_version == 0)?;
        self.amf = amf;
        self.vars = vars;
        Ok(())
    }
}

// ---- text ----

fn indent(level: usize) -> String {
    "  ".repeat(level)
}

fn quote(s: &str) -> String {
    format!("\"{}\"", escape_as(s))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Days since 1970-01-01 to (year, month, day).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `yyyy-MM-dd HH:mm:ss.SSS`, in UTC.
pub fn format_date(millis: f64) -> String {
    let ms = millis as i64;
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    let (y, mo, d) = civil(days);
    format!("{y:04}-{mo:02}-{d:02} {:02}:{:02}:{:02}.{:03}", rem / 3_600_000, rem / 60_000 % 60, rem / 1000 % 60, rem % 1000)
}

pub fn parse_date(s: &str) -> Option<f64> {
    let (date, time) = s.trim().split_once(' ')?;
    let mut dp = date.splitn(3, '-');
    let y: i64 = dp.next()?.parse().ok()?;
    let mo: u32 = dp.next()?.parse().ok()?;
    let d: u32 = dp.next()?.parse().ok()?;
    let (hms, frac) = time.split_once('.').unwrap_or((time, "0"));
    let mut tp = hms.splitn(3, ':');
    let h: i64 = tp.next()?.parse().ok()?;
    let mi: i64 = tp.next()?.parse().ok()?;
    let sec: i64 = tp.next()?.parse().ok()?;
    let ms: i64 = frac.parse().ok()?;
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    Some((days_from_civil(y, mo, d) * 86_400_000 + ((h * 60 + mi) * 60 + sec) * 1000 + ms) as f64)
}

/// `populateObjects`: how often each object is reached, and its `objN` alias.
struct Refs {
    count: HashMap<usize, usize>,
    alias: HashMap<usize, String>,
    done: Vec<usize>,
}

impl Refs {
    fn new<'a>(amf: &Amf, roots: impl Iterator<Item = &'a Value>, amf0: bool) -> Refs {
        let mut r = Refs { count: HashMap::new(), alias: HashMap::new(), done: Vec::new() };
        let mut list = 0;
        fn go(amf: &Amf, v: &Value, r: &mut Refs, list: &mut usize, amf0: bool) {
            let Value::Obj(id) = v else { return };
            let c = r.count.entry(*id).or_insert(0);
            *c += 1;
            if *c > 1 {
                return;
            }
            let o = &amf.objs[*id];
            if amf0 {
                if o.is_amf3() {
                    return;
                }
                r.alias.insert(*id, format!("obj{list}"));
                *list += 1;
                for c in o.children() {
                    go(amf, c, r, list, amf0);
                }
            } else {
                for c in o.children() {
                    go(amf, c, r, list, amf0);
                }
                *list += 1;
                r.alias.insert(*id, format!("obj{list}"));
            }
        }
        for v in roots {
            go(amf, v, &mut r, &mut list, amf0);
        }
        r
    }

    /// The reference text if the object was printed already, else the `"id"` line to add.
    fn visit(&mut self, id: usize, level: usize, nl: &str) -> std::result::Result<String, String> {
        let shared = self.count.get(&id).copied().unwrap_or(0) > 1;
        let alias = self.alias.get(&id).cloned().unwrap_or_default();
        if shared && self.done.contains(&id) {
            return Err(format!("{{{nl}{}\"type\": \"Reference\",{nl}{}\"referencedId\": \"{alias}\"{nl}{}}}", indent(level + 1), indent(level + 1), indent(level)));
        }
        self.done.push(id);
        Ok(if shared { format!("{}\"id\": \"{alias}\",{nl}", indent(level + 1)) } else { String::new() })
    }
}

fn double3(d: f64) -> String {
    let s = ecma_number(d);
    if s.bytes().all(|b| b.is_ascii_digit() || b == b'-') { format!("{s}.0") } else { s }
}

/// `Amf3Exporter.amfToString`.
fn to_string3(amf: &Amf, v: &Value, level: usize, nl: &str, r: &mut Refs) -> String {
    let id = match v {
        Value::Str(s) => return quote(s),
        Value::Int(i) => return i.to_string(),
        Value::Double(d) => return double3(*d),
        Value::Bool(b) => return b.to_string(),
        Value::Undefined => return format!("{{{nl}{}\"type\": \"Undefined\"{nl}{}}}", indent(level + 1), indent(level)),
        Value::Null => return "null".into(),
        Value::Unknown => return "unknown".into(),
        Value::Obj(id) => *id,
    };
    let add_id = match r.visit(id, level, nl) {
        Ok(a) => a,
        Err(reference) => return reference,
    };
    let i1 = indent(level + 1);
    let i2 = indent(level + 2);
    let mut s = String::new();
    let head = |s: &mut String, ty: &str| {
        s.push_str(&format!("{{{nl}{i1}\"type\": \"{ty}\",{nl}{add_id}"));
    };
    match &amf.objs[id] {
        Obj::VectorInt { fixed, values } | Obj::VectorUint { fixed, values } => {
            let sub = if matches!(amf.objs[id], Obj::VectorInt { .. }) { "int" } else { "uint" };
            head(&mut s, "Vector");
            s.push_str(&format!("{i1}\"fixed\": {fixed},{nl}{i1}\"subtype\": \"{sub}\",{nl}{i1}\"values\": [{}]{nl}", values.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ")));
        }
        Obj::VectorDouble { fixed, values } => {
            head(&mut s, "Vector");
            s.push_str(&format!("{i1}\"fixed\": {fixed},{nl}{i1}\"subtype\": \"Number\",{nl}{i1}\"values\": [{}]{nl}", values.iter().map(|v| double3(*v)).collect::<Vec<_>>().join(", ")));
        }
        Obj::VectorObject { fixed, type_name, values } => {
            head(&mut s, "Vector");
            let vals: Vec<String> = values.iter().map(|v| to_string3(amf, v, level + 1, nl, r)).collect();
            s.push_str(&format!("{i1}\"fixed\": {fixed},{nl}{i1}\"subtype\": {},{nl}{i1}\"values\": [{}]{nl}", quote(type_name), vals.join(", ")));
        }
        Obj::Object { class, dynamic, sealed, members } => {
            head(&mut s, "Object");
            s.push_str(&format!("{i1}\"className\": {},{nl}{i1}\"dynamic\": {dynamic},{nl}{i1}\"sealedMembers\": {{{nl}", quote(class)));
            for (n, (k, v)) in sealed.iter().enumerate() {
                let val = to_string3(amf, v, level + 1, nl, r);
                s.push_str(&format!("{i2}{}:{val}{}{nl}", quote(k), if n + 1 < sealed.len() { "," } else { "" }));
            }
            s.push_str(&format!("{i1}}},{nl}{i1}\"dynamicMembers\": {{{nl}"));
            for (n, (k, v)) in members.iter().enumerate() {
                let val = to_string3(amf, v, level + 2, nl, r);
                s.push_str(&format!("{i2}{}: {val}{}{nl}", quote(k), if n + 1 < members.len() { "," } else { "" }));
            }
            s.push_str(&format!("{i1}}}{nl}"));
        }
        Obj::Array { dense, assoc } => {
            head(&mut s, "Array");
            let vals: Vec<String> = dense.iter().map(|v| to_string3(amf, v, level + 2, nl, r)).collect();
            s.push_str(&format!("{i1}\"denseValues\": [{}],{nl}{i1}\"associativeValues\": {{", vals.join(", ")));
            if !assoc.is_empty() {
                s.push_str(nl);
            }
            for (n, (k, v)) in assoc.iter().enumerate() {
                let val = to_string3(amf, v, level + 1, nl, r);
                s.push_str(&format!("{i2}{} : {val}{}{nl}", quote(k), if n + 1 < assoc.len() { "," } else { "" }));
            }
            if !assoc.is_empty() {
                s.push_str(&i1);
            }
            s.push_str(&format!("}}{nl}"));
        }
        Obj::Dictionary { weak, entries } => {
            head(&mut s, "Dictionary");
            s.push_str(&format!("{i1}\"weakKeys\": {weak},{nl}{i1}\"entries\": [{nl}"));
            let i3 = indent(level + 3);
            for (n, (k, v)) in entries.iter().enumerate() {
                let ks = to_string3(amf, k, level + 3, nl, r);
                let vs = to_string3(amf, v, level + 3, nl, r);
                s.push_str(&format!("{i2}{{{nl}{i3}\"key\": {ks},{nl}{i3}\"value\": {vs}{nl}{i2}}}{}{nl}", if n + 1 < entries.len() { "," } else { "" }));
            }
            s.push_str(&format!("{i1}]{nl}"));
        }
        Obj::ByteArray(b) => {
            head(&mut s, "ByteArray");
            s.push_str(&format!("{i1}\"value\": \"{}\"{nl}", hex(b)));
        }
        Obj::Date(d) | Obj::Date0 { millis: d, .. } => {
            head(&mut s, "Date");
            s.push_str(&format!("{i1}\"value\": {}{nl}", quote(&format_date(*d))));
        }
        Obj::XmlDoc(x) | Obj::XmlDoc0(x) => {
            head(&mut s, "XMLDocument");
            s.push_str(&format!("{i1}\"value\": {}{nl}", quote(x)));
        }
        Obj::Xml(x) => {
            head(&mut s, "XML");
            s.push_str(&format!("{i1}\"value\": {}{nl}", quote(x)));
        }
        o => return to_string0(amf, &Value::Obj(id), level, nl, r, Some((o, add_id))),
    }
    s.push_str(&indent(level));
    s.push('}');
    s
}

/// `Amf0Exporter.amfToString`; AMF3 values inside are printed as `Amf3Exporter` does.
fn to_string0(amf: &Amf, v: &Value, level: usize, nl: &str, r: &mut Refs, visited: Option<(&Obj, String)>) -> String {
    let (o, add_id) = match (v, visited) {
        (_, Some(x)) => x,
        (Value::Double(d), _) => return ecma_number(*d),
        (Value::Obj(id), None) => {
            let o = &amf.objs[*id];
            if o.is_amf3() {
                return to_string3(amf, v, level, nl, r);
            }
            match r.visit(*id, level, nl) {
                Ok(a) => (o, a),
                Err(reference) => return reference,
            }
        }
        _ => return to_string3(amf, v, level, nl, r),
    };
    let i1 = indent(level + 1);
    let members = |label: &str, m: &[(String, Value)], r: &mut Refs| {
        let mut s = format!("{i1}\"{label}\": {{{nl}");
        let items: Vec<String> = m.iter().map(|(k, v)| format!("{}{}: {}", indent(level + 2), quote(k), to_string0(amf, v, level + 2, nl, r, None))).collect();
        s.push_str(&items.join(&format!(",{nl}")));
        s.push_str(&format!("{nl}{i1}}}"));
        s
    };
    let mut s = String::from("{");
    s.push_str(nl);
    match o {
        Obj::Object0(m) => {
            s.push_str(&format!("{i1}\"type\": \"Object\",{nl}{add_id}"));
            s.push_str(&members("members", m, r));
        }
        Obj::TypedObject0 { class, members: m } => {
            s.push_str(&format!("{i1}\"type\": \"TypedObject\",{nl}{add_id}{i1}\"className\": {},{nl}", quote(class)));
            s.push_str(&members("members", m, r));
        }
        Obj::EcmaArray { dense, assoc } => {
            s.push_str(&format!("{i1}\"type\": \"EcmaArray\",{nl}{add_id}"));
            s.push_str(&members("denseValues", dense, r));
            s.push_str(&format!(",{nl}"));
            s.push_str(&members("associativeValues", assoc, r));
        }
        Obj::StrictArray(vals) => {
            s.push_str(&format!("{i1}\"type\": \"Array\",{nl}{add_id}{i1}\"values\": [{nl}"));
            let items: Vec<String> = vals.iter().map(|v| to_string0(amf, v, level + 1, nl, r, None)).collect();
            s.push_str(&items.join(", "));
            s.push_str(&format!("{i1}]"));
        }
        Obj::Date0 { millis, timezone } => {
            s.push_str(&format!("{i1}\"type\": \"Date\",{nl}{add_id}{i1}\"value\": {},{nl}{i1}\"timezone\": {timezone}", quote(&format_date(*millis))));
        }
        Obj::XmlDoc0(x) => {
            s.push_str(&format!("{i1}\"type\": \"XMLDocument\",{nl}{add_id}{i1}\"value\": {}", quote(x)));
        }
        _ => unreachable!(),
    }
    s.push_str(nl);
    s.push_str(&indent(level));
    s.push('}');
    s
}

/// One AMF3 value as FFDec's text.
pub fn to_text(amf: &Amf, v: &Value) -> String {
    let mut r = Refs::new(amf, std::iter::once(v), false);
    to_string3(amf, v, 0, "\n", &mut r)
}

/// `amfMapToString` for AMF3 or AMF0 variables.
pub fn map_to_string(amf: &Amf, vars: &[(String, Value)], amf0: bool) -> String {
    let nl = "\n";
    let mut r = Refs::new(amf, vars.iter().map(|p| &p.1), amf0);
    let items: Vec<String> = vars
        .iter()
        .map(|(k, v)| format!("{}{}: {}", indent(1), quote(k), if amf0 { to_string0(amf, v, 1, nl, &mut r, None) } else { to_string3(amf, v, 1, nl, &mut r) }))
        .collect();
    format!("{{{nl}{}{nl}}}", items.join(if amf0 { ",\n" } else { ", \n" }))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

type PResult<T> = std::result::Result<T, ParseError>;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Open,
    Close,
    BOpen,
    BClose,
    Comma,
    Colon,
    Str(String),
    Int(i64),
    Double(f64),
    Null,
    Undefined,
    Unknown,
    True,
    False,
    Ref(String),
    Ident(String),
    Eof,
}

/// The tokens of `amf.flex`, with their lines.
fn lex(src: &str) -> Vec<(Tok, usize)> {
    let c: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let ident_start = |ch: char| ch.is_alphabetic() || ch == '_' || ch == '$';
    let ident_part = |ch: char| ch.is_alphanumeric() || ch == '_' || ch == '$';
    while i < c.len() {
        let ch = c[i];
        if ch == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if ch == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if ch == '/' && c.get(i + 1) == Some(&'*') {
            i += 2;
            while i < c.len() && !(c[i] == '*' && c.get(i + 1) == Some(&'/')) {
                if c[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 2;
            continue;
        }
        let single = match ch {
            '{' => Some(Tok::Open),
            '}' => Some(Tok::Close),
            '[' => Some(Tok::BOpen),
            ']' => Some(Tok::BClose),
            ',' => Some(Tok::Comma),
            ':' => Some(Tok::Colon),
            _ => None,
        };
        if let Some(t) = single {
            out.push((t, line));
            i += 1;
            continue;
        }
        if ch == '"' {
            let mut s = String::new();
            i += 1;
            while i < c.len() && c[i] != '"' {
                if c[i] == '\n' {
                    break;
                }
                if c[i] == '\\' && i + 1 < c.len() {
                    let e = c[i + 1];
                    let hexn = |n: usize| -> Option<char> {
                        let h: String = c.get(i + 2..i + 2 + n)?.iter().collect();
                        u32::from_str_radix(&h, 16).ok().and_then(char::from_u32)
                    };
                    let (val, skip) = match e {
                        'b' => (Some('\u{8}'), 2),
                        't' => (Some('\t'), 2),
                        'n' => (Some('\n'), 2),
                        'f' => (Some('\u{c}'), 2),
                        'r' => (Some('\r'), 2),
                        '"' => (Some('"'), 2),
                        '\'' => (Some('\''), 2),
                        '\\' => (Some('\\'), 2),
                        'x' if hexn(2).is_some() => (hexn(2), 4),
                        'u' if hexn(4).is_some() => (hexn(4), 6),
                        _ => (Some('\\'), 1),
                    };
                    s.extend(val);
                    i += skip;
                    continue;
                }
                s.push(c[i]);
                i += 1;
            }
            i += 1;
            out.push((Tok::Str(s), line));
            continue;
        }
        if ch == '#' && c.get(i + 1).is_some_and(|&x| ident_start(x)) {
            let st = i + 1;
            i += 1;
            while i < c.len() && ident_part(c[i]) {
                i += 1;
            }
            out.push((Tok::Ref(c[st..i].iter().collect()), line));
            continue;
        }
        if ch.is_ascii_digit() || ch == '-' || ch == '.' {
            let st = i;
            if ch == '-' {
                i += 1;
            }
            if c.get(i) == Some(&'0') && matches!(c.get(i + 1), Some('x' | 'X')) {
                i += 2;
                let hs = i;
                while i < c.len() && c[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let h: String = c[hs..i].iter().collect();
                out.push((Tok::Int(i64::from_str_radix(&h, 16).unwrap_or(0)), line));
                continue;
            }
            while i < c.len() && c[i].is_ascii_digit() {
                i += 1;
            }
            let mut is_double = false;
            if c.get(i) == Some(&'.') {
                is_double = true;
                i += 1;
                while i < c.len() && c[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if matches!(c.get(i), Some('e' | 'E')) {
                let save = i;
                i += 1;
                if matches!(c.get(i), Some('+' | '-')) {
                    i += 1;
                }
                if c.get(i).is_some_and(|x| x.is_ascii_digit()) {
                    is_double = true;
                    while i < c.len() && c[i].is_ascii_digit() {
                        i += 1;
                    }
                } else {
                    i = save;
                }
            }
            let text: String = c[st..i].iter().collect();
            let tok = if is_double {
                Tok::Double(text.parse().unwrap_or(f64::NAN))
            } else if text.len() > 1 && text.trim_start_matches('-').starts_with('0') {
                Tok::Int(i64::from_str_radix(&text, 8).unwrap_or(0))
            } else {
                match text.parse::<i64>() {
                    Ok(v) => Tok::Int(v),
                    Err(_) => Tok::Double(text.parse().unwrap_or(f64::NAN)),
                }
            };
            out.push((tok, line));
            continue;
        }
        if ident_start(ch) {
            let st = i;
            while i < c.len() && ident_part(c[i]) {
                i += 1;
            }
            let w: String = c[st..i].iter().collect();
            let t = match w.as_str() {
                "null" => Tok::Null,
                "undefined" => Tok::Undefined,
                "unknown" => Tok::Unknown,
                "true" => Tok::True,
                "false" => Tok::False,
                "NaN" => Tok::Double(f64::NAN),
                "Infinity" => Tok::Double(f64::INFINITY),
                _ => Tok::Ident(w),
            };
            out.push((t, line));
            continue;
        }
        i += 1;
    }
    out.push((Tok::Eof, line));
    out
}

/// The JSON-like structure of FFDec's AMF text, before typed objects are built.
#[derive(Debug, Clone)]
enum Js {
    Obj(Vec<(Js, Js)>, usize),
    Arr(Vec<Js>),
    Lit(Value),
    Ref(String),
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    p: usize,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.p.min(self.toks.len() - 1)].0
    }
    fn line(&self) -> usize {
        self.toks[self.p.min(self.toks.len() - 1)].1
    }
    fn next(&mut self) -> Tok {
        let t = self.peek().clone();
        self.p += 1;
        t
    }
    fn fail<T>(&self, m: impl Into<String>) -> PResult<T> {
        Err(ParseError { line: self.line(), message: m.into() })
    }
    fn expect(&mut self, t: Tok, what: &str) -> PResult<()> {
        if *self.peek() == t {
            self.p += 1;
            Ok(())
        } else {
            self.fail(format!("{what} expected but {:?} found", self.peek()))
        }
    }

    fn value(&mut self) -> PResult<Js> {
        let line = self.line();
        Ok(match self.next() {
            Tok::Open => {
                let mut items = Vec::new();
                if *self.peek() != Tok::Close {
                    loop {
                        let k = self.value()?;
                        self.expect(Tok::Colon, "COLON")?;
                        let v = self.value()?;
                        items.push((k, v));
                        if *self.peek() == Tok::Comma {
                            self.p += 1;
                        } else {
                            break;
                        }
                    }
                }
                self.expect(Tok::Close, "CURLY_CLOSE")?;
                Js::Obj(items, line)
            }
            Tok::BOpen => {
                let mut items = Vec::new();
                if *self.peek() != Tok::BClose {
                    loop {
                        items.push(self.value()?);
                        if *self.peek() == Tok::Comma {
                            self.p += 1;
                        } else {
                            break;
                        }
                    }
                }
                self.expect(Tok::BClose, "BRACKET_CLOSE")?;
                Js::Arr(items)
            }
            Tok::Str(s) => Js::Lit(Value::Str(s)),
            Tok::Int(i) => Js::Lit(Value::Int(i)),
            Tok::Double(d) => Js::Lit(Value::Double(d)),
            Tok::Null => Js::Lit(Value::Null),
            Tok::Undefined => Js::Lit(Value::Undefined),
            Tok::Unknown => Js::Lit(Value::Unknown),
            Tok::True => Js::Lit(Value::Bool(true)),
            Tok::False => Js::Lit(Value::Bool(false)),
            Tok::Ref(r) => Js::Ref(r),
            t => {
                self.p -= 1;
                return self.fail(format!("unexpected symbol {t:?}"));
            }
        })
    }
}

/// Builds typed objects from parsed text (`resolveObjects` and `replaceReferences`).
struct Build {
    amf: Amf,
    ids: HashMap<String, usize>,
    pending: Vec<(String, usize)>,
    amf0: bool,
}

const PENDING: usize = usize::MAX / 2;

impl Build {
    fn get<'a>(items: &'a [(Js, Js)], key: &str) -> Option<&'a Js> {
        items.iter().find(|(k, _)| matches!(k, Js::Lit(Value::Str(s)) if s == key)).map(|p| &p.1)
    }

    fn need<'a>(items: &'a [(Js, Js)], key: &str, line: usize) -> PResult<&'a Js> {
        Build::get(items, key).ok_or_else(|| ParseError { line, message: format!("\"{key}\" is missing") })
    }

    fn string(items: &[(Js, Js)], key: &str, line: usize) -> PResult<String> {
        match Build::need(items, key, line)? {
            Js::Lit(Value::Str(s)) => Ok(s.clone()),
            _ => Err(ParseError { line, message: format!("\"{key}\" value must be of type String") }),
        }
    }

    fn boolean(items: &[(Js, Js)], key: &str, line: usize) -> PResult<bool> {
        match Build::need(items, key, line)? {
            Js::Lit(Value::Bool(b)) => Ok(*b),
            _ => Err(ParseError { line, message: format!("\"{key}\" value must be of type Boolean") }),
        }
    }

    fn reference(&mut self, id: String) -> Value {
        self.pending.push((id, 0));
        Value::Obj(PENDING + self.pending.len() - 1)
    }

    fn members(&mut self, items: &[(Js, Js)], key: &str, line: usize) -> PResult<Vec<(String, Value)>> {
        match Build::need(items, key, line)? {
            Js::Obj(m, _) => {
                let mut out = Vec::new();
                for (k, v) in m {
                    if let Js::Lit(Value::Str(k)) = k {
                        out.push((k.clone(), self.value(v)?));
                    }
                }
                Ok(out)
            }
            _ => Err(ParseError { line, message: format!("\"{key}\" value must be of type JsObject") }),
        }
    }

    fn array(&mut self, items: &[(Js, Js)], key: &str, line: usize) -> PResult<Vec<Value>> {
        match Build::need(items, key, line)? {
            Js::Arr(a) => a.iter().map(|v| self.value(v)).collect(),
            _ => Err(ParseError { line, message: format!("\"{key}\" value must be of type JsArray") }),
        }
    }

    fn number(&self, v: Value) -> Value {
        match v {
            Value::Int(i) if self.amf0 => Value::Double(i as f64),
            v => v,
        }
    }

    fn value(&mut self, js: &Js) -> PResult<Value> {
        let (items, line) = match js {
            Js::Lit(v) => return Ok(self.number(v.clone())),
            Js::Ref(r) => return Ok(self.reference(r.clone())),
            Js::Arr(_) => return Err(ParseError { line: 0, message: "unexpected array".into() }),
            Js::Obj(items, line) => (items, *line),
        };
        let line = line;
        let ty = Build::string(items, "type", line)?;
        let id = match Build::get(items, "id") {
            Some(Js::Lit(Value::Str(s))) => Some(s.clone()),
            _ => None,
        };
        if ty == "Reference" {
            return Ok(self.reference(Build::string(items, "referencedId", line)?));
        }
        if ty == "Undefined" {
            return Ok(Value::Undefined);
        }
        let slot = self.amf.add(Obj::Xml(String::new()));
        if let Some(id) = id {
            self.ids.insert(id, slot);
        }
        let date = |s: String| parse_date(&s).ok_or(ParseError { line, message: format!("Invalid date format: {s}") });
        let has = |k: &str| Build::get(items, k).is_some();
        let amf3_inside = self.amf0 && match ty.as_str() {
            "Object" => has("sealedMembers"),
            "Array" => has("denseValues"),
            "Date" => !has("timezone"),
            "XML" | "Vector" | "ByteArray" | "Dictionary" => true,
            _ => false,
        };
        let outer = self.amf0;
        if amf3_inside {
            self.amf0 = false;
        }
        let o = self.typed(&ty, items, line, date);
        self.amf0 = outer;
        self.amf.objs[slot] = o?;
        Ok(Value::Obj(slot))
    }

    fn typed(&mut self, ty: &str, items: &[(Js, Js)], line: usize, date: impl Fn(String) -> PResult<f64>) -> PResult<Obj> {
        Ok(match (ty, self.amf0) {
            ("Date", false) => Obj::Date(date(Build::string(items, "value", line)?)?),
            ("Date", true) => {
                let tz = match Build::get(items, "timezone") {
                    Some(Js::Lit(Value::Int(i))) => *i as i16,
                    Some(Js::Lit(Value::Double(d))) => *d as i16,
                    _ => 0,
                };
                Obj::Date0 { millis: date(Build::string(items, "value", line)?)?, timezone: tz }
            }
            ("XML", _) => Obj::Xml(Build::string(items, "value", line)?),
            ("XMLDocument", false) => Obj::XmlDoc(Build::string(items, "value", line)?),
            ("XMLDocument", true) => Obj::XmlDoc0(Build::string(items, "value", line)?),
            ("Object", true) => Obj::Object0(self.members(items, "members", line)?),
            ("TypedObject", _) => Obj::TypedObject0 { class: Build::string(items, "className", line)?, members: self.members(items, "members", line)? },
            ("EcmaArray", _) => Obj::EcmaArray { dense: self.members(items, "denseValues", line)?, assoc: self.members(items, "associativeValues", line)? },
            ("Array", true) => Obj::StrictArray(self.array(items, "values", line)?),
            ("Object", false) => {
                let class = Build::string(items, "className", line)?;
                if Build::get(items, "serialized").is_some() {
                    return Err(ParseError { line, message: "serialized objects can not be imported".into() });
                }
                Obj::Object { class, dynamic: Build::boolean(items, "dynamic", line)?, sealed: self.members(items, "sealedMembers", line)?, members: self.members(items, "dynamicMembers", line)? }
            }
            ("Array", false) => Obj::Array { dense: self.array(items, "denseValues", line)?, assoc: self.members(items, "associativeValues", line)? },
            ("Vector", _) => {
                let fixed = Build::boolean(items, "fixed", line)?;
                let sub = Build::string(items, "subtype", line)?;
                let vals = self.array(items, "values", line)?;
                let bad = |what: &str| ParseError { line, message: format!("Not {what}") };
                match sub.as_str() {
                    "int" | "uint" => {
                        let mut out = Vec::new();
                        for v in vals {
                            match v {
                                Value::Int(i) if sub == "int" || i >= 0 => out.push(i),
                                _ => return Err(bad(if sub == "int" { "an Integer value" } else { "an Unsigned Integer value" })),
                            }
                        }
                        if sub == "int" { Obj::VectorInt { fixed, values: out } } else { Obj::VectorUint { fixed, values: out } }
                    }
                    "Number" => Obj::VectorDouble {
                        fixed,
                        values: vals
                            .into_iter()
                            .map(|v| match v {
                                Value::Int(i) => Ok(i as f64),
                                Value::Double(d) => Ok(d),
                                _ => Err(bad("a Number")),
                            })
                            .collect::<PResult<_>>()?,
                    },
                    _ => Obj::VectorObject { fixed, type_name: sub, values: vals },
                }
            }
            ("ByteArray", _) => {
                let h = Build::string(items, "value", line)?;
                let b = (0..h.len() / 2).map(|i| u8::from_str_radix(h.get(i * 2..i * 2 + 2).unwrap_or("zz"), 16)).collect::<std::result::Result<Vec<_>, _>>();
                Obj::ByteArray(b.map_err(|_| ParseError { line, message: "Invalid hex byte sequence".into() })?)
            }
            ("Dictionary", _) => {
                let weak = Build::boolean(items, "weakKeys", line)?;
                let Js::Arr(list) = Build::need(items, "entries", line)? else {
                    return Err(ParseError { line, message: "\"entries\" value must be of type JsArray".into() });
                };
                let mut entries = Vec::new();
                for e in list {
                    let Js::Obj(kv, l) = e else { return Err(ParseError { line, message: "Invalid dictionary entry".into() }) };
                    let k = self.value(Build::need(kv, "key", *l)?)?;
                    let v = self.value(Build::need(kv, "value", *l)?)?;
                    entries.push((k, v));
                }
                Obj::Dictionary { weak, entries }
            }
            _ => return Err(ParseError { line, message: format!("Unknown object type: {ty}") }),
        })
    }

    fn fix(&mut self, v: &mut Value) -> PResult<()> {
        if let Value::Obj(id) = v {
            if *id >= PENDING {
                let key = &self.pending[*id - PENDING].0;
                *id = *self.ids.get(key).ok_or_else(|| ParseError { line: 0, message: format!("Reference to undefined object: #{key}") })?;
            }
        }
        Ok(())
    }

    fn finish(&mut self, roots: &mut [&mut Value]) -> PResult<()> {
        for r in roots.iter_mut() {
            self.fix(r)?;
        }
        let mut objs = std::mem::take(&mut self.amf.objs);
        for o in &mut objs {
            for v in o.values_mut() {
                self.fix(v)?;
            }
        }
        self.amf.objs = objs;
        Ok(())
    }
}

/// `Amf3Importer.stringToAmf`.
pub fn from_text(src: &str) -> PResult<(Amf, Value)> {
    let mut p = Parser { toks: lex(src), p: 0 };
    let js = p.value()?;
    let mut b = Build { amf: Amf::default(), ids: HashMap::new(), pending: Vec::new(), amf0: false };
    let mut v = b.value(&js)?;
    b.finish(&mut [&mut v])?;
    Ok((b.amf, v))
}

/// `stringToAmfMap` for AMF3 or AMF0 variables.
pub fn parse_map(src: &str, amf0: bool) -> PResult<(Amf, Vec<(String, Value)>)> {
    let mut p = Parser { toks: lex(src), p: 0 };
    p.expect(Tok::Open, "CURLY_OPEN")?;
    let mut raw = Vec::new();
    while let Tok::Str(k) = p.peek().clone() {
        p.p += 1;
        p.expect(Tok::Colon, "COLON")?;
        raw.push((k, p.value()?));
        if *p.peek() == Tok::Comma {
            p.p += 1;
        } else {
            break;
        }
    }
    p.expect(Tok::Close, "CURLY_CLOSE")?;
    let mut b = Build { amf: Amf::default(), ids: HashMap::new(), pending: Vec::new(), amf0 };
    let mut vars = Vec::new();
    for (k, js) in &raw {
        vars.push((k.clone(), b.value(js)?));
    }
    let mut roots: Vec<&mut Value> = vars.iter_mut().map(|p| &mut p.1).collect();
    b.finish(&mut roots)?;
    Ok((b.amf, vars))
}
