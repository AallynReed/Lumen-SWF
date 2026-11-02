//! Port of afe.jar `com.adobe.fontengine.font.opentype.Ltsh`.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[derive(Clone, Debug, Default)]
pub struct Ltsh {
    pub data: OTByteArray,
}

impl Ltsh {
    pub fn new(a: FontByteArray) -> Ltsh {
        Ltsh { data: OTByteArray::from_font_byte_array(a) }
    }

    pub fn get_y_pels(&self, n: i32) -> FontResult<i32> {
        let n2 = self.data.getuint16(2)?;
        if n < n2 {
            return self.data.getuint8(4 + n);
        }
        Ok(1)
    }

    pub fn subset_and_stream(&self, subset: &dyn Subset, map: &mut BTreeMap<i32, OTByteArrayBuilder>) -> FontResult<()> {
        let mut b = OTByteArray::get_ot_byte_array_builder_instance_sized(subset.get_num_glyphs() * 4);
        b.setuint16(0, 0);
        b.setuint16(2, subset.get_num_glyphs());
        let mut n = 4;
        for i in 0..subset.get_num_glyphs() {
            let n2 = subset.get_full_gid(i);
            b.setuint8(n, self.get_y_pels(n2)?);
            n += 1;
        }
        map.insert(1280594760, b);
        Ok(())
    }
}
