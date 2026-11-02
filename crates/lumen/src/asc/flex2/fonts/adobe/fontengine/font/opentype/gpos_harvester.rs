//! Port of afe.jar `com.adobe.fontengine.font.opentype.GposHarvester`.

use std::cell::RefCell;

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::chaining_format1_harvester::ChainingFormat1Harvester;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::chaining_format2_harvester::ChainingFormat2Harvester;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::context_format1_harvester::ContextFormat1Harvester;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::context_format2_harvester::ContextFormat2Harvester;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gpos::Gpos;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table::CoverageConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table::LookupTable;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table_harvester::{gather_lookups_for_coverage, gather_lookups_for_coverage_all_subtables, CoverageLookupHarvester, LookupMap, LookupTableHarvester};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

pub struct GposHarvester<'a> {
    lookup_table: &'a LookupTable,
    num_glyphs: i32,
    /// One instance shared by every type 4-6 lookup; its "already iterated" flag is only reset per lookup.
    two_coverages_harvester: RefCell<NCoveragesHarvester>,
}

impl<'a> GposHarvester<'a> {
    pub fn new(gpos: &'a Gpos, n: i32) -> Self {
        GposHarvester { lookup_table: &gpos.base, num_glyphs: n, two_coverages_harvester: RefCell::new(NCoveragesHarvester::new(2, 2, 1)) }
    }

    pub fn gather_possible_lookups(&mut self, subset: &mut dyn Subset) -> FontResult<LookupMap> {
        let mut tree_map = LookupMap::new();
        let a = self.get_all_feature_list_features()?;
        self.harvest_all_features(&a, &mut tree_map, subset)?;
        Ok(tree_map)
    }

    fn gather_pair_pos_lookups(&self, n: i32, subset: &mut dyn Subset, list: Option<Vec<i32>>, n2: i32, map: &mut LookupMap) -> FontResult<Option<Vec<i32>>> {
        let n3 = self.lookup_table.data.getuint16(n)?;
        match n3 {
            1 => gather_lookups_for_coverage(self, n, 2, subset, list, &mut PairPosFormat1Harvester, n2, map),
            2 => gather_lookups_for_coverage(self, n, 2, subset, list, &mut NoopHarvester, n2, map),
            _ => Err(FontEngineException::invalid_font(format!("Invalid pair pos lookup format ({})", n3))),
        }
    }

    fn gather_pair_pos_lookups_for_all_subtables(&self, subset: &mut dyn Subset, n: i32, map: &mut LookupMap) -> FontResult<Option<Vec<i32>>> {
        let d = &self.lookup_table.data;
        let mut list = None;
        let n2 = d.getuint16(n + 4)?;
        for i in 0..n2 {
            let n3 = d.get_offset(n, 6 + 2 * i)?;
            list = self.gather_pair_pos_lookups(n3, subset, list, i, map)?;
        }
        Ok(list)
    }

    fn gather_contextual_lookup(&self, n: i32, subset: &mut dyn Subset, map: &mut LookupMap, list: Option<Vec<i32>>, n2: i32) -> FontResult<Option<Vec<i32>>> {
        let d = &self.lookup_table.data;
        let n3 = d.getuint16(n)?;
        match n3 {
            1 => gather_lookups_for_coverage(self, n, 2, subset, list, &mut ContextFormat1Harvester::new(false), n2, map),
            2 => gather_lookups_for_coverage(self, n, 2, subset, list, &mut ContextFormat2Harvester, n2, map),
            3 => {
                let n4 = d.getuint16(n + 2)?;
                if n4 > 0 {
                    let mut c = ContextFormat3Harvester { base: NCoveragesHarvester::new(6, d.getuint16(n + 2)?, 1) };
                    return gather_lookups_for_coverage(self, n, 6, subset, list, &mut c, n2, map);
                }
                Ok(list)
            }
            _ => Err(FontEngineException::invalid_font(format!("Invalid contextual lookup format ({})", n3))),
        }
    }

    fn gather_contextual_lookups_for_all_subtables(&self, subset: &mut dyn Subset, n: i32, map: &mut LookupMap) -> FontResult<Option<Vec<i32>>> {
        let d = &self.lookup_table.data;
        let mut list = None;
        let n2 = d.getuint16(n + 4)?;
        for i in 0..n2 {
            let n3 = d.get_offset(n, 6 + 2 * i)?;
            list = self.gather_contextual_lookup(n3, subset, map, list, i)?;
        }
        Ok(list)
    }

    fn gather_chaining_lookup(&self, n: i32, subset: &mut dyn Subset, map: &mut LookupMap, list: Option<Vec<i32>>, n2: i32) -> FontResult<Option<Vec<i32>>> {
        let d = &self.lookup_table.data;
        let n3 = d.getuint16(n)?;
        match n3 {
            1 => gather_lookups_for_coverage(self, n, 2, subset, list, &mut ChainingFormat1Harvester::new(false), n2, map),
            2 => gather_lookups_for_coverage(self, n, 2, subset, list, &mut ChainingFormat2Harvester, n2, map),
            3 => {
                let n4 = d.getuint16(n + 2)?;
                let n5 = d.getuint16(n + 4 + 2 * n4)?;
                if n5 > 0 {
                    let backtrack = d.getuint16(n + 2)?;
                    let mut c = ChainingFormat3Harvester { base: NCoveragesHarvester::new(4, backtrack, 1), backtrack_count: backtrack, subsequent_coverages_tested: false, subsequent_coverages_apply: false };
                    return gather_lookups_for_coverage(self, n, 4 + 2 * n4 + 2, subset, list, &mut c, n2, map);
                }
                Ok(list)
            }
            _ => Err(FontEngineException::invalid_font(format!("Invalid contextual lookup format ({})", n3))),
        }
    }

    fn gather_chaining_lookups_for_all_subtables(&self, subset: &mut dyn Subset, n: i32, map: &mut LookupMap) -> FontResult<Option<Vec<i32>>> {
        let d = &self.lookup_table.data;
        let mut list = None;
        let n2 = d.getuint16(n + 4)?;
        for i in 0..n2 {
            let n3 = d.get_offset(n, 6 + 2 * i)?;
            list = self.gather_chaining_lookup(n3, subset, map, list, i)?;
        }
        Ok(list)
    }

    fn gather_extension_glyphs_for_all_subtables(&self, subset: &mut dyn Subset, n: i32, map: &mut LookupMap) -> FontResult<Option<Vec<i32>>> {
        let d = &self.lookup_table.data;
        let mut array_list: Option<Vec<i32>> = None;
        let mut list: Option<Vec<i32>> = None;
        let n2 = d.getuint16(n + 4)?;
        for i in 0..n2 {
            let n4 = d.get_offset(n, 6 + 2 * i)?;
            let n5 = d.getuint16(n4)?;
            if n5 != 1 {
                return Err(FontEngineException::invalid_font(format!("Invalid extension format ({})", n5)));
            }
            let n6 = d.getuint16(n4 + 2)?;
            let n7 = d.getuint32asint(n4 + 4, "Only signed extension values supported")?;
            let st = n4 + n7;
            list = match n6 {
                1 | 3 => gather_lookups_for_coverage(self, st, 2, subset, list, &mut NoopHarvester, 0, map)?,
                2 => self.gather_pair_pos_lookups(st, subset, list, 0, map)?,
                4..=6 => {
                    let mut c = self.two_coverages_harvester.borrow_mut();
                    c.reset_coverage_info(2, 2, 1);
                    gather_lookups_for_coverage(self, st, 2, subset, list, &mut *c, 0, map)?
                }
                7 => self.gather_contextual_lookup(st, subset, map, list, 0)?,
                8 => self.gather_chaining_lookup(st, subset, map, list, 0)?,
                _ => return Err(FontEngineException::invalid_font(format!("Invalid GPOS lookup type ({})", n6))),
            };
            if list.is_none() {
                continue;
            }
            let a = array_list.get_or_insert_with(Vec::new);
            if !a.contains(&i) {
                a.push(i);
            }
        }
        Ok(array_list)
    }
}

impl LookupTableHarvester for GposHarvester<'_> {
    fn lookup_table(&self) -> &LookupTable {
        self.lookup_table
    }

    fn num_glyphs(&self) -> i32 {
        self.num_glyphs
    }

    fn harvest_subtables(&self, subset: &mut dyn Subset, n: i32, n2: i32, _n3: i32, map: &mut LookupMap) -> FontResult<Option<Vec<i32>>> {
        match n2 {
            1 | 3 => gather_lookups_for_coverage_all_subtables(self, subset, n, &mut NoopHarvester, map),
            2 => self.gather_pair_pos_lookups_for_all_subtables(subset, n, map),
            4..=6 => {
                let mut c = self.two_coverages_harvester.borrow_mut();
                c.reset_coverage_info(2, 2, 1);
                gather_lookups_for_coverage_all_subtables(self, subset, n, &mut *c, map)
            }
            7 => self.gather_contextual_lookups_for_all_subtables(subset, n, map),
            8 => self.gather_chaining_lookups_for_all_subtables(subset, n, map),
            9 => self.gather_extension_glyphs_for_all_subtables(subset, n, map),
            _ => Err(FontEngineException::invalid_font(format!("Invalid GPOS lookup type ({})", n2))),
        }
    }
}

struct Covered {
    covered: bool,
}

impl CoverageConsumer for Covered {
    fn glyph_info(&mut self, _n: i32, _n2: i32) -> FontResult<bool> {
        self.covered = true;
        Ok(false)
    }
}

struct NCoveragesHarvester {
    subsequent_coverages_iterated: bool,
    subsequent_coverages_covered: bool,
    offset_delta_to_coverages: i32,
    num_coverages: i32,
    coverage_start_index: i32,
}

impl NCoveragesHarvester {
    fn new(n: i32, n2: i32, n3: i32) -> Self {
        NCoveragesHarvester { subsequent_coverages_iterated: false, subsequent_coverages_covered: false, offset_delta_to_coverages: n, num_coverages: n2, coverage_start_index: n3 }
    }

    fn reset_coverage_info(&mut self, n: i32, n2: i32, n3: i32) {
        self.subsequent_coverages_iterated = false;
        self.offset_delta_to_coverages = n;
        self.num_coverages = n2;
        self.coverage_start_index = n3;
    }

    fn n_lookup_applies(&mut self, h: &dyn LookupTableHarvester, n2: i32, subset: &dyn Subset) -> FontResult<bool> {
        if self.subsequent_coverages_iterated {
            return Ok(self.subsequent_coverages_covered);
        }
        self.subsequent_coverages_iterated = true;
        let t = h.lookup_table();
        for i in self.coverage_start_index..self.num_coverages {
            let mut c = Covered { covered: false };
            t.iterate_coverage(t.data.get_offset(n2, self.offset_delta_to_coverages + 2 * i)?, Some(subset), &mut c)?;
            if c.covered {
                continue;
            }
            self.subsequent_coverages_covered = false;
            return Ok(false);
        }
        self.subsequent_coverages_covered = true;
        Ok(true)
    }
}

impl CoverageLookupHarvester for NCoveragesHarvester {
    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, _n: i32, n2: i32, _n3: i32, subset: &mut dyn Subset, _map: &mut LookupMap) -> FontResult<bool> {
        self.n_lookup_applies(h, n2, subset)
    }

    fn keep_going(&self) -> bool {
        false
    }
}

struct ChainingFormat3Harvester {
    base: NCoveragesHarvester,
    backtrack_count: i32,
    subsequent_coverages_tested: bool,
    subsequent_coverages_apply: bool,
}

impl CoverageLookupHarvester for ChainingFormat3Harvester {
    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, _n: i32, n2: i32, _n3: i32, subset: &mut dyn Subset, map: &mut LookupMap) -> FontResult<bool> {
        if self.subsequent_coverages_tested {
            return Ok(self.subsequent_coverages_apply);
        }
        self.subsequent_coverages_tested = true;
        let d = &h.lookup_table().data;
        if !self.base.n_lookup_applies(h, n2, subset)? {
            self.subsequent_coverages_apply = false;
            return Ok(false);
        }
        let n4 = d.getuint16(n2 + 4 + 2 * self.backtrack_count)?;
        self.base.reset_coverage_info(4 + 2 * self.backtrack_count + 2, n4, 0);
        if !self.base.n_lookup_applies(h, n2, subset)? {
            self.subsequent_coverages_apply = false;
            return Ok(false);
        }
        let n5 = d.getuint16(n2 + 6 + 2 * self.backtrack_count + 2 * n4)?;
        self.base.reset_coverage_info(8 + 2 * self.backtrack_count + 2 * n4, n5, 0);
        if !self.base.n_lookup_applies(h, n2, subset)? {
            self.subsequent_coverages_apply = false;
            return Ok(false);
        }
        let n6 = d.getuint16(n2 + 8 + 2 * self.backtrack_count + 2 * n4 + 2 * n5)?;
        for i in 0..n6 {
            let n7 = d.getuint16(n2 + 10 + 2 * self.backtrack_count + 2 * n4 + 2 * n5 + 4 * i + 2)?;
            h.harvest(subset, n7, map)?;
        }
        self.subsequent_coverages_apply = true;
        Ok(true)
    }

    fn keep_going(&self) -> bool {
        false
    }
}

struct ContextFormat3Harvester {
    base: NCoveragesHarvester,
}

impl CoverageLookupHarvester for ContextFormat3Harvester {
    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, _n: i32, n2: i32, _n3: i32, subset: &mut dyn Subset, map: &mut LookupMap) -> FontResult<bool> {
        let bl = self.base.n_lookup_applies(h, n2, subset)?;
        if bl {
            let d = &h.lookup_table().data;
            let n4 = d.getuint16(n2 + 4)?;
            for i in 0..n4 {
                let n5 = d.getuint16(n2 + 6 + 2 * self.base.num_coverages + 4 * i + 2)?;
                h.harvest(subset, n5, map)?;
            }
        }
        Ok(bl)
    }

    fn keep_going(&self) -> bool {
        false
    }
}

struct PairPosFormat1Harvester;

impl CoverageLookupHarvester for PairPosFormat1Harvester {
    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, _n: i32, n2: i32, n3: i32, subset: &mut dyn Subset, _map: &mut LookupMap) -> FontResult<bool> {
        let d = &h.lookup_table().data;
        let n4 = d.get_offset(n2, 10 + 2 * n3)?;
        let n5 = Gpos::get_value_record_size(d.getuint16(n2 + 4)?);
        let n6 = Gpos::get_value_record_size(d.getuint16(n2 + 6)?);
        let n7 = d.getuint16(n4)?;
        for i in 0..n7 {
            let n8 = d.getuint16(n4 + 2 + (2 + n5 + n6) * i)?;
            if subset.get_existing_subset_gid(n8)? == -1 {
                continue;
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn keep_going(&self) -> bool {
        false
    }
}

struct NoopHarvester;

impl CoverageLookupHarvester for NoopHarvester {
    fn lookup_applies(&mut self, _h: &dyn LookupTableHarvester, _n: i32, _n2: i32, _n3: i32, _subset: &mut dyn Subset, _map: &mut LookupMap) -> FontResult<bool> {
        Ok(true)
    }

    fn keep_going(&self) -> bool {
        false
    }
}
