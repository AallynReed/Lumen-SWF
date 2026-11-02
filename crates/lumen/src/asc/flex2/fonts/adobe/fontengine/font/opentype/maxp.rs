//! Port of afe.jar `com.adobe.fontengine.font.opentype.Maxp`.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::table::data_as_byte_array;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

#[derive(Clone, Debug, Default)]
pub struct Maxp {
    pub data: OTByteArray,
}

impl Maxp {
    pub fn new(a: FontByteArray) -> FontResult<Maxp> {
        let size = a.get_size();
        let t = Maxp { data: OTByteArray::from_font_byte_array(a) };
        if size < 6 {
            return Err(FontEngineException::invalid_font(format!("'maxp' table must be at least 6 bytes (actual = {})", size)));
        }
        let n = t.get_table_major_version()?;
        match n {
            0 => {
                let n2 = t.get_table_minor_version()?;
                if n2 < 20480 {
                    return Err(FontEngineException::unsupported_font(format!("'maxp' table version {}.{} are not supported", n, n2)));
                }
            }
            1 => {}
            _ => return Err(FontEngineException::unsupported_font(format!("'maxp' table major version {} are not supported", n))),
        }
        Ok(t)
    }

    pub fn get_table_major_version(&self) -> FontResult<i32> {
        self.data.getuint16(0)
    }

    pub fn get_table_minor_version(&self) -> FontResult<i32> {
        self.data.getuint16(2)
    }

    pub fn get_num_glyphs(&self) -> FontResult<i32> {
        self.data.getuint16(4)
    }

    pub fn subset_and_stream_for_cff(&self, n: i32, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        let mut b = OTByteArray::get_ot_byte_array_builder_instance_sized(6);
        b.setuint32(0, 20480);
        b.setuint16(4, n);
        map.insert(1835104368, b);
    }

    pub fn subset_and_stream(&self, subset: &dyn Subset, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        let mut b = OTByteArray::get_ot_byte_array_builder_instance_from(&self.data);
        b.setuint16(4, subset.get_num_glyphs());
        map.insert(1835104368, b);
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1835104368, data_as_byte_array(&self.data));
    }
}
