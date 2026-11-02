//! Port of afe.jar `com.adobe.fontengine.font.opentype.HmtxRaw`.

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::OTByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[derive(Clone, Debug, Default)]
pub struct HmtxRaw {
    pub data: OTByteArray,
}

impl HmtxRaw {
    pub fn new(a: FontByteArray) -> HmtxRaw {
        HmtxRaw { data: OTByteArray::from_font_byte_array(a) }
    }

    pub fn get_left_side_bearing(&self, n: i32, n2: i32) -> FontResult<i32> {
        if n < n2 {
            return self.data.getint16(4 * n + 2);
        }
        self.data.getint16(4 * n2 + 2 * (n - n2))
    }

    pub fn get_horizontal_advance(&self, n: i32, n2: i32) -> FontResult<i32> {
        if n < n2 {
            return self.data.getuint16(4 * n);
        }
        self.data.getuint16(4 * (n2 - 1))
    }
}
