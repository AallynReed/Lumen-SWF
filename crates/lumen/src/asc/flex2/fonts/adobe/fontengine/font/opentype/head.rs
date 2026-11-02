//! Port of afe.jar `com.adobe.fontengine.font.opentype.Head`.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::table::data_as_byte_array;
use crate::asc::flex2::fonts::adobe::fontengine::font::rect::Rect;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

pub const TAG_HEAD: i32 = 1751474532;

#[derive(Clone, Debug, Default)]
pub struct Head {
    pub data: OTByteArray,
}

impl Head {
    pub fn new(a: FontByteArray) -> FontResult<Head> {
        let size = a.get_size();
        let t = Head { data: OTByteArray::from_font_byte_array(a) };
        if size < 46 {
            return Err(FontEngineException::invalid_font("'head' table must be at least 46 bytes"));
        }
        let n = t.get_table_major_version()?;
        if n != 1 {
            return Err(FontEngineException::unsupported_font(format!("'head' tables with major version {} are not supported", n)));
        }
        Ok(t)
    }

    pub fn get_table_major_version(&self) -> FontResult<i32> {
        self.data.getuint16(0)
    }

    pub fn get_table_minor_version(&self) -> FontResult<i32> {
        self.data.getuint16(2)
    }

    pub fn checksum(&self) -> FontResult<i64> {
        let l = self.data.checksum(0, 8, 0)?;
        self.data.checksum(12, self.data.get_size(), l)
    }

    pub fn is_converted(&self) -> FontResult<bool> {
        Ok(self.data.getuint16(16)? & 0x1000 != 0)
    }

    pub fn get_units_per_em(&self) -> FontResult<i32> {
        self.data.getuint16(18)
    }

    pub fn get_index_to_loc_format(&self) -> FontResult<i32> {
        self.data.getint16(50)
    }

    pub fn is_italic(&self) -> FontResult<bool> {
        Ok(self.data.getuint16(44)? & 2 != 0)
    }

    pub fn get_x_min(&self) -> FontResult<i32> {
        self.data.getint16(36)
    }

    pub fn get_y_min(&self) -> FontResult<i32> {
        self.data.getint16(38)
    }

    pub fn get_x_max(&self) -> FontResult<i32> {
        self.data.getint16(40)
    }

    pub fn get_y_max(&self) -> FontResult<i32> {
        self.data.getint16(42)
    }

    pub fn get_font_bbox(&self) -> FontResult<Rect> {
        Ok(Rect::new(self.get_x_min()? as f64, self.get_y_min()? as f64, self.get_x_max()? as f64, self.get_y_max()? as f64))
    }

    /// Returns the map key of the builder, which `finalizeStreamData` patches later.
    pub fn subset_and_stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) -> i32 {
        let mut b = OTByteArray::get_ot_byte_array_builder_instance_from(&self.data);
        b.setint16(50, 1);
        map.insert(TAG_HEAD, b);
        TAG_HEAD
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) -> i32 {
        map.insert(TAG_HEAD, data_as_byte_array(&self.data));
        TAG_HEAD
    }

    pub fn clear_checksum_adjust(b: &mut OTByteArrayBuilder) {
        b.setuint32(8, 0);
    }

    pub fn set_checksum_adjust(b: &mut OTByteArrayBuilder, l: i64) {
        b.setuint32(8, l as i32);
    }
}
