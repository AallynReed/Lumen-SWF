//! Port of `macromedia.abc.ConstantPool` (with its package-private `IndexHistory`).

use std::rc::Rc;

use super::bytecode_buffer::{ArrayKind, ByteArrayPool, BytecodeBuffer};
use super::decoder_exception::DecoderException;
use super::multi_name::MultiName;
use super::q_name::QName;
use super::scanner;
use crate::asc::emit::constants::*;
use crate::asc::util::decimal128::Decimal128;

/// The `Object`s `ConstantPool.get`/`getGeneralMultiname` hand back.
#[derive(Clone, Debug)]
pub enum PoolValue {
    Str(String),
    Int(i32),
    Long(i64),
    Double(f64),
    Decimal(Decimal128),
    QName(QName),
    MultiName(MultiName),
    Bool(bool),
    Null,
    NamespaceSet(Vec<Option<String>>),
    List(Vec<Option<String>>),
}

#[derive(Debug)]
pub struct ConstantPool {
    pub pool_has_decimal: bool,
    pub(crate) in_: Rc<BytecodeBuffer>,
    pub(crate) history: Option<IndexHistory>,
    size: i32,

    pub(crate) intpositions: Vec<i32>,
    pub(crate) uintpositions: Vec<i32>,
    pub(crate) doublepositions: Vec<i32>,
    pub(crate) decimalpositions: Vec<i32>,
    pub(crate) strpositions: Vec<i32>,
    pub(crate) nspositions: Vec<i32>,
    pub(crate) nsspositions: Vec<i32>,
    pub(crate) mnpositions: Vec<i32>,

    pub(crate) int_end: i32,
    pub(crate) uint_end: i32,
    pub(crate) double_end: i32,
    pub(crate) decimal_end: i32,
    pub(crate) str_end: i32,
    pub(crate) ns_end: i32,
    pub(crate) nss_end: i32,
    pub(crate) mn_end: i32,
}

fn count(positions: &[i32]) -> i32 {
    if positions.is_empty() {
        0
    } else {
        positions.len() as i32 - 1
    }
}

impl ConstantPool {
    pub fn merge(pools: &[Rc<ConstantPool>]) -> ConstantPool {
        let mut preferred_size = 0;
        let mut has_decimal = false;
        for p in pools {
            if !p.decimalpositions.is_empty() {
                has_decimal = true;
            }
            preferred_size += p.mn_end;
        }
        let mut new_pool = ConstantPool::empty(has_decimal);
        new_pool.in_ = Rc::new(BytecodeBuffer::with_size(preferred_size));
        new_pool.history = Some(IndexHistory::new(pools.to_vec(), has_decimal));
        new_pool
    }

    fn empty(has_decimal: bool) -> ConstantPool {
        ConstantPool {
            pool_has_decimal: has_decimal,
            in_: Rc::new(BytecodeBuffer::from_bytes(Vec::new())),
            history: None,
            size: 0,
            intpositions: Vec::new(),
            uintpositions: Vec::new(),
            doublepositions: Vec::new(),
            decimalpositions: Vec::new(),
            strpositions: Vec::new(),
            nspositions: Vec::new(),
            nsspositions: Vec::new(),
            mnpositions: Vec::new(),
            int_end: 0,
            uint_end: 0,
            double_end: 0,
            decimal_end: 0,
            str_end: 0,
            ns_end: 0,
            nss_end: 0,
            mn_end: 0,
        }
    }

    pub fn new(in_: Rc<BytecodeBuffer>, has_decimal: bool) -> Result<ConstantPool, DecoderException> {
        let mut pool = ConstantPool::empty(has_decimal);
        pool.in_ = in_;
        pool.scan()?;
        Ok(pool)
    }

    fn scan(&mut self) -> Result<(), DecoderException> {
        let in_ = self.in_.clone();
        self.intpositions = scanner::scan_int_constants(&in_);
        self.int_end = in_.pos();
        self.uintpositions = scanner::scan_uint_constants(&in_);
        self.uint_end = in_.pos();
        self.doublepositions = scanner::scan_double_constants(&in_);
        self.double_end = in_.pos();
        if self.pool_has_decimal {
            self.decimalpositions = scanner::scan_decimal_constants(&in_);
            self.decimal_end = in_.pos();
        } else {
            self.decimalpositions = Vec::new();
            self.decimal_end = in_.pos();
        }
        self.strpositions = scanner::scan_str_constants(&in_);
        self.str_end = in_.pos();
        self.nspositions = scanner::scan_ns_constants(&in_);
        self.ns_end = in_.pos();
        self.nsspositions = scanner::scan_ns_set_constants(&in_);
        self.nss_end = in_.pos();
        self.mnpositions = scanner::scan_multiname_constants(&in_)?;
        self.mn_end = in_.pos();

        self.size = count(&self.intpositions)
            + count(&self.uintpositions)
            + count(&self.doublepositions)
            + count(&self.decimalpositions)
            + count(&self.strpositions)
            + count(&self.nspositions)
            + count(&self.nsspositions)
            + count(&self.mnpositions);
        Ok(())
    }

    pub fn size(&self) -> i32 {
        self.size
    }

    pub fn get_int(&self, index: i32) -> i32 {
        if index == 0 {
            return 0;
        }
        let pos = self.intpositions[index as usize];
        let original_pos = self.in_.pos();
        self.in_.seek(pos);
        let value = self.in_.read_u32();
        self.in_.seek(original_pos);
        value
    }

    pub fn get_long(&self, index: i32) -> i64 {
        if index == 0 {
            return 0;
        }
        let pos = self.uintpositions[index as usize];
        let original_pos = self.in_.pos();
        self.in_.seek(pos);
        let value = self.in_.read_u32() as i64;
        self.in_.seek(original_pos);
        value
    }

    pub fn get_double(&self, index: i32) -> f64 {
        if index == 0 {
            return 0.0;
        }
        let pos = self.doublepositions[index as usize];
        let original_pos = self.in_.pos();
        self.in_.seek(pos);
        let value = self.in_.read_double();
        self.in_.seek(original_pos);
        value
    }

    pub fn get_decimal(&self, index: i32) -> Decimal128 {
        if index == 0 {
            return Decimal128::zero();
        }
        let pos = self.decimalpositions[index as usize];
        let original_pos = self.in_.pos();
        self.in_.seek(pos);
        let valbytes = self.in_.read_bytes(16);
        self.in_.seek(original_pos);
        Decimal128::from_ieee(&valbytes).expect("16 bytes were read")
    }

    pub fn get_string(&self, index: i32) -> Result<Option<String>, DecoderException> {
        if index == 0 {
            return Ok(None);
        }
        let pos = self.strpositions[index as usize];
        let original_pos = self.in_.pos();
        self.in_.seek(pos);
        let len = self.in_.read_u32();
        let value = self.in_.read_string(len);
        self.in_.seek(original_pos);
        match value {
            Some(v) => Ok(Some(v)),
            None => Err(DecoderException::new("abc Decoder Erro: problem reading UTF-8 encoded strings.")),
        }
    }

    pub fn get_namespace_name(&self, index: i32) -> Result<Option<String>, DecoderException> {
        if index == 0 {
            return Ok(None);
        }
        let pos = self.nspositions[index as usize];
        let original_pos = self.in_.pos();
        self.in_.seek(pos);
        let kind = self.in_.read_u8();
        let value = match kind {
            CONSTANT_PrivateNamespace
            | CONSTANT_Namespace
            | CONSTANT_PackageNamespace
            | CONSTANT_PackageInternalNs
            | CONSTANT_ProtectedNamespace
            | CONSTANT_ExplicitNamespace
            | CONSTANT_StaticProtectedNs => {
                let i = self.in_.read_u32();
                self.get_string(i)?
            }
            _ => {
                return Err(DecoderException::new(format!(
                    "abc Decoder Error: constant pool index '{}' is not a Namespace type. The actual type is '{}'",
                    index, kind
                )))
            }
        };
        self.in_.seek(original_pos);
        Ok(value)
    }

    pub fn get_namespace_set(&self, index: i32) -> Result<Option<Vec<Option<String>>>, DecoderException> {
        if index == 0 {
            return Ok(None);
        }
        let pos = self.nsspositions[index as usize];
        let original_pos = self.in_.pos();
        self.in_.seek(pos);
        let count = self.in_.read_u32();
        let mut value = Vec::with_capacity(count.max(0) as usize);
        for _ in 0..count {
            let i = self.in_.read_u32();
            value.push(self.get_namespace_name(i)?);
        }
        self.in_.seek(original_pos);
        Ok(Some(value))
    }

    pub fn get_namespace_index_for_qname(&self, index: i32) -> Result<i32, DecoderException> {
        if index == 0 {
            return Ok(0);
        }
        let pos = self.mnpositions[index as usize];
        let original_pos = self.in_.pos();
        self.in_.seek(pos);
        let kind = self.in_.read_u8();
        match kind {
            CONSTANT_Qname | CONSTANT_QnameA => {
                let namespace_index = self.in_.read_u32();
                self.in_.seek(original_pos);
                Ok(namespace_index)
            }
            _ => {
                self.in_.seek(original_pos);
                Err(not_qname(index, kind))
            }
        }
    }

    pub fn get_namespace_kind(&self, namespace_index: i32) -> i32 {
        if namespace_index == 0 {
            return -1;
        }
        let pos = self.nspositions[namespace_index as usize];
        let original_pos = self.in_.pos();
        self.in_.seek(pos);
        let kind = self.in_.read_u8();
        self.in_.seek(original_pos);
        kind
    }

    pub fn get_qname(&self, index: i32) -> Result<Option<QName>, DecoderException> {
        if index == 0 {
            return Ok(None);
        }
        let pos = self.mnpositions[index as usize];
        let original_pos = self.in_.pos();
        self.in_.seek(pos);
        let kind = self.in_.read_u8();
        match kind {
            CONSTANT_Qname | CONSTANT_QnameA => {
                let namespace_index = self.in_.read_u32();
                let name_index = self.in_.read_u32();
                let value = QName::new(self.get_namespace_name(namespace_index)?, self.get_string(name_index)?);
                self.in_.seek(original_pos);
                Ok(Some(value))
            }
            CONSTANT_TypeName => {
                let name_index = self.in_.read_u32();
                let count = self.in_.read_u32();
                let mut params = String::from(".<");
                let base = self.get_qname(name_index)?;
                for i in 0..count {
                    let p = self.in_.read_u32();
                    let q = self.get_qname(p)?;
                    params.push_str(if i > 0 { "," } else { "" });
                    params.push_str(&q.map_or_else(|| "null".to_string(), |q| q.to_string()));
                }
                params.push('>');
                let base = base.expect("TypeName base is null");
                let local = format!("{}{}", base.get_local_part().unwrap_or("null"), params);
                let value = QName::new(base.get_namespace().map(str::to_string), Some(local));
                self.in_.seek(original_pos);
                Ok(Some(value))
            }
            _ => {
                self.in_.seek(original_pos);
                Err(not_qname(index, kind))
            }
        }
    }

    pub fn get_multi_name(&self, index: i32) -> Result<Option<MultiName>, DecoderException> {
        if index == 0 {
            return Ok(None);
        }
        let pos = self.mnpositions[index as usize];
        let original_pos = self.in_.pos();
        self.in_.seek(pos);
        let kind = self.in_.read_u8();
        match kind {
            CONSTANT_Multiname | CONSTANT_MultinameA => {
                let n = self.in_.read_u32();
                let name = self.get_string(n)?;
                let namespace_set = self.in_.read_u32();
                let namespaces = self.get_namespace_set(namespace_set)?;
                let value = MultiName::new(name, namespaces);
                self.in_.seek(original_pos);
                Ok(Some(value))
            }
            _ => {
                self.in_.seek(original_pos);
                Err(DecoderException::new(format!(
                    "abc Decoder Error: constant constantPool index '{}' is not a MultiName type. The actual type is '{}'",
                    index, kind
                )))
            }
        }
    }

    pub fn get_general_multiname(&self, index: i32) -> Result<Option<PoolValue>, DecoderException> {
        if index == 0 {
            return Ok(None);
        }
        let pos = self.mnpositions[index as usize];
        let original_pos = self.in_.pos();
        self.in_.seek(pos);
        let kind = self.in_.read_u8();
        match kind {
            CONSTANT_Qname | CONSTANT_QnameA => {
                let namespace_index = self.in_.read_u32();
                let name_index = self.in_.read_u32();
                let value = QName::new(self.get_namespace_name(namespace_index)?, self.get_string(name_index)?);
                self.in_.seek(original_pos);
                Ok(Some(PoolValue::QName(value)))
            }
            CONSTANT_Multiname | CONSTANT_MultinameA => {
                let n = self.in_.read_u32();
                let name = self.get_string(n)?;
                let namespace_set = self.in_.read_u32();
                let namespaces = self.get_namespace_set(namespace_set)?;
                let value = MultiName::new(name, namespaces);
                self.in_.seek(original_pos);
                Ok(Some(PoolValue::MultiName(value)))
            }
            CONSTANT_RTQnameL => {
                self.in_.seek(original_pos);
                Ok(Some(PoolValue::Str("CONSTANT_RTQnameL".to_string())))
            }
            CONSTANT_RTQnameLA => {
                self.in_.seek(original_pos);
                Ok(Some(PoolValue::Str("CONSTANT_RTQnameLA".to_string())))
            }
            CONSTANT_MultinameL | CONSTANT_MultinameLA => {
                let namespaceset_index = self.in_.read_u32();
                let value = self.get_namespace_set(namespaceset_index)?.unwrap_or_default();
                self.in_.seek(original_pos);
                Ok(Some(PoolValue::List(value)))
            }
            CONSTANT_RTQname | CONSTANT_RTQnameA => {
                let idx = self.in_.read_u32();
                let s = self.get_string(idx)?;
                self.in_.seek(original_pos);
                Ok(s.map(PoolValue::Str))
            }
            _ => {
                self.in_.seek(original_pos);
                Err(not_qname(index, kind))
            }
        }
    }

    pub fn get(&self, index: i32, kind: i32) -> Result<Option<PoolValue>, DecoderException> {
        if index == 0 {
            return Ok(None);
        }
        match kind {
            CONSTANT_Utf8 => Ok(self.get_string(index)?.map(PoolValue::Str)),
            CONSTANT_Integer => Ok(Some(PoolValue::Int(self.get_int(index)))),
            CONSTANT_UInteger => Ok(Some(PoolValue::Long(self.get_long(index)))),
            CONSTANT_Double => Ok(Some(PoolValue::Double(self.get_double(index)))),
            CONSTANT_Decimal => Ok(Some(PoolValue::Decimal(self.get_decimal(index)))),
            CONSTANT_Qname | CONSTANT_QnameA => Ok(self.get_qname(index)?.map(PoolValue::QName)),
            CONSTANT_Namespace
            | CONSTANT_PrivateNamespace
            | CONSTANT_PackageNamespace
            | CONSTANT_PackageInternalNs
            | CONSTANT_ProtectedNamespace
            | CONSTANT_ExplicitNamespace
            | CONSTANT_StaticProtectedNs => Ok(self.get_namespace_name(index)?.map(PoolValue::Str)),
            CONSTANT_Multiname | CONSTANT_MultinameA => Ok(self.get_multi_name(index)?.map(PoolValue::MultiName)),
            CONSTANT_False => Ok(Some(PoolValue::Bool(false))),
            CONSTANT_True => Ok(Some(PoolValue::Bool(true))),
            CONSTANT_Null => Ok(Some(PoolValue::Null)),
            CONSTANT_RTQname | CONSTANT_RTQnameA => self.get_general_multiname(index),
            CONSTANT_RTQnameL => Ok(Some(PoolValue::Str("CONSTANT_RTQnameL".to_string()))),
            CONSTANT_RTQnameLA => Ok(Some(PoolValue::Str("CONSTANT_RTQnameLA".to_string()))),
            CONSTANT_MultinameL | CONSTANT_MultinameLA => Ok(self.get_namespace_set(self.get_int(index))?.map(PoolValue::NamespaceSet)),
            CONSTANT_Namespace_Set => Ok(self.get_namespace_set(index)?.map(PoolValue::NamespaceSet)),
            _ => Err(DecoderException::new(format!("Error: Unhandled constant type - {}", kind))),
        }
    }

    pub fn write_to(&mut self, out: &mut Vec<u8>) {
        let in_ = Rc::get_mut(&mut self.in_).expect("merged pool buffer is not shared");
        self.history.as_ref().expect("only a merged pool is written").write_to(in_);
        in_.write_to(out).expect("writing to a Vec cannot fail");
    }
}

fn not_qname(index: i32, kind: i32) -> DecoderException {
    DecoderException::new(format!("abc Decoder Error: constant pool index '{}' is not a QName type. The actual type is '{}'", index, kind))
}

pub(crate) const CP_INT: i32 = 0;
pub(crate) const CP_UINT: i32 = 1;
pub(crate) const CP_DOUBLE: i32 = 2;
pub(crate) const CP_DECIMAL: i32 = 3;
pub(crate) const CP_STRING: i32 = 4;
pub(crate) const CP_NS: i32 = 5;
pub(crate) const CP_NSSET: i32 = 6;
pub(crate) const CP_MN: i32 = 7;

#[derive(Debug)]
pub(crate) struct IndexHistory {
    pub total: i32,
    pub duplicate: i32,
    pub total_bytes: i32,
    pub duplicate_bytes: i32,

    pools: Vec<Rc<ConstantPool>>,
    pool_sizes: Vec<i32>,
    map: Vec<i32>,

    has_decimal: bool,

    int_p: ByteArrayPool,
    uint_p: ByteArrayPool,
    double_p: ByteArrayPool,
    decimal_p: Option<ByteArrayPool>,
    string_p: ByteArrayPool,
    ns_p: ByteArrayPool,
    nss_p: ByteArrayPool,
    mn_p: ByteArrayPool,
    in_ns: BytecodeBuffer,
    in_nsset: BytecodeBuffer,
    in_mn: BytecodeBuffer,

    disable_debugging_info: bool,
}

impl IndexHistory {
    fn new(pools: Vec<Rc<ConstantPool>>, pool_has_decimal: bool) -> IndexHistory {
        let mut pool_sizes = vec![0; pools.len()];
        let mut size = 0;
        let mut preferred_size = 0;
        for (i, p) in pools.iter().enumerate() {
            pool_sizes[i] = if i == 0 { 0 } else { size };
            size += p.size();
            preferred_size += p.mn_end - p.str_end;
        }
        IndexHistory {
            total: 0,
            duplicate: 0,
            total_bytes: 0,
            duplicate_bytes: 0,
            pools,
            pool_sizes,
            map: vec![0; size as usize],
            has_decimal: pool_has_decimal,
            int_p: ByteArrayPool::new(ArrayKind::Bytes),
            uint_p: ByteArrayPool::new(ArrayKind::Bytes),
            double_p: ByteArrayPool::new(ArrayKind::Bytes),
            decimal_p: if pool_has_decimal { Some(ByteArrayPool::new(ArrayKind::Bytes)) } else { None },
            string_p: ByteArrayPool::new(ArrayKind::Bytes),
            ns_p: ByteArrayPool::new(ArrayKind::Ns),
            nss_p: ByteArrayPool::new(ArrayKind::Nss),
            mn_p: ByteArrayPool::new(ArrayKind::Mn),
            in_ns: BytecodeBuffer::with_size(preferred_size),
            in_nsset: BytecodeBuffer::with_size(preferred_size),
            in_mn: BytecodeBuffer::with_size(preferred_size),
            disable_debugging_info: false,
        }
    }

    pub(crate) fn disable_debugging(&mut self) {
        self.disable_debugging_info = true;
    }

    pub(crate) fn get_index(&mut self, pool_index: i32, kind: i32, index: i32) -> i32 {
        if index == 0 {
            0
        } else {
            let new_index = self.calculate_index(pool_index, kind, index);
            if self.map[new_index as usize] == 0 {
                self.decode_on_demand(pool_index, kind, index, new_index);
            }
            self.map[new_index as usize]
        }
    }

    pub(crate) fn write_to(&self, b: &mut BytecodeBuffer) {
        self.int_p.write_to(b);
        self.uint_p.write_to(b);
        self.double_p.write_to(b);
        if self.has_decimal {
            self.decimal_p.as_ref().expect("decimal pool exists").write_to(b);
        }
        self.string_p.write_to(b);
        self.ns_p.write_to(b);
        self.nss_p.write_to(b);
        self.mn_p.write_to(b);
    }

    /// `pool_index` and `kind` are 0-based, `old_index` 1-based.
    fn calculate_index(&self, pool_index: i32, kind: i32, old_index: i32) -> i32 {
        let pool = &self.pools[pool_index as usize];
        let mut index = self.pool_sizes[pool_index as usize];
        if kind > CP_INT {
            index += count(&pool.intpositions);
        }
        if kind > CP_UINT {
            index += count(&pool.uintpositions);
        }
        if kind > CP_DOUBLE {
            index += count(&pool.doublepositions);
        }
        if self.has_decimal && kind > CP_DECIMAL {
            index += count(&pool.decimalpositions);
        }
        if kind > CP_STRING {
            index += count(&pool.strpositions);
        }
        if kind > CP_NS {
            index += count(&pool.nspositions);
        }
        if kind > CP_NSSET {
            index += count(&pool.nsspositions);
        }
        if kind > CP_MN {
            index += count(&pool.mnpositions);
        }
        index += old_index - 1;
        index
    }

    fn decode_on_demand(&mut self, pool_index: i32, kind: i32, j: i32, j2: i32) {
        let pool = self.pools[pool_index as usize].clone();
        let pool_in: &BytecodeBuffer = &pool.in_;
        let (positions, end_pos): (&[i32], i32) = match kind {
            CP_INT => (&pool.intpositions, pool.int_end),
            CP_UINT => (&pool.uintpositions, pool.uint_end),
            CP_DOUBLE => (&pool.doublepositions, pool.double_end),
            CP_DECIMAL => (&pool.decimalpositions, pool.decimal_end),
            CP_STRING => (&pool.strpositions, pool.str_end),
            CP_NS => (&pool.nspositions, pool.ns_end),
            CP_NSSET => (&pool.nsspositions, pool.nss_end),
            _ => (&pool.mnpositions, pool.mn_end),
        };
        let length = positions.len() as i32;

        let mut start = positions[j as usize];
        let mut end = if j != length - 1 { positions[j as usize + 1] } else { end_pos };

        // 0 = source pool, 1 = in_ns, 2 = in_nsset, 3 = in_mn
        let mut source = 0;

        if kind == CP_NS {
            let pos = positions[j as usize];
            let original_pos = pool_in.pos();
            pool_in.seek(pos);
            start = self.in_ns.size();
            let ns_kind = pool_in.read_u8();
            self.in_ns.write_u8(ns_kind);
            match ns_kind {
                CONSTANT_PrivateNamespace if self.disable_debugging_info => {
                    self.in_ns.write_u32(0);
                }
                CONSTANT_PrivateNamespace
                | CONSTANT_Namespace
                | CONSTANT_PackageNamespace
                | CONSTANT_PackageInternalNs
                | CONSTANT_ProtectedNamespace
                | CONSTANT_ExplicitNamespace
                | CONSTANT_StaticProtectedNs => {
                    let index = pool_in.read_u32();
                    let new_index = self.get_index(pool_index, CP_STRING, index);
                    self.in_ns.write_u32(new_index);
                }
                _ => {}
            }
            pool_in.seek(original_pos);
            end = self.in_ns.size();
            source = 1;
        } else if kind == CP_NSSET {
            let pos = positions[j as usize];
            let original_pos = pool_in.pos();
            pool_in.seek(pos);
            start = self.in_nsset.size();
            let count = pool_in.read_u32();
            self.in_nsset.write_u32(count);
            for _ in 0..count {
                let index = pool_in.read_u32();
                let new_index = self.get_index(pool_index, CP_NS, index);
                self.in_nsset.write_u32(new_index);
            }
            pool_in.seek(original_pos);
            end = self.in_nsset.size();
            source = 2;
        } else if kind == CP_MN {
            let pos = positions[j as usize];
            let original_pos = pool_in.pos();
            pool_in.seek(pos);
            start = self.in_mn.size();
            let const_kind = pool_in.read_u8();
            if const_kind != CONSTANT_TypeName {
                self.in_mn.write_u8(const_kind);
            }
            match const_kind {
                CONSTANT_Qname | CONSTANT_QnameA => {
                    let namespace_index = pool_in.read_u32();
                    let new_namespace_index = self.get_index(pool_index, CP_NS, namespace_index);
                    self.in_mn.write_u32(new_namespace_index);
                    let name_index = pool_in.read_u32();
                    let new_name_index = self.get_index(pool_index, CP_STRING, name_index);
                    self.in_mn.write_u32(new_name_index);
                }
                CONSTANT_Multiname | CONSTANT_MultinameA => {
                    let name_index = pool_in.read_u32();
                    let new_name_index = self.get_index(pool_index, CP_STRING, name_index);
                    self.in_mn.write_u32(new_name_index);
                    let namespace_set = pool_in.read_u32();
                    let new_namespace_set = self.get_index(pool_index, CP_NSSET, namespace_set);
                    self.in_mn.write_u32(new_namespace_set);
                }
                CONSTANT_RTQname | CONSTANT_RTQnameA => {
                    let index = pool_in.read_u32();
                    let new_index = self.get_index(pool_index, CP_STRING, index);
                    self.in_mn.write_u32(new_index);
                }
                CONSTANT_RTQnameL | CONSTANT_RTQnameLA => {}
                CONSTANT_MultinameL | CONSTANT_MultinameLA => {
                    let namespace_set = pool_in.read_u32();
                    let new_namespace_set = self.get_index(pool_index, CP_NSSET, namespace_set);
                    self.in_mn.write_u32(new_namespace_set);
                }
                CONSTANT_TypeName => {
                    let name_index = pool_in.read_u32();
                    let new_name_index = self.get_index(pool_index, CP_MN, name_index);
                    let count = pool_in.read_u32();
                    let mut new_params = Vec::new();
                    for _ in 0..count {
                        let p = pool_in.read_u32();
                        new_params.push(self.get_index(pool_index, CP_MN, p));
                    }
                    start = self.in_mn.size();
                    self.in_mn.write_u8(const_kind);
                    self.in_mn.write_u32(new_name_index);
                    self.in_mn.write_u32(count);
                    for p in new_params {
                        self.in_mn.write_u32(p);
                    }
                }
                _ => {}
            }
            pool_in.seek(original_pos);
            end = self.in_mn.size();
            source = 3;
        }

        let buf: &BytecodeBuffer = match source {
            0 => pool_in,
            1 => &self.in_ns,
            2 => &self.in_nsset,
            _ => &self.in_mn,
        };
        let ba_pool = match kind {
            CP_INT => &mut self.int_p,
            CP_UINT => &mut self.uint_p,
            CP_DOUBLE => &mut self.double_p,
            CP_DECIMAL => self.decimal_p.as_mut().expect("decimal pool exists"),
            CP_STRING => &mut self.string_p,
            CP_NS => &mut self.ns_p,
            CP_NSSET => &mut self.nss_p,
            _ => &mut self.mn_p,
        };
        let mut new_index = ba_pool.contains(buf, start, end);
        if new_index == -1 {
            new_index = ba_pool.store(buf, start, end);
        } else {
            self.duplicate += 1;
            self.duplicate_bytes += end - start;
        }

        self.total += 1;
        self.total_bytes += end - start;

        if j != 0 {
            self.map[j2 as usize] = new_index;
        }
    }
}
