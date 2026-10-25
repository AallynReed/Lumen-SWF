//! Port of `macromedia.abc.DefaultVisitor`: the abstract hooks are a trait, its final `Visitor` methods are
//! functions here, and `default_visitor_methods!` wires them into a `Visitor` impl.

use super::constant_pool::PoolValue;
use super::decoder::Decoder;
use super::decoder_exception::DecoderException;
use super::multi_name::MultiName;
use super::q_name::QName;
use super::visitor::Visitor;
use crate::asc::emit::constants::*;

pub trait DefaultVisitor<'a>: Visitor {
    fn decoder(&self) -> &'a Decoder;

    fn begin_abc(&mut self);
    fn end_abc(&mut self);
    fn metadata(&mut self, name: Option<String>, keys: Option<Vec<Option<String>>>, values: Option<Vec<Option<String>>>);
    fn begin_class(&mut self, name: Option<QName>, slot_id: i32);
    fn instance_info(&mut self, name: Option<QName>, super_name: Option<QName>, interfaces: Option<Vec<Option<MultiName>>>);
    fn begin_i_init(&mut self, method_info: i32);
    fn end_i_init(&mut self);
    fn class_info(&mut self, name: Option<QName>);
    fn begin_c_init(&mut self, method_info: i32);
    fn end_c_init(&mut self);
    fn end_class_named(&mut self, name: Option<QName>);
    fn begin_var(&mut self, name: Option<QName>, slot_id: i32, type_: Option<QName>, value: Option<PoolValue>);
    fn end_var(&mut self, name: Option<QName>);
    fn begin_const(&mut self, name: Option<QName>, slot_id: i32, type_: Option<QName>, value: Option<PoolValue>);
    fn end_const(&mut self, name: Option<QName>);
    #[allow(clippy::too_many_arguments)]
    fn method_info_resolved(
        &mut self,
        return_type: Option<QName>,
        param_types: Option<Vec<Option<QName>>>,
        native_name: Option<String>,
        flags: i32,
        values: Option<Vec<Option<PoolValue>>>,
        param_names: Option<Vec<Option<String>>>,
    );
    fn begin_getter(&mut self, method_info: i32, name: Option<QName>, disp_id: i32, attr: i32);
    fn end_getter(&mut self, name: Option<QName>);
    fn begin_setter(&mut self, method_info: i32, name: Option<QName>, disp_id: i32, attr: i32);
    fn end_setter(&mut self, name: Option<QName>);
    fn begin_method(&mut self, method_info: i32, name: Option<QName>, disp_id: i32, attr: i32);
    fn end_method(&mut self, name: Option<QName>);
    fn begin_function(&mut self, method_info: i32, name: Option<QName>, slot_id: i32);
    fn end_function(&mut self, name: Option<QName>);
    fn begin_body(&mut self, method_id: i32, code_start: i32, code_length: i64);
    fn end_body(&mut self);
}

type R = Result<(), DecoderException>;

fn strings(d: &Decoder, ids: Option<&[i32]>) -> Result<Option<Vec<Option<String>>>, DecoderException> {
    match ids {
        None => Ok(None),
        Some(ids) => Ok(Some(ids.iter().map(|&i| d.constant_pool.get_string(i)).collect::<Result<_, _>>()?)),
    }
}

fn decode_metadata<'a, V: DefaultVisitor<'a>>(v: &mut V, metadata: Option<&[i32]>) -> R {
    let d = v.decoder();
    for &m in metadata.unwrap_or(&[]) {
        d.metadata_info.decode(m, v)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn method_info<'a, V: DefaultVisitor<'a>>(
    v: &mut V,
    return_type_id: i32,
    param_type_ids: Option<&[i32]>,
    native_name_id: i32,
    flags: i32,
    value_ids: Option<&[i32]>,
    value_kinds: Option<&[i32]>,
    param_names: Option<&[i32]>,
) -> R {
    let d = v.decoder();
    let cp = &d.constant_pool;
    let return_type = cp.get_qname(return_type_id)?;
    let native_name = cp.get_string(native_name_id)?;
    let params = match param_type_ids {
        None => None,
        Some(ids) => Some(ids.iter().map(|&i| cp.get_qname(i)).collect::<Result<Vec<_>, _>>()?),
    };
    let values = match value_ids {
        None => None,
        Some(ids) => {
            let kinds = value_kinds.unwrap_or(&[]);
            Some(ids.iter().zip(kinds).map(|(&i, &k)| cp.get(i, k)).collect::<Result<Vec<_>, _>>()?)
        }
    };
    let arg_names = strings(d, param_names)?;
    v.method_info_resolved(return_type, params, native_name, flags, values, arg_names);
    Ok(())
}

pub fn metadata_info<'a, V: DefaultVisitor<'a>>(
    v: &mut V,
    _index: i32,
    name_id: i32,
    key_ids: Option<&[i32]>,
    value_ids: Option<&[i32]>,
) -> R {
    let d = v.decoder();
    let name = d.constant_pool.get_string(name_id)?;
    let keys = strings(d, key_ids)?;
    let values = strings(d, value_ids)?;
    v.metadata(name, keys, values);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn slot_trait<'a, V: DefaultVisitor<'a>>(
    v: &mut V,
    kind: i32,
    name_id: i32,
    slot_id: i32,
    type_id: i32,
    value_id: i32,
    value_kind: i32,
    metadata: Option<&[i32]>,
) -> R {
    let d = v.decoder();
    let name = d.constant_pool.get_qname(name_id)?;
    let type_ = d.constant_pool.get_qname(type_id)?;
    let value = d.constant_pool.get(value_id, value_kind)?;

    let tag = kind & 0x0f;
    if tag == TRAIT_Var {
        v.begin_var(name.clone(), slot_id, type_, value);
    } else {
        v.begin_const(name.clone(), slot_id, type_, value);
    }
    decode_metadata(v, metadata)?;
    if tag == TRAIT_Var {
        v.end_var(name);
    } else {
        v.end_const(name);
    }
    Ok(())
}

pub fn method_trait<'a, V: DefaultVisitor<'a>>(
    v: &mut V,
    kind: i32,
    name_id: i32,
    disp_id: i32,
    method_info_id: i32,
    metadata: Option<&[i32]>,
) -> R {
    let d = v.decoder();
    let name = d.constant_pool.get_qname(name_id)?;
    let tag = kind & 0x0f;
    let attr = kind >> 4;
    if tag == TRAIT_Getter {
        v.begin_getter(method_info_id, name.clone(), disp_id, attr);
    } else if tag == TRAIT_Setter {
        v.begin_setter(method_info_id, name.clone(), disp_id, attr);
    } else {
        v.begin_method(method_info_id, name.clone(), disp_id, attr);
    }
    d.method_info.decode(method_info_id, v)?;
    decode_metadata(v, metadata)?;
    if tag == TRAIT_Getter {
        v.end_getter(name);
    } else if tag == TRAIT_Setter {
        v.end_setter(name);
    } else {
        v.end_method(name);
    }
    Ok(())
}

pub fn class_trait<'a, V: DefaultVisitor<'a>>(
    v: &mut V,
    _kind: i32,
    name_id: i32,
    slot_id: i32,
    class_index: i32,
    metadata: Option<&[i32]>,
) -> R {
    let d = v.decoder();
    let name = d.constant_pool.get_qname(name_id)?;
    v.begin_class(name.clone(), slot_id);
    d.class_info.decode(class_index, v)?;
    decode_metadata(v, metadata)?;
    v.end_class_named(name);
    Ok(())
}

pub fn function_trait<'a, V: DefaultVisitor<'a>>(
    v: &mut V,
    _kind: i32,
    name_id: i32,
    slot_id: i32,
    method_info_id: i32,
    metadata: Option<&[i32]>,
) -> R {
    let d = v.decoder();
    let name = d.constant_pool.get_qname(name_id)?;
    v.begin_function(method_info_id, name.clone(), slot_id);
    d.method_info.decode(method_info_id, v)?;
    decode_metadata(v, metadata)?;
    v.end_function(name);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn start_instance<'a, V: DefaultVisitor<'a>>(
    v: &mut V,
    name_id: i32,
    super_name_id: i32,
    _is_dynamic: bool,
    _is_final: bool,
    _is_interface: bool,
    interface_ids: Option<&[i32]>,
    iinit_id: i32,
    _protected_namespace: i32,
) -> R {
    let d = v.decoder();
    let name = d.constant_pool.get_qname(name_id)?;
    let super_name = d.constant_pool.get_qname(super_name_id)?;
    let interfaces = match interface_ids {
        None => None,
        Some(ids) => Some(ids.iter().map(|&i| d.constant_pool.get_multi_name(i)).collect::<Result<Vec<_>, _>>()?),
    };
    v.instance_info(name, super_name, interfaces);
    v.begin_i_init(iinit_id);
    d.method_info.decode(iinit_id, v)?;
    v.end_i_init();
    Ok(())
}

pub fn start_class<'a, V: DefaultVisitor<'a>>(v: &mut V, name_id: i32, cinit: i32) -> R {
    let d = v.decoder();
    let name = d.constant_pool.get_qname(name_id)?;
    v.class_info(name);
    v.begin_c_init(cinit);
    d.method_info.decode(cinit, v)?;
    v.end_c_init();
    Ok(())
}

/// Expands, inside `impl Visitor for T` where `T: DefaultVisitor`, to the methods `DefaultVisitor` makes final.
#[macro_export]
macro_rules! default_visitor_methods {
    () => {
        fn method_info(
            &mut self,
            return_type: i32,
            param_types: Option<&[i32]>,
            native_name: i32,
            flags: i32,
            values: Option<&[i32]>,
            value_kinds: Option<&[i32]>,
            param_names: Option<&[i32]>,
        ) -> Result<(), $crate::asc::abc::decoder_exception::DecoderException> {
            $crate::asc::abc::default_visitor::method_info(
                self,
                return_type,
                param_types,
                native_name,
                flags,
                values,
                value_kinds,
                param_names,
            )
        }
        fn metadata_info(
            &mut self,
            index: i32,
            name: i32,
            keys: Option<&[i32]>,
            values: Option<&[i32]>,
        ) -> Result<(), $crate::asc::abc::decoder_exception::DecoderException> {
            $crate::asc::abc::default_visitor::metadata_info(self, index, name, keys, values)
        }
        fn trait_count(&mut self, _trait_count: i32) {}
        fn slot_trait(
            &mut self,
            kind: i32,
            name: i32,
            slot_id: i32,
            type_: i32,
            value: i32,
            value_kind: i32,
            metadata: Option<&[i32]>,
        ) -> Result<(), $crate::asc::abc::decoder_exception::DecoderException> {
            $crate::asc::abc::default_visitor::slot_trait(self, kind, name, slot_id, type_, value, value_kind, metadata)
        }
        fn method_trait(
            &mut self,
            kind: i32,
            name: i32,
            disp_id: i32,
            method_info: i32,
            metadata: Option<&[i32]>,
        ) -> Result<(), $crate::asc::abc::decoder_exception::DecoderException> {
            $crate::asc::abc::default_visitor::method_trait(self, kind, name, disp_id, method_info, metadata)
        }
        fn class_trait(
            &mut self,
            kind: i32,
            name: i32,
            slot_id: i32,
            class_index: i32,
            metadata: Option<&[i32]>,
        ) -> Result<(), $crate::asc::abc::decoder_exception::DecoderException> {
            $crate::asc::abc::default_visitor::class_trait(self, kind, name, slot_id, class_index, metadata)
        }
        fn function_trait(
            &mut self,
            kind: i32,
            name: i32,
            slot_id: i32,
            method_info: i32,
            metadata: Option<&[i32]>,
        ) -> Result<(), $crate::asc::abc::decoder_exception::DecoderException> {
            $crate::asc::abc::default_visitor::function_trait(self, kind, name, slot_id, method_info, metadata)
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
        ) -> Result<(), $crate::asc::abc::decoder_exception::DecoderException> {
            $crate::asc::abc::default_visitor::start_instance(
                self,
                name,
                super_name,
                is_dynamic,
                is_final,
                is_interface,
                interfaces,
                iinit,
                protected_namespace,
            )
        }
        fn end_instance(&mut self) {}
        fn start_class(&mut self, name: i32, cinit: i32) -> Result<(), $crate::asc::abc::decoder_exception::DecoderException> {
            $crate::asc::abc::default_visitor::start_class(self, name, cinit)
        }
        fn end_class(&mut self) {}
        fn start_script(&mut self, _init_id: i32) {
            $crate::asc::abc::default_visitor::DefaultVisitor::begin_abc(self);
        }
        fn end_script(&mut self) {
            $crate::asc::abc::default_visitor::DefaultVisitor::end_abc(self);
        }
        fn start_method_body(
            &mut self,
            method_info: i32,
            _max_stack: i32,
            _max_regs: i32,
            _scope_depth: i32,
            _max_scope: i32,
            code_start: i32,
            code_length: i64,
        ) -> Result<(), $crate::asc::abc::decoder_exception::DecoderException> {
            $crate::asc::abc::default_visitor::DefaultVisitor::begin_body(self, method_info, code_start, code_length);
            Ok(())
        }
        fn end_method_body(&mut self) {
            $crate::asc::abc::default_visitor::DefaultVisitor::end_body(self);
        }
        fn start_opcodes(&mut self, _method_info: i32) {}
        fn end_opcodes(&mut self) {}
        fn exception(&mut self, _start: i64, _end: i64, _target: i64, _type: i32, _name: i32) {}
        fn start_exceptions(&mut self, _exception_count: i32) {}
        fn end_exceptions(&mut self) {}
    };
}
