//! Port of `flex2.compiler.as3.reflect.MetaData` (the only `flex2.compiler.abc.MetaData`): a metadata id and the value
//! array it shared with its node or slot when created.

use crate::asc::parser::node::MetaDataNode;
use crate::asc::semantics::value::Value;
use crate::asc::util::jhash::JHashMap;

#[derive(Clone, Default)]
pub struct MetaData {
    id: Option<String>,
    values: Option<Vec<Value>>,
}

impl MetaData {
    pub fn from_node(node: &MetaDataNode) -> MetaData {
        MetaData { id: node.get_id(), values: node.get_values() }
    }

    pub fn from_semantics(node: &crate::asc::semantics::meta_data::MetaData) -> MetaData {
        MetaData { id: node.id(), values: node.values() }
    }

    pub fn get_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// `getID()`, owned.
    pub fn id(&self) -> Option<String> {
        self.id.clone()
    }

    pub fn get_key(&self, index: i32) -> Option<String> {
        if index < 0 || index >= self.count() {
            panic!("ArrayIndexOutOfBoundsException");
        }
        match &self.values.as_ref().unwrap()[index as usize] {
            Value::KeyValuePair(kv) => Some(kv.key.clone()),
            _ => None,
        }
    }

    /// `getValue(String key)`.
    pub fn get_value_key(&self, key: &str) -> Option<String> {
        for v in self.values.iter().flatten() {
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
        match &self.values.as_ref().unwrap()[index as usize] {
            Value::KeylessValue(k) => Some(k.obj.clone()),
            Value::KeyValuePair(k) => Some(k.obj.clone()),
            _ => None,
        }
    }

    pub fn get_value_map(&self) -> JHashMap<String, String> {
        let mut result = JHashMap::new();
        for v in self.values.iter().flatten() {
            if let Value::KeyValuePair(kv) = v {
                result.insert(kv.key.clone(), kv.obj.clone());
            }
        }
        result
    }

    pub fn count(&self) -> i32 {
        self.values.as_ref().map_or(0, |v| v.len() as i32)
    }

    pub fn get_values(&self) -> Option<&[Value]> {
        self.values.as_deref()
    }
}
