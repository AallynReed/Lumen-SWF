//! Port of `flex2.compiler.as3.reflect.As3Class` (the `flex2.compiler.abc.AbcClass` built from ASC's class frame) and
//! of `SymbolTable.NoType`, the class `*` resolves to. One struct; `as3` is `None` for `NoType`. Java passes a null
//! `Context` to the frame lookups; a per-`ContextStatics` placeholder context stands in for it.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use super::meta_data::MetaData;
use super::method::Method;
use super::type_table::TypeTable;
use super::variable::Variable;
use crate::asc::parser::node::NodeRef;
use crate::asc::parser::tokens::{EMPTY_TOKEN, GET_TOKEN, SET_TOKEN};
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::slot::Slot;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::util::context::{Context, ContextStatics, NS_INTERNAL, NS_PRIVATE, NS_PROTECTED, NS_PUBLIC};
use crate::asc::util::names::Names;
use crate::asc::util::namespaces::Namespaces;

/// `SymbolTable.publicNamespace` etc.
pub const PUBLIC_NAMESPACE: &str = "";
pub const INTERNAL_NAMESPACE: &str = "internal";
pub const PRIVATE_NAMESPACE: &str = "private";
pub const PROTECTED_NAMESPACE: &str = "protected";

/// `SymbolTable.VISIBILITY_NAMESPACES`; `As3Class` recognises this array by identity.
pub static VISIBILITY_NAMESPACES: [&str; 4] = [PUBLIC_NAMESPACE, PROTECTED_NAMESPACE, INTERNAL_NAMESPACE, PRIVATE_NAMESPACE];

thread_local! {
    static NULL_CONTEXTS: RefCell<Vec<(Weak<RefCell<ContextStatics>>, Rc<Context>)>> = const { RefCell::new(Vec::new()) };
}

fn null_context(statics: &Rc<RefCell<ContextStatics>>) -> Rc<Context> {
    NULL_CONTEXTS.with(|c| {
        let mut c = c.borrow_mut();
        c.retain(|(w, _)| w.strong_count() > 0);
        if let Some((_, cx)) = c.iter().find(|(w, _)| std::ptr::eq(w.as_ptr(), Rc::as_ptr(statics))) {
            return cx.clone();
        }
        let cx = Rc::new(Context::new(statics.clone()));
        c.push((Rc::downgrade(statics), cx.clone()));
        cx
    })
}

struct As3Data {
    cframe: RefCell<TypeValue>,
    cx_statics: Rc<RefCell<ContextStatics>>,
    access_ns: Namespaces,
    interface_names: RefCell<Option<Vec<Option<String>>>>,
    type_table: RefCell<Option<TypeTable>>,
}

pub struct AbcClass {
    /// `getName()`.
    pub name: String,
    /// `getSuperTypeName()`.
    pub super_type_name: Option<String>,
    metadata: Option<Vec<MetaData>>,
    as3: Option<As3Data>,
}

impl AbcClass {
    /// `SymbolTable.NoType`.
    pub fn no_type() -> AbcClass {
        AbcClass { name: "*".to_string(), super_type_name: None, metadata: None, as3: None }
    }

    /// `new As3Class(clsdef, typeTable)`.
    pub fn new(clsdef: &NodeRef, type_table: Option<TypeTable>) -> AbcClass {
        let n = clsdef.borrow();
        let c = n.class_definition().expect("ClassCastException");
        let cframe = c.cframe.clone().expect("NullPointerException");
        let access_ns = Namespaces::with_capacity(4);
        for ns in [&c.public_namespace, &c.default_namespace, &c.private_namespace, &c.protected_namespace] {
            access_ns.add(ns.clone().expect("NullPointerException"));
        }
        let cx_statics = c.cx.as_ref().expect("NullPointerException").borrow().statics.clone();
        let metadata = TypeTable::create_meta_data(&c.def);
        let name = cframe.builder().and_then(|b| b.classname()).expect("NullPointerException").to_string();
        let super_type_name = cframe.baseclass().map(|b| b.name().to_string());
        AbcClass {
            name,
            super_type_name,
            metadata,
            as3: Some(As3Data {
                cframe: RefCell::new(cframe),
                cx_statics,
                access_ns,
                interface_names: RefCell::new(None),
                type_table: RefCell::new(type_table),
            }),
        }
    }

    /// `new As3Class(clsdef, null)`, as `TypeAnalyzer` makes one.
    pub fn from_class_definition(clsdef: &NodeRef) -> AbcClass {
        AbcClass::new(clsdef, None)
    }

    pub fn is_no_type(&self) -> bool {
        self.as3.is_none()
    }

    fn type_table(&self) -> Option<TypeTable> {
        self.as3.as_ref().and_then(|a| a.type_table.borrow().clone())
    }

    fn lookup(&self, class_name: Option<&str>) -> Option<Rc<AbcClass>> {
        self.type_table().expect("NullPointerException").get_class(class_name)
    }

    /// The class frame (`cframe`).
    pub fn get_cframe(&self) -> Option<TypeValue> {
        self.as3.as_ref().map(|a| a.cframe.borrow().clone())
    }

    /// `freeze()`.
    pub fn freeze(&self) {
        if let Some(a) = &self.as3 {
            let copy = a.cframe.borrow().copy_type();
            *a.cframe.borrow_mut() = copy;
        }
    }

    /// `getVariable(String[] namespaces, name, inherited)`.
    pub fn get_variable(&self, namespaces: &[&str], name: &str, inherited: bool) -> Option<Variable> {
        let a = self.as3.as_ref()?;
        let nsset = self.convert_namespaces(a, namespaces);
        self.get_variable_kind(a, name, inherited, &nsset, GET_TOKEN)
    }

    /// `getVariable(Namespaces namespaces, name, inherited)`.
    pub fn get_variable_ns(&self, namespaces: &Namespaces, name: &str, inherited: bool) -> Option<Variable> {
        let a = self.as3.as_ref()?;
        self.get_variable_kind(a, name, inherited, namespaces, GET_TOKEN)
    }

    fn get_variable_kind(&self, a: &As3Data, name: &str, inherited: bool, nsset: &Namespaces, kind: i32) -> Option<Variable> {
        let cx = null_context(&a.cx_statics);
        let cframe = a.cframe.borrow().clone();
        let mut slot: Option<Slot> = None;
        let iframe = cframe.prototype().expect("NullPointerException");
        let mut matching = iframe.has_names_base_objs(&cx, kind, name, nsset, inherited);
        if let Some(m) = matching.as_ref().filter(|m| m.size() > 0) {
            let slot_index = iframe.get_slot_index(&cx, kind, name, Some(&m.back()));
            slot = iframe.get_slot(&cx, slot_index);
        }
        if slot.is_none() {
            matching = cframe.has_names_base_objs(&cx, kind, name, nsset, inherited);
            if let Some(m) = matching.as_ref().filter(|m| m.size() > 0) {
                let slot_index = cframe.get_slot_index(&cx, kind, name, Some(&m.back()));
                slot = cframe.get_slot(&cx, slot_index);
            }
        }
        match slot {
            Some(s) if s.is_variable_slot() => Some(Variable::new(s, matching.expect("NullPointerException").back(), name)),
            _ => None,
        }
    }

    fn convert_namespaces(&self, a: &As3Data, namespaces: &[&str]) -> Namespaces {
        if std::ptr::eq(namespaces.as_ptr(), VISIBILITY_NAMESPACES.as_ptr()) && namespaces.len() == VISIBILITY_NAMESPACES.len() {
            return a.access_ns.clone();
        }
        let nsset = Namespaces::with_capacity(namespaces.len());
        for ns in namespaces {
            let qual: ObjectValue = match *ns {
                PUBLIC_NAMESPACE => a.access_ns.at(NS_PUBLIC as usize),
                INTERNAL_NAMESPACE => a.access_ns.at(NS_INTERNAL as usize),
                PROTECTED_NAMESPACE => a.access_ns.at(NS_PROTECTED as usize),
                PRIVATE_NAMESPACE => a.access_ns.at(NS_PRIVATE as usize),
                _ => a.cx_statics.borrow_mut().get_namespace(ns, NS_PUBLIC),
            };
            nsset.add(qual);
        }
        nsset
    }

    /// `getMethod(String[] namespaces, name, inherited)`.
    pub fn get_method(&self, namespaces: &[&str], name: &str, inherited: bool) -> Option<Method> {
        let a = self.as3.as_ref()?;
        let nsset = self.convert_namespaces(a, namespaces);
        self.get_method_kind(a, name, inherited, &nsset, EMPTY_TOKEN)
    }

    pub fn get_method_ns(&self, namespaces: &Namespaces, name: &str, inherited: bool) -> Option<Method> {
        let a = self.as3.as_ref()?;
        self.get_method_kind(a, name, inherited, namespaces, EMPTY_TOKEN)
    }

    fn method_slot_in(cx: &Context, frame: &ObjectValue, kind: i32, want_call_slot: bool, name: &str, nsset: &Namespaces, inherited: bool) -> (Option<Namespaces>, Option<Slot>) {
        let matching = frame.has_names_base_objs(cx, kind, name, nsset, inherited);
        let mut slot = None;
        if let Some(m) = matching.as_ref().filter(|m| m.size() > 0) {
            let slot_index = frame.get_slot_index(cx, kind, name, Some(&m.back()));
            slot = frame.get_slot(cx, slot_index);
            if let Some(s) = slot.clone() {
                if want_call_slot {
                    let implied_id = s.implies(cx, EMPTY_TOKEN);
                    slot = frame.get_slot(cx, implied_id);
                } else if kind == GET_TOKEN && !s.is_getter() {
                    slot = None;
                }
            }
        }
        (matching, slot)
    }

    fn get_method_kind(&self, a: &As3Data, name: &str, inherited: bool, nsset: &Namespaces, kind: i32) -> Option<Method> {
        let cx = null_context(&a.cx_statics);
        let cframe = a.cframe.borrow().clone();
        let mut kind = kind;
        let mut want_call_slot = false;
        if kind == EMPTY_TOKEN {
            want_call_slot = true;
            kind = GET_TOKEN;
        }
        let iframe = cframe.prototype().expect("NullPointerException");
        let (mut matching, mut slot) = Self::method_slot_in(&cx, &iframe, kind, want_call_slot, name, nsset, inherited);
        if slot.is_none() {
            (matching, slot) = Self::method_slot_in(&cx, &cframe, kind, want_call_slot, name, nsset, inherited);
        }
        match slot {
            Some(s) if s.is_method_slot() => Some(Method::new(s, matching.expect("NullPointerException").back(), name)),
            _ => None,
        }
    }

    pub fn get_getter(&self, namespaces: &[&str], name: &str, inherited: bool) -> Option<Method> {
        let a = self.as3.as_ref()?;
        let nsset = self.convert_namespaces(a, namespaces);
        self.get_method_kind(a, name, inherited, &nsset, GET_TOKEN)
    }

    pub fn get_getter_ns(&self, namespaces: &Namespaces, name: &str, inherited: bool) -> Option<Method> {
        let a = self.as3.as_ref()?;
        self.get_method_kind(a, name, inherited, namespaces, GET_TOKEN)
    }

    pub fn get_setter(&self, namespaces: &[&str], name: &str, inherited: bool) -> Option<Method> {
        let a = self.as3.as_ref()?;
        let nsset = self.convert_namespaces(a, namespaces);
        self.get_method_kind(a, name, inherited, &nsset, SET_TOKEN)
    }

    pub fn get_setter_ns(&self, namespaces: &Namespaces, name: &str, inherited: bool) -> Option<Method> {
        let a = self.as3.as_ref()?;
        self.get_method_kind(a, name, inherited, namespaces, SET_TOKEN)
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_super_type_name(&self) -> Option<&str> {
        self.super_type_name.as_deref()
    }

    /// `getInterfaceNames()`: null when there are none.
    pub fn get_interface_names(&self) -> Option<Vec<Option<String>>> {
        let a = self.as3.as_ref()?;
        if a.interface_names.borrow().is_none() {
            let cframe = a.cframe.borrow().clone();
            let proto = cframe.prototype().expect("NullPointerException");
            let bui = proto.builder().expect("NullPointerException");
            let refs = bui.instance_data().interface_refs.clone();
            let names = refs
                .iter()
                .map(|r| {
                    let v = r.slot().expect("NullPointerException").get_object_value();
                    v.and_then(|v| TypeValue::from_object(&v)).map(|t| t.name().to_string())
                })
                .collect();
            *a.interface_names.borrow_mut() = Some(names);
        }
        let names = a.interface_names.borrow().clone().unwrap();
        if names.is_empty() {
            None
        } else {
            Some(names)
        }
    }

    /// `getMetaData(id, inherited)`.
    pub fn get_meta_data(&self, id: &str, inherited: bool) -> Vec<MetaData> {
        let mut list = Vec::new();
        self.get_meta_data_into(id, inherited, &mut list);
        list
    }

    fn get_meta_data_into(&self, id: &str, inherited: bool, list: &mut Vec<MetaData>) {
        if self.as3.is_none() {
            return;
        }
        if let Some(md) = &self.metadata {
            for m in md {
                if m.get_id() == Some(id) {
                    list.push(m.clone());
                }
            }
        }
        if inherited {
            if let Some(super_type) = self.lookup(self.get_super_type_name()) {
                super_type.get_meta_data_into(id, true, list);
            }
        }
    }

    /// `getMetaData(id, inherited)` with superclasses found by `lookup` instead of the type table.
    pub fn get_meta_data_with(&self, id: &str, inherited: bool, lookup: &dyn Fn(&str) -> Option<Rc<AbcClass>>) -> Vec<MetaData> {
        let mut list: Vec<MetaData> = self.metadata.iter().flatten().filter(|m| m.get_id() == Some(id)).cloned().collect();
        if inherited {
            if let Some(sup) = self.super_type_name.as_deref().and_then(lookup) {
                list.extend(sup.get_meta_data_with(id, true, lookup));
            }
        }
        list
    }

    pub fn implements_interface(&self, interface_name: &str) -> bool {
        let Some(a) = &self.as3 else { return false };
        let mut result = false;
        let names = a.interface_names.borrow().clone();
        if let Some(names) = names {
            for n in &names {
                if n.as_deref() == Some(interface_name) {
                    result = true;
                } else {
                    let interface_type = self.lookup(n.as_deref()).expect("NullPointerException");
                    if interface_type.is_assignable_to(Some(interface_name)) {
                        result = true;
                    }
                }
            }
        }
        if !result {
            if let Some(super_type) = self.lookup(self.get_super_type_name()) {
                result = super_type.implements_interface(interface_name);
            }
        }
        result
    }

    pub fn is_subclass_of(&self, base_name: &str) -> bool {
        if self.as3.is_none() || base_name == "*" {
            false
        } else {
            self.is_assignable_to(Some(base_name))
        }
    }

    pub fn is_interface(&self) -> bool {
        match &self.as3 {
            Some(a) => a.cframe.borrow().is_interface(),
            None => {
                debug_assert!(false);
                false
            }
        }
    }

    pub fn is_public(&self) -> bool {
        match &self.as3 {
            Some(a) => a.cframe.borrow().name().ns().expect("NullPointerException").get_namespace_kind() == NS_PUBLIC,
            None => true,
        }
    }

    pub fn is_dynamic(&self) -> bool {
        match &self.as3 {
            Some(a) => a.cframe.borrow().prototype().expect("NullPointerException").is_dynamic(),
            None => true,
        }
    }

    pub fn is_assignable_to(&self, base_name: Option<&str>) -> bool {
        if base_name == Some("*") || base_name == Some(self.get_name()) {
            return true;
        }
        let super_type_name = self.get_super_type_name();
        if super_type_name.is_some() && super_type_name == base_name {
            return true;
        }
        let interface_names = self.get_interface_names();
        for n in interface_names.iter().flatten() {
            if base_name.is_some() && base_name == n.as_deref() {
                return true;
            }
        }
        if super_type_name.is_some() {
            let super_type = self.lookup(super_type_name).expect("NullPointerException");
            if super_type.is_assignable_to(base_name) {
                return true;
            }
        }
        for n in interface_names.iter().flatten() {
            if n.is_some() {
                let interface_type = self.lookup(n.as_deref()).expect("NullPointerException");
                if interface_type.is_assignable_to(base_name) {
                    return true;
                }
            }
        }
        false
    }

    /// `setTypeTable(typeTable)`.
    pub fn set_type_table(&self, type_table: TypeTable) {
        if let Some(a) = &self.as3 {
            *a.type_table.borrow_mut() = Some(type_table);
        }
    }

    /// `getVarIterator()`: class members first, then instance members.
    pub fn get_var_iterator(&self) -> Vec<Variable> {
        let mut result = Vec::new();
        let Some(a) = &self.as3 else { return result };
        let cframe = a.cframe.borrow().clone();
        let cx = null_context(&a.cx_statics);
        let frames: [ObjectValue; 2] = [(*cframe).clone(), cframe.prototype().expect("NullPointerException")];
        for frame in &frames {
            let Some(names) = frame.builder().expect("NullPointerException").get_names() else { continue };
            let mut i = 0;
            loop {
                i = names.has_next(i);
                if i == -1 {
                    break;
                }
                let iu = i as usize;
                if names.get_type(iu) == Names::GET_NAMES {
                    if let Some(s) = frame.get_slot(&cx, names.get_slot(iu)) {
                        if s.is_variable_slot() {
                            result.push(Variable::new(s, names.get_namespace(iu), &names.get_name(iu)));
                        }
                    }
                }
                i += 1;
            }
        }
        result
    }

    fn method_iterator(&self, kind: i32) -> Vec<Method> {
        let mut result = Vec::new();
        let Some(a) = &self.as3 else { return result };
        let (getters, kind) = if kind == Names::METHOD_NAMES {
            (false, Names::GET_NAMES)
        } else if kind == Names::GET_NAMES {
            (true, kind)
        } else {
            (false, kind)
        };
        let cframe = a.cframe.borrow().clone();
        let cx = null_context(&a.cx_statics);
        let frames: [ObjectValue; 2] = [(*cframe).clone(), cframe.prototype().expect("NullPointerException")];
        for frame in &frames {
            let Some(names) = frame.builder().expect("NullPointerException").get_names() else { continue };
            let mut i = 0;
            loop {
                i = names.has_next(i);
                if i == -1 {
                    break;
                }
                let iu = i as usize;
                if names.get_type(iu) == kind {
                    if let Some(s) = frame.get_slot(&cx, names.get_slot(iu)) {
                        if s.is_method_slot() && getters == s.is_getter() {
                            result.push(Method::new(s, names.get_namespace(iu), &names.get_name(iu)));
                        }
                    }
                }
                i += 1;
            }
        }
        result
    }

    pub fn get_method_iterator(&self) -> Vec<Method> {
        self.method_iterator(Names::METHOD_NAMES)
    }

    pub fn get_getter_iterator(&self) -> Vec<Method> {
        self.method_iterator(Names::GET_NAMES)
    }

    pub fn get_setter_iterator(&self) -> Vec<Method> {
        self.method_iterator(Names::SET_NAMES)
    }
}
