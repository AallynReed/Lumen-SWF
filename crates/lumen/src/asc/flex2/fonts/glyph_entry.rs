//! Port of swfutils `flash.swf.types.GlyphEntry` as the font builders use it (the text-record half is
//! `flash::swf::types::GlyphEntry`).

use std::cell::RefCell;
use std::rc::Rc;

use crate::asc::flex2::flash::swf::types::{Rect, Shape, ZoneRecord};

#[derive(Clone, Default)]
pub struct GlyphEntry {
    pub index: i32,
    pub advance: i32,
    pub character: u16,
    pub bounds: Option<Rect>,
    pub zone_record: Option<Rc<ZoneRecord>>,
    pub shape: Option<Shape>,
}

/// A glyph entry shared by a face's glyph cache and the `FontBuilder`s that use it.
pub type GlyphEntryRef = Rc<RefCell<GlyphEntry>>;

impl GlyphEntry {
    pub fn get_index(&self) -> i32 {
        self.index
    }

    pub fn set_index(&mut self, index: i32) {
        self.index = index;
    }
}
