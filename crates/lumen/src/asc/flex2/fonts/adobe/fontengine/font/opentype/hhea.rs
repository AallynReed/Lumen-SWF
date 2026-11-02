//! Port of afe.jar `com.adobe.fontengine.font.opentype.Hhea`.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::table::data_as_byte_array;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

#[derive(Clone, Debug, Default)]
pub struct Hhea {
    pub data: OTByteArray,
}

impl Hhea {
    pub fn new(a: FontByteArray) -> FontResult<Hhea> {
        let t = Hhea { data: OTByteArray::from_font_byte_array(a) };
        let n = t.get_table_major_version()?;
        if n != 1 {
            return Err(FontEngineException::unsupported_font(format!("'hhea' tables with major version {}are not supported", n)));
        }
        Ok(t)
    }

    pub fn get_table_major_version(&self) -> FontResult<i32> {
        self.data.getuint16(0)
    }

    pub fn get_table_minor_version(&self) -> FontResult<i32> {
        self.data.getuint16(2)
    }

    pub fn get_number_of_h_metrics(&self) -> FontResult<i32> {
        self.data.getuint16(34)
    }

    pub fn get_rise(&self) -> FontResult<i32> {
        self.data.getint16(18)
    }

    pub fn get_run(&self) -> FontResult<i32> {
        self.data.getint16(20)
    }

    pub fn get_ascender(&self) -> FontResult<i32> {
        self.data.getint16(4)
    }

    pub fn get_descender(&self) -> FontResult<i32> {
        self.data.getint16(6)
    }

    pub fn get_line_gap(&self) -> FontResult<i32> {
        self.data.getint16(8)
    }

    pub fn subset_and_stream(&self, subset: &dyn Subset, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        let mut b = OTByteArray::get_ot_byte_array_builder_instance_from(&self.data);
        b.setuint16(34, subset.get_num_glyphs());
        map.insert(1751672161, b);
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1751672161, data_as_byte_array(&self.data));
    }
}
