//! Port of afe.jar `com.adobe.fontengine.font.opentype.ContextFormat2Harvester`.

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::chaining_format2_harvester::ClassCoveredBySet;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table_harvester::{CoverageLookupHarvester, LookupMap, LookupTableHarvester};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub struct ContextFormat2Harvester;

impl CoverageLookupHarvester for ContextFormat2Harvester {
    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, n: i32, n2: i32, _n3: i32, subset: &mut dyn Subset, map: &mut LookupMap) -> FontResult<bool> {
        let t = h.lookup_table();
        let d = &t.data;
        let n4 = d.get_offset(n2, 4)?;
        let n5 = t.get_class_index(n, n4)?;
        let n6 = d.get_offset(n2, 8 + 2 * n5)?;
        if n6 == 0 {
            return Ok(false);
        }
        let mut bl = false;
        let n7 = d.getuint16(n6)?;
        for i in 0..n7 {
            let n10 = d.get_offset(n6, 2 + 2 * i)?;
            let n11 = d.getuint16(n10)?;
            let mut covered = true;
            for n9 in 0..n11 - 1 {
                let n8 = d.getuint16(n10 + 4 + 2 * n9)?;
                let mut c = ClassCoveredBySet { gids: &*subset, class_covered_by_set: false };
                t.iterate_class(n4, h.num_glyphs(), &mut c, n8)?;
                covered = c.class_covered_by_set;
                if !covered {
                    break;
                }
            }
            if !covered {
                continue;
            }
            let n8 = d.getuint16(n10 + 2)?;
            for n9 in 0..n8 {
                let n12 = d.getuint16(n10 + 4 + 2 * (n11 - 1) + 4 * n9 + 2)?;
                h.harvest(subset, n12, map)?;
            }
            bl = true;
        }
        Ok(bl)
    }

    fn keep_going(&self) -> bool {
        true
    }
}
