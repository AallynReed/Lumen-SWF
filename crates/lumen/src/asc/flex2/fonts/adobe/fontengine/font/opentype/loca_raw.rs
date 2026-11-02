//! Port of afe.jar `com.adobe.fontengine.font.opentype.LocaRaw`.

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::OTByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[derive(Clone, Debug, Default)]
pub struct LocaRaw {
    pub data: OTByteArray,
}

impl LocaRaw {
    pub fn new(a: FontByteArray) -> LocaRaw {
        LocaRaw { data: OTByteArray::from_font_byte_array(a) }
    }

    pub fn get_glyph_location(&self, n: i32, n2: i32) -> FontResult<i32> {
        if n2 == 0 {
            return Ok(self.data.getuint16(n * 2)? * 2);
        }
        self.data.getuint32asint(n * 4, "loca index too big")
    }
}
