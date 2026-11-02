//! Port of `flex2.compiler.mxml.rep.VariableDeclaration`.

use std::rc::Rc;

use crate::asc::flex2::velocity::{JType, JavaException, JavaObject, Method, Value};

#[derive(Clone, Debug)]
pub struct VariableDeclaration {
    namespace: String,
    name: String,
    type_: String,
    initializer: String,
}

impl VariableDeclaration {
    pub fn new(namespace: &str, name: &str, type_: &str, initializer: &str) -> VariableDeclaration {
        VariableDeclaration { namespace: namespace.into(), name: name.into(), type_: type_.into(), initializer: initializer.into() }
    }

    pub fn get_namespace(&self) -> &str {
        &self.namespace
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_type(&self) -> &str {
        &self.type_
    }

    pub fn get_initializer(&self) -> &str {
        &self.initializer
    }

    pub fn value(&self) -> Value {
        Value::Object(Rc::new(self.clone()))
    }
}

static METHODS: &[Method] = &[
    Method::new("getNamespace", &[], JType::String),
    Method::new("getName", &[], JType::String),
    Method::new("getType", &[], JType::String),
    Method::new("getInitializer", &[], JType::String),
];

impl JavaObject for VariableDeclaration {
    fn class_name(&self) -> &str {
        "flex2.compiler.mxml.rep.VariableDeclaration"
    }

    fn methods(&self) -> &[Method] {
        METHODS
    }

    fn invoke(&self, method: &Method, _args: &[Value]) -> Result<Value, JavaException> {
        Ok(match &*method.name {
            "getNamespace" => Value::str(&self.namespace),
            "getName" => Value::str(&self.name),
            "getType" => Value::str(&self.type_),
            "getInitializer" => Value::str(&self.initializer),
            _ => Value::Null,
        })
    }
}
