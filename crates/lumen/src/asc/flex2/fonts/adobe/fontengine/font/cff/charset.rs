//! Port of afe.jar `com.adobe.fontengine.font.cff.Charset`.

use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::{CFFByteArray, CFFByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

#[derive(Clone, Debug)]
pub struct Charset {
    pub data: Option<Rc<CFFByteArray>>,
    pub offset: i32,
    pub size: i32,
}

impl Charset {
    /// `new Charset(data, offset, numGlyphs)`.
    pub fn new(data: Rc<CFFByteArray>, n: i32, n2: i32) -> FontResult<Charset> {
        let mut c = Charset { data: if n < 3 { None } else { Some(data) }, offset: n, size: 0 };
        c.size = c.size(n2)?;
        Ok(c)
    }

    /// `new Charset(data)`.
    pub fn from_data(data: CFFByteArray) -> Charset {
        let size = data.get_size();
        Charset { data: Some(Rc::new(data)), offset: 0, size }
    }

    pub fn identity_charset(n: i32) -> Charset {
        let n = n - 2;
        let mut b;
        if n > 255 {
            b = CFFByteArray::get_cff_byte_array_builder_instance_sized(5);
            b.add_card8(2);
            b.add_card16(1);
            b.add_card16(n);
        } else {
            b = CFFByteArray::get_cff_byte_array_builder_instance_sized(4);
            b.add_card8(1);
            b.add_card16(1);
            b.add_card8(n);
        }
        Charset::from_data(b.to_cff_byte_array())
    }

    fn format0_generator(cs: &Charset, subset: &dyn Subset) -> FontResult<Charset> {
        let mut b = CFFByteArray::get_cff_byte_array_builder_instance_sized(1 + 2 * (subset.get_num_glyphs() - 1));
        b.add_card8(0);
        for i in 1..subset.get_num_glyphs() {
            b.add_card16(cs.gid2sid(subset.get_full_gid(i))?);
        }
        Ok(Charset::from_data(b.to_cff_byte_array()))
    }

    fn format1_generator(cs: &Charset, subset: &dyn Subset, n: i32) -> FontResult<Charset> {
        let mut b = CFFByteArray::get_cff_byte_array_builder_instance_sized(1 + 3 * n);
        b.add_card8(1);
        let mut n2 = 0;
        let mut n3 = cs.gid2sid(subset.get_full_gid(1))?;
        b.add_card16(n3);
        for i in 2..subset.get_num_glyphs() {
            let n4 = cs.gid2sid(subset.get_full_gid(i))?;
            if n2 == 255 || n4 != n3 + 1 {
                b.add_card8(n2);
                n2 = 0;
                b.add_card16(n4);
            } else {
                n2 += 1;
            }
            n3 = n4;
        }
        b.add_card8(n2);
        Ok(Charset::from_data(b.to_cff_byte_array()))
    }

    fn format2_generator(cs: &Charset, subset: &dyn Subset, n: i32) -> FontResult<Charset> {
        let mut b = CFFByteArray::get_cff_byte_array_builder_instance_sized(1 + 3 * n);
        b.add_card8(2);
        let mut n2 = 0;
        let mut n3 = cs.gid2sid(subset.get_full_gid(1))?;
        b.add_card16(n3);
        for i in 2..subset.get_num_glyphs() {
            let n4 = cs.gid2sid(subset.get_full_gid(i))?;
            if n4 != n3 + 1 {
                b.add_card16(n2);
                n2 = 0;
                b.add_card16(n4);
            } else {
                n2 += 1;
            }
            n3 = n4;
        }
        // Java ends the format 2 table with addCard8.
        b.add_card8(n2);
        Ok(Charset::from_data(b.to_cff_byte_array()))
    }

    pub fn char_set_from_subset(cs: &Charset, subset: &dyn Subset) -> FontResult<Charset> {
        let n3 = subset.get_num_glyphs();
        let n4 = 2 * n3;
        let mut n5 = 1;
        let mut n6 = 1;
        let mut n7 = 0;
        if n3 > 1 {
            let mut n2 = cs.gid2sid(subset.get_full_gid(1))?;
            for i in 2..n3 {
                let n = cs.gid2sid(subset.get_full_gid(i))?;
                if n7 == 255 || n != n2 + 1 {
                    n5 += 1;
                    n7 = 0;
                } else {
                    n7 += 1;
                }
                if n != n2 + 1 {
                    n6 += 1;
                }
                n2 = n;
            }
        }
        let n2 = 3 * n5;
        let n = 4 * n6;
        if n4 <= n2 {
            if n4 <= n {
                return Charset::format0_generator(cs, subset);
            }
            return Charset::format2_generator(cs, subset, n6);
        }
        if n2 <= n {
            return Charset::format1_generator(cs, subset, n5);
        }
        Charset::format2_generator(cs, subset, n6)
    }

    fn predefined(&self) -> FontResult<&'static [i32]> {
        match self.offset {
            0 => Ok(&ISO_ADOBE_CHARSET),
            1 => Ok(&EXPERT_CHARSET),
            2 => Ok(&EXPERT_SUBSET_CHARSET),
            _ => Err(FontEngineException::invalid_font(format!("invalid charset offset ({})", self.offset))),
        }
    }

    pub fn gid2sid(&self, n: i32) -> FontResult<i32> {
        if n == 0 {
            return Ok(0);
        }
        let data = match &self.data {
            None => return Ok(self.predefined()?[n as usize]),
            Some(d) => d,
        };
        let mut n2 = self.offset;
        let n3 = data.getcard8(n2)?;
        n2 += 1;
        match n3 {
            0 => data.getcard16(n2 + 2 * (n - 1)),
            1 | 2 => {
                let mut n4 = 1;
                while n2 < self.offset + self.size {
                    let n5 = data.getcard16(n2)?;
                    n2 += 2;
                    let n6 = if n3 == 1 { data.getcard8(n2)? } else { data.getcard16(n2)? };
                    n2 += if n3 == 1 { 1 } else { 2 };
                    if n4 <= n && n <= n4 + n6 {
                        return Ok(n5 + (n - n4));
                    }
                    n4 += n6 + 1;
                }
                Ok(-1)
            }
            _ => Err(FontEngineException::unsupported_font(format!("CFF charset in format {}", n3))),
        }
    }

    pub fn sid2gid(&self, n: i32) -> FontResult<i32> {
        if n == 0 {
            return Ok(0);
        }
        let data = match &self.data {
            None => return Ok(self.predefined()?.iter().position(|&x| x == n).map_or(-1, |p| p as i32)),
            Some(d) => d,
        };
        let mut n2 = self.offset;
        let n3 = data.getcard8(n2)?;
        n2 += 1;
        match n3 {
            0 => {
                let mut n4 = 1;
                while n2 < self.offset + self.size {
                    if data.getcard16(n2)? == n {
                        return Ok(n4);
                    }
                    n2 += 2;
                    n4 += 1;
                }
                Ok(-1)
            }
            1 | 2 => {
                let mut n5 = 1;
                while n2 < self.offset + self.size {
                    let n6 = data.getcard16(n2)?;
                    n2 += 2;
                    let n7 = if n3 == 1 { data.getcard8(n2)? } else { data.getcard16(n2)? };
                    n2 += if n3 == 1 { 1 } else { 2 };
                    if n6 <= n && n <= n6 + n7 {
                        return Ok(n5 + (n - n6));
                    }
                    n5 += n7 + 1;
                }
                Ok(-1)
            }
            _ => Err(FontEngineException::unsupported_font(format!("CFF charset in format {}", n3))),
        }
    }

    pub fn size(&self, n: i32) -> FontResult<i32> {
        if self.offset < 3 {
            return Ok(0);
        }
        let data = self.data.as_ref().expect("charset data");
        let mut n2 = self.offset;
        let n3 = data.getcard8(n2)?;
        n2 += 1;
        match n3 {
            0 => return Ok(1 + 2 * n),
            1 => {
                let mut i = 1;
                while i < n {
                    n2 += 2;
                    let n4 = data.getcard8(n2)?;
                    n2 += 1;
                    i += n4 + 1;
                }
            }
            2 => {
                let mut i = 1;
                while i < n {
                    n2 += 2;
                    let n5 = data.getcard16(n2)?;
                    n2 += 2;
                    i += n5 + 1;
                }
            }
            _ => {}
        }
        Ok(n2 - self.offset)
    }

    pub fn predefined_offset(&self) -> i32 {
        if self.data.is_none() {
            return self.offset;
        }
        -1
    }

    pub fn stream(&self, b: &mut CFFByteArrayBuilder) -> FontResult<()> {
        if let Some(d) = &self.data {
            b.add_bytes_from(d, self.offset, self.size)?;
        }
        Ok(())
    }

    pub fn stream_char_set(b: &mut CFFByteArrayBuilder, a: &[i32]) {
        let len = a.len();
        let n2 = (len as i32 - 1) * 2;
        let mut n3 = 0;
        let mut n4 = 1;
        while n4 < len {
            n3 += 4;
            if n3 >= n2 {
                break;
            }
            let mut n = n4 + 1;
            while n < len && a[n] == a[n - 1] + 1 {
                n += 1;
            }
            n4 = n;
        }
        if n3 < n2 {
            b.add_card8(2);
            n4 = 1;
            while n4 < len {
                let mut n5 = 0;
                b.add_card16(a[n4]);
                let mut n = n4 + 1;
                while n < len && a[n] == a[n - 1] + 1 {
                    n5 += 1;
                    n += 1;
                }
                b.add_card16(n5);
                n4 = n;
            }
        } else {
            b.add_card8(0);
            for &v in &a[1..] {
                b.add_card16(v);
            }
        }
    }
}

pub static ISO_ADOBE_CHARSET: [i32; 229] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
    32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
    48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63,
    64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79,
    80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95,
    96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111,
    112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127,
    128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143,
    144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159,
    160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 174, 175,
    176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191,
    192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207,
    208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223,
    224, 225, 226, 227, 228,
];

pub static EXPERT_CHARSET: [i32; 166] = [
    0, 1, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 27, 28, 13, 14,
    15, 99, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252,
    253, 254, 255, 256, 257, 258, 259, 260, 261, 262, 263, 264, 265, 266, 109, 110,
    267, 268, 269, 270, 271, 272, 273, 274, 275, 276, 277, 278, 279, 280, 281, 282,
    283, 284, 285, 286, 287, 288, 289, 290, 291, 292, 293, 294, 295, 296, 297, 298,
    299, 300, 301, 302, 303, 304, 305, 306, 307, 308, 309, 310, 311, 312, 313, 314,
    315, 316, 317, 318, 158, 155, 163, 319, 320, 321, 322, 323, 324, 325, 326, 150,
    164, 169, 327, 328, 329, 330, 331, 332, 333, 334, 335, 336, 337, 338, 339, 340,
    341, 342, 343, 344, 345, 346, 347, 348, 349, 350, 351, 352, 353, 354, 355, 356,
    357, 358, 359, 360, 361, 362, 363, 364, 365, 366, 367, 368, 369, 370, 371, 372,
    373, 374, 375, 376, 377, 378,
];

pub static EXPERT_SUBSET_CHARSET: [i32; 87] = [
    0, 1, 231, 232, 235, 236, 237, 238, 13, 14, 15, 99, 239, 240, 241, 242,
    243, 244, 245, 246, 247, 248, 27, 28, 249, 250, 251, 253, 254, 255, 256, 257,
    258, 259, 260, 261, 262, 263, 264, 265, 266, 109, 110, 267, 268, 269, 270, 272,
    300, 301, 302, 305, 314, 315, 158, 155, 163, 320, 321, 322, 323, 324, 325, 326,
    150, 164, 169, 327, 328, 329, 330, 331, 332, 333, 334, 335, 336, 337, 338, 339,
    340, 341, 342, 343, 344, 345, 346,
];
