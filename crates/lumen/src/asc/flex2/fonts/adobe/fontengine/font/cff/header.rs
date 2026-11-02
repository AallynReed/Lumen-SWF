//! Port of afe.jar `com.adobe.fontengine.font.cff.Header`.

use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::{CFFByteArray, CFFByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[derive(Clone, Debug)]
pub struct Header {
    pub data: Rc<CFFByteArray>,
    pub offset: i32,
}

impl Header {
    pub fn new(data: Rc<CFFByteArray>, n: i32) -> Header {
        Header { data, offset: n }
    }

    pub fn get_major_version(&self) -> FontResult<i32> {
        self.data.getcard8(0)
    }

    pub fn get_minorversion(&self) -> FontResult<i32> {
        self.data.getcard8(1)
    }

    pub fn size(&self) -> FontResult<i32> {
        self.data.getcard8(2)
    }

    pub fn to_binary(b: &mut CFFByteArrayBuilder) {
        b.add_card8(1);
        b.add_card8(0);
        b.add_card8(4);
        b.add_card8(4);
    }
}
