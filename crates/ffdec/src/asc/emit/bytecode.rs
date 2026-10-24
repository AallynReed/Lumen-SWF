//! Port of `macromedia.asc.embedding.avmplus.ByteCodeFactory`.

use std::collections::HashMap;

use super::constants::*;
use crate::asc::util::decimal128::Decimal128;

pub struct ByteCodeFactory {
    pub cpool_out: String,
    pub defns_out: String,
    pub show_bytecode: bool,

    utf_constants: HashMap<String, Vec<u8>>,
    ns_constants: HashMap<i32, Vec<u8>>,
    ns_package_constants: HashMap<i32, Vec<u8>>,
    ns_private_constants: HashMap<i32, Vec<u8>>,
    ns_internal_constants: HashMap<i32, Vec<u8>>,
    ns_protected_constants: HashMap<i32, Vec<u8>>,
    ns_static_protected_constants: HashMap<i32, Vec<u8>>,
    int_constants: HashMap<i32, Vec<u8>>,
    uint_constants: HashMap<i32, Vec<u8>>,
    false_: Vec<u8>,
    true_: Vec<u8>,
    null: Vec<u8>,
}

impl Default for ByteCodeFactory {
    fn default() -> Self {
        ByteCodeFactory::new()
    }
}

fn flag(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

impl ByteCodeFactory {
    pub fn new() -> ByteCodeFactory {
        let mut f = ByteCodeFactory {
            cpool_out: String::new(),
            defns_out: String::new(),
            show_bytecode: false,
            utf_constants: HashMap::new(),
            ns_constants: HashMap::new(),
            ns_package_constants: HashMap::new(),
            ns_private_constants: HashMap::new(),
            ns_internal_constants: HashMap::new(),
            ns_protected_constants: HashMap::new(),
            ns_static_protected_constants: HashMap::new(),
            int_constants: HashMap::new(),
            uint_constants: HashMap::new(),
            false_: Vec::with_capacity(1),
            true_: Vec::with_capacity(1),
            null: Vec::with_capacity(1),
        };
        byte(&mut f.false_, CONSTANT_False);
        byte(&mut f.true_, CONSTANT_True);
        byte(&mut f.null, CONSTANT_Null);
        f
    }

    pub fn clear(&mut self) {
        self.utf_constants.clear();
        self.ns_constants.clear();
        self.ns_package_constants.clear();
        self.ns_private_constants.clear();
        self.ns_internal_constants.clear();
        self.ns_protected_constants.clear();
        self.ns_static_protected_constants.clear();
        self.int_constants.clear();
        self.uint_constants.clear();
    }

    pub fn alloc_bytes() -> Vec<u8> {
        Vec::new()
    }

    pub fn alloc_bytes_sized(size: usize) -> Vec<u8> {
        Vec::with_capacity(size)
    }

    pub fn constant_value(&mut self, value: i32) -> Vec<u8> {
        if self.show_bytecode {
            match value {
                CONSTANT_False => self.cpool_out.push_str("\n      ConstantFalse"),
                CONSTANT_True => self.cpool_out.push_str("\n      ConstantTrue"),
                CONSTANT_Null => self.cpool_out.push_str("\n      ConstantNull"),
                _ => {}
            }
        }

        match value {
            CONSTANT_False => self.false_.clone(),
            CONSTANT_True => self.true_.clone(),
            CONSTANT_Null => self.null.clone(),
            _ => {
                let mut bytes = Vec::with_capacity(1);
                byte(&mut bytes, value);
                bytes
            }
        }
    }

    fn cached_namespace(&mut self, label: &str, kind: i32, uri_index: i32) -> Vec<u8> {
        if self.show_bytecode {
            self.cpool_out.push_str(&format!("\n      {} {}", label, uri_index));
        }
        let map = match kind {
            CONSTANT_Namespace => &mut self.ns_constants,
            CONSTANT_PrivateNamespace => &mut self.ns_private_constants,
            CONSTANT_PackageNamespace => &mut self.ns_package_constants,
            CONSTANT_PackageInternalNs => &mut self.ns_internal_constants,
            CONSTANT_ProtectedNamespace => &mut self.ns_protected_constants,
            _ => &mut self.ns_static_protected_constants,
        };
        if let Some(b) = map.get(&uri_index) {
            return b.clone();
        }
        let mut bytes = Vec::with_capacity(3);
        byte(&mut bytes, kind);
        int(&mut bytes, uri_index as i64);
        map.insert(uri_index, bytes.clone());
        bytes
    }

    pub fn constant_namespace(&mut self, uri_index: i32) -> Vec<u8> {
        self.cached_namespace("ConstantNamespace", CONSTANT_Namespace, uri_index)
    }

    pub fn constant_private_namespace(&mut self, uri_index: i32) -> Vec<u8> {
        self.cached_namespace("ConstantPrivateNamespace", CONSTANT_PrivateNamespace, uri_index)
    }

    pub fn constant_package_namespace(&mut self, uri_index: i32) -> Vec<u8> {
        self.cached_namespace("ConstantPackageNamespace", CONSTANT_PackageNamespace, uri_index)
    }

    pub fn constant_internal_namespace(&mut self, uri_index: i32) -> Vec<u8> {
        self.cached_namespace("ConstantInternalNamespace", CONSTANT_PackageInternalNs, uri_index)
    }

    pub fn constant_protected_namespace(&mut self, uri_index: i32) -> Vec<u8> {
        self.cached_namespace("ConstantProtectedNamespace", CONSTANT_ProtectedNamespace, uri_index)
    }

    pub fn constant_static_protected_namespace(&mut self, uri_index: i32) -> Vec<u8> {
        self.cached_namespace("ConstantStaticProtectedNamespace", CONSTANT_StaticProtectedNs, uri_index)
    }

    /// `namespaces` in the caller's `Set` iteration order.
    pub fn constant_namespace_set(&mut self, namespaces: &[i32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(5 + namespaces.len() * 2);
        self.constant_namespace_set_into(&mut bytes, namespaces);
        bytes
    }

    pub fn constant_namespace_set_into(&mut self, bytes: &mut Vec<u8>, namespaces: &[i32]) {
        if self.show_bytecode {
            self.cpool_out.push_str(&format!("\n      ConstantNamespaceSet {} {{", namespaces.len()));
            for &ns_id in namespaces {
                if ns_id == 0 {
                    break;
                }
                self.cpool_out.push_str(&format!(" {}", ns_id));
            }
            self.cpool_out.push_str(" }");
        }

        int(bytes, namespaces.len() as i64);
        for &ns_id in namespaces {
            int(bytes, ns_id as i64);
        }
    }

    pub fn constant_type_name_into(&mut self, bytes: &mut Vec<u8>, name_index: i32, types: &[i32]) {
        if self.show_bytecode {
            self.cpool_out.push_str(&format!("\n     ConstantMultiname {} {{", name_index));
            for t in types {
                self.cpool_out.push_str(&format!(" {}", t));
            }
            self.cpool_out.push_str(" }");
        }
        byte(bytes, CONSTANT_TypeName);
        int(bytes, name_index as i64);
        int(bytes, types.len() as i64);
        for &t in types {
            int(bytes, t as i64);
        }
    }

    pub fn constant_type_name(&mut self, name_index: i32, types: &[i32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(3 + types.len() * 2);
        self.constant_type_name_into(&mut bytes, name_index, types);
        bytes
    }

    pub fn constant_multiname_into(&mut self, bytes: &mut Vec<u8>, name_index: i32, namespace_set_index: i32, is_attribute: bool) {
        if self.show_bytecode {
            self.cpool_out.push_str(&format!(
                "\n      ConstantMultiname {}{} {}",
                if is_attribute { "@" } else { "" },
                name_index,
                namespace_set_index
            ));
        }
        byte(bytes, if is_attribute { CONSTANT_MultinameA } else { CONSTANT_Multiname });
        int(bytes, name_index as i64);
        int(bytes, namespace_set_index as i64);
    }

    pub fn constant_multiname(&mut self, name_index: i32, namespace_set_index: i32, is_attribute: bool) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(5);
        self.constant_multiname_into(&mut bytes, name_index, namespace_set_index, is_attribute);
        bytes
    }

    pub fn constant_multiname_late_into(&mut self, bytes: &mut Vec<u8>, namespace_set_index: i32, is_attribute: bool) {
        if self.show_bytecode {
            self.cpool_out.push_str(&format!(
                "\n      ConstantMultinameLate {}{}",
                if is_attribute { "@" } else { "" },
                namespace_set_index
            ));
        }
        byte(bytes, if is_attribute { CONSTANT_MultinameLA } else { CONSTANT_MultinameL });
        int(bytes, namespace_set_index as i64);
    }

    pub fn constant_multiname_late(&mut self, namespace_set_index: i32, is_attribute: bool) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(5);
        self.constant_multiname_late_into(&mut bytes, namespace_set_index, is_attribute);
        bytes
    }

    pub fn constant_utf8_info(&mut self, text: &str) -> Vec<u8> {
        if self.show_bytecode {
            self.cpool_out.push_str(&format!("\n      ConstantUtf8Info {}", text));
        }

        if let Some(b) = self.utf_constants.get(text) {
            return b.clone();
        }

        let utf8_bytes = text.as_bytes();
        let text_length = utf8_bytes.len();
        let mut bytes = Vec::with_capacity(text_length + 3);
        int(&mut bytes, text_length as i64);
        bytes.extend_from_slice(utf8_bytes);

        self.utf_constants.insert(text.to_string(), bytes.clone());
        bytes
    }

    pub fn constant_integer_info(&mut self, value: i32) -> Vec<u8> {
        if self.show_bytecode {
            self.cpool_out.push_str(&format!("\n      ConstantIntegerInfo {}", value));
        }

        if let Some(b) = self.int_constants.get(&value) {
            return b.clone();
        }

        let mut bytes = Vec::with_capacity(5);
        int(&mut bytes, value as i64);

        self.int_constants.insert(value, bytes.clone());
        bytes
    }

    pub fn constant_uint_info(&mut self, value: i64) -> Vec<u8> {
        if self.show_bytecode {
            self.cpool_out.push_str(&format!("\n      ConstantUintInfo {}", value));
        }

        let num = value as i32;
        if let Some(b) = self.uint_constants.get(&num) {
            return b.clone();
        }

        let mut bytes = Vec::with_capacity(5);
        int(&mut bytes, value);

        self.uint_constants.insert(num, bytes.clone());
        bytes
    }

    pub fn constant_double_info_into(&mut self, bytes: &mut Vec<u8>, value: f64) {
        if self.show_bytecode {
            let mut num_str = String::new();
            il_format_double_as_string(value, &mut num_str);
            self.cpool_out.push_str(&format!("\n      ConstantDoubleInfo {}", num_str));
        }
        double(bytes, value);
    }

    pub fn constant_double_info(&mut self, value: f64) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(9);
        self.constant_double_info_into(&mut bytes, value);
        bytes
    }

    /// Java's `ByteList.set` replaces whatever `bytes` held with the 16 IEEE bytes.
    pub fn constant_decimal_info_into(&mut self, bytes: &mut Vec<u8>, value: &Decimal128) {
        if self.show_bytecode {
            self.cpool_out.push_str(&format!("\n      ConstantDecimalInfo {}", value));
        }
        *bytes = value.to_byte_list();
    }

    pub fn constant_decimal_info(&mut self, value: &Decimal128) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(16);
        self.constant_decimal_info_into(&mut bytes, value);
        bytes
    }

    pub fn constant_qualified_name_into(&mut self, bytes: &mut Vec<u8>, name_index: i32, namespace_index: i32, is_attribute: bool) {
        if self.show_bytecode {
            self.cpool_out.push_str(&format!(
                "\n      ConstantQualifiedName {}{} {}",
                if is_attribute { "@" } else { "" },
                name_index,
                namespace_index
            ));
        }
        byte(bytes, if is_attribute { CONSTANT_QnameA } else { CONSTANT_Qname });
        int(bytes, namespace_index as i64);
        int(bytes, name_index as i64);
    }

    pub fn constant_qualified_name(&mut self, name_index: i32, namespace_index: i32, is_attribute: bool) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(5);
        self.constant_qualified_name_into(&mut bytes, name_index, namespace_index, is_attribute);
        bytes
    }

    pub fn constant_runtime_qualified_name_into(&mut self, bytes: &mut Vec<u8>, name_index: i32, is_attribute: bool) {
        if self.show_bytecode {
            self.cpool_out.push_str(&format!("\n      ConstantRuntimeQualifiedName {}{}", if is_attribute { "@" } else { "" }, name_index));
        }
        byte(bytes, if is_attribute { CONSTANT_RTQnameA } else { CONSTANT_RTQname });
        int(bytes, name_index as i64);
    }

    pub fn constant_runtime_qualified_name(&mut self, name_index: i32, is_attribute: bool) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(3);
        self.constant_runtime_qualified_name_into(&mut bytes, name_index, is_attribute);
        bytes
    }

    pub fn constant_runtime_qualified_late_into(&mut self, bytes: &mut Vec<u8>, is_attribute: bool) {
        if self.show_bytecode {
            self.cpool_out.push_str(&format!("\n      ConstantRuntimeQualifiedLate {}", if is_attribute { "@" } else { "" }));
        }
        byte(bytes, if is_attribute { CONSTANT_RTQnameLA } else { CONSTANT_RTQnameL });
    }

    pub fn constant_runtime_qualified_late(&mut self, is_attribute: bool) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(1);
        self.constant_runtime_qualified_late_into(&mut bytes, is_attribute);
        bytes
    }

    #[allow(clippy::too_many_arguments)]
    pub fn trait_info(
        &mut self,
        bytes: &mut Vec<u8>,
        name_index: i32,
        tag: i32,
        id: i32,
        info: i32,
        other: i32,
        other2: i8,
        metadata: Option<&[i32]>,
    ) {
        if self.show_bytecode {
            let fv = |o: i32| {
                format!("{}{}", if (o & 0x01) != 0 { " final" } else { " virtual" }, if (o & 0x02) != 0 { " override" } else { " new" })
            };
            let line = match tag {
                TRAIT_Var => Some(format!(
                    "\n      *Trait name_index={} tag=var slot_id={} type_index={} value={} value_kind={}",
                    name_index, id, info, other, other2
                )),
                TRAIT_Const => Some(format!(
                    "\n      *Trait name_index={} tag=const slot_id={} type_index={} value={} value_kind={}",
                    name_index, id, info, other, other2
                )),
                TRAIT_Method => {
                    Some(format!("\n      *Trait name_index={} tag=method disp_id={} method_info={}{}", name_index, id, info, fv(other)))
                }
                TRAIT_Getter => {
                    Some(format!("\n      *Trait name_index={} tag=getter disp_id={} method_info={}{}", name_index, id, info, fv(other)))
                }
                TRAIT_Setter => {
                    Some(format!("\n      *Trait name_index={} tag=setter disp_id={} method_info={}{}", name_index, id, info, fv(other)))
                }
                TRAIT_Function => {
                    Some(format!("\n      *Trait name_index={} tag=function slot_id={} method_info={}", name_index, id, info))
                }
                TRAIT_Class => Some(format!("\n      *Trait name_index={} tag=class slot_id={} class_info={}", name_index, id, info)),
                _ => None,
            };
            if let Some(line) = line {
                self.defns_out.push_str(&line);
            }
        }
        let mut tag_flags = tag;
        if metadata.is_some() {
            tag_flags |= TRAIT_FLAG_metadata << 4;
        }
        if tag == TRAIT_Method || tag == TRAIT_Getter || tag == TRAIT_Setter {
            tag_flags |= other << 4;
        }
        int(bytes, name_index as i64);
        byte(bytes, tag_flags);
        match tag {
            TRAIT_Var | TRAIT_Const => {
                int(bytes, id as i64);
                int(bytes, info as i64);
                int(bytes, other as i64);
                if other != 0 {
                    byte(bytes, other2 as i32);
                }
            }
            TRAIT_Class | TRAIT_Function => {
                int(bytes, id as i64);
                int(bytes, info as i64);
            }
            TRAIT_Method | TRAIT_Getter | TRAIT_Setter => {
                int(bytes, id as i64);
                int(bytes, info as i64);
            }
            _ => {}
        }
        if let Some(metadata) = metadata {
            int(bytes, metadata.len() as i64);
            for &m in metadata {
                int(bytes, m as i64);
            }
        }
    }

    pub fn var_info(&mut self, bytes: &mut Vec<u8>, type_index: i32, value_index: i32, slot_index: i32, var_info: i32) {
        if self.show_bytecode {
            self.defns_out.push_str(&format!(
                "\n      VarInfo type_index={} value_index={} slot_index={} -> {}",
                type_index, value_index, slot_index, var_info
            ));
        }
        int(bytes, slot_index as i64);
        int(bytes, type_index as i64);
        int(bytes, value_index as i64);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn instance_info(
        &mut self,
        bytes: &mut Vec<u8>,
        name_index: i32,
        base_index: i32,
        flags: i32,
        protected_index: i32,
        interfaces_count: i32,
        interfaces: &[i32],
        iinit_index: i32,
        itraits: &[Vec<u8>],
        class_info: i32,
    ) {
        if self.show_bytecode {
            self.defns_out.push_str(&format!(
                "\n      InstanceInfo name_index={} base_index={} interfaces_count={} interfaces={{",
                name_index, base_index, interfaces_count
            ));
            for i in interfaces {
                self.defns_out.push_str(&format!(" {}", i));
            }
            self.defns_out.push_str(&format!(
                "}} flags={} iinit_index={} itraits_count={} -> {}",
                flags,
                iinit_index,
                itraits.len(),
                class_info
            ));
        }

        int(bytes, name_index as i64);
        int(bytes, base_index as i64);
        byte(bytes, flags);
        if protected_index != 0 {
            int(bytes, protected_index as i64);
        }
        int(bytes, interfaces_count as i64);
        interfaces_table(bytes, interfaces);
        int(bytes, iinit_index as i64);
        int(bytes, itraits.len() as i64);
        traits(bytes, itraits);
    }

    pub fn class_info(&mut self, bytes: &mut Vec<u8>, cinit_index: i32, ctraits: &[Vec<u8>], class_info: i32) {
        if self.show_bytecode {
            self.defns_out.push_str(&format!(
                "\n      ClassInfo  cinit_index={} ctraits_count={} -> {}",
                cinit_index,
                ctraits.len(),
                class_info
            ));
        }

        int(bytes, cinit_index as i64);
        int(bytes, ctraits.len() as i64);
        traits(bytes, ctraits);
    }

    pub fn script_info(&mut self, bytes: &mut Vec<u8>, init_index: i32, traits_: &[Vec<u8>], pkg_info: i32) {
        if self.show_bytecode {
            self.defns_out.push_str(&format!(
                "\n      ScriptInfo init_index={} traits_count={} -> {}",
                init_index,
                traits_.len(),
                pkg_info
            ));
        }
        int(bytes, init_index as i64);
        int(bytes, traits_.len() as i64);
        traits(bytes, traits_);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn method_info(
        &mut self,
        bytes: &mut Vec<u8>,
        param_count: i32,
        return_type: i32,
        param_types: Option<&[i32]>,
        param_values: Option<&[i32]>,
        param_kinds: &[u8],
        param_names: Option<&[i32]>,
        debug_name_index: i32,
        flags: i32,
        method_info_index: i32,
    ) {
        if self.show_bytecode {
            let out = &mut self.defns_out;
            out.push_str("\n      MethodInfo ");
            out.push_str(&format!(" param_count={}", param_count));
            out.push_str(&format!(" return_type={} param_types={{ ", return_type));
            for t in param_types.unwrap_or(&[]) {
                out.push_str(&format!("{} ", t));
            }
            out.push_str(&format!(
                "}} debug_name_index={} needs_arguments={} need_rest={} needs_activation={} has_optional={} ignore_rest={} native={} has_param_names ={}",
                debug_name_index,
                flag((flags & METHOD_Arguments) != 0),
                flag((flags & METHOD_Needrest) != 0),
                flag((flags & METHOD_Activation) != 0),
                flag((flags & METHOD_HasOptional) != 0),
                flag((flags & METHOD_IgnoreRest) != 0),
                flag((flags & METHOD_Native) != 0),
                flag((flags & METHOD_HasParamNames) != 0)
            ));
            if (flags & METHOD_HasOptional) != 0 {
                let values = param_values.expect("optional values");
                out.push_str(&format!(" optional_count={}", values.len()));
                out.push_str(" optional_indexes={ ");
                for v in values {
                    out.push_str(&format!(" {}", v));
                }
                out.push_str(" }");
                out.push_str(" optional_kinds={ ");
                for i in 0..values.len() {
                    out.push_str(&format!(" {}", param_kinds[i] as i8));
                }
                out.push_str(" }");
            }
            if (flags & METHOD_HasParamNames) != 0 {
                out.push_str(" param_names={ ");
                for n in param_names.unwrap_or(&[]) {
                    out.push_str(&format!(" {}", n));
                }
                out.push_str(" }");
            }
            out.push_str(&format!(" -> {}", method_info_index));
        }

        int(bytes, param_count as i64);
        int(bytes, return_type as i64);
        let param_types = param_types.unwrap_or(&[]);
        for i in 0..param_count as usize {
            int(bytes, param_types[i] as i64);
        }
        int(bytes, debug_name_index as i64);
        byte(bytes, flags);
        if (flags & METHOD_HasOptional) != 0 {
            let values = param_values.expect("optional values");
            int(bytes, values.len() as i64);
            for i in 0..values.len() {
                int(bytes, values[i] as i64);
                bytes.push(param_kinds[i]);
            }
        }
        if (flags & METHOD_HasParamNames) != 0 {
            let names = param_names.expect("param names");
            for i in 0..param_count as usize {
                int(bytes, names[i] as i64);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn method_body(
        &mut self,
        bytes: &mut Vec<u8>,
        max_stack: i32,
        max_locals: i32,
        scope_depth: i32,
        max_scope: i32,
        code_length: i32,
        code: &[u8],
        exception_table_length: i32,
        exception_table: &[u8],
        traits_: &[Vec<u8>],
        method_info_index: i32,
    ) {
        if self.show_bytecode {
            self.defns_out.push_str(&format!("\n      MethodBody max_stack={}", max_stack));
            self.defns_out.push_str(&format!(" max_locals={}", max_locals));
            self.defns_out.push_str(&format!(" scope_depth={} max_scope={}", scope_depth, max_scope));
            self.defns_out.push_str(&format!(" code_length={} traits_count={} -> {}", code_length, traits_.len(), method_info_index));
        }

        int(bytes, method_info_index as i64);
        int(bytes, max_stack as i64);
        int(bytes, max_locals as i64);
        int(bytes, scope_depth as i64);
        int(bytes, max_scope as i64);
        int(bytes, code_length as i64);
        if code_length != 0 {
            bytes.extend_from_slice(code);
        }
        int(bytes, exception_table_length as i64);
        if exception_table_length != 0 {
            bytes.extend_from_slice(exception_table);
        }
        int(bytes, traits_.len() as i64);
        traits(bytes, traits_);
    }

    pub fn metadata_info(
        &mut self,
        bytes: &mut Vec<u8>,
        name_index: i32,
        values_count: i32,
        keys: &[i32],
        values: &[i32],
        metadata_info_index: i32,
    ) {
        if self.show_bytecode {
            self.defns_out.push_str("\n      MetadataInfo ");
            self.defns_out.push_str(&format!(" nameIndex={}", name_index));
            self.defns_out.push_str(&format!(" valuesCount={} keys={{ ", values_count));
            for k in keys {
                self.defns_out.push_str(&format!("{} ", k));
            }
            self.defns_out.push_str("} values={");
            for v in values {
                self.defns_out.push_str(&format!("{} ", v));
            }
            self.defns_out.push_str(" }");
            self.defns_out.push_str(&format!(" -> {}", metadata_info_index));
        }

        int(bytes, name_index as i64);
        int(bytes, values_count as i64);
        for i in 0..values_count as usize {
            int(bytes, keys[i] as i64);
        }
        for i in 0..values_count as usize {
            int(bytes, values[i] as i64);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn action_block(
        &mut self,
        bytes: &mut Vec<u8>,
        minor_version: i32,
        major_version: i32,
        constant_int_pool_count: i32,
        constant_int_pool: &[Vec<u8>],
        constant_uint_pool_count: i32,
        constant_uint_pool: &[Vec<u8>],
        constant_double_pool_count: i32,
        constant_double_pool: &[Vec<u8>],
        constant_decimal_pool_count: i32,
        constant_decimal_pool: &[Vec<u8>],
        constant_utf8_pool_count: i32,
        constant_utf8_pool: &[Vec<u8>],
        constant_mn_pool_count: i32,
        constant_mn_pool: &[Vec<u8>],
        constant_nss_pool_count: i32,
        constant_nss_pool: &[Vec<u8>],
        constant_ns_pool_count: i32,
        constant_ns_pool: &[Vec<u8>],
        methods_count: i32,
        methods: &[Vec<u8>],
        metadata_count: i32,
        metadata: &[Vec<u8>],
        classes_count: i32,
        instances: &[Vec<u8>],
        classes: &[Vec<u8>],
        scripts_count: i32,
        scripts: &[Vec<u8>],
        bodies_count: i32,
        bodies: &[Vec<u8>],
    ) {
        if self.show_bytecode {
            self.defns_out.push_str(&format!(
                "\n      ActionBlock major_version={} minor_version={} constant_int_count= {} constant_uint_count=: {} constant_double_count= {} constant_decimal_count= {} constant_utf8_count={} constant_namespace_count={} constant_namespaceset_count={} constant_multiname_count={} methods_count={} metadata_count={} classes_count={} scripts_count={} bodies_count={}",
                major_version,
                minor_version,
                constant_int_pool_count,
                constant_uint_pool_count,
                constant_double_pool_count,
                constant_decimal_pool_count,
                constant_utf8_pool_count,
                constant_ns_pool_count,
                constant_nss_pool_count,
                constant_mn_pool_count,
                methods_count,
                metadata_count,
                classes_count,
                scripts_count,
                bodies_count
            ));
        }
        real_short(bytes, minor_version);
        real_short(bytes, major_version);
        int(bytes, constant_int_pool_count as i64);
        if constant_int_pool_count != 0 {
            constant_pool(bytes, constant_int_pool);
        }
        int(bytes, constant_uint_pool_count as i64);
        if constant_uint_pool_count != 0 {
            constant_pool(bytes, constant_uint_pool);
        }
        int(bytes, constant_double_pool_count as i64);
        if constant_double_pool_count != 0 {
            constant_pool(bytes, constant_double_pool);
        }
        if minor_version >= MINORwithDECIMAL {
            int(bytes, constant_decimal_pool_count as i64);
            if constant_decimal_pool_count != 0 {
                constant_pool(bytes, constant_decimal_pool);
            }
        }
        int(bytes, constant_utf8_pool_count as i64);
        if constant_utf8_pool_count != 0 {
            constant_pool(bytes, constant_utf8_pool);
        }
        int(bytes, constant_ns_pool_count as i64);
        if constant_ns_pool_count != 0 {
            constant_pool(bytes, constant_ns_pool);
        }
        int(bytes, constant_nss_pool_count as i64);
        if constant_nss_pool_count != 0 {
            constant_pool(bytes, constant_nss_pool);
        }
        int(bytes, constant_mn_pool_count as i64);
        if constant_mn_pool_count != 0 {
            constant_pool(bytes, constant_mn_pool);
        }
        int(bytes, methods_count as i64);
        if methods_count != 0 {
            concat(bytes, methods);
        }
        int(bytes, metadata_count as i64);
        if metadata_count != 0 {
            concat(bytes, metadata);
        }
        int(bytes, classes_count as i64);
        if classes_count != 0 {
            concat(bytes, instances);
            concat(bytes, classes);
        }
        int(bytes, scripts_count as i64);
        if scripts_count != 0 {
            concat(bytes, scripts);
        }
        int(bytes, bodies_count as i64);
        if bodies_count != 0 {
            concat(bytes, bodies);
        }
    }
}

fn concat(bytes: &mut Vec<u8>, lists: &[Vec<u8>]) {
    for list in lists {
        bytes.extend_from_slice(list);
    }
}

pub fn constant_pool(bytes: &mut Vec<u8>, constants: &[Vec<u8>]) {
    concat(bytes, constants);
}

pub fn namespaces(bytes: &mut Vec<u8>, namespaces: &[i32]) {
    for &n in namespaces {
        int(bytes, n as i64);
    }
}

/// `Interfaces(bytes, interfaces)`.
pub fn interfaces_table(bytes: &mut Vec<u8>, interfaces: &[i32]) {
    for &i in interfaces {
        int(bytes, i as i64);
    }
}

pub fn traits(bytes: &mut Vec<u8>, traits: &[Vec<u8>]) {
    concat(bytes, traits);
}

pub fn vars(bytes: &mut Vec<u8>, vars: &[Vec<u8>]) {
    concat(bytes, vars);
}

pub fn methods(bytes: &mut Vec<u8>, methods: &[Vec<u8>]) {
    concat(bytes, methods);
}

pub fn metadata(bytes: &mut Vec<u8>, metadata: &[Vec<u8>]) {
    concat(bytes, metadata);
}

pub fn bodies(bytes: &mut Vec<u8>, bodies: &[Vec<u8>]) {
    concat(bytes, bodies);
}

pub fn classes(bytes: &mut Vec<u8>, classes: &[Vec<u8>]) {
    concat(bytes, classes);
}

pub fn instances(bytes: &mut Vec<u8>, instances: &[Vec<u8>]) {
    concat(bytes, instances);
}

pub fn packages(bytes: &mut Vec<u8>, scripts: &[Vec<u8>]) {
    concat(bytes, scripts);
}

pub fn attributes(bytes: &mut Vec<u8>, attributes: &[Vec<u8>]) {
    concat(bytes, attributes);
}

pub fn byte(bytes: &mut Vec<u8>, v: i32) {
    bytes.push(v as u8);
}

pub fn real_short(bytes: &mut Vec<u8>, v: i32) {
    bytes.push(v as u8);
    bytes.push((v >> 8) as u8);
}

pub fn int24(bytes: &mut Vec<u8>, v: i32) {
    bytes.push(v as u8);
    bytes.push((v >> 8) as u8);
    bytes.push((v >> 16) as u8);
}

pub fn int(bytes: &mut Vec<u8>, v: i64) {
    if v < 128 && v > -1 {
        bytes.push(v as u8);
    } else if v < 16384 && v > -1 {
        bytes.push(((v & 0x7F) | 0x80) as u8);
        bytes.push(((v >> 7) & 0x7F) as u8);
    } else if v < 2097152 && v > -1 {
        bytes.push(((v & 0x7F) | 0x80) as u8);
        bytes.push((v >> 7 | 0x80) as u8);
        bytes.push(((v >> 14) & 0x7F) as u8);
    } else if v < 268435456 && v > -1 {
        bytes.push(((v & 0x7F) | 0x80) as u8);
        bytes.push((v >> 7 | 0x80) as u8);
        bytes.push((v >> 14 | 0x80) as u8);
        bytes.push(((v >> 21) & 0x7F) as u8);
    } else {
        bytes.push(((v & 0x7F) | 0x80) as u8);
        bytes.push((v >> 7 | 0x80) as u8);
        bytes.push((v >> 14 | 0x80) as u8);
        bytes.push((v >> 21 | 0x80) as u8);
        bytes.push(((v >> 28) & 0x0F) as u8);
    }
}

pub fn double(bytes: &mut Vec<u8>, v: f64) {
    let bits = if v.is_nan() { 0x7ff8000000000000u64 } else { v.to_bits() };
    for k in 0..8 {
        bytes.push((bits >> (8 * k)) as u8);
    }
}

/// The C++-compatible double formatting used for `.il` dumps.
pub fn il_format_double_as_string(mut val: f64, buff: &mut String) {
    if val == 0.0 {
        buff.push_str("0.0");
        return;
    } else if val == f64::INFINITY {
        buff.push_str("Infinity");
        return;
    } else if val == f64::NEG_INFINITY {
        buff.push_str("-Infinity");
        return;
    } else if val.is_nan() {
        buff.push_str("NaN");
        return;
    } else if val == (val as i32) as f64 {
        buff.push_str(&(val as i32).to_string());
        return;
    }

    let mut neg = false;
    if val < 0.0 {
        val = 0.0 - val;
        neg = true;
    }

    let mut exp = 0;
    if val >= 1.0 {
        while val > 10.0 {
            val /= 10.0;
            exp += 1;
        }
        let mut trunc_val = val as i32;
        buff.push_str(if neg { "-" } else { "+" });
        buff.push_str(&format!("{}.", trunc_val));

        for _ in 0..5 {
            val -= trunc_val as f64;
            val *= 10.0;
            trunc_val = val as i32;
            buff.push_str(&trunc_val.to_string());
        }

        if exp > 0 {
            buff.push_str(&format!("e{}", exp));
        }
    } else {
        while val < 1.0 {
            val *= 10.0;
            exp += 1;
        }
        let mut trunc_val = val as i32;
        buff.push_str(if neg { "-" } else { "+" });
        buff.push_str(&format!("{}.", trunc_val));

        for _ in 0..5 {
            val -= trunc_val as f64;
            val *= 10.0;
            trunc_val = val as i32;
            buff.push_str(&trunc_val.to_string());
        }
        buff.push_str(&format!("e-{}", exp));
    }
}
