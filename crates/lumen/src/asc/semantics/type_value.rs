//! Port of `macromedia/asc/semantics/TypeValue.java`, plus `FlowAnalyzer.inheritClassSlotsStatic`, which
//! `instantiateParameterizedType` needs. A `TypeValue` is an `ObjectValue` of kind `Type`.

use std::cell::{Ref, RefMut};
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;

use crate::asc::emit::class_builder::new_class_builder;
use crate::asc::emit::instance_builder::new_instance_builder;
use crate::asc::emit::runtime_constants::TYPE_object;
use crate::asc::semantics::builder::Builder;
use crate::asc::semantics::interface_walker::InterfaceWalker;
use crate::asc::semantics::object_value_walker::ObjectValueWalker;
use crate::asc::semantics::object_value::{ObjectKind, ObjectValue};
use crate::asc::semantics::object_value_wrapper::new_object_value_wrapper;
use crate::asc::semantics::q_name::QName;
use crate::asc::semantics::type_info::TypeInfo;
use crate::asc::util::context::{Context, NS_PROTECTED, NS_STATIC_PROTECTED};

pub struct TypeData {
    pub prototype: Option<ObjectValue>,
    pub baseclass: Option<TypeValue>,
    pub is_parameterized: bool,
    pub indexed_type: Option<TypeValue>,
    /// The `QName name` field (null while the constructor's `super()` runs the builder).
    pub name: Option<QName>,
    pub type_id: i32,
    /// Whether the type has been compiled or imported for real, rather than being a placeholder.
    pub resolved: bool,
    default_typeinfo: Option<TypeInfo>,
    explicit_nullable_typeinfo: Option<TypeInfo>,
    explicit_nonnullable_typeinfo: Option<TypeInfo>,
    pub is_nullable: bool,
    parameterized_types: Option<HashMap<String, TypeValue>>,
}

fn raw_type_data() -> TypeData {
    // Java field initialisers have not run yet while `super()` builds the object.
    TypeData {
        prototype: None,
        baseclass: None,
        is_parameterized: false,
        indexed_type: None,
        name: None,
        type_id: 0,
        resolved: false,
        default_typeinfo: None,
        explicit_nullable_typeinfo: None,
        explicit_nonnullable_typeinfo: None,
        is_nullable: false,
        parameterized_types: None,
    }
}

/// A `TypeValue` reference; derefs to the `ObjectValue` it is.
#[derive(Clone, PartialEq, Eq)]
pub struct TypeValue(ObjectValue);

impl Hash for TypeValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl Deref for TypeValue {
    type Target = ObjectValue;
    fn deref(&self) -> &ObjectValue {
        &self.0
    }
}

impl From<TypeValue> for ObjectValue {
    fn from(t: TypeValue) -> ObjectValue {
        t.0
    }
}

impl TypeValue {
    pub fn init() {}

    pub fn clear() {}

    /// The cast `(TypeValue)ob`, when `ob` is one.
    pub fn from_object(ob: &ObjectValue) -> Option<TypeValue> {
        if ob.is_type_value() {
            Some(TypeValue(ob.clone()))
        } else {
            None
        }
    }

    pub fn as_object(&self) -> &ObjectValue {
        &self.0
    }

    pub fn into_object(self) -> ObjectValue {
        self.0
    }

    pub fn ptr_eq(&self, other: &TypeValue) -> bool {
        self.0.ptr_eq(&other.0)
    }

    pub fn td(&self) -> Ref<'_, TypeData> {
        Ref::map(self.0.data(), |d| match &d.kind {
            ObjectKind::Type(t) => &**t,
            _ => unreachable!(),
        })
    }

    pub fn td_mut(&self) -> RefMut<'_, TypeData> {
        RefMut::map(self.0.data_mut(), |d| match &mut d.kind {
            ObjectKind::Type(t) => &mut **t,
            _ => unreachable!(),
        })
    }

    /// Creates the `TypeValue` for a name, or re-initialises the existing one so that references to it stay valid.
    pub fn define_type_value(cx: &Context, builder: Builder, name: &QName, type_id: i32) -> TypeValue {
        let fullname = name.to_string();
        match cx.user_defined(&fullname) {
            None => {
                let t = TypeValue::new(cx, builder, name, type_id);
                cx.set_user_defined(&fullname, t.clone());
                t.td_mut().resolved = true;
                t
            }
            Some(t) => {
                // Set early: clear_instance runs the builder, which can call back into this type.
                t.td_mut().resolved = true;
                t.clear_instance(cx, builder, None, &fullname, false);
                let (a, b, c) = {
                    let mut td = t.td_mut();
                    td.type_id = type_id;
                    td.name = Some(name.clone());
                    td.baseclass = None;
                    (td.default_typeinfo.clone(), td.explicit_nonnullable_typeinfo.clone(), td.explicit_nullable_typeinfo.clone())
                };
                t.set_type(None);
                for ti in [a, b, c].into_iter().flatten() {
                    ti.clear_instance();
                }
                t
            }
        }
    }

    /// The `TypeValue` for a name, creating an unresolved placeholder (filled in when the type is defined) if needed.
    pub fn get_type_value(cx: &Context, name: &QName) -> TypeValue {
        let fullname = name.to_string();
        if let Some(t) = cx.user_defined(&fullname) {
            return t;
        }
        let mut type_: Option<TypeValue> = None;
        if name.name() == "Vector" && name.is_parameterized_name() {
            let indexed_name = name.type_params().expect("ClassCastException")[0].clone();
            let indexed_type = if cx.is_builtin(&indexed_name.to_string()) {
                cx.builtin(&indexed_name.to_string()).expect("NullPointerException")
            } else {
                TypeValue::get_type_value(cx, &indexed_name)
            };
            type_ = indexed_type.get_parameterized_type(&fullname);
            if type_.is_none() {
                let t = TypeValue::new_placeholder(cx, name);
                t.td_mut().indexed_type = Some(indexed_type.clone());
                indexed_type.add_parameterized_type(&fullname, t.clone());
                type_ = Some(t);
            }
        }
        let type_ = match type_ {
            Some(t) => t,
            None => TypeValue::new_placeholder(cx, name),
        };
        cx.set_user_defined(&fullname, type_.clone());
        type_
    }

    /// `new TypeValue(cx, builder, name, type_id)`.
    pub fn new(cx: &Context, builder: Builder, name: &QName, type_id: i32) -> TypeValue {
        let ob = ObjectValue::from_kind(ObjectKind::Type(Box::new(raw_type_data())));
        ob.clear_instance(cx, builder, None, "", false);
        let t = TypeValue(ob);
        {
            let mut td = t.td_mut();
            td.default_typeinfo = None;
            td.explicit_nullable_typeinfo = None;
            td.explicit_nonnullable_typeinfo = None;
            td.is_nullable = true;
            td.type_id = type_id;
            td.prototype = None;
            td.name = Some(name.clone());
            td.baseclass = None;
            td.resolved = false;
        }
        t.set_type(None);
        t.0.set_name(&name.to_string());
        t
    }

    /// The private `TypeValue(cx, name)`: an unbuilt placeholder.
    fn new_placeholder(_cx: &Context, name: &QName) -> TypeValue {
        let ob = ObjectValue::from_kind(ObjectKind::Type(Box::new(raw_type_data())));
        let t = TypeValue(ob);
        {
            let mut td = t.td_mut();
            td.is_nullable = true;
            td.name = Some(name.clone());
            td.resolved = false;
        }
        t
    }

    pub fn prototype(&self) -> Option<ObjectValue> {
        self.td().prototype.clone()
    }

    pub fn set_prototype(&self, p: Option<ObjectValue>) {
        self.td_mut().prototype = p;
    }

    pub fn baseclass(&self) -> Option<TypeValue> {
        self.td().baseclass.clone()
    }

    pub fn set_baseclass(&self, b: Option<TypeValue>) {
        self.td_mut().baseclass = b;
    }

    pub fn is_parameterized(&self) -> bool {
        self.td().is_parameterized
    }

    pub fn set_is_parameterized(&self, v: bool) {
        self.td_mut().is_parameterized = v;
    }

    pub fn indexed_type(&self) -> Option<TypeValue> {
        self.td().indexed_type.clone()
    }

    pub fn set_indexed_type(&self, t: Option<TypeValue>) {
        self.td_mut().indexed_type = t;
    }

    /// The `QName name` field.
    pub fn name(&self) -> QName {
        self.td().name.clone().expect("NullPointerException")
    }

    pub fn name_opt(&self) -> Option<QName> {
        self.td().name.clone()
    }

    pub fn set_qname(&self, name: Option<QName>) {
        self.td_mut().name = name;
    }

    pub fn type_id(&self) -> i32 {
        self.td().type_id
    }

    pub fn set_type_id(&self, v: i32) {
        self.td_mut().type_id = v;
    }

    pub fn resolved(&self) -> bool {
        self.td().resolved
    }

    pub fn set_resolved(&self, v: bool) {
        self.td_mut().resolved = v;
    }

    pub fn is_nullable(&self) -> bool {
        self.td().is_nullable
    }

    pub fn set_is_nullable(&self, v: bool) {
        self.td_mut().is_nullable = v;
    }

    pub fn get_type_id(&self) -> i32 {
        self.type_id()
    }

    pub fn includes(&self, cx: &Context, type_: Option<&TypeValue>) -> bool {
        if self.ptr_eq(&cx.no_type()) {
            return true;
        }
        if !self.resolved() {
            self.resolve(cx);
        }
        if !self.is_interface() {
            let mut t = type_.cloned();
            while let Some(ty) = t {
                if self.ptr_eq(&ty) {
                    return true;
                }
                t = ty.baseclass();
            }
        } else if let Some(t) = type_ {
            let mut walker = InterfaceWalker::new(t.as_object());
            while walker.has_next() {
                let next = walker.next();
                if next.and_then(|n| n.type_()).and_then(|ti| ti.get_type_value()).map_or(false, |tv| tv.ptr_eq(self)) {
                    return true;
                }
            }
        }
        false
    }

    pub fn build(&self, _cx: &Context, _ob: &ObjectValue) {}

    pub fn get_printable_name(&self) -> String {
        self.name().name().to_string()
    }

    pub fn is_numeric(&self, cx: &Context) -> bool {
        self.ptr_eq(&cx.int_type())
            || self.ptr_eq(&cx.uint_type())
            || self.ptr_eq(&cx.double_type())
            || self.ptr_eq(&cx.number_type())
            || (cx.statics.borrow().es4_numerics && cx.decimal_type().map_or(false, |d| self.ptr_eq(&d)))
    }

    pub fn get_default_type_info(&self) -> TypeInfo {
        if let Some(ti) = self.td().default_typeinfo.clone() {
            return ti;
        }
        let nullable = self.is_nullable();
        let ti = TypeInfo::new(Some(self), nullable, true);
        self.td_mut().default_typeinfo = Some(ti.clone());
        ti
    }

    pub fn get_type_info(&self, nullable: bool) -> TypeInfo {
        let existing = {
            let td = self.td();
            if nullable {
                td.explicit_nullable_typeinfo.clone()
            } else {
                td.explicit_nonnullable_typeinfo.clone()
            }
        };
        let ti = match existing {
            Some(ti) => ti,
            None => {
                let ti = TypeInfo::new(Some(self), nullable, false);
                let mut td = self.td_mut();
                if nullable {
                    td.explicit_nullable_typeinfo = Some(ti.clone());
                } else {
                    td.explicit_nonnullable_typeinfo = Some(ti.clone());
                }
                ti
            }
        };
        let prototype = self.prototype();
        let same = match (ti.get_prototype(), &prototype) {
            (None, None) => true,
            (Some(a), Some(b)) => a.ptr_eq(b),
            _ => false,
        };
        if same {
            let prototype = prototype.expect("NullPointerException");
            let nullable_proto = new_object_value_wrapper(prototype.clone());
            nullable_proto.set_builder(prototype.builder());
            nullable_proto.set_type(Some(ti.clone()));
            ti.set_prototype(Some(nullable_proto));
        }
        ti
    }

    pub fn add_parameterized_type(&self, name: &str, type_value: TypeValue) {
        self.td_mut().parameterized_types.get_or_insert_with(HashMap::new).insert(name.to_string(), type_value);
    }

    pub fn get_parameterized_type(&self, name: &str) -> Option<TypeValue> {
        self.td().parameterized_types.as_ref().and_then(|m| m.get(name).cloned())
    }

    /// Copies what an instantiation (e.g. `Vector.<int>`) inherits from its generic type.
    pub fn copy_instantiation_data(&self, uninstantiated_type: &TypeValue) {
        let is_final = uninstantiated_type.builder().expect("NullPointerException").is_final();
        self.builder().expect("NullPointerException").set_is_final(is_final);
    }

    /// `copyType()`, the copy Flex's symbol table makes (the prototype is copied too).
    pub fn copy_type(&self) -> TypeValue {
        let ob = ObjectValue::copy_data(&self.0, ObjectKind::Type(Box::new(raw_type_data())));
        let t = TypeValue(ob);
        let (type_id, prototype, name, baseclass) = {
            let td = self.td();
            (td.type_id, td.prototype.clone(), td.name.clone(), td.baseclass.clone())
        };
        let proto_copy = prototype.map(|p| ObjectValue::copy_data(&p, ObjectKind::Object));
        let mut td = t.td_mut();
        td.is_nullable = true;
        td.type_id = type_id;
        td.prototype = proto_copy;
        td.name = name;
        td.baseclass = baseclass;
        drop(td);
        t
    }

    /// `if (!resolved) resolve(cx)`, for the `hasName` family's overrides.
    pub(crate) fn resolve_if_needed(ob: &ObjectValue, cx: &Context) {
        let t = TypeValue(ob.clone());
        if !t.resolved() {
            t.resolve(cx);
        }
    }

    /// Resolves a parameterized type imported from an ABC on first use.
    fn resolve(&self, cx: &Context) -> bool {
        if self.resolved() {
            return true;
        }
        if self.indexed_type().is_some() {
            let name = self.name_opt();
            if let Some(pname) = name.filter(|n| n.is_parameterized_name()) {
                Self::instantiate_parameterized_type(cx, &pname);
            }
        }
        self.resolved()
    }

    /// Sets up the instantiation `pname` (e.g. `Vector.<int>`) of a parameterized type, if not done yet.
    pub fn instantiate_parameterized_type(cx: &Context, pname: &QName) -> TypeValue {
        let cframe = TypeValue::get_type_value(cx, pname);
        if cframe.resolved() {
            return cframe;
        }
        let prot_ns = cx.get_namespace_kind(&pname.to_string(), NS_PROTECTED);
        let static_prot_ns = cx.get_namespace_kind(&pname.to_string(), NS_STATIC_PROTECTED);
        let cframe = TypeValue::define_type_value(cx, new_class_builder(pname.clone(), Some(prot_ns), Some(static_prot_ns)), pname, TYPE_object);
        cframe.set_type(Some(cx.type_type().get_default_type_info()));
        let iframe = ObjectValue::with_builder(cx, new_instance_builder(pname.clone()), Some(&cframe));
        cframe.set_prototype(Some(iframe.clone()));
        let uninstantiated_generic = cx.vector_obj_type();
        inherit_class_slots_static(&cframe, &iframe, &uninstantiated_generic, cx);
        cframe.copy_instantiation_data(&uninstantiated_generic);
        cframe
    }
}

/// `FlowAnalyzer.inheritClassSlotsStatic` (its live part; the rest is commented out in Java).
pub fn inherit_class_slots_static(cframe: &TypeValue, iframe: &ObjectValue, base_type: &TypeValue, _cx: &Context) {
    let super_type = base_type.clone();
    cframe.set_baseclass(Some(super_type.clone()));
    let baseobj = base_type.prototype().expect("NullPointerException");
    let basebui = super_type.prototype().expect("NullPointerException").builder().filter(|b| b.is_instance_builder());
    let bui = iframe.builder().expect("NullPointerException");
    let basebui = basebui.expect("NullPointerException");
    let can_early_bind = basebui.instance_data().can_early_bind;
    bui.instance_data_mut().can_early_bind = can_early_bind;
    bui.instance_data_mut().basebui = Some(basebui.clone());
    let (b_var_offset, b_method_offset, b_method_count) = {
        let b = basebui.data();
        (b.var_offset, b.method_offset, b.method_count)
    };
    {
        let mut d = bui.data_mut();
        d.var_offset = b_var_offset + baseobj.var_count();
        d.method_offset = b_method_offset + b_method_count;
    }
    let baseclass = cframe.baseclass();
    let classbui = cframe.builder().filter(|b| b.is_class_builder()).expect("NullPointerException");
    let base_cb = baseclass.and_then(|bc| bc.builder()).filter(|b| b.is_class_builder());
    classbui.class_data_mut().basebui = base_cb.clone();
    if let Some(base_cb) = &base_cb {
        let base_base = base_cb.class_data().basebui.clone();
        if let Some(bb) = base_base {
            let v = bb.data().is_intrinsic;
            base_cb.data_mut().is_intrinsic = v;
        }
    }
    iframe.add_base_obj(&baseobj);
    if !cframe.is_interface() && !base_type.is_interface() {
        let p = cframe.builder().expect("NullPointerException").class_data().protected_namespace.clone();
        let bp = base_type.builder().expect("NullPointerException").class_data().protected_namespace.clone();
        iframe.set_protected_namespaces(p, bp);
    }
}

impl fmt::Display for TypeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name_opt() {
            Some(n) => fmt::Display::fmt(&n, f),
            None => Ok(()),
        }
    }
}

impl fmt::Debug for TypeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypeValue({})", self)
    }
}
