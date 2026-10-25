//! Port of `macromedia.abc.Visitor`; every method defaults to doing nothing, as `OpcodeVisitor` does.

use super::decoder_exception::DecoderException;

pub trait Visitor {
    fn method_info(
        &mut self,
        _return_type: i32,
        _param_types: Option<&[i32]>,
        _native_name: i32,
        _flags: i32,
        _values: Option<&[i32]>,
        _value_kinds: Option<&[i32]>,
        _param_names: Option<&[i32]>,
    ) -> Result<(), DecoderException> {
        Ok(())
    }
    fn metadata_info(&mut self, _index: i32, _name: i32, _keys: Option<&[i32]>, _values: Option<&[i32]>) -> Result<(), DecoderException> {
        Ok(())
    }
    fn start_instance(
        &mut self,
        _name: i32,
        _super_name: i32,
        _is_dynamic: bool,
        _is_final: bool,
        _is_interface: bool,
        _interfaces: Option<&[i32]>,
        _iinit: i32,
        _protected_namespace: i32,
    ) -> Result<(), DecoderException> {
        Ok(())
    }
    fn end_instance(&mut self) {}
    fn start_class(&mut self, _name: i32, _cinit: i32) -> Result<(), DecoderException> {
        Ok(())
    }
    fn end_class(&mut self) {}
    fn start_script(&mut self, _init_id: i32) {}
    fn end_script(&mut self) {}
    fn start_method_body(
        &mut self,
        _method_info: i32,
        _max_stack: i32,
        _max_regs: i32,
        _scope_depth: i32,
        _max_scope: i32,
        _code_start: i32,
        _code_length: i64,
    ) -> Result<(), DecoderException> {
        Ok(())
    }
    fn end_method_body(&mut self) {}
    fn start_opcodes(&mut self, _method_info: i32) {}
    fn end_opcodes(&mut self) {}
    fn exception(&mut self, _start: i64, _end: i64, _target: i64, _type: i32, _name: i32) {}
    fn start_exceptions(&mut self, _exception_count: i32) {}
    fn end_exceptions(&mut self) {}
    fn trait_count(&mut self, _trait_count: i32) {}
    fn slot_trait(
        &mut self,
        _kind: i32,
        _name: i32,
        _slot_id: i32,
        _type: i32,
        _value: i32,
        _value_kind: i32,
        _metadata: Option<&[i32]>,
    ) -> Result<(), DecoderException> {
        Ok(())
    }
    fn method_trait(
        &mut self,
        _kind: i32,
        _name: i32,
        _disp_id: i32,
        _method_info: i32,
        _metadata: Option<&[i32]>,
    ) -> Result<(), DecoderException> {
        Ok(())
    }
    fn class_trait(
        &mut self,
        _kind: i32,
        _name: i32,
        _slot_id: i32,
        _class_index: i32,
        _metadata: Option<&[i32]>,
    ) -> Result<(), DecoderException> {
        Ok(())
    }
    fn function_trait(
        &mut self,
        _kind: i32,
        _name: i32,
        _slot_id: i32,
        _method_info: i32,
        _metadata: Option<&[i32]>,
    ) -> Result<(), DecoderException> {
        Ok(())
    }
    fn target(&mut self, _pos: i32) {}
    fn op_returnvoid(&mut self) {}
    fn op_returnvalue(&mut self) {}
    fn op_nop(&mut self) {}
    fn op_bkpt(&mut self) {}
    fn op_timestamp(&mut self) {}
    fn op_debugline(&mut self, _linenum: i32) {}
    fn op_bkptline(&mut self) {}
    fn op_debug(&mut self, _di_local: i32, _index: i32, _slot: i32, _linenum: i32) {}
    fn op_debugfile(&mut self, _index: i32) {}
    fn op_jump(&mut self, _jump: i32, _pos: i32) {}
    fn op_pushnull(&mut self) {}
    fn op_pushundefined(&mut self) {}
    fn op_pushstring(&mut self, _index: i32) {}
    fn op_pushnamespace(&mut self, _index: i32) {}
    fn op_pushint(&mut self, _index: i32) {}
    fn op_pushuint(&mut self, _index: i32) {}
    fn op_pushdouble(&mut self, _index: i32) {}
    fn op_getlocal(&mut self, _index: i32) {}
    fn op_pushtrue(&mut self) {}
    fn op_pushfalse(&mut self) {}
    fn op_pushnan(&mut self) {}
    fn op_pop(&mut self) {}
    fn op_dup(&mut self) {}
    fn op_swap(&mut self) {}
    fn op_pushdecimal(&mut self, _index: i32) {}
    fn op_pushdnan(&mut self) {}
    fn op_convert_s(&mut self) {}
    fn op_esc_xelem(&mut self) {}
    fn op_esc_xattr(&mut self) {}
    fn op_checkfilter(&mut self) {}
    fn op_convert_d(&mut self) {}
    fn op_convert_b(&mut self) {}
    fn op_convert_o(&mut self) {}
    fn op_convert_m(&mut self) {}
    fn op_convert_m_p(&mut self, _param: i32) {}
    fn op_negate_p(&mut self, _param: i32) {}
    fn op_negate(&mut self) {}
    fn op_negate_i(&mut self) {}
    fn op_increment_p(&mut self, _param: i32) {}
    fn op_increment(&mut self) {}
    fn op_increment_i(&mut self) {}
    fn op_inclocal(&mut self, _index: i32) {}
    fn op_inclocal_p(&mut self, _param: i32, _index: i32) {}
    fn op_kill(&mut self, _index: i32) {}
    fn op_inclocal_i(&mut self, _index: i32) {}
    fn op_decrement(&mut self) {}
    fn op_decrement_p(&mut self, _param: i32) {}
    fn op_decrement_i(&mut self) {}
    fn op_declocal(&mut self, _index: i32) {}
    fn op_declocal_p(&mut self, _param: i32, _index: i32) {}
    fn op_declocal_i(&mut self, _index: i32) {}
    fn op_typeof(&mut self) {}
    fn op_not(&mut self) {}
    fn op_bitnot(&mut self) {}
    fn op_setlocal(&mut self, _index: i32) {}
    fn op_add(&mut self) {}
    fn op_add_i(&mut self) {}
    fn op_subtract(&mut self) {}
    fn op_subtract_i(&mut self) {}
    fn op_multiply(&mut self) {}
    fn op_multiply_i(&mut self) {}
    fn op_divide(&mut self) {}
    fn op_modulo(&mut self) {}
    fn op_add_p(&mut self, _param: i32) {}
    fn op_subtract_p(&mut self, _param: i32) {}
    fn op_multiply_p(&mut self, _param: i32) {}
    fn op_divide_p(&mut self, _param: i32) {}
    fn op_modulo_p(&mut self, _param: i32) {}
    fn op_lshift(&mut self) {}
    fn op_rshift(&mut self) {}
    fn op_urshift(&mut self) {}
    fn op_bitand(&mut self) {}
    fn op_bitor(&mut self) {}
    fn op_bitxor(&mut self) {}
    fn op_equals(&mut self) {}
    fn op_strictequals(&mut self) {}
    fn op_lookupswitch(&mut self, _default_pos: i32, _case_pos: &[i32], _this_pos: i32, _case_table_pos: i32) {}
    fn op_iftrue(&mut self, _offset: i32, _next_pos: i32) {}
    fn op_iffalse(&mut self, _offset: i32, _next_pos: i32) {}
    fn op_ifeq(&mut self, _offset: i32, _next_pos: i32) {}
    fn op_ifne(&mut self, _offset: i32, _next_pos: i32) {}
    fn op_ifstricteq(&mut self, _offset: i32, _next_pos: i32) {}
    fn op_ifstrictne(&mut self, _offset: i32, _next_pos: i32) {}
    fn op_iflt(&mut self, _offset: i32, _next_pos: i32) {}
    fn op_ifle(&mut self, _offset: i32, _next_pos: i32) {}
    fn op_ifgt(&mut self, _offset: i32, _next_pos: i32) {}
    fn op_ifge(&mut self, _offset: i32, _next_pos: i32) {}
    fn op_lessthan(&mut self) {}
    fn op_lessequals(&mut self) {}
    fn op_greaterthan(&mut self) {}
    fn op_greaterequals(&mut self) {}
    fn op_newobject(&mut self, _size: i32) {}
    fn op_newarray(&mut self, _size: i32) {}
    fn op_getproperty(&mut self, _index: i32) {}
    fn op_setproperty(&mut self, _index: i32) {}
    fn op_initproperty(&mut self, _index: i32) {}
    fn op_getdescendants(&mut self, _index: i32) {}
    fn op_findpropstrict(&mut self, _index: i32) {}
    fn op_findproperty(&mut self, _index: i32) {}
    fn op_finddef(&mut self, _index: i32) {}
    fn op_nextname(&mut self) {}
    fn op_nextvalue(&mut self) {}
    fn op_hasnext(&mut self) {}
    fn op_hasnext2(&mut self, _object_register: i32, _index_register: i32) {}
    fn op_getlex(&mut self, _index: i32) {}
    fn op_deleteproperty(&mut self, _index: i32) {}
    fn op_setslot(&mut self, _index: i32) {}
    fn op_getslot(&mut self, _index: i32) {}
    fn op_setglobalslot(&mut self, _index: i32) {}
    fn op_getglobalslot(&mut self, _index: i32) {}
    fn op_call(&mut self, _size: i32) {}
    fn op_construct(&mut self, _size: i32) {}
    fn op_applytype(&mut self, _size: i32) {}
    fn op_newfunction(&mut self, _id: i32) {}
    fn op_newclass(&mut self, _id: i32) {}
    fn op_callstatic(&mut self, _id: i32, _argc: i32) {}
    fn op_callmethod(&mut self, _id: i32, _argc: i32) {}
    fn op_callproperty(&mut self, _index: i32, _argc: i32) {}
    fn op_callproplex(&mut self, _index: i32, _argc: i32) {}
    fn op_constructprop(&mut self, _index: i32, _argc: i32) {}
    fn op_callsuper(&mut self, _index: i32, _argc: i32) {}
    fn op_getsuper(&mut self, _index: i32) {}
    fn op_setsuper(&mut self, _index: i32) {}
    fn op_constructsuper(&mut self, _argc: i32) {}
    fn op_pushshort(&mut self, _n: i32) {}
    fn op_astype(&mut self, _index: i32) {}
    fn op_astypelate(&mut self) {}
    fn op_coerce(&mut self, _index: i32) {}
    fn op_coerce_b(&mut self) {}
    fn op_coerce_o(&mut self) {}
    fn op_coerce_a(&mut self) {}
    fn op_coerce_i(&mut self) {}
    fn op_coerce_u(&mut self) {}
    fn op_coerce_d(&mut self) {}
    fn op_coerce_s(&mut self) {}
    fn op_istype(&mut self, _index: i32) {}
    fn op_istypelate(&mut self) {}
    fn op_pushbyte(&mut self, _n: i32) {}
    fn op_getscopeobject(&mut self, _index: i32) {}
    fn op_pushscope(&mut self) {}
    fn op_popscope(&mut self) {}
    fn op_convert_i(&mut self) {}
    fn op_convert_u(&mut self) {}
    fn op_throw(&mut self) {}
    fn op_instanceof(&mut self) {}
    fn op_in(&mut self) {}
    fn op_dxns(&mut self, _index: i32) {}
    fn op_dxnslate(&mut self) {}
    fn op_ifnlt(&mut self, _offset: i32, _next_pos: i32) {}
    fn op_ifnle(&mut self, _offset: i32, _next_pos: i32) {}
    fn op_ifngt(&mut self, _offset: i32, _next_pos: i32) {}
    fn op_ifnge(&mut self, _offset: i32, _next_pos: i32) {}
    fn op_pushwith(&mut self) {}
    fn op_newactivation(&mut self) {}
    fn op_newcatch(&mut self, _index: i32) {}
    fn op_deldescendants(&mut self) {}
    fn op_getglobalscope(&mut self) {}
    fn op_getlocal0(&mut self) {}
    fn op_getlocal1(&mut self) {}
    fn op_getlocal2(&mut self) {}
    fn op_getlocal3(&mut self) {}
    fn op_setlocal0(&mut self) {}
    fn op_setlocal1(&mut self) {}
    fn op_setlocal2(&mut self) {}
    fn op_setlocal3(&mut self) {}
    fn op_label(&mut self) {}
    fn op_pushconstant(&mut self, _id: i32) {}
    fn op_callsupervoid(&mut self, _index: i32, _argc: i32) {}
    fn op_callpropvoid(&mut self, _index: i32, _argc: i32) {}
    fn op_li8(&mut self) {}
    fn op_li16(&mut self) {}
    fn op_li32(&mut self) {}
    fn op_lf32(&mut self) {}
    fn op_lf64(&mut self) {}
    fn op_si8(&mut self) {}
    fn op_si16(&mut self) {}
    fn op_si32(&mut self) {}
    fn op_sf32(&mut self) {}
    fn op_sf64(&mut self) {}
    fn op_sxi1(&mut self) {}
    fn op_sxi8(&mut self) {}
    fn op_sxi16(&mut self) {}
}
