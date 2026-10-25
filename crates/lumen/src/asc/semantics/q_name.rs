//! Port of `macromedia/asc/semantics/QName.java`. A `QName` with `type_params` is a `ParameterizedName`
//! (see `parameterized_name.rs`).

use std::cell::OnceCell;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::asc::semantics::object_value::ObjectValue;

pub struct QNameData {
    pub ns: Option<ObjectValue>,
    pub name: String,
    /// `ParameterizedName.type_params`.
    pub type_params: Option<Vec<QName>>,
    fullname: OnceCell<String>,
    namepart: OnceCell<String>,
}

#[derive(Clone)]
pub struct QName(Rc<QNameData>);

impl QName {
    pub fn new(ns: Option<ObjectValue>, name: &str) -> QName {
        QName(Rc::new(QNameData { ns, name: name.to_string(), type_params: None, fullname: OnceCell::new(), namepart: OnceCell::new() }))
    }

    pub(crate) fn new_parameterized(ns: Option<ObjectValue>, name: &str, type_params: Vec<QName>) -> QName {
        QName(Rc::new(QNameData {
            ns,
            name: name.to_string(),
            type_params: Some(type_params),
            fullname: OnceCell::new(),
            namepart: OnceCell::new(),
        }))
    }

    pub fn ns(&self) -> Option<ObjectValue> {
        self.0.ns.clone()
    }

    pub fn name(&self) -> &str {
        &self.0.name
    }

    pub fn ptr_eq(&self, other: &QName) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    /// `instanceof ParameterizedName`.
    pub fn is_parameterized_name(&self) -> bool {
        self.0.type_params.is_some()
    }

    pub fn type_params(&self) -> Option<&Vec<QName>> {
        self.0.type_params.as_ref()
    }

    /// `equals`, which is `QName.equals` or `ParameterizedName.equals` depending on the receiver.
    pub fn java_equals(&self, rhs: &QName) -> bool {
        if self.is_parameterized_name() {
            return self.parameterized_equals(rhs);
        }
        self.qname_equals(rhs)
    }

    pub(crate) fn qname_equals(&self, rhs: &QName) -> bool {
        let ns_eq = match (&self.0.ns, &rhs.0.ns) {
            (Some(a), Some(b)) => a.ptr_eq(b),
            (None, _) => panic!("NullPointerException"),
            (Some(_), None) => false,
        };
        ns_eq && self.0.name == rhs.0.name
    }

    /// `hashCode()`; the namespace part is an identity hash, so this is not Java's value.
    pub fn java_hash_code(&self) -> i32 {
        let ns = self.0.ns.as_ref().expect("NullPointerException").addr() as i32;
        let mut h = ns.wrapping_add(crate::asc::util::jhash::string_hash(&self.0.name));
        if let Some(tp) = &self.0.type_params {
            let mut l: i32 = 1;
            for q in tp {
                l = l.wrapping_mul(31).wrapping_add(q.java_hash_code());
            }
            h = h.wrapping_add(l);
        }
        h
    }

    pub(crate) fn namepart_cell(&self) -> &OnceCell<String> {
        &self.0.namepart
    }

    pub(crate) fn qname_to_string(&self) -> String {
        self.0
            .fullname
            .get_or_init(|| match &self.0.ns {
                Some(ns) if !ns.name_ref().is_empty() => format!("{}:{}", ns.name_ref(), self.0.name),
                _ => self.0.name.clone(),
            })
            .clone()
    }

    pub(crate) fn fullname_cell(&self) -> &OnceCell<String> {
        &self.0.fullname
    }
}

impl PartialEq for QName {
    fn eq(&self, other: &Self) -> bool {
        self.java_equals(other)
    }
}

impl Hash for QName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.ns.as_ref().map(|n| n.addr()).hash(state);
        self.0.name.hash(state);
    }
}

impl fmt::Display for QName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_parameterized_name() {
            f.write_str(&self.parameterized_to_string())
        } else {
            f.write_str(&self.qname_to_string())
        }
    }
}

impl fmt::Debug for QName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QName({})", self)
    }
}
