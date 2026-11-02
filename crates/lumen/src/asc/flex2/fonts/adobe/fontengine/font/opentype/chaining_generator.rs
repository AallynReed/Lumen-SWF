//! Port of afe.jar `com.adobe.fontengine.font.opentype.ChainingGenerator`.

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table::{CoverageConsumer, LayoutTable};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table_subsetter::{ClassCoveredBySubset, ClassDefGenerator};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table_subsetter::{add_to_coverages_map, patch_coverage_at_offset, CoverageMap, LookupSubset, LookupSubtableGenerator};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::OTByteArrayBuilder;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::set_generator::{SetGenerator, SetMembers};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

/// `newChainingInstance(table, builder, origOffset, newOffset, subset, coverages, lookupSubset, numGlyphs)`.
#[allow(clippy::too_many_arguments)]
pub fn new_chaining_instance<'b>(t: &'b LayoutTable, builder: &'b mut OTByteArrayBuilder, n: i32, n2: i32, subset: &'b dyn Subset, map: &CoverageMap, ls: &'b LookupSubset<'b>, n3: i32) -> FontResult<Box<dyn LookupSubtableGenerator + 'b>> {
    let n4 = t.data.getuint16(n)?;
    match n4 {
        1 => {
            let n5 = t.data.get_offset(n, 2)?;
            let count = map.get(&n5).expect("java.lang.NullPointerException").glyph_count;
            let g = SetGenerator::new(t, subset, builder, n2, n, count, 4, false, true)?;
            Ok(Box::new(ChainingFormat1Generator { g, m: Format1Members { lookup_subset: ls }, rule_count: count }))
        }
        2 => {
            let n5 = t.data.getuint16(n + 10)?;
            let g = SetGenerator::new(t, subset, builder, n2, n, n5, 10, true, true)?;
            Ok(Box::new(ChainingFormat2Generator { g, m: Format2Members { lookup_subset: ls, class_covered: ClassCoveredBySubset::new(subset), num_glyphs: n3 }, class_count: n5 }))
        }
        3 => Ok(Box::new(ChainingFormat3Generator { orig_table: t, builder, orig_st_offset: n, new_st_offset: n2, lookup_subset: ls })),
        _ => Err(FontEngineException::invalid_font(format!("Invalid chaining contextual lookup format: {}", n4))),
    }
}

pub fn gather_coverages_for_subtable(t: &LayoutTable, n: i32, _n2: i32, n3: i32, n4: i32, map: &mut CoverageMap, subset: &dyn Subset) -> FontResult<()> {
    let n5 = t.data.getuint16(n)?;
    match n5 {
        1 | 2 => {
            let n6 = t.data.get_offset(n, 2)?;
            add_to_coverages_map(t, n6, map, n3, n4, subset)
        }
        3 => {
            let mut n10 = t.data.getuint16(n + 2)?;
            for n9 in 0..n10 {
                let n8 = t.data.get_offset(n, 4 + 2 * n9)?;
                add_to_coverages_map(t, n8, map, n3, n4, subset)?;
            }
            let mut n9 = 4 + 2 * n10;
            n10 = t.data.getuint16(n + n9)?;
            for n8 in 0..n10 {
                let n7 = t.data.get_offset(n, n9 + 2 + 2 * n8)?;
                add_to_coverages_map(t, n7, map, n3, n4, subset)?;
            }
            n9 += 2 + 2 * n10;
            n10 = t.data.getuint16(n + n9)?;
            for n8 in 0..n10 {
                let n7 = t.data.get_offset(n, n9 + 2 + 2 * n8)?;
                add_to_coverages_map(t, n7, map, n3, n4, subset)?;
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
            let n6 = t.data.getuint16(n + 2)?;
            let n7 = t.data.getuint16(n + 4 + 2 * n6)?;
            let n8 = t.data.getuint16(n + 6 + 2 * n6 + 2 * n7)?;
            for n5 in 0..n6 {
                patch_coverage_at_offset(t, builder, n, n2, 4 + 2 * n5, map)?;
            }
            for n5 in 0..n7 {
                patch_coverage_at_offset(t, builder, n, n2, 6 + 2 * n6 + 2 * n5, map)?;
            }
            for n5 in 0..n8 {
                patch_coverage_at_offset(t, builder, n, n2, 8 + 2 * n7 + 2 * n6 + 2 * n5, map)?;
            }
            Ok(())
        }
        _ => Err(FontEngineException::invalid_font(format!("Unrecognized lookup type 6 format: {}", n4))),
    }
}

struct ChainingFormat3Generator<'b> {
    orig_table: &'b LayoutTable,
    builder: &'b mut OTByteArrayBuilder,
    orig_st_offset: i32,
    new_st_offset: i32,
    lookup_subset: &'b LookupSubset<'b>,
}

impl LookupSubtableGenerator for ChainingFormat3Generator<'_> {
    fn write_subtable(&mut self) -> FontResult<i32> {
        let d = &self.orig_table.data;
        let n = d.getuint16(self.orig_st_offset + 2)?;
        self.orig_st_offset += 4 + 2 * n;
        let n2 = d.getuint16(self.orig_st_offset)?;
        self.orig_st_offset += 2 + 2 * n2;
        let n3 = d.getuint16(self.orig_st_offset)?;
        self.orig_st_offset += 2 + 2 * n3;
        let n4 = d.getuint16(self.orig_st_offset)?;
        self.orig_st_offset += 2;
        let mut n5 = 10 + 2 * n + 2 * n2 + 2 * n3;
        self.builder.ensure_capacity(self.new_st_offset + n5);
        self.builder.setuint16(self.new_st_offset, 3);
        self.builder.setuint16(self.new_st_offset + 2, n);
        self.new_st_offset += 4 + 2 * n;
        self.builder.setuint16(self.new_st_offset, n2);
        self.new_st_offset += 2 + 2 * n2;
        self.builder.setuint16(self.new_st_offset, n3);
        self.new_st_offset += 2 + 2 * n3;
        let mut n6 = 0;
        for i in 0..n4 {
            let n7 = self.lookup_subset.get_existing_subset_gid(d.getuint16(self.orig_st_offset + 4 * i + 2)?)?;
            if n7 == -1 {
                continue;
            }
            n5 += 4;
            self.builder.ensure_capacity(self.new_st_offset + n5);
            self.builder.setuint16(self.new_st_offset + 2 + 4 * n6, d.getuint16(self.orig_st_offset + 4 * i)?);
            self.builder.setuint16(self.new_st_offset + 2 + 4 * n6 + 2, n7);
            n6 += 1;
        }
        self.builder.setuint16(self.new_st_offset, n6);
        Ok(n5)
    }
}

struct Format2Members<'b> {
    lookup_subset: &'b LookupSubset<'b>,
    class_covered: ClassCoveredBySubset<'b>,
    num_glyphs: i32,
}

impl SetMembers for Format2Members<'_> {
    fn which_rule_set_index_applies(&mut self, g: &SetGenerator, n: i32, _n2: i32) -> FontResult<i32> {
        g.orig_table.get_class_index(n, g.orig_table.data.get_offset(g.orig_st_offset, 6)?)
    }

    fn compute_members_to_keep(&mut self, g: &SetGenerator, _n: i32, n2: i32) -> FontResult<Vec<bool>> {
        let t = g.orig_table;
        let d = &t.data;
        let n3 = d.getuint16(n2)?;
        let mut bl = vec![true; n3 as usize];
        'block0: for i in 0..n3 {
            let n5 = d.get_offset(n2, 2 + 2 * i)?;
            let mut n6 = 2;
            let mut n7 = d.getuint16(n5)?;
            let mut n8 = d.get_offset(g.orig_st_offset, 4)?;
            for n4 in 0..n7 {
                if self.class_covered.class_covered_by_subset(t, n8, self.num_glyphs, d.getuint16(n5 + n6 + 2 * n4)?)? {
                    continue;
                }
                bl[i as usize] = false;
                break;
            }
            n6 += 2 * n7;
            n8 = d.get_offset(g.orig_st_offset, 6)?;
            n7 = d.getuint16(n5 + n6)?;
            n6 += 2;
            for n4 in 0..n7 - 1 {
                if self.class_covered.class_covered_by_subset(t, n8, self.num_glyphs, d.getuint16(n5 + n6 + 2 * n4)?)? {
                    continue;
                }
                bl[i as usize] = false;
                break;
            }
            n6 += 2 * (n7 - 1);
            n8 = d.get_offset(g.orig_st_offset, 8)?;
            n7 = d.getuint16(n5 + n6)?;
            n6 += 2;
            for n4 in 0..n7 {
                if self.class_covered.class_covered_by_subset(t, n8, self.num_glyphs, d.getuint16(n5 + n6 + 2 * n4)?)? {
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
        let n4 = d.getuint16(n2)?;
        let n5 = d.getuint16(n2 + 2 + 2 * n4)?;
        let n6 = d.getuint16(n2 + 2 + 2 * n4 + 2 + 2 * (n5 - 1))?;
        let n7 = d.getuint16(n2 + 2 + 2 * n4 + 2 + 2 * (n5 - 1) + 2 + 2 * n6)?;
        let mut n8 = 2 + 2 * n4 + 2 + 2 * (n5 - 1) + 2 + 2 * n6 + 2;
        g.builder.ensure_capacity(n + n8);
        g.builder.setuint16(n, n4);
        let mut n9 = 2;
        for _ in 0..n4 {
            g.builder.setuint16(n + n9, d.getuint16(n2 + n9)?);
            n9 += 2;
        }
        g.builder.setuint16(n + n9, n5);
        n9 += 2;
        for _ in 0..n5 - 1 {
            g.builder.setuint16(n + n9, d.getuint16(n2 + n9)?);
            n9 += 2;
        }
        g.builder.setuint16(n + n9, n6);
        n9 += 2;
        for _ in 0..n6 {
            g.builder.setuint16(n + n9, d.getuint16(n2 + n9)?);
            n9 += 2;
        }
        let n3 = n9;
        let mut n10 = 0;
        n9 += 2;
        for _ in 0..n7 {
            let n11 = self.lookup_subset.get_existing_subset_gid(d.getuint16(n2 + n9 + 2)?)?;
            if n11 != -1 {
                g.builder.ensure_capacity(n + n8 + 4);
                g.builder.setuint16(n + n8, d.getuint16(n2 + n9)?);
                g.builder.setuint16(n + n8 + 2, n11);
                n8 += 4;
                n10 += 1;
            }
            n9 += 4;
        }
        g.builder.setuint16(n + n3, n10);
        Ok(n8)
    }

    fn get_orig_record_size(&self) -> i32 {
        0
    }
}

struct ChainingFormat2Generator<'b> {
    g: SetGenerator<'b>,
    m: Format2Members<'b>,
    class_count: i32,
}

impl ChainingFormat2Generator<'_> {
    fn write_class_def(&mut self, n: i32) -> FontResult<()> {
        let g = &mut self.g;
        let cdg = ClassDefGenerator::new_instance(g.orig_table, g.orig_table.data.get_offset(g.orig_st_offset, n)?, g.subset, self.m.num_glyphs)?;
        g.builder.setuint16(g.new_st_offset + n, g.subtable_size);
        let a = cdg.generate_class().to_ot_byte_array();
        let at = g.new_st_offset + g.subtable_size;
        g.subtable_size += g.write_byte_array_at_offset(at, &a)?;
        Ok(())
    }
}

impl CoverageConsumer for ChainingFormat2Generator<'_> {
    fn glyph_info(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        self.g.glyph_info(&mut self.m, n, n2)
    }
}

impl LookupSubtableGenerator for ChainingFormat2Generator<'_> {
    fn write_subtable(&mut self) -> FontResult<i32> {
        let t = self.g.orig_table;
        let subset = self.g.subset;
        let n = t.data.get_offset(self.g.orig_st_offset, 2)?;
        let s = self.g.new_st_offset;
        self.g.builder.ensure_capacity(s + 12 + 2 * self.class_count);
        self.g.builder.setuint16(s, 2);
        self.g.builder.setuint16(s + 10, self.class_count);
        t.iterate_coverage(n, Some(subset), self)?;
        self.write_class_def(4)?;
        self.write_class_def(6)?;
        self.write_class_def(8)?;
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
        let s = g.subset;
        let n3 = d.getuint16(n2)?;
        let mut bl = vec![true; n3 as usize];
        'block0: for i in 0..n3 {
            let iu = i as usize;
            let n6 = d.get_offset(n2, 2 + 2 * i)?;
            let n7 = d.getuint16(n6)?;
            for n5 in 0..n7 {
                if s.get_existing_subset_gid(d.getuint16(n6 + 2 + 2 * n5)?)? != -1 {
                    continue;
                }
                bl[iu] = false;
                break;
            }
            if !bl[iu] {
                continue;
            }
            let n5 = d.getuint16(n6 + 2 + 2 * n7)?;
            for n4 in 0..n5 - 1 {
                if s.get_existing_subset_gid(d.getuint16(n6 + 4 + 2 * n7 + 2 * n4)?)? != -1 {
                    continue;
                }
                bl[iu] = false;
                break;
            }
            if !bl[iu] {
                continue;
            }
            let n4 = d.getuint16(n6 + 4 + 2 * (n5 - 1) + 2 * n7)?;
            for j in 0..n4 {
                if s.get_existing_subset_gid(d.getuint16(n6 + 6 + 2 * (n5 - 1) + 2 * n7 + 2 * j)?)? != -1 {
                    continue;
                }
                bl[iu] = false;
                continue 'block0;
            }
        }
        Ok(bl)
    }

    fn write_member(&mut self, g: &mut SetGenerator, mut n: i32, mut n2: i32) -> FontResult<i32> {
        let d = &g.orig_table.data;
        let s = g.subset;
        let n5 = d.getuint16(n2)?;
        let n6 = d.getuint16(n2 + 2 + 2 * n5)?;
        let n7 = d.getuint16(n2 + 4 + 2 * n5 + 2 * (n6 - 1))?;
        let n8 = d.getuint16(n2 + 6 + 2 * n5 + 2 * (n6 - 1) + 2 * n7)?;
        let mut n9 = 8 + 2 * n5 + 2 * (n6 - 1) + 2 * n7;
        g.builder.ensure_capacity(n + n9);
        g.builder.setuint16(n, n5);
        for n4 in 0..n5 {
            let n3 = s.get_existing_subset_gid(d.getuint16(n2 + 2 + 2 * n4)?)?;
            if n3 == -1 {
                panic!("Backtrack gid not in subset? Can't happen!");
            }
            g.builder.setuint16(n + 2 + 2 * n4, n3);
        }
        n2 += 2 + 2 * n5;
        n += 2 + 2 * n5;
        g.builder.setuint16(n, n6);
        for n4 in 0..n6 - 1 {
            let n3 = s.get_existing_subset_gid(d.getuint16(n2 + 2 + 2 * n4)?)?;
            if n3 == -1 {
                panic!("Input gid not in subset? Can't happen!");
            }
            g.builder.setuint16(n + 2 + 2 * n4, n3);
        }
        n2 += 2 + 2 * (n6 - 1);
        n += 2 + 2 * (n6 - 1);
        g.builder.setuint16(n, n7);
        for n4 in 0..n7 {
            let n3 = s.get_existing_subset_gid(d.getuint16(n2 + 2 + 2 * n4)?)?;
            if n3 == -1 {
                panic!("Lookahead gid not in subset? Can't happen!");
            }
            g.builder.setuint16(n + 2 + 2 * n4, n3);
        }
        n += 2 + 2 * n7;
        n2 += 2 + 2 * n7;
        let mut n4 = 0;
        for n3 in 0..n8 {
            let n10 = self.lookup_subset.get_existing_subset_gid(d.getuint16(n2 + 2 + 4 * n3 + 2)?)?;
            if n10 == -1 {
                continue;
            }
            n9 += 4;
            g.builder.ensure_capacity(n + n9);
            g.builder.setuint16(n + 2 + 4 * n4, d.getuint16(n2 + 2 + 4 * n3)?);
            g.builder.setuint16(n + 2 + 4 * n4 + 2, n10);
            n4 += 1;
        }
        g.builder.setuint16(n, n4);
        Ok(n9)
    }

    fn get_orig_record_size(&self) -> i32 {
        0
    }
}

struct ChainingFormat1Generator<'b> {
    g: SetGenerator<'b>,
    m: Format1Members<'b>,
    rule_count: i32,
}

impl CoverageConsumer for ChainingFormat1Generator<'_> {
    fn glyph_info(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        self.g.glyph_info(&mut self.m, n, n2)
    }
}

impl LookupSubtableGenerator for ChainingFormat1Generator<'_> {
    fn write_subtable(&mut self) -> FontResult<i32> {
        let t = self.g.orig_table;
        let subset = self.g.subset;
        let n = t.data.get_offset(self.g.orig_st_offset, 2)?;
        let s = self.g.new_st_offset;
        self.g.builder.ensure_capacity(s + 6 + 2 * self.rule_count);
        self.g.builder.setuint16(s, 1);
        self.g.builder.setuint16(s + 4, self.rule_count);
        t.iterate_coverage(n, Some(subset), self)?;
        self.g.write_subtable(&mut self.m)
    }
}
