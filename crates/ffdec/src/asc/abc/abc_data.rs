//! Port of `macromedia/abc/AbcData.java`: the parts of an ABC file `AbcParser` needs (constant pool, method signatures,
//! metadata, class and script traits). Java's serialized on-disk `AbcDataCache` is not ported; `AbcParser` never uses it.

use std::cell::RefCell;
use std::rc::Rc;

use crate::asc::abc::bytecode_buffer::BytecodeBuffer;
use crate::asc::emit::constants::*;
use crate::asc::parser::node::NodeRef;
use crate::asc::semantics::meta_data::MetaData;
use crate::asc::semantics::value::{KeyValuePair, KeylessValue, Value};
use crate::asc::util::decimal128::Decimal128;

pub struct Trait {
    name_index: i32,
    kind: i32,
    data: Vec<i32>,
    metadata: Option<Vec<i32>>,
}

impl Trait {
    fn new(name_index: i32, kind: i32, data: Vec<i32>) -> Trait {
        Trait { name_index, kind, data, metadata: None }
    }

    pub fn add_metadata(&mut self, metadata: Vec<i32>) {
        self.metadata = Some(metadata);
    }

    pub fn get_name_index(&self) -> i32 {
        self.name_index
    }

    pub fn get_attrs(&self) -> i32 {
        self.kind >> 4
    }

    pub fn get_tag(&self) -> i32 {
        self.kind & 0xf
    }

    pub fn has_metadata(&self) -> bool {
        (self.get_attrs() & TRAIT_FLAG_metadata) != 0
    }

    pub fn is_const_trait(&self) -> bool {
        self.get_tag() == TRAIT_Const
    }

    pub fn get_metadata(&self) -> Option<&Vec<i32>> {
        self.metadata.as_ref()
    }

    pub fn get_slot_id(&self) -> i32 {
        self.data[0]
    }

    pub fn get_type_name(&self) -> i32 {
        self.data[1]
    }

    pub fn get_value_index(&self) -> i32 {
        self.data[2]
    }

    pub fn get_value_kind(&self) -> i32 {
        self.data[3]
    }

    pub fn get_disp_id(&self) -> i32 {
        self.data[0]
    }

    pub fn get_method_id(&self) -> i32 {
        self.data[1]
    }

    pub fn get_class_id(&self) -> i32 {
        self.data[1]
    }
}

/// An unstructured view of a multiname entry.
pub struct NameData {
    kind: i32,
    params: Option<Vec<i32>>,
}

impl NameData {
    pub fn get_kind(&self) -> i32 {
        self.kind
    }

    pub fn get_params(&self) -> Option<&Vec<i32>> {
        self.params.as_ref()
    }

    /// The API versions in the name's namespace URI, as `getVersions()` gathers them.
    fn get_versions(&self, abc: &AbcData) -> Option<Vec<i32>> {
        let params = self.params.as_ref().expect("NullPointerException");
        let ns: Option<&Namespace> = match self.kind {
            CONSTANT_Qname | CONSTANT_QnameA => abc.namespaces[params[0] as usize].as_ref(),
            CONSTANT_Multiname | CONSTANT_MultinameA => {
                let nss = &abc.namespace_sets[params[1] as usize].as_ref().expect("NullPointerException").namespace_ids;
                if !nss.is_empty() {
                    abc.namespaces[nss[0] as usize].as_ref()
                } else {
                    None
                }
            }
            _ => None,
        };
        let ns = ns?;
        Self::get_version(&abc.strings[ns.name_offset as usize])
    }

    fn get_version(uri: &str) -> Option<Vec<i32>> {
        let last = uri.encode_utf16().last()? as i32;
        if (0xE000..=0xF000).contains(&last) {
            Some(vec![last - 0xE000])
        } else {
            None
        }
    }
}

/// `AbcData.BinaryMN`: a name as `AbcParser` reads it.
pub struct BinaryMN {
    pub kind: i32,
    /// String index of the name (unused for `CONSTANT_TypeName`).
    pub name_id: i32,
    /// Namespace or namespace set index.
    pub ns_id: i32,
    pub ns_is_set: bool,
    /// `CONSTANT_TypeName`: the base name's multiname index (e.g. `Vector`).
    pub base_mn: i32,
    /// `CONSTANT_TypeName`: the parameters' multiname indexes.
    pub params: Vec<i32>,
    pub versions: Option<Vec<i32>>,
}

pub struct Namespace {
    name_offset: i32,
    pub ns_kind: i8,
    name: String,
}

impl Namespace {
    pub fn get_name(&self) -> String {
        self.name.clone()
    }
}

pub struct NamespaceSet {
    namespace_ids: Vec<i32>,
}

impl NamespaceSet {
    pub fn get_namespace_ids(&self) -> &Vec<i32> {
        &self.namespace_ids
    }
}

pub struct Method {
    return_type: i32,
    param_types: Vec<i32>,
    name_index: i32,
    flags: i32,
    optional_param_types: Vec<i32>,
    optional_param_kinds: Vec<i32>,
    param_names: Vec<String>,
}

impl Method {
    pub fn get_return_type(&self) -> i32 {
        self.return_type
    }
    pub fn get_param_types(&self) -> &Vec<i32> {
        &self.param_types
    }
    pub fn get_name_index(&self) -> i32 {
        self.name_index
    }
    pub fn get_flags(&self) -> i32 {
        self.flags
    }
    pub fn get_optional_param_types(&self) -> &Vec<i32> {
        &self.optional_param_types
    }
    pub fn get_optional_param_kinds(&self) -> &Vec<i32> {
        &self.optional_param_kinds
    }
    pub fn get_param_names(&self) -> Vec<String> {
        self.param_names.clone()
    }
    pub fn get_needs_rest(&self) -> bool {
        (self.flags & METHOD_Needrest) != 0 || (self.flags & METHOD_IgnoreRest) != 0
    }
    pub fn get_has_optional(&self) -> bool {
        (self.flags & METHOD_HasOptional) != 0
    }
    pub fn get_has_param_names(&self) -> bool {
        (self.flags & METHOD_HasParamNames) != 0
    }
}

pub struct Metadata {
    name_index: i32,
    keys: Vec<i32>,
    values: Vec<i32>,
}

impl Metadata {
    pub fn get_data_count(&self) -> usize {
        self.keys.len()
    }
}

pub struct InstanceInfo {
    name_index: i32,
    super_index: i32,
    flags: i32,
    protected_ns: i32,
    interfaces: Vec<i32>,
    init_index: i32,
    i_traits: Vec<Trait>,
}

impl InstanceInfo {
    pub fn get_instance_name_id(&self) -> i32 {
        self.name_index
    }
    pub fn get_super_id(&self) -> i32 {
        self.super_index
    }
    pub fn get_flags(&self) -> i32 {
        self.flags
    }
    pub fn get_protected_ns(&self) -> i32 {
        self.protected_ns
    }
    pub fn get_interfaces(&self) -> &Vec<i32> {
        &self.interfaces
    }
    pub fn get_init_index(&self) -> i32 {
        self.init_index
    }
    pub fn get_i_traits(&self) -> &Vec<Trait> {
        &self.i_traits
    }
}

pub struct ClassInfo {
    clinit_index: i32,
    c_traits: Vec<Trait>,
}

impl ClassInfo {
    pub fn get_clinit_index(&self) -> i32 {
        self.clinit_index
    }
    pub fn get_c_traits(&self) -> &Vec<Trait> {
        &self.c_traits
    }
}

pub struct ScriptInfo {
    init_index: i32,
    script_traits: Vec<Trait>,
}

impl ScriptInfo {
    pub fn get_init_index(&self) -> i32 {
        self.init_index
    }
    pub fn get_script_traits(&self) -> &Vec<Trait> {
        &self.script_traits
    }
}

#[derive(Default)]
pub struct AbcData {
    script_name: String,
    dirty: bool,
    name_data: Vec<Option<NameData>>,
    binary_multinames: RefCell<Vec<Option<Rc<BinaryMN>>>>,
    namespaces: Vec<Option<Namespace>>,
    namespace_sets: Vec<Option<NamespaceSet>>,
    strings: Vec<String>,
    ints: Vec<i32>,
    uints: Vec<i64>,
    doubles: Vec<f64>,
    decimals: Vec<Option<Decimal128>>,
    methods: Vec<Method>,
    raw_metadata: Vec<Metadata>,
    semantic_metadata: RefCell<Vec<Option<MetaData>>>,
    instance_infos: Vec<InstanceInfo>,
    class_infos: Vec<ClassInfo>,
    script_infos: Vec<ScriptInfo>,
}

impl AbcData {
    pub fn new(script_name: &str) -> AbcData {
        AbcData { script_name: script_name.to_string(), dirty: true, ..Default::default() }
    }

    pub fn script_name(&self) -> &str {
        &self.script_name
    }

    pub fn is_building(&self) -> bool {
        self.dirty
    }

    pub fn get_name(&self, idx: i32) -> Rc<BinaryMN> {
        if let Some(mn) = &self.binary_multinames.borrow()[idx as usize] {
            return mn.clone();
        }
        let nd = self.name_data[idx as usize].as_ref().expect("NullPointerException");
        let params = nd.params.as_ref();
        let mn = match nd.kind {
            CONSTANT_Qname | CONSTANT_QnameA => {
                let p = params.expect("NullPointerException");
                BinaryMN { kind: nd.kind, name_id: p[1], ns_id: p[0], ns_is_set: false, base_mn: 0, params: Vec::new(), versions: nd.get_versions(self) }
            }
            CONSTANT_TypeName => {
                let p = params.expect("NullPointerException");
                let count = p.len() - 2;
                BinaryMN { kind: nd.kind, name_id: 0, ns_id: 0, ns_is_set: false, base_mn: p[0], params: p[2..2 + count].to_vec(), versions: None }
            }
            CONSTANT_Multiname | CONSTANT_MultinameA => {
                let p = params.expect("NullPointerException");
                BinaryMN { kind: nd.kind, name_id: p[0], ns_id: p[1], ns_is_set: true, base_mn: 0, params: Vec::new(), versions: nd.get_versions(self) }
            }
            k => panic!("IllegalArgumentException: Unexpected multiname type: {}", k),
        };
        let mn = Rc::new(mn);
        self.binary_multinames.borrow_mut()[idx as usize] = Some(mn.clone());
        mn
    }

    pub fn get_namespace(&self, i: i32) -> &Namespace {
        self.namespaces[i as usize].as_ref().expect("NullPointerException")
    }

    pub fn get_namespace_set(&self, id: i32) -> &NamespaceSet {
        self.namespace_sets[id as usize].as_ref().expect("NullPointerException")
    }

    pub fn get_string(&self, i: i32) -> String {
        self.strings[i as usize].clone()
    }

    pub fn get_int(&self, i: i32) -> i32 {
        self.ints[i as usize]
    }

    pub fn get_uint(&self, i: i32) -> i64 {
        self.uints[i as usize]
    }

    pub fn get_double(&self, i: i32) -> f64 {
        self.doubles[i as usize]
    }

    pub fn get_decimal(&self, i: i32) -> Decimal128 {
        self.decimals[i as usize].clone().expect("NullPointerException")
    }

    pub fn get_method(&self, index: i32) -> &Method {
        &self.methods[index as usize]
    }

    /// The metadata entry as a `MetaData`, filled in through the first node that asks and shared with later ones.
    pub fn get_metadata(&self, idx: i32, meta_node: &NodeRef) -> Option<MetaData> {
        if self.semantic_metadata.borrow()[idx as usize].is_none() {
            let m = &self.raw_metadata[idx as usize];
            let vals: Vec<Value> = m
                .keys
                .iter()
                .zip(m.values.iter())
                .map(|(&k, &v)| {
                    let value = &self.strings[v as usize];
                    if k == 0 {
                        Value::KeylessValue(Rc::new(KeylessValue::new(value)))
                    } else {
                        Value::KeyValuePair(Rc::new(KeyValuePair::new(&self.strings[k as usize], value)))
                    }
                })
                .collect();
            let md = {
                let mut n = meta_node.borrow_mut();
                let mdn = n.as_meta_data_mut().expect("ClassCastException");
                mdn.set_id(Some(self.get_string(m.name_index)));
                mdn.set_values(Some(vals));
                mdn.get_metadata()
            };
            self.semantic_metadata.borrow_mut()[idx as usize] = md;
        }
        self.semantic_metadata.borrow()[idx as usize].clone()
    }

    pub fn get_instance_info(&self, i: i32) -> &InstanceInfo {
        &self.instance_infos[i as usize]
    }

    pub fn get_class_info_size(&self) -> i32 {
        self.class_infos.len() as i32
    }

    pub fn get_class_info(&self, idx: i32) -> &ClassInfo {
        &self.class_infos[idx as usize]
    }

    pub fn get_script_info_size(&self) -> i32 {
        self.script_infos.len() as i32
    }

    pub fn get_script_info(&self, idx: i32) -> &ScriptInfo {
        &self.script_infos[idx as usize]
    }

    pub fn read_abc_bytes(&mut self, raw_abc: Vec<u8>) {
        if !self.is_building() {
            return;
        }
        self.read_abc(&BytecodeBuffer::from_bytes(raw_abc));
    }

    pub fn read_abc(&mut self, buf: &BytecodeBuffer) {
        if !self.is_building() {
            return;
        }
        let minor_version = buf.read_u16();
        buf.read_u16();
        self.scan_cpool(buf, minor_version >= MINORwithDECIMAL);
        self.scan_methods(buf);
        self.scan_metadata(buf);
        self.scan_classes(buf);
        self.scan_scripts(buf);
    }

    fn scan_cpool(&mut self, buf: &BytecodeBuffer, has_decimal: bool) {
        let size = buf.read_u32() as usize;
        self.ints = vec![0; size];
        for i in 1..size {
            self.ints[i] = buf.read_u32();
        }

        let size = buf.read_u32() as usize;
        self.uints = vec![0; size];
        for i in 1..size {
            self.uints[i] = buf.read_u32() as i64 & 0xFFFFFFFF;
        }

        let size = buf.read_u32() as usize;
        self.doubles = vec![0.0; size];
        for i in 1..size {
            self.doubles[i] = buf.read_double();
        }

        if has_decimal {
            let size = buf.read_u32() as usize;
            self.decimals = vec![None; size];
            for i in 1..size {
                let rep = buf.read_bytes(16);
                self.decimals[i] = Some(Decimal128::from_ieee(&rep).expect("bad decimal"));
            }
        }

        let size = buf.read_u32() as usize;
        self.strings = vec![String::new(); size.max(1)];
        self.strings.truncate(size);
        if size > 0 {
            self.strings[0] = String::new();
        }
        for i in 1..size {
            let length = buf.read_u32();
            self.strings[i] = buf.read_string(length).unwrap_or_default();
            buf.skip(length as i64);
        }

        let size = buf.read_u32() as usize;
        self.namespaces = (0..size).map(|_| None).collect();
        for i in 1..size {
            let ns_kind = buf.read_u8();
            let name_offset = buf.read_u32();
            let name = self.strings[name_offset as usize].clone();
            self.namespaces[i] = Some(Namespace { name_offset, ns_kind: ns_kind as i8, name });
        }

        let size = buf.read_u32() as usize;
        self.namespace_sets = (0..size).map(|_| None).collect();
        for i in 1..size {
            let count = buf.read_u32();
            let ns_ids = (0..count).map(|_| buf.read_u32()).collect();
            self.namespace_sets[i] = Some(NamespaceSet { namespace_ids: ns_ids });
        }

        let size = buf.read_u32() as usize;
        self.name_data = (0..size).map(|_| None).collect();
        *self.binary_multinames.borrow_mut() = vec![None; size];
        for i in 1..size {
            let kind = buf.read_u8();
            let params = match kind {
                CONSTANT_Qname | CONSTANT_QnameA => {
                    let a = buf.read_u32();
                    let b = buf.read_u32();
                    Some(vec![a, b])
                }
                CONSTANT_RTQname | CONSTANT_RTQnameA => Some(vec![buf.read_u32()]),
                CONSTANT_Multiname | CONSTANT_MultinameA => {
                    let a = buf.read_u32();
                    let b = buf.read_u32();
                    Some(vec![a, b])
                }
                CONSTANT_MultinameL | CONSTANT_MultinameLA => Some(vec![buf.read_u32()]),
                CONSTANT_TypeName => {
                    let name_index = buf.read_u32();
                    let count = buf.read_u32();
                    let mut entries = vec![name_index, count];
                    for _ in 0..count {
                        entries.push(buf.read_u32());
                    }
                    Some(entries)
                }
                CONSTANT_RTQnameL | CONSTANT_RTQnameLA => None,
                _ => panic!("bad multiname type: {}", kind),
            };
            self.name_data[i] = Some(NameData { kind, params });
        }
    }

    fn scan_methods(&mut self, buf: &BytecodeBuffer) {
        let method_entries = buf.read_u32();
        self.methods = Vec::with_capacity(method_entries.max(0) as usize);
        for _ in 0..method_entries {
            let param_count = buf.read_u32();
            let return_type = buf.read_u32();
            let param_types: Vec<i32> = (0..param_count).map(|_| buf.read_u32()).collect();
            let name_index = buf.read_u32();
            let flags = buf.read_u8();
            let optional_param_count = if (flags & METHOD_HasOptional) != 0 { buf.read_u32() } else { 0 };
            let mut optional_param_types = Vec::with_capacity(optional_param_count.max(0) as usize);
            let mut optional_param_kinds = Vec::with_capacity(optional_param_count.max(0) as usize);
            for _ in 0..optional_param_count {
                optional_param_types.push(buf.read_u32());
                optional_param_kinds.push(buf.read_u8());
            }
            let param_name_count = if (flags & METHOD_HasParamNames) != 0 { param_count } else { 0 };
            let param_names: Vec<String> = (0..param_name_count).map(|_| self.strings[buf.read_u32() as usize].clone()).collect();
            self.methods.push(Method { return_type, param_types, name_index, flags, optional_param_types, optional_param_kinds, param_names });
        }
    }

    fn scan_metadata(&mut self, buf: &BytecodeBuffer) {
        let metadata_entries = buf.read_u32();
        self.raw_metadata = Vec::new();
        *self.semantic_metadata.borrow_mut() = vec![None; metadata_entries.max(0) as usize];
        for _ in 0..metadata_entries {
            let name_index = buf.read_u32();
            let value_count = buf.read_u32();
            let keys: Vec<i32> = (0..value_count).map(|_| buf.read_u32()).collect();
            let values: Vec<i32> = (0..value_count).map(|_| buf.read_u32()).collect();
            self.raw_metadata.push(Metadata { name_index, keys, values });
        }
    }

    fn scan_classes(&mut self, buf: &BytecodeBuffer) {
        let class_entries = buf.read_u32();
        self.instance_infos = Vec::new();
        self.class_infos = Vec::new();
        for _ in 0..class_entries {
            let name_index = buf.read_u32();
            let super_index = buf.read_u32();
            let flags = buf.read_u8();
            let protected_ns = if (flags & CLASS_FLAG_protected) != 0 { buf.read_u32() } else { 0 };
            let interface_count = buf.read_u32();
            let interfaces: Vec<i32> = (0..interface_count).map(|_| buf.read_u32()).collect();
            let init_index = buf.read_u32();
            let i_traits = Self::scan_traits(buf);
            self.instance_infos.push(InstanceInfo { name_index, super_index, flags, protected_ns, interfaces, init_index, i_traits });
        }
        for _ in 0..class_entries {
            let clinit_index = buf.read_u32();
            let c_traits = Self::scan_traits(buf);
            self.class_infos.push(ClassInfo { clinit_index, c_traits });
        }
    }

    fn scan_scripts(&mut self, buf: &BytecodeBuffer) {
        let script_count = buf.read_u32();
        self.script_infos = Vec::new();
        for _ in 0..script_count {
            let init_index = buf.read_u32();
            let script_traits = Self::scan_traits(buf);
            self.script_infos.push(ScriptInfo { init_index, script_traits });
        }
    }

    fn scan_traits(buf: &BytecodeBuffer) -> Vec<Trait> {
        let count = buf.read_u32();
        let mut result = Vec::with_capacity(count.max(0) as usize);
        for _ in 0..count {
            let name_index = buf.read_u32();
            let kind = buf.read_u8();
            let tag = kind & 0x0f;
            let mut new_trait = match tag {
                TRAIT_Var | TRAIT_Const => {
                    let slot_id = buf.read_u32();
                    let trait_type = buf.read_u32();
                    let value_index = buf.read_u32();
                    let value_kind = if value_index > 0 { buf.read_u8() } else { 0 };
                    Trait::new(name_index, kind, vec![slot_id, trait_type, value_index, value_kind])
                }
                TRAIT_Method | TRAIT_Getter | TRAIT_Setter => {
                    let disp_id = buf.read_u32();
                    let method = buf.read_u32();
                    Trait::new(name_index, kind, vec![disp_id, method])
                }
                TRAIT_Class | TRAIT_Function => {
                    let slot_id = buf.read_u32();
                    let class_id = buf.read_u32();
                    Trait::new(name_index, kind, vec![slot_id, class_id])
                }
                _ => panic!("NullPointerException: invalid trait kind {}", kind),
            };
            if new_trait.has_metadata() {
                let metadata_count = buf.read_u32();
                let metadata: Vec<i32> = (0..metadata_count).map(|_| buf.read_u32()).collect();
                new_trait.add_metadata(metadata);
            }
            result.push(new_trait);
        }
        result
    }
}
