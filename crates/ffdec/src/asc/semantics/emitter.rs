//! Port of `macromedia/asc/semantics/Emitter.java`: a decorator chain where every method forwards to `impl`.
//! Subclasses implement [`Emitter`] and override any subset; `base::x(self, ...)` is Java's `super.x(...)`.

#![allow(clippy::too_many_arguments)]

use std::cell::RefCell;
use std::rc::Rc;

use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::q_name::QName;
use crate::asc::semantics::type_info::TypeInfo;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::util::byte_list::ByteList;
use crate::asc::util::context::ContextRef;
use crate::asc::util::namespaces::Namespaces;
use crate::asc::util::number_constant::NumberConstant;
use crate::asc::util::number_usage::NumberUsage;

pub type EmitterRef = Rc<RefCell<dyn Emitter>>;

/// The fields of `Emitter`.
#[derive(Default)]
pub struct EmitterBase {
    doing_method: bool,
    doing_class: bool,
    doing_package: bool,
    pub impl_: Option<EmitterRef>,
    pub origin: Option<String>,
    pub ln_num: i32,
    pub col_pos: i32,
    pub pos: i32,
}

impl EmitterBase {
    /// `new Emitter(impl)`.
    pub fn new(impl_: Option<EmitterRef>) -> EmitterBase {
        EmitterBase { impl_, ..Default::default() }
    }
}

fn impl_of<E: Emitter + ?Sized>(this: &E) -> Option<EmitterRef> {
    this.emitter_base().impl_.clone()
}

macro_rules! emitter_methods {
    ($(fn $name:ident(&mut $this:ident $(, $arg:ident: $ty:ty)* $(,)?) $(-> $ret:ty)? $body:block)*) => {
        /// `Emitter`'s methods; each default is the Java base body, also callable as `base::name(self, ...)`.
        pub trait Emitter {
            fn emitter_base(&self) -> &EmitterBase;
            fn emitter_base_mut(&mut self) -> &mut EmitterBase;
            $(fn $name(&mut self $(, $arg: $ty)*) $(-> $ret)? { base::$name(self $(, $arg)*) })*
        }

        /// The `Emitter` implementations, for `super.name(...)` calls from overrides.
        pub mod base {
            use super::*;
            $(pub fn $name<E: Emitter + ?Sized>($this: &mut E $(, $arg: $ty)*) $(-> $ret)? $body)*
        }
    };
}

macro_rules! fwd {
    ($this:ident.$name:ident($($arg:expr),*)) => {
        if let Some(i) = impl_of($this) {
            i.borrow_mut().$name($($arg),*);
        }
    };
}

emitter_methods! {
    fn get_method_info(&mut this, name: &str) -> i32 {
        let mut n = -1;
        if let Some(i) = impl_of(this) {
            n = i.borrow_mut().get_method_info(name);
        }
        n
    }

    fn get_method_id(&mut this, name: &str, namespaces: &Namespaces) -> i32 {
        let mut n = -1;
        if let Some(i) = impl_of(this) {
            n = i.borrow_mut().get_method_id(name, namespaces);
        }
        n
    }

    fn emit(&mut this, bytes: &mut ByteList) {
        fwd!(this.emit(bytes));
    }

    fn get_value_of_number_literal(&mut this, str: &str, type_: &mut Option<TypeValue>, usage: Option<&NumberUsage>) -> Option<NumberConstant> {
        if let Some(i) = impl_of(this) {
            return i.borrow_mut().get_value_of_number_literal(str, type_, usage);
        }
        None
    }

    fn set_position(&mut this, ln_num: i32, col_pos: i32, pos: i32) {
        if let Some(i) = impl_of(this) {
            i.borrow_mut().set_position(ln_num, col_pos, pos);
        } else {
            let b = this.emitter_base_mut();
            b.ln_num = ln_num;
            b.col_pos = col_pos;
            b.pos = pos;
        }
    }

    fn clear_position_info(&mut this) {
        fwd!(this.clear_position_info());
    }

    fn set_origin(&mut this, origin: &str) {
        if let Some(i) = impl_of(this) {
            i.borrow_mut().set_origin(origin);
        } else {
            this.emitter_base_mut().origin = Some(origin.to_string());
        }
    }

    fn get_origin_and_position(&mut this, origin: &mut Option<String>, ln_num: &mut i32, col_pos: &mut i32) -> i32 {
        if let Some(i) = impl_of(this) {
            i.borrow_mut().get_origin_and_position(origin, ln_num, col_pos)
        } else {
            let b = this.emitter_base();
            *origin = b.origin.clone();
            *ln_num = b.ln_num;
            *col_pos = b.col_pos;
            b.pos
        }
    }

    fn define_slot_variable(&mut this, cx: &ContextRef, debug_name: &str, function_debug_name: &str, pos: i32, type_: Option<&TypeInfo>, slot_index: i32) {
        fwd!(this.define_slot_variable(cx, debug_name, function_debug_name, pos, type_, slot_index));
    }

    fn start_method_empty(&mut this) {
        this.start_method("", 0, 0, 0, false, 0);
    }

    fn start_method_counts(&mut this, name: &str, param_count: i32, local_count: i32) {
        this.start_method(name, param_count, local_count, 0, false, 0);
    }

    fn start_method(&mut this, name: &str, param_count: i32, local_count: i32, temp_count: i32, needs_activation: bool, needs_arguments: i32) {
        this.emitter_base_mut().doing_method = true;
        fwd!(this.start_method(name, param_count, local_count, temp_count, needs_activation, needs_arguments));
    }

    fn finish_method(
        &mut this,
        cx: &ContextRef,
        name: &str,
        type_: Option<&TypeInfo>,
        types: Option<&[TypeInfo]>,
        activation: Option<&ObjectValue>,
        needs_arguments: i32,
        scope_depth: i32,
        debug_name: &str,
        is_native: bool,
        is_interface: bool,
        arg_names: Option<&[String]>,
    ) -> i32 {
        this.emitter_base_mut().doing_method = false;
        if let Some(i) = impl_of(this) {
            return i.borrow_mut().finish_method(
                cx, name, type_, types, activation, needs_arguments, scope_depth, debug_name, is_native, is_interface, arg_names,
            );
        }
        0
    }

    fn start_class(&mut this, name: &str) {
        this.emitter_base_mut().doing_class = true;
        fwd!(this.start_class(name));
    }

    fn finish_class(&mut this, cx: &ContextRef, name: &QName, basename: Option<&QName>, is_dynamic: bool, is_final: bool, is_interface: bool, is_nullable: bool) {
        this.emitter_base_mut().doing_class = false;
        fwd!(this.finish_class(cx, name, basename, is_dynamic, is_final, is_interface, is_nullable));
    }

    fn start_program(&mut this, filename: &str) {
        fwd!(this.start_program(filename));
    }

    fn finish_program(&mut this, cx: &ContextRef, name: &str, init_info: i32) {
        fwd!(this.finish_program(cx, name, init_info));
    }

    fn doing_method(&mut this) -> bool {
        this.emitter_base().doing_method
    }

    fn doing_class(&mut this) -> bool {
        this.emitter_base().doing_class
    }

    fn doing_package(&mut this) -> bool {
        this.emitter_base().doing_package
    }

    fn allocate_temp(&mut this) -> i32 {
        if let Some(i) = impl_of(this) {
            return i.borrow_mut().allocate_temp();
        }
        0
    }

    fn free_temp(&mut this, t: i32) {
        fwd!(this.free_temp(t));
    }

    fn kill(&mut this, t: i32) {
        fwd!(this.kill(t));
    }

    fn get_temp_count(&mut this) -> i32 {
        if let Some(i) = impl_of(this) {
            return i.borrow_mut().get_temp_count();
        }
        0
    }

    fn get_ip(&mut this) -> i32 {
        if let Some(i) = impl_of(this) {
            return i.borrow_mut().get_ip();
        }
        0
    }

    fn break_(&mut this, loop_index: i32) {
        fwd!(this.break_(loop_index));
    }

    fn case_label(&mut this, is_default: bool) {
        fwd!(this.case_label(is_default));
    }

    fn label_statement_end(&mut this, loop_index: i32) {
        fwd!(this.label_statement_end(loop_index));
    }

    fn label_statement_begin(&mut this) {
        fwd!(this.label_statement_begin());
    }

    fn try_(&mut this, has_finally: bool) {
        fwd!(this.try_(has_finally));
    }

    fn catch_clauses_begin(&mut this) {
        fwd!(this.catch_clauses_begin());
    }

    fn catch_clauses_end(&mut this) {
        fwd!(this.catch_clauses_end());
    }

    fn call_finally(&mut this, num_finallys: i32) {
        fwd!(this.call_finally(num_finallys));
    }

    fn finally_clause_begin(&mut this) {
        fwd!(this.finally_clause_begin());
    }

    fn finally_clause_end(&mut this) {
        fwd!(this.finally_clause_end());
    }

    fn catch(&mut this, type_: Option<&TypeValue>, name: Option<&QName>) {
        fwd!(this.catch(type_, name));
    }

    fn throw(&mut this) {
        fwd!(this.throw());
    }

    fn nop(&mut this) {
        fwd!(this.nop());
    }

    fn check_type(&mut this, name: &QName) {
        fwd!(this.check_type(name));
    }

    fn continue_(&mut this, loop_index: i32) {
        fwd!(this.continue_(loop_index));
    }

    fn define_var(&mut this, name: &str, value: i32, slot: i32) -> i32 {
        if let Some(i) = impl_of(this) {
            return i.borrow_mut().define_var(name, value, slot);
        }
        -1
    }

    fn dup(&mut this) {
        fwd!(this.dup());
    }

    fn else_(&mut this) {
        fwd!(this.else_());
    }

    fn get_property(&mut this, is_qualified: bool, is_super: bool, is_attr: bool, used_def_namespaces: Option<&Namespaces>) {
        fwd!(this.get_property(is_qualified, is_super, is_attr, used_def_namespaces));
    }

    fn get_property_name(&mut this, name: &str, is_super: bool, is_attr: bool) {
        fwd!(this.get_property_name(name, is_super, is_attr));
    }

    fn get_property_name_nss(&mut this, name: &str, namespaces: Option<&Namespaces>, is_qualified: bool, is_super: bool, is_attr: bool) {
        fwd!(this.get_property_name_nss(name, namespaces, is_qualified, is_super, is_attr));
    }

    fn delete_property(&mut this, is_qualified: bool, is_super: bool, is_attr: bool, used_def_namespaces: Option<&Namespaces>) {
        fwd!(this.delete_property(is_qualified, is_super, is_attr, used_def_namespaces));
    }

    fn delete_property_name(&mut this, name: &str, is_super: bool, is_attr: bool) {
        fwd!(this.delete_property_name(name, is_super, is_attr));
    }

    fn delete_property_name_nss(&mut this, name: &str, namespaces: Option<&Namespaces>, is_qualified: bool, is_super: bool, is_attr: bool) {
        fwd!(this.delete_property_name_nss(name, namespaces, is_qualified, is_super, is_attr));
    }

    fn set_property(&mut this, is_qualified: bool, is_super: bool, is_attr: bool, used_def_namespaces: Option<&Namespaces>, is_constinit: bool) {
        fwd!(this.set_property(is_qualified, is_super, is_attr, used_def_namespaces, is_constinit));
    }

    fn set_property_name(&mut this, name: &str, is_super: bool, is_attr: bool) {
        fwd!(this.set_property_name(name, is_super, is_attr));
    }

    fn set_property_name_nss(&mut this, name: &str, namespaces: Option<&Namespaces>, is_qualified: bool, is_super: bool, is_attr: bool, is_constinit: bool) {
        fwd!(this.set_property_name_nss(name, namespaces, is_qualified, is_super, is_attr, is_constinit));
    }

    fn get_descendants(&mut this, is_qualified: bool, is_attr: bool, used_def_namespaces: Option<&Namespaces>) {
        fwd!(this.get_descendants(is_qualified, is_attr, used_def_namespaces));
    }

    fn get_descendants_name(&mut this, name: &str, is_super: bool, is_attr: bool) {
        fwd!(this.get_descendants_name(name, is_super, is_attr));
    }

    fn get_descendants_name_nss(&mut this, name: &str, namespaces: Option<&Namespaces>, is_qualified: bool, is_super: bool, is_attr: bool) {
        fwd!(this.get_descendants_name_nss(name, namespaces, is_qualified, is_super, is_attr));
    }

    fn delete_descendants_name(&mut this, name: &str, is_super: bool, is_attr: bool) {
        fwd!(this.delete_descendants_name(name, is_super, is_attr));
    }

    fn delete_descendants_name_nss(&mut this, name: &str, namespaces: Option<&Namespaces>, is_qualified: bool, is_super: bool, is_attr: bool) {
        fwd!(this.delete_descendants_name_nss(name, namespaces, is_qualified, is_super, is_attr));
    }

    fn get_base_object(&mut this, scope_index: i32) {
        fwd!(this.get_base_object(scope_index));
    }

    fn get_global_scope(&mut this) {
        fwd!(this.get_global_scope());
    }

    fn get_scope_chain(&mut this) {
        fwd!(this.get_scope_chain());
    }

    fn get_scope_on_top(&mut this) {
        fwd!(this.get_scope_on_top());
    }

    fn if_(&mut this, kind: i32) {
        fwd!(this.if_(kind));
    }

    fn invoke_binary(&mut this, operator_id: i32, number_usage: Option<&NumberUsage>) {
        fwd!(this.invoke_binary(operator_id, number_usage));
    }

    fn invoke_closure(&mut this, as_construct: bool, size: i32) {
        fwd!(this.invoke_closure(as_construct, size));
    }

    fn invoke_method(&mut this, local_dispatch: bool, method_id: i32, size: i32) {
        fwd!(this.invoke_method(local_dispatch, method_id, size));
    }

    fn invoke_super(&mut this, construct: bool, size: i32) {
        fwd!(this.invoke_super(construct, size));
    }

    fn apply_type(&mut this, size: i32) {
        fwd!(this.apply_type(size));
    }

    fn call_property(&mut this, name: &str, namespaces: Option<&Namespaces>, size: i32, is_qualified: bool, is_super: bool, is_attr: bool, is_lex: bool) {
        fwd!(this.call_property(name, namespaces, size, is_qualified, is_super, is_attr, is_lex));
    }

    fn construct_property(&mut this, name: &str, namespaces: Option<&Namespaces>, size: i32, is_qualified: bool, is_super: bool, is_attr: bool) {
        fwd!(this.construct_property(name, namespaces, size, is_qualified, is_super, is_attr));
    }

    fn invoke_unary(&mut this, operator_id: i32, size: i32, data: i32, used_def_namespaces: Option<&Namespaces>, usage: Option<&NumberUsage>) {
        fwd!(this.invoke_unary(operator_id, size, data, used_def_namespaces, usage));
    }

    fn load_global(&mut this, index: i32, type_id: i32) {
        fwd!(this.load_global(index, type_id));
    }

    fn load_global_name(&mut this, name: &str) {
        fwd!(this.load_global_name(name));
    }

    fn load_register(&mut this, reg: i32, type_id: i32) {
        fwd!(this.load_register(reg, type_id));
    }

    fn load_this(&mut this) {
        fwd!(this.load_this());
    }

    fn load_super(&mut this) {
        fwd!(this.load_super());
    }

    fn load_var(&mut this, index: i32) {
        fwd!(this.load_var(index));
    }

    fn find_property(&mut this, is_strict: bool, is_attr: bool, is_qualified: bool, used_def_namespaces: Option<&Namespaces>) {
        fwd!(this.find_property(is_strict, is_attr, is_qualified, used_def_namespaces));
    }

    fn find_property_name(&mut this, name: &str, is_strict: bool, is_attr: bool) {
        fwd!(this.find_property_name(name, is_strict, is_attr));
    }

    fn find_property_name_nss(&mut this, name: &str, namespaces: Option<&Namespaces>, is_strict: bool, is_qualified: bool, is_attr: bool) {
        fwd!(this.find_property_name_nss(name, namespaces, is_strict, is_qualified, is_attr));
    }

    fn loop_begin(&mut this) {
        fwd!(this.loop_begin());
    }

    fn loop_end(&mut this, kind: i32) {
        fwd!(this.loop_end(kind));
    }

    fn new_array(&mut this, size: i32) {
        fwd!(this.new_array(size));
    }

    fn new_class_object(&mut this, name: &QName) {
        fwd!(this.new_class_object(name));
    }

    fn new_function_object(&mut this, name: &str) {
        fwd!(this.new_function_object(name));
    }

    fn new_object(&mut this, size: i32) {
        fwd!(this.new_object(size));
    }

    fn patch_break(&mut this, loop_index: i32) {
        fwd!(this.patch_break(loop_index));
    }

    fn patch_continue(&mut this, loop_index: i32) {
        fwd!(this.patch_continue(loop_index));
    }

    fn patch_else(&mut this, addr: i32) {
        fwd!(this.patch_else(addr));
    }

    fn patch_if(&mut this, addr: i32) {
        fwd!(this.patch_if(addr));
    }

    fn patch_loop_begin(&mut this, addr: i32) {
        fwd!(this.patch_loop_begin(addr));
    }

    fn patch_switch_begin(&mut this, addr: i32) {
        fwd!(this.patch_switch_begin(addr));
    }

    fn pop(&mut this) {
        fwd!(this.pop());
    }

    fn push_boolean(&mut this, b: bool) {
        fwd!(this.push_boolean(b));
    }

    fn push_case_index(&mut this, index: i32) {
        fwd!(this.push_case_index(index));
    }

    fn push_empty(&mut this) {
        fwd!(this.push_empty());
    }

    fn push_null(&mut this) {
        fwd!(this.push_null());
    }

    fn push_number(&mut this, val: &NumberConstant, type_id: i32) {
        fwd!(this.push_number(val, type_id));
    }

    fn push_namespace(&mut this, ns: &ObjectValue) {
        fwd!(this.push_namespace(ns));
    }

    fn new_namespace(&mut this, ns: &ObjectValue) {
        fwd!(this.new_namespace(ns));
    }

    fn push_string(&mut this, str: &str) {
        fwd!(this.push_string(str));
    }

    fn push_undefined(&mut this) {
        fwd!(this.push_undefined());
    }

    fn push_uninitialized(&mut this) {
        fwd!(this.push_uninitialized());
    }

    fn return_(&mut this, type_id: i32) {
        fwd!(this.return_(type_id));
    }

    fn pre_store_global(&mut this, index: i32, type_id: i32) {
        fwd!(this.pre_store_global(index, type_id));
    }

    fn store_global(&mut this, index: i32, type_id: i32) {
        fwd!(this.store_global(index, type_id));
    }

    fn store_global_name(&mut this, name: &str) {
        fwd!(this.store_global_name(name));
    }

    fn store_register(&mut this, reg: i32, type_id: i32) {
        this.store_register_name(reg, type_id, "");
    }

    fn store_register_name(&mut this, reg: i32, type_id: i32, var_name: &str) {
        fwd!(this.store_register_name(reg, type_id, var_name));
    }

    fn has_next(&mut this, object_register: i32, index_register: i32) {
        fwd!(this.has_next(object_register, index_register));
    }

    fn store_var(&mut this, index: i32) {
        fwd!(this.store_var(index));
    }

    fn switch_begin(&mut this) {
        fwd!(this.switch_begin());
    }

    fn switch_table(&mut this) {
        fwd!(this.switch_table());
    }

    fn to_boolean(&mut this, type_id: i32) {
        fwd!(this.to_boolean(type_id));
    }

    fn to_native_bool(&mut this) {
        fwd!(this.to_native_bool());
    }

    fn to_int(&mut this) {
        fwd!(this.to_int());
    }

    fn to_uint(&mut this) {
        fwd!(this.to_uint());
    }

    fn to_double(&mut this, type_id: i32) {
        fwd!(this.to_double(type_id));
    }

    fn to_decimal(&mut this, type_id: i32) {
        fwd!(this.to_decimal(type_id));
    }

    fn to_object(&mut this) {
        fwd!(this.to_object());
    }

    fn to_primitive(&mut this) {
        fwd!(this.to_primitive());
    }

    fn to_string_(&mut this) {
        fwd!(this.to_string_());
    }

    fn push_scope(&mut this) {
        fwd!(this.push_scope());
    }

    fn push_with(&mut this) {
        fwd!(this.push_with());
    }

    fn new_activation(&mut this) {
        fwd!(this.new_activation());
    }

    fn new_catch(&mut this, index: i32) {
        fwd!(this.new_catch(index));
    }

    fn pop_scope(&mut this) {
        fwd!(this.pop_scope());
    }

    fn pop_with(&mut this) {
        fwd!(this.pop_with());
    }

    fn default_xml_namespace_name(&mut this, name: &str) {
        fwd!(this.default_xml_namespace_name(name));
    }

    fn default_xml_namespace(&mut this) {
        fwd!(this.default_xml_namespace());
    }

    fn swap(&mut this) {
        fwd!(this.swap());
    }
}

/// `Emitter.size(list)` for `ObjectList`, `IntList` and `ByteList`.
pub fn size<T>(list: Option<&[T]>) -> i32 {
    list.map_or(0, |l| l.len() as i32)
}

/// `new Emitter(impl)` with no overrides.
pub struct PlainEmitter {
    pub base: EmitterBase,
}

impl PlainEmitter {
    pub fn new(impl_: Option<EmitterRef>) -> PlainEmitter {
        PlainEmitter { base: EmitterBase::new(impl_) }
    }
}

impl Emitter for PlainEmitter {
    fn emitter_base(&self) -> &EmitterBase {
        &self.base
    }

    fn emitter_base_mut(&mut self) -> &mut EmitterBase {
        &mut self.base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A leaf that records the calls reaching it.
    struct Log(EmitterBase, Vec<String>);

    impl Emitter for Log {
        fn emitter_base(&self) -> &EmitterBase {
            &self.0
        }

        fn emitter_base_mut(&mut self) -> &mut EmitterBase {
            &mut self.0
        }

        fn start_method(&mut self, name: &str, param_count: i32, local_count: i32, temp_count: i32, needs_activation: bool, needs_arguments: i32) {
            self.1.push(format!("start {} {} {} {} {} {}", name, param_count, local_count, temp_count, needs_activation, needs_arguments));
        }

        fn store_register_name(&mut self, reg: i32, type_id: i32, var_name: &str) {
            self.1.push(format!("store {} {} '{}'", reg, type_id, var_name));
        }

        fn allocate_temp(&mut self) -> i32 {
            7
        }
    }

    /// A middle link that overrides one method and calls the base behaviour from it.
    struct Mid(EmitterBase, i32);

    impl Emitter for Mid {
        fn emitter_base(&self) -> &EmitterBase {
            &self.0
        }

        fn emitter_base_mut(&mut self) -> &mut EmitterBase {
            &mut self.0
        }

        fn allocate_temp(&mut self) -> i32 {
            self.1 += 1;
            base::allocate_temp(self) + 100
        }
    }

    #[test]
    fn chain_forwards_and_overloads_dispatch_virtually() {
        let leaf = Rc::new(RefCell::new(Log(EmitterBase::new(None), Vec::new())));
        let mid = Rc::new(RefCell::new(Mid(EmitterBase::new(Some(leaf.clone())), 0)));
        let mut top = PlainEmitter::new(Some(mid.clone()));

        top.start_method_empty();
        top.start_method_counts("f", 2, 3);
        top.store_register(4, 5);
        assert!(top.doing_method());
        assert!(mid.borrow_mut().doing_method());
        assert_eq!(top.allocate_temp(), 107);
        assert_eq!(mid.borrow().1, 1);
        assert_eq!(top.get_method_info("x"), -1);
        assert_eq!(top.define_var("v", 0, 0), -1);

        top.set_position(3, 4, 5);
        top.set_origin("o.as");
        let (mut origin, mut ln, mut col) = (None, 0, 0);
        assert_eq!(top.get_origin_and_position(&mut origin, &mut ln, &mut col), 5);
        assert_eq!((origin.as_deref(), ln, col), (Some("o.as"), 3, 4));
        assert_eq!(top.base.ln_num, 0);
        assert_eq!(leaf.borrow().0.ln_num, 3);

        assert_eq!(leaf.borrow().1, ["start  0 0 0 false 0", "start f 2 3 0 false 0", "store 4 5 ''"]);
        assert_eq!(size::<i32>(None), 0);
        assert_eq!(size(Some(&[1u8, 2][..])), 2);
    }
}
