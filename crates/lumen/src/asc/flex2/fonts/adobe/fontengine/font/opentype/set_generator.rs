//! Port of afe.jar `com.adobe.fontengine.font.opentype.SetGenerator`.

use std::collections::HashMap;

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table::LayoutTable;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table_subsetter;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

/// The abstract methods of `SetGenerator`, implemented by each subclass.
pub trait SetMembers {
    fn which_rule_set_index_applies(&mut self, g: &SetGenerator, n: i32, n2: i32) -> FontResult<i32>;
    fn compute_members_to_keep(&mut self, g: &SetGenerator, n: i32, n2: i32) -> FontResult<Vec<bool>>;
    fn write_member(&mut self, g: &mut SetGenerator, n: i32, n2: i32) -> FontResult<i32>;
    fn get_orig_record_size(&self) -> i32;
}

/// The state of `SetGenerator`.
pub struct SetGenerator<'b> {
    pub subset: &'b dyn Subset,
    pub builder: &'b mut OTByteArrayBuilder,
    pub orig_table: &'b LayoutTable,
    pub new_st_offset: i32,
    pub orig_st_offset: i32,
    pub subtable_size: i32,
    pub this_set: i32,
    pub orig_set_count_delta: i32,
    pub preserve_rule_set_indexing: bool,
    subset_gid_to_orig_set_offset: Vec<i32>,
    orig_set_offset_to_new_set_offset: HashMap<i32, i32>,
    set_elements_have_offsets: bool,
}

impl<'b> SetGenerator<'b> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(orig_table: &'b LayoutTable, subset: &'b dyn Subset, builder: &'b mut OTByteArrayBuilder, n: i32, n2: i32, mut n3: i32, n4: i32, bl: bool, bl2: bool) -> FontResult<SetGenerator<'b>> {
        let subtable_size = n4 + 2 + 2 * n3;
        let mut subset_gid_to_orig_set_offset = Vec::new();
        if bl {
            n3 = orig_table.data.getuint16(n2 + n4)?;
        } else {
            subset_gid_to_orig_set_offset = vec![-1; subset.get_num_glyphs().max(0) as usize];
        }
        builder.ensure_capacity(n + n4 + 2 + 2 * n3);
        for i in 0..n3 {
            builder.setuint16(n + n4 + 2 + 2 * i, 0);
        }
        Ok(SetGenerator {
            subset,
            builder,
            orig_table,
            new_st_offset: n,
            orig_st_offset: n2,
            subtable_size,
            this_set: 0,
            orig_set_count_delta: n4,
            preserve_rule_set_indexing: bl,
            subset_gid_to_orig_set_offset,
            orig_set_offset_to_new_set_offset: HashMap::new(),
            set_elements_have_offsets: bl2,
        })
    }

    /// `glyphInfo(gid, coverageIndex)` as a `CoverageConsumer`.
    pub fn glyph_info(&mut self, m: &mut dyn SetMembers, n: i32, n2: i32) -> FontResult<bool> {
        let n3 = m.which_rule_set_index_applies(self, n, n2)?;
        let n4 = self.orig_table.data.get_offset(self.orig_st_offset, self.orig_set_count_delta + 2 + 2 * n3)?;
        if self.preserve_rule_set_indexing {
            self.write_subtable_entry(m, n3, n4)?;
        } else {
            let g = self.subset.get_existing_subset_gid(n)?;
            self.subset_gid_to_orig_set_offset[g as usize] = n4;
        }
        Ok(true)
    }

    pub fn write_subtable(&mut self, m: &mut dyn SetMembers) -> FontResult<i32> {
        if self.preserve_rule_set_indexing {
            return Ok(self.subtable_size);
        }
        let mut n = 0;
        for i in 0..self.subset_gid_to_orig_set_offset.len() {
            if self.subset_gid_to_orig_set_offset[i] == -1 {
                continue;
            }
            let o = self.subset_gid_to_orig_set_offset[i];
            self.write_subtable_entry(m, n, o)?;
            n += 1;
        }
        Ok(self.subtable_size)
    }

    fn write_subtable_entry(&mut self, m: &mut dyn SetMembers, n: i32, n2: i32) -> FontResult<()> {
        let slot = self.new_st_offset + self.orig_set_count_delta + 2 + 2 * n;
        if self.builder.getuint16(slot) != 0 {
            return Ok(());
        }
        if n2 != 0 {
            match self.orig_set_offset_to_new_set_offset.get(&n2) {
                None => {
                    self.orig_set_offset_to_new_set_offset.insert(n2, self.subtable_size);
                    self.builder.setuint16(slot, self.subtable_size);
                    let at = self.new_st_offset + self.subtable_size;
                    self.subtable_size += self.write_set(m, at, n2)?;
                }
                Some(&n3) => {
                    self.builder.setuint16(slot, n3);
                }
            }
        }
        Ok(())
    }

    fn write_set(&mut self, m: &mut dyn SetMembers, n: i32, n2: i32) -> FontResult<i32> {
        let bl_array = m.compute_members_to_keep(self, n, n2)?;
        let n4 = bl_array.iter().filter(|&&b| b).count() as i32;
        let mut n3 = if self.set_elements_have_offsets { 2 + 2 * n4 } else { 2 };
        self.builder.ensure_capacity(n + n3);
        self.builder.setuint16(n, n4);
        let mut n5 = 0;
        for i in 0..bl_array.len() as i32 {
            if !bl_array[i as usize] {
                continue;
            }
            let n6;
            if self.set_elements_have_offsets {
                self.builder.setuint16(n + 2 + 2 * n5, n3);
                n6 = self.orig_table.data.get_offset(n2, 2 + 2 * i)?;
            } else {
                n6 = n2 + 2 + m.get_orig_record_size() * i;
            }
            n3 += m.write_member(self, n + n3, n6)?;
            n5 += 1;
        }
        Ok(n3)
    }

    pub fn write_byte_array_at_offset(&mut self, n: i32, a: &OTByteArray) -> FontResult<i32> {
        layout_table_subsetter::write_byte_array_at_offset(self.builder, n, a)
    }
}
