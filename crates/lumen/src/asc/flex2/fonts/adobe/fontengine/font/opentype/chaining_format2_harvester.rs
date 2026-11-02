//! Port of afe.jar `com.adobe.fontengine.font.opentype.ChainingFormat2Harvester`.

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table::ClassConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table_harvester::{CoverageLookupHarvester, LookupMap, LookupTableHarvester};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub struct ChainingFormat2Harvester;

/// The `ClassConsumer` half of `ChainingFormat2Harvester` and `ContextFormat2Harvester`.
pub struct ClassCoveredBySet<'s> {
    pub gids: &'s dyn Subset,
    pub class_covered_by_set: bool,
}

impl ClassConsumer for ClassCoveredBySet<'_> {
    fn glyph(&mut self, n: i32, _n2: i32) -> FontResult<bool> {
        if self.gids.get_existing_subset_gid(n)? != -1 {
            self.class_covered_by_set = true;
            return Ok(false);
        }
        Ok(true)
    }
}

impl ChainingFormat2Harvester {
    fn classes_apply(h: &dyn LookupTableHarvester, gids: &dyn Subset, n: i32, n2: i32, n3: i32, n4: i32) -> FontResult<bool> {
        let t = h.lookup_table();
        for i in 0..n {
            let n5 = t.data.getuint16(n3 + n4 + 2 * i)?;
            let mut c = ClassCoveredBySet { gids, class_covered_by_set: false };
            t.iterate_class(n2, h.num_glyphs(), &mut c, n5)?;
            if c.class_covered_by_set {
                continue;
            }
            return Ok(false);
        }
        Ok(true)
    }
}

impl CoverageLookupHarvester for ChainingFormat2Harvester {
    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, n: i32, n2: i32, _n3: i32, subset: &mut dyn Subset, map: &mut LookupMap) -> FontResult<bool> {
        let t = h.lookup_table();
        let d = &t.data;
        let n4 = d.get_offset(n2, 6)?;
        let n5 = t.get_class_index(n, n4)?;
        let n6 = d.get_offset(n2, 12 + 2 * n5)?;
        if n6 == 0 {
            return Ok(false);
        }
        let n7 = d.get_offset(n2, 4)?;
        let n8 = d.get_offset(n2, 8)?;
        let mut bl = false;
        let n9 = d.getuint16(n6)?;
        for i in 0..n9 {
            let n12 = d.get_offset(n6, 2 + 2 * i)?;
            let n13 = d.getuint16(n12)?;
            let mut n14 = 0;
            if !Self::classes_apply(h, subset, n13, n7, n12, 2)? {
                continue;
            }
            n14 += 2 + 2 * n13;
            let n11 = d.getuint16(n12 + n14)? - 1;
            if !Self::classes_apply(h, subset, n11, n4, n12, n14 + 2)? {
                continue;
            }
            n14 += 2 + 2 * n11;
            let n10 = d.getuint16(n12 + n14)?;
            if !Self::classes_apply(h, subset, n10, n8, n12, n14 + 2)? {
                continue;
            }
            n14 += 2 + 2 * n10;
            let n15 = d.getuint16(n12 + n14)?;
            for j in 0..n15 {
                let n16 = d.getuint16(n12 + n14 + 2 + 4 * j + 2)?;
                h.harvest(subset, n16, map)?;
            }
            bl = true;
        }
        Ok(bl)
    }

    fn keep_going(&self) -> bool {
        true
    }
}
