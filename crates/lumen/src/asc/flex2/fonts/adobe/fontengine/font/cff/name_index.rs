//! Port of afe.jar `com.adobe.fontengine.font.cff.NameIndex`.

use std::ops::Deref;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::{CFFByteArray, CFFByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::index::Index;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[derive(Clone, Debug)]
pub struct NameIndex {
    pub base: Index,
}

impl Deref for NameIndex {
    type Target = Index;
    fn deref(&self) -> &Index {
        &self.base
    }
}

impl NameIndex {
    pub fn new(data: Rc<CFFByteArray>, n: i32) -> FontResult<NameIndex> {
        Ok(NameIndex { base: Index::new(data, n)? })
    }

    /// Each byte becomes one char (Latin-1).
    pub fn get_nth_name(&self, n: i32) -> FontResult<Option<String>> {
        if n < 0 || self.entry_count < n {
            return Ok(None);
        }
        let mut n2 = self.offset_of(n);
        let n3 = self.size_of(n);
        let mut s = String::with_capacity(n3.max(0) as usize);
        for _ in 0..n3 {
            s.push(self.data.getcard8(n2)? as u8 as char);
            n2 += 1;
        }
        Ok(Some(s))
    }

    /// Writes the low byte of each UTF-16 unit, skipping NULs.
    pub fn to_binary(b: &mut CFFByteArrayBuilder, s: &str) {
        let cursor = Index::start_index(b, 1);
        for c in s.encode_utf16() {
            if c == 0 {
                continue;
            }
            b.add_card8(c as i32);
        }
        Index::element_entered(b, cursor);
    }
}
