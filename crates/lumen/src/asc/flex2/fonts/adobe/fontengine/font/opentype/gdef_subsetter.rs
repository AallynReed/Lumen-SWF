//! Port of afe.jar `com.adobe.fontengine.font.opentype.GdefSubsetter`.

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gdef::Gdef;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table::{CoverageConsumer, LayoutTable};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table_subsetter::{ClassDefGenerator, CoverageGenerator, LayoutTableSubsetter};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub struct GdefSubsetter<'a> {
    pub base: LayoutTableSubsetter<'a>,
}

struct LigGlyphCollector<'s> {
    t: &'s LayoutTable,
    subset: &'s dyn Subset,
    num_lig_glyphs: i32,
    orig_lcl_offset: i32,
    keep_lig_glyph: Vec<i32>,
}

impl CoverageConsumer for LigGlyphCollector<'_> {
    fn glyph_info(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        if self.subset.get_existing_subset_gid(n)? != -1 {
            let d = &self.t.data;
            let n3 = d.get_offset(self.orig_lcl_offset, 4 + 2 * n2)?;
            let n4 = d.getuint16(n3)?;
            for i in 0..n4 {
                let n5 = d.get_offset(n3, 2 + 2 * i)?;
                let n6 = d.getuint16(n5)?;
                if n6 != 2 {
                    continue;
                }
                return Ok(true);
            }
            let g = self.subset.get_existing_subset_gid(n)?;
            self.keep_lig_glyph[g as usize] = n2;
            self.num_lig_glyphs += 1;
        }
        Ok(true)
    }
}

impl<'a> GdefSubsetter<'a> {
    pub fn new(gdef: &'a Gdef) -> Self {
        GdefSubsetter { base: LayoutTableSubsetter::new(&gdef.base, OTByteArray::get_ot_byte_array_builder_instance_sized(12)) }
    }

    pub fn subset_and_stream(&mut self, subset: &dyn Subset, n: i32) -> FontResult<OTByteArrayBuilder> {
        let t = self.base.orig_table;
        let mut n2 = 12;
        self.base.builder.setuint32(0, 65536);
        let mut n3 = t.data.get_offset(0, 4)?;
        if n3 == 0 {
            self.base.builder.setuint16(4, 0);
        } else {
            self.base.builder.setuint16(4, n2);
            n2 += self.write_class_def(n3, n2, subset, n)?;
        }
        self.base.builder.setuint16(6, 0);
        n3 = t.data.get_offset(0, 8)?;
        if n3 == 0 {
            self.base.builder.setuint16(8, 0);
        } else {
            self.base.builder.setuint16(8, n2);
            n2 += self.write_lig_caret_list(n3, n2, subset)?;
        }
        n3 = t.data.get_offset(0, 10)?;
        if n3 <= 10 {
            self.base.builder.setuint16(10, 0);
        } else {
            self.base.builder.setuint16(10, n2);
            self.write_class_def(n3, n2, subset, n)?;
        }
        Ok(std::mem::take(&mut self.base.builder))
    }

    pub fn write_lig_caret_list(&mut self, n: i32, n2: i32, subset: &dyn Subset) -> FontResult<i32> {
        let t = self.base.orig_table;
        let mut c = LigGlyphCollector { t, subset, num_lig_glyphs: 0, orig_lcl_offset: n, keep_lig_glyph: vec![-1; subset.get_num_glyphs().max(0) as usize] };
        let n3 = t.data.get_offset(n, 0)?;
        t.iterate_coverage(n3, None, &mut c)?;
        let mut n4 = 4 + 2 * c.num_lig_glyphs;
        self.base.builder.ensure_capacity(n2 + n4);
        self.base.builder.setuint16(n2, n4);
        n4 += self.write_coverage(t.data.get_offset(n, 0)?, n2 + n4, subset)?;
        self.base.builder.setuint16(n2 + 2, c.num_lig_glyphs);
        let mut n5 = 0;
        for &n6 in c.keep_lig_glyph.iter() {
            if n6 == -1 {
                continue;
            }
            self.base.builder.setuint16(n2 + 4 + 2 * n5, n4);
            n4 += self.write_lig_glyph(t.data.get_offset(n, 4 + 2 * n6)?, n2 + n4)?;
            n5 += 1;
        }
        Ok(n4)
    }

    pub fn write_coverage(&mut self, n: i32, n2: i32, subset: &dyn Subset) -> FontResult<i32> {
        let mut g = CoverageGenerator::new_instance(self.base.orig_table, n, subset)?;
        let a = g.generate_coverage()?.to_ot_byte_array();
        self.base.write_byte_array_at_offset(n2, &a)
    }

    pub fn write_lig_glyph(&mut self, n: i32, n2: i32) -> FontResult<i32> {
        let t = self.base.orig_table;
        let n3 = t.data.getuint16(n)?;
        let mut n4 = 2 + 2 * n3;
        self.base.builder.ensure_capacity(n2 + n4);
        self.base.builder.setuint16(n2, n3);
        for i in 0..n3 {
            self.base.builder.setuint16(n2 + 2 + 2 * i, n4);
            n4 += self.write_caret_value(t.data.get_offset(n, 2 + 2 * i)?, n2 + n4)?;
        }
        Ok(n4)
    }

    pub fn write_caret_value(&mut self, n: i32, n2: i32) -> FontResult<i32> {
        let t = self.base.orig_table;
        self.base.builder.ensure_capacity(n2 + 4);
        self.base.builder.setuint16(n2, 1);
        self.base.builder.setint16(n2 + 2, t.data.getint16(n + 2)?);
        Ok(4)
    }

    pub fn write_class_def(&mut self, n: i32, n2: i32, subset: &dyn Subset, n3: i32) -> FontResult<i32> {
        let g = ClassDefGenerator::new_instance(self.base.orig_table, n, subset, n3)?;
        let a = g.generate_class().to_ot_byte_array();
        self.base.write_byte_array_at_offset(n2, &a)
    }
}
