//! Port of `flex2.compiler.as3.binding.InterfaceInfo`.

use std::cell::RefCell;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use super::info::Info;
use crate::asc::flex2::compiler::util::MultiName;

pub type InterfaceInfoRef = Rc<RefCell<InterfaceInfo>>;

pub struct InterfaceInfo {
    info: Info,
    interface_name: String,
    base_interface_info: Option<InterfaceInfoRef>,
    base_interface_name: Option<String>,
    base_interface_multi_name: Option<MultiName>,
}

impl Deref for InterfaceInfo {
    type Target = Info;
    fn deref(&self) -> &Info {
        &self.info
    }
}

impl DerefMut for InterfaceInfo {
    fn deref_mut(&mut self) -> &mut Info {
        &mut self.info
    }
}

impl fmt::Display for InterfaceInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InterfaceInfo: {}", self.interface_name)
    }
}

impl InterfaceInfo {
    pub fn new(interface_name: &str) -> InterfaceInfo {
        let mut i = InterfaceInfo { info: Info::default(), interface_name: interface_name.to_string(), base_interface_info: None, base_interface_name: None, base_interface_multi_name: None };
        if let Some(l) = interface_name.rfind(':').filter(|&l| l > 0) {
            i.info.add_import(&interface_name[..l]);
        }
        i
    }

    /// `definesFunction(functionName, inherited)`.
    pub fn defines_function_inherited(&self, function_name: &str, inherited: bool) -> bool {
        let mut result = self.info.defines_function(function_name);
        if !result && inherited {
            if let Some(b) = &self.base_interface_info {
                result = b.borrow().defines_function_inherited(function_name, inherited);
            }
        }
        result
    }

    /// `definesGetter(getterName)`, which always searches the base interface.
    pub fn defines_getter(&self, getter_name: &str) -> bool {
        let mut result = self.info.defines_getter(getter_name);
        if !result {
            if let Some(b) = &self.base_interface_info {
                result = b.borrow().defines_getter(getter_name);
            }
        }
        result
    }

    /// `definesSetter(setterName, inherited)`.
    pub fn defines_setter_inherited(&self, setter_name: &str, inherited: bool) -> bool {
        let mut result = self.info.defines_setter(setter_name);
        if !result && inherited {
            if let Some(b) = &self.base_interface_info {
                result = b.borrow().defines_setter_inherited(setter_name, inherited);
            }
        }
        result
    }

    pub fn get_interface_name(&self) -> &str {
        &self.interface_name
    }

    pub fn get_base_interface_name(&self) -> Option<&str> {
        self.base_interface_name.as_deref()
    }

    pub fn get_base_interface_multi_name(&mut self) -> MultiName {
        if self.base_interface_multi_name.is_none() {
            let n = self.base_interface_name.clone().expect("NullPointerException");
            self.base_interface_multi_name = Some(self.info.get_multi_name(&n));
        }
        self.base_interface_multi_name.clone().unwrap()
    }

    pub fn extends_interface(&self, namespace: &str, interface_name: &str) -> bool {
        let mut result = false;
        if let Some(b) = &self.base_interface_info {
            let b = b.borrow();
            result = b.get_interface_name() == format!("{namespace}:{interface_name}");
            if !result {
                result = b.extends_interface(namespace, interface_name);
            }
        }
        result
    }

    pub fn implements_interface(&self, namespace: &str, interface_name: &str) -> bool {
        let mut result = self.info.implements_interface(namespace, interface_name);
        if !result {
            if let Some(b) = &self.base_interface_info {
                result = b.borrow().implements_interface(namespace, interface_name);
            }
        }
        result
    }

    pub fn set_base_interface_info(&mut self, base_interface_info: InterfaceInfoRef) {
        self.base_interface_info = Some(base_interface_info);
    }

    pub fn set_base_interface_name(&mut self, base_interface_name: &str) {
        self.base_interface_name = Some(base_interface_name.to_string());
    }
}
