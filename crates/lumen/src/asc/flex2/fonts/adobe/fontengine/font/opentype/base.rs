//! Port of afe.jar `com.adobe.fontengine.font.opentype.Base`.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

pub const TAG_BASE: i32 = 1111577413;
/// `Tag.script_DFLT`.
const SCRIPT_DFLT: i32 = 0x44464C54;

#[derive(Clone, Debug, Default)]
pub struct Base {
    pub data: OTByteArray,
}

impl Base {
    pub fn new(font_byte_array: FontByteArray) -> FontResult<Base> {
        let b = Base { data: OTByteArray::from_font_byte_array(font_byte_array) };
        let n = b.get_table_major_version()?;
        if n != 1 {
            return Err(FontEngineException::unsupported_font(format!("'BASE' tables with major version {}are not supported", n)));
        }
        Ok(b)
    }

    pub fn get_table_major_version(&self) -> FontResult<i32> {
        self.data.getuint16(0)
    }

    pub fn get_table_minor_version(&self) -> FontResult<i32> {
        self.data.getuint16(2)
    }

    /// `getBaselinePosition(orientation, script, baseline)`; `horizontal` is `orientation == Orientation.HORIZONTAL`.
    pub fn get_baseline_position(&self, horizontal: bool, n: i32, n2: i32) -> FontResult<i32> {
        let d = &self.data;
        let n5 = d.get_offset(0, if horizontal { 4 } else { 6 })?;
        if n5 == 0 {
            return Ok(i32::MAX);
        }
        let n6 = d.get_offset(n5, 0)?;
        if n6 == 0 {
            return Ok(i32::MAX);
        }
        let mut n7 = -1;
        let n8 = d.getuint16(n6)?;
        for n4 in 0..n8 {
            if d.getuint32(n6 + 2 + 4 * n4)? != n2 as i64 {
                continue;
            }
            n7 = n4;
            break;
        }
        if n7 == -1 {
            return Ok(i32::MAX);
        }
        let n4 = d.get_offset(n5, 2)?;
        let n9 = d.getuint16(n4)?;
        let mut n10 = -1;
        for n3 in 0..n9 {
            if SCRIPT_DFLT == n || d.getuint32(n4 + 2 + 6 * n3)? == n as i64 {
                n10 = n3;
                break;
            }
            if n10 != -1 || d.getuint32(n4 + 2 + 6 * n3)? != SCRIPT_DFLT as i64 {
                continue;
            }
            n10 = n3;
            break;
        }
        if n10 == -1 {
            return Ok(i32::MAX);
        }
        let n3 = d.get_offset(n4, 2 + 6 * n10 + 4)?;
        let n11 = d.get_offset(n3, 0)?;
        if n11 == 0 {
            return Ok(i32::MAX);
        }
        let n12 = d.get_offset(n11, 4 + 2 * n7)?;
        d.getint16(n12 + 2)
    }

    fn base_coord_needs_regenerated(&self, n: i32) -> FontResult<bool> {
        Ok(n != 0 && self.data.getuint16(n)? == 2)
    }

    fn min_max_needs_regenerated(&self, n: i32) -> FontResult<bool> {
        if n == 0 {
            return Ok(false);
        }
        if self.base_coord_needs_regenerated(self.data.get_offset(n, 0)?)? {
            return Ok(true);
        }
        if self.base_coord_needs_regenerated(self.data.get_offset(n, 2)?)? {
            return Ok(true);
        }
        let n2 = self.data.getuint16(n + 4)?;
        for i in 0..n2 {
            if self.base_coord_needs_regenerated(self.data.get_offset(n, 6 + 8 * i + 4)?)? {
                return Ok(true);
            }
            if !self.base_coord_needs_regenerated(self.data.get_offset(n, 6 + 8 * i + 6)?)? {
                continue;
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn axis_needs_regenerated(&self, n: i32) -> FontResult<bool> {
        if n == 0 {
            return Ok(false);
        }
        let d = &self.data;
        let n2 = d.get_offset(n, 2)?;
        let n3 = d.getuint16(n2)?;
        for i in 0..n3 {
            let n7 = d.get_offset(n2, 2 + 6 * i + 4)?;
            let n8 = d.get_offset(n7, 0)?;
            if n8 != 0 {
                let n6 = d.getuint16(n8 + 2)?;
                for n5 in 0..n6 {
                    let n4 = d.get_offset(n8, 4 + 2 * n5)?;
                    if !self.base_coord_needs_regenerated(n4)? {
                        continue;
                    }
                    return Ok(true);
                }
            }
            let n6 = d.get_offset(n7, 2)?;
            if self.min_max_needs_regenerated(n6)? {
                return Ok(true);
            }
            let n5 = d.getuint16(n7 + 4)?;
            for n4 in 0..n5 {
                let n9 = d.get_offset(n7, 6 + 6 * n4 + 4)?;
                if !self.min_max_needs_regenerated(n9)? {
                    continue;
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn base_needs_regenerated(&self) -> FontResult<bool> {
        Ok(self.axis_needs_regenerated(self.data.get_offset(0, 4)?)? || self.axis_needs_regenerated(self.data.get_offset(0, 6)?)?)
    }

    fn generate_base_coord(&self, b: &mut OTByteArrayBuilder, n: i32, n2: i32) -> FontResult<i32> {
        b.ensure_capacity(n2 + 4);
        b.setuint16(n2, 1);
        b.setint16(n2 + 2, self.data.getint16(n + 2)?);
        Ok(4)
    }

    fn generate_min_max(&self, b: &mut OTByteArrayBuilder, n: i32, n2: i32) -> FontResult<i32> {
        let d = &self.data;
        let n3 = d.getuint16(n + 4)?;
        let mut n4 = 6 + 8 * n3;
        b.ensure_capacity(n2 + n4);
        let n5 = d.get_offset(n, 0)?;
        if n5 != 0 {
            b.setuint16(n2, n4);
            n4 += self.generate_base_coord(b, n5, n2 + n4)?;
        } else {
            b.setuint16(n2, 0);
        }
        let n6 = d.get_offset(n, 2)?;
        if n6 != 0 {
            b.setuint16(n2 + 2, n4);
            n4 += self.generate_base_coord(b, n6, n2 + n4)?;
        } else {
            b.setuint16(n2 + 2, 0);
        }
        for i in 0..n3 {
            for j in 0..4 {
                b.setuint8(n2 + 6 + 8 * i + j, d.getuint8(n + 6 + i * 8 + j)?);
            }
            let n5 = d.get_offset(n, 6 + 8 * i + 4)?;
            if n5 != 0 {
                b.setuint16(n2 + 6 + 8 * i + 4, n4);
                n4 += self.generate_base_coord(b, n5, n2 + n4)?;
            } else {
                b.setuint16(n2 + 6 + 8 * i + 4, 0);
            }
            let n6 = d.get_offset(n, 6 + 8 * i + 6)?;
            if n6 != 0 {
                b.setuint16(n2 + 6 + 8 * i + 6, n4);
                n4 += self.generate_base_coord(b, n6, n2 + n4)?;
                continue;
            }
            b.setuint16(n2 + 6 + 8 * i + 6, 0);
        }
        Ok(n4)
    }

    fn generate_base_values(&self, b: &mut OTByteArrayBuilder, n: i32, n2: i32) -> FontResult<i32> {
        let d = &self.data;
        let n3 = d.getuint16(n + 2)?;
        let mut n4 = 4 + 2 * n3;
        b.ensure_capacity(n2 + n4);
        b.setuint16(n2, d.getuint16(n)?);
        b.setuint16(n2 + 2, n3);
        for i in 0..n3 {
            b.setuint16(n2 + 4 + 2 * i, n4);
            n4 += self.generate_base_coord(b, d.get_offset(n, 4 + 2 * i)?, n2 + n4)?;
        }
        Ok(n4)
    }

    fn generate_base_script(&self, b: &mut OTByteArrayBuilder, n: i32, n2: i32) -> FontResult<i32> {
        let d = &self.data;
        let n3 = d.getuint16(n2 + 4)?;
        let mut n4 = 6 + 6 * n3;
        b.ensure_capacity(n + n4);
        let n5 = d.get_offset(n2, 0)?;
        if n5 != 0 {
            b.setuint16(n, n4);
            n4 += self.generate_base_values(b, n5, n + n4)?;
        } else {
            b.setuint16(n, 0);
        }
        let n6 = d.get_offset(n2, 2)?;
        if n6 != 0 {
            b.setuint16(n + 2, n4);
            n4 += self.generate_min_max(b, n6, n + n4)?;
        } else {
            b.setuint16(n + 2, 0);
        }
        b.setuint16(n + 4, n3);
        for i in 0..n3 {
            for j in 0..4 {
                b.setuint8(n + 6 + 6 * i + j, d.getuint8(n2 + 6 + i * 6 + j)?);
            }
            b.setuint16(n + 6 + 6 * i + 4, n4);
            n4 += self.generate_min_max(b, d.get_offset(n2, 6 + 6 * i + 4)?, n + n4)?;
        }
        Ok(n4)
    }

    fn generate_axis(&self, b: &mut OTByteArrayBuilder, n: i32, n2: i32) -> FontResult<i32> {
        let d = &self.data;
        let n6 = d.get_offset(n, 0)?;
        let n7 = d.get_offset(n, 2)?;
        let mut n8 = 4;
        b.ensure_capacity(n2 + n8);
        if n6 != 0 {
            b.setuint16(n2, 4);
            let n5 = d.getuint16(n6)?;
            let n4 = 2 + n5 * 4;
            n8 += n4;
            b.ensure_capacity(n2 + n8);
            for n3 in 0..n4 {
                b.setuint8(n2 + 4 + n3, d.getuint8(n6 + n3)?);
            }
        } else {
            b.setuint16(n2, 0);
        }
        if n7 != 0 {
            let n5 = d.getuint16(n7)?;
            let n4 = n2 + n8;
            let mut n3 = 0;
            let mut n9 = 0;
            b.ensure_capacity(n4 + n5 * 6 + 2);
            n8 += n5 * 6 + 2;
            b.setuint16(n2 + 2, n4 - n2);
            b.setuint16(n4, n5);
            for i in 0..n5 {
                for n11 in 0..4 {
                    b.setuint8(n4 + 2 + i * 6 + n11, d.getuint8(n7 + 2 + i * 6 + n11)?);
                }
                let n12 = d.get_offset(n7, 2 + i * 6 + 4)?;
                let mut n11 = 0;
                while n11 < i && d.get_offset(n7, 2 + n11 * 6 + 4)? != n12 {
                    n11 += 1;
                }
                if n11 == i {
                    b.setuint16(n4 + 2 + i * 6 + 4, 2 + n5 * 6 + n9 + n3);
                    n9 += n3;
                    n3 = self.generate_base_script(b, n2 + n8, d.get_offset(n7, 2 + i * 6 + 4)?)?;
                    n8 += n3;
                    continue;
                }
                let v = b.getuint16(n4 + 2 + n11 * 6 + 4);
                b.setuint16(n4 + 2 + i * 6 + 4, v);
            }
        } else {
            b.setuint16(n2 + 2, 0);
        }
        Ok(n8)
    }

    pub fn regenerate_base_table(&self) -> FontResult<OTByteArrayBuilder> {
        let mut b = OTByteArray::get_ot_byte_array_builder_instance();
        b.ensure_capacity(8);
        b.setuint32(0, 65536);
        let n2 = self.data.get_offset(0, 4)?;
        let n3 = if n2 == 0 { 0 } else { 8 };
        b.setuint16(4, n3);
        let mut n4 = 0;
        if n3 != 0 {
            n4 = self.generate_axis(&mut b, n2, n3)?;
        }
        let n = self.data.get_offset(0, 6)?;
        if n == 0 {
            b.setuint16(6, 0);
        } else {
            b.setuint16(6, n4 + 8);
            self.generate_axis(&mut b, n, n4 + 8)?;
        }
        Ok(b)
    }

    pub fn subset_and_stream(&self, _subset: &dyn Subset, map: &mut BTreeMap<i32, OTByteArrayBuilder>) -> FontResult<()> {
        let b = if self.base_needs_regenerated()? { self.regenerate_base_table()? } else { OTByteArray::get_ot_byte_array_builder_instance_from(&self.data) };
        map.insert(TAG_BASE, b);
        Ok(())
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(TAG_BASE, OTByteArray::get_ot_byte_array_builder_instance_from(&self.data));
    }
}
