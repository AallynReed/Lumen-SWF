//! Port of `macromedia/asc/util/Slots.java`: slots kept sorted by id.

use crate::asc::semantics::slot::Slot;

#[derive(Clone, Default)]
pub struct Slots {
    items: Vec<Slot>,
}

impl Slots {
    pub fn new() -> Slots {
        Slots { items: Vec::new() }
    }

    /// Inserts by binary search on the id; a slot whose id is already present is not added.
    pub fn put(&mut self, slot: Slot) -> bool {
        let mut lo: i32 = -1;
        let mut hi: i32 = self.items.len() as i32;
        let id = slot.id();
        while hi - lo > 1 {
            let pivot = (lo + hi) >> 1;
            let test_id = self.items[pivot as usize].id();
            if id == test_id {
                return true;
            } else if id < test_id {
                hi = pivot;
            } else {
                lo = pivot;
            }
        }
        self.items.insert(hi as usize, slot);
        true
    }

    pub fn get_by_id(&self, id: i32) -> Option<Slot> {
        let mut lo: i32 = 0;
        let mut hi: i32 = self.items.len() as i32 - 1;
        if hi > lo && (id < self.items[lo as usize].id() || id > self.items[hi as usize].id()) {
            return None;
        }
        while lo <= hi {
            let pivot = (lo + hi) >> 1;
            let slot = &self.items[pivot as usize];
            let test_id = slot.id();
            if id == test_id {
                return Some(slot.clone());
            } else if id < test_id {
                hi = pivot - 1;
            } else {
                lo = pivot + 1;
            }
        }
        None
    }

    pub fn size(&self) -> usize {
        self.items.len()
    }

    pub fn at(&self, i: usize) -> Slot {
        self.items[i].clone()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Slot> {
        self.items.iter()
    }
}
