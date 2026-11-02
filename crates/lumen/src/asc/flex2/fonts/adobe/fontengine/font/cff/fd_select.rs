//! Port of afe.jar `com.adobe.fontengine.font.cff.FdSelect`.

use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::{CFFByteArray, CFFByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

#[derive(Clone, Debug)]
pub struct FdSelect {
    pub data: Rc<CFFByteArray>,
    pub offset: i32,
    pub size: i32,
}

impl FdSelect {
    pub fn new(data: Rc<CFFByteArray>, n: i32, n2: i32) -> FontResult<FdSelect> {
        let mut f = FdSelect { data, offset: n, size: 0 };
        f.size = f.size(n2)?;
        Ok(f)
    }

    fn from_builder(b: CFFByteArrayBuilder, n: i32) -> FontResult<FdSelect> {
        FdSelect::new(Rc::new(b.to_cff_byte_array()), 0, n)
    }

    pub fn single_font(n: i32) -> FontResult<FdSelect> {
        let mut b = CFFByteArray::get_cff_byte_array_builder_instance_sized(8);
        b.add_card8(3);
        b.add_card16(1);
        b.add_card16(0);
        b.add_card8(0);
        b.add_card16(n);
        FdSelect::from_builder(b, n)
    }

    fn format0_generator(fd: &FdSelect, subset: &dyn Subset) -> FontResult<FdSelect> {
        let mut b = CFFByteArray::get_cff_byte_array_builder_instance_sized(1 + subset.get_num_glyphs());
        b.add_card8(0);
        for i in 0..subset.get_num_glyphs() {
            b.add_card8(fd.component_of(subset.get_full_gid(i))?);
        }
        FdSelect::from_builder(b, subset.get_num_glyphs())
    }

    fn format3_generator(fd: &FdSelect, subset: &dyn Subset, n: i32) -> FontResult<FdSelect> {
        let mut b = CFFByteArray::get_cff_byte_array_builder_instance_sized(5 + n * 3);
        let mut n2 = fd.component_of(subset.get_full_gid(0))?;
        b.add_card8(3);
        b.add_card16(n);
        b.add_card16(0);
        for i in 1..subset.get_num_glyphs() {
            let n3 = fd.component_of(subset.get_full_gid(i))?;
            if n2 == n3 {
                continue;
            }
            b.add_card8(n2);
            b.add_card16(i);
            n2 = n3;
        }
        b.add_card8(n2);
        b.add_card16(subset.get_num_glyphs());
        FdSelect::from_builder(b, subset.get_num_glyphs())
    }

    pub fn fd_select_from_subset(fd: &FdSelect, subset: &dyn Subset) -> FontResult<FdSelect> {
        let mut n3 = fd.component_of(subset.get_full_gid(0))?;
        let mut n4 = 1;
        for n2 in 1..subset.get_num_glyphs() {
            let n = fd.component_of(subset.get_full_gid(n2))?;
            if n3 == n {
                continue;
            }
            n4 += 1;
            n3 = n;
        }
        if subset.get_num_glyphs() <= 4 + n4 * 3 {
            return FdSelect::format0_generator(fd, subset);
        }
        FdSelect::format3_generator(fd, subset, n4)
    }

    pub fn component_of(&self, n: i32) -> FontResult<i32> {
        let mut n2 = self.offset;
        let n3 = self.data.getcard8(n2)?;
        n2 += 1;
        match n3 {
            0 => self.data.getcard8(n2 + n),
            3 => {
                let n4 = self.data.getcard16(n2)?;
                n2 += 2;
                let mut n5 = 0;
                let mut n6 = n4 - 1;
                while n5 <= n6 {
                    let n7 = (n5 + n6) / 2;
                    let n8 = self.data.getcard16(n2 + 3 * n7)?;
                    let n9 = self.data.getcard16(n2 + 3 * (n7 + 1))?;
                    if n < n8 {
                        n6 = n7 - 1;
                        continue;
                    }
                    if n9 <= n {
                        n5 = n7 + 1;
                        continue;
                    }
                    return self.data.getcard8(n2 + 3 * n7 + 2);
                }
                Err(FontEngineException::invalid_font(format!("invalid fdSelect at offset {}", self.offset)))
            }
            _ => Err(FontEngineException::invalid_font(format!("invalid fdSelect format at {}", self.offset))),
        }
    }

    pub fn size(&self, n: i32) -> FontResult<i32> {
        let n2 = self.offset;
        let n3 = self.data.getcard8(n2)?;
        match n3 {
            0 => Ok(1 + n),
            3 => {
                let n4 = self.data.getcard16(n2 + 1)?;
                Ok(3 + 3 * n4 + 2)
            }
            _ => Err(FontEngineException::invalid_font(format!("invalid fdSelect format at {}", self.offset))),
        }
    }

    pub fn stream(&self, b: &mut CFFByteArrayBuilder) -> FontResult<()> {
        b.add_bytes_from(&self.data, self.offset, self.size)
    }
}
