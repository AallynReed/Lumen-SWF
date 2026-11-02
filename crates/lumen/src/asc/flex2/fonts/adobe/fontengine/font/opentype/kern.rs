//! Port of afe.jar `com.adobe.fontengine.font.opentype.Kern`.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{ExceptionKind, FontEngineException, FontResult};

pub const TAG_KERN: i32 = 1801810542;

/// `KernInterpretter`: `MSKernTable` or `AppleKernTable`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernInterpretter {
    Ms,
    Apple,
}

impl KernInterpretter {
    fn get_num_tables(self, d: &OTByteArray) -> FontResult<i32> {
        match self {
            KernInterpretter::Ms => {
                if d.get_size() < 4 {
                    return Ok(0);
                }
                d.getuint16(2)
            }
            KernInterpretter::Apple => {
                if d.get_size() < 8 {
                    return Ok(0);
                }
                d.getuint32asint(4, "Too many subtables in kern table")
            }
        }
    }

    fn get_offset_of_first_subtable(self) -> i32 {
        match self {
            KernInterpretter::Ms => 4,
            KernInterpretter::Apple => 8,
        }
    }

    fn get_subtable_header_size(self) -> i32 {
        match self {
            KernInterpretter::Ms => 6,
            KernInterpretter::Apple => 8,
        }
    }

    fn get_coverage(self, d: &OTByteArray, n: i32) -> FontResult<i32> {
        d.getuint16(n + 4)
    }

    fn get_length(self, d: &OTByteArray, n: i32) -> FontResult<i32> {
        match self {
            KernInterpretter::Ms => d.getuint16(n + 2),
            KernInterpretter::Apple => d.getuint32asint(n, "Kern subtable too large"),
        }
    }

    fn vertical_subtable(self, n: i32) -> bool {
        match self {
            KernInterpretter::Ms => (n & 1) == 0,
            KernInterpretter::Apple => (n & 0x8000) != 0,
        }
    }

    fn minimum_subtable(self, n: i32) -> bool {
        match self {
            KernInterpretter::Ms => (n & 2) != 0,
            KernInterpretter::Apple => false,
        }
    }

    fn override_subtable(self, n: i32) -> bool {
        match self {
            KernInterpretter::Ms => (n & 8) != 0,
            KernInterpretter::Apple => false,
        }
    }

    fn cross_stream_subtable(self, n: i32) -> bool {
        match self {
            KernInterpretter::Ms => (n & 4) != 0,
            KernInterpretter::Apple => (n & 0x4000) != 0,
        }
    }

    fn subtable_format(self, n: i32) -> i32 {
        match self {
            KernInterpretter::Ms => n >> 8,
            KernInterpretter::Apple => n & 0xFF,
        }
    }

    fn write_subtable_header(self, d: &OTByteArray, b: &mut OTByteArrayBuilder, n: i32, n2: i32, n3: i32, n4: i32) -> FontResult<()> {
        let n5 = self.get_coverage(d, n)?;
        match self {
            KernInterpretter::Ms => {
                let n5 = n5 & 0xFF | n4 << 8 & 0xFF00;
                b.setuint16(n2, d.getuint16(n)?);
                b.setuint16(n2 + 2, n3);
                b.setuint16(n2 + 4, n5);
            }
            KernInterpretter::Apple => {
                let n5 = n5 & 0xFF00 | n4 & 0xFF;
                b.setuint32(n2, n3);
                b.setuint16(n2 + 4, n5);
                b.setuint16(n2 + 6, d.getuint16(n + 6)?);
            }
        }
        Ok(())
    }

    fn write_header(self, b: &mut OTByteArrayBuilder, n: i32) {
        match self {
            KernInterpretter::Ms => {
                b.setuint16(0, 0);
                b.setuint16(2, n);
            }
            KernInterpretter::Apple => {
                b.setuint32(0, 65536);
                b.setuint32(4, n);
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct KernEntry {
    left: i32,
    right: i32,
    kern: i32,
}

#[derive(Clone, Debug)]
pub struct Kern {
    pub data: OTByteArray,
    interpretter: KernInterpretter,
}

impl Kern {
    pub fn new(font_byte_array: FontByteArray) -> FontResult<Kern> {
        let data = OTByteArray::from_font_byte_array(font_byte_array);
        let n = data.getuint16(0)?;
        let interpretter = match n {
            0 => KernInterpretter::Ms,
            1 => {
                if data.getuint16(2)? != 0 {
                    return Err(FontEngineException::unsupported_font("'kern' table version 1.x not supported yet"));
                }
                KernInterpretter::Apple
            }
            _ => return Err(FontEngineException::unsupported_font(format!("'kern' table major version = {} not supported", n))),
        };
        Ok(Kern { data, interpretter })
    }

    pub fn get_table_major_version(&self) -> FontResult<i32> {
        self.data.getuint16(0)
    }

    pub fn get_kern_vector(&self, n: i32, n2: i32) -> FontResult<[i32; 2]> {
        let mut v = [0, 0];
        let mut bl = false;
        let it = self.interpretter;
        let n3 = it.get_num_tables(&self.data)?;
        let mut n4 = it.get_offset_of_first_subtable();
        for _ in 0..n3 {
            bl = self.apply_kern_subtable(&mut v, n4, bl, n, n2)?;
            n4 += it.get_length(&self.data, n4)?;
        }
        Ok(v)
    }

    fn apply_kern_subtable(&self, v: &mut [i32; 2], n: i32, bl: bool, n2: i32, n3: i32) -> FontResult<bool> {
        let it = self.interpretter;
        let n4 = it.get_coverage(&self.data, n)?;
        let n5 = it.subtable_format(n4);
        if it.vertical_subtable(n4) {
            return Ok(bl);
        }
        match n5 {
            0 => self.apply_kern_subtable_format0(v, n, bl, n2, n3),
            2 => self.apply_kern_subtable_format2(v, n, bl, n2, n3),
            _ => Err(FontEngineException::invalid_font(format!("invalid kern subtable format ({})", n5))),
        }
    }

    fn apply_kern_value(&self, v: &mut [i32; 2], n: i32, bl: bool, value: i32) -> FontResult<bool> {
        if (value & 0xFFFF) == 32768 {
            return Ok(false);
        }
        let it = self.interpretter;
        let idx = if bl { 1 } else { 0 };
        let cov = it.get_coverage(&self.data, n)?;
        if it.minimum_subtable(cov) {
            v[idx] = v[idx].max(value);
        } else if it.override_subtable(cov) {
            v[idx] = value;
        } else {
            v[idx] += value;
        }
        Ok(bl)
    }

    fn apply_kern_subtable_format2(&self, v: &mut [i32; 2], n: i32, bl: bool, n2: i32, n3: i32) -> FontResult<bool> {
        let d = &self.data;
        let h = self.interpretter.get_subtable_header_size();
        let n5 = d.get_offset(n, h + 2)?;
        let n6 = d.get_offset(n, h + 4)?;
        let n7 = d.getuint16(n5)?;
        let n8 = d.getuint16(n5 + 2)?;
        if n2 < n7 || n2 >= n7 + n8 {
            return Ok(bl);
        }
        let n9 = d.getuint16(n6)?;
        let n10 = d.getuint16(n6 + 2)?;
        if n3 < n9 || n3 >= n9 + n10 {
            return Ok(bl);
        }
        let n11 = d.getuint16(n5 + (n2 - n7) * 2 + 4)?;
        let n4 = d.getuint16(n6 + (n3 - n9) * 2 + 4)?;
        let n12 = d.getint16(n + n11 + n4)?;
        self.apply_kern_value(v, n, bl, n12)
    }

    fn apply_kern_subtable_format0(&self, v: &mut [i32; 2], n: i32, bl: bool, n2: i32, n3: i32) -> FontResult<bool> {
        let d = &self.data;
        let h = self.interpretter.get_subtable_header_size();
        let n4 = d.getuint16(n + h)?;
        let mut n5 = 0;
        let mut n6 = n4;
        while n5 < n6 {
            let n7 = (n5 + n6) / 2;
            let n8 = d.getuint16(n + h + 8 + 6 * n7)?;
            let n9 = d.getuint16(n + h + 8 + 6 * n7 + 2)?;
            if n8 < n2 || n8 == n2 && n9 < n3 {
                n5 = n7 + 1;
                continue;
            }
            if n2 < n8 || n2 == n8 && n3 < n9 {
                n6 = n7;
                continue;
            }
            let n10 = d.getint16(n + h + 8 + 6 * n7 + 4)?;
            return self.apply_kern_value(v, n, bl, n10);
        }
        Ok(bl)
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(TAG_KERN, OTByteArray::get_ot_byte_array_builder_instance_from(&self.data));
    }

    fn write_table_data_format0_list(b: &mut OTByteArrayBuilder, n: i32, list: &mut Vec<KernEntry>) -> i32 {
        if list.is_empty() {
            return 0;
        }
        list.sort_by(|a, b| a.left.cmp(&b.left).then(a.right.cmp(&b.right)));
        let size = list.len() as i32;
        let n2 = 8 + 6 * size;
        b.ensure_capacity(n + n2);
        let n3 = Self::log2(size as f64).floor() as i32;
        let n4 = 2f64.powi(n3) as i32;
        b.setuint16(n, size);
        b.setuint16(n + 2, n4 * 6);
        b.setuint16(n + 4, n3);
        b.setuint16(n + 6, (size - n4) * 6);
        for (i, e) in list.iter().enumerate() {
            let i = i as i32;
            b.setuint16(n + 8 + 6 * i, e.left);
            b.setuint16(n + 8 + 6 * i + 2, e.right);
            b.setuint16(n + 8 + 6 * i + 4, e.kern);
        }
        n2
    }

    fn write_table_data_format0(&self, b: &mut OTByteArrayBuilder, n: i32, n2: i32, subset: &dyn Subset) -> FontResult<i32> {
        let d = &self.data;
        let n3 = d.getuint16(n)?;
        let mut list = Vec::new();
        for i in 0..n3 {
            let n5 = match d.getuint16(n + 8 + 6 * i) {
                Ok(v) => v,
                Err(e) if e.kind == ExceptionKind::InvalidFont => break,
                Err(e) => return Err(e),
            };
            if subset.get_existing_subset_gid(n5)? == -1 {
                continue;
            }
            let n4 = d.getuint16(n + 8 + 6 * i + 2)?;
            if subset.get_existing_subset_gid(n4)? == -1 {
                continue;
            }
            list.push(KernEntry { left: subset.get_existing_subset_gid(n5)?, right: subset.get_existing_subset_gid(n4)?, kern: d.getint16(n + 8 + 6 * i + 4)? });
        }
        Ok(Self::write_table_data_format0_list(b, n2, &mut list))
    }

    fn write_table_data_format2_as0(&self, b: &mut OTByteArrayBuilder, n: i32, n2: i32, subset: &dyn Subset) -> FontResult<i32> {
        let d = &self.data;
        let h = self.interpretter.get_subtable_header_size();
        let n3 = n - h + d.getuint16(n + 2)?;
        let n4 = n - h + d.getuint16(n + 4)?;
        let n5 = d.getuint16(n3)?;
        let n6 = d.getuint16(n3 + 2)?;
        let n7 = d.getuint16(n4)?;
        let n8 = d.getuint16(n4 + 2)?;
        let mut list = Vec::new();
        for i in 0..subset.get_num_glyphs() {
            let n9 = subset.get_full_gid(i);
            if n9 < n5 || n9 >= n5 + n6 {
                continue;
            }
            for j in 0..subset.get_num_glyphs() {
                let n10 = subset.get_full_gid(j);
                if n10 < n7 || n10 >= n7 + n8 {
                    continue;
                }
                let n11 = d.getuint16(n3 + (n9 - n5) * 2 + 4)?;
                let n12 = d.getuint16(n4 + (n10 - n7) * 2 + 4)?;
                let n13 = d.getint16(n - h + n11 + n12)?;
                if n13 == 0 {
                    continue;
                }
                list.push(KernEntry { left: i, right: j, kern: n13 });
            }
        }
        Ok(Self::write_table_data_format0_list(b, n2, &mut list))
    }

    fn write_format0_table_data(&self, b: &mut OTByteArrayBuilder, n: i32, n2: i32, subset: &dyn Subset, n3: i32) -> FontResult<i32> {
        match n3 {
            0 => self.write_table_data_format0(b, n, n2, subset),
            2 => self.write_table_data_format2_as0(b, n, n2, subset),
            _ => Ok(0),
        }
    }

    /// `Math.log(d) / Math.log(2.0)`.
    fn log2(d: f64) -> f64 {
        d.ln() / 2f64.ln()
    }

    pub fn subset_and_stream_for_swf(&self, subset: &dyn Subset, map: &mut BTreeMap<i32, OTByteArrayBuilder>) -> FontResult<()> {
        let it = self.interpretter;
        let mut b = OTByteArray::get_ot_byte_array_builder_instance();
        let n = it.get_num_tables(&self.data)?;
        if n > 0 {
            let mut n2 = 0;
            let mut n3 = it.get_offset_of_first_subtable();
            let mut n4 = it.get_offset_of_first_subtable();
            for _ in 0..n {
                let n5 = it.get_coverage(&self.data, n3)?;
                // Java skips these without advancing to the next subtable.
                if it.vertical_subtable(n5) || it.minimum_subtable(n5) || it.cross_stream_subtable(n5) {
                    continue;
                }
                let n6 = it.subtable_format(n5);
                let h = it.get_subtable_header_size();
                let n7 = self.write_format0_table_data(&mut b, n3 + h, n4 + h, subset, n6)?;
                if n7 > 0 {
                    it.write_subtable_header(&self.data, &mut b, n3, n4, n7, 0)?;
                    n2 += 1;
                }
                n3 += it.get_length(&self.data, n3)?;
                n4 += n7;
            }
            if n2 != 0 {
                it.write_header(&mut b, n2);
                map.insert(TAG_KERN, b);
            }
        }
        Ok(())
    }
}
