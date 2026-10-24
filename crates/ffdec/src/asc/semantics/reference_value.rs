//! Port of `macromedia/asc/semantics/ReferenceValue.java`: a name plus the namespaces it may be qualified with, bound
//! lazily to the slot it refers to.

use std::cell::{Ref, RefCell, RefMut};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::asc::parser::tokens::*;
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::parameterized_name::new_parameterized_name_from_types;
use crate::asc::semantics::slot::{Slot, CALL_Method};
use crate::asc::semantics::type_info::TypeInfo;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::semantics::value::*;
use crate::asc::util::bit_set::BitSet;
use crate::asc::util::context::Context;
use crate::asc::util::errors::*;
use crate::asc::util::namespaces::Namespaces;

pub struct ReferenceValueData {
    pub flags: i32,
    base: Option<ObjectValue>,
    get_slot_index: i32,
    set_method_slot_index: i32,
    ud_bits: Option<BitSet>,
    type_: Option<TypeInfo>,
    src_position: i32,
    pub slot: Option<Slot>,
    pub name: String,
    pub namespaces: Namespaces,
    pub type_params: Option<Vec<ReferenceValue>>,
    pub is_nullable: bool,
    pub has_nullable_anno: bool,
}

/// A Java `ReferenceValue` reference; equality and hashing are identity, as in Java.
#[derive(Clone)]
pub struct ReferenceValue(Rc<RefCell<ReferenceValueData>>);

impl PartialEq for ReferenceValue {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ReferenceValue {}

/// Identity hash: a `HashSet<ReferenceValue>` iterates in an order unrelated to Java's.
impl Hash for ReferenceValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.addr().hash(state);
    }
}

impl ReferenceValue {
    fn raw(base: Option<ObjectValue>, name: &str, namespaces: Namespaces) -> ReferenceValue {
        ReferenceValue(Rc::new(RefCell::new(ReferenceValueData {
            flags: 0,
            base,
            get_slot_index: 0,
            set_method_slot_index: 0,
            ud_bits: None,
            type_: None,
            src_position: 0,
            slot: None,
            name: name.to_string(),
            namespaces,
            type_params: None,
            is_nullable: true,
            has_nullable_anno: false,
        })))
    }

    /// `new ReferenceValue(cx, base, name, qualifier)`.
    pub fn new(cx: &Context, base: Option<ObjectValue>, name: &str, qualifier: Option<&ObjectValue>) -> ReferenceValue {
        Self::new_kind(cx, base, name, qualifier, GET_TOKEN)
    }

    /// `new ReferenceValue(cx, base, name, qualifier, kind)`.
    pub fn new_kind(cx: &Context, base: Option<ObjectValue>, name: &str, qualifier: Option<&ObjectValue>, kind: i32) -> ReferenceValue {
        let namespaces = cx.statics.borrow_mut().intern_namespaces.intern_one(qualifier);
        let r = Self::raw(base, name, namespaces);
        r.set_kind(kind);
        r.set_qualified(qualifier.is_some());
        r.set_get_slot_index(-1);
        r.set_set_slot_index(-1);
        r.set_scope_index(-1);
        r
    }

    /// `new ReferenceValue(cx, base, name, namespaces)`.
    pub fn with_namespaces(cx: &Context, base: Option<ObjectValue>, name: &str, namespaces: &Namespaces) -> ReferenceValue {
        Self::with_namespaces_kind(cx, base, name, namespaces, GET_TOKEN)
    }

    /// `new ReferenceValue(cx, base, name, namespaces, kind)`.
    pub fn with_namespaces_kind(cx: &Context, base: Option<ObjectValue>, name: &str, namespaces: &Namespaces, kind: i32) -> ReferenceValue {
        let namespaces = cx.statics.borrow_mut().intern_namespaces.intern(namespaces);
        let r = Self::raw(base, name, namespaces);
        r.set_kind(kind);
        r.set_get_slot_index(-1);
        r.set_set_slot_index(-1);
        r.set_scope_index(-1);
        r
    }

    pub fn ptr_eq(&self, other: &ReferenceValue) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn addr(&self) -> usize {
        Rc::as_ptr(&self.0) as *const u8 as usize
    }

    pub fn data(&self) -> Ref<'_, ReferenceValueData> {
        self.0.borrow()
    }

    pub fn data_mut(&self) -> RefMut<'_, ReferenceValueData> {
        self.0.borrow_mut()
    }

    pub fn set_position(&self, pos: i32) {
        self.0.borrow_mut().src_position = pos;
    }

    pub fn get_position(&self) -> i32 {
        self.0.borrow().src_position
    }

    /// The `slot` field.
    pub fn slot(&self) -> Option<Slot> {
        self.0.borrow().slot.clone()
    }

    pub fn set_slot(&self, slot: Option<Slot>) {
        self.0.borrow_mut().slot = slot;
    }

    /// The private `ud_bits` field, for inspection.
    pub fn ud_bits(&self) -> Option<BitSet> {
        self.0.borrow().ud_bits.clone()
    }

    /// The private `type` field (the cached `getType` result), for inspection.
    pub fn type_field(&self) -> Option<TypeInfo> {
        self.0.borrow().type_.clone()
    }

    /// The `name` field.
    pub fn name(&self) -> String {
        self.0.borrow().name.clone()
    }

    pub fn set_name(&self, name: &str) {
        self.0.borrow_mut().name = name.to_string();
    }

    /// The `type_params` field.
    pub fn type_params(&self) -> Option<Vec<ReferenceValue>> {
        self.0.borrow().type_params.clone()
    }

    pub fn is_nullable(&self) -> bool {
        self.0.borrow().is_nullable
    }

    pub fn has_nullable_anno(&self) -> bool {
        self.0.borrow().has_nullable_anno
    }

    fn flags(&self) -> i32 {
        self.0.borrow().flags
    }

    fn set_flags(&self, f: i32) {
        self.0.borrow_mut().flags = f;
    }

    pub fn set_is_attribute_identifier(&self, is_attrid: bool) {
        let f = self.flags();
        self.set_flags(if is_attrid { f | IS_ATTRID_Flag } else { f & !IS_ATTRID_Flag });
    }

    pub fn is_attribute_identifier(&self) -> bool {
        (self.flags() & IS_ATTRID_Flag) != 0
    }

    /// `getValue(cx)`: the slot's value, unless the slot is an accessor.
    pub fn get_value(&self, cx: &Context) -> Option<Value> {
        let s = self.get_slot_kind(cx, self.get_kind());
        match s {
            Some(s) if s.get_method_id() < 0 => s.get_value(),
            _ => None,
        }
    }

    pub fn get_type(&self, cx: &Context) -> TypeInfo {
        self.get_type_kind(cx, self.get_kind())
    }

    /// `getType(cx, kind)`: the declared slot type, or the more specific type of the reaching definitions.
    pub fn get_type_kind(&self, cx: &Context, kind: i32) -> TypeInfo {
        if let Some(t) = self.0.borrow().type_.clone() {
            return t;
        }
        let ud_bits = self.0.borrow().ud_bits.clone();
        let deftype = cx.get_def_type(ud_bits);
        let mut slottype: Option<TypeInfo> = None;
        if let Some(s) = self.get_slot_kind(cx, kind) {
            slottype = s.get_type();
        }
        let slottype = slottype.unwrap_or_else(|| cx.no_type().get_default_type_info());
        let deftype_value = deftype.as_ref().and_then(|d| d.get_type_value());
        let new_type = if slottype.type_value().includes(cx, deftype_value.as_ref()) {
            let tv = slottype.type_value();
            if tv.is_numeric(cx) || tv.ptr_eq(&cx.no_type()) {
                Some(slottype)
            } else {
                deftype
            }
        } else {
            Some(slottype)
        };
        let t = new_type.unwrap_or_else(|| cx.no_type().get_default_type_info());
        self.0.borrow_mut().type_ = Some(t.clone());
        t
    }

    pub fn get_slot(&self, cx: &Context) -> Option<Slot> {
        self.get_slot_kind(cx, GET_TOKEN)
    }

    /// `getSlot(cx, kind)`: binds on first use, then finds the slot of the wanted kind next to the bound one.
    pub fn get_slot_kind(&self, cx: &Context, kind: i32) -> Option<Slot> {
        if self.0.borrow().slot.is_none() {
            if self.is_attribute_identifier() {
                return None;
            }
            self.set_kind(if kind == SET_TOKEN { SET_TOKEN } else { GET_TOKEN });
            let flags = self.flags();
            if self.lookup(cx, flags) {
                if kind == self.get_kind() {
                    return self.slot();
                }
            } else if kind != GET_TOKEN {
                return None;
            }
        }
        let mut slot = self.slot();
        if self.get_kind() != kind {
            let base = match self.get_base() {
                Some(b) => b,
                None => {
                    let si = self.get_scope_index();
                    if si >= 0 && (si as usize) < cx.get_scope_depth() {
                        cx.scope_at(si)
                    } else {
                        return None;
                    }
                }
            };
            match kind {
                SET_TOKEN => {
                    slot = base.get_slot(cx, self.get_set_slot_index());
                    self.set_slot(slot.clone());
                    self.set_kind(SET_TOKEN);
                }
                GET_TOKEN => {
                    slot = base.get_slot(cx, self.get_get_slot_index());
                    self.set_slot(slot.clone());
                    self.set_kind(GET_TOKEN);
                }
                _ => {
                    slot = base.get_slot(cx, self.get_get_slot_index());
                    self.set_slot(slot.clone());
                    self.set_kind(GET_TOKEN);
                    if let Some(s) = &slot {
                        let index = s.implies(cx, kind);
                        slot = base.get_slot(cx, index);
                    }
                }
            }
            if let Some(s) = &slot {
                if cx.check_version() && s.get_version() as i32 > cx.version() {
                    cx.error3(
                        self.get_position(),
                        kError_WrongVersion,
                        &self.name(),
                        &s.get_version().to_string(),
                        &cx.version().to_string(),
                    );
                }
            }
        }
        slot
    }

    /// Looks the name up on the base, or else along the scope chain from the innermost scope out.
    pub fn lookup(&self, cx: &Context, flags: i32) -> bool {
        let mut is_found = false;
        if self.get_base().is_none() {
            let scopes = cx.get_scopes();
            let lowest_scope: i32 = if self.is_type_annotation() { 0 } else { cx.statics.borrow().with_depth + 1 };
            let mut i = scopes.len() as i32 - 1;
            while i >= lowest_scope {
                self.0.borrow_mut().base = Some(scopes[i as usize].clone());
                is_found = self.lookup_with_base(cx, flags);
                if is_found {
                    self.set_scope_index(i);
                    break;
                }
                i -= 1;
            }
            self.0.borrow_mut().base = None;
        } else {
            is_found = self.lookup_with_base(cx, flags);
        }
        is_found
    }

    pub fn lookup_with_base(&self, cx: &Context, flags: i32) -> bool {
        if !self.is_qualified() {
            self.find_unqualified(cx, flags)
        } else {
            self.find_qualified(cx, flags)
        }
    }

    pub fn find_qualified(&self, cx: &Context, _flags: i32) -> bool {
        let namespaces = self.get_immutable_namespaces().to_vec();
        let name = self.name();
        for qualifier in &namespaces {
            let mut obj = self.get_base();
            while let Some(o) = obj {
                if o.has_name(cx, self.get_kind(), &name, qualifier) {
                    if self.0.borrow().type_params.is_some() {
                        let index = o.get_slot_index(cx, self.get_kind(), &name, Some(qualifier));
                        let slot = o.get_slot(cx, index);
                        self.bind_to_type_param_slot(cx, &o, Some(qualifier), slot);
                    } else {
                        self.bind_to_slot(cx, &o, qualifier);
                    }
                    return true;
                }
                obj = o.proto();
            }
        }
        false
    }

    pub fn find_unqualified(&self, cx: &Context, _flags: i32) -> bool {
        let name = self.name();
        let namespaces = self.get_immutable_namespaces();
        let mut obj = self.get_base();
        while let Some(o) = obj {
            if let Some(has_namespaces) = o.has_names(cx, self.get_kind(), &name, &namespaces) {
                let mut local_qualifier: Option<ObjectValue> = None;
                let mut error_reported = false;
                if self.get_kind() == SET_TOKEN || self.get_kind() == GET_TOKEN {
                    // A getter here and a setter in another open namespace would be ambiguous at runtime.
                    let opposite_kind = if self.get_kind() == SET_TOKEN { GET_TOKEN } else { SET_TOKEN };
                    if let Some(has_namespaces2) = o.has_names(cx, opposite_kind, &name, &namespaces) {
                        let is_only_protected = if has_namespaces.size() == 1 { has_namespaces.at(0).is_protected() } else { false };
                        for lq in has_namespaces2.to_vec() {
                            local_qualifier = Some(lq.clone());
                            if !error_reported && !(is_only_protected && lq.is_protected()) && !has_namespaces.contains(&lq) {
                                has_namespaces.add(lq);
                                break;
                            }
                        }
                    }
                }
                let mut last_index = 0;
                let mut slot: Option<Slot> = None;
                for lq in has_namespaces.to_vec() {
                    local_qualifier = Some(lq.clone());
                    let index = o.get_slot_index(cx, self.get_kind(), &name, Some(&lq));
                    slot = o.get_slot(cx, index);
                    if let Some(s) = &slot {
                        if s.get_base_node().is_none() {
                            if last_index != 0 && index != last_index && !error_reported {
                                cx.error1(self.get_position(), kError_AmbiguousReference, &name);
                                error_reported = true;
                            }
                            last_index = index;
                        }
                    }
                }
                if self.0.borrow().type_params.is_some() {
                    self.bind_to_type_param_slot(cx, &o, local_qualifier.as_ref(), slot);
                } else {
                    self.bind_to_slot(cx, &o, local_qualifier.as_ref().expect("NullPointerException"));
                }
                has_namespaces.clear();
                return true;
            }
            obj = o.proto();
        }
        false
    }

    fn bind_to_type_param_slot(&self, cx: &Context, obj: &ObjectValue, qualifier: Option<&ObjectValue>, s: Option<Slot>) {
        let s = s.expect("NullPointerException");
        let factory = match s.get_value().and_then(|v| v.as_type_value()) {
            Some(f) => f,
            None => return,
        };
        let type_params = self.type_params().unwrap();
        let mut types: Vec<TypeValue> = Vec::with_capacity(type_params.len());
        for (i, r) in type_params.iter().enumerate() {
            match r.get_slot(cx) {
                Some(type_slot) => {
                    if let Some(t) = type_slot.get_value().and_then(|v| v.as_type_value()) {
                        types.push(t);
                    }
                }
                None => {
                    if r.name() == "*" && r.get_immutable_namespaces().contains(&cx.public_namespace()) {
                        types.push(cx.no_type());
                    }
                }
            }
            if types.len() != i + 1 {
                // Could not resolve a type parameter, so the whole type is unknown.
                self.set_slot(None);
                return;
            }
        }
        if factory.is_parameterized() {
            let fullname = new_parameterized_name_from_types(qualifier.cloned(), &self.name(), &types);
            let name = fullname.get_name_part();
            let q = qualifier.expect("NullPointerException");
            if !obj.has_name(cx, GET_TOKEN, &name, q) {
                let bui = obj.builder().expect("NullPointerException");
                let slot_id = bui.implicit_var(cx, obj, &name, q, Some(&cx.type_type()), -1, -1, -1);
                let cframe = match types[0].get_parameterized_type(&name) {
                    Some(c) => c,
                    None => {
                        let c = TypeValue::instantiate_parameterized_type(cx, &fullname);
                        types[0].add_parameterized_type(&name, c.clone());
                        c
                    }
                };
                let slot = obj.get_slot(cx, slot_id).expect("NullPointerException");
                slot.set_value(Some(Value::Object(cframe.as_object().clone())));
                slot.set_const(true);
                slot.set_declared_by(None);
                bui.implicit_call(cx, obj, slot_id, Some(&cframe), CALL_Method, -1, -1);
                bui.implicit_construct(cx, obj, slot_id, Some(&cframe), CALL_Method, -1, -1);
                if factory.ptr_eq(&cx.vector_type()) {
                    cframe.set_indexed_type(Some(types[0].clone()));
                }
            } else {
                let slot_id = obj.get_slot_index(cx, GET_TOKEN, &name, Some(q));
                obj.get_slot(cx, slot_id);
            }
            self.bind_to_slot_named(cx, &name, obj, fullname.ns().as_ref().expect("NullPointerException"));
        } else {
            cx.internal_error("type parameters with a non-parameterized type");
        }
    }

    fn bind_to_slot(&self, cx: &Context, obj: &ObjectValue, qualifier: &ObjectValue) {
        let name = self.name();
        self.bind_to_slot_named(cx, &name, obj, qualifier);
    }

    fn bind_to_slot_named(&self, cx: &Context, name: &str, obj: &ObjectValue, qualifier: &ObjectValue) {
        let set_slot_index = obj.get_slot_index(cx, SET_TOKEN, name, Some(qualifier));
        let get_slot_index = obj.get_slot_index(cx, GET_TOKEN, name, Some(qualifier));
        let method_slot_index = obj.get_slot_index(cx, EMPTY_TOKEN, name, Some(qualifier));
        self.set_get_slot_index(get_slot_index);
        if method_slot_index != -1 {
            self.set_method_slot_index(method_slot_index);
        } else {
            self.set_set_slot_index(set_slot_index);
        }
        let idx = obj.get_slot_index(cx, self.get_kind(), name, Some(qualifier));
        let slot = obj.get_slot(cx, idx);
        self.set_slot(slot.clone());
        if cx.check_version() {
            let s = slot.as_ref().expect("NullPointerException");
            if s.get_version() as i32 > cx.version() {
                cx.error3(self.get_position(), kError_WrongVersion, &self.name(), &s.get_version().to_string(), &cx.version().to_string());
            }
        }
        // Never early bind to XML properties: dynamic XML children win over declared members of the same name.
        let is_xml_property = slot.as_ref().and_then(|s| s.declared_by()).and_then(|db| db.type_()).map_or(false, |t| {
            t.get_type_value().map_or(false, |tv| tv.ptr_eq(&cx.xml_type()) || tv.ptr_eq(&cx.xml_list_type()))
        });
        if cx.use_static_semantics() && !is_xml_property && self.0.borrow().type_params.is_none() {
            self.set_qualifier(cx, Some(qualifier));
        }
    }

    pub fn calc_use_definitions(&self, cx: &Context, rch_bits: Option<BitSet>) {
        self.get_slot_kind(cx, GET_TOKEN);
        if self.get_get_slot_index() < 0 {
            return;
        }
        let base = self.get_base();
        let slot = if base.as_ref().map_or(false, |b| {
            b.get_type(cx).and_then(|t| t.get_type_value()).map_or(false, |t| t.ptr_eq(&cx.no_type()))
        }) && self.get_get_slot_index() >= 0
        {
            base.unwrap().get_slot(cx, self.get_get_slot_index())
        } else if self.get_scope_index() == 0 && self.get_get_slot_index() >= 0 {
            cx.global_scope().expect("NullPointerException").get_slot(cx, self.get_get_slot_index())
        } else if self.get_scope_index() == cx.get_scope_depth() as i32 - 1 {
            cx.scope().expect("NullPointerException").get_slot(cx, self.get_get_slot_index())
        } else {
            return;
        };
        if let Some(slot) = slot {
            self.0.borrow_mut().ud_bits = BitSet::and(rch_bits, slot.get_def_bits());
        }
    }

    pub fn used_before_initialized(&self) -> bool {
        BitSet::is_empty(&self.0.borrow().ud_bits)
    }

    pub fn get_slot_index(&self, kind: i32) -> i32 {
        if kind == GET_TOKEN {
            return self.get_get_slot_index();
        }
        if kind == SET_TOKEN {
            return self.get_set_slot_index();
        }
        if kind == EMPTY_TOKEN {
            return self.get_method_slot_index();
        }
        -1
    }

    pub fn get_scope_index_kind(&self, _kind: i32) -> i32 {
        self.get_scope_index()
    }

    pub fn set_base(&self, base: Option<ObjectValue>) -> ReferenceValue {
        self.0.borrow_mut().base = base;
        self.clone()
    }

    pub fn get_base(&self) -> Option<ObjectValue> {
        self.0.borrow().base.clone()
    }

    pub fn is_reference(&self) -> bool {
        true
    }

    pub fn set_qualifier(&self, cx: &Context, qual: Option<&ObjectValue>) {
        self.set_qualified(qual.is_some());
        let n = cx.statics.borrow_mut().intern_namespaces.intern_one(qual);
        self.0.borrow_mut().namespaces = n;
    }

    pub fn to_multi_name(&self) -> String {
        format!("{}::{}", self.get_immutable_namespaces(), self.name())
    }

    pub fn is_qualified(&self) -> bool {
        (self.flags() & HAS_QUALIFIER_Flag) != 0
    }

    fn set_qualified(&self, qualified: bool) {
        let f = self.flags();
        self.set_flags(if qualified { f | HAS_QUALIFIER_Flag } else { f & !HAS_QUALIFIER_Flag });
    }

    pub fn set_immutable_namespaces(&self, namespaces: Namespaces) {
        self.0.borrow_mut().namespaces = namespaces;
    }

    /// The interned namespaces, which must not be modified.
    pub fn get_immutable_namespaces(&self) -> Namespaces {
        self.0.borrow().namespaces.clone()
    }

    pub fn set_scope_index(&self, scope_index: i32) {
        let mut f = self.flags();
        f &= !SCOPE_INDEX_Mask;
        f |= (scope_index << SCOPE_INDEX_Shift) & SCOPE_INDEX_Mask;
        self.set_flags(f);
    }

    pub fn get_scope_index(&self) -> i32 {
        (((self.flags() & SCOPE_INDEX_Mask) >> SCOPE_INDEX_Shift) as i16) as i32
    }

    pub fn set_kind(&self, kind: i32) {
        let mut f = self.flags();
        f &= !KIND_Mask;
        f |= (kind << KIND_Shift) & KIND_Mask;
        self.set_flags(f);
    }

    pub fn get_kind(&self) -> i32 {
        (((self.flags() & KIND_Mask) >> KIND_Shift) as i8) as i32
    }

    fn set_get_slot_index(&self, v: i32) {
        self.0.borrow_mut().get_slot_index = v;
    }

    fn get_get_slot_index(&self) -> i32 {
        self.0.borrow().get_slot_index
    }

    fn set_method_slot_index(&self, v: i32) {
        let mut d = self.0.borrow_mut();
        d.flags |= HAS_METHOD_INDEX_Flag;
        d.set_method_slot_index = v;
    }

    fn get_method_slot_index(&self) -> i32 {
        let d = self.0.borrow();
        if (d.flags & HAS_METHOD_INDEX_Flag) != 0 {
            d.set_method_slot_index
        } else {
            -1
        }
    }

    fn set_set_slot_index(&self, v: i32) {
        let mut d = self.0.borrow_mut();
        d.flags &= !HAS_METHOD_INDEX_Flag;
        d.set_method_slot_index = v;
    }

    fn get_set_slot_index(&self) -> i32 {
        let d = self.0.borrow();
        if (d.flags & HAS_METHOD_INDEX_Flag) != 0 {
            -1
        } else {
            d.set_method_slot_index
        }
    }

    pub fn set_type_annotation(&self, is_type_annotation: bool) {
        let f = self.flags();
        self.set_flags(if is_type_annotation { f | TYPE_ANNOTATION_Flag } else { f & !TYPE_ANNOTATION_Flag });
    }

    pub fn is_type_annotation(&self) -> bool {
        (self.flags() & TYPE_ANNOTATION_Flag) != 0
    }

    pub fn set_nullable_annotation(&self, is_explicit: bool, is_nullable: bool) {
        let mut d = self.0.borrow_mut();
        d.has_nullable_anno = is_explicit;
        d.is_nullable = is_nullable;
    }

    pub fn is_config_ref(&self) -> bool {
        let n = self.get_immutable_namespaces();
        n.size() == 1 && n.at(0).is_config_ns()
    }

    pub fn add_type_param(&self, type_: ReferenceValue) {
        self.0.borrow_mut().type_params.get_or_insert_with(|| Vec::with_capacity(1)).push(type_);
    }

    /// The name for diagnostics: the type parameter's name for a parameterized type such as `Vector.<T>`.
    pub fn get_diagnostic_type_name(&self) -> String {
        match self.type_params() {
            Some(tp) if !tp.is_empty() => tp[0].get_diagnostic_type_name(),
            _ => self.name(),
        }
    }
}

impl fmt::Display for ReferenceValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if use_debug_to_strings() {
            let d = self.0.borrow();
            let ty = d.type_.as_ref().map_or("null".to_string(), |t| t.to_string());
            write!(f, "RefValue: <{}> {}::{}", ty, d.name, d.namespaces)
        } else {
            Ok(())
        }
    }
}

impl fmt::Debug for ReferenceValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.try_borrow() {
            Ok(d) => write!(f, "ReferenceValue({:?})", d.name),
            Err(_) => write!(f, "ReferenceValue(<borrowed>)"),
        }
    }
}
