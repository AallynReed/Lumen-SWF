//! Port of afe.jar `com.adobe.fontengine.font.opentype.Gdef`.

use std::collections::BTreeMap;
use std::ops::Deref;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gdef_subsetter::GdefSubsetter;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table::LayoutTable;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub const TAG_GDEF: i32 = 1195656518;

#[derive(Clone, Debug, Default)]
pub struct Gdef {
    pub base: LayoutTable,
}

impl Deref for Gdef {
    type Target = LayoutTable;
    fn deref(&self) -> &LayoutTable {
        &self.base
    }
}

impl Gdef {
    pub fn new(font_byte_array: FontByteArray) -> FontResult<Gdef> {
        Ok(Gdef { base: LayoutTable::new(font_byte_array) })
    }

    pub fn get_glyph_class(&self, n: i32) -> FontResult<i32> {
        let n2 = self.data.getuint16(4)?;
        self.get_class_index(n, n2)
    }

    pub fn get_mark_attach_class(&self, n: i32) -> FontResult<i32> {
        let n2 = self.data.getuint16(10)?;
        if n2 != 0 {
            return self.get_class_index(n, n2);
        }
        Ok(0)
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(TAG_GDEF, OTByteArray::get_ot_byte_array_builder_instance_from(&self.data));
    }

    pub fn subset_and_stream(&self, subset: &dyn Subset, n: i32, map: &mut BTreeMap<i32, OTByteArrayBuilder>) -> FontResult<()> {
        let mut gdef_subsetter = GdefSubsetter::new(self);
        let builder = gdef_subsetter.subset_and_stream(subset, n)?;
        map.insert(TAG_GDEF, builder);
        Ok(())
    }
}
