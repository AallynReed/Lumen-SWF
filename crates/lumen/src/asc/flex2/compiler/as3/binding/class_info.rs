//! Port of `flex2.compiler.as3.binding.ClassInfo`.

use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use super::info::Info;
use crate::asc::flex2::compiler::util::{MultiName, QName};
use crate::asc::util::jhash::JHashMap;

pub type ClassInfoRef = Rc<RefCell<ClassInfo>>;

pub struct ClassInfo {
    info: Info,
    class_name: String,
    variables: Option<Vec<QName>>,
    base_class_info: Option<ClassInfoRef>,
    base_class_name: Option<String>,
    base_class_multi_name: Option<MultiName>,
    skin_parts: Option<JHashMap<String, bool>>,
}

impl Deref for ClassInfo {
    type Target = Info;
    fn deref(&self) -> &Info {
        &self.info
    }
}

impl DerefMut for ClassInfo {
    fn deref_mut(&mut self) -> &mut Info {
        &mut self.info
    }
}

impl ClassInfo {
    pub fn new(class_name: &str) -> ClassInfo {
        let mut c = ClassInfo {
            info: Info::default(),
            class_name: class_name.to_string(),
            variables: None,
            base_class_info: None,
            base_class_name: None,
            base_class_multi_name: None,
            skin_parts: None,
        };
        if let Some(i) = class_name.rfind(':').filter(|&i| i > 0) {
            c.info.add_import(&class_name[..i]);
        }
        c
    }

    pub fn add_variable(&mut self, variable_name: QName) {
        self.variables.get_or_insert_with(Vec::new).push(variable_name);
    }

    pub fn add_skin_part(&mut self, field_name: &str, required: bool) {
        self.skin_parts.get_or_insert_with(JHashMap::new).insert(field_name.to_string(), required);
    }

    /// `definesFunction(functionName, inherited)`.
    pub fn defines_function_inherited(&self, function_name: &str, inherited: bool) -> bool {
        let mut result = self.info.defines_function(function_name);
        if !result && inherited {
            if let Some(b) = &self.base_class_info {
                result = b.borrow().defines_function_inherited(function_name, inherited);
            }
        }
        result
    }

    /// `definesGetter(getterName, inherited)`.
    pub fn defines_getter_inherited(&self, getter_name: &str, inherited: bool) -> bool {
        let mut result = self.info.defines_getter(getter_name);
        if !result && inherited {
            if let Some(b) = &self.base_class_info {
                result = b.borrow().defines_getter_inherited(getter_name, inherited);
            }
        }
        result
    }

    /// `definesSetter(setterName, inherited)`.
    pub fn defines_setter_inherited(&self, setter_name: &str, inherited: bool) -> bool {
        let mut result = self.info.defines_setter(setter_name);
        if !result && inherited {
            if let Some(b) = &self.base_class_info {
                result = b.borrow().defines_setter_inherited(setter_name, inherited);
            }
        }
        result
    }

    pub fn get_class_name(&self) -> &str {
        &self.class_name
    }

    pub fn get_base_class_info(&self) -> Option<ClassInfoRef> {
        self.base_class_info.clone()
    }

    pub fn get_base_class_name(&self) -> Option<&str> {
        self.base_class_name.as_deref()
    }

    /// `getSkinParts(inherited)`: inherited, a new `HashMap` of the local parts, then the base ones over them.
    pub fn get_skin_parts(&self, inherited: bool) -> Option<JHashMap<String, bool>> {
        if inherited {
            let base_parts = self.base_class_info.as_ref().and_then(|b| b.borrow().get_skin_parts(inherited));
            let mut inherited_map = JHashMap::new();
            if let Some(s) = &self.skin_parts {
                inherited_map.put_all(s.iter().map(|(k, v)| (k.clone(), *v)), s.len());
            }
            if let Some(b) = &base_parts {
                inherited_map.put_all(b.iter().map(|(k, v)| (k.clone(), *v)), b.len());
            }
            Some(inherited_map)
        } else {
            self.skin_parts.clone()
        }
    }

    pub fn get_base_class_multi_name(&mut self) -> MultiName {
        if self.base_class_multi_name.is_none() {
            let n = self.base_class_name.clone().expect("NullPointerException");
            self.base_class_multi_name = Some(self.info.get_multi_name(&n));
        }
        self.base_class_multi_name.clone().unwrap()
    }

    pub fn defines_variable(&self, variable_name: &str) -> bool {
        let mut result = self.variables.iter().flatten().any(|q| q.local_part == variable_name);
        if !result {
            if let Some(b) = &self.base_class_info {
                result = b.borrow().defines_variable(variable_name);
            }
        }
        result
    }

    pub fn extends_class(&self, class_name: &str) -> bool {
        let mut result = self.class_name == class_name;
        if !result {
            if let Some(b) = &self.base_class_info {
                result = b.borrow().extends_class(class_name);
            }
        }
        result
    }

    pub fn implements_interface(&self, namespace: &str, interface_name: &str) -> bool {
        let mut result = self.info.implements_interface(namespace, interface_name);
        if !result {
            if let Some(b) = &self.base_class_info {
                result = b.borrow().implements_interface(namespace, interface_name);
            }
        }
        result
    }

    /// `setBaseClassInfo(baseClassInfo)`; a class that extends itself keeps no base.
    pub fn set_base_class_info(this: &ClassInfoRef, base_class_info: ClassInfoRef) {
        if !Rc::ptr_eq(this, &base_class_info) {
            this.borrow_mut().base_class_info = Some(base_class_info);
        }
    }

    pub fn set_base_class_name(&mut self, base_class_name: &str) {
        self.base_class_name = Some(base_class_name.to_string());
    }
}
