//! Port of afe.jar `com.adobe.fontengine.font.opentype.LookupTableHarvester`.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::int_growable_array::IntGrowableArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table::CoverageVisitor;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table::LookupTable;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

/// The `TreeMap` of lookup index to the indexes of its subtables to keep.
pub type LookupMap = BTreeMap<i32, Vec<i32>>;

/// `LookupTableHarvester.CoverageLookupHarvester`; the Java implementations hold `map`, which is always the one
/// `harvest` was given.
pub trait CoverageLookupHarvester {
    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, n: i32, n2: i32, n3: i32, subset: &mut dyn Subset, map: &mut LookupMap) -> FontResult<bool>;
    fn keep_going(&self) -> bool;
}

pub trait LookupTableHarvester {
    fn lookup_table(&self) -> &LookupTable;
    fn num_glyphs(&self) -> i32;
    fn harvest_subtables(&self, subset: &mut dyn Subset, n: i32, n2: i32, n3: i32, map: &mut LookupMap) -> FontResult<Option<Vec<i32>>>;

    fn get_all_feature_list_features(&self) -> FontResult<IntGrowableArray> {
        let t = self.lookup_table();
        let n = t.get_feature_list_offset()?;
        let n2 = t.data.getuint16(n)?;
        let mut a = IntGrowableArray::new();
        for i in 0..n2 {
            let n3 = n + t.data.getuint16(n + 2 + i * 6 + 4)?;
            let n4 = t.data.getuint16(n3 + 2)?;
            for j in 0..n4 {
                a.set(t.data.getuint16(n3 + 4 + j * 2)?);
            }
        }
        let len = a.get_length() as usize;
        a.get_buffer_mut()[..len].sort();
        Ok(a)
    }

    fn harvest_all_features(&self, a: &IntGrowableArray, map: &mut LookupMap, subset: &mut dyn Subset) -> FontResult<()> {
        if a.get_length() > 0 {
            self.harvest(subset, a.get_buffer()[0], map)?;
            for i in 1..a.get_length() {
                if a.get(i) == a.get(i - 1) {
                    continue;
                }
                self.harvest(subset, a.get(i), map)?;
            }
        }
        Ok(())
    }

    fn harvest(&self, subset: &mut dyn Subset, n: i32, map: &mut LookupMap) -> FontResult<()> {
        let t = self.lookup_table();
        let n4 = t.get_lookup_list_offset()?;
        let n5 = t.data.get_offset(n4, 2 + 2 * n)?;
        let n3 = t.data.getuint16(n5)?;
        let n2 = t.data.getuint16(n5 + 2)?;
        if let Some(list) = self.harvest_subtables(subset, n5, n3, n2, map)? {
            map.insert(n, list);
        }
        Ok(())
    }

}

/// `gatherLookupsForCoverage`, callable on a `dyn LookupTableHarvester`.
#[allow(clippy::too_many_arguments)]
pub fn gather_lookups_for_coverage(h: &dyn LookupTableHarvester, n: i32, n2: i32, subset: &mut dyn Subset, list: Option<Vec<i32>>, c: &mut dyn CoverageLookupHarvester, n3: i32, map: &mut LookupMap) -> FontResult<Option<Vec<i32>>> {
    let t = h.lookup_table();
    let offset = t.data.get_offset(n, n2)?;
    let mut ch = CoverageHarvester { h, st_offset: n, gids: subset, harvester: c, subtable: n3, subtables: list, map };
    t.iterate_coverage_with(offset, &mut ch)?;
    Ok(ch.subtables)
}

/// `gatherLookupsForCoverageAllSubtables`.
pub fn gather_lookups_for_coverage_all_subtables(h: &dyn LookupTableHarvester, subset: &mut dyn Subset, n: i32, c: &mut dyn CoverageLookupHarvester, map: &mut LookupMap) -> FontResult<Option<Vec<i32>>> {
    let t = h.lookup_table();
    let mut list = None;
    let n2 = t.data.getuint16(n + 4)?;
    for i in 0..n2 {
        let n3 = t.data.get_offset(n, 6 + 2 * i)?;
        list = gather_lookups_for_coverage(h, n3, 2, subset, list, c, i, map)?;
    }
    Ok(list)
}

/// `LookupTableHarvester.CoverageHarvester`.
struct CoverageHarvester<'h, 's, 'c, 'm> {
    h: &'h dyn LookupTableHarvester,
    st_offset: i32,
    gids: &'s mut dyn Subset,
    harvester: &'c mut dyn CoverageLookupHarvester,
    subtable: i32,
    subtables: Option<Vec<i32>>,
    map: &'m mut LookupMap,
}

impl CoverageVisitor for CoverageHarvester<'_, '_, '_, '_> {
    fn skip(&mut self, gid: i32) -> FontResult<bool> {
        Ok(self.gids.get_existing_subset_gid(gid)? == -1)
    }

    fn glyph_info(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        if self.harvester.lookup_applies(self.h, n, self.st_offset, n2, self.gids, self.map)? {
            let list = self.subtables.get_or_insert_with(Vec::new);
            if !list.contains(&self.subtable) {
                list.push(self.subtable);
            }
            return Ok(self.harvester.keep_going());
        }
        Ok(true)
    }
}
