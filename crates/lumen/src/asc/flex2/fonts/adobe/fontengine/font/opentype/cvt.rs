//! Port of afe.jar `com.adobe.fontengine.font.opentype.Cvt`.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::table::data_as_byte_array;

#[derive(Clone, Debug, Default)]
pub struct Cvt {
    pub data: OTByteArray,
}

impl Cvt {
    pub fn new(a: FontByteArray) -> Cvt {
        Cvt { data: OTByteArray::from_font_byte_array(a) }
    }

    pub fn get_nb_entries(&self) -> i32 {
        self.data.get_size() / 2
    }

    pub fn subset_and_stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1668707360, OTByteArray::get_ot_byte_array_builder_instance_from(&self.data));
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1668707360, data_as_byte_array(&self.data));
    }

    pub fn get_data(&self) -> &OTByteArray {
        &self.data
    }
}
