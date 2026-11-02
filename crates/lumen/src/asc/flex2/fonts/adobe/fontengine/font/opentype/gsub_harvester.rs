//! Port of afe.jar `com.adobe.fontengine.font.opentype.GsubHarvester`.

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::chaining_format1_harvester::ChainingFormat1Harvester;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::chaining_format2_harvester::ChainingFormat2Harvester;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::context_format1_harvester::ContextFormat1Harvester;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::context_format2_harvester::ContextFormat2Harvester;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gsub::Gsub;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table::CoverageConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table::LookupTable;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table_harvester::{gather_lookups_for_coverage, gather_lookups_for_coverage_all_subtables, CoverageLookupHarvester, LookupMap, LookupTableHarvester};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{ExceptionKind, FontEngineException, FontResult};

pub struct GsubHarvester<'a> {
    lookup_table: &'a LookupTable,
    num_glyphs: i32,
}

/// `catch (InvalidFontException e)` around `subset.getSubsetGid`: `Ok(false)` when it was caught.
fn try_get_subset_gid(subset: &mut dyn Subset, n: i32) -> FontResult<bool> {
    match subset.get_subset_gid(n) {
        Ok(_) => Ok(true),
        Err(e) if matches!(e.kind, ExceptionKind::InvalidFont | ExceptionKind::InvalidGlyph) => Ok(false),
        Err(e) => Err(e),
    }
}

impl<'a> GsubHarvester<'a> {
    pub fn new(gsub: &'a Gsub, n: i32) -> Self {
        GsubHarvester { lookup_table: &gsub.base, num_glyphs: n }
    }

    pub fn gather_possible_lookups(&mut self, subset: &mut dyn Subset) -> FontResult<LookupMap> {
        let mut tree_map = LookupMap::new();
        let a = self.get_all_feature_list_features()?;
        loop {
            let n = subset.get_num_glyphs();
            self.harvest_all_features(&a, &mut tree_map, subset)?;
            if subset.get_num_glyphs() == n {
                break;
            }
        }
        Ok(tree_map)
    }

    fn gather_contextual_glyphs(&self, n: i32, subset: &mut dyn Subset, map: &mut LookupMap, list: Option<Vec<i32>>, n2: i32) -> FontResult<Option<Vec<i32>>> {
        let d = &self.lookup_table.data;
        let n3 = d.getuint16(n)?;
        match n3 {
            1 => gather_lookups_for_coverage(self, n, 2, subset, list, &mut ContextFormat1Harvester::new(true), n2, map),
            2 => gather_lookups_for_coverage(self, n, 2, subset, list, &mut ContextFormat2Harvester, n2, map),
            3 => {
                let n4 = d.getuint16(n + 2)?;
                if n4 <= 0 {
                    return Ok(list);
                }
                gather_lookups_for_coverage(self, n, 6, subset, list, &mut ContextFormat3SubstHarvester { coverage_contains_gid: false }, n2, map)
            }
            _ => Err(FontEngineException::invalid_font(format!("Invalid contextual lookup format ({})", n3))),
        }
    }

    fn gather_contextual_glyphs_for_all_subtables(&self, subset: &mut dyn Subset, n: i32, map: &mut LookupMap) -> FontResult<Option<Vec<i32>>> {
        let d = &self.lookup_table.data;
        let mut list = None;
        let n2 = d.getuint16(n + 4)?;
        for i in 0..n2 {
            let n3 = d.get_offset(n, 6 + 2 * i)?;
            list = self.gather_contextual_glyphs(n3, subset, map, list, i)?;
        }
        Ok(list)
    }

    fn gather_single_subst_glyphs_for_all_subtables(&self, subset: &mut dyn Subset, n: i32, map: &mut LookupMap) -> FontResult<Option<Vec<i32>>> {
        let d = &self.lookup_table.data;
        let mut list = None;
        let n2 = d.getuint16(n + 4)?;
        for i in 0..n2 {
            let n3 = d.get_offset(n, 6 + 2 * i)?;
            let n4 = d.getuint16(n3)?;
            match n4 {
                1 => list = gather_lookups_for_coverage(self, n3, 2, subset, list, &mut SingleSubstFormat1Harvester, i, map)?,
                2 => list = gather_lookups_for_coverage(self, n3, 2, subset, list, &mut SingleSubstFormat2Harvester, i, map)?,
                _ => {}
            }
        }
        Ok(list)
    }

    fn gather_chaining_glyphs(&self, n: i32, subset: &mut dyn Subset, map: &mut LookupMap, list: Option<Vec<i32>>, n2: i32) -> FontResult<Option<Vec<i32>>> {
        let d = &self.lookup_table.data;
        let n3 = d.getuint16(n)?;
        match n3 {
            1 => gather_lookups_for_coverage(self, n, 2, subset, list, &mut ChainingFormat1Harvester::new(true), n2, map),
            2 => gather_lookups_for_coverage(self, n, 2, subset, list, &mut ChainingFormat2Harvester, n2, map),
            3 => {
                let n4 = d.getuint16(n + 2)?;
                let n5 = d.getuint16(n + 4 + 2 * n4)?;
                if n5 <= 0 {
                    return Ok(list);
                }
                gather_lookups_for_coverage(self, n, 4 + 2 * n4 + 2, subset, list, &mut ChainingFormat3SubstHarvester { coverage_contains_gid: false }, n2, map)
            }
            _ => Err(FontEngineException::invalid_font(format!("Invalid contextual lookup format ({})", n3))),
        }
    }

    fn gather_chaining_glyphs_for_all_subtables(&self, subset: &mut dyn Subset, n: i32, map: &mut LookupMap) -> FontResult<Option<Vec<i32>>> {
        let d = &self.lookup_table.data;
        let mut list = None;
        let n2 = d.getuint16(n + 4)?;
        for i in 0..n2 {
            let n3 = d.get_offset(n, 6 + 2 * i)?;
            list = self.gather_chaining_glyphs(n3, subset, map, list, i)?;
        }
        Ok(list)
    }

    fn gather_extension_glyphs_for_all_subtables(&self, subset: &mut dyn Subset, n: i32, map: &mut LookupMap, _n2: i32) -> FontResult<Option<Vec<i32>>> {
        let d = &self.lookup_table.data;
        let mut array_list: Option<Vec<i32>> = None;
        let mut list: Option<Vec<i32>> = None;
        let n3 = d.getuint16(n + 4)?;
        for i in 0..n3 {
            let n5 = d.get_offset(n, 6 + 2 * i)?;
            let n6 = d.getuint16(n5)?;
            if n6 != 1 {
                return Err(FontEngineException::invalid_font(format!("Invalid extension format ({})", n6)));
            }
            let n7 = d.getuint16(n5 + 2)?;
            let n8 = d.getuint32asint(n5 + 4, "Only signed extension values supported")?;
            let st = n5 + n8;
            list = match n7 {
                1 => {
                    if d.getuint16(st)? == 1 {
                        gather_lookups_for_coverage(self, st, 2, subset, list, &mut SingleSubstFormat1Harvester, 0, map)?
                    } else {
                        gather_lookups_for_coverage(self, st, 2, subset, list, &mut SingleSubstFormat2Harvester, 0, map)?
                    }
                }
                2 => gather_lookups_for_coverage(self, st, 2, subset, list, &mut MultSubstHarvester, 0, map)?,
                3 => gather_lookups_for_coverage(self, st, 2, subset, list, &mut AltSubstHarvester, 0, map)?,
                4 => gather_lookups_for_coverage(self, st, 2, subset, list, &mut LigaSubstHarvester, 0, map)?,
                5 => self.gather_contextual_glyphs(st, subset, map, list, 0)?,
                6 => self.gather_chaining_glyphs(st, subset, map, list, 0)?,
                _ => return Err(FontEngineException::invalid_font(format!("Invalid GSUB lookup type ({})", n7))),
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

impl LookupTableHarvester for GsubHarvester<'_> {
    fn lookup_table(&self) -> &LookupTable {
        self.lookup_table
    }

    fn num_glyphs(&self) -> i32 {
        self.num_glyphs
    }

    fn harvest_subtables(&self, subset: &mut dyn Subset, n: i32, n2: i32, n3: i32, map: &mut LookupMap) -> FontResult<Option<Vec<i32>>> {
        match n2 {
            1 => self.gather_single_subst_glyphs_for_all_subtables(subset, n, map),
            2 => gather_lookups_for_coverage_all_subtables(self, subset, n, &mut MultSubstHarvester, map),
            3 => gather_lookups_for_coverage_all_subtables(self, subset, n, &mut AltSubstHarvester, map),
            4 => gather_lookups_for_coverage_all_subtables(self, subset, n, &mut LigaSubstHarvester, map),
            5 => self.gather_contextual_glyphs_for_all_subtables(subset, n, map),
            6 => self.gather_chaining_glyphs_for_all_subtables(subset, n, map),
            7 => self.gather_extension_glyphs_for_all_subtables(subset, n, map, n3),
            _ => Err(FontEngineException::invalid_font(format!("Invalid GSUB lookup type ({})", n2))),
        }
    }
}

/// The `CoverageConsumer` half of the two format-3 harvesters: stops at the first glyph found.
struct CoverageContainsGid {
    found: bool,
}

impl CoverageConsumer for CoverageContainsGid {
    fn glyph_info(&mut self, _n: i32, _n2: i32) -> FontResult<bool> {
        self.found = true;
        Ok(false)
    }
}

struct ContextFormat3SubstHarvester {
    coverage_contains_gid: bool,
}

impl CoverageLookupHarvester for ContextFormat3SubstHarvester {
    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, _n: i32, n2: i32, _n3: i32, subset: &mut dyn Subset, map: &mut LookupMap) -> FontResult<bool> {
        let t = h.lookup_table();
        let n5 = t.data.getuint16(n2 + 2)?;
        if n5 == 1 {
            self.coverage_contains_gid = true;
        }
        let mut n4 = 1;
        while n4 < n5 {
            let mut c = CoverageContainsGid { found: false };
            t.iterate_coverage(t.data.get_offset(n2, 6 + n4 * 2)?, Some(&*subset), &mut c)?;
            self.coverage_contains_gid = c.found;
            if !self.coverage_contains_gid {
                break;
            }
            n4 += 1;
        }
        if n4 == n5 {
            let n6 = t.data.getuint16(n2 + 4)?;
            for i in 0..n6 {
                let n7 = t.data.getuint16(n2 + 6 + 2 * n5 + 4 * i + 2)?;
                h.harvest(subset, n7, map)?;
            }
        }
        Ok(self.coverage_contains_gid)
    }

    fn keep_going(&self) -> bool {
        true
    }
}

struct ChainingFormat3SubstHarvester {
    coverage_contains_gid: bool,
}

impl ChainingFormat3SubstHarvester {
    fn check_coverages(&mut self, h: &dyn LookupTableHarvester, n: i32, n2: i32, n3: i32, subset: &dyn Subset) -> FontResult<bool> {
        let t = h.lookup_table();
        for i in 0..n3 {
            let mut c = CoverageContainsGid { found: false };
            t.iterate_coverage(t.data.get_offset(n, n2 + 2 * i)?, Some(subset), &mut c)?;
            self.coverage_contains_gid = c.found;
            if self.coverage_contains_gid {
                continue;
            }
            return Ok(false);
        }
        Ok(true)
    }
}

impl CoverageLookupHarvester for ChainingFormat3SubstHarvester {
    /// Follows the bytecode; CFR's rendering of the input-coverage step reads the count from the wrong offset.
    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, _n: i32, n2: i32, _n3: i32, subset: &mut dyn Subset, map: &mut LookupMap) -> FontResult<bool> {
        let d = &h.lookup_table().data;
        let n7 = d.getuint16(n2 + 2)?;
        let mut n6 = 4;
        if !self.check_coverages(h, n2, n6, n7, subset)? {
            return Ok(false);
        }
        n6 += 2 * n7;
        let n8 = d.getuint16(n2 + n6)?;
        n6 += 2;
        if !self.check_coverages(h, n2, n6 + 2, n8 - 1, subset)? {
            return Ok(false);
        }
        n6 += 2 * n8;
        let n9 = d.getuint16(n2 + n6)?;
        n6 += 2;
        if !self.check_coverages(h, n2, n6, n9, subset)? {
            return Ok(false);
        }
        n6 += 2 * n9;
        let n10 = d.getuint16(n2 + n6)?;
        n6 += 2;
        for i in 0..n10 {
            let n12 = d.getuint16(n2 + n6 + 4 * i + 2)?;
            h.harvest(subset, n12, map)?;
        }
        Ok(true)
    }

    fn keep_going(&self) -> bool {
        true
    }
}

struct LigaSubstHarvester;

impl CoverageLookupHarvester for LigaSubstHarvester {
    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, _n: i32, n2: i32, n3: i32, subset: &mut dyn Subset, _map: &mut LookupMap) -> FontResult<bool> {
        let d = &h.lookup_table().data;
        let n4 = d.get_offset(n2, 6 + n3 * 2)?;
        let n5 = d.getuint16(n4)?;
        let mut bl = false;
        for i in 0..n5 {
            let n8 = d.get_offset(n4, 2 + i * 2)?;
            let n9 = d.getuint16(n8 + 2)?;
            let mut n7 = 0;
            while n7 < n9 - 1 && subset.get_existing_subset_gid(d.getuint16(n8 + 4 + 2 * n7)?)? != -1 {
                n7 += 1;
            }
            if n7 != n9 - 1 {
                continue;
            }
            bl = true;
            let n6 = d.getuint16(n8)?;
            if !try_get_subset_gid(subset, n6)? {
                bl = false;
            }
        }
        Ok(bl)
    }

    fn keep_going(&self) -> bool {
        true
    }
}

struct AltSubstHarvester;

impl CoverageLookupHarvester for AltSubstHarvester {
    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, _n: i32, n2: i32, n3: i32, subset: &mut dyn Subset, _map: &mut LookupMap) -> FontResult<bool> {
        let d = &h.lookup_table().data;
        let n4 = d.get_offset(n2, 6 + n3 * 2)?;
        let n5 = d.getuint16(n4)?;
        for i in 0..n5 {
            let n6 = d.getuint16(n4 + 2 + 2 * i)?;
            if !try_get_subset_gid(subset, n6)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn keep_going(&self) -> bool {
        true
    }
}

struct MultSubstHarvester;

impl CoverageLookupHarvester for MultSubstHarvester {
    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, _n: i32, n2: i32, n3: i32, subset: &mut dyn Subset, _map: &mut LookupMap) -> FontResult<bool> {
        let d = &h.lookup_table().data;
        let n4 = d.get_offset(n2, 6 + 2 * n3)?;
        let n5 = d.getuint16(n4)?;
        for i in 0..n5 {
            if !try_get_subset_gid(subset, d.getuint16(n4 + 2 + 2 * i)?)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn keep_going(&self) -> bool {
        true
    }
}

struct SingleSubstFormat2Harvester;

impl CoverageLookupHarvester for SingleSubstFormat2Harvester {
    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, _n: i32, n2: i32, n3: i32, subset: &mut dyn Subset, _map: &mut LookupMap) -> FontResult<bool> {
        let n4 = h.lookup_table().data.getuint16(n2 + 6 + 2 * n3)?;
        try_get_subset_gid(subset, n4)
    }

    fn keep_going(&self) -> bool {
        true
    }
}

struct SingleSubstFormat1Harvester;

impl CoverageLookupHarvester for SingleSubstFormat1Harvester {
    fn lookup_applies(&mut self, h: &dyn LookupTableHarvester, n: i32, n2: i32, _n3: i32, subset: &mut dyn Subset, _map: &mut LookupMap) -> FontResult<bool> {
        let n4 = h.lookup_table().data.getint16(n2 + 4)?;
        try_get_subset_gid(subset, n + n4)
    }

    fn keep_going(&self) -> bool {
        true
    }
}
