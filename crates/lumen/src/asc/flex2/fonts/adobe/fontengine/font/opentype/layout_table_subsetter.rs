//! Port of afe.jar `com.adobe.fontengine.font.opentype.LayoutTableSubsetter` and its coverage and class
//! generators.

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table::{ClassConsumer, CoverageConsumer, LayoutTable};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub const MAX_OFFSET: i32 = 65535;

pub struct LayoutTableSubsetter<'a> {
    pub orig_table: &'a LayoutTable,
    pub builder: OTByteArrayBuilder,
}

impl<'a> LayoutTableSubsetter<'a> {
    pub fn new(orig_table: &'a LayoutTable, builder: OTByteArrayBuilder) -> Self {
        LayoutTableSubsetter { orig_table, builder }
    }

    pub fn write_byte_array_at_offset(&mut self, n: i32, a: &OTByteArray) -> FontResult<i32> {
        write_byte_array_at_offset(&mut self.builder, n, a)
    }

    pub fn write_class_def(&mut self, n: i32, n2: i32, n3: i32, n4: i32, subset: &dyn Subset, n5: i32) -> FontResult<i32> {
        write_class_def(self.orig_table, &mut self.builder, n, n2, n3, n4, subset, n5)
    }
}

/// `writeByteArrayAtOffset`, shared by the subsetters and `SetGenerator`.
pub fn write_byte_array_at_offset(builder: &mut OTByteArrayBuilder, n: i32, a: &OTByteArray) -> FontResult<i32> {
    builder.replace_from(n, &a.base, 0, a.get_size())?;
    Ok(a.get_size())
}

/// `LayoutTableSubsetter.writeClassDef`.
#[allow(clippy::too_many_arguments)]
pub fn write_class_def(orig: &LayoutTable, builder: &mut OTByteArrayBuilder, n: i32, n2: i32, n3: i32, n4: i32, subset: &dyn Subset, n5: i32) -> FontResult<i32> {
    let g = ClassDefGenerator::new_instance(orig, orig.data.get_offset(n, n2)?, subset, n5)?;
    builder.setuint16(n3 + n2, n4);
    let a = g.generate_class().to_ot_byte_array();
    write_byte_array_at_offset(builder, n3 + n4, &a)
}

/// `LayoutTableSubsetter.ClassCoveredBySubset`.
pub struct ClassCoveredBySubset<'s> {
    subset: &'s dyn Subset,
    glyph_found: bool,
}

impl ClassConsumer for ClassCoveredBySubset<'_> {
    fn glyph(&mut self, n: i32, _n2: i32) -> FontResult<bool> {
        if self.subset.get_existing_subset_gid(n)? != -1 {
            self.glyph_found = true;
            return Ok(false);
        }
        Ok(true)
    }
}

impl<'s> ClassCoveredBySubset<'s> {
    pub fn new(subset: &'s dyn Subset) -> Self {
        ClassCoveredBySubset { subset, glyph_found: false }
    }

    pub fn class_covered_by_subset(&mut self, t: &LayoutTable, n: i32, n2: i32, n3: i32) -> FontResult<bool> {
        self.glyph_found = false;
        t.iterate_class(n, n2, self, n3)?;
        Ok(self.glyph_found)
    }
}

/// `CoverageGenerator` with its `CoverageFormat1Generator` / `CoverageFormat2Generator` subclasses.
pub struct CoverageGenerator {
    pub new_coverage: OTByteArrayBuilder,
    pub orig_coverage_offset: i32,
    pub num_glyphs_found: i32,
    format2_num_ranges: Option<i32>,
    subset_glyph_in_coverage: Vec<bool>,
}

impl CoverageGenerator {
    pub fn new_instance(t: &LayoutTable, n: i32, subset: &dyn Subset) -> FontResult<CoverageGenerator> {
        let mut d = CoverageFormatDecider::new(subset);
        t.iterate_coverage(n, None, &mut d)?;
        let n2 = d.count_ranges();
        let n3 = 2 * d.num_gids;
        let n4 = 6 * n2;
        if n3 < n4 {
            let mut c = OTByteArray::get_ot_byte_array_builder_instance_sized(4 + 2 * d.num_gids);
            c.setuint16(0, 1);
            c.setuint16(2, d.num_gids);
            return Ok(CoverageGenerator { new_coverage: c, orig_coverage_offset: n, num_glyphs_found: 0, format2_num_ranges: None, subset_glyph_in_coverage: d.subset_glyph_in_coverage });
        }
        let mut c = OTByteArray::get_ot_byte_array_builder_instance_sized(4 + 6 * n2);
        c.setuint16(0, 2);
        c.setuint16(2, n2);
        Ok(CoverageGenerator { new_coverage: c, orig_coverage_offset: n, num_glyphs_found: 0, format2_num_ranges: Some(n2), subset_glyph_in_coverage: d.subset_glyph_in_coverage })
    }

    fn write_ranges(&mut self) {
        let s = &self.subset_glyph_in_coverage;
        let mut n: i32 = 0;
        if s[0] {
            self.new_coverage.setuint16(4, 0);
            self.num_glyphs_found += 1;
        } else {
            n = -1;
        }
        for i in 1..s.len() {
            if s[i] {
                self.num_glyphs_found += 1;
            }
            if !s[i - 1] && s[i] {
                n += 1;
                self.new_coverage.setuint16(4 + 6 * n, i as i32);
                continue;
            }
            if !s[i - 1] || s[i] {
                continue;
            }
            self.new_coverage.setuint16(4 + 6 * n + 2, i as i32 - 1);
        }
        if s[s.len() - 1] {
            self.new_coverage.setuint16(4 + 6 * n + 2, s.len() as i32 - 1);
        }
    }

    /// `generateCoverage()`; the builder is handed over, Java callers only `toOTByteArray()` it.
    pub fn generate_coverage(&mut self) -> FontResult<OTByteArrayBuilder> {
        match self.format2_num_ranges {
            None => {
                let mut n = 0;
                for i in 0..self.subset_glyph_in_coverage.len() {
                    if !self.subset_glyph_in_coverage[i] {
                        continue;
                    }
                    self.new_coverage.setuint16(4 + 2 * n, i as i32);
                    n += 1;
                }
                self.num_glyphs_found = n;
            }
            Some(num_ranges) => {
                self.write_ranges();
                if num_ranges != 0 {
                    let mut n = 0;
                    for i in 0..num_ranges {
                        let n2 = self.new_coverage.getuint16(4 + 6 * i);
                        let n3 = self.new_coverage.getuint16(4 + 6 * i + 2);
                        self.new_coverage.setuint16(4 + 6 * i + 4, n);
                        n += 1 + n3 - n2;
                    }
                }
            }
        }
        Ok(std::mem::take(&mut self.new_coverage))
    }
}

/// `ClassDefGenerator` with its `ClassDef1Generator` / `ClassDef2Generator` subclasses.
pub struct ClassDefGenerator {
    pub subset_classes: Vec<i32>,
    builder: OTByteArrayBuilder,
    format2_n_ranges: Option<i32>,
}

impl ClassDefGenerator {
    pub fn new_instance(t: &LayoutTable, n: i32, subset: &dyn Subset, n2: i32) -> FontResult<ClassDefGenerator> {
        let mut d = ClassFormatDecider::new(subset);
        t.iterate_class(n, n2, &mut d, -1)?;
        let n3 = d.get_format1_size();
        let n4 = d.get_format2_size();
        if n3 < n4 {
            return Ok(ClassDefGenerator { subset_classes: d.subset_classes, builder: OTByteArray::get_ot_byte_array_builder_instance_sized(n3), format2_n_ranges: None });
        }
        Ok(ClassDefGenerator { subset_classes: d.subset_classes, builder: OTByteArray::get_ot_byte_array_builder_instance_sized(n4), format2_n_ranges: Some((n4 - 4) / 6) })
    }

    pub fn generate_class(mut self) -> OTByteArrayBuilder {
        let c = &self.subset_classes;
        let len = c.len();
        match self.format2_n_ranges {
            None => {
                let mut n2 = 0;
                while n2 < len && c[n2] == 0 {
                    n2 += 1;
                }
                let mut n = len as isize - 1;
                while n >= 0 && c[n as usize] == 0 {
                    n -= 1;
                }
                let (n, n2) = (n as i32, n2 as i32);
                self.builder.setuint16(0, 1);
                if n2 <= n {
                    self.builder.setuint16(2, n2);
                    self.builder.setuint16(4, n - n2 + 1);
                } else {
                    self.builder.setuint16(2, 0);
                    self.builder.setuint16(4, 0);
                }
                for i in 0..=(n - n2) {
                    self.builder.setuint16(6 + 2 * i, c[(n2 + i) as usize]);
                }
            }
            Some(n_ranges) => {
                self.builder.setuint16(0, 2);
                self.builder.setuint16(2, n_ranges);
                let mut n2: i32 = 0;
                let mut n = 0usize;
                while n < len && c[n] == 0 {
                    n += 1;
                }
                if n < len {
                    self.builder.setuint16(4, n as i32);
                    self.builder.setuint16(8, c[n]);
                    n2 += 1;
                }
                n += 1;
                while n < len {
                    while n < len && c[n] == c[n - 1] {
                        n += 1;
                    }
                    self.builder.setuint16(4 + 6 * (n2 - 1) + 2, n as i32 - 1);
                    while n < len && c[n] == 0 {
                        n += 1;
                    }
                    if n < len {
                        self.builder.setuint16(4 + 6 * n2, n as i32);
                        self.builder.setuint16(4 + 6 * n2 + 4, c[n]);
                        n2 += 1;
                    }
                    n += 1;
                }
            }
        }
        self.builder
    }
}

struct ClassFormatDecider<'s> {
    subset: &'s dyn Subset,
    subset_classes: Vec<i32>,
}

impl<'s> ClassFormatDecider<'s> {
    fn new(subset: &'s dyn Subset) -> Self {
        ClassFormatDecider { subset, subset_classes: vec![0; subset.get_num_glyphs().max(0) as usize] }
    }

    fn get_format1_size(&self) -> i32 {
        let c = &self.subset_classes;
        let mut n2 = 0usize;
        while n2 < c.len() && c[n2] == 0 {
            n2 += 1;
        }
        let mut n = c.len() as isize - 1;
        while n > n2 as isize && c[n as usize] == 0 {
            n -= 1;
        }
        6 + 2 * (n as i32 - n2 as i32 + 1)
    }

    fn get_format2_size(&self) -> i32 {
        let c = &self.subset_classes;
        let len = c.len();
        let mut n2 = 0;
        let mut n = 0usize;
        while n < len && c[n] == 0 {
            n += 1;
        }
        if n < len {
            n2 += 1;
        }
        n += 1;
        while n < len {
            while n < len && c[n] == c[n - 1] {
                n += 1;
            }
            while n < len && c[n] == 0 {
                n += 1;
            }
            if n < len {
                n2 += 1;
            }
            n += 1;
        }
        4 + 6 * n2
    }
}

impl ClassConsumer for ClassFormatDecider<'_> {
    fn glyph(&mut self, n: i32, n2: i32) -> FontResult<bool> {
        let n3 = self.subset.get_existing_subset_gid(n)?;
        if n3 != -1 {
            self.subset_classes[n3 as usize] = n2;
        }
        Ok(true)
    }
}

struct CoverageFormatDecider<'s> {
    subset: &'s dyn Subset,
    subset_glyph_in_coverage: Vec<bool>,
    num_gids: i32,
}

impl<'s> CoverageFormatDecider<'s> {
    fn new(subset: &'s dyn Subset) -> Self {
        CoverageFormatDecider { subset, subset_glyph_in_coverage: vec![false; subset.get_num_glyphs().max(0) as usize], num_gids: 0 }
    }

    fn count_ranges(&self) -> i32 {
        let s = &self.subset_glyph_in_coverage;
        let mut n = if s[0] { 1 } else { 0 };
        for i in 1..s.len() {
            if s[i - 1] || !s[i] {
                continue;
            }
            n += 1;
        }
        n
    }
}

impl CoverageConsumer for CoverageFormatDecider<'_> {
    fn glyph_info(&mut self, n: i32, _n2: i32) -> FontResult<bool> {
        let n3 = self.subset.get_existing_subset_gid(n)?;
        if n3 != -1 {
            self.subset_glyph_in_coverage[n3 as usize] = true;
            self.num_gids += 1;
        }
        Ok(true)
    }
}
