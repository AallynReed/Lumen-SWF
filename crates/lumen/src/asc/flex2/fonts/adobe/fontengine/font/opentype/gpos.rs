//! Port of afe.jar `com.adobe.fontengine.font.opentype.Gpos` (the parts outside inline formatting).

use std::collections::BTreeMap;
use std::ops::Deref;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gpos_subsetter::GposSubsetter;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table::LookupTable;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table_subsetter::LookupTableSubsetter;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::optical_size_data::OpticalSizeData;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub const TAG_GPOS: i32 = 1196445523;
/// `Tag.feature_size`.
const FEATURE_SIZE: i32 = 0x73697A65;

#[derive(Clone, Debug, Default)]
pub struct Gpos {
    pub base: LookupTable,
}

impl Deref for Gpos {
    type Target = LookupTable;
    fn deref(&self) -> &LookupTable {
        &self.base
    }
}

impl Gpos {
    pub fn new(font_byte_array: FontByteArray) -> FontResult<Gpos> {
        Ok(Gpos { base: LookupTable::new(font_byte_array) })
    }

    pub fn get_value_record_size(n: i32) -> i32 {
        let mut n2 = 0;
        for bit in [1, 2, 4, 8, 0x10, 0x20, 0x40, 0x80] {
            if (n & bit) != 0 {
                n2 += 2;
            }
        }
        n2
    }

    pub fn get_optical_size_data(&self, bl: bool) -> FontResult<Option<OpticalSizeData>> {
        let n = self.get_feature_list_offset()?;
        let n2 = self.data.getuint16(n)?;
        for i in 0..n2 {
            if self.data.getint32(n + 2 + 6 * i)? != FEATURE_SIZE {
                continue;
            }
            let n3 = self.data.get_offset(n, 2 + 6 * i + 4)?;
            let mut n4 = self.data.get_offset(0, n3)?;
            n4 += if bl { n } else { n3 };
            return Ok(Some(OpticalSizeData::new(self.data.getuint16(n4)?, self.data.getuint16(n4 + 2)?, self.data.getuint16(n4 + 4)?, self.data.getuint16(n4 + 6)?, self.data.getuint16(n4 + 8)?)));
        }
        Ok(None)
    }

    /// `lookups` is what `GposHarvester.gatherPossibleLookups` returned; `has_kern` is whether `map` holds a 'kern'.
    pub fn subset_and_stream(&self, subset: &dyn Subset, lookups: &BTreeMap<i32, Vec<i32>>, map: &mut BTreeMap<i32, OTByteArrayBuilder>, n: i32, has_kern: bool) -> FontResult<()> {
        let mut s = GposSubsetter::new(&self.base, n, has_kern);
        let builder = s.subset_and_stream(lookups, subset)?;
        map.insert(TAG_GPOS, builder);
        Ok(())
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(TAG_GPOS, OTByteArray::get_ot_byte_array_builder_instance_from(&self.data));
    }
}
