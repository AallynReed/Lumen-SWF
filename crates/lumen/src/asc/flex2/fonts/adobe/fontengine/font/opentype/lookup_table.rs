//! Port of afe.jar `com.adobe.fontengine.font.opentype.LookupTable` (the parts outside inline formatting).

use std::ops::Deref;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table::LayoutTable;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

/// `Tag.script_DFLT`.
const SCRIPT_DFLT: i32 = 0x44464C54;

#[derive(Clone, Debug, Default)]
pub struct LookupTable {
    pub base: LayoutTable,
}

impl Deref for LookupTable {
    type Target = LayoutTable;
    fn deref(&self) -> &LayoutTable {
        &self.base
    }
}

impl LookupTable {
    pub fn new(font_byte_array: FontByteArray) -> LookupTable {
        LookupTable { base: LayoutTable::new(font_byte_array) }
    }

    /// Same in `Gsub` and `Gpos`.
    pub fn get_script_list_offset(&self) -> FontResult<i32> {
        self.data.get_offset(0, 4)
    }

    pub fn get_feature_list_offset(&self) -> FontResult<i32> {
        self.data.get_offset(0, 6)
    }

    pub fn get_lookup_list_offset(&self) -> FontResult<i32> {
        self.data.get_offset(0, 8)
    }

    pub fn resolve_feature_tag(&self, n: i32, n2: i32, n_array: &[i32]) -> FontResult<Vec<Vec<i32>>> {
        let mut result: Vec<Vec<i32>> = vec![Vec::new(); n_array.len()];
        let n6 = self.get_script_list_offset()?;
        let n7 = self.data.getuint16(n6)?;
        let mut n8 = -1;
        for n5 in 0..n7 {
            let n4 = self.data.getint32(n6 + 2 + 6 * n5)?;
            if n == n4 {
                n8 = n5;
                break;
            }
            if SCRIPT_DFLT != n4 {
                continue;
            }
            n8 = n5;
        }
        if n8 == -1 {
            return Ok(result);
        }
        let n5 = self.data.get_offset(n6, 2 + 6 * n8 + 4)?;
        let n4 = self.data.getuint16(n5 + 2)?;
        let mut n9 = self.data.get_offset(n5, 0)?;
        for n3 in 0..n4 {
            if n2 as i64 != self.data.getuint32(n5 + 4 + 6 * n3)? {
                continue;
            }
            n9 = self.data.get_offset(n5, 4 + 6 * n3 + 4)?;
            break;
        }
        if n9 == 0 {
            return Ok(result);
        }
        let n3 = self.get_feature_list_offset()?;
        for i in 0..n_array.len() {
            let mut n13 = -1;
            if 0 == n_array[i] {
                let n12 = self.data.getuint16(n9 + 2)?;
                n13 = if n12 == 65535 { -1 } else { n12 };
            } else {
                let n12 = self.data.getuint16(n9 + 4)?;
                for n11 in 0..n12 {
                    let n10 = self.data.getuint16(n9 + 6 + 2 * n11)?;
                    if n_array[i] as i64 != self.data.getuint32(n3 + 2 + 6 * n10)? {
                        continue;
                    }
                    n13 = n10;
                }
            }
            if n13 == -1 {
                continue;
            }
            let n12 = n3 + self.data.getuint16(n3 + 2 + n13 * 6 + 4)?;
            let n11 = self.data.getuint16(n12 + 2)?;
            let mut v = Vec::with_capacity(n11 as usize);
            for n10 in 0..n11 {
                v.push(self.data.getuint16(n12 + 4 + n10 * 2)?);
            }
            v.sort();
            let n10 = self.get_lookup_list_offset()?;
            for e in v.iter_mut() {
                *e = self.data.get_offset(n10, 2 + 2 * *e)?;
            }
            result[i] = v;
        }
        Ok(result)
    }

    pub fn feature_is_present(&self, n: i32) -> FontResult<bool> {
        let n2 = self.get_feature_list_offset()?;
        let n3 = self.data.getuint16(n2)?;
        for i in 0..n3 {
            if n as i64 != self.data.getuint32(n2 + 2 + 6 * i)? {
                continue;
            }
            return Ok(true);
        }
        Ok(false)
    }
}
