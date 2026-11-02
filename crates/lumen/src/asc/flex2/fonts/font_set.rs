//! Port of swfutils `flash.fonts.FontSet`: the faces of one family by style (an `IntMapLRUCache` of four that
//! never purges).

use std::collections::HashMap;

use crate::asc::flex2::fonts::font_face::FaceRef;

#[derive(Clone, Default)]
pub struct FontSet {
    entries: HashMap<i32, FaceRef>,
}

impl FontSet {
    pub fn new(_max_faces_per_font: i32) -> FontSet {
        FontSet::default()
    }

    pub fn put(&mut self, style: i32, entry: FaceRef) {
        self.entries.insert(style, entry);
    }

    pub fn get(&self, style: i32) -> Option<FaceRef> {
        self.entries.get(&style).cloned()
    }
}

pub type FontSetRef = std::rc::Rc<std::cell::RefCell<FontSet>>;
