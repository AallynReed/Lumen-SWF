//! Port of `macromedia/asc/semantics/PrototypeWalker.java`: walks `proto()` links from a starting object.

use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::object_value_walker::ObjectValueWalker;

pub struct PrototypeWalker {
    cur_obj_val: Option<ObjectValue>,
}

impl PrototypeWalker {
    pub fn new(start_obj: Option<ObjectValue>) -> PrototypeWalker {
        PrototypeWalker { cur_obj_val: start_obj }
    }
}

impl Iterator for PrototypeWalker {
    type Item = ObjectValue;
    fn next(&mut self) -> Option<ObjectValue> {
        let ret = self.cur_obj_val.take()?;
        self.cur_obj_val = ret.proto();
        Some(ret)
    }
}

impl ObjectValueWalker for PrototypeWalker {
    fn has_next(&mut self) -> bool {
        self.cur_obj_val.is_some()
    }

    fn clear(&mut self) {
        self.cur_obj_val = None;
    }
}
