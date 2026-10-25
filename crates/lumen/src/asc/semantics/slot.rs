//! Port of `macromedia/asc/semantics/Slot.java`: the compile-time description of a property's storage or code. The
//! abstract methods are implemented per `kind` in `variable_slot.rs` and `method_slot.rs`.

#![allow(non_upper_case_globals)]

use std::any::Any;
use std::cell::{Ref, RefCell, RefMut};
use std::fmt;
use std::rc::Rc;

use crate::asc::parser::node::NodeRef;
use crate::asc::parser::tokens::*;
use crate::asc::semantics::meta_data::MetaData;
use crate::asc::semantics::method_slot::MethodSlotData;
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::reference_value::ReferenceValue;
use crate::asc::semantics::type_info::TypeInfo;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::semantics::value::Value;
use crate::asc::semantics::variable_slot::VariableSlotData;
use crate::asc::util::bit_set::BitSet;
use crate::asc::util::byte_list::ByteList;
use crate::asc::util::context::Context;

pub const PUSH_none: i32 = 0;
pub const PUSH_env: i32 = 0x1 << 0;
pub const PUSH_this: i32 = 0x1 << 1;
pub const PUSH_opd1: i32 = 0x1 << 2;
pub const PUSH_opd2: i32 = 0x1 << 3;
pub const PUSH_args: i32 = 0x1 << 4;
pub const PUSH_size: i32 = 0x1 << 5;

pub const CALL_Empty: i32 = PUSH_none;
pub const CALL_Unary: i32 = PUSH_opd1 | PUSH_args;
pub const CALL_ThisUnary: i32 = PUSH_this | PUSH_opd1 | PUSH_args;
pub const CALL_Binary: i32 = PUSH_opd1 | PUSH_opd2;
pub const CALL_Method: i32 = PUSH_args;
pub const CALL_ThisMethod: i32 = PUSH_this | PUSH_args;
pub const CALL_Closure: i32 = PUSH_env | PUSH_this | PUSH_args | PUSH_size;

pub const CALL_Args: i32 = PUSH_args;
pub const CALL_ThisArgs: i32 = PUSH_this | PUSH_args;
pub const CALL_EnvThisArgs: i32 = PUSH_env | PUSH_this | PUSH_args | PUSH_size;

pub const DISPATCH_final: i32 = 0;
pub const DISPATCH_virtual: i32 = 1;

pub const DISP_undefined: i32 = DISPATCH_virtual;
pub const DISP_CallProperty: i32 = DISP_undefined + 1;
pub const DISP_ConstructProperty: i32 = DISP_CallProperty + 1;
pub const DISP_CallClosure: i32 = DISP_ConstructProperty + 1;
pub const DISP_ConstructClosure: i32 = DISP_CallClosure + 1;
pub const DISP_CallPropLex: i32 = DISP_ConstructClosure + 1;
pub const DISP_CallFinal: i32 = DISPATCH_final;

pub const THIS_undefined: i32 = 0;
pub const THIS_Global: i32 = THIS_undefined + 1;
pub const THIS_Scope: i32 = THIS_Global + 1;
pub const THIS_Base: i32 = THIS_Scope + 1;
pub const THIS_Temp: i32 = THIS_Base + 1;
pub const THIS_None: i32 = THIS_Temp + 1;

pub const PARAM_Required: i32 = 0;
pub const PARAM_Optional: i32 = PARAM_Required + 1;
pub const PARAM_Rest: i32 = PARAM_Optional + 1;
pub const PARAM_Void: i32 = PARAM_Rest + 1;

const FINAL_Flag: i32 = 1;
const OVERRIDE_Flag: i32 = 2;
const CONST_Flag: i32 = 4;
const IMPORTED_Flag: i32 = 8;
const INTRINSIC_Flag: i32 = 16;
const NEEDS_INIT_Flag: i32 = 32;
const METHOD_Flag: i32 = 64;

const CALL_SEQUENCE_Mask: i32 = 0x0000FF00;
const DISPATCH_KIND_Mask: i32 = 0x00FF0000;
const CALL_SEQUENCE_Shift: i32 = 8;
const DISPATCH_KIND_Shift: i32 = 16;

/// `Slot.Overload`, a `HashMap<TypeValue, Integer>` in Java; kept in insertion order here. Java iterates it in
/// identity-hash order, but `dispatch` cannot tell the difference because overload keys have distinct type ids.
pub type Overload = Vec<(TypeValue, i32)>;

/// An `ObjectList<TypeInfo>` that Java shares between slots (`setTypes(other.getTypes())`).
pub type TypeInfos = Rc<RefCell<Vec<TypeInfo>>>;
/// A `ByteList` of `PARAM_*` styles, shared like `TypeInfos`.
pub type DeclStyles = Rc<RefCell<ByteList>>;

pub enum SlotKind {
    Variable(VariableSlotData),
    Method(MethodSlotData),
}

/// The fields of `Slot`. Java keeps the rarely-used ones in an `auxDataItems` array; here each is a field.
pub struct SlotData {
    flags: i32,
    pub id: i32,
    pub declared_by: Option<ObjectValue>,
    def_bits: Option<BitSet>,
    value: Option<Value>,
    type_: Option<TypeInfo>,
    types: Option<TypeInfos>,
    version: i8,
    overridden_slot: Option<Slot>,
    embedded_data: Option<Rc<dyn Any>>,
    metadata: Option<Vec<MetaData>>,
    base_node: Option<NodeRef>,
    impl_node: Option<NodeRef>,
    unary_overloads: Option<Overload>,
    binary_overloads: Option<Vec<(TypeValue, Overload)>>,
    debug_name: Option<String>,
    implicit_call: Option<i32>,
    implicit_construct: Option<i32>,
    pub kind: SlotKind,
}

#[derive(Clone)]
pub struct Slot(pub(crate) Rc<RefCell<SlotData>>);

impl PartialEq for Slot {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

fn overload_get(o: &Overload, t: &TypeValue) -> Option<i32> {
    o.iter().find(|(k, _)| k.ptr_eq(t)).map(|(_, v)| *v)
}

fn overload_put(o: &mut Overload, t: &TypeValue, v: i32) {
    match o.iter_mut().find(|(k, _)| k.ptr_eq(t)) {
        Some(e) => e.1 = v,
        None => o.push((t.clone(), v)),
    }
}

impl Slot {
    /// `Slot(TypeInfo type, int id)`, for the subclass constructors.
    pub(crate) fn construct(type_: Option<TypeInfo>, id: i32, kind: SlotKind) -> Slot {
        let s = Slot(Rc::new(RefCell::new(SlotData {
            flags: OVERRIDE_Flag,
            id,
            declared_by: None,
            def_bits: None,
            value: None,
            type_,
            types: None,
            version: 0,
            overridden_slot: None,
            embedded_data: None,
            metadata: None,
            base_node: None,
            impl_node: None,
            unary_overloads: None,
            binary_overloads: None,
            debug_name: None,
            implicit_call: None,
            implicit_construct: None,
            kind,
        })));
        s.set_call_sequence(CALL_ThisUnary);
        s.set_dispatch_kind(DISPATCH_final);
        s
    }

    pub fn ptr_eq(&self, other: &Slot) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn addr(&self) -> usize {
        Rc::as_ptr(&self.0) as *const u8 as usize
    }

    pub fn data(&self) -> Ref<'_, SlotData> {
        self.0.borrow()
    }

    pub fn data_mut(&self) -> RefMut<'_, SlotData> {
        self.0.borrow_mut()
    }

    pub fn is_variable_slot(&self) -> bool {
        matches!(self.0.borrow().kind, SlotKind::Variable(_))
    }

    pub fn is_method_slot(&self) -> bool {
        matches!(self.0.borrow().kind, SlotKind::Method(_))
    }

    pub fn id(&self) -> i32 {
        self.0.borrow().id
    }

    pub fn set_id(&self, id: i32) {
        self.0.borrow_mut().id = id;
    }

    pub fn declared_by(&self) -> Option<ObjectValue> {
        self.0.borrow().declared_by.clone()
    }

    pub fn set_declared_by(&self, ob: Option<ObjectValue>) {
        self.0.borrow_mut().declared_by = ob;
    }

    pub fn is_compatible(&self, t1: Option<&TypeValue>, t2: Option<&TypeValue>) -> bool {
        match (t1, t2) {
            (Some(a), Some(b)) => (a.get_type_id() & b.get_type_id()) != 0,
            _ => false,
        }
    }

    pub fn add_def_bits(&self, mask: Option<BitSet>) {
        let bits = BitSet::set(self.get_def_bits(), mask);
        self.set_def_bits(bits);
    }

    /// Records the slot of the implied call (`EMPTY_TOKEN`) or construct (`NEW_TOKEN`).
    pub fn implicit(&self, kind: i32, slot_index: i32) {
        let mut d = self.0.borrow_mut();
        match kind {
            EMPTY_TOKEN => d.implicit_call = Some(slot_index),
            NEW_TOKEN => d.implicit_construct = Some(slot_index),
            _ => {}
        }
    }

    pub fn implies(&self, _cx: &Context, kind: i32) -> i32 {
        let d = self.0.borrow();
        let value = match kind {
            EMPTY_TOKEN => d.implicit_call,
            NEW_TOKEN => d.implicit_construct,
            _ => None,
        };
        value.unwrap_or(0)
    }

    /// `overload(t1, slot_index)`.
    pub fn overload(&self, t1: &TypeValue, slot_index: i32) {
        let mut d = self.0.borrow_mut();
        overload_put(d.unary_overloads.get_or_insert_with(Vec::new), t1, slot_index);
    }

    /// `overload(t1, t2, slot_index)`.
    pub fn overload2(&self, t1: &TypeValue, t2: &TypeValue, slot_index: i32) {
        let mut o = Overload::new();
        overload_put(&mut o, t2, slot_index);
        let mut d = self.0.borrow_mut();
        let b = d.binary_overloads.get_or_insert_with(Vec::new);
        match b.iter_mut().find(|(k, _)| k.ptr_eq(t1)) {
            Some(e) => e.1 = o,
            None => b.push((t1.clone(), o)),
        }
    }

    /// `dispatch(cx, t1)`: the unary overload for the operand type, 0 if none or ambiguous.
    pub fn dispatch(&self, _cx: &Context, t1: Option<&TypeValue>) -> i32 {
        let mut found = false;
        let mut value = 0;
        let overloads = self.0.borrow().unary_overloads.clone();
        if let Some(unary_overloads) = overloads {
            for (t, v) in &unary_overloads {
                if t1.map_or(false, |t1| t.ptr_eq(t1)) {
                    return *v;
                } else if self.is_compatible(Some(t), t1) {
                    if !found {
                        found = true;
                        value = *v;
                    } else {
                        return 0;
                    }
                }
            }
        }
        value
    }

    /// `dispatch(cx, t1, t2)`: the binary overload, caching misses as 0 as Java does.
    pub fn dispatch2(&self, cx: &Context, t1: &TypeValue, t2: &TypeValue) -> i32 {
        let binary = self.0.borrow().binary_overloads.clone();
        if let Some(binary_overloads) = &binary {
            for (i, overload) in binary_overloads {
                if self.is_compatible(Some(i), Some(t1)) {
                    for (j, v) in overload {
                        if self.is_compatible(Some(j), Some(t2)) {
                            return *v;
                        }
                    }
                }
            }
        }
        let _ = cx;
        let mut d = self.0.borrow_mut();
        let b = d.binary_overloads.get_or_insert_with(Vec::new);
        match b.iter_mut().find(|(k, _)| k.ptr_eq(t1)) {
            None => {
                let mut o = Overload::new();
                overload_put(&mut o, t2, 0);
                b.push((t1.clone(), o));
            }
            Some((_, o)) => {
                if overload_get(o, t2).is_none() {
                    overload_put(o, t2, 0);
                }
            }
        }
        let o = &b.iter().find(|(k, _)| k.ptr_eq(t1)).unwrap().1;
        overload_get(o, t2).unwrap()
    }

    pub fn attrs(&self, call_seq: i32, method_id: i32) {
        self.set_call_sequence(call_seq as i8 as i32);
        self.set_method_id(method_id);
    }

    pub fn add_metadata(&self, meta: Option<MetaData>) {
        let meta = match meta {
            Some(m) => m,
            None => return,
        };
        self.0.borrow_mut().metadata.get_or_insert_with(Vec::new).push(meta);
    }

    fn flag(&self, f: i32) -> bool {
        (self.0.borrow().flags & f) != 0
    }

    fn set_flag(&self, f: i32, on: bool) {
        let mut d = self.0.borrow_mut();
        d.flags = if on { d.flags | f } else { d.flags & !f };
    }

    pub fn is_final(&self) -> bool {
        self.flag(FINAL_Flag)
    }

    pub fn set_final(&self, is_final: bool) {
        self.set_flag(FINAL_Flag, is_final);
    }

    pub fn is_override(&self) -> bool {
        self.flag(OVERRIDE_Flag)
    }

    pub fn set_override(&self, is_override: bool) {
        self.set_flag(OVERRIDE_Flag, is_override);
    }

    pub fn is_const(&self) -> bool {
        self.flag(CONST_Flag)
    }

    pub fn set_const(&self, is_const: bool) {
        self.set_flag(CONST_Flag, is_const);
    }

    pub fn is_imported(&self) -> bool {
        self.flag(IMPORTED_Flag)
    }

    pub fn set_imported(&self, is_imported: bool) {
        self.set_flag(IMPORTED_Flag, is_imported);
    }

    pub fn is_intrinsic(&self) -> bool {
        self.flag(INTRINSIC_Flag)
    }

    pub fn needs_init(&self) -> bool {
        self.flag(NEEDS_INIT_Flag)
    }

    pub fn set_needs_init(&self, needs_init: bool) {
        self.set_flag(NEEDS_INIT_Flag, needs_init);
    }

    pub fn set_intrinsic(&self, is_intrinsic: bool) {
        self.set_flag(INTRINSIC_Flag, is_intrinsic);
    }

    pub fn get_call_sequence(&self) -> i32 {
        (self.0.borrow().flags & CALL_SEQUENCE_Mask) >> CALL_SEQUENCE_Shift
    }

    pub fn set_call_sequence(&self, call_seq: i32) {
        let mut d = self.0.borrow_mut();
        d.flags &= !CALL_SEQUENCE_Mask;
        d.flags |= call_seq << CALL_SEQUENCE_Shift;
    }

    pub fn get_dispatch_kind(&self) -> i32 {
        (self.0.borrow().flags & DISPATCH_KIND_Mask) >> DISPATCH_KIND_Shift
    }

    pub fn set_dispatch_kind(&self, dispatch_kind: i32) {
        let mut d = self.0.borrow_mut();
        d.flags &= !DISPATCH_KIND_Mask;
        d.flags |= dispatch_kind << DISPATCH_KIND_Shift;
    }

    pub fn set_base_node(&self, base_node: Option<NodeRef>) {
        self.0.borrow_mut().base_node = base_node;
    }

    pub fn get_base_node(&self) -> Option<NodeRef> {
        self.0.borrow().base_node.clone()
    }

    pub fn set_impl_node(&self, impl_node: Option<NodeRef>) {
        self.0.borrow_mut().impl_node = impl_node;
    }

    pub fn get_impl_node(&self) -> Option<NodeRef> {
        self.0.borrow().impl_node.clone()
    }

    pub fn set_metadata(&self, metadata: Option<Vec<MetaData>>) {
        self.0.borrow_mut().metadata = metadata;
    }

    pub fn get_metadata(&self) -> Option<Vec<MetaData>> {
        self.0.borrow().metadata.clone()
    }

    pub fn set_embedded_data(&self, embedded_data: Option<Rc<dyn Any>>) {
        self.0.borrow_mut().embedded_data = embedded_data;
    }

    pub fn get_embedded_data(&self) -> Option<Rc<dyn Any>> {
        self.0.borrow().embedded_data.clone()
    }

    pub fn set_overridden_slot(&self, overridden_slot: Option<Slot>) {
        self.0.borrow_mut().overridden_slot = overridden_slot;
    }

    pub fn get_overridden_slot(&self) -> Option<Slot> {
        self.0.borrow().overridden_slot.clone()
    }

    pub fn set_object_value(&self, object_value: Option<ObjectValue>) {
        self.set_value(object_value.map(Value::Object));
    }

    /// `getObjectValue()`: `(ObjectValue)getValue()`.
    pub fn get_object_value(&self) -> Option<ObjectValue> {
        self.get_value().map(|v| v.as_object_value().expect("ClassCastException"))
    }

    /// `getInitializerValue()`: the value even when the slot is not const.
    pub fn get_initializer_value(&self) -> Option<ObjectValue> {
        self.0.borrow().value.clone().map(|v| v.as_object_value().expect("ClassCastException"))
    }

    pub fn set_debug_name(&self, debug_name: &str) {
        self.0.borrow_mut().debug_name = Some(debug_name.to_string());
    }

    pub fn get_debug_name(&self) -> String {
        self.0.borrow().debug_name.clone().unwrap_or_default()
    }

    pub fn is_getter(&self) -> bool {
        (self.0.borrow().flags & METHOD_Flag) == 0
    }

    pub fn set_getter(&self, is_getter: bool) {
        self.set_flag(METHOD_Flag, !is_getter);
    }

    pub fn set_var_index(&self, var_index: i32) {
        match &mut self.0.borrow_mut().kind {
            SlotKind::Variable(v) => v.var_index = var_index,
            SlotKind::Method(_) => panic!("IllegalArgumentException"),
        }
    }

    pub fn get_var_index(&self) -> i32 {
        match &self.0.borrow().kind {
            SlotKind::Variable(v) => v.var_index,
            SlotKind::Method(_) => -1,
        }
    }

    pub fn set_method_id(&self, method_id: i32) {
        match &mut self.0.borrow_mut().kind {
            SlotKind::Variable(_) => {
                if method_id != -1 {
                    panic!("IllegalArgumentException");
                }
            }
            SlotKind::Method(m) => m.method_id = method_id,
        }
    }

    pub fn get_method_id(&self) -> i32 {
        match &self.0.borrow().kind {
            SlotKind::Variable(_) => -1,
            SlotKind::Method(m) => m.method_id,
        }
    }

    pub fn set_method_name(&self, method_name: &str) {
        match &mut self.0.borrow_mut().kind {
            SlotKind::Variable(v) => v.method_name = Some(method_name.to_string()),
            SlotKind::Method(m) => m.method_name = method_name.to_string(),
        }
    }

    pub fn get_method_name(&self) -> String {
        match &self.0.borrow().kind {
            SlotKind::Variable(v) => v.method_name.clone().unwrap_or_default(),
            SlotKind::Method(m) => m.method_name.clone(),
        }
    }

    pub fn set_type_ref(&self, typeref: Option<ReferenceValue>) {
        match &mut self.0.borrow_mut().kind {
            SlotKind::Variable(v) => v.typeref = typeref,
            SlotKind::Method(_) => panic!("IllegalArgumentException"),
        }
    }

    pub fn get_type_ref(&self) -> Option<ReferenceValue> {
        match &self.0.borrow().kind {
            SlotKind::Variable(v) => v.typeref.clone(),
            SlotKind::Method(_) => None,
        }
    }

    pub fn set_decl_styles(&self, decl_styles: Option<DeclStyles>) {
        match &mut self.0.borrow_mut().kind {
            SlotKind::Variable(_) => panic!("IllegalArgumentException"),
            SlotKind::Method(m) => m.decl_styles = decl_styles,
        }
    }

    pub fn get_decl_styles(&self) -> Option<DeclStyles> {
        match &self.0.borrow().kind {
            SlotKind::Variable(_) => None,
            SlotKind::Method(m) => m.decl_styles.clone(),
        }
    }

    pub fn add_decl_style(&self, style: i32) {
        if let SlotKind::Method(m) = &mut self.0.borrow_mut().kind {
            m.decl_styles.get_or_insert_with(|| Rc::new(RefCell::new(ByteList::with_capacity(2)))).borrow_mut().push(style as u8);
        }
    }

    pub fn set_type(&self, type_: Option<TypeInfo>) {
        let mut d = self.0.borrow_mut();
        if let SlotKind::Variable(v) = &mut d.kind {
            v.typeref = None;
        }
        d.type_ = type_;
    }

    pub fn get_type(&self) -> Option<TypeInfo> {
        self.0.borrow().type_.clone()
    }

    pub fn set_types(&self, types: Option<TypeInfos>) {
        self.0.borrow_mut().types = types;
    }

    pub fn get_types(&self) -> Option<TypeInfos> {
        self.0.borrow().types.clone()
    }

    pub fn add_type(&self, type_: TypeInfo) {
        let mut d = self.0.borrow_mut();
        d.types.get_or_insert_with(|| Rc::new(RefCell::new(Vec::with_capacity(2)))).borrow_mut().push(type_);
    }

    pub fn set_value(&self, value: Option<Value>) {
        self.0.borrow_mut().value = value;
    }

    /// `getValue()`; a variable slot only reports its value when it is const.
    pub fn get_value(&self) -> Option<Value> {
        let d = self.0.borrow();
        match &d.kind {
            SlotKind::Variable(_) if (d.flags & CONST_Flag) == 0 => None,
            _ => d.value.clone(),
        }
    }

    pub fn set_def_bits(&self, def_bits: Option<BitSet>) {
        self.0.borrow_mut().def_bits = def_bits;
    }

    pub fn get_def_bits(&self) -> Option<BitSet> {
        self.0.borrow().def_bits.clone()
    }

    pub fn set_version(&self, version: i8) {
        self.0.borrow_mut().version = version;
    }

    pub fn get_version(&self) -> i8 {
        self.0.borrow().version
    }
}

impl SlotData {
    /// The `value` field itself, whatever the slot kind.
    pub fn kind_value(&self) -> Option<Value> {
        self.value.clone()
    }
}

impl fmt::Debug for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.try_borrow() {
            Ok(d) => write!(f, "Slot({}, {})", d.id, if matches!(d.kind, SlotKind::Variable(_)) { "var" } else { "method" }),
            Err(_) => write!(f, "Slot(<borrowed>)"),
        }
    }
}
