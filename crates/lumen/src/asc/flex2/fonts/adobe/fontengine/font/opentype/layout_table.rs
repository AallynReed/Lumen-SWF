//! Port of afe.jar `com.adobe.fontengine.font.opentype.LayoutTable`.

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::OTByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

#[derive(Clone, Debug, Default)]
pub struct LayoutTable {
    pub data: OTByteArray,
}

/// `LayoutTable.CoverageConsumer`.
pub trait CoverageConsumer {
    fn glyph_info(&mut self, gid: i32, coverage_index: i32) -> FontResult<bool>;
}

/// `LayoutTable.ClassConsumer`.
pub trait ClassConsumer {
    fn glyph(&mut self, gid: i32, class: i32) -> FontResult<bool>;
}

/// A coverage consumer that does the `subset.getExistingSubsetGid(gid) == -1` filtering itself, for callers whose
/// subset is mutated by the consumer.
pub trait CoverageVisitor {
    fn skip(&mut self, gid: i32) -> FontResult<bool>;
    fn glyph_info(&mut self, gid: i32, coverage_index: i32) -> FontResult<bool>;
}

struct SubsetFilter<'a, 'b> {
    subset: Option<&'a dyn Subset>,
    consumer: &'b mut dyn CoverageConsumer,
}

impl CoverageVisitor for SubsetFilter<'_, '_> {
    fn skip(&mut self, gid: i32) -> FontResult<bool> {
        match self.subset {
            Some(s) => Ok(s.get_existing_subset_gid(gid)? == -1),
            None => Ok(false),
        }
    }

    fn glyph_info(&mut self, gid: i32, coverage_index: i32) -> FontResult<bool> {
        self.consumer.glyph_info(gid, coverage_index)
    }
}

impl LayoutTable {
    pub fn new(font_byte_array: FontByteArray) -> LayoutTable {
        LayoutTable { data: OTByteArray::from_font_byte_array(font_byte_array) }
    }

    fn iterate_class_format1(&self, n: i32, n2: i32, c: &mut dyn ClassConsumer, n3: i32) -> FontResult<()> {
        let n5 = self.data.getuint16(n + 2)?;
        let n6 = n5 + self.data.getuint16(n + 4)?;
        if n3 == 0 || n3 == -1 {
            for n4 in 0..n5 {
                if !c.glyph(n4, 0)? {
                    return Ok(());
                }
            }
        }
        for n4 in n5..n6 {
            let n7 = self.data.getuint16(n + 6 + 2 * (n4 - n5))?;
            if n7 != n3 && n3 != -1 || c.glyph(n4, n7)? {
                continue;
            }
            return Ok(());
        }
        if n3 == 0 || n3 == -1 {
            for n4 in n6..n2 {
                if !c.glyph(n4, 0)? {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn iterate_class_format2(&self, n: i32, n2: i32, c: &mut dyn ClassConsumer, n3: i32) -> FontResult<()> {
        let mut n5 = 0;
        let n6 = self.data.getuint16(n + 2)?;
        if n3 == 0 || n3 == -1 {
            if n6 == 0 {
                while n5 < n2 {
                    let g = n5;
                    n5 += 1;
                    if !c.glyph(g, 0)? {
                        return Ok(());
                    }
                }
                return Ok(());
            }
            let n4 = self.data.getuint16(n + 4)?;
            while n5 < n4 {
                let g = n5;
                n5 += 1;
                if !c.glyph(g, 0)? {
                    return Ok(());
                }
            }
        }
        for i in 0..n6 {
            let n7 = self.data.getuint16(n + 4 + 6 * i + 4)?;
            let mut n8 = self.data.getuint16(n + 4 + 6 * i + 2)?;
            if n7 == n3 || n3 == -1 {
                let n4 = self.data.getuint16(n + 4 + 6 * i)?;
                n8 = self.data.getuint16(n + 4 + 6 * i + 2)?;
                n5 = n4;
                while n5 <= n8 {
                    if !c.glyph(n5, n7)? {
                        return Ok(());
                    }
                    n5 += 1;
                }
            }
            n5 = n8 + 1;
            if n3 != 0 && n3 != -1 {
                continue;
            }
            n8 = if i == n6 - 1 { n2 } else { self.data.getuint16(n + 4 + 6 * (i + 1))? };
            while n5 < n8 {
                let g = n5;
                n5 += 1;
                if !c.glyph(g, 0)? {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    pub fn iterate_class(&self, n: i32, n2: i32, c: &mut dyn ClassConsumer, n3: i32) -> FontResult<()> {
        let n4 = self.data.getuint16(n)?;
        match n4 {
            1 => self.iterate_class_format1(n, n2, c, n3),
            2 => self.iterate_class_format2(n, n2, c, n3),
            _ => Err(FontEngineException::invalid_font(format!("Invalid class def format ({})", n4))),
        }
    }

    fn iterate_format1_coverage(&self, n: i32, v: &mut dyn CoverageVisitor) -> FontResult<()> {
        let n2 = self.data.getuint16(n + 2)?;
        for i in 0..n2 {
            let n3 = self.data.getuint16(n + 4 + 2 * i)?;
            if v.skip(n3)? || v.glyph_info(n3, i)? {
                continue;
            }
            return Ok(());
        }
        Ok(())
    }

    fn iterate_format2_coverage(&self, n: i32, v: &mut dyn CoverageVisitor) -> FontResult<()> {
        let n2 = self.data.getuint16(n + 2)?;
        let mut n3 = 0;
        let mut n4 = if n2 > 0 { self.data.getuint16(n + 4 + 2)? - self.data.getuint16(n + 4)? + 1 } else { 0 };
        let mut i = 0;
        while i < n4 || n3 < n2 - 1 {
            if i == n4 {
                i = 0;
                n3 += 1;
                n4 = self.data.getuint16(n + 4 + 6 * n3 + 2)? - self.data.getuint16(n + 4 + 6 * n3)? + 1;
            }
            let n6 = self.data.getuint16(n + 4 + 6 * n3)? + i;
            if !v.skip(n6)? {
                let n5 = self.data.getuint16(n + 4 + 6 * n3 + 4)? + i;
                if !v.glyph_info(n6, n5)? {
                    return Ok(());
                }
            }
            i += 1;
        }
        Ok(())
    }

    /// `iterateCoverage(n, subset, consumer)`: `subset` filters out the glyphs it does not contain.
    pub fn iterate_coverage(&self, n: i32, subset: Option<&dyn Subset>, c: &mut dyn CoverageConsumer) -> FontResult<()> {
        self.iterate_coverage_with(n, &mut SubsetFilter { subset, consumer: c })
    }

    pub fn iterate_coverage_with(&self, n: i32, v: &mut dyn CoverageVisitor) -> FontResult<()> {
        let n2 = self.data.getuint16(n)?;
        match n2 {
            1 => self.iterate_format1_coverage(n, v),
            2 => self.iterate_format2_coverage(n, v),
            _ => Err(FontEngineException::invalid_font(format!("Invalid coverage format {}", n2))),
        }
    }

    pub fn get_class_index(&self, n: i32, n2: i32) -> FontResult<i32> {
        if n == -1 {
            return Ok(-1);
        }
        let n3 = self.data.getuint16(n2)?;
        match n3 {
            1 => {
                let n4 = self.data.getuint16(n2 + 2)?;
                let n5 = self.data.getuint16(n2 + 4)?;
                if n4 <= n && n < n4 + n5 {
                    return self.data.getuint16(n2 + 6 + (n - n4) * 2);
                }
                Ok(0)
            }
            2 => {
                let n6 = self.data.getuint16(n2 + 2)?;
                let mut n7 = 0;
                let mut n8 = n6 - 1;
                while n7 <= n8 {
                    let n9 = (n7 + n8) / 2;
                    let n10 = self.data.getuint16(n2 + 4 + n9 * 6)?;
                    let n11 = self.data.getuint16(n2 + 4 + n9 * 6 + 2)?;
                    if n < n10 {
                        n8 = n9 - 1;
                    } else if n11 < n {
                        n7 = n9 + 1;
                    } else {
                        return self.data.getuint16(n2 + 4 + n9 * 6 + 4);
                    }
                }
                Ok(0)
            }
            _ => Err(FontEngineException::invalid_font(format!("invalid classdef format ({})", n3))),
        }
    }
}
