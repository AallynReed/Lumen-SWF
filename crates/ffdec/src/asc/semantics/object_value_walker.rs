//! Port of `macromedia/asc/semantics/ObjectValueWalker.java`: an on-demand iterator up the prototype or interface chain.

use crate::asc::semantics::object_value::ObjectValue;

pub trait ObjectValueWalker: Iterator<Item = ObjectValue> {
    fn has_next(&mut self) -> bool;
    fn clear(&mut self);
}
