//! Iggy streams: a growable little-endian buffer that can record the relocation
//! program of what is written (`IggyIndexBuilder`), and the program parser and encoder
//! (`IggyFixup`).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::error::{Error, Result};

pub fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error::Other(msg.into()))
}

/// `MemoryDataStream`, optionally recording an index program while writing.
#[derive(Default, Clone)]
pub struct Stream {
    pub data: Vec<u8>,
    pub pos: usize,
    read_only: bool,
    pub index: Option<IndexBuilder>,
}

impl Stream {
    pub fn new() -> Stream {
        Stream::default()
    }

    pub fn reader(data: &[u8]) -> Stream {
        Stream { data: data.to_vec(), pos: 0, read_only: true, index: None }
    }

    pub fn indexed() -> Stream {
        Stream { index: Some(IndexBuilder::new()), ..Default::default() }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn available(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn idx(&mut self) -> &mut IndexBuilder {
        self.index.get_or_insert_with(IndexBuilder::new)
    }

    pub fn read(&mut self) -> Result<u8> {
        let Some(&b) = self.data.get(self.pos) else { return err("Unexpected end of Iggy data") };
        self.pos += 1;
        Ok(b)
    }

    pub fn u8(&mut self) -> Result<u32> {
        Ok(self.read()? as u32)
    }

    pub fn u16(&mut self) -> Result<u32> {
        Ok(self.read()? as u32 | (self.read()? as u32) << 8)
    }

    pub fn i16(&mut self) -> Result<i32> {
        Ok(self.u16()? as u16 as i16 as i32)
    }

    pub fn u32(&mut self) -> Result<u64> {
        let mut v = 0u64;
        for i in 0..4 {
            v |= (self.read()? as u64) << (8 * i);
        }
        Ok(v)
    }

    pub fn i32(&mut self) -> Result<i64> {
        Ok(self.u32()? as u32 as i32 as i64)
    }

    pub fn u64(&mut self) -> Result<u64> {
        Ok(self.u32()? | self.u32()? << 32)
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()? as u32))
    }

    pub fn bytes(&mut self, n: usize) -> Result<Vec<u8>> {
        if n > self.available() {
            return err("Invalid Iggy byte range");
        }
        let v = self.data[self.pos..self.pos + n].to_vec();
        self.pos += n;
        Ok(v)
    }

    pub fn write(&mut self, b: u8) {
        if self.pos >= self.data.len() {
            self.data.resize(self.pos + 1, 0);
        }
        self.data[self.pos] = b;
        self.pos += 1;
    }

    pub fn write_bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.write(x);
        }
    }

    pub fn w8(&mut self, v: u32) {
        self.write(v as u8);
    }

    pub fn w16(&mut self, v: u32) {
        self.write(v as u8);
        self.write((v >> 8) as u8);
    }

    pub fn w32(&mut self, v: u64) {
        for i in 0..4 {
            self.write((v >> (8 * i)) as u8);
        }
    }

    pub fn w64(&mut self, v: u64) {
        for i in 0..8 {
            self.write((v >> (8 * i)) as u8);
        }
    }

    pub fn wf32(&mut self, v: f32) {
        self.w32(v.to_bits() as u64);
    }

    pub fn seek(&mut self, pos: u64) -> Result<()> {
        if pos > i32::MAX as u64 || (self.read_only && pos as usize > self.data.len()) {
            return err(format!("Invalid Iggy seek: {pos}"));
        }
        if !self.read_only && pos as usize > self.data.len() {
            self.data.resize(pos as usize, 0);
        }
        self.pos = pos as usize;
        Ok(())
    }

    pub fn position(&self) -> u64 {
        self.pos as u64
    }

    pub fn pad8(&mut self) {
        if self.read_only {
            self.pos += (8 - self.pos % 8) % 8;
            return;
        }
        while self.pos % 8 != 0 {
            self.write(0);
        }
    }

    /// `readWChar`: a zero-terminated UTF-16 string.
    pub fn wchar(&mut self) -> Result<String> {
        let mut units = Vec::new();
        loop {
            let v = self.u16().map_err(|_| Error::Other("Unterminated Iggy UTF-16 string".into()))?;
            if v == 0 {
                break;
            }
            units.push(v as u16);
        }
        Ok(String::from_utf16_lossy(&units))
    }

    pub fn write_wchar(&mut self, s: &str) {
        for u in s.encode_utf16() {
            self.w16(u as u32);
        }
        self.w16(0);
    }

    /// `IggyLayout.readPointer`: a self-relative offset, 1 for null.
    pub fn pointer(&mut self, is64: bool) -> Result<u64> {
        let p = self.position();
        let v = if is64 { self.u64()? } else { self.u32()? };
        Ok(if v == 1 {
            0
        } else if is64 {
            p.wrapping_add(v)
        } else {
            p.wrapping_add(v) & 0xffff_ffff
        })
    }

    pub fn write_pointer(&mut self, is64: bool, target: u64) {
        let v = if target == 0 { 1 } else { target.wrapping_sub(self.position()) };
        if is64 {
            self.w64(v);
        } else {
            self.w32(v & 0xffff_ffff);
        }
    }
}

/// `IggyIndexBuilder`: the relocation program of written data.
#[derive(Clone)]
pub struct IndexBuilder {
    consts: Vec<(u8, Vec<u8>, Vec<u8>)>,
    out: Vec<u8>,
    position: u64,
}

pub const CONST_SHAPE_NODE_SIZE: u32 = 0;
pub const CONST_KERNING_RECORD_SIZE: u32 = 1;
pub const CONST_CHAR_OFFSET_SIZE: u32 = 2;
pub const CONST_BIN_INFO_SIZE: u32 = 3;
pub const CONST_TYPE_INFO_SIZE: u32 = 4;
pub const CONST_SHAPE_SIZE: u32 = 5;
pub const CONST_GENERAL_FONT_INFO_SIZE: u32 = 6;
pub const CONST_GENERAL_FONT_INFO2_SIZE: u32 = 7;
pub const CONST_TEXT_DATA_SIZE: u32 = 8;
pub const CONST_SEQUENCE_SIZE: u32 = 9;

impl Default for IndexBuilder {
    fn default() -> Self {
        IndexBuilder::new()
    }
}

impl IndexBuilder {
    pub fn new() -> IndexBuilder {
        let c = |size: u8, o: &[u8], t: &[u8]| (size, o.to_vec(), t.to_vec());
        IndexBuilder {
            consts: vec![
                c(24, &[0, 4, 8, 0xC, 0x12, 0x14, 0x16], &[5, 5, 5, 5, 4, 4, 4]),
                c(6, &[0, 2, 4], &[4, 4, 4]),
                c(32, &[8, 0x10, 0x18], &[4, 8, 2]),
                c(96, &[0, 9, 0xC, 0x10, 0x14, 0x18, 0x1C, 0x20, 0x24, 0x28, 0x30, 0x38, 0x3A], &[2, 3, 5, 5, 5, 5, 5, 5, 5, 4, 2, 4, 4]),
                c(24, &[8, 0x10], &[2, 5]),
                c(64, &[0, 4, 8, 0xC, 0x10, 0x18, 0x20, 0x28, 0x30], &[5, 5, 5, 5, 2, 5, 2, 2, 2]),
                c(112, &[0x1, 0x02, 0x08, 0x10, 0x20, 0x22, 0x24, 0x26, 0x2B, 0x30, 0x38, 0x40, 0x48, 0x4C, 0x50, 0x54, 0x58, 0x60], &[0xC, 4, 5, 2, 4, 4, 4, 4, 3, 2, 2, 2, 5, 5, 5, 5, 5, 2]),
                c(240, &[0x10], &[2]),
                c(104, &[0x01, 0x02, 0x08, 0x10, 0x20, 0x24, 0x28, 0x2C, 0x30, 0x32, 0x38, 0x44, 0x46, 0x48, 0x4A, 0x4C, 0x4E, 0x60], &[0x0C, 4, 5, 2, 5, 5, 5, 5, 4, 4, 2, 4, 4, 4, 4, 4, 4, 2]),
                c(16, &[0, 8], &[2, 5]),
            ],
            out: Vec::new(),
            position: 0,
        }
    }

    /// The table of templates followed by the program.
    pub fn bytes(&self) -> Vec<u8> {
        let mut v = vec![self.consts.len() as u8];
        for (size, offs, types) in &self.consts {
            v.push(*size);
            v.push(offs.len() as u8);
            for (o, t) in offs.iter().zip(types) {
                v.push(*o);
                v.push(*t);
            }
        }
        v.extend_from_slice(&self.out);
        v
    }

    pub fn position(&self) -> u64 {
        self.position
    }

    pub fn const_length(&mut self, i: u32) -> u64 {
        self.index(i, false, 0, &[], &[])
    }

    pub fn const_length_array(&mut self, i: u32, count: u64) -> u64 {
        let mut ret = 0;
        let mut rem = count;
        loop {
            if rem == 0 {
                break;
            }
            if rem == 1 {
                ret += self.index(i, false, 0, &[], &[]);
                break;
            }
            if rem <= 64 {
                ret += self.index(0x80 + rem as u32 - 1, false, i as u64, &[], &[]);
                break;
            }
            rem -= 64;
            ret += self.index(0x80 + 63, false, i as u64, &[], &[]);
        }
        ret
    }

    pub fn pad8(&mut self) {
        match self.position % 8 {
            2 => {
                self.padding(2);
            }
            4 => {
                self.padding(1);
            }
            6 => {
                self.padding(0);
            }
            _ => {}
        }
    }

    pub fn padding(&mut self, n: u32) -> u64 {
        self.index(0xC0 + n, false, 0, &[], &[])
    }

    fn array(&mut self, op: u32, is64: bool, mut count: u64) -> u64 {
        let mut r = 0;
        while count > 0 {
            let batch = count.min(256);
            r += self.index(0xD0 + op, is64, batch - 1, &[], &[]);
            count -= batch;
        }
        r
    }

    pub fn pointer_array(&mut self, is64: bool, count: u64) -> u64 {
        self.array(2, is64, count)
    }

    pub fn array16(&mut self, count: u64) -> u64 {
        self.array(4, false, count)
    }

    pub fn array32(&mut self, count: u64) -> u64 {
        self.array(5, false, count)
    }

    pub fn array64(&mut self, count: u64) -> u64 {
        self.array(6, false, count)
    }

    pub fn skip_one(&mut self) -> u64 {
        self.index(0xFC, false, 0, &[], &[])
    }

    pub fn custom(&mut self, total: u32, offsets: &[u32], types: &[u32]) -> u64 {
        let o: Vec<u8> = offsets.iter().map(|&x| x as u8).collect();
        let t: Vec<u8> = types.iter().map(|&x| x as u8).collect();
        self.index(0xFD, false, total as u64, &o, &t)
    }

    pub fn byte_positive(&mut self, v: u32) -> u64 {
        self.index(0xFE, false, v as u64, &[], &[])
    }

    pub fn ui32(&mut self, v: u64) -> u64 {
        self.index(0xFF, false, v, &[], &[])
    }

    fn index(&mut self, code: u32, is64: bool, val: u64, offsets: &[u8], types: &[u8]) -> u64 {
        self.out.push(code as u8);
        let add = |s: &mut Self, n: u64| {
            s.position += n;
            n
        };
        if code < 0x80 {
            match self.consts.get(code as usize) {
                Some(c) => {
                    let n = c.0 as u64;
                    add(self, n)
                }
                None => 0,
            }
        } else if code < 0xC0 {
            self.out.push(val as u8);
            match self.consts.get(val as usize) {
                Some(c) => {
                    let n = c.0 as u64 * (code - 0x7F) as u64;
                    add(self, n)
                }
                None => 0,
            }
        } else if code < 0xD0 {
            add(self, (code * 2 - 0x17E) as u64)
        } else if code < 0xE0 {
            let i = code & 0xF;
            self.out.push(val as u8);
            let n = val + 1;
            let w = match (is64, i) {
                (true, 0..=2) => 8,
                (true, 3 | 4) => 2,
                (true, 5) => 4,
                (true, 6) => 8,
                (false, 2) => 4,
                (false, 4) => 2,
                (false, 5) => 4,
                (false, 6) => 8,
                _ => 0,
            };
            add(self, w * n)
        } else if code == 0xFC {
            self.out.push(0);
            0
        } else if code == 0xFD {
            self.out.push(val as u8);
            self.out.push(offsets.len() as u8);
            for (o, t) in offsets.iter().zip(types) {
                self.out.push(*o);
                self.out.push(*t);
            }
            add(self, val)
        } else if code == 0xFE {
            self.out.push(val as u8);
            add(self, val + 1)
        } else if code == 0xFF {
            self.out.extend_from_slice(&(val as u32).to_le_bytes());
            add(self, val)
        } else {
            0
        }
    }
}

/// One relocated field: its offset and operation (2 pointer, 4 word, 5 dword, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub offset: u64,
    pub op: u32,
}

pub fn field(offset: u64, op: u32) -> Field {
    Field { offset, op }
}

struct Template {
    advance: u64,
    ops: Vec<(u64, u32)>,
}

fn width(op: u32, ptr: u64) -> Result<u64> {
    Ok(match op {
        2 | 8..=11 => ptr,
        3 | 12..=15 => 1,
        4 => 2,
        5 => 4,
        6 => 8,
        _ => return err(format!("Invalid Iggy fixup operation: {op}")),
    })
}

fn byte(s: &mut Stream) -> Result<u32> {
    s.u8().map_err(|_| Error::Other("Truncated Iggy fixup program".into()))
}

fn template(s: &mut Stream) -> Result<Template> {
    let advance = byte(s)? as u64;
    let n = byte(s)?;
    let mut ops = Vec::new();
    for _ in 0..n {
        let o = byte(s)? as u64;
        let op = byte(s)?;
        width(op, 8)?;
        ops.push((o, op));
    }
    Ok(Template { advance, ops })
}

/// `IggyFixup`: the fields a relocation program touches.
pub struct Fixup {
    pub fields: Vec<Field>,
    pub cursor: u64,
    target: u64,
    ptr: u64,
}

impl Fixup {
    fn field(&mut self, pos: u64, op: u32) -> Result<()> {
        let size = width(op, self.ptr)?;
        if pos.checked_add(size).is_none_or(|e| e > self.target) {
            return err("Iggy fixup outside target block");
        }
        self.fields.push(Field { offset: pos, op });
        Ok(())
    }

    fn advance(&mut self, n: u64) -> Result<()> {
        if n > self.target - self.cursor {
            return err("Iggy fixup cursor outside target block");
        }
        self.cursor += n;
        Ok(())
    }

    fn apply(&mut self, t: &Template) -> Result<()> {
        for &(o, op) in &t.ops {
            self.field(self.cursor + o, op)?;
        }
        self.advance(t.advance)
    }

    pub fn parse(is64: bool, program: &[u8], target: u64) -> Result<Fixup> {
        let mut s = Stream::reader(program);
        let mut r = Fixup { fields: Vec::new(), cursor: 0, target, ptr: if is64 { 8 } else { 4 } };
        let n = byte(&mut s)?;
        let mut templates = Vec::new();
        for _ in 0..n {
            templates.push(template(&mut s)?);
        }
        while s.available() > 0 {
            let code = byte(&mut s)?;
            if code < 0xC0 {
                let repeats = if code < 0x80 { 1 } else { code - 127 };
                let index = if code < 0x80 { code } else { byte(&mut s)? } as usize;
                let Some(t) = templates.get(index) else { return err("Invalid Iggy fixup template index") };
                for _ in 0..repeats {
                    r.apply(t)?;
                }
            } else if code < 0xD0 {
                r.advance((2 * code - 382) as u64)?;
            } else if matches!(code, 0xD2 | 0xD4 | 0xD5 | 0xD6) {
                let count = byte(&mut s)? + 1;
                let op = code & 15;
                let size = width(op, r.ptr)?;
                for _ in 0..count {
                    r.field(r.cursor, op)?;
                    r.advance(size)?;
                }
            } else if code == 0xFC {
                byte(&mut s)?;
            } else if code == 0xFD {
                let t = template(&mut s)?;
                r.apply(&t)?;
            } else if code == 0xFE {
                let n = byte(&mut s)? as u64 + 1;
                r.advance(n)?;
            } else if code == 0xFF {
                let n = s.u32().map_err(|_| Error::Other("Truncated Iggy cursor advance".into()))?;
                r.advance(n)?;
            } else {
                return err(format!("Invalid Iggy fixup opcode: {code}"));
            }
        }
        Ok(r)
    }

    /// `normalize`: big-endian or non-native bitfield payloads to the native layout.
    pub fn normalize(&self, data: &mut [u8], little_endian: bool, native_bitfields: bool) -> Result<()> {
        for f in &self.fields {
            let p = f.offset as usize;
            if f.op == 3 && !native_bitfields {
                data[p] = data[p].reverse_bits();
            } else if !little_endian && (f.op == 2 || (4..=6).contains(&f.op)) {
                let size = width(f.op, self.ptr)? as usize;
                data[p..p + size].reverse();
            }
        }
        Ok(())
    }
}

fn skip(out: &mut Vec<u8>, distance: u64) -> Result<()> {
    if distance > 0xffff_ffff {
        return err("Invalid Iggy fixup skip");
    }
    if distance == 0 {
        return Ok(());
    }
    if distance <= 32 && distance % 2 == 0 {
        out.push((191 + distance / 2) as u8);
    } else if distance <= 256 {
        out.push(0xfe);
        out.push((distance - 1) as u8);
    } else {
        out.push(0xff);
        out.extend_from_slice(&(distance as u32).to_le_bytes());
    }
    Ok(())
}

fn encode(fields: &[Field], size: u64) -> Result<Option<Vec<u8>>> {
    let mut keys = vec![0u32; fields.len()];
    let mut counts: HashMap<u32, u32> = HashMap::new();
    let mut zero: BTreeSet<u32> = BTreeSet::new();
    for (i, f) in fields.iter().enumerate() {
        let gap = if i + 1 < fields.len() { fields[i + 1].offset as i64 - f.offset as i64 } else { 0 };
        if gap < 0 {
            return Ok(None);
        }
        let key = f.op * 256 + if gap <= 255 { gap as u32 } else { 0 };
        keys[i] = key;
        *counts.entry(key).or_insert(0) += 1;
        zero.insert(f.op * 256);
    }
    let mut templates: Vec<u32> = zero.into_iter().collect();
    let mut cands: Vec<u32> = counts.keys().copied().collect();
    cands.sort_by(|a, b| counts[b].cmp(&counts[a]).then(a.cmp(b)));
    for k in cands {
        if templates.len() < 128 && !templates.contains(&k) {
            templates.push(k);
        }
    }
    let mut indices: HashMap<u32, usize> = HashMap::new();
    let mut out = vec![templates.len() as u8];
    for (i, &k) in templates.iter().enumerate() {
        indices.insert(k, i);
        out.extend_from_slice(&[(k & 255) as u8, 1, 0, (k / 256) as u8]);
    }
    let mut cursor = 0u64;
    let mut i = 0;
    while i < fields.len() {
        let f = fields[i];
        skip(&mut out, f.offset - cursor)?;
        cursor = f.offset;
        let mut key = keys[i];
        if !indices.contains_key(&key) {
            key = f.op * 256;
        }
        let adv = (key & 255) as u64;
        let mut repeats = 1;
        while repeats < 64 && i + repeats < fields.len() && keys[i + repeats] == key && fields[i + repeats].offset == cursor + repeats as u64 * adv {
            repeats += 1;
        }
        if repeats > 1 {
            out.push((127 + repeats) as u8);
        }
        out.push(indices[&key] as u8);
        cursor += repeats as u64 * adv;
        i += repeats;
    }
    skip(&mut out, size - cursor)?;
    Ok(Some(out))
}

pub const NATIVE_BUFFER_SIZE: usize = 65536;

fn split(is64: bool, program: &[u8], size: u64) -> Result<Vec<Vec<u8>>> {
    if program.len() <= NATIVE_BUFFER_SIZE {
        return Ok(vec![program.to_vec()]);
    }
    let mut s = Stream::reader(program);
    let n = byte(&mut s)?;
    let mut templates = Vec::new();
    for _ in 0..n {
        templates.push(template(&mut s)?);
    }
    let header = s.pos;
    let mut chunk = program[..header].to_vec();
    let mut result = Vec::new();
    let mut cursor = 0u64;
    let ptr = if is64 { 8 } else { 4 };
    while s.available() > 0 {
        let start = s.pos;
        let before = cursor;
        let code = byte(&mut s)?;
        if code < 0xc0 {
            let repeats = if code < 128 { 1 } else { code - 127 };
            let index = if code < 128 { code } else { byte(&mut s)? } as usize;
            cursor += repeats as u64 * templates[index].advance;
        } else if code < 0xd0 {
            cursor += (2 * code - 382) as u64;
        } else if matches!(code, 0xd2 | 0xd4 | 0xd5 | 0xd6) {
            cursor += (byte(&mut s)? as u64 + 1) * width(code & 15, ptr)?;
        } else if code == 0xfc {
            byte(&mut s)?;
        } else if code == 0xfd {
            cursor += template(&mut s)?.advance;
        } else if code == 0xfe {
            cursor += byte(&mut s)? as u64 + 1;
        } else if code == 0xff {
            cursor += s.u32()?;
        } else {
            return err(format!("Invalid Iggy fixup opcode: {code}"));
        }
        let length = s.pos - start;
        if chunk.len() + length > NATIVE_BUFFER_SIZE {
            result.push(std::mem::take(&mut chunk));
            chunk = program[..header].to_vec();
            skip(&mut chunk, before)?;
        }
        if chunk.len() + length > NATIVE_BUFFER_SIZE || cursor > size {
            return err("Iggy fixup instruction exceeds native buffer or target");
        }
        chunk.extend_from_slice(&program[start..s.pos]);
    }
    result.push(chunk);
    Ok(result)
}

/// `IggyFixup.compact`: a smaller program when one exists, split into native-size chunks.
pub fn compact(is64: bool, program: &[u8], size: u64) -> Result<Vec<Vec<u8>>> {
    let expected = Fixup::parse(is64, program, size)?.fields;
    let encoded = encode(&expected, size)?;
    let chosen = match &encoded {
        Some(e) if e.len() < program.len() => e.as_slice(),
        _ => program,
    };
    let chunks = split(is64, chosen, size)?;
    let mut next = 0;
    for c in &chunks {
        for f in Fixup::parse(is64, c, size)?.fields {
            if expected.get(next) != Some(&f) {
                return err("Compacted Iggy fixup changed an operation");
            }
            next += 1;
        }
    }
    if next != expected.len() {
        return err("Compacted Iggy fixup lost operations");
    }
    Ok(chunks)
}

/// `IggyFixup.writeFields`: fields as a program, sorted by offset.
pub fn write_fields(b: &mut IndexBuilder, fields: &[Field], size: u64) -> Result<()> {
    let mut ordered = fields.to_vec();
    ordered.sort_by_key(|f| f.offset);
    let mut cursor = 0;
    for f in ordered {
        if f.offset < cursor || f.offset >= size {
            return err("Invalid output fixup position");
        }
        if f.offset > cursor {
            b.ui32(f.offset - cursor);
            cursor = f.offset;
        }
        b.custom(0, &[0], &[f.op]);
    }
    if size > cursor {
        b.ui32(size - cursor);
    }
    Ok(())
}

/// A field map ordered by insertion, as the Java code's `LinkedHashMap<Long, Field>`.
#[derive(Default, Clone)]
pub struct Fields {
    order: Vec<u64>,
    map: BTreeMap<u64, Field>,
}

impl Fields {
    pub fn put(&mut self, f: Field) {
        if self.map.insert(f.offset, f).is_none() {
            self.order.push(f.offset);
        }
    }

    pub fn remove(&mut self, offset: u64) {
        if self.map.remove(&offset).is_some() {
            self.order.retain(|&o| o != offset);
        }
    }

    pub fn values(&self) -> Vec<Field> {
        self.order.iter().map(|o| self.map[o]).collect()
    }
}
