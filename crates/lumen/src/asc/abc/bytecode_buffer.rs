//! Port of `macromedia.abc.BytecodeBuffer` (with its package-private `ByteArrayPool` and `ByteArray` family).

use std::cell::Cell;
use std::collections::HashMap;
use std::io::{self, Write};

use crate::asc::emit::constants::*;

/// Reads move a shared cursor (`pos`) through `&self`, like the Java object shared by the decoder and its pools.
#[derive(Debug, Clone)]
pub struct BytecodeBuffer {
    bytecodes: Vec<u8>,
    pos: Cell<i32>,
    size: i32,
}

impl BytecodeBuffer {
    pub fn from_bytes(bytecodes: Vec<u8>) -> BytecodeBuffer {
        let size = bytecodes.len() as i32;
        BytecodeBuffer { bytecodes, pos: Cell::new(0), size }
    }

    pub fn from_file(filename: &str) -> io::Result<BytecodeBuffer> {
        Ok(BytecodeBuffer::from_bytes(std::fs::read(filename)?))
    }

    pub fn with_size(preferred_size: i32) -> BytecodeBuffer {
        let preferred_size = if preferred_size <= 0 { 1000 } else { preferred_size };
        BytecodeBuffer { bytecodes: vec![0; preferred_size as usize], pos: Cell::new(0), size: 0 }
    }

    pub fn clear(&mut self) {
        self.pos.set(0);
        self.size = 0;
    }

    pub fn pos(&self) -> i32 {
        self.pos.get()
    }

    pub fn size(&self) -> i32 {
        self.size
    }

    pub fn delete(&mut self, count: i32) {
        self.size -= count;
    }

    pub fn write_s8(&mut self, v: i32) {
        self.write_u8(v);
    }

    pub fn write_u8(&mut self, v: i32) {
        self.resize(1);
        self.push(v as u8);
    }

    pub fn write_u8_at(&mut self, pos: i32, v: i32) {
        self.bytecodes[pos as usize] = v as u8;
    }

    pub fn write_u16(&mut self, v: i32) {
        self.resize(2);
        self.push(v as u8);
        self.push((v >> 8) as u8);
    }

    pub fn write_s24(&mut self, v: i32) {
        self.write_u24(v);
    }

    pub fn write_s24_at(&mut self, pos: i32, v: i32) {
        let p = pos as usize;
        self.bytecodes[p] = v as u8;
        self.bytecodes[p + 1] = (v >> 8) as u8;
        self.bytecodes[p + 2] = (v >> 16) as u8;
    }

    pub fn write_u24(&mut self, v: i32) {
        self.resize(3);
        self.push(v as u8);
        self.push((v >> 8) as u8);
        self.push((v >> 16) as u8);
    }

    pub fn write_u32(&mut self, v: impl Into<i64>) {
        let v: i64 = v.into();
        if v < 128 && v > -1 {
            self.resize(1);
            self.push(v as u8);
        } else if v < 16384 && v > -1 {
            self.resize(2);
            self.push(((v & 0x7F) | 0x80) as u8);
            self.push(((v >> 7) & 0x7F) as u8);
        } else if v < 2097152 && v > -1 {
            self.resize(3);
            self.push(((v & 0x7F) | 0x80) as u8);
            self.push(((v >> 7) | 0x80) as u8);
            self.push(((v >> 14) & 0x7F) as u8);
        } else if v < 268435456 && v > -1 {
            self.resize(4);
            self.push(((v & 0x7F) | 0x80) as u8);
            self.push((v >> 7 | 0x80) as u8);
            self.push((v >> 14 | 0x80) as u8);
            self.push(((v >> 21) & 0x7F) as u8);
        } else {
            self.resize(5);
            self.push(((v & 0x7F) | 0x80) as u8);
            self.push((v >> 7 | 0x80) as u8);
            self.push((v >> 14 | 0x80) as u8);
            self.push((v >> 21 | 0x80) as u8);
            self.push(((v >> 28) & 0x0F) as u8);
        }
    }

    pub fn write_double(&mut self, v: f64) {
        self.resize(8);
        let bits = if v.is_nan() { 0x7ff8000000000000u64 } else { v.to_bits() };
        for k in 0..8 {
            self.push((bits >> (8 * k)) as u8);
        }
    }

    /// `start` inclusive, `end` exclusive.
    pub fn write_bytes(&mut self, b: &BytecodeBuffer, start: i32, end: i32) {
        self.resize(end - start);
        for i in start..end {
            let v = b.bytecodes[i as usize];
            self.push(v);
        }
    }

    pub(crate) fn write_slice(&mut self, bytes: &[u8]) {
        self.resize(bytes.len() as i32);
        for &v in bytes {
            self.push(v);
        }
    }

    fn push(&mut self, v: u8) {
        self.bytecodes[self.size as usize] = v;
        self.size += 1;
    }

    // Java grows once by half; looping only differs where Java would overflow the array.
    fn resize(&mut self, increment: i32) {
        if self.size + increment > self.bytecodes.len() as i32 {
            let mut new_len = self.bytecodes.len() * 3 / 2 + 1;
            while (self.size + increment) as usize > new_len {
                new_len = new_len * 3 / 2 + 1;
            }
            self.bytecodes.resize(new_len, 0);
        }
    }

    pub fn read_u8(&self) -> i32 {
        let p = self.pos.get();
        let value = self.bytecodes[p as usize] as i32;
        self.pos.set(p + 1);
        value
    }

    pub fn read_u8_at(&self, pos: i32) -> i32 {
        self.bytecodes[pos as usize] as i32
    }

    pub fn read_u16(&self) -> i32 {
        let a = self.read_u8();
        let b = self.read_u8();
        a | (b << 8)
    }

    pub fn read_u32(&self) -> i32 {
        let mut result = self.read_u8();
        if 0 == (result & 0x00000080) {
            return result;
        }
        result = result & 0x0000007f | self.read_u8() << 7;
        if 0 == (result & 0x00004000) {
            return result;
        }
        result = result & 0x00003fff | self.read_u8() << 14;
        if 0 == (result & 0x00200000) {
            return result;
        }
        result = result & 0x001fffff | self.read_u8() << 21;
        if 0 == (result & 0x10000000) {
            return result;
        }
        result & 0x0fffffff | self.read_u8().wrapping_shl(28)
    }

    pub fn read_u32_at(&self, pos: i32) -> i64 {
        let mut pos = pos;
        let mut next = || {
            let v = self.read_u8_at(pos);
            pos += 1;
            v
        };
        let mut result = next();
        if 0 == (result & 0x00000080) {
            return result as i64;
        }
        result = result & 0x0000007f | next() << 7;
        if 0 == (result & 0x00004000) {
            return result as i64;
        }
        result = result & 0x00003fff | next() << 14;
        if 0 == (result & 0x00200000) {
            return result as i64;
        }
        result = result & 0x001fffff | next() << 21;
        if 0 == (result & 0x10000000) {
            return result as i64;
        }
        (result & 0x0fffffff | next().wrapping_shl(28)) as i64
    }

    pub fn read_s8(&self) -> i32 {
        let p = self.pos.get();
        let value = self.bytecodes[p as usize] as i8 as i32;
        self.pos.set(p + 1);
        value
    }

    pub fn read_s24(&self) -> i32 {
        let a = self.read_u8();
        let b = self.read_u8();
        let c = self.read_s8();
        a | (b << 8) | (c << 16)
    }

    pub fn read_double(&self) -> f64 {
        let mut first = 0u32;
        for k in 0..4 {
            first |= (self.read_u8() as u32) << (8 * k);
        }
        let mut second = 0u32;
        for k in 0..4 {
            second |= (self.read_u8() as u32) << (8 * k);
        }
        f64::from_bits(first as u64 | (second as u64) << 32)
    }

    pub fn read_bytes(&self, length: i32) -> Vec<u8> {
        let p = self.pos.get() as usize;
        let bytes = self.bytecodes[p..p + length as usize].to_vec();
        self.pos.set(self.pos.get() + length);
        bytes
    }

    /// Does not move the cursor.
    pub fn read_string(&self, length: i32) -> Option<String> {
        let p = self.pos.get() as usize;
        Some(crate::asc::util::jstr::from_java_utf8_bytes(&self.bytecodes[p..p + length as usize]))
    }

    pub fn close(&self) {
        self.pos.set(0);
    }

    pub fn skip(&self, length: i64) {
        self.pos.set((self.pos.get() as i64 + length) as i32);
    }

    pub fn skip_entries(&self, entries: i64) {
        let mut i = 0i64;
        while i < entries {
            self.read_u32();
            i += 1;
        }
    }

    pub fn seek(&self, pos: i32) {
        self.pos.set(pos);
    }

    pub fn same(&self, b: &BytecodeBuffer, start1: i32, end1: i32, start2: i32, end2: i32) -> bool {
        if (end1 - start1) != (end2 - start2) {
            return false;
        }
        self.bytecodes[start1 as usize..end1 as usize] == b.bytecodes[start2 as usize..end2 as usize]
    }

    pub fn hash_code(&self, start: i32, end: i32) -> i32 {
        let mut hash: i64 = 1234;
        for j in start..end {
            hash ^= self.bytecodes[j as usize] as i8 as i64;
        }
        ((hash >> 32) ^ hash) as i32
    }

    pub fn write_to(&self, out: &mut impl Write) -> io::Result<()> {
        out.write_all(&self.bytecodes[..self.size as usize])
    }

    pub fn minor_version(&self) -> i32 {
        (self.bytecodes[0] as i32) | ((self.bytecodes[1] as i32) << 8)
    }

    pub(crate) fn slice(&self, start: i32, end: i32) -> &[u8] {
        &self.bytecodes[start as usize..end as usize]
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.bytecodes[..self.size as usize].to_vec()
    }
}

/// Which `ByteArray` subclass the pool creates, i.e. which `equals` its entries use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArrayKind {
    Bytes,
    Ns,
    Nss,
    Mn,
}

/// The `equals` of `ByteArray`/`NS`/`NSS`/`MN`, as a map key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ArrayKey {
    Bytes(Vec<u8>),
    Ns(i32, i32),
    Nss(Vec<i32>),
    Mn(i32, i32, i32),
    Unique(u64),
}

#[derive(Debug)]
pub(crate) struct ByteArrayPool {
    kind: ArrayKind,
    map: HashMap<ArrayKey, i32>,
    // Entries in index order; Java sorts its map by index before writing.
    entries: Vec<Vec<u8>>,
    unique: u64,
}

impl ByteArrayPool {
    pub(crate) fn new(kind: ArrayKind) -> ByteArrayPool {
        ByteArrayPool { kind, map: HashMap::new(), entries: Vec::new(), unique: 0 }
    }

    fn key(&mut self, b: &BytecodeBuffer, start: i32, end: i32) -> ArrayKey {
        match self.kind {
            ArrayKind::Bytes => ArrayKey::Bytes(b.slice(start, end).to_vec()),
            ArrayKind::Ns => {
                let original_pos = b.pos();
                b.seek(start);
                let ns_kind = b.read_u8();
                let index = match ns_kind {
                    CONSTANT_PrivateNamespace
                    | CONSTANT_Namespace
                    | CONSTANT_PackageNamespace
                    | CONSTANT_PackageInternalNs
                    | CONSTANT_ProtectedNamespace
                    | CONSTANT_ExplicitNamespace
                    | CONSTANT_StaticProtectedNs => b.read_u32(),
                    _ => 0,
                };
                b.seek(original_pos);
                if ns_kind == CONSTANT_PrivateNamespace {
                    // equal only to the very same entry, and every entry is stored at a fresh position
                    self.unique += 1;
                    ArrayKey::Unique(self.unique)
                } else {
                    ArrayKey::Ns(ns_kind, index)
                }
            }
            ArrayKind::Nss => {
                let original_pos = b.pos();
                b.seek(start);
                let count = b.read_u32();
                let set = (0..count).map(|_| b.read_u32()).collect();
                b.seek(original_pos);
                ArrayKey::Nss(set)
            }
            ArrayKind::Mn => {
                let original_pos = b.pos();
                b.seek(start);
                let const_kind = b.read_u8();
                let key = match const_kind {
                    CONSTANT_Qname | CONSTANT_QnameA | CONSTANT_Multiname | CONSTANT_MultinameA => {
                        let index1 = b.read_u32();
                        let index2 = b.read_u32();
                        ArrayKey::Mn(const_kind, index1, index2)
                    }
                    CONSTANT_RTQname | CONSTANT_RTQnameA | CONSTANT_MultinameL | CONSTANT_MultinameLA => {
                        let index1 = b.read_u32();
                        ArrayKey::Mn(const_kind, index1, 0)
                    }
                    CONSTANT_RTQnameL | CONSTANT_RTQnameLA => ArrayKey::Mn(const_kind, 0, 0),
                    CONSTANT_TypeName => {
                        let index1 = b.read_u32();
                        let _count = b.read_u32();
                        // only the first type parameter takes part in equality
                        let index2 = b.read_u32();
                        ArrayKey::Mn(const_kind, index1, index2)
                    }
                    _ => {
                        self.unique += 1;
                        ArrayKey::Unique(self.unique)
                    }
                };
                b.seek(original_pos);
                key
            }
        }
    }

    pub(crate) fn store(&mut self, b: &BytecodeBuffer, start: i32, end: i32) -> i32 {
        let key = self.key(b, start, end);
        let index = self.map.len() as i32 + 1;
        self.map.insert(key, index);
        self.entries.push(b.slice(start, end).to_vec());
        index
    }

    pub(crate) fn contains(&mut self, b: &BytecodeBuffer, start: i32, end: i32) -> i32 {
        let key = self.key(b, start, end);
        self.map.get(&key).copied().unwrap_or(-1)
    }

    #[allow(dead_code)]
    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.entries.clear();
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn write_to(&self, b: &mut BytecodeBuffer) {
        b.write_u32(if self.entries.is_empty() { 0 } else { self.entries.len() as i32 + 1 });
        self.write_entries(b);
    }

    pub(crate) fn write_entries(&self, b: &mut BytecodeBuffer) {
        for e in &self.entries {
            b.write_slice(e);
        }
    }
}
