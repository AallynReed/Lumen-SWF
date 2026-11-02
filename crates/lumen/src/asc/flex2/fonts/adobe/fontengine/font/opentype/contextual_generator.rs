//! Port of afe.jar `com.adobe.fontengine.font.opentype.ContextualGenerator`.

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table::{CoverageConsumer, LayoutTable};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table_subsetter::{ClassCoveredBySubset, ClassDefGenerator};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table_subsetter::{add_to_coverages_map, patch_coverage_at_offset, CoverageMap, LookupSubset, LookupSubtableGenerator};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::OTByteArrayBuilder;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::set_generator::{SetGenerator, SetMembers};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

pub fn gather_coverages_for_subtable(t: &LayoutTable, n: i32, _n2: i32, n3: i32, n4: i32, map: &mut CoverageMap, subset: &dyn Subset) -> FontResult<()> {
    let n5 = t.data.getuint16(n)?;
    match n5 {
        1 | 2 => {
            let n6 = t.data.get_offset(n, 2)?;
            add_to_coverages_map(t, n6, map, n3, n4, subset)
        }
        3 => {
            let n7 = t.data.getuint16(n + 2)?;
            for i in 0..n7 {
                let n8 = t.data.get_offset(n, 6 + 2 * i)?;
                add_to_coverages_map(t, n8, map, n3, n4, subset)?;
            }
            Ok(())
        }
        _ => Err(FontEngineException::invalid_font(format!("Invalid contextual subtable format ({})", n5))),
    }
}

pub fn patch_subtable_coverage(t: &LayoutTable, builder: &mut OTByteArrayBuilder, n: i32, n2: i32, map: &CoverageMap, _n3: i32) -> FontResult<()> {
    let n4 = t.data.getuint16(n)?;
    match n4 {
        1 | 2 => patch_coverage_at_offset(t, builder, n, n2, 2, map),
        3 => {
            let n5 = t.data.getuint16(n + 2)?;
            for i in 0..n5 {
                patch_coverage_at_offset(t, builder, n, n2, 6 + 2 * i, map)?;
            }
            Ok(())
        }
        _ => Err(FontEngineException::invalid_font(format!("Unrecognized lookup type 5 format: {}", n4))),
    }
}

/// `newContextualGenerator(table, builder, origOffset, newOffset, subset, coverages, lookupSubset, numGlyphs)`.
#[allow(clippy::too_many_arguments)]
pub fn new_contextual_generator<'b>(t: &'b LayoutTable, builder: &'b mut OTByteArrayBuilder, n: i32, n2: i32, subset: &'b dyn Subset, map: &CoverageMap, ls: &'b LookupSubset<'b>, n3: i32) -> FontResult<Box<dyn LookupSubtableGenerator + 'b>> {
    let n4 = t.data.getuint16(n)?;
    match n4 {
        1 => Ok(Box::new(ContextualFormat1Generator::new_instance(n, n2, t, subset, builder, map, ls)?)),
        2 => Ok(Box::new(ContextualFormat2Generator::new_instance(t, subset, builder, n2, n, ls, n3)?)),
        3 => Ok(Box::new(ContextualFormat3Generator { orig_st_offset: n, new_st_offset: n2, lookup_subset: ls, orig_table: t, builder })),
        _ => Err(FontEngineException::invalid_font(format!("Invalid contextual lookup format: {}", n4))),
    }
}

struct ContextualFormat3Generator<'b> {
    orig_st_offset: i32,
    new_st_offset: i32,
    lookup_subset: &'b LookupSubset<'b>,
    orig_table: &'b LayoutTable,
    builder: &'b mut OTByteArrayBuilder,
}

impl LookupSubtableGenerator for ContextualFormat3Generator<'_> {
    fn write_subtable(&mut self) -> FontResult<i32> {
        let d = &self.orig_table.data;
        let o = self.orig_st_offset;
        let s = self.new_st_offset;
        let n = d.getuint16(o + 2)?;
        let n2 = d.getuint16(o + 4)?;
        let mut n3 = 6 + 2 * n;
        self.builder.ensure_capacity(s + n3);
        self.builder.setuint16(s, 3);
        self.builder.setuint16(s + 2, n);
        let mut n4 = 0;
        for i in 0..n2 {
            let n5 = d.getuint16(o + 6 + 2 * n + 4 * i + 2)?;
            let n6 = self.lookup_subset.get_existing_subset_gid(n5)?;
            if n6 == -1 {
                continue;
            }
            n3 += 4;
            self.builder.ensure_capacity(s + n3);
            self.builder.setuint16(s + 6 + 2 * n + 4 * n4, d.getuint16(o + 6 + 2 * n + 4 * i)?);
            self.builder.setuint16(s + 6 + 2 * n + 4 * n4 + 2, n6);
            n4 += 1;
        }
        self.builder.setuint16(s + 4, n4);
        Ok(n3)
    }
}

struct Format2Members<'b> {
    lookup_subset: &'b LookupSubset<'b>,
    class_covered: ClassCoveredBySubset<'b>,
    orig_class_def_offset: i32,
    num_glyphs: i32,
}

impl SetMembers for Format2Members<'_> {
    fn which_rule_set_index_applies(&mut self, g: &SetGenerator, n: i32, _n2: i32) -> FontResult<i32> {
        g.orig_table.get_class_index(n, self.orig_class_def_offset)
    }

    fn compute_members_to_keep(&mut self, g: &SetGenerator, _n: i32, n2: i32) -> FontResult<Vec<bool>> {
        let d = &g.orig_table.data;
        let n3 = d.getuint16(n2)?;
        let mut bl = vec![true; n3 as usize];
        for i in 0..n3 {
            let n4 = d.get_offset(n2, 2 + 2 * i)?;
            let n5 = d.getuint16(n4)?;
            for j in 0..n5 - 1 {
                if self.class_covered.class_covered_by_subset(g.orig_table, self.orig_class_def_offset, self.num_glyphs, d.getuint16(n4 + 4 + 2 * j)?)? {
                    continue;
                }
                bl[i as usize] = false;
                break;
            }
        }
        Ok(bl)
    }

    fn write_member(&mut self, g: &mut SetGenerator, n: i32, n2: i32) -> FontResult<i32> {
        let d = &g.orig_table.data;
        let n4 = d.getuint16(n2)?;
        let n5 = d.getuint16(n2 + 2)?;
        let mut n6 = 4 + 2 * (n4 - 1);
        g.builder.ensure_capacity(n + n6);
        g.builder.setuint16(n, n4);
        for n3 in 0..n4 - 1 {
            g.builder.setuint16(n + 4 + 2 * n3, d.getuint16(n2 + 4 + n3 * 2)?);
        }
        let mut n3 = 0;
        for i in 0..n5 {
            let n7 = self.lookup_subset.get_existing_subset_gid(d.getuint16(n2 + 4 + 2 * (n4 - 1) + i * 4 + 2)?)?;
            if n7 == -1 {
                continue;
            }
            n6 += 4;
            g.builder.ensure_capacity(n + n6);
            let n8 = d.getuint16(n2 + 4 + 2 * (n4 - 1) + i * 4)?;
            g.builder.setuint16(n + 4 + 2 * (n4 - 1) + 4 * n3, n8);
            g.builder.setuint16(n + 4 + 2 * (n4 - 1) + 4 * n3 + 2, n7);
            n3 += 1;
        }
        g.builder.setuint16(n + 2, n3);
        Ok(n6)
    }

    fn get_orig_record_size(&self) -> i32 {
        0
    }
}

struct ContextualFormat2Generator<'b> {
    g: SetGenerator<'b>,
    m: Format2Members<'b>,
    class_count: i32,
}

impl<'b> ContextualFormat2Generator<'b> {
    fn new_instance(t: &'b LayoutTable, subset: &'b dyn Subset, builder: &'b mut OTByteArrayBuilder, n: i32, n2: i32, ls: &'b LookupSubset<'b>, n3: i32) -> FontResult<Self> {
        let n4 = t.data.getuint16(n2 + 6)?;
        let g = SetGenerator::new(t, subset, builder, n, n2, n4, 6, true, true)?;
        let orig_class_def_offset = t.data.get_offset(n2, 4)?;
        Ok(ContextualFormat2Generator { g, m: Format2Members { lookup_subset: ls, class_covered: ClassCoveredBySubset::new(subset), orig_class_def_offset, num_glyphs: n3 }, class_count: n4 })
    }

    fn write_class_def(&mut self) -> FontResult<()> {
        let g = &mut self.g;
        let cdg = ClassDefGenerator::new_instance(g.orig_table, self.m.orig_class_def_offset, g.subset, self.m.num_glyphs)?;
        g.builder.setuint16(g.new_st_offset + 4, g.subtable_size);
        let a = cdg.generate_class().to_ot_byte_array();
        let at = g.new_st_offset + g.subtable_size;
        g.subtable_size += g.write_byte_array_at_offset(at, &a)?;
        Ok(())
    }
}

impl CoverageConsumer for ContextualFormat2Generator<'_> {
    fn glyph_info(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        self.g.glyph_info(&mut self.m, n, n2)
    }
}

impl LookupSubtableGenerator for ContextualFormat2Generator<'_> {
    fn write_subtable(&mut self) -> FontResult<i32> {
        let t = self.g.orig_table;
        let subset = self.g.subset;
        let n = t.data.get_offset(self.g.orig_st_offset, 2)?;
        let s = self.g.new_st_offset;
        self.g.builder.ensure_capacity(s + 8 + 2 * self.class_count);
        self.g.builder.setuint16(s, 2);
        self.g.builder.setuint16(s + 6, self.class_count);
        t.iterate_coverage(n, Some(subset), self)?;
        self.write_class_def()?;
        Ok(self.g.subtable_size)
    }
}

struct Format1Members<'b> {
    lookup_subset: &'b LookupSubset<'b>,
}

impl SetMembers for Format1Members<'_> {
    fn which_rule_set_index_applies(&mut self, _g: &SetGenerator, _n: i32, n2: i32) -> FontResult<i32> {
        Ok(n2)
    }

    fn compute_members_to_keep(&mut self, g: &SetGenerator, _n: i32, n2: i32) -> FontResult<Vec<bool>> {
        let d = &g.orig_table.data;
        let n3 = d.getuint16(n2)?;
        let mut bl = vec![true; n3 as usize];
        for i in 0..n3 {
            let n4 = d.get_offset(n2, 2 + 2 * i)?;
            let n5 = d.getuint16(n4)?;
            for j in 0..n5 - 1 {
                if g.subset.get_existing_subset_gid(d.getuint16(n4 + 4 + 2 * j)?)? != -1 {
                    continue;
                }
                bl[i as usize] = false;
                break;
            }
        }
        Ok(bl)
    }

    fn write_member(&mut self, g: &mut SetGenerator, n: i32, n2: i32) -> FontResult<i32> {
        let d = &g.orig_table.data;
        let n5 = d.getuint16(n2)?;
        let n6 = d.getuint16(n2 + 2)?;
        let mut n7 = 4 + 2 * (n5 - 1);
        g.builder.ensure_capacity(n + n7);
        g.builder.setuint16(n, n5);
        for n4 in 0..n5 - 1 {
            let n3 = g.subset.get_existing_subset_gid(d.getuint16(n2 + 4 + n4 * 2)?)?;
            g.builder.setuint16(n + 4 + 2 * n4, n3);
        }
        let mut n4 = 0;
        for n3 in 0..n6 {
            let n8 = self.lookup_subset.get_existing_subset_gid(d.getuint16(n2 + 4 + 2 * (n5 - 1) + n3 * 4 + 2)?)?;
            if n8 == -1 {
                continue;
            }
            n7 += 4;
            g.builder.ensure_capacity(n + n7);
            let n9 = d.getuint16(n2 + 4 + 2 * (n5 - 1) + n3 * 4)?;
            g.builder.setuint16(n + 4 + 2 * (n5 - 1) + 4 * n4, n9);
            g.builder.setuint16(n + 4 + 2 * (n5 - 1) + 4 * n4 + 2, n8);
            n4 += 1;
        }
        g.builder.setuint16(n + 2, n4);
        Ok(n7)
    }

    fn get_orig_record_size(&self) -> i32 {
        0
    }
}

struct ContextualFormat1Generator<'b> {
    g: SetGenerator<'b>,
    m: Format1Members<'b>,
}

impl<'b> ContextualFormat1Generator<'b> {
    fn new_instance(n: i32, n2: i32, t: &'b LayoutTable, subset: &'b dyn Subset, builder: &'b mut OTByteArrayBuilder, map: &CoverageMap, ls: &'b LookupSubset<'b>) -> FontResult<Self> {
        let n3 = t.data.get_offset(n, 2)?;
        let glyph_count = map.get(&n3).expect("java.lang.NullPointerException").glyph_count;
        let g = SetGenerator::new(t, subset, builder, n2, n, glyph_count, 4, false, true)?;
        g.builder.ensure_capacity(n2 + 6 + 2 * glyph_count);
        g.builder.setuint16(n2, 1);
        g.builder.setuint16(n2 + 4, glyph_count);
        let mut gen = ContextualFormat1Generator { g, m: Format1Members { lookup_subset: ls } };
        t.iterate_coverage(n3, Some(subset), &mut gen)?;
        Ok(gen)
    }
}

impl CoverageConsumer for ContextualFormat1Generator<'_> {
    fn glyph_info(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        self.g.glyph_info(&mut self.m, n, n2)
    }
}

impl LookupSubtableGenerator for ContextualFormat1Generator<'_> {
    fn write_subtable(&mut self) -> FontResult<i32> {
        self.g.write_subtable(&mut self.m)
    }
}
