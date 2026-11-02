//! Port of afe.jar `com.adobe.fontengine.font.opentype.Fpgm`.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::table::data_as_byte_array;

#[derive(Clone, Debug, Default)]
pub struct Fpgm {
    pub data: OTByteArray,
}

impl Fpgm {
    pub fn new(a: FontByteArray) -> Fpgm {
        Fpgm { data: OTByteArray::from_font_byte_array(a) }
    }

    pub fn subset_and_stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1718642541, OTByteArray::get_ot_byte_array_builder_instance_from(&self.data));
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1718642541, data_as_byte_array(&self.data));
    }

    pub fn get_data(&self) -> &OTByteArray {
        &self.data
    }
}
