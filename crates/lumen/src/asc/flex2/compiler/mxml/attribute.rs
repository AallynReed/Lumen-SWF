//! Port of `flex2.compiler.mxml.Attribute`; its `Object` value is a string or an `AtResource`.

use std::rc::Rc;

use super::rep::at_resource::AtResource;

#[derive(Clone, Debug)]
pub enum AttrValue {
    Str(String),
    AtResource(Rc<AtResource>),
}

impl AttrValue {
    /// `(String) value`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            AttrValue::Str(s) => Some(s),
            AttrValue::AtResource(_) => None,
        }
    }

    /// `value.toString()`.
    pub fn to_java_string(&self) -> String {
        match self {
            AttrValue::Str(s) => s.clone(),
            AttrValue::AtResource(r) => crate::asc::flex2::velocity::JavaObject::to_java_string(r.as_ref()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Attribute {
    uri: String,
    local_part: String,
    value: AttrValue,
    line: i32,
}

impl Attribute {
    pub(super) fn new(uri: &str, local_part: &str, value: AttrValue, line: i32) -> Attribute {
        Attribute { uri: uri.to_string(), local_part: local_part.to_string(), value, line }
    }

    pub fn get_local_part(&self) -> &str {
        &self.local_part
    }

    pub fn get_namespace(&self) -> &str {
        &self.uri
    }

    pub fn get_value(&self) -> &AttrValue {
        &self.value
    }

    pub fn get_line(&self) -> i32 {
        self.line
    }
}
