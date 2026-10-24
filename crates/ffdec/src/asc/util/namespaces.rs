//! Port of `macromedia/asc/util/Namespaces.java`. A shared, mutable list handle like the Java object: cloning shares,
//! `copy_of` is `new Namespaces(namespaces)`.

use std::cell::{Ref, RefCell, RefMut};
use std::fmt;
use std::rc::Rc;

use crate::asc::semantics::object_value::ObjectValue;

#[derive(Clone, Default)]
pub struct Namespaces(Rc<RefCell<Vec<ObjectValue>>>);

impl Namespaces {
    pub fn new() -> Namespaces {
        Namespaces(Rc::new(RefCell::new(Vec::with_capacity(1))))
    }

    /// `new Namespaces(qualifier)`.
    pub fn of(qualifier: ObjectValue) -> Namespaces {
        Namespaces(Rc::new(RefCell::new(vec![qualifier])))
    }

    pub fn with_capacity(size: usize) -> Namespaces {
        Namespaces(Rc::new(RefCell::new(Vec::with_capacity(size))))
    }

    /// `new Namespaces(namespaces)`: a copy.
    pub fn copy_of(namespaces: &Namespaces) -> Namespaces {
        Namespaces(Rc::new(RefCell::new(namespaces.0.borrow().clone())))
    }

    pub fn from_vec(v: Vec<ObjectValue>) -> Namespaces {
        Namespaces(Rc::new(RefCell::new(v)))
    }

    pub fn ptr_eq(&self, other: &Namespaces) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn borrow(&self) -> Ref<'_, Vec<ObjectValue>> {
        self.0.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, Vec<ObjectValue>> {
        self.0.borrow_mut()
    }

    /// A snapshot of the elements, safe to iterate while the list changes.
    pub fn to_vec(&self) -> Vec<ObjectValue> {
        self.0.borrow().clone()
    }

    pub fn size(&self) -> usize {
        self.0.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }

    pub fn get(&self, i: usize) -> ObjectValue {
        self.0.borrow()[i].clone()
    }

    pub fn at(&self, i: usize) -> ObjectValue {
        self.get(i)
    }

    pub fn first(&self) -> ObjectValue {
        self.get(0)
    }

    pub fn back(&self) -> ObjectValue {
        self.0.borrow().last().expect("IndexOutOfBoundsException").clone()
    }

    pub fn last(&self) -> ObjectValue {
        self.back()
    }

    pub fn set(&self, i: usize, ns: ObjectValue) {
        self.0.borrow_mut()[i] = ns;
    }

    pub fn add(&self, ns: ObjectValue) {
        self.0.borrow_mut().push(ns);
    }

    pub fn push_back(&self, ns: ObjectValue) {
        self.add(ns);
    }

    pub fn add_all(&self, other: &Namespaces) {
        let items = other.to_vec();
        self.0.borrow_mut().extend(items);
    }

    pub fn insert(&self, i: usize, ns: ObjectValue) {
        self.0.borrow_mut().insert(i, ns);
    }

    pub fn remove(&self, i: usize) -> ObjectValue {
        self.0.borrow_mut().remove(i)
    }

    pub fn pop_back(&self) {
        self.0.borrow_mut().pop();
    }

    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }

    /// `contains`, which uses `ObjectValue.equals`: identity.
    pub fn contains(&self, ns: &ObjectValue) -> bool {
        self.0.borrow().iter().any(|n| n.ptr_eq(ns))
    }

    pub fn index_of(&self, ns: &ObjectValue) -> i32 {
        self.0.borrow().iter().position(|n| n.ptr_eq(ns)).map_or(-1, |i| i as i32)
    }

    /// `Namespaces.equals`: same elements by identity, in order.
    pub fn equals(&self, other: &Namespaces) -> bool {
        if self.ptr_eq(other) {
            return true;
        }
        let a = self.0.borrow();
        let b = other.0.borrow();
        a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.ptr_eq(y))
    }
}

/// `AbstractCollection.toString`: `[a, b]` with each element's `toString()`.
impl fmt::Display for Namespaces {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[")?;
        for (i, ns) in self.0.borrow().iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{}", ns)?;
        }
        f.write_str("]")
    }
}

impl fmt::Debug for Namespaces {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
