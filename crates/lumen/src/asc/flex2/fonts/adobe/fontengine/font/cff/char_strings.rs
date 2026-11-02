//! Port of afe.jar `com.adobe.fontengine.font.cff.CharStrings`.

use std::ops::Deref;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::{CFFByteArray, CFFByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::index::Index;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[derive(Clone, Debug)]
pub struct CharStrings {
    pub base: Index,
}

impl Deref for CharStrings {
    type Target = Index;
    fn deref(&self) -> &Index {
        &self.base
    }
}

impl CharStrings {
    pub fn new(data: Rc<CFFByteArray>, n: i32) -> FontResult<CharStrings> {
        Ok(CharStrings { base: Index::new(data, n)? })
    }

    /// `new CharStrings(builder.toCFFByteArray(), 0)`.
    pub fn from_builder(b: CFFByteArrayBuilder) -> FontResult<CharStrings> {
        CharStrings::new(Rc::new(b.to_cff_byte_array()), 0)
    }

    pub fn create_empty_charstrings() -> FontResult<CharStrings> {
        let mut b = CFFByteArray::get_cff_byte_array_builder_instance_sized(2);
        b.add_card16(0);
        CharStrings::from_builder(b)
    }
}
