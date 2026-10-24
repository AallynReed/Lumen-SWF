//! Port of `macromedia/asc/semantics/ObjectValue.java`. One struct for ObjectValue and its subclasses (`TypeValue`,
//! `NamespaceValue`, `UnresolvedNamespace`, `ObjectValueWrapper`), whose data sits in `kind`; the methods those
//! subclasses override dispatch on it.

use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::asc::parser::node::NodeRef;
use crate::asc::parser::tokens::*;
use crate::asc::semantics::builder::Builder;
use crate::asc::semantics::method_slot::new_method_slot;
use crate::asc::semantics::namespace_value::{new_namespace_value, NamespaceData};
use crate::asc::semantics::slot::Slot;
use crate::asc::semantics::type_info::TypeInfo;
use crate::asc::semantics::type_value::{TypeData, TypeValue};
use crate::asc::semantics::value::*;
use crate::asc::semantics::variable_slot::new_variable_slot;
use crate::asc::util::context::{Context, NS_PRIVATE, NS_PUBLIC};
use crate::asc::util::jhash::java_compare_to;
use crate::asc::util::names::Names;
use crate::asc::util::namespaces::Namespaces;
use crate::asc::util::number_usage::NumberUsage;
use crate::asc::util::qualifiers::Qualifiers;
use crate::asc::util::slots::Slots;

pub enum ObjectKind {
    Object,
    /// `NamespaceValue`, and `UnresolvedNamespace` when `NamespaceData::unresolved` is set.
    Namespace(NamespaceData),
    /// `ObjectValueWrapper`.
    Wrapper(ObjectValue),
    Type(Box<TypeData>),
}

pub struct ObjectValueData {
    /// `Value.flags`.
    pub flags: i32,
    pub builder: Option<Builder>,
    pub type_: Option<TypeInfo>,
    value: String,
    number_usage: Option<NumberUsage>,
    names: Option<Names>,
    pub slots: Option<Slots>,
    /// `_proto_`.
    pub proto: Option<ObjectValue>,
    pub name: String,
    base_objs: Option<Vec<ObjectValue>>,
    protected_ns: Option<ObjectValue>,
    base_protected_ns: Option<ObjectValue>,
    slot_ids: Option<SlotIdCache>,
    pub method_info: i32,
    pub var_count: i32,
    pub activation: Option<ObjectValue>,
    deferred_class_map: Option<HashMap<TypeValue, NodeRef>>,
    init_only_view: bool,
    pub kind: ObjectKind,
}

/// A Java `ObjectValue` reference: clones share the object, `==`/`equals` is identity.
#[derive(Clone)]
pub struct ObjectValue(pub(crate) Rc<RefCell<ObjectValueData>>);

impl PartialEq for ObjectValue {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ObjectValue {}

/// Hashes the identity; Java's identity hash order is not reproducible, so only use this where order cannot leak.
impl Hash for ObjectValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.addr().hash(state);
    }
}

pub(crate) enum Disp {
    Plain,
    Namespace,
    Type,
    Wrapper(ObjectValue),
}

struct Statics {
    undefined_value: ObjectValue,
    null_value: ObjectValue,
    enumerable_namespace: ObjectValue,
    label_namespace: ObjectValue,
    loop_label_namespace: ObjectValue,
    object_prototype: ObjectValue,
    internal_namespace: ObjectValue,
    intrinsic_attribute: ObjectValue,
    intrinsic_namespace: ObjectValue,
    static_attribute: ObjectValue,
    dynamic_attribute: ObjectValue,
    final_attribute: ObjectValue,
    virtual_attribute: ObjectValue,
    override_attribute: ObjectValue,
    native_attribute: ObjectValue,
}

thread_local! {
    static STATICS: RefCell<Option<Rc<Statics>>> = const { RefCell::new(None) };
}

fn statics() -> Rc<Statics> {
    ObjectValue::init();
    STATICS.with(|s| s.borrow().clone().unwrap())
}

/// `ObjectValue.ObjectValueCompare.compare`: reverse name order, then namespace kind.
pub fn object_value_compare(o1: &ObjectValue, o2: &ObjectValue) -> i32 {
    let mut result = java_compare_to(&o2.name_ref(), &o1.name_ref());
    if result == 0 {
        result += o2.get_namespace_kind() as i32 - o1.get_namespace_kind() as i32;
    }
    result
}

fn new_data(kind: ObjectKind) -> ObjectValueData {
    ObjectValueData {
        flags: 0,
        builder: None,
        type_: None,
        value: String::new(),
        number_usage: None,
        names: None,
        slots: None,
        proto: None,
        name: String::new(),
        base_objs: None,
        protected_ns: None,
        base_protected_ns: None,
        slot_ids: None,
        method_info: -1,
        var_count: 0,
        activation: None,
        deferred_class_map: None,
        init_only_view: false,
        kind,
    }
}

impl ObjectValue {
    /// `NamespacesFromQualifiers`.
    pub fn namespaces_from_qualifiers(quals: &Qualifiers, namespaces: &Namespaces) {
        namespaces.clear();
        for it in quals.key_set() {
            namespaces.push_back(it);
        }
    }

    /// `init()`: creates the shared singletons (also done lazily on first use).
    pub fn init() {
        STATICS.with(|s| {
            if s.borrow().is_none() {
                let label_namespace = new_namespace_value(NS_PRIVATE);
                label_namespace.set_name("label namespace");
                let loop_label_namespace = new_namespace_value(NS_PRIVATE);
                loop_label_namespace.set_name("loop label namespace name");
                let st = Statics {
                    undefined_value: ObjectValue::new(),
                    null_value: ObjectValue::new(),
                    enumerable_namespace: ObjectValue::new(),
                    label_namespace,
                    loop_label_namespace,
                    object_prototype: ObjectValue::new(),
                    internal_namespace: ObjectValue::new(),
                    intrinsic_attribute: ObjectValue::new(),
                    intrinsic_namespace: ObjectValue::new(),
                    static_attribute: ObjectValue::new(),
                    dynamic_attribute: ObjectValue::new(),
                    final_attribute: ObjectValue::new(),
                    virtual_attribute: ObjectValue::new(),
                    override_attribute: ObjectValue::new(),
                    native_attribute: ObjectValue::new(),
                };
                *s.borrow_mut() = Some(Rc::new(st));
            }
        });
    }

    /// `clear()`; the next use creates fresh singletons where Java would see nulls.
    pub fn clear() {
        STATICS.with(|s| *s.borrow_mut() = None);
    }

    pub fn undefined_value() -> ObjectValue {
        statics().undefined_value.clone()
    }
    pub fn null_value() -> ObjectValue {
        statics().null_value.clone()
    }
    pub fn enumerable_namespace() -> ObjectValue {
        statics().enumerable_namespace.clone()
    }
    pub fn label_namespace() -> ObjectValue {
        statics().label_namespace.clone()
    }
    pub fn loop_label_namespace() -> ObjectValue {
        statics().loop_label_namespace.clone()
    }
    pub fn object_prototype() -> ObjectValue {
        statics().object_prototype.clone()
    }
    pub fn internal_namespace() -> ObjectValue {
        statics().internal_namespace.clone()
    }
    pub fn intrinsic_attribute() -> ObjectValue {
        statics().intrinsic_attribute.clone()
    }
    pub fn intrinsic_namespace() -> ObjectValue {
        statics().intrinsic_namespace.clone()
    }
    pub fn static_attribute() -> ObjectValue {
        statics().static_attribute.clone()
    }
    pub fn dynamic_attribute() -> ObjectValue {
        statics().dynamic_attribute.clone()
    }
    pub fn final_attribute() -> ObjectValue {
        statics().final_attribute.clone()
    }
    pub fn virtual_attribute() -> ObjectValue {
        statics().virtual_attribute.clone()
    }
    pub fn override_attribute() -> ObjectValue {
        statics().override_attribute.clone()
    }
    pub fn native_attribute() -> ObjectValue {
        statics().native_attribute.clone()
    }

    pub(crate) fn from_kind(kind: ObjectKind) -> ObjectValue {
        ObjectValue(Rc::new(RefCell::new(new_data(kind))))
    }

    /// `new ObjectValue()`.
    pub fn new() -> ObjectValue {
        Self::from_kind(ObjectKind::Object)
    }

    /// `new ObjectValue(TypeValue type)`.
    pub fn with_type(type_: Option<&TypeValue>) -> ObjectValue {
        let o = Self::new();
        o.0.borrow_mut().type_ = type_.map(|t| t.get_default_type_info());
        o
    }

    /// `new ObjectValue(String value, TypeValue type)`.
    pub fn with_value(value: &str, type_: Option<&TypeValue>) -> ObjectValue {
        let o = Self::new();
        o.set_value(value);
        o.0.borrow_mut().type_ = type_.map(|t| t.get_default_type_info());
        o
    }

    /// `new ObjectValue(String value, TypeInfo type)`.
    pub fn with_value_type_info(value: &str, type_: &TypeInfo) -> ObjectValue {
        let o = Self::new();
        o.set_value(value);
        o.0.borrow_mut().type_ = Some(type_.clone());
        o
    }

    /// `new ObjectValue(Context cx, Builder builder, TypeValue type)`.
    pub fn with_builder(cx: &Context, builder: Builder, type_: Option<&TypeValue>) -> ObjectValue {
        let o = Self::new();
        o.clear_instance(cx, builder, type_, "", false);
        o
    }

    /// The protected copy constructor `ObjectValue(ObjectValue ov)` (names, slots and base objects are copied, where
    /// Java shares them).
    pub(crate) fn copy_data(ov: &ObjectValue, kind: ObjectKind) -> ObjectValue {
        let d = ov.0.borrow();
        let mut n = new_data(kind);
        n.builder = d.builder.clone();
        n.proto = d.proto.clone();
        n.var_count = d.var_count;
        n.number_usage = d.number_usage.clone();
        n.activation = d.activation.clone();
        n.type_ = d.type_.clone();
        n.method_info = d.method_info;
        n.flags = d.flags;
        n.value = d.value.clone();
        n.names = d.names.clone();
        n.slots = d.slots.clone();
        n.base_objs = d.base_objs.clone();
        n.protected_ns = d.protected_ns.clone();
        n.base_protected_ns = d.base_protected_ns.clone();
        ObjectValue(Rc::new(RefCell::new(n)))
    }

    pub fn ptr_eq(&self, other: &ObjectValue) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn addr(&self) -> usize {
        Rc::as_ptr(&self.0) as *const u8 as usize
    }

    pub fn data(&self) -> Ref<'_, ObjectValueData> {
        self.0.borrow()
    }

    pub fn data_mut(&self) -> RefMut<'_, ObjectValueData> {
        self.0.borrow_mut()
    }

    pub(crate) fn disp(&self) -> Disp {
        match &self.0.borrow().kind {
            ObjectKind::Object => Disp::Plain,
            ObjectKind::Namespace(_) => Disp::Namespace,
            ObjectKind::Type(_) => Disp::Type,
            ObjectKind::Wrapper(w) => Disp::Wrapper(w.clone()),
        }
    }

    /// `instanceof TypeValue`.
    pub fn as_type_value(&self) -> Option<TypeValue> {
        TypeValue::from_object(self)
    }

    pub fn is_type_value(&self) -> bool {
        matches!(self.0.borrow().kind, ObjectKind::Type(_))
    }

    /// `instanceof NamespaceValue` (which includes `UnresolvedNamespace`).
    pub fn is_namespace_value(&self) -> bool {
        matches!(self.0.borrow().kind, ObjectKind::Namespace(_))
    }

    /// `instanceof UnresolvedNamespace`.
    pub fn is_unresolved_namespace(&self) -> bool {
        matches!(&self.0.borrow().kind, ObjectKind::Namespace(n) if n.unresolved.is_some())
    }

    /// `instanceof ObjectValueWrapper`.
    pub fn is_wrapper(&self) -> bool {
        matches!(self.0.borrow().kind, ObjectKind::Wrapper(_))
    }

    pub fn builder(&self) -> Option<Builder> {
        self.0.borrow().builder.clone()
    }

    pub fn set_builder(&self, builder: Option<Builder>) {
        self.0.borrow_mut().builder = builder;
    }

    /// The `type` field.
    pub fn type_(&self) -> Option<TypeInfo> {
        self.0.borrow().type_.clone()
    }

    pub fn set_type(&self, type_: Option<TypeInfo>) {
        self.0.borrow_mut().type_ = type_;
    }

    /// The `name` field (for a `TypeValue`, the inherited string field, not its `QName`).
    pub fn name(&self) -> String {
        self.0.borrow().name.clone()
    }

    pub fn name_ref(&self) -> Ref<'_, str> {
        Ref::map(self.0.borrow(), |d| d.name.as_str())
    }

    pub fn set_name(&self, name: &str) {
        self.0.borrow_mut().name = name.to_string();
    }

    /// The `_proto_` field.
    pub fn proto_field(&self) -> Option<ObjectValue> {
        self.0.borrow().proto.clone()
    }

    pub fn set_proto(&self, proto: Option<ObjectValue>) {
        self.0.borrow_mut().proto = proto;
    }

    pub fn method_info(&self) -> i32 {
        self.0.borrow().method_info
    }

    pub fn set_method_info(&self, v: i32) {
        self.0.borrow_mut().method_info = v;
    }

    pub fn var_count(&self) -> i32 {
        self.0.borrow().var_count
    }

    pub fn set_var_count(&self, v: i32) {
        self.0.borrow_mut().var_count = v;
    }

    pub fn activation(&self) -> Option<ObjectValue> {
        self.0.borrow().activation.clone()
    }

    pub fn set_activation(&self, v: Option<ObjectValue>) {
        self.0.borrow_mut().activation = v;
    }

    /// The `slots` field, in id order.
    pub fn slots(&self) -> Option<Vec<Slot>> {
        self.0.borrow().slots.as_ref().map(|s| s.iter().cloned().collect())
    }

    pub fn clear_instance(&self, cx: &Context, builder: Builder, type_: Option<&TypeValue>, name: &str, save_slot_ids: bool) {
        {
            let mut d = self.0.borrow_mut();
            d.flags = 0;
            d.value = String::new();
            d.names = None;
            if save_slot_ids {
                if let Some(slots) = &d.slots {
                    if slots.size() > 0 {
                        d.slot_ids = Some(SlotIdCache::new(slots));
                    }
                }
            }
            d.slots = None;
            d.deferred_class_map = None;
            d.base_objs = None;
            d.protected_ns = None;
            d.base_protected_ns = None;
            d.builder = Some(builder.clone());
        }
        let ti = type_.map(|t| t.get_default_type_info());
        {
            let mut d = self.0.borrow_mut();
            d.type_ = ti;
            d.proto = None;
            d.var_count = 0;
            d.number_usage = None;
            d.activation = None;
            d.method_info = -1;
        }
        self.init_instance(None, type_);
        builder.build(cx, self);
        self.0.borrow_mut().name = name.to_string();
    }

    pub fn init_instance(&self, proto_object: Option<ObjectValue>, _class_object: Option<&TypeValue>) {
        self.0.borrow_mut().proto = proto_object;
    }

    /// `removeName`: puts the name back with slot -1.
    pub fn remove_name(&self, cx: &Context, kind: i32, name: &str, qualifier: &ObjectValue) -> bool {
        if let Disp::Wrapper(w) = self.disp() {
            return w.remove_name(cx, kind, name, qualifier);
        }
        if self.0.borrow().names.is_some() {
            self.with_names_taken(|names| names.put(name, qualifier, Names::get_type_from_kind(kind), -1));
        }
        true
    }

    /// Runs `f` on the names table with this object unborrowed: a qualifier can be this very object (an interface is
    /// its own default namespace), and `Names` reads the qualifier's name.
    fn with_names_taken<R>(&self, f: impl FnOnce(&mut Names) -> R) -> R {
        let mut names = self.0.borrow_mut().names.take().unwrap_or_default();
        let r = f(&mut names);
        self.0.borrow_mut().names = Some(names);
        r
    }

    pub fn has_name(&self, cx: &Context, kind: i32, name: &str, qualifier: &ObjectValue) -> bool {
        match self.disp() {
            Disp::Wrapper(w) => w.has_name(cx, kind, name, qualifier),
            Disp::Type => {
                TypeValue::resolve_if_needed(self, cx);
                self.base_has_name(cx, kind, name, qualifier)
            }
            _ => self.base_has_name(cx, kind, name, qualifier),
        }
    }

    fn base_has_name(&self, cx: &Context, kind: i32, name: &str, qualifier: &ObjectValue) -> bool {
        let (init_only, mut ret, base_objs, q) = {
            let d = self.0.borrow();
            if d.init_only_view && kind != SET_TOKEN {
                return false;
            }
            let ret = d.names.as_ref().map_or(false, |n| n.contains_key(name, qualifier, Names::get_type_from_kind(kind)));
            let q = if d.protected_ns.as_ref().map_or(false, |p| p.ptr_eq(qualifier)) {
                d.base_protected_ns.clone()
            } else {
                Some(qualifier.clone())
            };
            (d.init_only_view, ret, d.base_objs.clone(), q)
        };
        if !init_only && !ret {
            if let Some(base_objs) = base_objs {
                for b in &base_objs {
                    if ret {
                        break;
                    }
                    ret = match &q {
                        Some(q) => b.has_name(cx, kind, name, q),
                        None => false,
                    };
                }
            }
        }
        ret
    }

    pub fn has_name_unqualified(&self, cx: &Context, name: &str, kind: i32) -> bool {
        match self.disp() {
            Disp::Wrapper(w) => w.has_name_unqualified(cx, name, kind),
            Disp::Type => {
                TypeValue::resolve_if_needed(self, cx);
                self.base_has_name_unqualified(cx, name, kind)
            }
            _ => self.base_has_name_unqualified(cx, name, kind),
        }
    }

    fn base_has_name_unqualified(&self, cx: &Context, name: &str, kind: i32) -> bool {
        let (init_only, mut ret, base_objs) = {
            let d = self.0.borrow();
            if d.init_only_view && kind != SET_TOKEN {
                return false;
            }
            let ret = d.names.as_ref().map_or(false, |n| n.exist(name, Names::get_type_from_kind(kind)));
            (d.init_only_view, ret, d.base_objs.clone())
        };
        if !init_only && !ret {
            if let Some(base_objs) = base_objs {
                for b in &base_objs {
                    if ret {
                        break;
                    }
                    ret = b.has_name_unqualified(cx, name, kind);
                }
            }
        }
        ret
    }

    /// `hasNames(cx, kind, name, namespaces)`: the namespaces under which the name is defined, for unqualified lookup.
    pub fn has_names(&self, cx: &Context, kind: i32, name: &str, namespaces: &Namespaces) -> Option<Namespaces> {
        match self.disp() {
            Disp::Wrapper(w) => w.has_names(cx, kind, name, namespaces),
            _ => self.has_names_base_objs(cx, kind, name, namespaces, true),
        }
    }

    /// `hasNames(cx, kind, name, namespaces, search_base_objs)`.
    pub fn has_names_base_objs(&self, cx: &Context, kind: i32, name: &str, namespaces: &Namespaces, search_base_objs: bool) -> Option<Namespaces> {
        if let Disp::Type = self.disp() {
            TypeValue::resolve_if_needed(self, cx);
        }
        self.base_has_names(cx, kind, name, namespaces, search_base_objs)
    }

    fn base_has_names(&self, cx: &Context, kind: i32, name: &str, namespaces: &Namespaces, search_base_objs: bool) -> Option<Namespaces> {
        let nss = namespaces.to_vec();
        let mut has_namespaces: Option<Namespaces> = None;
        let mut protected_index: i32 = -1;
        let mut searched_protected = false;
        let mut matched_protected = false;
        let (init_only, base_objs, protected_ns, base_protected_ns) = {
            let d = self.0.borrow();
            if d.init_only_view && kind != SET_TOKEN {
                return None;
            }
            if let Some(names) = &d.names {
                let ty = Names::get_type_from_kind(kind);
                if !(nss.len() > 1 && !names.contains_key_type(name, ty)) {
                    searched_protected = true;
                    for (i, qual) in nss.iter().enumerate() {
                        let is_protected = d.protected_ns.as_ref().map_or(false, |p| p.ptr_eq(qual));
                        if names.contains_key(name, qual, ty) {
                            has_namespaces.get_or_insert_with(Namespaces::new).add(qual.clone());
                            if is_protected {
                                matched_protected = true;
                            }
                        }
                        if is_protected {
                            protected_index = i as i32;
                        }
                    }
                }
            }
            (d.init_only_view, d.base_objs.clone(), d.protected_ns.clone(), d.base_protected_ns.clone())
        };
        if !init_only && search_base_objs {
            if let Some(base_objs) = base_objs {
                if !searched_protected && !matched_protected {
                    if let Some(p) = &protected_ns {
                        if let Some(i) = nss.iter().position(|n| n.ptr_eq(p)) {
                            protected_index = i as i32;
                        }
                    }
                }
                let for_base = if protected_index != -1 && !matched_protected {
                    let mut v = nss.clone();
                    match base_protected_ns {
                        Some(b) => v[protected_index as usize] = b,
                        None => {
                            v.remove(protected_index as usize);
                        }
                    }
                    Namespaces::from_vec(v)
                } else {
                    namespaces.clone()
                };
                for b in &base_objs {
                    if let Some(temp) = b.has_names(cx, kind, name, &for_base) {
                        match &has_namespaces {
                            None => has_namespaces = Some(temp),
                            Some(h) => h.add_all(&temp),
                        }
                    }
                }
            }
        }
        has_namespaces
    }

    /// `get(cx, name, qualifier)`: the slot of the getter bound to the name.
    pub fn get(&self, cx: &Context, name: &str, qualifier: &ObjectValue) -> Option<Slot> {
        if let Disp::Wrapper(w) = self.disp() {
            return w.get(cx, name, qualifier);
        }
        let index = self.get_index(name, Some(qualifier), Names::GET_NAMES);
        self.get_slot(cx, index)
    }

    /// The private `get(name, qualifier, type)`, which does not dispatch.
    fn get_index(&self, name: &str, qualifier: Option<&ObjectValue>, ty: i32) -> i32 {
        let (mut ret, base_objs, q) = {
            let d = self.0.borrow();
            let ret = match (&d.names, qualifier) {
                (Some(n), Some(q)) => n.get(name, q, ty),
                _ => -1,
            };
            let q = match qualifier {
                Some(q) if d.protected_ns.as_ref().map_or(false, |p| p.ptr_eq(q)) => d.base_protected_ns.clone(),
                other => other.cloned(),
            };
            (ret, d.base_objs.clone(), q)
        };
        if ret == -1 {
            if let Some(base_objs) = base_objs {
                for b in &base_objs {
                    if ret != -1 {
                        break;
                    }
                    ret = b.get_index(name, q.as_ref(), ty);
                }
            }
        }
        ret
    }

    pub fn define_name(&self, cx: &Context, kind: i32, name: &str, qualifier: &ObjectValue, slot_index: i32) -> i32 {
        if let Disp::Wrapper(w) = self.disp() {
            return w.define_name(cx, kind, name, qualifier, slot_index);
        }
        {
            let mut d = self.0.borrow_mut();
            if d.names.is_none() {
                d.names = Some(Names::new());
            }
        }
        if self.has_name(cx, kind, name, qualifier) {
            0
        } else {
            self.with_names_taken(|names| names.put(name, qualifier, Names::get_type_from_kind(kind), slot_index));
            1
        }
    }

    /// `getNamesAndCreate()`, as a closure over the table.
    pub fn with_names_and_create<R>(&self, f: impl FnOnce(&mut Names) -> R) -> R {
        if let Disp::Wrapper(w) = self.disp() {
            return w.with_names_and_create(f);
        }
        self.with_names_taken(f)
    }

    pub fn define_names(&self, cx: &Context, kind: i32, name: &str, namespaces: &Namespaces, slot_index: i32) -> bool {
        if let Disp::Wrapper(w) = self.disp() {
            return w.define_names(cx, kind, name, namespaces, slot_index);
        }
        let nss = namespaces.to_vec();
        self.with_names_taken(|names| {
            for it in &nss {
                names.put(name, it, Names::get_type_from_kind(kind), slot_index);
            }
        });
        true
    }

    pub fn add_variable_slot(&self, cx: &Context, type_: Option<&TypeValue>, var_index: i32) -> i32 {
        let id = self.next_slot_id(cx);
        let new_slot = new_variable_slot(type_, id, var_index);
        new_slot.set_declared_by(Some(self.clone()));
        let mut d = self.0.borrow_mut();
        d.slots.get_or_insert_with(Slots::new).put(new_slot.clone());
        new_slot.id()
    }

    pub fn add_method_slot(&self, cx: &Context, type_: Option<&TypeValue>) -> i32 {
        let id = self.next_slot_id(cx);
        let new_slot = new_method_slot(type_, id);
        new_slot.set_declared_by(Some(self.clone()));
        let mut d = self.0.borrow_mut();
        d.slots.get_or_insert_with(Slots::new).put(new_slot.clone());
        new_slot.id()
    }

    fn next_slot_id(&self, cx: &Context) -> i32 {
        {
            let mut d = self.0.borrow_mut();
            if d.slots.is_none() {
                d.slots = Some(Slots::new());
            }
        }
        let has_cache = self.0.borrow().slot_ids.is_some();
        if has_cache {
            let mut cache = self.0.borrow_mut().slot_ids.take().unwrap();
            let id = cache.get_next_slot_id(cx);
            self.0.borrow_mut().slot_ids = Some(cache);
            id
        } else {
            cx.statics.borrow_mut().get_next_slot_id()
        }
    }

    pub fn add_slot(&self, slot: Slot) {
        self.0.borrow_mut().slots.get_or_insert_with(Slots::new).put(slot);
    }

    pub fn add_slot_implicit(&self, cx: &Context, slot_index: i32, kind: i32, type_: Option<&TypeValue>) -> i32 {
        let index = self.add_method_slot(cx, type_);
        self.get_slot(cx, slot_index).expect("NullPointerException").implicit(kind, index);
        index
    }

    /// `addSlotOverload(cx, slot_index, type, t1)`.
    pub fn add_slot_overload(&self, cx: &Context, slot_index: i32, type_: Option<&TypeValue>, t1: &TypeValue) -> i32 {
        let index = self.add_method_slot(cx, type_);
        self.get_slot(cx, slot_index).expect("NullPointerException").overload(t1, index);
        let slot = self.get_slot(cx, index).expect("NullPointerException");
        slot.add_type(t1.get_default_type_info());
        index
    }

    /// `addSlotOverload(cx, slot_index, type, t1, t2)`.
    pub fn add_slot_overload2(&self, cx: &Context, slot_index: i32, type_: Option<&TypeValue>, t1: &TypeValue, t2: &TypeValue) -> i32 {
        let index = self.add_method_slot(cx, type_);
        self.get_slot(cx, slot_index).expect("NullPointerException").overload2(t1, t2, index);
        let slot = self.get_slot(cx, index).expect("NullPointerException");
        slot.add_type(t1.get_default_type_info());
        slot.add_type(t2.get_default_type_info());
        index
    }

    /// The slot with the given id, here or in a base object.
    pub fn get_slot(&self, cx: &Context, index: i32) -> Option<Slot> {
        if let Disp::Wrapper(w) = self.disp() {
            return w.get_slot(cx, index);
        }
        let (ret, base_objs) = {
            let d = self.0.borrow();
            (d.slots.as_ref().and_then(|s| s.get_by_id(index)), d.base_objs.clone())
        };
        if ret.is_some() {
            return ret;
        }
        if let Some(base_objs) = base_objs {
            for b in &base_objs {
                let r = b.get_slot(cx, index);
                if r.is_some() {
                    return r;
                }
            }
        }
        None
    }

    /// `getSlotIndex`; a null qualifier means the public namespace.
    pub fn get_slot_index(&self, cx: &Context, kind: i32, name: &str, qualifier: Option<&ObjectValue>) -> i32 {
        if let Disp::Wrapper(w) = self.disp() {
            return w.get_slot_index(cx, kind, name, qualifier);
        }
        let mut index = -1;
        let qualifier = match qualifier {
            Some(q) => q.clone(),
            None => cx.public_namespace(),
        };
        let ty = Names::get_type_from_kind(kind);
        if !self.has_name_unqualified(cx, name, kind) {
            if let Some(p) = self.proto() {
                index = p.get_slot_index(cx, kind, name, Some(&qualifier));
            }
        } else {
            index = self.get_index(name, Some(&qualifier), ty);
        }
        index
    }

    pub fn get_implicit_index(&self, cx: &Context, slot_index: i32, kind: i32) -> i32 {
        if let Disp::Wrapper(w) = self.disp() {
            return w.get_implicit_index(cx, slot_index, kind);
        }
        let slot = self.get_slot(cx, slot_index);
        let index = slot.map_or(0, |s| s.implies(cx, kind));
        if index != 0 {
            index
        } else {
            slot_index
        }
    }

    /// `getOverloadIndex(cx, slot_index, t1)`.
    pub fn get_overload_index(&self, cx: &Context, slot_index: i32, t1: Option<&TypeValue>) -> i32 {
        if let Disp::Wrapper(w) = self.disp() {
            return w.get_overload_index(cx, slot_index, t1);
        }
        let index = self.get_slot(cx, slot_index).expect("NullPointerException").dispatch(cx, t1);
        if index != 0 {
            index
        } else {
            slot_index
        }
    }

    /// `getOverloadIndex(cx, slot_index, t1, t2)`.
    pub fn get_overload_index2(&self, cx: &Context, slot_index: i32, t1: Option<&TypeValue>, t2: Option<&TypeValue>) -> i32 {
        if let Disp::Wrapper(w) = self.disp() {
            return w.get_overload_index2(cx, slot_index, t1, t2);
        }
        let t1 = t1.cloned().unwrap_or_else(|| cx.no_type());
        let t2 = t2.cloned().unwrap_or_else(|| cx.no_type());
        let index = self.get_slot(cx, slot_index).expect("NullPointerException").dispatch2(cx, &t1, &t2);
        if index != 0 {
            index
        } else {
            slot_index
        }
    }

    pub fn add_var(&self, _cx: &Context) -> i32 {
        let mut d = self.0.borrow_mut();
        let v = d.var_count;
        d.var_count += 1;
        v
    }

    /// `proto()`: a `TypeValue` has none; a wrapper forwards.
    pub fn proto(&self) -> Option<ObjectValue> {
        match self.disp() {
            Disp::Type => None,
            Disp::Wrapper(w) => w.proto(),
            _ => self.0.borrow().proto.clone(),
        }
    }

    pub fn get_type(&self, cx: &Context) -> Option<TypeInfo> {
        match self.disp() {
            Disp::Type => Some(cx.type_type().get_default_type_info()),
            Disp::Wrapper(_) => self.0.borrow().type_.clone(),
            _ => {
                let t = self.0.borrow().type_.clone();
                match t {
                    Some(t) => Some(t),
                    None => Some(cx.no_type().get_default_type_info()),
                }
            }
        }
    }

    pub fn get_type_id(&self) -> i32 {
        match &self.0.borrow().kind {
            ObjectKind::Type(t) => t.type_id,
            _ => 0,
        }
    }

    pub fn is_dynamic(&self) -> bool {
        if let Disp::Type = self.disp() {
            return false;
        }
        let b = self.0.borrow().builder.clone();
        b.map_or(false, |b| b.is_dynamic())
    }

    pub fn is_final(&self) -> bool {
        let b = self.0.borrow().builder.clone();
        b.map_or(false, |b| b.is_final())
    }

    /// `equals`: identity.
    pub fn equals(&self, o: &ObjectValue) -> bool {
        self.ptr_eq(o)
    }

    /// `compareTo` against another `ObjectValue`: by name.
    pub fn compare_to(&self, o: &ObjectValue) -> i32 {
        java_compare_to(&self.name_ref(), &o.name_ref())
    }

    pub fn deferred_class_map_put(&self, key: TypeValue, value: NodeRef) -> Option<NodeRef> {
        self.0.borrow_mut().deferred_class_map.get_or_insert_with(HashMap::new).insert(key, value)
    }

    pub fn deferred_class_map_get(&self, key: &TypeValue) -> Option<NodeRef> {
        self.0.borrow_mut().deferred_class_map.get_or_insert_with(HashMap::new).get(key).cloned()
    }

    pub fn deferred_class_map_clear(&self) {
        self.0.borrow_mut().deferred_class_map.get_or_insert_with(HashMap::new).clear();
    }

    pub fn is_interface(&self) -> bool {
        if let Disp::Wrapper(w) = self.disp() {
            return w.is_interface();
        }
        let b = self.0.borrow().builder.clone();
        b.map_or(false, |b| b.is_class_builder() && b.class_data().is_interface)
    }

    pub fn can_early_bind(&self) -> bool {
        if let Disp::Wrapper(w) = self.disp() {
            return w.can_early_bind();
        }
        let b = self.0.borrow().builder.clone();
        match b {
            Some(b) if b.is_instance_builder() => b.instance_data().can_early_bind,
            _ => true,
        }
    }

    /// `getNames()`, as a copy of the table.
    pub fn get_names(&self) -> Option<Names> {
        if let Disp::Wrapper(w) = self.disp() {
            return w.get_names();
        }
        self.0.borrow().names.clone()
    }

    /// `getNames()` without copying.
    pub fn with_names<R>(&self, f: impl FnOnce(Option<&Names>) -> R) -> R {
        if let Disp::Wrapper(w) = self.disp() {
            return w.with_names(f);
        }
        f(self.0.borrow().names.as_ref())
    }

    pub fn set_number_usage(&self, usage: Option<NumberUsage>) {
        self.0.borrow_mut().number_usage = usage;
    }

    pub fn get_number_usage(&self) -> Option<NumberUsage> {
        self.0.borrow().number_usage.clone()
    }

    pub fn get_value(&self) -> String {
        if let Disp::Wrapper(w) = self.disp() {
            return w.get_value();
        }
        self.0.borrow().value.clone()
    }

    pub fn set_value(&self, value: &str) {
        if let Disp::Wrapper(w) = self.disp() {
            return w.set_value(value);
        }
        let mut d = self.0.borrow_mut();
        d.flags |= HAS_VALUE_Flag;
        d.value = value.to_string();
    }

    pub fn has_value(&self) -> bool {
        if let Disp::Wrapper(w) = self.disp() {
            return w.has_value();
        }
        (self.0.borrow().flags & HAS_VALUE_Flag) != 0
    }

    pub fn boolean_value(&self) -> bool {
        if let Disp::Wrapper(w) = self.disp() {
            return w.boolean_value();
        }
        self.get_value() == "true"
    }

    pub fn set_package(&self, package_flag: bool) {
        if let Disp::Wrapper(w) = self.disp() {
            return w.set_package(package_flag);
        }
        let mut d = self.0.borrow_mut();
        d.flags = if package_flag { d.flags | IS_PACKAGE_Flag } else { d.flags & !IS_PACKAGE_Flag };
    }

    pub fn is_package(&self) -> bool {
        if let Disp::Wrapper(w) = self.disp() {
            return w.is_package();
        }
        (self.0.borrow().flags & IS_PACKAGE_Flag) != 0
    }

    pub fn set_init_only(&self, b: bool) {
        self.0.borrow_mut().init_only_view = b;
    }

    pub fn is_init_only(&self) -> bool {
        self.0.borrow().init_only_view
    }

    pub fn is_internal(&self) -> bool {
        match &self.0.borrow().kind {
            ObjectKind::Namespace(n) => n.ns_kind == crate::asc::util::context::NS_INTERNAL,
            _ => false,
        }
    }

    pub fn is_protected(&self) -> bool {
        match &self.0.borrow().kind {
            ObjectKind::Namespace(n) => n.ns_kind == crate::asc::util::context::NS_PROTECTED,
            _ => false,
        }
    }

    pub fn is_private(&self) -> bool {
        match &self.0.borrow().kind {
            ObjectKind::Namespace(n) => n.ns_kind == NS_PRIVATE,
            _ => false,
        }
    }

    pub fn get_namespace_kind(&self) -> u8 {
        match &self.0.borrow().kind {
            ObjectKind::Namespace(n) => n.ns_kind,
            _ => NS_PUBLIC,
        }
    }

    pub fn is_config_ns(&self) -> bool {
        match &self.0.borrow().kind {
            ObjectKind::Namespace(n) => n.config_ns,
            _ => false,
        }
    }

    /// Adds a base class or interface whose names and slots this object sees.
    pub fn add_base_obj(&self, base: &ObjectValue) {
        let mut d = self.0.borrow_mut();
        let b = d.base_objs.get_or_insert_with(|| Vec::with_capacity(1));
        if !b.iter().any(|o| o.ptr_eq(base)) {
            b.push(base.clone());
        }
    }

    /// The `base_objs` field.
    pub fn base_objs(&self) -> Option<Vec<ObjectValue>> {
        self.0.borrow().base_objs.clone()
    }

    pub fn set_protected_namespaces(&self, protected_ns: Option<ObjectValue>, base_protected: Option<ObjectValue>) {
        let mut d = self.0.borrow_mut();
        d.protected_ns = protected_ns;
        d.base_protected_ns = base_protected;
    }

    /// `Object.toString()` for classes that do not override `getPrintableName`; Java prints an identity hash here.
    pub fn get_printable_name(&self) -> String {
        match self.disp() {
            Disp::Type => TypeValue::from_object(self).unwrap().name().name().to_string(),
            Disp::Namespace => {
                let class = if self.is_unresolved_namespace() { "UnresolvedNamespace" } else { "NamespaceValue" };
                format!("macromedia.asc.semantics.{}@{:x}", class, self.addr())
            }
            Disp::Wrapper(_) => format!("macromedia.asc.semantics.ObjectValueWrapper@{:x}", self.addr()),
            Disp::Plain => format!("macromedia.asc.semantics.ObjectValue@{:x}", self.addr()),
        }
    }
}

impl Default for ObjectValue {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ObjectValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(t) = TypeValue::from_object(self) {
            return fmt::Display::fmt(&t, f);
        }
        if use_debug_to_strings() {
            let d = self.0.borrow();
            let ty = d.type_.as_ref().map_or("null".to_string(), |t| t.to_string());
            write!(f, "ObjVal: <{}> {}", ty, d.name)?;
            if let Some(names) = &d.names {
                if names.size() > 0 {
                    write!(f, "\nmethods: macromedia.asc.util.Names@{:x}", names as *const Names as usize)?;
                }
            }
            Ok(())
        } else {
            f.write_str(&self.get_value())
        }
    }
}

impl fmt::Debug for ObjectValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.try_borrow() {
            Ok(d) => write!(f, "ObjectValue({:?})", d.name),
            Err(_) => write!(f, "ObjectValue(<borrowed>)"),
        }
    }
}

/// `ObjectValue.SlotIDCache`: replays the slot ids of a recompiled object so that its slots keep their ids.
#[derive(Clone)]
struct SlotIdCache {
    slot_id_boundaries: Vec<i32>,
    cur_range_index: usize,
    cur_slot_id: i32,
}

impl SlotIdCache {
    fn new(slots: &Slots) -> SlotIdCache {
        let mut c = SlotIdCache { slot_id_boundaries: Vec::new(), cur_range_index: 0, cur_slot_id: -1 };
        c.init(slots);
        c
    }

    fn init(&mut self, slots: &Slots) {
        self.cur_range_index = 0;
        self.cur_slot_id = -1;
        if slots.size() > 0 {
            let lo_id = slots.at(0).id();
            let hi_id = slots.at(slots.size() - 1).id();
            if slots.size() as i32 - 1 == hi_id - lo_id {
                self.slot_id_boundaries = vec![lo_id, hi_id];
            } else {
                let mut b = Vec::with_capacity(6);
                let mut last_id = lo_id;
                let mut start_id = lo_id;
                for i in 1..slots.size() {
                    let id = slots.at(i).id();
                    if id != last_id + 1 {
                        b.push(start_id);
                        b.push(last_id);
                        start_id = id;
                    }
                    last_id = id;
                }
                b.push(start_id);
                b.push(last_id);
                self.slot_id_boundaries = b;
            }
            self.cur_slot_id = self.slot_id_boundaries[0];
        }
    }

    fn get_next_slot_id(&mut self, cx: &Context) -> i32 {
        let mut slot_id = -1;
        let b = &self.slot_id_boundaries;
        if self.cur_slot_id > *b.last().expect("IndexOutOfBoundsException") {
            slot_id = cx.statics.borrow_mut().get_next_slot_id();
        }
        while slot_id == -1 && self.cur_range_index + 1 < b.len() {
            let hi = b[self.cur_range_index + 1];
            if self.cur_slot_id > hi {
                self.cur_range_index += 2;
                if self.cur_range_index + 1 < b.len() {
                    self.cur_slot_id = b[self.cur_range_index];
                }
            } else {
                slot_id = self.cur_slot_id;
                self.cur_slot_id += 1;
            }
        }
        if slot_id == -1 {
            slot_id = cx.statics.borrow_mut().get_next_slot_id();
        }
        slot_id
    }
}
