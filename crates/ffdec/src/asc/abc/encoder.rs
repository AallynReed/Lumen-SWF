//! Port of `macromedia.abc.Encoder`: re-encodes decoded abc, merging constant pools and applying peephole rewrites.

use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use super::bytecode_buffer::{ArrayKind, ByteArrayPool, BytecodeBuffer};
use super::constant_pool::{ConstantPool, CP_DECIMAL, CP_DOUBLE, CP_INT, CP_MN, CP_NS, CP_NSSET, CP_STRING, CP_UINT};
use super::decoder::Decoder;
use super::decoder_exception::DecoderException;
use super::visitor::Visitor;
use crate::asc::emit::constants::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Current {
    ClassInfo,
    ScriptInfo,
    MethodBodies,
}

pub struct Encoder {
    pool: Option<ConstantPool>,
    major_version: i32,
    minor_version: i32,
    pool_index: i32,
    opcode_pass: i32,
    ex_pass: i32,
    disable_debugging: bool,
    remove_metadata: bool,
    peep_hole: bool,

    keep_metadata: HashSet<String>,

    method_info: BytecodeBuffer2,
    metadata_info: ByteArrayPool2,
    class_info: BytecodeBuffer2,
    script_info: BytecodeBuffer2,
    method_bodies: BytecodeBuffer2,
    opcodes: BytecodeBuffer3,
    exceptions: BytecodeBuffer,

    current_buffer: Option<Current>,

    pools: Vec<Rc<ConstantPool>>,

    window: [i32; W],
    window_size: i32,
    head: i32,
}

const W: usize = 8;

fn slot_value_kind(value_kind: i32) -> i32 {
    match value_kind {
        CONSTANT_Utf8 => CP_STRING,
        CONSTANT_Integer => CP_INT,
        CONSTANT_UInteger => CP_UINT,
        CONSTANT_Double => CP_DOUBLE,
        CONSTANT_Decimal => CP_DECIMAL,
        CONSTANT_Namespace
        | CONSTANT_PrivateNamespace
        | CONSTANT_PackageNamespace
        | CONSTANT_PackageInternalNs
        | CONSTANT_ProtectedNamespace
        | CONSTANT_ExplicitNamespace
        | CONSTANT_StaticProtectedNs => CP_NS,
        CONSTANT_Qname | CONSTANT_QnameA | CONSTANT_Multiname | CONSTANT_MultinameA | CONSTANT_TypeName => CP_MN,
        CONSTANT_Namespace_Set => CP_NSSET,
        _ => -1,
    }
}

impl Encoder {
    pub fn new(major_version: i32, minor_version: i32) -> Encoder {
        Encoder {
            pool: None,
            major_version,
            minor_version,
            pool_index: 0,
            opcode_pass: 0,
            ex_pass: 0,
            disable_debugging: false,
            remove_metadata: false,
            peep_hole: false,
            keep_metadata: HashSet::new(),
            method_info: BytecodeBuffer2::new(0, Vec::new()),
            metadata_info: ByteArrayPool2::new(Vec::new()),
            class_info: BytecodeBuffer2::new(0, Vec::new()),
            script_info: BytecodeBuffer2::new(0, Vec::new()),
            method_bodies: BytecodeBuffer2::new(0, Vec::new()),
            opcodes: BytecodeBuffer3::new(Vec::new(), 4096),
            exceptions: BytecodeBuffer::with_size(4096),
            current_buffer: None,
            pools: Vec::new(),
            window: [0; W],
            window_size: 0,
            head: 0,
        }
    }

    pub fn enable_peep_hole(&mut self) {
        self.peep_hole = true;
    }

    pub fn disable_debugging(&mut self) {
        self.disable_debugging = true;
        self.history().disable_debugging();
    }

    pub fn remove_metadata(&mut self) {
        self.remove_metadata = true;
    }

    pub fn add_metadata_to_keep(&mut self, meta_name: &str) {
        self.keep_metadata.insert(meta_name.to_string());
    }

    pub fn add_constant_pools(&mut self, pools: Vec<Rc<ConstantPool>>) {
        self.pool = Some(ConstantPool::merge(&pools));
        self.pools = pools;
        if self.disable_debugging {
            self.history().disable_debugging();
        }
    }

    /// Java's testing hook; its whole body is commented out.
    pub fn test(&self) {}

    pub fn configure(&mut self, decoders: &[Decoder]) {
        let mut estimated_size = 0;
        let mut total = 0;
        let mut sizes = vec![0; decoders.len()];
        for (i, d) in decoders.iter().enumerate() {
            estimated_size += d.method_info.estimated_size;
            sizes[i] = d.method_info.size();
            total += sizes[i];
        }
        self.method_info = BytecodeBuffer2::new(estimated_size, sizes);
        self.method_info.write_u32(total);

        let mut sizes = vec![0; decoders.len()];
        for (i, d) in decoders.iter().enumerate() {
            sizes[i] = d.metadata_info.size();
        }
        self.metadata_info = ByteArrayPool2::new(sizes);

        let mut estimated_size = 0;
        let mut total = 0;
        let mut sizes = vec![0; decoders.len()];
        for (i, d) in decoders.iter().enumerate() {
            estimated_size += d.class_info.estimated_size;
            sizes[i] = d.class_info.size();
            total += sizes[i];
        }
        self.class_info = BytecodeBuffer2::new(estimated_size, sizes);
        self.class_info.write_u32(total);

        let mut estimated_size = 0;
        let mut total = 0;
        let mut sizes = vec![0; decoders.len()];
        for (i, d) in decoders.iter().enumerate() {
            estimated_size += d.script_info.estimated_size;
            sizes[i] = d.script_info.size();
            total += sizes[i];
        }
        self.script_info = BytecodeBuffer2::new(estimated_size, sizes);
        self.script_info.write_u32(total);

        let mut estimated_size = 0;
        let mut total = 0;
        let mut sizes = vec![0; decoders.len()];
        for (i, d) in decoders.iter().enumerate() {
            estimated_size += d.method_bodies.estimated_size;
            sizes[i] = d.method_bodies.size();
            total += sizes[i];
        }
        self.method_bodies = BytecodeBuffer2::new(estimated_size, sizes);
        self.method_bodies.write_u32(total);

        self.opcodes = BytecodeBuffer3::new(decoders.iter().map(|d| d.input()).collect(), 4096);
        self.exceptions = BytecodeBuffer::with_size(4096);
    }

    pub fn use_constant_pool(&mut self, index: i32) {
        self.pool_index = index;
    }

    pub fn to_abc(&mut self) -> Vec<u8> {
        let mut baos = Vec::new();
        baos.push(self.minor_version as u8);
        baos.push((self.minor_version >> 8) as u8);
        baos.push(self.major_version as u8);
        baos.push((self.major_version >> 8) as u8);

        self.pool.as_mut().expect("constant pools were added").write_to(&mut baos);
        let _ = self.method_info.write_to(&mut baos);
        self.metadata_info.write_to_os(&mut baos);
        let _ = self.class_info.write_to(&mut baos);
        let _ = self.script_info.write_to(&mut baos);
        let _ = self.method_bodies.write_to(&mut baos);
        baos
    }

    fn history(&mut self) -> &mut super::constant_pool::IndexHistory {
        self.pool.as_mut().expect("constant pools were added").history.as_mut().expect("merged pool has a history")
    }

    /// `pool.history.getIndex(poolIndex, kind, index)`.
    fn cp(&mut self, kind: i32, index: i32) -> i32 {
        let pool_index = self.pool_index;
        self.history().get_index(pool_index, kind, index)
    }

    fn cur(&mut self) -> &mut BytecodeBuffer {
        match self.current_buffer.expect("currentBuffer is null") {
            Current::ClassInfo => &mut self.class_info,
            Current::ScriptInfo => &mut self.script_info,
            Current::MethodBodies => &mut self.method_bodies,
        }
    }

    fn encode_meta_data(&mut self, kind: i32, metadata: &[i32]) {
        if ((kind >> 4) & TRAIT_FLAG_metadata) != 0 {
            let cur = self.cur();
            cur.write_u32(metadata.len() as i32);
            for &m in metadata {
                cur.write_u32(m);
            }
        }
    }

    fn trim_metadata(&self, metadata: Option<&[i32]>) -> Vec<i32> {
        let mut new_metadata = Vec::new();
        for &m in metadata.unwrap_or(&[]) {
            let new_index = self.metadata_info.get_index(self.pool_index, m);
            if new_index != -1 {
                new_metadata.push(new_index);
            }
        }
        new_metadata
    }

    fn clear_window(&mut self) {
        self.window = [0; W];
        self.window_size = 0;
    }

    fn beginop(&mut self, opcode: i32) {
        self.window[self.head as usize] = self.opcodes.size();
        self.head = (self.head + 1) & (W as i32 - 1);
        if self.window_size < 8 {
            self.window_size += 1;
        }
        self.opcodes.write_u8(opcode);
    }

    fn window_at(&self, i: i32) -> i32 {
        self.window[((self.head - i) & (W as i32 - 1)) as usize]
    }

    fn opat(&self, i: i32) -> i32 {
        if self.peep_hole && i <= self.window_size {
            let ip = self.window_at(i);
            if ip < self.opcodes.size() {
                return self.opcodes.read_u8_at(ip);
            }
        }
        0
    }

    fn set_opcode_at(&mut self, i: i32, opcode: i32) {
        if i <= self.window_size {
            let ip = self.window_at(i);
            if ip < self.opcodes.size() {
                self.opcodes.write_u8_at(ip, opcode);
            }
        }
    }

    fn read_byte_at(&self, i: i32) -> i32 {
        if i <= self.window_size {
            let ip = 1 + self.window_at(i);
            return self.opcodes.read_u8_at(ip) as i8 as i32;
        }
        0
    }

    fn read_int_at(&self, i: i32) -> i32 {
        if i <= self.window_size {
            let ip = 1 + self.window_at(i);
            return self.opcodes.read_u32_at(ip) as i32;
        }
        0
    }

    fn rewind(&mut self, i: i32) {
        let to = (self.head - i) & (W as i32 - 1);
        let ip = self.window[to as usize];
        let end = self.opcodes.size();
        self.opcodes.delete(end - ip);
        self.head = to;
        self.window_size -= i;
    }
}

impl Visitor for Encoder {
    fn method_info(
        &mut self,
        return_type: i32,
        param_types: Option<&[i32]>,
        native_name: i32,
        mut flags: i32,
        values: Option<&[i32]>,
        value_kinds: Option<&[i32]>,
        param_names: Option<&[i32]>,
    ) -> Result<(), DecoderException> {
        self.method_info.write_u32(param_types.map_or(0, |p| p.len() as i32));

        let v = self.cp(CP_MN, return_type);
        self.method_info.write_u32(v);

        for &p in param_types.unwrap_or(&[]) {
            let v = self.cp(CP_MN, p);
            self.method_info.write_u32(v);
        }

        let v = if self.disable_debugging { 0 } else { self.cp(CP_STRING, native_name) };
        self.method_info.write_u32(v);

        if self.disable_debugging {
            flags &= !METHOD_HasParamNames;
        }

        self.method_info.write_u8(flags);

        if (flags & METHOD_HasOptional) != 0 {
            self.method_info.write_u32(values.map_or(0, |v| v.len() as i32));
        }

        let values = values.unwrap_or(&[]);
        let value_kinds = value_kinds.unwrap_or(&[]);
        for i in 0..values.len() {
            let kind = slot_value_kind(value_kinds[i]);
            let new_index = match value_kinds[i] {
                0 | CONSTANT_False | CONSTANT_True | CONSTANT_Null => values[i],
                _ => {
                    if kind == -1 {
                        println!("writing MethodInfo: don't know what constant type it is... {},{}", value_kinds[i], values[i]);
                    }
                    self.cp(kind, values[i])
                }
            };
            self.method_info.write_u32(new_index);
            self.method_info.write_u8(value_kinds[i]);
        }
        if (flags & METHOD_HasParamNames) != 0 {
            if let Some(param_names) = param_names {
                for _ in 0..param_names.len() {
                    // Java writes the return type's index here, not the parameter name's
                    let v = self.cp(CP_STRING, return_type);
                    self.method_info.write_u32(v);
                }
            }
        }
        Ok(())
    }

    fn metadata_info(&mut self, index: i32, name: i32, keys: Option<&[i32]>, values: Option<&[i32]>) -> Result<(), DecoderException> {
        let s = match self.pools[self.pool_index as usize].get_string(name) {
            Ok(s) => s,
            Err(_) => return Ok(()),
        };
        if self.remove_metadata && !s.as_ref().is_some_and(|s| self.keep_metadata.contains(s)) {
            return Ok(());
        }

        let mut b = BytecodeBuffer::with_size(6);
        b.write_u32(self.cp(CP_STRING, name));
        b.write_u32(keys.map_or(0, |k| k.len() as i32));

        for &k in keys.unwrap_or(&[]) {
            b.write_u32(self.cp(CP_STRING, k));
        }

        for &v in values.unwrap_or(&[]) {
            b.write_u32(self.cp(CP_STRING, v));
        }

        let pool_index = self.pool_index;
        self.metadata_info.add_byte_array(pool_index, index, &b);
        Ok(())
    }

    fn start_instance(
        &mut self,
        name: i32,
        super_name: i32,
        is_dynamic: bool,
        is_final: bool,
        is_interface: bool,
        interfaces: Option<&[i32]>,
        iinit: i32,
        protected_namespace: i32,
    ) -> Result<(), DecoderException> {
        let v = self.cp(CP_MN, name);
        self.class_info.write_u32(v);
        let v = self.cp(CP_MN, super_name);
        self.class_info.write_u32(v);

        let mut flags = 0;
        flags = if is_final { flags | CLASS_FLAG_final } else { flags };
        flags = if !is_dynamic { flags | CLASS_FLAG_sealed } else { flags };
        flags = if is_interface { flags | CLASS_FLAG_interface } else { flags };
        flags = if protected_namespace != 0 { flags | CLASS_FLAG_protected } else { flags };
        self.class_info.write_u8(flags);

        if protected_namespace != 0 {
            let v = self.cp(CP_NS, protected_namespace);
            self.class_info.write_u32(v);
        }

        self.class_info.write_u32(interfaces.map_or(0, |i| i.len() as i32));

        for &i in interfaces.unwrap_or(&[]) {
            let v = self.cp(CP_MN, i);
            self.class_info.write_u32(v);
        }

        let v = self.method_info.get_index(self.pool_index, iinit);
        self.class_info.write_u32(v);

        self.current_buffer = Some(Current::ClassInfo);
        Ok(())
    }

    fn end_instance(&mut self) {
        self.current_buffer = None;
    }

    fn start_class(&mut self, _name: i32, cinit: i32) -> Result<(), DecoderException> {
        let v = self.method_info.get_index(self.pool_index, cinit);
        self.class_info.write_u32(v);

        self.current_buffer = Some(Current::ClassInfo);
        Ok(())
    }

    fn end_class(&mut self) {
        self.current_buffer = None;
    }

    fn start_script(&mut self, init_id: i32) {
        let v = self.method_info.get_index(self.pool_index, init_id);
        self.script_info.write_u32(v);

        self.current_buffer = Some(Current::ScriptInfo);
    }

    fn end_script(&mut self) {
        self.current_buffer = None;
    }

    fn start_method_body(
        &mut self,
        method_info: i32,
        max_stack: i32,
        max_regs: i32,
        scope_depth: i32,
        max_scope: i32,
        _code_start: i32,
        _code_length: i64,
    ) -> Result<(), DecoderException> {
        let v = self.method_info.get_index(self.pool_index, method_info);
        self.method_bodies.write_u32(v);
        self.method_bodies.write_u32(max_stack);
        self.method_bodies.write_u32(max_regs);
        self.method_bodies.write_u32(scope_depth);
        self.method_bodies.write_u32(max_scope);

        self.current_buffer = Some(Current::MethodBodies);
        self.opcode_pass = 1;
        self.ex_pass = 1;
        Ok(())
    }

    fn end_method_body(&mut self) {
        self.current_buffer = None;
        self.opcodes.clear();
        self.exceptions.clear();
    }

    fn start_opcodes(&mut self, _method_info: i32) {}

    fn end_opcodes(&mut self) {
        if self.opcode_pass == 1 {
            self.opcode_pass = 2;
        } else if self.opcode_pass == 2 {
            let size = self.opcodes.size();
            self.method_bodies.write_u32(size);
            self.method_bodies.write_bytes(&self.opcodes, 0, size);
        }
    }

    fn exception(&mut self, start: i64, end: i64, target: i64, type_: i32, name: i32) {
        if self.ex_pass == 2 {
            let v = self.opcodes.get_offset(start);
            self.exceptions.write_u32(v);
            let v = self.opcodes.get_offset(end);
            self.exceptions.write_u32(v);
            let v = self.opcodes.get_offset(target);
            self.exceptions.write_u32(v);
            let v = self.cp(CP_MN, type_);
            self.exceptions.write_u32(v);
            if self.minor_version != 15 {
                let v = self.cp(CP_MN, name);
                self.exceptions.write_u32(v);
            }
        }
    }

    fn start_exceptions(&mut self, exception_count: i32) {
        if self.ex_pass == 2 {
            self.exceptions.write_u32(exception_count);
        }
    }

    fn end_exceptions(&mut self) {
        if self.ex_pass == 1 {
            self.ex_pass += 1;
        } else if self.ex_pass == 2 {
            let size = self.exceptions.size();
            self.method_bodies.write_bytes(&self.exceptions, 0, size);
        }
    }

    fn trait_count(&mut self, trait_count: i32) {
        self.cur().write_u32(trait_count);
    }

    fn slot_trait(
        &mut self,
        mut trait_kind: i32,
        name: i32,
        slot_id: i32,
        type_: i32,
        value: i32,
        value_kind: i32,
        metadata: Option<&[i32]>,
    ) -> Result<(), DecoderException> {
        let v = self.cp(CP_MN, name);
        self.cur().write_u32(v);
        let new_metadata = self.trim_metadata(metadata);
        if ((trait_kind >> 4) & TRAIT_FLAG_metadata) != 0 && new_metadata.is_empty() {
            trait_kind &= !(TRAIT_FLAG_metadata << 4);
        }
        self.cur().write_u8(trait_kind);

        self.cur().write_u32(slot_id);
        let v = self.cp(CP_MN, type_);
        self.cur().write_u32(v);

        let kind = slot_value_kind(value_kind);
        let new_index = match value_kind {
            0 | CONSTANT_False | CONSTANT_True | CONSTANT_Null => value,
            _ => {
                if kind == -1 {
                    println!("writing slotTrait: don't know what constant type it is... {},{}", value_kind, value);
                }
                self.cp(kind, value)
            }
        };

        self.cur().write_u32(new_index);
        if value != 0 {
            self.cur().write_u8(value_kind);
        }

        self.encode_meta_data(trait_kind, &new_metadata);
        Ok(())
    }

    fn method_trait(
        &mut self,
        mut trait_kind: i32,
        name: i32,
        disp_id: i32,
        method_info: i32,
        metadata: Option<&[i32]>,
    ) -> Result<(), DecoderException> {
        let v = self.cp(CP_MN, name);
        self.cur().write_u32(v);
        let new_metadata = self.trim_metadata(metadata);
        if ((trait_kind >> 4) & TRAIT_FLAG_metadata) != 0 && new_metadata.is_empty() {
            trait_kind &= !(TRAIT_FLAG_metadata << 4);
        }
        self.cur().write_u8(trait_kind);

        self.cur().write_u32(disp_id);
        let v = self.method_info.get_index(self.pool_index, method_info);
        self.cur().write_u32(v);

        self.encode_meta_data(trait_kind, &new_metadata);
        Ok(())
    }

    fn class_trait(
        &mut self,
        mut kind: i32,
        name: i32,
        slot_id: i32,
        class_index: i32,
        metadata: Option<&[i32]>,
    ) -> Result<(), DecoderException> {
        let v = self.cp(CP_MN, name);
        self.cur().write_u32(v);
        let new_metadata = self.trim_metadata(metadata);
        if ((kind >> 4) & TRAIT_FLAG_metadata) != 0 && new_metadata.is_empty() {
            kind &= !(TRAIT_FLAG_metadata << 4);
        }
        self.cur().write_u8(kind);

        self.cur().write_u32(slot_id);
        let v = self.class_info.get_index(self.pool_index, class_index);
        self.cur().write_u32(v);

        self.encode_meta_data(kind, &new_metadata);
        Ok(())
    }

    fn function_trait(
        &mut self,
        mut kind: i32,
        name: i32,
        slot_id: i32,
        method_info: i32,
        metadata: Option<&[i32]>,
    ) -> Result<(), DecoderException> {
        let v = self.cp(CP_MN, name);
        self.cur().write_u32(v);
        let new_metadata = self.trim_metadata(metadata);
        if ((kind >> 4) & TRAIT_FLAG_metadata) != 0 && new_metadata.is_empty() {
            kind &= !(TRAIT_FLAG_metadata << 4);
        }
        self.cur().write_u8(kind);

        self.cur().write_u32(slot_id);
        let v = self.method_info.get_index(self.pool_index, method_info);
        self.cur().write_u32(v);

        self.encode_meta_data(kind, &new_metadata);
        Ok(())
    }

    fn target(&mut self, old_pos: i32) {
        if self.opcode_pass == 1 {
            self.opcodes.map_offsets(old_pos as i64);
            self.clear_window();
        }
    }

    fn op_returnvoid(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_returnvoid);
        }
    }

    fn op_returnvalue(&mut self) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_coerce_a {
                self.rewind(1);
            }

            if self.opat(1) == OP_pushundefined {
                self.rewind(1);
                self.op_returnvoid();
                return;
            }

            self.beginop(OP_returnvalue);
        }
    }

    fn op_nop(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_nop);
        }
    }

    fn op_bkpt(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_bkpt);
        }
    }

    fn op_timestamp(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_timestamp);
        }
    }

    fn op_debugline(&mut self, linenum: i32) {
        if self.opcode_pass == 1 {
            if !self.disable_debugging {
                self.beginop(OP_debugline);
                self.opcodes.write_u32(linenum);
            }
        }
    }

    fn op_bkptline(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_bkptline);
        }
    }

    fn op_debug(&mut self, di_local: i32, index: i32, slot: i32, linenum: i32) {
        if self.opcode_pass == 1 {
            if !self.disable_debugging {
                self.beginop(OP_debug);
                self.opcodes.write_u8(di_local);

                let v = self.cp(CP_STRING, index);

                self.opcodes.write_u32(v);
                self.opcodes.write_u8(slot);
                self.opcodes.write_u32(linenum);
            }
        }
    }

    fn op_debugfile(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            if !self.disable_debugging {
                self.beginop(OP_debugfile);
                let v = self.cp(CP_STRING, index);
                self.opcodes.write_u32(v);
            }
        }
    }

    fn op_jump(&mut self, offset: i32, pos: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_jump);
            self.opcodes.write_s24(offset);
            self.opcodes.map_offsets(pos as i64);
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset(self.pool_index, pos as i64 + offset as i64);
        }
    }

    fn op_pushnull(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_pushnull);
        }
    }

    fn op_pushundefined(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_pushundefined);
        }
    }

    fn op_pushstring(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_pushstring);
            let v = self.cp(CP_STRING, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_pushnamespace(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_pushnamespace);
            let v = self.cp(CP_NS, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_pushint(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_pushint);
            let v = self.cp(CP_INT, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_pushuint(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_pushuint);
            let v = self.cp(CP_UINT, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_pushdouble(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_pushdouble);
            let v = self.cp(CP_DOUBLE, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_pushdecimal(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_pushdecimal);
            let v = self.cp(CP_DECIMAL, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_getlocal(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_setlocal && self.read_int_at(1) == index {
                self.rewind(1);
                self.op_dup();
                self.op_setlocal(index);
                return;
            }

            self.beginop(OP_getlocal);
            self.opcodes.write_u32(index);
        }
    }

    fn op_pushtrue(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_pushtrue);
        }
    }

    fn op_pushfalse(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_pushfalse);
        }
    }

    fn op_pushnan(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_pushnan);
        }
    }

    fn op_pushdnan(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_pushdnan);
        }
    }

    fn op_pop(&mut self) {
        if self.opcode_pass == 1 {
            match self.opat(1) {
                OP_callproperty => {
                    self.set_opcode_at(1, OP_callpropvoid);
                    return;
                }
                OP_callsuper => {
                    self.set_opcode_at(1, OP_callsupervoid);
                    return;
                }
                _ => {}
            }

            self.beginop(OP_pop);
        }
    }

    fn op_dup(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_dup);
        }
    }

    fn op_swap(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_swap);
        }
    }

    fn op_convert_s(&mut self) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_coerce_a {
                self.rewind(1);
            }

            if matches!(self.opat(1), OP_coerce_s | OP_convert_s | OP_pushstring | OP_typeof) {
                return;
            }

            if self.opat(2) == OP_pushstring && self.opat(1) == OP_add {
                return;
            }

            self.beginop(OP_convert_s);
        }
    }

    fn op_esc_xelem(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_esc_xelem);
        }
    }

    fn op_esc_xattr(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_esc_xattr);
        }
    }

    fn op_checkfilter(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_checkfilter);
        }
    }

    fn op_convert_d(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_convert_d);
        }
    }

    fn op_convert_m(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_convert_m);
        }
    }

    fn op_convert_m_p(&mut self, param: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_convert_m);
            self.opcodes.write_u32(param);
        }
    }

    fn op_convert_b(&mut self) {
        if self.opcode_pass == 1 {
            if matches!(
                self.opat(1),
                OP_equals
                    | OP_strictequals
                    | OP_not
                    | OP_greaterthan
                    | OP_lessthan
                    | OP_greaterequals
                    | OP_lessequals
                    | OP_istype
                    | OP_istypelate
                    | OP_instanceof
                    | OP_deleteproperty
                    | OP_in
                    | OP_convert_b
                    | OP_pushtrue
                    | OP_pushfalse
            ) {
                return;
            }

            self.beginop(OP_convert_b);
        }
    }

    fn op_convert_o(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_convert_o);
        }
    }

    fn op_negate(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_negate);
        }
    }

    fn op_negate_p(&mut self, param: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_negate_p);
            self.opcodes.write_u32(param);
        }
    }

    fn op_negate_i(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_negate_i);
        }
    }

    fn op_increment(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_increment);
        }
    }

    fn op_increment_p(&mut self, param: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_increment_p);
            self.opcodes.write_u32(param);
        }
    }

    fn op_increment_i(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_increment_i);
        }
    }

    fn op_inclocal(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_inclocal);
            self.opcodes.write_u32(index);
        }
    }

    fn op_inclocal_p(&mut self, param: i32, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_inclocal_p);
            self.opcodes.write_u32(param);
            self.opcodes.write_u32(index);
        }
    }

    fn op_kill(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            if matches!(self.opat(1), OP_returnvalue | OP_returnvoid) {
                return;
            }

            self.beginop(OP_kill);
            self.opcodes.write_u32(index);
        }
    }

    fn op_inclocal_i(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_inclocal_i);
            self.opcodes.write_u32(index);
        }
    }

    fn op_decrement(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_decrement);
        }
    }

    fn op_decrement_p(&mut self, param: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_decrement_p);
            self.opcodes.write_u32(param);
        }
    }

    fn op_decrement_i(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_decrement_i);
        }
    }

    fn op_declocal(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_declocal);
            self.opcodes.write_u32(index);
        }
    }

    fn op_declocal_p(&mut self, param: i32, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_declocal_p);
            self.opcodes.write_u32(param);
            self.opcodes.write_u32(index);
        }
    }

    fn op_declocal_i(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_declocal_i);
            self.opcodes.write_u32(index);
        }
    }

    fn op_typeof(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_typeof);
        }
    }

    fn op_not(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_not);
        }
    }

    fn op_bitnot(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_bitnot);
        }
    }

    fn op_setlocal(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            if self.opat(2) == OP_getlocal && self.read_int_at(2) == index && self.opat(1) == OP_increment_i {
                self.rewind(2);
                self.op_inclocal_i(index);
                return;
            }

            if self.opat(2) == OP_getlocal && self.read_int_at(2) == index && self.opat(1) == OP_increment {
                self.rewind(2);
                self.op_inclocal(index);
                return;
            }

            self.beginop(OP_setlocal);
            self.opcodes.write_u32(index);
        }
    }

    fn op_add(&mut self) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_coerce_a {
                self.rewind(1);
            }
            self.beginop(OP_add);
        }
    }

    fn op_add_p(&mut self, param: i32) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_coerce_a {
                self.rewind(1);
            }
            self.beginop(OP_add_p);
            self.opcodes.write_u32(param);
        }
    }

    fn op_add_i(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_add_i);
        }
    }

    fn op_subtract(&mut self) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_pushbyte && self.read_byte_at(1) == 1 {
                self.rewind(1);
                self.op_decrement();
                return;
            }
            self.beginop(OP_subtract);
        }
    }

    fn op_subtract_p(&mut self, param: i32) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_pushbyte && self.read_byte_at(1) == 1 {
                self.rewind(1);
                self.op_decrement_p(param);
                return;
            }
            self.beginop(OP_subtract_p);
            self.opcodes.write_u32(param);
        }
    }

    fn op_subtract_i(&mut self) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_pushbyte && self.read_int_at(1) == 1 {
                self.rewind(1);
                self.op_decrement_i();
                return;
            }
            self.beginop(OP_subtract_i);
        }
    }

    fn op_multiply(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_multiply);
        }
    }

    fn op_multiply_p(&mut self, param: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_multiply_p);
            self.opcodes.write_u32(param);
        }
    }

    fn op_multiply_i(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_multiply_i);
        }
    }

    fn op_divide(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_divide);
        }
    }

    fn op_divide_p(&mut self, param: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_divide_p);
            self.opcodes.write_u32(param);
        }
    }

    fn op_modulo(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_modulo);
        }
    }

    fn op_modulo_p(&mut self, param: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_modulo_p);
            self.opcodes.write_u32(param);
        }
    }

    fn op_lshift(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_lshift);
        }
    }

    fn op_rshift(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_rshift);
        }
    }

    fn op_urshift(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_urshift);
        }
    }

    fn op_bitand(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_bitand);
        }
    }

    fn op_bitor(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_bitor);
        }
    }

    fn op_bitxor(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_bitxor);
        }
    }

    fn op_equals(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_equals);
        }
    }

    fn op_strictequals(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_strictequals);
        }
    }

    fn op_lookupswitch(&mut self, default_pos: i32, case_pos: &[i32], old_pos: i32, old_table_pos: i32) {
        if self.opcode_pass == 1 {
            self.opcodes.map_offsets(old_pos as i64);
            self.beginop(OP_lookupswitch);
            self.opcodes.map_offsets(old_pos as i64 + 1);
            self.opcodes.write_s24(default_pos);
            self.opcodes.write_u32(if case_pos.is_empty() { 0 } else { case_pos.len() as i32 - 1 });
            for (i, &c) in case_pos.iter().enumerate() {
                self.opcodes.map_offsets(old_table_pos as i64 + 3 * i as i64);
                self.opcodes.write_s24(c);
            }
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset3(old_pos as i64 + 1, old_pos as i64, old_pos as i64 + default_pos as i64);
            for (i, &c) in case_pos.iter().enumerate() {
                self.opcodes.update_offset3(old_table_pos as i64 + (3 * i as i64), old_pos as i64, old_pos as i64 + c as i64);
            }
        }
    }

    fn op_iftrue(&mut self, offset: i32, pos: i32) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_convert_b {
                self.rewind(1);
            }

            if self.opat(1) == OP_pushtrue {
                self.rewind(1);
                self.op_jump(offset, pos);
                return;
            }

            self.beginop(OP_iftrue);
            self.opcodes.write_s24(offset);
            self.opcodes.map_offsets(pos as i64);
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset(self.pool_index, pos as i64 + offset as i64);
        }
    }

    fn op_iffalse(&mut self, offset: i32, pos: i32) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_convert_b {
                self.rewind(1);
            }

            if self.opat(2) == OP_strictequals && self.opat(1) == OP_not {
                self.rewind(2);
                self.op_ifstricteq(offset, pos);
                return;
            }

            if self.opat(2) == OP_equals && self.opat(1) == OP_not {
                self.rewind(2);
                self.op_ifeq(offset, pos);
                return;
            }

            if self.opat(1) == OP_not {
                self.rewind(1);
                self.op_iftrue(offset, pos);
                return;
            }

            if self.opat(1) == OP_pushfalse {
                self.rewind(1);
                self.op_jump(offset, pos);
                return;
            }

            self.beginop(OP_iffalse);
            self.opcodes.write_s24(offset);
            self.opcodes.map_offsets(pos as i64);
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset(self.pool_index, pos as i64 + offset as i64);
        }
    }

    fn op_ifeq(&mut self, offset: i32, pos: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_ifeq);
            self.opcodes.write_s24(offset);
            self.opcodes.map_offsets(pos as i64);
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset(self.pool_index, pos as i64 + offset as i64);
        }
    }

    fn op_ifne(&mut self, offset: i32, pos: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_ifne);
            self.opcodes.write_s24(offset);
            self.opcodes.map_offsets(pos as i64);
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset(self.pool_index, pos as i64 + offset as i64);
        }
    }

    fn op_ifstricteq(&mut self, offset: i32, pos: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_ifstricteq);
            self.opcodes.write_s24(offset);
            self.opcodes.map_offsets(pos as i64);
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset(self.pool_index, pos as i64 + offset as i64);
        }
    }

    fn op_ifstrictne(&mut self, offset: i32, pos: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_ifstrictne);
            self.opcodes.write_s24(offset);
            self.opcodes.map_offsets(pos as i64);
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset(self.pool_index, pos as i64 + offset as i64);
        }
    }

    fn op_iflt(&mut self, offset: i32, pos: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_iflt);
            self.opcodes.write_s24(offset);
            self.opcodes.map_offsets(pos as i64);
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset(self.pool_index, pos as i64 + offset as i64);
        }
    }

    fn op_ifle(&mut self, offset: i32, pos: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_ifle);
            self.opcodes.write_s24(offset);
            self.opcodes.map_offsets(pos as i64);
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset(self.pool_index, pos as i64 + offset as i64);
        }
    }

    fn op_ifgt(&mut self, offset: i32, pos: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_ifgt);
            self.opcodes.write_s24(offset);
            self.opcodes.map_offsets(pos as i64);
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset(self.pool_index, pos as i64 + offset as i64);
        }
    }

    fn op_ifge(&mut self, offset: i32, pos: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_ifge);
            self.opcodes.write_s24(offset);
            self.opcodes.map_offsets(pos as i64);
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset(self.pool_index, pos as i64 + offset as i64);
        }
    }

    fn op_lessthan(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_lessthan);
        }
    }

    fn op_lessequals(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_lessequals);
        }
    }

    fn op_greaterthan(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_greaterthan);
        }
    }

    fn op_greaterequals(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_greaterequals);
        }
    }

    fn op_newobject(&mut self, size: i32) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_coerce_a && size >= 1 {
                self.rewind(1);
            }

            self.beginop(OP_newobject);
            self.opcodes.write_u32(size);
        }
    }

    fn op_newarray(&mut self, size: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_newarray);
            self.opcodes.write_u32(size);
        }
    }

    fn op_getproperty(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_findpropstrict && self.read_int_at(1) == self.cp(CP_MN, index) {
                self.rewind(1);
                self.op_getlex(index);
                return;
            }

            self.beginop(OP_getproperty);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_setproperty(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_coerce_a {
                self.rewind(1);
            }

            self.beginop(OP_setproperty);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_initproperty(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_coerce_a {
                self.rewind(1);
            }

            self.beginop(OP_initproperty);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_getdescendants(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_getdescendants);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_findpropstrict(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_findpropstrict);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_findproperty(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_findproperty);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_finddef(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_finddef);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_getlex(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_getlex);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_nextname(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_nextname);
        }
    }

    fn op_nextvalue(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_nextvalue);
        }
    }

    fn op_hasnext(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_hasnext);
        }
    }

    fn op_hasnext2(&mut self, object_register: i32, index_register: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_hasnext2);
            self.opcodes.write_u32(object_register);
            self.opcodes.write_u32(index_register);
        }
    }

    fn op_deleteproperty(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_deleteproperty);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_setslot(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_setslot);
            self.opcodes.write_u32(index);
        }
    }

    fn op_getslot(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_getslot);
            self.opcodes.write_u32(index);
        }
    }

    fn op_setglobalslot(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_coerce_a {
                self.rewind(1);
            }

            self.beginop(OP_setglobalslot);
            self.opcodes.write_u32(index);
        }
    }

    fn op_getglobalslot(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_getglobalslot);
            self.opcodes.write_u32(index);
        }
    }

    fn op_call(&mut self, size: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_call);
            self.opcodes.write_u32(size);
        }
    }

    fn op_construct(&mut self, size: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_construct);
            self.opcodes.write_u32(size);
        }
    }

    fn op_applytype(&mut self, size: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_applytype);
            self.opcodes.write_u32(size);
        }
    }

    fn op_newfunction(&mut self, id: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_newfunction);
            self.opcodes.write_u32(self.method_info.get_index(self.pool_index, id));
        }
    }

    fn op_newclass(&mut self, id: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_newclass);
            self.opcodes.write_u32(self.class_info.get_index(self.pool_index, id));
        }
    }

    fn op_callstatic(&mut self, id: i32, argc: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_callstatic);
            self.opcodes.write_u32(self.method_info.get_index(self.pool_index, id));
            self.opcodes.write_u32(argc);
        }
    }

    fn op_callmethod(&mut self, id: i32, argc: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_callmethod);
            self.opcodes.write_u32(self.method_info.get_index(self.pool_index, id));
            self.opcodes.write_u32(argc);
        }
    }

    fn op_callproperty(&mut self, index: i32, argc: i32) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_coerce_a {
                self.rewind(1);
            }

            self.beginop(OP_callproperty);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
            self.opcodes.write_u32(argc);
        }
    }

    fn op_callproplex(&mut self, index: i32, argc: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_callproplex);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
            self.opcodes.write_u32(argc);
        }
    }

    fn op_constructprop(&mut self, index: i32, argc: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_constructprop);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
            self.opcodes.write_u32(argc);
        }
    }

    fn op_callsuper(&mut self, index: i32, argc: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_callsuper);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
            self.opcodes.write_u32(argc);
        }
    }

    fn op_getsuper(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_getsuper);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_setsuper(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_setsuper);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_constructsuper(&mut self, argc: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_constructsuper);
            self.opcodes.write_u32(argc);
        }
    }

    fn op_pushshort(&mut self, n: i32) {
        if self.opcode_pass == 1 {
            if self.peep_hole && n >= -128 && n <= 127 {
                self.op_pushbyte(n);
                return;
            }
            self.beginop(OP_pushshort);
            self.opcodes.write_u32(n);
        }
    }

    fn op_astype(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_astype);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_astypelate(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_astypelate);
        }
    }

    fn op_coerce(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_coerce && self.read_int_at(1) == self.cp(CP_MN, index) {
                return;
            }

            self.beginop(OP_coerce);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_coerce_b(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_coerce_b);
        }
    }

    fn op_coerce_o(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_coerce_o);
        }
    }

    fn op_coerce_a(&mut self) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_coerce_a {
                return;
            }
            self.beginop(OP_coerce_a);
        }
    }

    fn op_coerce_i(&mut self) {
        if self.opcode_pass == 1 {
            if matches!(
                self.opat(1),
                OP_coerce_i
                    | OP_convert_i
                    | OP_increment_i
                    | OP_decrement_i
                    | OP_pushbyte
                    | OP_pushshort
                    | OP_pushint
                    | OP_bitand
                    | OP_bitor
                    | OP_bitxor
                    | OP_lshift
                    | OP_rshift
                    | OP_add_i
                    | OP_subtract_i
                    | OP_multiply_i
                    | OP_bitnot
            ) {
                return;
            }
            self.beginop(OP_coerce_i);
        }
    }

    fn op_coerce_u(&mut self) {
        if self.opcode_pass == 1 {
            if matches!(self.opat(1), OP_coerce_u | OP_convert_u | OP_urshift) {
                return;
            }
            self.beginop(OP_coerce_u);
        }
    }

    fn op_coerce_d(&mut self) {
        if self.opcode_pass == 1 {
            if matches!(
                self.opat(1),
                OP_subtract
                    | OP_multiply
                    | OP_divide
                    | OP_modulo
                    | OP_increment
                    | OP_decrement
                    | OP_inclocal
                    | OP_declocal
                    | OP_coerce_d
                    | OP_convert_d
            ) {
                return;
            }
            self.beginop(OP_coerce_d);
        }
    }

    fn op_coerce_s(&mut self) {
        if self.opcode_pass == 1 {
            if matches!(self.opat(1), OP_coerce_s | OP_convert_s | OP_pushstring | OP_typeof) {
                return;
            }

            if self.opat(2) == OP_pushstring && self.opat(1) == OP_add {
                return;
            }

            self.beginop(OP_coerce_s);
        }
    }

    fn op_istype(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_istype);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_istypelate(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_istypelate);
        }
    }

    fn op_pushbyte(&mut self, n: i32) {
        if self.opcode_pass == 1 {
            if self.opat(1) == OP_pushbyte && self.read_byte_at(1) == n
                || self.opat(1) == OP_dup && self.opat(2) == OP_pushbyte && self.read_byte_at(2) == n
            {
                self.op_dup();
                return;
            }
            self.beginop(OP_pushbyte);
            self.opcodes.write_u8(n);
        }
    }

    fn op_getscopeobject(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_getscopeobject);
            self.opcodes.write_u8(index);
        }
    }

    fn op_pushscope(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_pushscope);
        }
    }

    fn op_popscope(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_popscope);
        }
    }

    fn op_convert_i(&mut self) {
        if self.opcode_pass == 1 {
            if matches!(
                self.opat(1),
                OP_convert_i
                    | OP_coerce_i
                    | OP_bitand
                    | OP_bitor
                    | OP_bitxor
                    | OP_lshift
                    | OP_rshift
                    | OP_add_i
                    | OP_subtract_i
                    | OP_increment_i
                    | OP_decrement_i
                    | OP_multiply_i
                    | OP_pushbyte
                    | OP_pushshort
                    | OP_pushint
            ) {
                return;
            }

            self.beginop(OP_convert_i);
        }
    }

    fn op_convert_u(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_convert_u);
        }
    }

    fn op_throw(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_throw);
        }
    }

    fn op_instanceof(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_instanceof);
        }
    }

    fn op_in(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_in);
        }
    }

    fn op_dxns(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_dxns);
            let v = self.cp(CP_STRING, index);
            self.opcodes.write_u32(v);
        }
    }

    fn op_dxnslate(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_dxnslate);
        }
    }

    fn op_ifnlt(&mut self, offset: i32, pos: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_ifnlt);
            self.opcodes.write_s24(offset);
            self.opcodes.map_offsets(pos as i64);
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset(self.pool_index, pos as i64 + offset as i64);
        }
    }

    fn op_ifnle(&mut self, offset: i32, pos: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_ifnle);
            self.opcodes.write_s24(offset);
            self.opcodes.map_offsets(pos as i64);
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset(self.pool_index, pos as i64 + offset as i64);
        }
    }

    fn op_ifngt(&mut self, offset: i32, pos: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_ifngt);
            self.opcodes.write_s24(offset);
            self.opcodes.map_offsets(pos as i64);
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset(self.pool_index, pos as i64 + offset as i64);
        }
    }

    fn op_ifnge(&mut self, offset: i32, pos: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_ifnge);
            self.opcodes.write_s24(offset);
            self.opcodes.map_offsets(pos as i64);
        } else if self.opcode_pass == 2 {
            self.opcodes.update_offset(self.pool_index, pos as i64 + offset as i64);
        }
    }

    fn op_pushwith(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_pushwith);
        }
    }

    fn op_newactivation(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_newactivation);
        }
    }

    fn op_newcatch(&mut self, index: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_newcatch);
            self.opcodes.write_u32(index);
        }
    }

    fn op_deldescendants(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_deldescendants);
        }
    }

    fn op_getglobalscope(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_getglobalscope);
        }
    }

    fn op_getlocal0(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_getlocal0);
        }
    }

    fn op_getlocal1(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_getlocal1);
        }
    }

    fn op_getlocal2(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_getlocal2);
        }
    }

    fn op_getlocal3(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_getlocal3);
        }
    }

    fn op_setlocal0(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_setlocal0);
        }
    }

    fn op_setlocal1(&mut self) {
        if self.opcode_pass == 1 {
            if self.opat(2) == OP_getlocal1 && self.opat(1) == OP_increment_i {
                self.rewind(2);
                self.op_inclocal_i(1);
                return;
            }
            if self.opat(2) == OP_getlocal1 && self.opat(1) == OP_increment {
                self.rewind(2);
                self.op_inclocal(1);
                return;
            }
            self.beginop(OP_setlocal1);
        }
    }

    fn op_setlocal2(&mut self) {
        if self.opcode_pass == 1 {
            if self.opat(2) == OP_getlocal2 && self.opat(1) == OP_increment_i {
                self.rewind(2);
                self.op_inclocal_i(2);
                return;
            }
            if self.opat(2) == OP_getlocal2 && self.opat(1) == OP_increment {
                self.rewind(2);
                self.op_inclocal(2);
                return;
            }
            self.beginop(OP_setlocal2);
        }
    }

    fn op_setlocal3(&mut self) {
        if self.opcode_pass == 1 {
            if self.opat(2) == OP_getlocal3 && self.opat(1) == OP_increment_i {
                self.rewind(2);
                self.op_inclocal_i(3);
                return;
            }
            if self.opat(2) == OP_getlocal3 && self.opat(1) == OP_increment {
                self.rewind(2);
                self.op_inclocal(3);
                return;
            }
            self.beginop(OP_setlocal3);
        }
    }

    fn op_label(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_label);
        }
    }

    fn op_pushconstant(&mut self, id: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_pushuninitialized);
            self.opcodes.write_u32(id);
        }
    }

    fn op_callsupervoid(&mut self, index: i32, argc: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_callsupervoid);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
            self.opcodes.write_u32(argc);
        }
    }

    fn op_callpropvoid(&mut self, index: i32, argc: i32) {
        if self.opcode_pass == 1 {
            self.beginop(OP_callpropvoid);
            let v = self.cp(CP_MN, index);
            self.opcodes.write_u32(v);
            self.opcodes.write_u32(argc);
        }
    }

    fn op_li8(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_li8);
        }
    }

    fn op_li16(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_li16);
        }
    }

    fn op_li32(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_li32);
        }
    }

    fn op_lf32(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_lf32);
        }
    }

    fn op_lf64(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_lf64);
        }
    }

    fn op_si8(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_si8);
        }
    }

    fn op_si16(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_si16);
        }
    }

    fn op_si32(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_si32);
        }
    }

    fn op_sf32(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_sf32);
        }
    }

    fn op_sf64(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_sf64);
        }
    }

    fn op_sxi1(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_sxi1);
        }
    }

    fn op_sxi8(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_sxi8);
        }
    }

    fn op_sxi16(&mut self) {
        if self.opcode_pass == 1 {
            self.beginop(OP_sxi16);
        }
    }
}

struct ByteArrayPool2 {
    pool: ByteArrayPool,
    size: i32,
    sizes: Vec<i32>,
    indexes: HashMap<i32, i32>,
}

impl ByteArrayPool2 {
    fn new(sizes: Vec<i32>) -> ByteArrayPool2 {
        ByteArrayPool2 { pool: ByteArrayPool::new(ArrayKind::Bytes), size: 0, sizes, indexes: HashMap::new() }
    }

    fn add_byte_array(&mut self, pool_index: i32, old_index: i32, ba: &BytecodeBuffer) -> i32 {
        let mut index = self.pool.contains(ba, 0, ba.size());
        if index == -1 {
            index = self.pool.store(ba, 0, ba.size());
            self.size += ba.size();
        }
        // ByteArrayPool is 1-based, metadata infos are 0-based
        let key = self.calc_index(pool_index, old_index);
        self.indexes.insert(key, index - 1);
        index
    }

    fn calc_index(&self, pool_index: i32, old_index: i32) -> i32 {
        let mut new_index = 0;
        for i in 0..pool_index {
            new_index += self.sizes[i as usize];
        }
        new_index + old_index
    }

    fn get_index(&self, pool_index: i32, old_index: i32) -> i32 {
        let new_index = self.calc_index(pool_index, old_index);
        self.indexes.get(&new_index).copied().unwrap_or(-1)
    }

    fn size(&self) -> i32 {
        self.size
    }

    fn write_to(&self, b: &mut BytecodeBuffer) {
        b.write_u32(self.pool.len() as i32);
        self.pool.write_entries(b);
    }

    fn write_to_os(&self, os: &mut Vec<u8>) {
        let mut b = BytecodeBuffer::with_size(self.size());
        self.write_to(&mut b);
        let _ = b.write_to(os);
    }
}

struct BytecodeBuffer2 {
    buf: BytecodeBuffer,
    sizes: Vec<i32>,
    #[allow(dead_code)]
    estimated_size: i32,
}

impl BytecodeBuffer2 {
    fn new(estimated_size: i32, sizes: Vec<i32>) -> BytecodeBuffer2 {
        BytecodeBuffer2 { buf: BytecodeBuffer::with_size(estimated_size), sizes, estimated_size }
    }

    fn get_index(&self, pool_index: i32, old_index: i32) -> i32 {
        let mut new_index = 0;
        for i in 0..pool_index {
            new_index += self.sizes[i as usize];
        }
        new_index + old_index
    }
}

impl Deref for BytecodeBuffer2 {
    type Target = BytecodeBuffer;
    fn deref(&self) -> &BytecodeBuffer {
        &self.buf
    }
}

impl DerefMut for BytecodeBuffer2 {
    fn deref_mut(&mut self) -> &mut BytecodeBuffer {
        &mut self.buf
    }
}

struct BytecodeBuffer3 {
    buf: BytecodeBuffer,
    offsets: HashMap<i32, i32>,
    decoders: Vec<Rc<BytecodeBuffer>>,
}

/// `HashMap.toString()`, with entries in key order rather than Java's bucket order.
fn offsets_string(offsets: &HashMap<i32, i32>) -> String {
    let mut keys: Vec<&i32> = offsets.keys().collect();
    keys.sort();
    let body: Vec<String> = keys.iter().map(|k| format!("{}={}", k, offsets[k])).collect();
    format!("{{{}}}", body.join(", "))
}

impl BytecodeBuffer3 {
    fn new(decoders: Vec<Rc<BytecodeBuffer>>, estimated_size: i32) -> BytecodeBuffer3 {
        BytecodeBuffer3 { buf: BytecodeBuffer::with_size(estimated_size), offsets: HashMap::new(), decoders }
    }

    fn map_offsets(&mut self, offset: i64) {
        let new_pos = self.buf.size();
        self.offsets.insert(offset as i32, new_pos);
    }

    fn get_offset(&self, offset: i64) -> i64 {
        match self.offsets.get(&(offset as i32)) {
            Some(&i) => i as i64,
            None => {
                println!("getOffset: can't match {} with a new offset ", offset);
                println!("{}", offsets_string(&self.offsets));
                0
            }
        }
    }

    /// The branch's own position comes from the decoder that is driving this pass.
    fn update_offset(&mut self, pool_index: i32, offset: i64) {
        let i = self.offsets.get(&(offset as i32)).copied();
        let p = self.offsets.get(&self.decoders[pool_index as usize].pos()).copied();
        if let (Some(i), Some(p)) = (i, p) {
            self.buf.write_s24_at(p - 3, i - p);
        }
    }

    fn update_offset3(&mut self, old_offset_pos: i64, old_pos: i64, old_target: i64) {
        let i = self.offsets.get(&(old_target as i32)).copied();
        let p = self.offsets.get(&(old_pos as i32)).copied();
        let s = self.offsets.get(&(old_offset_pos as i32)).copied();
        if let (Some(i), Some(p), Some(s)) = (i, p, s) {
            self.buf.write_s24_at(s, i - p);
        } else {
            if i.is_none() {
                println!("updateOffset2: can't match i {} with a new offset", old_target);
            }
            if p.is_none() {
                println!("updateOffset2: can't match p {} with a new offset", old_pos);
            }
            if s.is_none() {
                println!("updateOffset2: can't match s {} with a new offset", old_offset_pos);
            }
            println!("{}", offsets_string(&self.offsets));
        }
    }

    fn clear(&mut self) {
        self.buf.clear();
        self.offsets.clear();
    }
}

impl Deref for BytecodeBuffer3 {
    type Target = BytecodeBuffer;
    fn deref(&self) -> &BytecodeBuffer {
        &self.buf
    }
}

impl DerefMut for BytecodeBuffer3 {
    fn deref_mut(&mut self) -> &mut BytecodeBuffer {
        &mut self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn run(input: Vec<u8>, mode: &str) -> Result<Vec<u8>, DecoderException> {
        let d = Decoder::new(BytecodeBuffer::from_bytes(input))?;
        let mut e = Encoder::new(d.major_version, d.minor_version);
        e.enable_peep_hole();
        e.add_constant_pools(vec![d.constant_pool.clone()]);
        if mode == "B" {
            e.disable_debugging();
            e.remove_metadata();
            e.add_metadata_to_keep("Embed");
            e.add_metadata_to_keep("Event");
            e.add_metadata_to_keep("SWF");
        }
        e.configure(std::slice::from_ref(&d));
        e.use_constant_pool(0);
        for k in 0..d.method_info.size() {
            d.method_info.decode(k, &mut e)?;
        }
        for k in 0..d.metadata_info.size() {
            d.metadata_info.decode(k, &mut e)?;
        }
        for k in 0..d.class_info.size() {
            d.class_info.decode_instance(k, &mut e)?;
        }
        for k in 0..d.class_info.size() {
            d.class_info.decode_class(k, 0, &mut e)?;
        }
        for k in 0..d.script_info.size() {
            d.script_info.decode(k, &mut e)?;
        }
        for k in 0..d.method_bodies.size() {
            d.method_bodies.decode_passes(k, 2, &mut e)?;
        }
        Ok(e.to_abc())
    }

    /// Re-encodes every `N.abc` in `ASC_DUMP` keeping debug info and metadata (mode A) or keeping only some
    /// metadata (mode B), writing `N.rustA.abc`/`N.rustB.abc` to compare with the Java `EncDrv`.
    #[test]
    #[ignore]
    fn encode_modes_for_java_comparison() {
        let Ok(dst) = std::env::var("ASC_DUMP") else {
            eprintln!("skipped: set ASC_DUMP");
            return;
        };
        let mut names: Vec<PathBuf> = std::fs::read_dir(&dst)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.file_name().unwrap().to_str().unwrap().len() == 9 && p.extension().is_some_and(|x| x == "abc"))
            .collect();
        names.sort();
        for p in names {
            for mode in ["A", "B"] {
                let input = std::fs::read(&p).unwrap();
                if let Ok(Ok(out)) = std::panic::catch_unwind(|| run(input, mode)) {
                    let stem = p.file_stem().unwrap().to_str().unwrap();
                    std::fs::write(Path::new(&dst).join(format!("{stem}.rust{mode}.abc")), out).unwrap();
                }
            }
        }
    }
}
