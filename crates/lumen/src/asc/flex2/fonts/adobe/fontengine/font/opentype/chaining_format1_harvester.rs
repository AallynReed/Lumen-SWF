//! Port of afe.jar `com.adobe.fontengine.font.opentype.ChainingFormat1Harvester`.

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table_harvester::{CoverageLookupHarvester, LookupMap, LookupTableHarvester};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub struct ChainingFormat1Harvester {
    continue_iterating: bool,
}

impl ChainingFormat1Harvester {
    pub fn new(continue_iterating: bool) -> Self {
        ChainingFormat1Harvester { continue_iterating }
    }

    fn glyphs_apply(h: &dyn LookupTableHarvester, n: i32, n2: i32, n3: i32, subset: &dyn Subset) -> FontResult<bool> {
        for i in 0..n {
            let n4 = h.lookup_table().data.getuint16(n2 + n3 + 2 + 2 * i)?;
            if subset.get_existing_subset_gid(n4)? != -1 {
                continue;
            }
            return Ok(false);
        }
        Ok(true)
    }
}

impl CoverageLookupHarvester for ChainingFormat1Harvester {
    fn keep_going(&self) -> bool {
        self.continue_iterating
    }

    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, _n: i32, n2: i32, n3: i32, subset: &mut dyn Subset, map: &mut LookupMap) -> FontResult<bool> {
        let d = &h.lookup_table().data;
        let n4 = d.get_offset(n2, 6 + 2 * n3)?;
        let n5 = d.getuint16(n4)?;
        let mut bl = false;
        for i in 0..n5 {
            let n9 = d.get_offset(n4, 2 + 2 * i)?;
            let n10 = d.getuint16(n9)?;
            if !Self::glyphs_apply(h, n10, n9, 0, subset)? {
                continue;
            }
            let mut n7 = 2 + 2 * n10;
            let n8 = d.getuint16(n9 + n7)? - 1;
            if !Self::glyphs_apply(h, n8, n9, n7, subset)? {
                continue;
            }
            n7 += 2 + 2 * n8;
            let n6 = d.getuint16(n9 + n7)?;
            if !Self::glyphs_apply(h, n6, n9, n7, subset)? {
                continue;
            }
            n7 += 2 + 2 * n6;
            let n11 = d.getuint16(n9 + n7)?;
            for j in 0..n11 {
                let n12 = d.getuint16(n9 + n7 + 2 + 4 * j + 2)?;
                h.harvest(subset, n12, map)?;
            }
            bl = true;
        }
        Ok(bl)
    }
}
