//! Port of afe.jar `com.adobe.fontengine.font.opentype.Gasp`.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[derive(Clone, Debug, Default)]
pub struct Gasp {
    pub data: OTByteArray,
}

impl Gasp {
    pub fn new(a: FontByteArray) -> Gasp {
        Gasp { data: OTByteArray::from_font_byte_array(a) }
    }

    pub fn get_behavior(&self, d: f64) -> FontResult<i32> {
        let n = self.data.getuint16(2)?;
        let mut n2 = 4;
        while n > 0 {
            let n3 = self.data.getuint16(n2)?;
            if d <= n3 as f64 {
                return self.data.getuint16(n2 + 2);
            }
            n2 += 4;
        }
        Ok(0)
    }

    pub fn subset_and_stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1734439792, OTByteArray::get_ot_byte_array_builder_instance_from(&self.data));
    }
}
