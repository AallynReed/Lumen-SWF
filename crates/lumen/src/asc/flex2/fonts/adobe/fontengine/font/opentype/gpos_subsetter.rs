//! Port of afe.jar `com.adobe.fontengine.font.opentype.GposSubsetter`.

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::chaining_generator;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::contextual_generator;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gpos::Gpos;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table::{ClassConsumer, CoverageConsumer, LayoutTable};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table_subsetter::write_class_def;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table::LookupTable;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table_subsetter::{add_to_coverages_map, patch_coverage_at_offset, CoverageMap, LookupSubset, LookupSubtableGenerator, LookupTableSubsetter, LookupTableSubsetterBase};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::set_generator::{SetGenerator, SetMembers};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset_default_impl::SubsetDefaultImpl;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

const NPE: &str = "java.lang.NullPointerException";

pub struct GposSubsetter<'a> {
    pub base: LookupTableSubsetterBase<'a>,
    preserve_non_empty_kern_feature: bool,
}

impl<'a> GposSubsetter<'a> {
    pub fn new(gpos: &'a LookupTable, n: i32, bl: bool) -> Self {
        GposSubsetter { base: LookupTableSubsetterBase::new(gpos, OTByteArray::get_ot_byte_array_builder_instance(), n), preserve_non_empty_kern_feature: bl }
    }

    fn write_lookup1_format1(&mut self, n: i32, n2: i32, n_array: Option<&[i32]>) -> FontResult<i32> {
        let t = self.base.orig_table;
        let n3 = t.data.getuint16(n + 4)?;
        let n4 = target_value_format(n3);
        let n5 = Gpos::get_value_record_size(n4);
        let n6 = 6 + n5;
        let b = &mut self.base.builder;
        b.ensure_capacity(n2 + n6);
        b.setuint16(n2, 1);
        b.setuint16(n2 + 4, n4);
        write_value_format(b, n2 + 6, n3, n4, n_array.expect(NPE));
        Ok(n6)
    }
}

fn target_value_format(n: i32) -> i32 {
    n & 0xF
}

fn write_value_format(b: &mut OTByteArrayBuilder, n: i32, n2: i32, n3: i32, n_array: &[i32]) {
    let mut n4 = 0;
    let mut n5 = 0;
    let mut i = 1;
    while i <= 8 && (n4 as usize) < n_array.len() {
        if (n2 & i) != 0 {
            if (n3 & i) != 0 {
                b.setint16(n + 2 * n5, n_array[n4 as usize]);
                n5 += 1;
            }
            n4 += 1;
        }
        i <<= 1;
    }
}

fn write_value_format_from_original(t: &LayoutTable, b: &mut OTByteArrayBuilder, n: i32, n2: i32, n3: i32, n4: i32) -> FontResult<()> {
    let mut n5 = 0;
    let mut n6 = 0;
    let mut i = 1;
    while i <= 8 {
        if (n3 & i) != 0 {
            if (n4 & i) != 0 {
                let n7 = t.data.getint16(n + 2 * n5)?;
                b.setint16(n2 + 2 * n6, n7);
                n6 += 1;
            }
            n5 += 1;
        }
        i <<= 1;
    }
    Ok(())
}

fn write_anchor_from_original(t: &LayoutTable, b: &mut OTByteArrayBuilder, n: i32, n2: i32) -> FontResult<i32> {
    b.ensure_capacity(n2 + 6);
    b.setuint16(n2, 1);
    b.setint16(n2 + 2, t.data.getint16(n + 2)?);
    b.setint16(n2 + 4, t.data.getint16(n + 4)?);
    Ok(6)
}

impl<'a> LookupTableSubsetter<'a> for GposSubsetter<'a> {
    fn base(&self) -> &LookupTableSubsetterBase<'a> {
        &self.base
    }

    fn base_mut(&mut self) -> &mut LookupTableSubsetterBase<'a> {
        &mut self.base
    }

    fn gather_coverages_for_subtable(&mut self, mut n: i32, mut n2: i32, n3: i32, n4: i32, map: &mut CoverageMap, map2: &mut CoverageMap, subset: &dyn Subset) -> FontResult<()> {
        let t: &LayoutTable = self.base.orig_table;
        let mut map3 = map;
        if n2 == 9 {
            n2 = t.data.getuint16(n + 2)?;
            n += t.data.getuint32asint(n + 4, "Unhandled extension offset")?;
            map3 = map2;
        }
        match n2 {
            1..=3 => {
                let n5 = t.data.get_offset(n, 2)?;
                add_to_coverages_map(t, n5, map3, n3, n4, subset)
            }
            4..=6 => {
                let n6 = t.data.get_offset(n, 2)?;
                add_to_coverages_map(t, n6, map3, n3, n4, subset)?;
                let n6 = t.data.get_offset(n, 4)?;
                add_to_coverages_map(t, n6, map3, n3, n4, subset)
            }
            7 => contextual_generator::gather_coverages_for_subtable(t, n, n2, n3, n4, map3, subset),
            8 => chaining_generator::gather_coverages_for_subtable(t, n, n2, n3, n4, map3, subset),
            _ => Err(FontEngineException::invalid_font(format!("Invalid GSUB lookup type ({})", n2))),
        }
    }

    fn get_extension_subtable_size(&self) -> i32 {
        8
    }

    fn patch_subtable_coverage(&mut self, n: i32, n2: i32, map: &CoverageMap, n3: i32) -> FontResult<()> {
        let t: &LayoutTable = self.base.orig_table;
        let b = &mut self.base.builder;
        match n3 {
            1..=3 => patch_coverage_at_offset(t, b, n, n2, 2, map),
            4..=6 => {
                patch_coverage_at_offset(t, b, n, n2, 2, map)?;
                patch_coverage_at_offset(t, b, n, n2, 4, map)
            }
            7 => contextual_generator::patch_subtable_coverage(t, b, n, n2, map, n3),
            8 => chaining_generator::patch_subtable_coverage(t, b, n, n2, map, n3),
            9 => Ok(()),
            _ => Err(FontEngineException::invalid_font(format!("Unrecognized lookup type: {}", n3))),
        }
    }

    fn write_subtable(&mut self, n: i32, n2: i32, map: Option<&CoverageMap>, n3: i32, subset: Option<&dyn Subset>, lookup_subset: Option<&LookupSubset>) -> FontResult<i32> {
        let t: &LayoutTable = self.base.orig_table;
        let num_glyphs = self.base.orig_num_glyphs;
        match n3 {
            1 => {
                let n5 = t.data.getuint16(n)?;
                let (n4, n_array) = if n5 == 2 {
                    let mut d = L1F2TargetFormatDeterminer { t, can_use_format1: true, orig_st_offset: n, value_format_size: Gpos::get_value_record_size(t.data.getuint16(n + 4)?) / 2, bytes: None };
                    let f = if d.can_use_format1(subset.expect(NPE))? { 1 } else { 2 };
                    (f, d.bytes)
                } else {
                    let len = Gpos::get_value_record_size(t.data.getuint16(n + 4)?) / 2;
                    let mut v = Vec::with_capacity(len as usize);
                    for i in 0..len {
                        v.push(t.data.getint16(n + 6 + 2 * i)?);
                    }
                    (1, Some(v))
                };
                if n4 == 1 {
                    return self.write_lookup1_format1(n, n2, n_array.as_deref());
                }
                let subset = subset.expect(NPE);
                let mut g = Type1Format2Generator { t, builder: &mut self.base.builder, subset, new_st_offset: n2, orig_st_offset: n, glyph_count: 0, subset_to_orig_coverage_index: vec![-1; subset.get_num_glyphs().max(0) as usize] };
                t.iterate_coverage(t.data.get_offset(n, 2)?, Some(subset), &mut g)?;
                g.write_subtable()
            }
            2 => {
                let n6 = t.data.getuint16(n)?;
                let subset = subset.expect(NPE);
                match n6 {
                    1 => {
                        let map = map.expect(NPE);
                        let count = map.get(&t.data.get_offset(n, 2)?).expect(NPE).glyph_count;
                        let value1_format = t.data.getuint16(n + 4)?;
                        let value2_format = t.data.getuint16(n + 6)?;
                        let m = Type2Format1Members {
                            value1_size: Gpos::get_value_record_size(value1_format),
                            value2_size: Gpos::get_value_record_size(value2_format),
                            value1_format,
                            value2_format,
                            target_value1_format: target_value_format(value1_format),
                            target_value2_format: target_value_format(value2_format),
                            target_value1_size: Gpos::get_value_record_size(target_value_format(value1_format)),
                            target_value2_size: Gpos::get_value_record_size(target_value_format(value2_format)),
                        };
                        let g = SetGenerator::new(t, subset, &mut self.base.builder, n2, n, count, 8, false, false)?;
                        let mut gen = Type2Format1Generator { g, m };
                        gen.g.builder.ensure_capacity(n2 + 8);
                        gen.g.builder.setuint16(n2, 1);
                        gen.g.builder.setuint16(n2 + 4, gen.m.target_value1_format);
                        gen.g.builder.setuint16(n2 + 6, gen.m.target_value2_format);
                        t.iterate_coverage(t.data.get_offset(n, 2)?, Some(subset), &mut gen)?;
                        gen.write_subtable()
                    }
                    2 => {
                        let mut g = Type2Format2Generator::new(t, &mut self.base.builder, num_glyphs, n, n2, subset)?;
                        g.write_subtable()
                    }
                    _ => Err(FontEngineException::invalid_font(format!("Invalid type 2 gpos lookup: {}", n6))),
                }
            }
            3 => {
                let subset = subset.expect(NPE);
                let mut g = Type3Generator { t, builder: &mut self.base.builder, subtable_size: 0, orig_st_offset: n, new_st_offset: n2, subset, entry_exit_count: 0, subset_gid_to_coverage_index: vec![-1; subset.get_num_glyphs().max(0) as usize] };
                let n3 = t.data.get_offset(n, 2)?;
                t.iterate_coverage(n3, Some(subset), &mut g)?;
                g.write_subtable()
            }
            4 | 6 => {
                let mut g = MarkGenerator::new(t, &mut self.base.builder, n, n2, subset.expect(NPE), false);
                g.write_subtable()
            }
            5 => {
                let mut g = MarkGenerator::new(t, &mut self.base.builder, n, n2, subset.expect(NPE), true);
                g.write_subtable()
            }
            7 => {
                let mut g = contextual_generator::new_contextual_generator(t, &mut self.base.builder, n, n2, subset.expect(NPE), map.expect(NPE), lookup_subset.expect(NPE), num_glyphs)?;
                g.write_subtable()
            }
            8 => {
                let mut g = chaining_generator::new_chaining_instance(t, &mut self.base.builder, n, n2, subset.expect(NPE), map.expect(NPE), lookup_subset.expect(NPE), num_glyphs)?;
                g.write_subtable()
            }
            9 => {
                let b = &mut self.base.builder;
                b.setuint16(n2, 1);
                b.setuint16(n2 + 2, t.data.getuint16(n + 2)?);
                Ok(8)
            }
            _ => Err(FontEngineException::invalid_font(format!("Invalid GPOS lookup type ({})", n3))),
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
        9
    }

    fn write_extension_subtable(&mut self, n: i32, n2: i32) -> i32 {
        self.base.builder.setuint16(n, 1);
        self.base.builder.setuint16(n + 2, n2);
        8
    }

    fn must_keep_feature(&self, n: i32) -> FontResult<bool> {
        if self.preserve_non_empty_kern_feature {
            let t = self.base.orig_table;
            let kern = [107, 101, 114, 110];
            for n2 in 0..4 {
                if t.data.getuint8(n + n2)? == kern[n2 as usize] {
                    continue;
                }
                return Ok(false);
            }
            let n2 = t.data.get_offset(n, 4)?;
            let n3 = t.data.getuint16(n2 + 2)?;
            return Ok(n3 > 0);
        }
        Ok(false)
    }
}

/// `MarkGenerator` with its `Type4Or6Generator` and `Type5Generator` subclasses.
struct MarkGenerator<'b> {
    t: &'b LayoutTable,
    builder: &'b mut OTByteArrayBuilder,
    subset: &'b dyn Subset,
    orig_st_offset: i32,
    new_st_offset: i32,
    entry_count: i32,
    subset_to_coverage: Vec<i32>,
    type5: bool,
}

impl CoverageConsumer for MarkGenerator<'_> {
    fn glyph_info(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        let g = self.subset.get_existing_subset_gid(n)?;
        self.subset_to_coverage[g as usize] = n2;
        self.entry_count += 1;
        Ok(true)
    }
}

impl<'b> MarkGenerator<'b> {
    fn new(t: &'b LayoutTable, builder: &'b mut OTByteArrayBuilder, n: i32, n2: i32, subset: &'b dyn Subset, type5: bool) -> Self {
        MarkGenerator { t, builder, subset, orig_st_offset: n, new_st_offset: n2, entry_count: 0, subset_to_coverage: vec![-1; subset.get_num_glyphs().max(0) as usize], type5 }
    }

    fn write_target_array(&mut self, n: i32, n2: i32, cs: &SubsetDefaultImpl, n3: i32) -> FontResult<i32> {
        if self.type5 {
            return self.write_target_array_type5(n, n2, cs, n3);
        }
        let t = self.t;
        let mut n4 = 2 + 2 * cs.get_num_glyphs() * self.entry_count;
        self.builder.ensure_capacity(n2 + n4);
        self.builder.setuint16(n2, self.entry_count);
        let mut n5 = 0;
        let mut i = 0;
        while n5 < self.entry_count && i < self.subset.get_num_glyphs() {
            if self.subset_to_coverage[i as usize] == -1 {
                i += 1;
                continue;
            }
            for j in 0..cs.get_num_glyphs() {
                self.builder.setuint16(n2 + 2 + 2 * (n5 * cs.get_num_glyphs() + j), n4);
                let o = t.data.get_offset(n, 2 + 2 * (self.subset_to_coverage[i as usize] * n3 + cs.get_full_gid(j)))?;
                n4 += write_anchor_from_original(t, self.builder, o, n2 + n4)?;
            }
            n5 += 1;
            i += 1;
        }
        Ok(n4)
    }

    fn write_target_array_type5(&mut self, n: i32, n2: i32, cs: &SubsetDefaultImpl, n3: i32) -> FontResult<i32> {
        let mut n4 = 2 + 2 * self.entry_count;
        self.builder.ensure_capacity(n2 + n4);
        self.builder.setuint16(n2, self.entry_count);
        let mut n5 = 0;
        let mut i = 0;
        while n5 < self.entry_count && i < self.subset.get_num_glyphs() {
            if self.subset_to_coverage[i as usize] == -1 {
                i += 1;
                continue;
            }
            self.builder.setuint16(n2 + 2 + 2 * n5, n4);
            let c = self.subset_to_coverage[i as usize];
            n4 += self.write_array_entry(n, 2 + 2 * c, n2 + n4, cs, n3, c)?;
            n5 += 1;
            i += 1;
        }
        Ok(n4)
    }

    fn write_array_entry(&mut self, n: i32, n2: i32, n3: i32, cs: &SubsetDefaultImpl, n4: i32, _n5: i32) -> FontResult<i32> {
        let t = self.t;
        let n6 = t.data.get_offset(n, n2)?;
        let n7 = t.data.getuint16(n6)?;
        let k = cs.get_num_glyphs();
        let mut n8 = 2 + 2 * n7 * k;
        self.builder.ensure_capacity(n3 + n8);
        self.builder.setuint16(n3, n7);
        for i in 0..n7 {
            for j in 0..k {
                let n9 = 2 + 2 * (n4 * i + cs.get_full_gid(j));
                let n10 = t.data.get_offset(n6, n9)?;
                if n10 == 0 {
                    self.builder.setuint16(n3 + 2 + 2 * (i * k + j), 0);
                    continue;
                }
                self.builder.setuint16(n3 + 2 + 2 * (i * k + j), n8);
                n8 += write_anchor_from_original(t, self.builder, n10, n3 + n8)?;
            }
        }
        Ok(n8)
    }
}

impl LookupSubtableGenerator for MarkGenerator<'_> {
    fn write_subtable(&mut self) -> FontResult<i32> {
        let t = self.t;
        let mut n = 12;
        let mut mag = MarkArrayGenerator { subset: self.subset, subset_to_coverage_index: vec![-1; self.subset.get_num_glyphs().max(0) as usize], entry_count: 0, subtable_size: 0, class_subset: None };
        let n2 = t.data.get_offset(self.orig_st_offset, 2)?;
        let n3 = t.data.get_offset(self.orig_st_offset, 8)?;
        let n4 = t.data.getuint16(self.orig_st_offset + 6)?;
        self.builder.ensure_capacity(self.new_st_offset + n);
        self.builder.setuint16(self.new_st_offset, 1);
        self.builder.setuint16(self.new_st_offset + 8, n);
        n += mag.write_mark_array(t, self.builder, n2, n3, self.new_st_offset + n, n4)?;
        let cs = mag.class_subset.take().unwrap();
        self.builder.setuint16(self.new_st_offset + 6, cs.get_num_glyphs());
        let subset = self.subset;
        t.iterate_coverage(t.data.get_offset(self.orig_st_offset, 4)?, Some(subset), self)?;
        self.builder.setuint16(self.new_st_offset + 10, n);
        let at = self.new_st_offset + n;
        n += self.write_target_array(t.data.get_offset(self.orig_st_offset, 10)?, at, &cs, n4)?;
        Ok(n)
    }
}

struct MarkArrayGenerator<'b> {
    subset: &'b dyn Subset,
    subset_to_coverage_index: Vec<i32>,
    entry_count: i32,
    subtable_size: i32,
    class_subset: Option<SubsetDefaultImpl>,
}

impl CoverageConsumer for MarkArrayGenerator<'_> {
    fn glyph_info(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        let g = self.subset.get_existing_subset_gid(n)?;
        self.subset_to_coverage_index[g as usize] = n2;
        self.entry_count += 1;
        Ok(true)
    }
}

impl MarkArrayGenerator<'_> {
    fn write_mark_array(&mut self, t: &LayoutTable, b: &mut OTByteArrayBuilder, n: i32, n2: i32, n3: i32, n4: i32) -> FontResult<i32> {
        let subset = self.subset;
        t.iterate_coverage(n, Some(subset), self)?;
        let mut cs = SubsetDefaultImpl::new(n4, true);
        self.subtable_size = 2 + 4 * self.entry_count;
        b.ensure_capacity(n3 + self.subtable_size);
        b.setuint16(n3, self.entry_count);
        let mut n5 = 0;
        let mut i = 0;
        while n5 < self.entry_count && i < subset.get_num_glyphs() {
            let ci = self.subset_to_coverage_index[i as usize];
            if ci == -1 {
                i += 1;
                continue;
            }
            let n6 = t.data.getuint16(n2 + 2 + 4 * ci)?;
            b.setuint16(n3 + 2 + 4 * n5, cs.get_subset_gid(n6)?);
            b.setuint16(n3 + 4 + 4 * n5, self.subtable_size);
            let o = t.data.get_offset(n2, 4 + 4 * ci)?;
            self.subtable_size += write_anchor_from_original(t, b, o, n3 + self.subtable_size)?;
            n5 += 1;
            i += 1;
        }
        self.class_subset = Some(cs);
        Ok(self.subtable_size)
    }
}

struct Type3Generator<'b> {
    t: &'b LayoutTable,
    builder: &'b mut OTByteArrayBuilder,
    subtable_size: i32,
    orig_st_offset: i32,
    new_st_offset: i32,
    subset: &'b dyn Subset,
    entry_exit_count: i32,
    subset_gid_to_coverage_index: Vec<i32>,
}

impl CoverageConsumer for Type3Generator<'_> {
    fn glyph_info(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        let g = self.subset.get_existing_subset_gid(n)?;
        self.subset_gid_to_coverage_index[g as usize] = n2;
        self.entry_exit_count += 1;
        Ok(true)
    }
}

impl LookupSubtableGenerator for Type3Generator<'_> {
    fn write_subtable(&mut self) -> FontResult<i32> {
        let t = self.t;
        self.subtable_size = 6 + 4 * self.entry_exit_count;
        self.builder.ensure_capacity(self.new_st_offset + self.subtable_size);
        self.builder.setuint16(self.new_st_offset, 1);
        self.builder.setuint16(self.new_st_offset + 4, self.entry_exit_count);
        let mut n = 0;
        let mut i = 0;
        while n < self.entry_exit_count * 2 && i < self.subset.get_num_glyphs() {
            let ci = self.subset_gid_to_coverage_index[i as usize];
            if ci == -1 {
                i += 1;
                continue;
            }
            for j in 0..2 {
                let n2 = t.data.get_offset(self.orig_st_offset, 6 + 4 * ci + 2 * j)?;
                if n2 == 0 {
                    self.builder.setuint16(self.new_st_offset + 6 + 2 * n, 0);
                } else {
                    self.builder.setuint16(self.new_st_offset + 6 + 2 * n, self.subtable_size);
                    let at = self.new_st_offset + self.subtable_size;
                    self.subtable_size += write_anchor_from_original(t, self.builder, n2, at)?;
                }
                n += 1;
            }
            i += 1;
        }
        Ok(self.subtable_size)
    }
}

struct Type2Format2Generator<'b> {
    t: &'b LayoutTable,
    builder: &'b mut OTByteArrayBuilder,
    orig_num_glyphs: i32,
    subtable_size: i32,
    class1_count_in_subset: i32,
    class2_count_in_subset: i32,
    orig_class2_count: i32,
    subset: &'b dyn Subset,
    value1_format: i32,
    value2_format: i32,
    value1_size: i32,
    value2_size: i32,
    target1_format: i32,
    target2_format: i32,
    target1_size: i32,
    target2_size: i32,
    new_st_offset: i32,
    orig_st_offset: i32,
}

struct MaxClassInSubset<'s> {
    subset: &'s dyn Subset,
    current_max: i32,
}

impl ClassConsumer for MaxClassInSubset<'_> {
    fn glyph(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        if self.subset.get_existing_subset_gid(n)? != -1 && n2 > self.current_max {
            self.current_max = n2;
        }
        Ok(true)
    }
}

impl<'b> Type2Format2Generator<'b> {
    fn new(t: &'b LayoutTable, builder: &'b mut OTByteArrayBuilder, orig_num_glyphs: i32, n: i32, n2: i32, subset: &'b dyn Subset) -> FontResult<Self> {
        let n3 = t.data.get_offset(n, 8)?;
        let n4 = t.data.get_offset(n, 10)?;
        let orig_class2_count = t.data.getuint16(n + 14)?;
        let mut c = MaxClassInSubset { subset, current_max: 0 };
        t.iterate_class(n3, orig_num_glyphs, &mut c, -1)?;
        let class1_count_in_subset = c.current_max + 1;
        let class2_count_in_subset = if n3 == n4 {
            c.current_max
        } else {
            c.current_max = 0;
            t.iterate_class(n4, orig_num_glyphs, &mut c, -1)?;
            c.current_max + 1
        };
        let value1_format = t.data.getuint16(n + 4)?;
        let value2_format = t.data.getuint16(n + 6)?;
        Ok(Type2Format2Generator {
            t,
            builder,
            orig_num_glyphs,
            subtable_size: 0,
            class1_count_in_subset,
            class2_count_in_subset,
            orig_class2_count,
            subset,
            value1_format,
            value2_format,
            value1_size: Gpos::get_value_record_size(value1_format),
            value2_size: Gpos::get_value_record_size(value2_format),
            target1_format: target_value_format(value1_format),
            target2_format: target_value_format(value2_format),
            target1_size: Gpos::get_value_record_size(target_value_format(value1_format)),
            target2_size: Gpos::get_value_record_size(target_value_format(value2_format)),
            new_st_offset: n2,
            orig_st_offset: n,
        })
    }

    fn write_header(&mut self) -> FontResult<()> {
        let t = self.t;
        self.subtable_size = 16 + (self.target1_size + self.target2_size) * self.class2_count_in_subset * self.class1_count_in_subset;
        self.builder.ensure_capacity(self.new_st_offset + self.subtable_size);
        self.builder.setuint16(self.new_st_offset, 2);
        self.builder.setuint16(self.new_st_offset + 4, self.target1_format);
        self.builder.setuint16(self.new_st_offset + 6, self.target2_format);
        self.subtable_size += write_class_def(t, self.builder, self.orig_st_offset, 8, self.new_st_offset, self.subtable_size, self.subset, self.orig_num_glyphs)?;
        let n = t.data.get_offset(self.orig_st_offset, 8)?;
        let n2 = t.data.get_offset(self.orig_st_offset, 10)?;
        if n == n2 {
            let v = self.builder.getuint16(self.new_st_offset + 8);
            self.builder.setuint16(self.new_st_offset + 10, v);
        } else {
            self.subtable_size += write_class_def(t, self.builder, self.orig_st_offset, 10, self.new_st_offset, self.subtable_size, self.subset, self.orig_num_glyphs)?;
        }
        self.builder.setuint16(self.new_st_offset + 12, self.class1_count_in_subset);
        self.builder.setuint16(self.new_st_offset + 14, self.class2_count_in_subset);
        Ok(())
    }
}

impl LookupSubtableGenerator for Type2Format2Generator<'_> {
    fn write_subtable(&mut self) -> FontResult<i32> {
        self.write_header()?;
        let t = self.t;
        for i in 0..self.class1_count_in_subset {
            for j in 0..self.class2_count_in_subset {
                let o = self.orig_st_offset + 16 + (self.value1_size + self.value2_size) * (self.orig_class2_count * i + j);
                let s = self.new_st_offset + 16 + (self.target1_size + self.target2_size) * (self.class2_count_in_subset * i + j);
                write_value_format_from_original(t, self.builder, o, s, self.value1_format, self.target1_format)?;
                write_value_format_from_original(t, self.builder, o + self.value1_size, s + self.target1_size, self.value2_format, self.target2_format)?;
            }
        }
        Ok(self.subtable_size)
    }
}

struct Type2Format1Members {
    value1_size: i32,
    value2_size: i32,
    value1_format: i32,
    value2_format: i32,
    target_value1_format: i32,
    target_value2_format: i32,
    target_value1_size: i32,
    target_value2_size: i32,
}

impl SetMembers for Type2Format1Members {
    fn which_rule_set_index_applies(&mut self, _g: &SetGenerator, _n: i32, n2: i32) -> FontResult<i32> {
        Ok(n2)
    }

    fn compute_members_to_keep(&mut self, g: &SetGenerator, _n: i32, n2: i32) -> FontResult<Vec<bool>> {
        let d = &g.orig_table.data;
        let n3 = d.getuint16(n2)?;
        let mut bl = vec![true; n3 as usize];
        for i in 0..n3 {
            let n4 = d.getuint16(n2 + 2 + (2 + self.value1_size + self.value2_size) * i)?;
            if g.subset.get_existing_subset_gid(n4)? != -1 {
                continue;
            }
            bl[i as usize] = false;
        }
        Ok(bl)
    }

    fn write_member(&mut self, g: &mut SetGenerator, n: i32, n2: i32) -> FontResult<i32> {
        let t = g.orig_table;
        g.builder.ensure_capacity(n + 2 + self.target_value1_size + self.target_value2_size);
        let n3 = t.data.getuint16(n2)?;
        let v = g.subset.get_existing_subset_gid(n3)?;
        g.builder.setuint16(n, v);
        write_value_format_from_original(t, g.builder, n2 + 2, n + 2, self.value1_format, self.target_value1_format)?;
        write_value_format_from_original(t, g.builder, n2 + 2 + self.value1_size, n + 2 + self.target_value1_size, self.value2_format, self.target_value2_format)?;
        Ok(2 + self.target_value1_size + self.target_value2_size)
    }

    fn get_orig_record_size(&self) -> i32 {
        2 + self.value1_size + self.value2_size
    }
}

struct Type2Format1Generator<'b> {
    g: SetGenerator<'b>,
    m: Type2Format1Members,
}

impl CoverageConsumer for Type2Format1Generator<'_> {
    fn glyph_info(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        self.g.glyph_info(&mut self.m, n, n2)
    }
}

impl LookupSubtableGenerator for Type2Format1Generator<'_> {
    fn write_subtable(&mut self) -> FontResult<i32> {
        self.g.write_subtable(&mut self.m)
    }
}

struct Type1Format2Generator<'b> {
    t: &'b LayoutTable,
    builder: &'b mut OTByteArrayBuilder,
    subset: &'b dyn Subset,
    new_st_offset: i32,
    orig_st_offset: i32,
    glyph_count: i32,
    subset_to_orig_coverage_index: Vec<i32>,
}

impl CoverageConsumer for Type1Format2Generator<'_> {
    fn glyph_info(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        let n3 = self.subset.get_existing_subset_gid(n)?;
        self.subset_to_orig_coverage_index[n3 as usize] = n2;
        self.glyph_count += 1;
        Ok(true)
    }
}

impl LookupSubtableGenerator for Type1Format2Generator<'_> {
    fn write_subtable(&mut self) -> FontResult<i32> {
        let t = self.t;
        let n = t.data.getuint16(self.orig_st_offset + 4)?;
        let n2 = target_value_format(n);
        let n3 = Gpos::get_value_record_size(n2);
        let n4 = Gpos::get_value_record_size(n);
        self.builder.ensure_capacity(self.new_st_offset + 8 + n3 * self.glyph_count);
        self.builder.setuint16(self.new_st_offset, 2);
        self.builder.setuint16(self.new_st_offset + 4, n2);
        self.builder.setuint16(self.new_st_offset + 6, self.glyph_count);
        let mut n5 = 0;
        let mut i = 0;
        while i < self.subset.get_num_glyphs() && n5 < self.glyph_count {
            let ci = self.subset_to_orig_coverage_index[i as usize];
            if ci != -1 {
                write_value_format_from_original(t, self.builder, self.orig_st_offset + 8 + n4 * ci, self.new_st_offset + 8 + n3 * n5, n, n2)?;
                n5 += 1;
            }
            i += 1;
        }
        Ok(8 + n3 * self.glyph_count)
    }
}

struct L1F2TargetFormatDeterminer<'b> {
    t: &'b LayoutTable,
    can_use_format1: bool,
    orig_st_offset: i32,
    value_format_size: i32,
    bytes: Option<Vec<i32>>,
}

impl CoverageConsumer for L1F2TargetFormatDeterminer<'_> {
    fn glyph_info(&mut self, _n: i32, n2: i32) -> FontResult<bool> {
        let d = &self.t.data;
        // Java steps the value record by one byte per field (`+ i`), not two.
        match &self.bytes {
            None => {
                let mut v = Vec::with_capacity(self.value_format_size as usize);
                for i in 0..self.value_format_size {
                    v.push(d.getint16(self.orig_st_offset + 8 + n2 * self.value_format_size * 2 + i)?);
                }
                self.bytes = Some(v);
            }
            Some(bytes) => {
                for i in 0..self.value_format_size {
                    let n3 = d.getint16(self.orig_st_offset + 8 + n2 * self.value_format_size * 2 + i)?;
                    if n3 == bytes[i as usize] {
                        continue;
                    }
                    self.can_use_format1 = false;
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

impl L1F2TargetFormatDeterminer<'_> {
    fn can_use_format1(&mut self, subset: &dyn Subset) -> FontResult<bool> {
        let t = self.t;
        t.iterate_coverage(t.data.get_offset(self.orig_st_offset, 2)?, Some(subset), self)?;
        Ok(self.can_use_format1)
    }
}
