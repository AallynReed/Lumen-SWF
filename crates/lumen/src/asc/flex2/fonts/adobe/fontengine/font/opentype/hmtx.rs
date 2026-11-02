//! Port of afe.jar `com.adobe.fontengine.font.opentype.Hmtx`.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::hmtx_raw::HmtxRaw;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::table::data_as_byte_array;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[derive(Clone, Debug)]
pub struct Hmtx {
    pub number_of_h_metrics: i32,
    pub hmtx: Rc<HmtxRaw>,
}

impl Hmtx {
    pub fn new(hmtx: Rc<HmtxRaw>, n: i32) -> Hmtx {
        Hmtx { hmtx, number_of_h_metrics: n }
    }

    pub fn get_left_side_bearing(&self, n: i32) -> FontResult<i32> {
        self.hmtx.get_left_side_bearing(n, self.number_of_h_metrics)
    }

    pub fn get_horizontal_advance(&self, n: i32) -> FontResult<i32> {
        self.hmtx.get_horizontal_advance(n, self.number_of_h_metrics)
    }

    pub fn subset_and_stream(&self, subset: &dyn Subset, map: &mut BTreeMap<i32, OTByteArrayBuilder>) -> FontResult<()> {
        let mut b = OTByteArray::get_ot_byte_array_builder_instance_sized(subset.get_num_glyphs() * 4);
        let mut n = 0;
        for i in 0..subset.get_num_glyphs() {
            let n2 = subset.get_full_gid(i);
            b.setuint16(n, self.get_horizontal_advance(n2)?);
            b.setint16(n + 2, self.get_left_side_bearing(n2)?);
            n += 4;
        }
        map.insert(1752003704, b);
        Ok(())
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1752003704, data_as_byte_array(&self.hmtx.data));
    }
}
