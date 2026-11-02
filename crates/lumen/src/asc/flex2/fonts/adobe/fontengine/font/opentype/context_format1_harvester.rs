//! Port of afe.jar `com.adobe.fontengine.font.opentype.ContextFormat1Harvester`.

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table_harvester::{CoverageLookupHarvester, LookupMap, LookupTableHarvester};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub struct ContextFormat1Harvester {
    continue_iterating: bool,
}

impl ContextFormat1Harvester {
    pub fn new(continue_iterating: bool) -> Self {
        ContextFormat1Harvester { continue_iterating }
    }
}

impl CoverageLookupHarvester for ContextFormat1Harvester {
    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, _n: i32, n2: i32, n3: i32, subset: &mut dyn Subset, map: &mut LookupMap) -> FontResult<bool> {
        let d = &h.lookup_table().data;
        let n4 = d.get_offset(n2, 6 + 2 * n3)?;
        let n5 = d.getuint16(n4)?;
        let mut bl = false;
        for i in 0..n5 {
            let n8 = d.get_offset(n4, 2 + 2 * i)?;
            let n9 = d.getuint16(n8)?;
            let mut n7 = 0;
            while n7 < n9 - 1 && subset.get_existing_subset_gid(d.getuint16(n8 + 4 + 2 * n7)?)? != -1 {
                n7 += 1;
            }
            if n7 != n9 - 1 {
                continue;
            }
            let n6 = d.getuint16(n8 + 2)?;
            for n7 in 0..n6 {
                let n10 = d.getuint16(n8 + 4 + 2 * (n9 - 1) + 4 * n7 + 2)?;
                h.harvest(subset, n10, map)?;
            }
            bl = true;
        }
        Ok(bl)
    }

    fn keep_going(&self) -> bool {
        self.continue_iterating
    }
}
