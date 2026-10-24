//! Port of `macromedia/asc/semantics/TypeInfo.java`: a type plus nullability, shared by reference like the Java object.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use crate::asc::semantics::builder::Builder;
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::q_name::QName;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::util::context::Context;

pub struct TypeInfoData {
    is_nullable: bool,
    is_default: bool,
    #[allow(dead_code)]
    annotate_names: bool,
    type_: Option<TypeValue>,
    prototype: Option<ObjectValue>,
    name: Option<QName>,
}

#[derive(Clone)]
pub struct TypeInfo(Rc<RefCell<TypeInfoData>>);

impl PartialEq for TypeInfo {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl TypeInfo {
    /// `new TypeInfo(type)`.
    pub fn of(type_: Option<&TypeValue>) -> TypeInfo {
        TypeInfo(Rc::new(RefCell::new(TypeInfoData {
            is_nullable: true,
            is_default: false,
            annotate_names: false,
            type_: type_.cloned(),
            prototype: None,
            name: None,
        })))
    }

    /// `new TypeInfo(type, nullable, is_default)`.
    pub fn new(type_: Option<&TypeValue>, nullable: bool, is_default: bool) -> TypeInfo {
        let prototype = type_.and_then(|t| t.prototype());
        TypeInfo(Rc::new(RefCell::new(TypeInfoData {
            is_nullable: nullable,
            is_default,
            annotate_names: false,
            type_: type_.cloned(),
            prototype,
            name: None,
        })))
    }

    /// Java `==`.
    pub fn ptr_eq(&self, other: &TypeInfo) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn clear_instance(&self) {
        let mut d = self.0.borrow_mut();
        d.prototype = None;
        d.name = None;
    }

    pub fn is_nullable(&self) -> bool {
        self.0.borrow().is_nullable
    }

    pub fn get_type_value(&self) -> Option<TypeValue> {
        self.0.borrow().type_.clone()
    }

    /// `getTypeValue()` where Java dereferences it straight away.
    pub fn type_value(&self) -> TypeValue {
        self.get_type_value().expect("NullPointerException")
    }

    pub fn set_is_nullable(&self, nullable: bool) {
        self.0.borrow_mut().is_nullable = nullable;
    }

    pub fn is_numeric(&self, cx: &Context) -> bool {
        let t = self.get_type_value();
        let is = |o: TypeValue| t.as_ref().map_or(false, |t| t.ptr_eq(&o));
        is(cx.int_type())
            || is(cx.uint_type())
            || is(cx.double_type())
            || is(cx.number_type())
            || (cx.statics.borrow().es4_numerics && cx.decimal_type().map_or(t.is_none(), is))
    }

    pub fn get_type_id(&self) -> i32 {
        self.type_value().get_type_id()
    }

    pub fn includes(&self, cx: &Context, other_type: &TypeInfo) -> bool {
        if cx.statics.borrow().es4_nullability && !self.is_nullable() && other_type.is_nullable() {
            false
        } else {
            self.type_value().includes(cx, other_type.get_type_value().as_ref())
        }
    }

    pub fn get_name(&self) -> QName {
        if let Some(n) = self.0.borrow().name.clone() {
            return n;
        }
        let n = self.type_value().name();
        self.0.borrow_mut().name = Some(n.clone());
        n
    }

    /// `getName(cx)`: with ES4 nullability, a non-default type info's name carries `?` or `!`.
    pub fn get_name_cx(&self, cx: &Context) -> QName {
        if let Some(n) = self.0.borrow().name.clone() {
            return n;
        }
        let mut n = self.type_value().name();
        let (is_default, is_nullable) = {
            let d = self.0.borrow();
            (d.is_default, d.is_nullable)
        };
        if cx.statics.borrow().es4_nullability && !is_default {
            n = QName::new(n.ns(), &format!("{}{}", n.name(), if is_nullable { "?" } else { "!" }));
        }
        self.0.borrow_mut().name = Some(n.clone());
        n
    }

    pub fn get_builder(&self) -> Option<Builder> {
        self.type_value().builder()
    }

    pub fn get_prototype(&self) -> Option<ObjectValue> {
        if self.0.borrow().prototype.is_none() {
            let p = self.get_type_value().and_then(|t| t.prototype());
            self.0.borrow_mut().prototype = p;
        }
        self.0.borrow().prototype.clone()
    }

    pub fn is_interface(&self) -> bool {
        self.type_value().is_interface()
    }

    pub fn set_prototype(&self, proto: Option<ObjectValue>) {
        self.0.borrow_mut().prototype = proto;
    }
}

/// `Object.toString()`; Java prints an identity hash.
impl fmt::Display for TypeInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "macromedia.asc.semantics.TypeInfo@{:x}", Rc::as_ptr(&self.0) as *const u8 as usize)
    }
}

impl fmt::Debug for TypeInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.try_borrow() {
            Ok(d) => write!(f, "TypeInfo({:?})", d.type_),
            Err(_) => write!(f, "TypeInfo(<borrowed>)"),
        }
    }
}
