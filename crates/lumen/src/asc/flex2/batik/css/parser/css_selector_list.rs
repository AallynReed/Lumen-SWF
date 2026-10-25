//! Port of `org.apache.flex.forks.batik.css.parser.CSSSelectorList`.

use super::selectors::SelectorRef;

#[derive(Clone, Debug, Default)]
pub struct CSSSelectorList {
    list: Vec<SelectorRef>,
}

impl CSSSelectorList {
    pub fn new() -> CSSSelectorList {
        CSSSelectorList::default()
    }

    pub fn get_length(&self) -> i32 {
        self.list.len() as i32
    }

    pub fn item(&self, index: i32) -> Option<&SelectorRef> {
        if index < 0 {
            return None;
        }
        self.list.get(index as usize)
    }

    pub fn append(&mut self, item: SelectorRef) {
        self.list.push(item);
    }
}
