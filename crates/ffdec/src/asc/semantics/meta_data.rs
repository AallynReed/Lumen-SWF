//! Port of `macromedia/asc/semantics/MetaData.java`: metadata kept on slots, shared by reference with the node it came
//! from as in Java.

use std::cell::RefCell;
use std::rc::Rc;

use crate::asc::semantics::value::Value;

#[derive(Default)]
pub struct MetaDataData {
    pub id: Option<String>,
    pub values: Option<Vec<Value>>,
}

#[derive(Clone, Default)]
pub struct MetaData(Rc<RefCell<MetaDataData>>);

impl MetaData {
    pub fn new() -> MetaData {
        MetaData::default()
    }

    pub fn ptr_eq(&self, other: &MetaData) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn id(&self) -> Option<String> {
        self.0.borrow().id.clone()
    }

    pub fn set_id(&self, id: Option<String>) {
        self.0.borrow_mut().id = id;
    }

    pub fn values(&self) -> Option<Vec<Value>> {
        self.0.borrow().values.clone()
    }

    pub fn set_values(&self, values: Option<Vec<Value>>) {
        self.0.borrow_mut().values = values;
    }

    /// `getValue(String key)`.
    pub fn get_value_key(&self, key: &str) -> Option<String> {
        let d = self.0.borrow();
        for v in d.values.iter().flatten() {
            if let Value::KeyValuePair(kv) = v {
                if kv.key == key {
                    return Some(kv.obj.clone());
                }
            }
        }
        None
    }

    /// `getValue(int index)`.
    pub fn get_value_index(&self, index: i32) -> Option<String> {
        if index < 0 || index >= self.count() {
            panic!("ArrayIndexOutOfBoundsException");
        }
        let d = self.0.borrow();
        match &d.values.as_ref().unwrap()[index as usize] {
            Value::KeylessValue(k) => Some(k.obj.clone()),
            Value::KeyValuePair(k) => Some(k.obj.clone()),
            _ => None,
        }
    }

    pub fn count(&self) -> i32 {
        self.0.borrow().values.as_ref().map_or(0, |v| v.len() as i32)
    }
}
