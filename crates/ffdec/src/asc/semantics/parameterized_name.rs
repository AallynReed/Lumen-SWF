//! Port of `macromedia/asc/semantics/ParameterizedName.java`: a `QName` with type parameters, e.g. `Vector.<int>`.

use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::q_name::QName;
use crate::asc::semantics::type_value::TypeValue;

/// `new ParameterizedName(ns, name, ObjectList<TypeValue> type_params)`.
pub fn new_parameterized_name_from_types(ns: Option<ObjectValue>, name: &str, type_params: &[TypeValue]) -> QName {
    QName::new_parameterized(ns, name, type_params.iter().map(|t| t.name()).collect())
}

/// `new ParameterizedName(ObjectList<QName> type_names, ns, name)`.
pub fn new_parameterized_name(type_names: Vec<QName>, ns: Option<ObjectValue>, name: &str) -> QName {
    QName::new_parameterized(ns, name, type_names)
}

impl QName {
    /// `ParameterizedName.equals`, including Java's comparison of every parameter against `rp.type_params.at(1)`.
    pub(crate) fn parameterized_equals(&self, rhs: &QName) -> bool {
        if rhs.is_parameterized_name() && self.qname_equals(rhs) {
            let tp = self.type_params().unwrap();
            let rtp = rhs.type_params().unwrap();
            if rtp.len() == tp.len() {
                for p in tp {
                    if !p.java_equals(rtp.get(1).expect("IndexOutOfBoundsException")) {
                        return false;
                    }
                }
                return true;
            }
        }
        false
    }

    /// `getNamePart()`: e.g. `Vector.<int>`.
    pub fn get_name_part(&self) -> String {
        self.namepart_cell()
            .get_or_init(|| {
                let mut namepart = self.name().to_string();
                namepart.push_str(".<");
                for p in self.type_params().expect("ClassCastException") {
                    namepart.push_str(&p.to_string());
                }
                namepart.push('>');
                namepart
            })
            .clone()
    }

    pub(crate) fn parameterized_to_string(&self) -> String {
        if let Some(s) = self.fullname_cell().get() {
            return s.clone();
        }
        let s = match self.ns() {
            Some(ns) if !ns.name_ref().is_empty() => format!("{}:{}", ns.name_ref(), self.get_name_part()),
            _ => self.get_name_part(),
        };
        self.fullname_cell().get_or_init(|| s).clone()
    }
}
