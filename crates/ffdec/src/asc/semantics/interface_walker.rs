//! Port of `macromedia/asc/semantics/InterfaceWalker.java`: a breadth-first walk over the interfaces of a type and its
//! base classes, yielding each interface's prototype once.

use std::collections::{HashSet, VecDeque};

use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::object_value_walker::ObjectValueWalker;
use crate::asc::semantics::type_value::TypeValue;

pub struct InterfaceWalker {
    queue: VecDeque<TypeValue>,
    set: HashSet<TypeValue>,
    next_interface: Option<ObjectValue>,
    super_interfaces: bool,
}

impl InterfaceWalker {
    pub fn new(start_obj: &ObjectValue) -> InterfaceWalker {
        Self::with_super_interfaces(start_obj, true)
    }

    pub fn with_super_interfaces(start_obj: &ObjectValue, super_interfaces: bool) -> InterfaceWalker {
        let mut w = InterfaceWalker { queue: VecDeque::new(), set: HashSet::new(), next_interface: None, super_interfaces };
        let start = match start_obj.as_type_value() {
            Some(t) => t,
            None => start_obj.type_().expect("NullPointerException").type_value(),
        };
        w.queue.push_back(start);
        w.process_queue();
        // The starting object is not part of the stream.
        if w.next_interface.as_ref().map_or(false, |n| n.ptr_eq(start_obj)) {
            w.next_interface = None;
        }
        w
    }

    fn process_queue(&mut self) {
        while self.next_interface.is_none() {
            let type_ = match self.queue.pop_front() {
                Some(t) => t,
                None => break,
            };
            // Unresolved base classes can come from versioned definitions; they are reported later.
            if !type_.resolved() {
                continue;
            }
            if let Some(b) = type_.baseclass() {
                if !type_.is_interface() {
                    self.queue.push_back(b);
                }
            }
            let ibui = type_.prototype().expect("NullPointerException").builder().expect("NullPointerException");
            let refs = ibui.instance_data().interface_refs.clone();
            if self.super_interfaces || !type_.is_interface() {
                for r in &refs {
                    let t = r.slot().expect("NullPointerException").get_object_value().expect("NullPointerException");
                    self.queue.push_back(TypeValue::from_object(&t).expect("ClassCastException"));
                }
            }
            if type_.is_interface() && !self.set.contains(&type_) {
                self.next_interface = type_.prototype();
                self.set.insert(type_);
            }
        }
    }
}

impl Iterator for InterfaceWalker {
    type Item = ObjectValue;
    fn next(&mut self) -> Option<ObjectValue> {
        self.process_queue();
        self.next_interface.take()
    }
}

impl ObjectValueWalker for InterfaceWalker {
    fn has_next(&mut self) -> bool {
        self.process_queue();
        self.next_interface.is_some()
    }

    fn clear(&mut self) {
        self.set.clear();
        self.queue.clear();
        self.next_interface = None;
    }
}
