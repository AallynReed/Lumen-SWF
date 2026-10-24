//! Port of `macromedia/asc/semantics/Builder.java`. One handle type for every builder subclass; `kind` says which, and
//! each method Java overrides dispatches on it to the subclass's version in `asc/emit/*_builder.rs`. Java's shorter
//! overloads only forward with -1 defaults, so just the full forms are here.

use std::cell::{Ref, RefCell, RefMut};
use std::fmt;
use std::rc::Rc;

use crate::asc::emit::class_builder::{self, ClassBuilderData};
use crate::asc::emit::instance_builder::{self, InstanceBuilderData};
use crate::asc::emit::runtime_constants::{get_binary_op, get_unary_op};
use crate::asc::emit::{activation_builder, global_builder, import_builder, package_builder};
use crate::asc::parser::tokens::*;
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::q_name::QName;
use crate::asc::semantics::slot::*;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::util::context::Context;
use crate::asc::util::names::Names;
use crate::asc::util::namespaces::Namespaces;

/// `Builder.removeBuilderNames`: names live on the `ObjectValue`, not the builder.
pub const REMOVE_BUILDER_NAMES: bool = true;

pub enum BuilderKind {
    Ordinary,
    Configuration,
    Activation,
    Catch { has_parameter: bool },
    Function,
    With,
    Package,
    Global { is_in_package: bool },
    Import,
    Class(ClassBuilderData),
    Instance(InstanceBuilderData),
}

pub struct BuilderData {
    object_value: Option<ObjectValue>,
    pub context_id: i32,
    pub classname: Option<QName>,
    pub pkgname: String,
    pub var_offset: i32,
    pub method_offset: i32,
    pub method_count: i32,
    pub reg_offset: i32,
    pub temp_reg: i32,
    pub is_intrinsic: bool,
    pub is_dynamic: bool,
    pub is_final: bool,
    pub local_method_count: i32,
    pub kind: BuilderKind,
}

#[derive(Clone)]
pub struct Builder(Rc<RefCell<BuilderData>>);

impl PartialEq for Builder {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for Builder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Builder({})", self.kind_name())
    }
}

impl Builder {
    pub fn from_kind(kind: BuilderKind) -> Builder {
        Builder(Rc::new(RefCell::new(BuilderData {
            object_value: None,
            context_id: 0,
            classname: None,
            pkgname: String::new(),
            var_offset: 0,
            method_offset: 0,
            method_count: 0,
            reg_offset: 0,
            temp_reg: -1,
            is_intrinsic: false,
            is_dynamic: false,
            is_final: false,
            local_method_count: 0,
            kind,
        })))
    }

    pub fn ptr_eq(&self, other: &Builder) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn data(&self) -> Ref<'_, BuilderData> {
        self.0.borrow()
    }

    pub fn data_mut(&self) -> RefMut<'_, BuilderData> {
        self.0.borrow_mut()
    }

    pub fn kind_name(&self) -> &'static str {
        match &self.0.borrow().kind {
            BuilderKind::Ordinary => "OrdinaryBuilder",
            BuilderKind::Configuration => "ConfigurationBuilder",
            BuilderKind::Activation => "ActivationBuilder",
            BuilderKind::Catch { .. } => "CatchBuilder",
            BuilderKind::Function => "FunctionBuilder",
            BuilderKind::With => "WithBuilder",
            BuilderKind::Package => "PackageBuilder",
            BuilderKind::Global { .. } => "GlobalBuilder",
            BuilderKind::Import => "ImportBuilder",
            BuilderKind::Class(_) => "ClassBuilder",
            BuilderKind::Instance(_) => "InstanceBuilder",
        }
    }

    /// `instanceof ActivationBuilder` (true for `CatchBuilder` too).
    pub fn is_activation_builder(&self) -> bool {
        matches!(self.0.borrow().kind, BuilderKind::Activation | BuilderKind::Catch { .. })
    }
    pub fn is_catch_builder(&self) -> bool {
        matches!(self.0.borrow().kind, BuilderKind::Catch { .. })
    }
    pub fn is_class_builder(&self) -> bool {
        matches!(self.0.borrow().kind, BuilderKind::Class(_))
    }
    pub fn is_instance_builder(&self) -> bool {
        matches!(self.0.borrow().kind, BuilderKind::Instance(_))
    }
    /// `instanceof PackageBuilder` (true for `GlobalBuilder` too).
    pub fn is_package_builder(&self) -> bool {
        matches!(self.0.borrow().kind, BuilderKind::Package | BuilderKind::Global { .. })
    }
    pub fn is_global_builder(&self) -> bool {
        matches!(self.0.borrow().kind, BuilderKind::Global { .. })
    }
    pub fn is_import_builder(&self) -> bool {
        matches!(self.0.borrow().kind, BuilderKind::Import)
    }
    pub fn is_function_builder(&self) -> bool {
        matches!(self.0.borrow().kind, BuilderKind::Function)
    }
    pub fn is_with_builder(&self) -> bool {
        matches!(self.0.borrow().kind, BuilderKind::With)
    }
    pub fn is_configuration_builder(&self) -> bool {
        matches!(self.0.borrow().kind, BuilderKind::Configuration)
    }
    pub fn is_ordinary_builder(&self) -> bool {
        matches!(self.0.borrow().kind, BuilderKind::Ordinary)
    }

    /// The `ClassBuilder` fields; panics like a failed cast otherwise.
    pub fn class_data(&self) -> Ref<'_, ClassBuilderData> {
        Ref::map(self.0.borrow(), |d| match &d.kind {
            BuilderKind::Class(c) => c,
            _ => panic!("ClassCastException: not a ClassBuilder"),
        })
    }

    pub fn class_data_mut(&self) -> RefMut<'_, ClassBuilderData> {
        RefMut::map(self.0.borrow_mut(), |d| match &mut d.kind {
            BuilderKind::Class(c) => c,
            _ => panic!("ClassCastException: not a ClassBuilder"),
        })
    }

    /// The `InstanceBuilder` fields; panics like a failed cast otherwise.
    pub fn instance_data(&self) -> Ref<'_, InstanceBuilderData> {
        Ref::map(self.0.borrow(), |d| match &d.kind {
            BuilderKind::Instance(c) => c,
            _ => panic!("ClassCastException: not an InstanceBuilder"),
        })
    }

    pub fn instance_data_mut(&self) -> RefMut<'_, InstanceBuilderData> {
        RefMut::map(self.0.borrow_mut(), |d| match &mut d.kind {
            BuilderKind::Instance(c) => c,
            _ => panic!("ClassCastException: not an InstanceBuilder"),
        })
    }

    /// `GlobalBuilder.is_in_package`.
    pub fn is_in_package(&self) -> bool {
        matches!(self.0.borrow().kind, BuilderKind::Global { is_in_package: true })
    }

    pub fn set_is_in_package(&self, v: bool) {
        if let BuilderKind::Global { is_in_package } = &mut self.0.borrow_mut().kind {
            *is_in_package = v;
        }
    }

    /// `CatchBuilder.hasParameter`.
    pub fn has_parameter(&self) -> bool {
        matches!(self.0.borrow().kind, BuilderKind::Catch { has_parameter: true })
    }

    pub fn set_has_parameter(&self, v: bool) {
        if let BuilderKind::Catch { has_parameter } = &mut self.0.borrow_mut().kind {
            *has_parameter = v;
        }
    }

    pub fn object_value(&self) -> Option<ObjectValue> {
        self.0.borrow().object_value.clone()
    }

    pub fn classname(&self) -> Option<QName> {
        self.0.borrow().classname.clone()
    }

    pub fn is_intrinsic(&self) -> bool {
        self.0.borrow().is_intrinsic
    }

    pub fn set_is_intrinsic(&self, v: bool) {
        self.0.borrow_mut().is_intrinsic = v;
    }

    pub fn is_dynamic(&self) -> bool {
        self.0.borrow().is_dynamic
    }

    pub fn set_is_dynamic(&self, v: bool) {
        self.0.borrow_mut().is_dynamic = v;
    }

    pub fn is_final(&self) -> bool {
        self.0.borrow().is_final
    }

    pub fn set_is_final(&self, v: bool) {
        self.0.borrow_mut().is_final = v;
    }

    /// `objectValue = ob; contextId = cx.getId();`, which every `build` starts with.
    pub(crate) fn bind(&self, cx: &Context, ob: &ObjectValue) {
        let mut d = self.0.borrow_mut();
        d.object_value = Some(ob.clone());
        d.context_id = cx.get_id();
    }

    pub fn build(&self, cx: &Context, ob: &ObjectValue) {
        enum K {
            Bind,
            Activation,
            Package,
            Global,
            Import,
            Class,
            Instance,
        }
        let k = match &self.0.borrow().kind {
            BuilderKind::Ordinary => return,
            BuilderKind::Configuration | BuilderKind::Function | BuilderKind::With => K::Bind,
            BuilderKind::Activation | BuilderKind::Catch { .. } => K::Activation,
            BuilderKind::Package => K::Package,
            BuilderKind::Global { .. } => K::Global,
            BuilderKind::Import => K::Import,
            BuilderKind::Class(_) => K::Class,
            BuilderKind::Instance(_) => K::Instance,
        };
        match k {
            K::Bind => self.bind(cx, ob),
            K::Activation => activation_builder::build(self, cx, ob),
            K::Package => package_builder::build(self, cx, ob),
            K::Global => global_builder::build(self, cx, ob),
            K::Import => import_builder::build(self, cx, ob),
            K::Class => class_builder::build(self, cx, ob),
            K::Instance => instance_builder::build(self, cx, ob),
        }
    }

    pub fn add_names(&self, names: Option<&Names>) {
        if let Some(names) = names {
            self.with_names_and_create(|n| n.put_all(names));
        }
    }

    pub fn get_names(&self) -> Option<Names> {
        self.object_value().expect("NullPointerException").get_names()
    }

    fn with_names_and_create<R>(&self, f: impl FnOnce(&mut Names) -> R) -> R {
        self.object_value().expect("NullPointerException").with_names_and_create(f)
    }

    pub fn clear_names(&self) {}

    pub fn unary_operator(&self, cx: &Context, ob: &ObjectValue, type_: Option<&TypeValue>, call_seq: i32, slot_id: i32) {
        cx.statics.borrow_mut().push_expected_slot_id(slot_id);
        let id = ob.add_method_slot(cx, type_);
        self.check_slot_index(slot_id, id);
        ob.get_slot(cx, slot_id).expect("NullPointerException").attrs(call_seq, get_unary_op(slot_id));
    }

    pub fn unary_overload(&self, cx: &Context, ob: &ObjectValue, slot_id: i32, type_: Option<&TypeValue>, opd_type: &TypeValue, call_seq: i32, over_id: i32) {
        cx.statics.borrow_mut().push_expected_slot_id(over_id);
        let id = ob.add_slot_overload(cx, slot_id, type_, opd_type);
        self.check_slot_index(over_id, id);
        ob.get_slot(cx, over_id).expect("NullPointerException").attrs(call_seq, get_unary_op(over_id));
    }

    pub fn binary_operator(&self, cx: &Context, ob: &ObjectValue, type_: Option<&TypeValue>, slot_id: i32) {
        cx.statics.borrow_mut().push_expected_slot_id(slot_id);
        let id = ob.add_method_slot(cx, type_);
        self.check_slot_index(slot_id, id);
        ob.get_slot(cx, slot_id).expect("NullPointerException").attrs(CALL_Binary, get_binary_op(slot_id));
    }

    #[allow(clippy::too_many_arguments)]
    pub fn binary_overload(&self, cx: &Context, ob: &ObjectValue, slot_id: i32, type_: Option<&TypeValue>, lhs_type: &TypeValue, rhs_type: &TypeValue, over_id: i32) {
        cx.statics.borrow_mut().push_expected_slot_id(over_id);
        let id = ob.add_slot_overload2(cx, slot_id, type_, lhs_type, rhs_type);
        self.check_slot_index(over_id, id);
        ob.get_slot(cx, over_id).expect("NullPointerException").attrs(CALL_Binary, get_binary_op(over_id));
    }

    /// `ExplicitGet(cx, ob, name, ObjectValue ns, ...)`; no subclass overrides it.
    #[allow(clippy::too_many_arguments)]
    pub fn explicit_get(&self, cx: &Context, ob: &ObjectValue, name: &str, ns: &ObjectValue, type_: Option<&TypeValue>, _is_final: bool, _is_override: bool, expected_id: i32, method_id: i32, _var_id: i32) -> i32 {
        let slot_id = ob.add_method_slot(cx, type_);
        self.check_slot_index(expected_id, slot_id);
        ob.define_name(cx, GET_TOKEN, name, ns, slot_id);
        ob.get_slot(cx, slot_id).expect("NullPointerException").attrs(CALL_ThisMethod, method_id);
        slot_id
    }

    /// `ExplicitGet(cx, ob, name, Namespaces namespaces, ...)`.
    #[allow(clippy::too_many_arguments)]
    pub fn explicit_get_nss(&self, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
        enum K {
            Base,
            Class,
            Instance,
            Import,
        }
        let k = match &self.0.borrow().kind {
            BuilderKind::Class(_) => K::Class,
            BuilderKind::Instance(_) => K::Instance,
            BuilderKind::Import => K::Import,
            _ => K::Base,
        };
        match k {
            K::Base => self.base_explicit_get_nss(cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id),
            K::Class => class_builder::explicit_get_nss(self, cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id),
            K::Instance => instance_builder::explicit_get_nss(self, cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id),
            K::Import => import_builder::explicit_get_nss(self, cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn base_explicit_get_nss(&self, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, _var_id: i32) -> i32 {
        let slot_id = ob.add_method_slot(cx, type_);
        self.check_slot_index(expected_id, slot_id);
        ob.define_names(cx, GET_TOKEN, name, namespaces, slot_id);
        let slot = ob.get_slot(cx, slot_id).expect("NullPointerException");
        slot.attrs(CALL_ThisMethod, method_id);
        slot.set_final(is_final);
        slot.set_override(is_override);
        slot_id
    }

    /// `ExplicitSet(cx, ob, name, ObjectValue ns, ...)`.
    #[allow(clippy::too_many_arguments)]
    pub fn explicit_set(&self, cx: &Context, ob: &ObjectValue, name: &str, ns: &ObjectValue, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
        if self.is_class_builder() {
            return class_builder::explicit_set(self, cx, ob, name, ns, type_, is_final, is_override, expected_id, method_id, var_id);
        }
        if self.is_instance_builder() {
            return instance_builder::explicit_set(self, cx, ob, name, ns, type_, is_final, is_override, expected_id, method_id, var_id);
        }
        let slot_id = ob.add_method_slot(cx, type_);
        self.check_slot_index(expected_id, slot_id);
        ob.define_name(cx, SET_TOKEN, name, ns, slot_id);
        let slot = ob.get_slot(cx, slot_id).expect("NullPointerException");
        slot.attrs(CALL_ThisMethod, method_id);
        slot.set_final(is_final);
        slot.set_override(is_override);
        slot_id
    }

    /// `ExplicitSet(cx, ob, name, Namespaces namespaces, ...)`.
    #[allow(clippy::too_many_arguments)]
    pub fn explicit_set_nss(&self, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
        if self.is_class_builder() {
            return class_builder::explicit_set_nss(self, cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id);
        }
        if self.is_instance_builder() {
            return instance_builder::explicit_set_nss(self, cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id);
        }
        if self.is_import_builder() {
            return import_builder::explicit_set_nss(self, cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id);
        }
        self.base_explicit_set_nss(cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn base_explicit_set_nss(&self, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, _var_id: i32) -> i32 {
        let slot_id = ob.add_method_slot(cx, type_);
        self.check_slot_index(expected_id, slot_id);
        ob.define_names(cx, SET_TOKEN, name, namespaces, slot_id);
        let slot = ob.get_slot(cx, slot_id).expect("NullPointerException");
        slot.attrs(CALL_ThisMethod, method_id);
        slot.set_final(is_final);
        slot.set_override(is_override);
        slot_id
    }

    /// `ExplicitVar(cx, ob, name, ObjectValue ns, ...)`.
    #[allow(clippy::too_many_arguments)]
    pub fn explicit_var(&self, cx: &Context, ob: &ObjectValue, name: &str, ns: &ObjectValue, type_: Option<&TypeValue>, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
        if self.is_class_builder() {
            return class_builder::explicit_var(self, cx, ob, name, ns, type_, expected_id, method_id, var_id);
        }
        if self.is_instance_builder() {
            return instance_builder::explicit_var(self, cx, ob, name, ns, type_, expected_id, method_id, var_id);
        }
        let slot_id = ob.add_variable_slot(cx, type_, var_id);
        ob.get_slot(cx, slot_id).expect("NullPointerException").add_type(type_.expect("NullPointerException").get_default_type_info());
        self.check_slot_index(expected_id, slot_id);
        ob.define_name(cx, GET_TOKEN, name, ns, slot_id);
        ob.define_name(cx, SET_TOKEN, name, ns, slot_id);
        ob.get_slot(cx, slot_id).expect("NullPointerException").attrs(CALL_ThisMethod, method_id);
        if var_id >= 0 {
            self.name(cx, VAR_TOKEN, name, ns);
        }
        slot_id
    }

    /// `ExplicitVar(cx, ob, name, Namespaces namespaces, ...)`.
    #[allow(clippy::too_many_arguments)]
    pub fn explicit_var_nss(&self, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
        if self.is_class_builder() {
            return class_builder::explicit_var_nss(self, cx, ob, name, namespaces, type_, expected_id, method_id, var_id);
        }
        if self.is_instance_builder() {
            return instance_builder::explicit_var_nss(self, cx, ob, name, namespaces, type_, expected_id, method_id, var_id);
        }
        if self.is_import_builder() {
            return import_builder::explicit_var_nss(self, cx, ob, name, namespaces, type_, expected_id, method_id, var_id);
        }
        self.base_explicit_var_nss(cx, ob, name, namespaces, type_, expected_id, method_id, var_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn base_explicit_var_nss(&self, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
        let slot_id = ob.add_variable_slot(cx, type_, var_id);
        ob.get_slot(cx, slot_id).expect("NullPointerException").add_type(type_.expect("NullPointerException").get_default_type_info());
        self.check_slot_index(expected_id, slot_id);
        ob.define_names(cx, GET_TOKEN, name, namespaces, slot_id);
        ob.define_names(cx, SET_TOKEN, name, namespaces, slot_id);
        ob.get_slot(cx, slot_id).expect("NullPointerException").attrs(CALL_ThisMethod, method_id);
        if var_id >= 0 {
            for n in namespaces.to_vec() {
                self.name(cx, VAR_TOKEN, name, &n);
            }
        }
        slot_id
    }

    #[allow(clippy::too_many_arguments)]
    pub fn implicit_var(&self, cx: &Context, ob: &ObjectValue, name: &str, ns: &ObjectValue, type_: Option<&TypeValue>, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
        cx.statics.borrow_mut().push_expected_slot_id(expected_id);
        let slot_id = ob.add_variable_slot(cx, type_, var_id);
        self.check_slot_index(expected_id, slot_id);
        ob.define_name(cx, GET_TOKEN, name, ns, slot_id);
        ob.define_name(cx, SET_TOKEN, name, ns, slot_id);
        ob.get_slot(cx, slot_id).expect("NullPointerException").attrs(CALL_ThisMethod, method_id);
        slot_id
    }

    /// `ImplicitCall`; the `ClassBuilder` and `InstanceBuilder` overrides only call this.
    #[allow(clippy::too_many_arguments)]
    pub fn implicit_call(&self, cx: &Context, ob: &ObjectValue, slot_id: i32, type_: Option<&TypeValue>, call_seq: i32, method_id: i32, expected_id: i32) {
        cx.statics.borrow_mut().push_expected_slot_id(expected_id);
        let implied_id = ob.add_slot_implicit(cx, slot_id, EMPTY_TOKEN, type_);
        self.check_slot_index(expected_id, implied_id);
        let s = ob.get_slot(cx, implied_id).expect("NullPointerException");
        s.attrs(call_seq, method_id);
        s.set_getter(false);
    }

    /// `ExplicitCall(cx, ob, name, ObjectValue ns, ...)`.
    #[allow(clippy::too_many_arguments)]
    pub fn explicit_call(&self, cx: &Context, ob: &ObjectValue, name: &str, ns: &ObjectValue, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
        if self.is_instance_builder() {
            return instance_builder::explicit_call(self, cx, ob, name, ns, type_, is_final, is_override, expected_id, method_id, var_id);
        }
        let function_type = if cx.use_static_semantics() { Some(cx.function_type()) } else { type_.cloned() };
        let slot_id = ob.add_method_slot(cx, function_type.as_ref());
        self.check_slot_index(expected_id, slot_id);
        ob.define_name(cx, EMPTY_TOKEN, name, ns, slot_id);
        let slot = ob.get_slot(cx, slot_id).expect("NullPointerException");
        slot.attrs(CALL_ThisMethod, method_id);
        slot.set_final(is_final);
        slot.set_override(is_override);
        slot.set_getter(false);
        self.name(cx, EMPTY_TOKEN, name, ns);
        slot_id
    }

    /// `ExplicitCall(cx, ob, name, Namespaces namespaces, ...)`.
    #[allow(clippy::too_many_arguments)]
    pub fn explicit_call_nss(&self, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
        enum K {
            Base,
            Activation,
            Package,
            Global,
            Class,
            Instance,
            Import,
        }
        let k = match &self.0.borrow().kind {
            BuilderKind::Activation | BuilderKind::Catch { .. } => K::Activation,
            BuilderKind::Package => K::Package,
            BuilderKind::Global { .. } => K::Global,
            BuilderKind::Class(_) => K::Class,
            BuilderKind::Instance(_) => K::Instance,
            BuilderKind::Import => K::Import,
            _ => K::Base,
        };
        match k {
            K::Base => self.base_explicit_call_nss(cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id),
            K::Activation => activation_builder::explicit_call_nss(self, cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id),
            K::Package => package_builder::explicit_call_nss(self, cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id),
            K::Global => global_builder::explicit_call_nss(self, cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id),
            K::Class => class_builder::explicit_call_nss(self, cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id),
            K::Instance => instance_builder::explicit_call_nss(self, cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id),
            K::Import => import_builder::explicit_call_nss(self, cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn base_explicit_call_nss(&self, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, _var_id: i32) -> i32 {
        let function_type = if cx.use_static_semantics() { Some(cx.function_type()) } else { type_.cloned() };
        let slot_id = ob.add_method_slot(cx, function_type.as_ref());
        self.check_slot_index(expected_id, slot_id);
        ob.define_names(cx, GET_TOKEN, name, namespaces, slot_id);
        let slot = ob.get_slot(cx, slot_id).expect("NullPointerException");
        slot.attrs(CALL_ThisMethod, method_id);
        slot.set_final(is_final);
        slot.set_override(is_override);
        slot.set_getter(false);
        if method_id >= 0 {
            for n in namespaces.to_vec() {
                self.name(cx, EMPTY_TOKEN, name, &n);
            }
        }
        slot_id
    }

    #[allow(clippy::too_many_arguments)]
    pub fn implicit_construct(&self, cx: &Context, ob: &ObjectValue, slot_id: i32, type_: Option<&TypeValue>, call_seq: i32, method_id: i32, expected_id: i32) {
        let implied_id = ob.add_slot_implicit(cx, slot_id, NEW_TOKEN, type_);
        self.check_slot_index(expected_id, implied_id);
        let s = ob.get_slot(cx, implied_id).expect("NullPointerException");
        s.attrs(call_seq, method_id);
        s.set_getter(false);
    }

    /// `Variable(cx, ob)`: allocates a variable index.
    pub fn variable(&self, cx: &Context, ob: &ObjectValue) -> i32 {
        let (overridden, intrinsic_result) = match &self.0.borrow().kind {
            BuilderKind::Package | BuilderKind::Global { .. } | BuilderKind::Import | BuilderKind::Class(_) => (true, -1),
            BuilderKind::Instance(_) => (true, 0),
            _ => (false, 0),
        };
        if overridden && self.is_intrinsic() {
            return intrinsic_result;
        }
        ob.add_var(cx)
    }

    /// `Method(cx, ob, name, namespaces, is_intrinsic)`: allocates a method id.
    pub fn method(&self, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, is_intrinsic: bool) -> i32 {
        enum K {
            Base,
            Activation,
            Import,
            Class,
            Instance,
        }
        let k = match &self.0.borrow().kind {
            BuilderKind::Activation | BuilderKind::Catch { .. } => K::Activation,
            BuilderKind::Import => K::Import,
            BuilderKind::Class(_) => K::Class,
            BuilderKind::Instance(_) => K::Instance,
            _ => K::Base,
        };
        match k {
            K::Activation | K::Class => {
                if self.is_intrinsic() || is_intrinsic {
                    return -1;
                }
                self.get_method_id(cx, name, namespaces)
            }
            K::Import => {
                if self.is_intrinsic() {
                    return -1;
                }
                0
            }
            K::Instance => instance_builder::method(self, cx, ob, name, namespaces, is_intrinsic),
            K::Base => {
                if self.is_intrinsic() || is_intrinsic {
                    return -1;
                }
                let emitter = cx.get_emitter().expect("NullPointerException");
                let r = emitter.borrow_mut().get_method_id(name, namespaces);
                r
            }
        }
    }

    /// `PackageBuilder.Method(cx, ob, name, namespaces)` and its `GlobalBuilder` override (both get the same id).
    pub fn package_method(&self, cx: &Context, _ob: &ObjectValue, name: &str, namespaces: &Namespaces) -> i32 {
        assert!(self.is_package_builder(), "ClassCastException: not a PackageBuilder");
        self.get_method_id(cx, name, namespaces)
    }

    /// Translates a method name into an index of the local method table.
    pub fn get_method_id(&self, cx: &Context, name: &str, namespaces: &Namespaces) -> i32 {
        let ov = self.object_value().expect("NullPointerException");
        if ov.with_names(|n| n.is_some()) {
            for ns in namespaces.to_vec() {
                let slot_id = ov.get_slot_index(cx, GET_TOKEN, name, Some(&ns));
                if slot_id > 0 {
                    let s = ov.get_slot(cx, slot_id).expect("NullPointerException");
                    if s.get_method_id() != -1 && s.declared_by().map_or(false, |d| d.ptr_eq(&ov)) {
                        return s.get_method_id();
                    }
                }
            }
        }
        let mut d = self.0.borrow_mut();
        d.local_method_count += 1;
        d.local_method_count
    }

    /// `Name(cx, kind, name, qualifier)`: with names on the object, only get and set names are recorded.
    pub fn name(&self, _cx: &Context, kind: i32, name: &str, qualifier: &ObjectValue) {
        if self.is_import_builder() {
            return;
        }
        let ty = Names::get_type_from_kind(kind);
        if REMOVE_BUILDER_NAMES && (ty == Names::METHOD_NAMES || ty == Names::VAR_NAMES) {
            return;
        }
        self.with_names_and_create(|n| n.put_mask(name, qualifier, ty));
    }

    /// `CHECK_SLOT_INDEX`: an `assert(false)` on mismatch, which a release JVM does not run.
    pub fn check_slot_index(&self, _a: i32, _b: i32) {}

    pub fn inherit_call(&self, cx: &Context, ob: &ObjectValue, name: &str, ns: &ObjectValue, explicit_slot: Slot, implicit_slot: Slot) {
        let id = explicit_slot.id();
        ob.add_slot(explicit_slot);
        ob.add_slot(implicit_slot);
        ob.define_name(cx, GET_TOKEN, name, ns, id);
        self.name(cx, GET_TOKEN, name, ns);
        self.name(cx, EMPTY_TOKEN, name, ns);
    }

    pub fn inherit_get(&self, cx: &Context, ob: &ObjectValue, name: &str, ns: &ObjectValue, slot: Slot) {
        let id = slot.id();
        ob.add_slot(slot);
        ob.define_name(cx, GET_TOKEN, name, ns, id);
        self.name(cx, GET_TOKEN, name, ns);
    }

    pub fn inherit_set(&self, cx: &Context, ob: &ObjectValue, name: &str, ns: &ObjectValue, slot: Slot) {
        let id = slot.id();
        ob.add_slot(slot);
        ob.define_name(cx, SET_TOKEN, name, ns, id);
        self.name(cx, SET_TOKEN, name, ns);
    }

    pub fn inherit_var(&self, cx: &Context, ob: &ObjectValue, name: &str, ns: &ObjectValue, slot: Slot) {
        let id = slot.id();
        let var_index = slot.get_var_index();
        ob.add_slot(slot);
        ob.define_name(cx, GET_TOKEN, name, ns, id);
        ob.define_name(cx, SET_TOKEN, name, ns, id);
        if var_index >= 0 {
            self.name(cx, VAR_TOKEN, name, ns);
        }
    }

    pub fn has_register_offset(&self) -> bool {
        !matches!(self.0.borrow().kind, BuilderKind::Catch { .. } | BuilderKind::With)
    }
}
