//! Port of `macromedia.abc.Decoder`.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use super::bytecode_buffer::BytecodeBuffer;
use super::constant_pool::ConstantPool;
use super::decoder_exception::DecoderException;
use super::scanner;
use super::visitor::Visitor;
use crate::asc::emit::constants::*;

pub struct Decoder {
    pub minor_version: i32,
    pub major_version: i32,
    pub constant_pool: Rc<ConstantPool>,
    pub method_info: MethodInfo,
    pub metadata_info: MetaDataInfo,
    pub class_info: ClassInfo,
    pub script_info: ScriptInfo,
    pub method_bodies: MethodBodies,
    pub opcodes: Rc<Opcodes>,

    in_: Rc<BytecodeBuffer>,
}

impl Decoder {
    pub fn new(in_: BytecodeBuffer) -> Result<Decoder, DecoderException> {
        let in_ = Rc::new(in_);
        let minor_version = in_.read_u16();
        let major_version = in_.read_u16();
        let constant_pool = Rc::new(ConstantPool::new(in_.clone(), minor_version >= MINORwithDECIMAL)?);

        let pos = in_.pos();
        let mut method_info = MethodInfo::new(in_.clone());
        method_info.estimated_size = in_.pos() - pos;

        let pos = in_.pos();
        let mut metadata_info = MetaDataInfo::new(in_.clone());
        metadata_info.estimated_size = in_.pos() - pos;

        let pos = in_.pos();
        let mut class_info = ClassInfo::new(in_.clone());
        class_info.estimated_size = in_.pos() - pos;

        let pos = in_.pos();
        let mut script_info = ScriptInfo::new(in_.clone());
        script_info.estimated_size = in_.pos() - pos;

        let opcodes = Rc::new(Opcodes::new(in_.clone()));

        let pos = in_.pos();
        let mut method_bodies = MethodBodies::new(in_.clone(), opcodes.clone());
        method_bodies.estimated_size = in_.pos() - pos;

        Ok(Decoder {
            minor_version,
            major_version,
            constant_pool,
            method_info,
            metadata_info,
            class_info,
            script_info,
            method_bodies,
            opcodes,
            in_,
        })
    }

    pub fn pos(&self) -> i32 {
        self.in_.pos()
    }

    pub(crate) fn input(&self) -> Rc<BytecodeBuffer> {
        self.in_.clone()
    }
}

pub struct MethodInfo {
    in_: Rc<BytecodeBuffer>,
    pub(crate) estimated_size: i32,
    positions: Vec<i32>,
}

impl MethodInfo {
    fn new(in_: Rc<BytecodeBuffer>) -> MethodInfo {
        let positions = scanner::scan_methods(&in_);
        MethodInfo { in_, estimated_size: 0, positions }
    }

    pub fn size(&self) -> i32 {
        self.positions.len() as i32
    }

    pub fn decode<V: Visitor + ?Sized>(&self, index: i32, visitor: &mut V) -> Result<(), DecoderException> {
        let in_ = &*self.in_;
        let pos = self.positions[index as usize];
        let original_pos = in_.pos();
        in_.seek(pos);

        let param_count = in_.read_u32();
        let return_type = in_.read_u32();

        let mut param_types = None;
        if param_count > 0 {
            param_types = Some((0..param_count).map(|_| in_.read_u32()).collect::<Vec<i32>>());
        }

        let native_name = in_.read_u32();
        let flags = in_.read_u8();

        let optional_count = if (flags & METHOD_HasOptional) != 0 { in_.read_u32() } else { 0 };

        let mut values = None;
        let mut value_kinds = None;
        if optional_count > 0 {
            let mut v = Vec::with_capacity(optional_count as usize);
            let mut k = Vec::with_capacity(optional_count as usize);
            for _ in 0..optional_count {
                v.push(in_.read_u32());
                k.push(in_.read_u8());
            }
            values = Some(v);
            value_kinds = Some(k);
        }

        let mut param_names = None;
        let param_name_count = if (flags & METHOD_HasParamNames) != 0 { param_count } else { 0 };
        if param_name_count > 0 {
            param_names = Some((0..param_name_count).map(|_| in_.read_u32()).collect::<Vec<i32>>());
        }
        in_.seek(original_pos);

        visitor.method_info(
            return_type,
            param_types.as_deref(),
            native_name,
            flags,
            values.as_deref(),
            value_kinds.as_deref(),
            param_names.as_deref(),
        )
    }
}

pub struct MetaDataInfo {
    in_: Rc<BytecodeBuffer>,
    pub(crate) estimated_size: i32,
    positions: Vec<i32>,
}

impl MetaDataInfo {
    fn new(in_: Rc<BytecodeBuffer>) -> MetaDataInfo {
        let positions = scanner::scan_metadata(&in_);
        MetaDataInfo { in_, estimated_size: 0, positions }
    }

    pub fn size(&self) -> i32 {
        self.positions.len() as i32
    }

    pub fn decode<V: Visitor + ?Sized>(&self, index: i32, visitor: &mut V) -> Result<(), DecoderException> {
        let in_ = &*self.in_;
        let pos = self.positions[index as usize];
        let original_pos = in_.pos();
        in_.seek(pos);

        let name_index = in_.read_u32();
        let value_count = in_.read_u32();

        let mut keys = None;
        let mut values = None;
        if value_count > 0 {
            keys = Some((0..value_count).map(|_| in_.read_u32()).collect::<Vec<i32>>());
            values = Some((0..value_count).map(|_| in_.read_u32()).collect::<Vec<i32>>());
        }

        in_.seek(original_pos);

        visitor.metadata_info(index, name_index, keys.as_deref(), values.as_deref())
    }
}

pub struct ClassInfo {
    in_: Rc<BytecodeBuffer>,
    pub(crate) estimated_size: i32,
    c_positions: Vec<i32>,
    i_positions: Vec<i32>,
}

impl ClassInfo {
    fn new(in_: Rc<BytecodeBuffer>) -> ClassInfo {
        let size = in_.read_u32();
        let i_positions = scanner::scan_instances(&in_, size);
        let c_positions = scanner::scan_classes(&in_, size);
        ClassInfo { in_, estimated_size: 0, c_positions, i_positions }
    }

    pub fn size(&self) -> i32 {
        self.c_positions.len() as i32
    }

    pub fn decode<V: Visitor + ?Sized>(&self, index: i32, visitor: &mut V) -> Result<(), DecoderException> {
        let name = self.decode_instance(index, visitor)?;
        self.decode_class(index, name, visitor)
    }

    pub fn decode_instance<V: Visitor + ?Sized>(&self, index: i32, visitor: &mut V) -> Result<i32, DecoderException> {
        let in_ = &*self.in_;
        let pos = self.i_positions[index as usize];
        let original_pos = in_.pos();
        in_.seek(pos);

        let name = in_.read_u32();
        let super_name = in_.read_u32();

        let flags = in_.read_u8();
        let is_final = (flags & CLASS_FLAG_final) != 0;
        let is_dynamic = (flags & CLASS_FLAG_sealed) == 0;
        let is_interface = (flags & CLASS_FLAG_interface) != 0;
        let has_protected = (flags & CLASS_FLAG_protected) != 0;

        let protected_namespace = if has_protected { in_.read_u32() } else { 0 };

        let interface_count = in_.read_u32();
        let interfaces: Vec<i32> = (0..interface_count).map(|_| in_.read_u32()).collect();

        let iinit = in_.read_u32();
        visitor.start_instance(name, super_name, is_dynamic, is_final, is_interface, Some(&interfaces), iinit, protected_namespace)?;
        decode_traits(in_, visitor)?;
        visitor.end_instance();

        in_.seek(original_pos);

        Ok(name)
    }

    pub fn decode_class<V: Visitor + ?Sized>(&self, index: i32, name: i32, visitor: &mut V) -> Result<(), DecoderException> {
        let in_ = &*self.in_;
        let pos = self.c_positions[index as usize];
        let original_pos = in_.pos();
        in_.seek(pos);

        let cinit = in_.read_u32();
        visitor.start_class(name, cinit)?;
        decode_traits(in_, visitor)?;
        visitor.end_class();

        in_.seek(original_pos);
        Ok(())
    }
}

pub struct ScriptInfo {
    in_: Rc<BytecodeBuffer>,
    pub(crate) estimated_size: i32,
    positions: Vec<i32>,
}

impl ScriptInfo {
    fn new(in_: Rc<BytecodeBuffer>) -> ScriptInfo {
        let positions = scanner::scan_scripts(&in_);
        ScriptInfo { in_, estimated_size: 0, positions }
    }

    pub fn size(&self) -> i32 {
        self.positions.len() as i32
    }

    pub fn decode<V: Visitor + ?Sized>(&self, index: i32, visitor: &mut V) -> Result<(), DecoderException> {
        let in_ = &*self.in_;
        let pos = self.positions[index as usize];
        let original_pos = in_.pos();
        in_.seek(pos);

        let init_id = in_.read_u32();
        visitor.start_script(init_id);
        decode_traits(in_, visitor)?;
        visitor.end_script();

        in_.seek(original_pos);
        Ok(())
    }
}

pub struct MethodBodies {
    in_: Rc<BytecodeBuffer>,
    pub(crate) estimated_size: i32,
    positions: Vec<i32>,
    opcodes: Rc<Opcodes>,
}

impl MethodBodies {
    fn new(in_: Rc<BytecodeBuffer>, opcodes: Rc<Opcodes>) -> MethodBodies {
        let positions = scanner::scan_method_bodies(&in_);
        MethodBodies { in_, estimated_size: 0, positions, opcodes }
    }

    pub fn size(&self) -> i32 {
        self.positions.len() as i32
    }

    pub fn decode<V: Visitor + ?Sized>(&self, index: i32, visitor: &mut V) -> Result<(), DecoderException> {
        self.decode_passes(index, 1, visitor)
    }

    pub fn decode_passes<V: Visitor + ?Sized>(&self, index: i32, opcode_pass: i32, visitor: &mut V) -> Result<(), DecoderException> {
        let in_ = &*self.in_;
        let pos = self.positions[index as usize];
        let original_pos = in_.pos();
        in_.seek(pos);

        let method_info = in_.read_u32();
        let max_stack = in_.read_u32();
        let max_regs = in_.read_u32();
        let scope_depth = in_.read_u32();
        let max_scope = in_.read_u32();

        let code_length = in_.read_u32() as i64;
        let code_start = in_.pos();
        in_.skip(code_length as i32 as i64);

        visitor.start_method_body(method_info, max_stack, max_regs, scope_depth, max_scope, code_start, code_length)?;

        let ex_pos = in_.pos();
        for _ in 0..opcode_pass {
            self.opcodes.reset();
            in_.seek(ex_pos);
            let ex_count = in_.read_u32();

            visitor.start_opcodes(method_info);
            visitor.start_exceptions(ex_count);

            self.decode_exceptions(in_, code_start, visitor, ex_count);
            self.opcodes.decode(code_start, code_length, visitor)?;

            visitor.end_opcodes();
            visitor.end_exceptions();
        }
        decode_traits(in_, visitor)?;
        visitor.end_method_body();

        in_.seek(original_pos);
        Ok(())
    }

    fn decode_exceptions<V: Visitor + ?Sized>(&self, in_: &BytecodeBuffer, code_start: i32, visitor: &mut V, ex_count: i32) {
        let has_names = in_.minor_version() != 15;

        for _ in 0..ex_count {
            let start = code_start as i64 + in_.read_u32() as i64;
            let end = code_start as i64 + in_.read_u32() as i64;
            let target = code_start as i64 + in_.read_u32() as i64;

            let type_ = in_.read_u32();

            let name_index = if has_names { in_.read_u32() } else { 0 };

            self.opcodes.add_target(start as i32);
            self.opcodes.add_target(end as i32);
            self.opcodes.add_target(target as i32);

            visitor.exception(start, end, target, type_, name_index);
        }
    }
}

fn decode_traits<V: Visitor + ?Sized>(in_: &BytecodeBuffer, visitor: &mut V) -> Result<(), DecoderException> {
    let count = in_.read_u32();
    visitor.trait_count(count);

    for _ in 0..count {
        let name = in_.read_u32();
        let kind = in_.read_u8();
        let mut value_kind = 0;

        match kind & 0x0f {
            TRAIT_Var | TRAIT_Const => {
                let slot_id = in_.read_u32();
                let type_id = in_.read_u32();
                let value_id = in_.read_u32();
                if value_id != 0 {
                    value_kind = in_.read_u8();
                }
                let md = decode_meta_data(in_, kind);
                visitor.slot_trait(kind, name, slot_id, type_id, value_id, value_kind, md.as_deref())?;
            }
            TRAIT_Method | TRAIT_Getter | TRAIT_Setter => {
                let disp_id = in_.read_u32();
                let meth_info = in_.read_u32();
                let md = decode_meta_data(in_, kind);
                visitor.method_trait(kind, name, disp_id, meth_info, md.as_deref())?;
            }
            TRAIT_Class => {
                let slot_id = in_.read_u32();
                let class_id = in_.read_u32();
                let md = decode_meta_data(in_, kind);
                visitor.class_trait(kind, name, slot_id, class_id, md.as_deref())?;
            }
            TRAIT_Function => {
                let slot_id = in_.read_u32();
                let meth_info = in_.read_u32();
                let md = decode_meta_data(in_, kind);
                visitor.function_trait(kind, name, slot_id, meth_info, md.as_deref())?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn decode_meta_data(in_: &BytecodeBuffer, kind: i32) -> Option<Vec<i32>> {
    let mut md = None;
    if ((kind >> 4) & TRAIT_FLAG_metadata) != 0 {
        let length = in_.read_u32();
        if length > 0 {
            md = Some((0..length).map(|_| in_.read_u32()).collect());
        }
    }
    md
}

pub struct Opcodes {
    in_: Rc<BytecodeBuffer>,
    target_set: RefCell<Option<HashSet<i32>>>,
}

impl Opcodes {
    pub fn new(in_: Rc<BytecodeBuffer>) -> Opcodes {
        Opcodes { in_, target_set: RefCell::new(None) }
    }

    pub fn add_target(&self, pos: i32) {
        self.target_set.borrow_mut().get_or_insert_with(HashSet::new).insert(pos);
    }

    pub fn reset(&self) {
        *self.target_set.borrow_mut() = None;
    }

    fn is_target(&self, pos: i32) -> bool {
        self.target_set.borrow().as_ref().is_some_and(|s| s.contains(&pos))
    }

    pub fn decode_n<V: Visitor + ?Sized>(&self, start: i32, length: i64, passes: i32, v: &mut V) -> Result<(), DecoderException> {
        for _ in 0..passes {
            self.decode(start, length, v)?;
        }
        Ok(())
    }

    pub fn decode<V: Visitor + ?Sized>(&self, start: i32, length: i64, v: &mut V) -> Result<(), DecoderException> {
        let in_ = &*self.in_;
        let original_pos = in_.pos();
        in_.seek(start);

        let end = start as i64 + length;
        while (in_.pos() as i64) < end {
            let pos = in_.pos();
            let opcode = in_.read_u8();

            if opcode == OP_label {
                self.add_target(pos);
            }

            if self.is_target(pos) {
                v.target(pos);
            }

            match opcode {
                OP_ifnlt => {
                    let offset = in_.read_s24();
                    self.add_target(offset + in_.pos());
                    v.op_ifnlt(offset, in_.pos());
                }
                OP_ifnle => {
                    let offset = in_.read_s24();
                    self.add_target(offset + in_.pos());
                    v.op_ifnle(offset, in_.pos());
                }
                OP_ifngt => {
                    let offset = in_.read_s24();
                    self.add_target(offset + in_.pos());
                    v.op_ifngt(offset, in_.pos());
                }
                OP_ifnge => {
                    let offset = in_.read_s24();
                    self.add_target(offset + in_.pos());
                    v.op_ifnge(offset, in_.pos());
                }
                OP_pushscope => v.op_pushscope(),
                OP_newactivation => v.op_newactivation(),
                OP_newcatch => {
                    let index = in_.read_u32();
                    v.op_newcatch(index);
                }
                OP_deldescendants => v.op_deldescendants(),
                OP_getglobalscope => v.op_getglobalscope(),
                OP_getlocal0 => v.op_getlocal0(),
                OP_getlocal1 => v.op_getlocal1(),
                OP_getlocal2 => v.op_getlocal2(),
                OP_getlocal3 => v.op_getlocal3(),
                OP_setlocal0 => v.op_setlocal0(),
                OP_setlocal1 => v.op_setlocal1(),
                OP_setlocal2 => v.op_setlocal2(),
                OP_setlocal3 => v.op_setlocal3(),
                OP_returnvoid => v.op_returnvoid(),
                OP_returnvalue => v.op_returnvalue(),
                OP_nop => v.op_nop(),
                OP_bkpt => v.op_bkpt(),
                OP_timestamp => v.op_timestamp(),
                OP_debugline => {
                    let linenum = in_.read_u32();
                    v.op_debugline(linenum);
                }
                OP_bkptline => {
                    in_.read_u32();
                    v.op_bkptline();
                }
                OP_debug => {
                    let di_local = in_.read_u8();
                    let index = in_.read_u32();
                    let slot = in_.read_u8();
                    let linenum = in_.read_u32();
                    v.op_debug(di_local, index, slot, linenum);
                }
                OP_debugfile => {
                    let index = in_.read_u32();
                    v.op_debugfile(index);
                }
                OP_jump => {
                    let jump = in_.read_s24();
                    self.add_target(jump + in_.pos());
                    v.op_jump(jump, in_.pos());
                }
                OP_pushnull => v.op_pushnull(),
                OP_pushundefined => v.op_pushundefined(),
                OP_pushstring => {
                    let index = in_.read_u32();
                    v.op_pushstring(index);
                }
                OP_pushnamespace => {
                    let index = in_.read_u32();
                    v.op_pushnamespace(index);
                }
                OP_pushint => {
                    let index = in_.read_u32();
                    v.op_pushint(index);
                }
                OP_pushuint => {
                    let index = in_.read_u32();
                    v.op_pushuint(index);
                }
                OP_pushdouble => {
                    let index = in_.read_u32();
                    v.op_pushdouble(index);
                }
                OP_pushdecimal => {
                    let index = in_.read_u32();
                    v.op_pushdecimal(index);
                }
                OP_getlocal => {
                    let index = in_.read_u32();
                    v.op_getlocal(index);
                }
                OP_pushtrue => v.op_pushtrue(),
                OP_pushfalse => v.op_pushfalse(),
                OP_pushnan => v.op_pushnan(),
                OP_pushdnan => v.op_pushdnan(),
                OP_pop => v.op_pop(),
                OP_dup => v.op_dup(),
                OP_swap => v.op_swap(),
                OP_convert_s => v.op_convert_s(),
                OP_esc_xelem => v.op_esc_xelem(),
                OP_esc_xattr => v.op_esc_xattr(),
                OP_checkfilter => v.op_checkfilter(),
                OP_convert_d => v.op_convert_d(),
                OP_convert_b => v.op_convert_b(),
                OP_convert_o => v.op_convert_o(),
                OP_convert_m => v.op_convert_m(),
                OP_convert_m_p => {
                    let param = in_.read_u32();
                    v.op_convert_m_p(param);
                }
                OP_negate => v.op_negate(),
                OP_negate_p => {
                    let param = in_.read_u32();
                    v.op_negate_p(param);
                }
                OP_negate_i => v.op_negate_i(),
                OP_increment => v.op_increment(),
                OP_increment_p => {
                    let param = in_.read_u32();
                    v.op_increment_p(param);
                }
                OP_increment_i => v.op_increment_i(),
                OP_inclocal => {
                    let index = in_.read_u32();
                    v.op_inclocal(index);
                }
                OP_inclocal_p => {
                    let param = in_.read_u32();
                    let index = in_.read_u32();
                    v.op_inclocal_p(param, index);
                }
                OP_kill => {
                    let index = in_.read_u32();
                    v.op_kill(index);
                }
                OP_label => v.op_label(),
                OP_inclocal_i => {
                    let index = in_.read_u32();
                    v.op_inclocal_i(index);
                }
                OP_decrement => v.op_decrement(),
                OP_decrement_p => {
                    let param = in_.read_u32();
                    v.op_decrement_p(param);
                }
                OP_decrement_i => v.op_decrement_i(),
                OP_declocal => {
                    let index = in_.read_u32();
                    v.op_declocal(index);
                }
                OP_declocal_p => {
                    let param = in_.read_u32();
                    let index = in_.read_u32();
                    v.op_declocal_p(param, index);
                }
                OP_declocal_i => {
                    let index = in_.read_u32();
                    v.op_declocal_i(index);
                }
                OP_typeof => v.op_typeof(),
                OP_not => v.op_not(),
                OP_bitnot => v.op_bitnot(),
                OP_setlocal => {
                    let index = in_.read_u32();
                    v.op_setlocal(index);
                }
                OP_add => v.op_add(),
                OP_add_p => {
                    let param = in_.read_u32();
                    v.op_add_p(param);
                }
                OP_add_i => v.op_add_i(),
                OP_subtract => v.op_subtract(),
                OP_subtract_p => {
                    let param = in_.read_u32();
                    v.op_subtract_p(param);
                }
                OP_subtract_i => v.op_subtract_i(),
                OP_multiply => v.op_multiply(),
                OP_multiply_p => {
                    let param = in_.read_u32();
                    v.op_multiply_p(param);
                }
                OP_multiply_i => v.op_multiply_i(),
                OP_divide => v.op_divide(),
                OP_divide_p => {
                    let param = in_.read_u32();
                    v.op_divide_p(param);
                }
                OP_modulo => v.op_modulo(),
                OP_modulo_p => {
                    let param = in_.read_u32();
                    v.op_modulo_p(param);
                }
                OP_lshift => v.op_lshift(),
                OP_rshift => v.op_rshift(),
                OP_urshift => v.op_urshift(),
                OP_bitand => v.op_bitand(),
                OP_bitor => v.op_bitor(),
                OP_bitxor => v.op_bitxor(),
                OP_equals => v.op_equals(),
                OP_strictequals => v.op_strictequals(),
                OP_lookupswitch => {
                    let op_pos = in_.pos() - 1;
                    let default_pos = in_.read_s24();
                    self.add_target(default_pos + op_pos);
                    let size_1 = in_.read_u32();
                    let mut case_pos = vec![0; (size_1 + 1).max(0) as usize];
                    let case_table_pos = in_.pos();
                    for c in case_pos.iter_mut() {
                        *c = in_.read_s24();
                        self.add_target(*c + op_pos);
                    }
                    v.op_lookupswitch(default_pos, &case_pos, op_pos, case_table_pos);
                }
                OP_iftrue => {
                    let offset = in_.read_s24();
                    self.add_target(offset + in_.pos());
                    v.op_iftrue(offset, in_.pos());
                }
                OP_iffalse => {
                    let offset = in_.read_s24();
                    self.add_target(offset + in_.pos());
                    v.op_iffalse(offset, in_.pos());
                }
                OP_ifeq => {
                    let offset = in_.read_s24();
                    self.add_target(offset + in_.pos());
                    v.op_ifeq(offset, in_.pos());
                }
                OP_ifne => {
                    let offset = in_.read_s24();
                    self.add_target(offset + in_.pos());
                    v.op_ifne(offset, in_.pos());
                }
                OP_ifstricteq => {
                    let offset = in_.read_s24();
                    self.add_target(offset + in_.pos());
                    v.op_ifstricteq(offset, in_.pos());
                }
                OP_ifstrictne => {
                    let offset = in_.read_s24();
                    self.add_target(offset + in_.pos());
                    v.op_ifstrictne(offset, in_.pos());
                }
                OP_iflt => {
                    let offset = in_.read_s24();
                    self.add_target(offset + in_.pos());
                    v.op_iflt(offset, in_.pos());
                }
                OP_ifle => {
                    let offset = in_.read_s24();
                    self.add_target(offset + in_.pos());
                    v.op_ifle(offset, in_.pos());
                }
                OP_ifgt => {
                    let offset = in_.read_s24();
                    self.add_target(offset + in_.pos());
                    v.op_ifgt(offset, in_.pos());
                }
                OP_ifge => {
                    let offset = in_.read_s24();
                    self.add_target(offset + in_.pos());
                    v.op_ifge(offset, in_.pos());
                }
                OP_lessthan => v.op_lessthan(),
                OP_lessequals => v.op_lessequals(),
                OP_greaterthan => v.op_greaterthan(),
                OP_greaterequals => v.op_greaterequals(),
                OP_newobject => {
                    let size = in_.read_u32();
                    v.op_newobject(size);
                }
                OP_newarray => {
                    let size = in_.read_u32();
                    v.op_newarray(size);
                }
                OP_getproperty => {
                    let index = in_.read_u32();
                    v.op_getproperty(index);
                }
                OP_setproperty => {
                    let index = in_.read_u32();
                    v.op_setproperty(index);
                }
                OP_initproperty => {
                    let index = in_.read_u32();
                    v.op_initproperty(index);
                }
                OP_getdescendants => {
                    let index = in_.read_u32();
                    v.op_getdescendants(index);
                }
                OP_findpropstrict => {
                    let index = in_.read_u32();
                    v.op_findpropstrict(index);
                }
                OP_getlex => {
                    let index = in_.read_u32();
                    v.op_getlex(index);
                }
                OP_findproperty => {
                    let index = in_.read_u32();
                    v.op_findproperty(index);
                }
                OP_finddef => {
                    let index = in_.read_u32();
                    v.op_finddef(index);
                }
                OP_nextname => v.op_nextname(),
                OP_nextvalue => v.op_nextvalue(),
                OP_hasnext => v.op_hasnext(),
                OP_hasnext2 => {
                    let object_register = in_.read_u32();
                    let index_register = in_.read_u32();
                    v.op_hasnext2(object_register, index_register);
                }
                OP_deleteproperty => {
                    let index = in_.read_u32();
                    v.op_deleteproperty(index);
                }
                OP_setslot => {
                    let index = in_.read_u32();
                    v.op_setslot(index);
                }
                OP_getslot => {
                    let index = in_.read_u32();
                    v.op_getslot(index);
                }
                OP_setglobalslot => {
                    let index = in_.read_u32();
                    v.op_setglobalslot(index);
                }
                OP_getglobalslot => {
                    let index = in_.read_u32();
                    v.op_getglobalslot(index);
                }
                OP_call => {
                    let size = in_.read_u32();
                    v.op_call(size);
                }
                OP_construct => {
                    let size = in_.read_u32();
                    v.op_construct(size);
                }
                OP_applytype => {
                    let size = in_.read_u32();
                    v.op_applytype(size);
                }
                OP_newfunction => {
                    let id = in_.read_u32();
                    v.op_newfunction(id);
                }
                OP_newclass => {
                    let id = in_.read_u32();
                    v.op_newclass(id);
                }
                OP_callstatic => {
                    let id = in_.read_u32();
                    let argc = in_.read_u32();
                    v.op_callstatic(id, argc);
                }
                OP_callmethod => {
                    let id = in_.read_u32();
                    let argc = in_.read_u32();
                    v.op_callmethod(id, argc);
                }
                OP_callproperty => {
                    let index = in_.read_u32();
                    let argc = in_.read_u32();
                    v.op_callproperty(index, argc);
                }
                OP_callproplex => {
                    let index = in_.read_u32();
                    let argc = in_.read_u32();
                    v.op_callproplex(index, argc);
                }
                OP_constructprop => {
                    let index = in_.read_u32();
                    let argc = in_.read_u32();
                    v.op_constructprop(index, argc);
                }
                OP_callsuper => {
                    let index = in_.read_u32();
                    let argc = in_.read_u32();
                    v.op_callsuper(index, argc);
                }
                OP_getsuper => {
                    let index = in_.read_u32();
                    v.op_getsuper(index);
                }
                OP_setsuper => {
                    let index = in_.read_u32();
                    v.op_setsuper(index);
                }
                OP_constructsuper => {
                    let argc = in_.read_u32();
                    v.op_constructsuper(argc);
                }
                OP_pushshort => {
                    let n = in_.read_u32();
                    v.op_pushshort(n);
                }
                OP_astype => {
                    let index = in_.read_u32();
                    v.op_astype(index);
                }
                OP_astypelate => v.op_astypelate(),
                OP_coerce => {
                    let index = in_.read_u32();
                    v.op_coerce(index);
                }
                OP_coerce_b => v.op_coerce_b(),
                OP_coerce_o => v.op_coerce_o(),
                OP_coerce_a => v.op_coerce_a(),
                OP_coerce_i => v.op_coerce_i(),
                OP_coerce_u => v.op_coerce_u(),
                OP_coerce_d => v.op_coerce_d(),
                OP_coerce_s => v.op_coerce_s(),
                OP_istype => {
                    let index = in_.read_u32();
                    v.op_istype(index);
                }
                OP_istypelate => v.op_istypelate(),
                OP_pushbyte => {
                    let n = in_.read_u8();
                    v.op_pushbyte(n);
                }
                OP_getscopeobject => {
                    let index = in_.read_u8();
                    v.op_getscopeobject(index);
                }
                OP_pushwith => v.op_pushwith(),
                OP_popscope => v.op_popscope(),
                OP_convert_i => v.op_convert_i(),
                OP_convert_u => v.op_convert_u(),
                OP_throw => v.op_throw(),
                OP_instanceof => v.op_instanceof(),
                OP_in => v.op_in(),
                OP_dxns => {
                    let index = in_.read_u32();
                    v.op_dxns(index);
                }
                OP_dxnslate => v.op_dxnslate(),
                OP_pushuninitialized => {
                    let id = in_.read_u32();
                    v.op_pushconstant(id);
                }
                OP_callsupervoid => {
                    let index = in_.read_u32();
                    let argc = in_.read_u32();
                    v.op_callsupervoid(index, argc);
                }
                OP_callpropvoid => {
                    let index = in_.read_u32();
                    let argc = in_.read_u32();
                    v.op_callpropvoid(index, argc);
                }
                OP_li8 => v.op_li8(),
                OP_li16 => v.op_li16(),
                OP_li32 => v.op_li32(),
                OP_lf32 => v.op_lf32(),
                OP_lf64 => v.op_lf64(),
                OP_si8 => v.op_si8(),
                OP_si16 => v.op_si16(),
                OP_si32 => v.op_si32(),
                OP_sf32 => v.op_sf32(),
                OP_sf64 => v.op_sf64(),
                OP_sxi1 => v.op_sxi1(),
                OP_sxi8 => v.op_sxi8(),
                OP_sxi16 => v.op_sxi16(),
                _ => return Err(DecoderException::new(format!("unknown opcode?? {}", opcode))),
            }
        }

        in_.seek(original_pos);
        Ok(())
    }
}
