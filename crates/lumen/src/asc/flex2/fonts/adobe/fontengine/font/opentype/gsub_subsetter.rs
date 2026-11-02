//! Port of afe.jar `com.adobe.fontengine.font.opentype.GsubSubsetter`.

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::chaining_generator;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::contextual_generator;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table::{CoverageConsumer, LayoutTable};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table::LookupTable;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table_subsetter::{add_to_coverages_map, patch_coverage_at_offset, CoverageMap, LookupSubset, LookupSubtableGenerator, LookupTableSubsetter, LookupTableSubsetterBase};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::set_generator::{SetGenerator, SetMembers};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

pub struct GsubSubsetter<'a> {
    pub base: LookupTableSubsetterBase<'a>,
}

impl<'a> GsubSubsetter<'a> {
    pub fn new(lookup_table: &'a LookupTable, n: i32) -> Self {
        GsubSubsetter { base: LookupTableSubsetterBase::new(lookup_table, OTByteArray::get_ot_byte_array_builder_instance_sized(10), n) }
    }

    fn write_type1_format1(&mut self, _n: i32, n2: i32, n3: i32) -> i32 {
        let b = &mut self.base.builder;
        b.ensure_capacity(n2 + 6);
        b.setuint16(n2, 1);
        b.setint16(n2 + 4, n3);
        6
    }
}

impl<'a> LookupTableSubsetter<'a> for GsubSubsetter<'a> {
    fn base(&self) -> &LookupTableSubsetterBase<'a> {
        &self.base
    }

    fn base_mut(&mut self) -> &mut LookupTableSubsetterBase<'a> {
        &mut self.base
    }

    fn patch_subtable_coverage(&mut self, n: i32, n2: i32, map: &CoverageMap, n3: i32) -> FontResult<()> {
        let t: &LayoutTable = self.base.orig_table;
        let b = &mut self.base.builder;
        match n3 {
            1..=4 => patch_coverage_at_offset(t, b, n, n2, 2, map),
            5 => contextual_generator::patch_subtable_coverage(t, b, n, n2, map, n3),
            6 => chaining_generator::patch_subtable_coverage(t, b, n, n2, map, n3),
            7 => Ok(()),
            _ => Err(FontEngineException::invalid_font(format!("Unrecognized lookup type: {}", n3))),
        }
    }

    fn write_subtable(&mut self, n: i32, n2: i32, map: Option<&CoverageMap>, n3: i32, subset: Option<&dyn Subset>, lookup_subset: Option<&LookupSubset>) -> FontResult<i32> {
        let t: &LayoutTable = self.base.orig_table;
        let num_glyphs = self.base.orig_num_glyphs;
        let npe = "java.lang.NullPointerException";
        match n3 {
            1 => {
                let subset = subset.expect(npe);
                let n4 = t.data.getuint16(n)?;
                let fetcher = match n4 {
                    1 => Type1TargetGIDFetcher::Format1 { delta: t.data.getint16(n + 4)? },
                    2 => Type1TargetGIDFetcher::Format2 { orig_st_offset: n },
                    _ => return Err(FontEngineException::invalid_font(format!("Invalid lookup type 1 format:{}", n4))),
                };
                let mut dd = DeltaDeterminer { subset, format1_ok: true, delta: 0, delta_initialized: false, fetcher, t };
                t.iterate_coverage(t.data.get_offset(n, 2)?, Some(subset), &mut dd)?;
                if dd.format1_ok {
                    return Ok(self.write_type1_format1(n, n2, dd.delta));
                }
                let map = map.expect(npe);
                let mut g = Type1Format2Generator { t, subset, builder: &mut self.base.builder, new_st_offset: n2, glyph_count: 0, fetcher, subset_to_target: vec![-1; subset.get_num_glyphs().max(0) as usize], orig_st_offset: n, new_coverages: map };
                g.write_subtable()
            }
            2 | 3 => {
                let subset = subset.expect(npe);
                let map = map.expect(npe);
                let n5 = t.data.get_offset(n, 2)?;
                let glyph_count = map.get(&n5).expect(npe).glyph_count;
                let mut g = Type2Or3Generator { t, subset, builder: &mut self.base.builder, new_st_offset: n2, orig_st_offset: n, subset_to_coverage_index: vec![-1; subset.get_num_glyphs().max(0) as usize], glyph_count };
                g.write_subtable()
            }
            4 => {
                let subset = subset.expect(npe);
                let map = map.expect(npe);
                let n5 = t.data.get_offset(n, 2)?;
                let count = map.get(&n5).expect(npe).glyph_count;
                let g = SetGenerator::new(t, subset, &mut self.base.builder, n2, n, count, 4, false, true)?;
                g.builder.ensure_capacity(n2 + 6 + 2 * count);
                g.builder.setuint16(n2, 1);
                g.builder.setuint16(n2 + 4, count);
                let mut gen = Type4Generator { g, m: Type4Members };
                t.iterate_coverage(n5, Some(subset), &mut gen)?;
                gen.write_subtable()
            }
            5 => {
                let mut g = contextual_generator::new_contextual_generator(t, &mut self.base.builder, n, n2, subset.expect(npe), map.expect(npe), lookup_subset.expect(npe), num_glyphs)?;
                g.write_subtable()
            }
            6 => {
                let mut g = chaining_generator::new_chaining_instance(t, &mut self.base.builder, n, n2, subset.expect(npe), map.expect(npe), lookup_subset.expect(npe), num_glyphs)?;
                g.write_subtable()
            }
            7 => {
                let b = &mut self.base.builder;
                b.ensure_capacity(n2 + 4);
                b.setuint16(n2, 1);
                b.setuint16(n2 + 2, t.data.getuint16(n + 2)?);
                Ok(8)
            }
            _ => Err(FontEngineException::invalid_font(format!("Invalid GSUB lookup type ({})", n3))),
        }
    }

    fn get_extension_subtable_size(&self) -> i32 {
        8
    }

    fn gather_coverages_for_subtable(&mut self, mut n: i32, mut n2: i32, n3: i32, n4: i32, map: &mut CoverageMap, map2: &mut CoverageMap, subset: &dyn Subset) -> FontResult<()> {
        let t: &LayoutTable = self.base.orig_table;
        let mut map3 = map;
        if n2 == 7 {
            n2 = t.data.getuint16(n + 2)?;
            n += t.data.getuint32asint(n + 4, "Unhandled extension offset")?;
            map3 = map2;
        }
        match n2 {
            1..=4 => {
                let n5 = t.data.get_offset(n, 2)?;
                add_to_coverages_map(t, n5, map3, n3, n4, subset)
            }
            5 => contextual_generator::gather_coverages_for_subtable(t, n, n2, n3, n4, map3, subset),
            6 => chaining_generator::gather_coverages_for_subtable(t, n, n2, n3, n4, map3, subset),
            _ => Err(FontEngineException::invalid_font(format!("Invalid GSUB lookup type ({})", n2))),
        }
    }

    fn get_referenced_lookup_type(&self, n: i32) -> FontResult<i32> {
        self.base.orig_table.data.getuint16(n + 2)
    }

    fn patch_extension_offset(&mut self, n: i32, n2: i32) {
        self.base.builder.setuint32(n + 4, n2 - n);
    }

    fn get_referenced_subtable_offset(&self, n: i32) -> FontResult<i32> {
        Ok(n + self.base.orig_table.data.getuint32asint(n + 4, "Only handle extensions that fit in 31 bits")?)
    }

    fn get_extension_lookup_type(&self) -> i32 {
        7
    }

    fn write_extension_subtable(&mut self, n: i32, n2: i32) -> i32 {
        self.base.builder.setuint16(n, 1);
        self.base.builder.setuint16(n + 2, n2);
        8
    }

    fn must_keep_feature(&self, _n: i32) -> FontResult<bool> {
        Ok(false)
    }
}

struct Type4Members;

impl SetMembers for Type4Members {
    fn which_rule_set_index_applies(&mut self, _g: &SetGenerator, _n: i32, n2: i32) -> FontResult<i32> {
        Ok(n2)
    }

    fn compute_members_to_keep(&mut self, g: &SetGenerator, _n: i32, n2: i32) -> FontResult<Vec<bool>> {
        let d = &g.orig_table.data;
        let n3 = d.getuint16(n2)?;
        let mut bl = vec![true; n3 as usize];
        'block0: for i in 0..n3 {
            let n4 = d.get_offset(n2, 2 + 2 * i)?;
            let n5 = d.getuint16(n4)?;
            if g.subset.get_existing_subset_gid(n5)? == -1 {
                bl[i as usize] = false;
                continue;
            }
            let n6 = d.getuint16(n4 + 2)?;
            for j in 0..n6 - 1 {
                let n5 = d.getuint16(n4 + 4 + 2 * j)?;
                if g.subset.get_existing_subset_gid(n5)? != -1 {
                    continue;
                }
                bl[i as usize] = false;
                continue 'block0;
            }
        }
        Ok(bl)
    }

    fn write_member(&mut self, g: &mut SetGenerator, n: i32, n2: i32) -> FontResult<i32> {
        let d = &g.orig_table.data;
        let n3 = d.getuint16(n2 + 2)?;
        let n4 = 4 + 2 * (n3 - 1);
        g.builder.ensure_capacity(n + n4);
        let v = g.subset.get_existing_subset_gid(d.getuint16(n2)?)?;
        g.builder.setuint16(n, v);
        g.builder.setuint16(n + 2, n3);
        for i in 0..n3 - 1 {
            let v = g.subset.get_existing_subset_gid(d.getuint16(n2 + 4 + 2 * i)?)?;
            g.builder.setuint16(n + 4 + 2 * i, v);
        }
        Ok(n4)
    }

    fn get_orig_record_size(&self) -> i32 {
        0
    }
}

struct Type4Generator<'b> {
    g: SetGenerator<'b>,
    m: Type4Members,
}

impl CoverageConsumer for Type4Generator<'_> {
    fn glyph_info(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        self.g.glyph_info(&mut self.m, n, n2)
    }
}

impl LookupSubtableGenerator for Type4Generator<'_> {
    fn write_subtable(&mut self) -> FontResult<i32> {
        self.g.write_subtable(&mut self.m)
    }
}

struct Type2Or3Generator<'b> {
    t: &'b LayoutTable,
    subset: &'b dyn Subset,
    builder: &'b mut OTByteArrayBuilder,
    new_st_offset: i32,
    orig_st_offset: i32,
    subset_to_coverage_index: Vec<i32>,
    glyph_count: i32,
}

impl CoverageConsumer for Type2Or3Generator<'_> {
    fn glyph_info(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        let g = self.subset.get_existing_subset_gid(n)?;
        self.subset_to_coverage_index[g as usize] = n2;
        Ok(true)
    }
}

impl LookupSubtableGenerator for Type2Or3Generator<'_> {
    fn write_subtable(&mut self) -> FontResult<i32> {
        let t = self.t;
        let s = self.new_st_offset;
        let mut n = 6 + 2 * self.glyph_count;
        self.builder.ensure_capacity(s + 6 + 2 * self.glyph_count);
        self.builder.setuint16(s, 1);
        self.builder.setuint16(s + 4, self.glyph_count);
        let subset = self.subset;
        t.iterate_coverage(t.data.get_offset(self.orig_st_offset, 2)?, Some(subset), self)?;
        let mut n2 = 0;
        for i in 0..self.subset_to_coverage_index.len() {
            if self.subset_to_coverage_index[i] == -1 {
                continue;
            }
            self.builder.setuint16(s + 6 + 2 * n2, n);
            let n3 = t.data.get_offset(self.orig_st_offset, 6 + 2 * self.subset_to_coverage_index[i])?;
            let n4 = t.data.getuint16(n3)?;
            self.builder.ensure_capacity(s + n + 2 + 2 * n4);
            self.builder.setuint16(s + n, n4);
            for j in 0..n4 {
                let n5 = subset.get_existing_subset_gid(t.data.getuint16(n3 + 2 + 2 * j)?)?;
                if n5 == -1 {
                    panic!("Matched a gid not in our subset?? Can't happen..");
                }
                self.builder.setuint16(s + n + 2 + 2 * j, n5);
            }
            n += 2 + 2 * n4;
            n2 += 1;
        }
        Ok(n)
    }
}

struct Type1Format2Generator<'b> {
    t: &'b LayoutTable,
    subset: &'b dyn Subset,
    builder: &'b mut OTByteArrayBuilder,
    new_st_offset: i32,
    glyph_count: i32,
    fetcher: Type1TargetGIDFetcher,
    subset_to_target: Vec<i32>,
    orig_st_offset: i32,
    new_coverages: &'b CoverageMap,
}

impl CoverageConsumer for Type1Format2Generator<'_> {
    fn glyph_info(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        let n4 = self.fetcher.get_target(self.t, n, n2)?;
        let n3 = self.subset.get_existing_subset_gid(n4)?;
        let g = self.subset.get_existing_subset_gid(n)?;
        self.subset_to_target[g as usize] = n3;
        if n3 == -1 {
            panic!("Matched a gid not in our subset?? Can't happen..");
        }
        self.glyph_count += 1;
        Ok(true)
    }
}

impl LookupSubtableGenerator for Type1Format2Generator<'_> {
    fn write_subtable(&mut self) -> FontResult<i32> {
        let t = self.t;
        let n = t.data.get_offset(self.orig_st_offset, 2)?;
        let gc = self.new_coverages.get(&n).expect("java.lang.NullPointerException").glyph_count;
        let s = self.new_st_offset;
        self.builder.ensure_capacity(s + 6 + 2 * gc);
        self.builder.setuint16(s, 2);
        self.builder.setuint16(s + 4, gc);
        let subset = self.subset;
        t.iterate_coverage(n, Some(subset), self)?;
        let mut n2 = 0;
        for i in 0..self.subset_to_target.len() {
            if self.subset_to_target[i] == -1 {
                continue;
            }
            self.builder.setuint16(s + 6 + 2 * n2, self.subset_to_target[i]);
            n2 += 1;
        }
        Ok(6 + 2 * gc)
    }
}

struct DeltaDeterminer<'b> {
    subset: &'b dyn Subset,
    format1_ok: bool,
    delta: i32,
    delta_initialized: bool,
    fetcher: Type1TargetGIDFetcher,
    t: &'b LayoutTable,
}

impl CoverageConsumer for DeltaDeterminer<'_> {
    fn glyph_info(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        let n3 = self.subset.get_existing_subset_gid(n)?;
        let n4 = self.fetcher.get_target(self.t, n, n2)?;
        let n5 = self.subset.get_existing_subset_gid(n4)?;
        if self.delta_initialized {
            if n5 - n3 == self.delta {
                return Ok(true);
            }
            self.format1_ok = false;
            return Ok(false);
        }
        self.delta = n5 - n3;
        self.delta_initialized = true;
        if self.delta > i16::MAX as i32 || self.delta < i16::MIN as i32 {
            self.format1_ok = false;
            return Ok(false);
        }
        Ok(true)
    }
}

/// `Type1TargetGIDFetcher` and its two implementations.
#[derive(Clone, Copy)]
enum Type1TargetGIDFetcher {
    Format1 { delta: i32 },
    Format2 { orig_st_offset: i32 },
}

impl Type1TargetGIDFetcher {
    fn get_target(&self, t: &LayoutTable, n: i32, n2: i32) -> FontResult<i32> {
        match *self {
            Type1TargetGIDFetcher::Format1 { delta } => Ok(n + delta),
            Type1TargetGIDFetcher::Format2 { orig_st_offset } => t.data.getuint16(orig_st_offset + 6 + 2 * n2),
        }
    }
}
