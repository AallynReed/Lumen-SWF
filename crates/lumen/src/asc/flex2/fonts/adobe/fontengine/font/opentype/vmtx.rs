//! Port of afe.jar `com.adobe.fontengine.font.opentype.Vmtx`.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::table::data_as_byte_array;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::vmtx_raw::VmtxRaw;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[derive(Clone, Debug)]
pub struct Vmtx {
    pub number_of_v_metrics: i32,
    pub vmtx: Rc<VmtxRaw>,
}

impl Vmtx {
    pub fn new(vmtx: Rc<VmtxRaw>, n: i32) -> Vmtx {
        Vmtx { vmtx, number_of_v_metrics: n }
    }

    pub fn get_top_side_bearing(&self, n: i32) -> FontResult<i32> {
        self.vmtx.get_top_side_bearing(n, self.number_of_v_metrics)
    }

    pub fn get_horizontal_advance(&self, n: i32) -> FontResult<i32> {
        self.vmtx.get_vertical_advance(n, self.number_of_v_metrics)
    }

    pub fn subset_and_stream(&self, subset: &dyn Subset, map: &mut BTreeMap<i32, OTByteArrayBuilder>) -> FontResult<()> {
        let mut b = OTByteArray::get_ot_byte_array_builder_instance_sized(subset.get_num_glyphs() * 4);
        let mut n = 0;
        for i in 0..subset.get_num_glyphs() {
            let n2 = subset.get_full_gid(i);
            b.setuint16(n, self.get_horizontal_advance(n2)?);
            b.setint16(n + 2, self.get_top_side_bearing(n2)?);
            n += 4;
        }
        map.insert(1986884728, b);
        Ok(())
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1986884728, data_as_byte_array(&self.vmtx.data));
    }
}
