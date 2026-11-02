//! Port of afe.jar `com.adobe.fontengine.font.opentype.Os2` (without its `CodePage`/`UnicodeRange` tables).

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::font::embedding_permission::EmbeddingPermission;
use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::table::data_as_byte_array;
use crate::asc::flex2::fonts::adobe::fontengine::font::permission::Permission;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[derive(Clone, Debug, Default)]
pub struct Os2 {
    pub data: OTByteArray,
}

impl Os2 {
    pub fn new(a: FontByteArray) -> Os2 {
        Os2 { data: OTByteArray::from_font_byte_array(a) }
    }

    pub fn get_table_version(&self) -> FontResult<i32> {
        self.data.getuint16(0)
    }

    pub fn get_weight_class(&self) -> FontResult<i32> {
        self.data.getuint16(4)
    }

    pub fn get_width_class(&self) -> FontResult<i32> {
        self.data.getuint16(6)
    }

    pub fn panose_indicates_proportional(&self) -> FontResult<i32> {
        let n = self.data.getint8(32)?;
        let n2 = self.data.getint8(35)?;
        if n == 0 || n2 == 0 {
            return Ok(-1);
        }
        Ok(match n {
            2 | 4 => {
                if n2 == 9 {
                    0
                } else {
                    1
                }
            }
            3 | 5 => {
                if n2 == 3 {
                    0
                } else {
                    1
                }
            }
            _ => -1,
        })
    }

    pub fn get_selection(&self) -> FontResult<i32> {
        self.data.getuint16(62)
    }

    pub fn get_first_char(&self) -> FontResult<i32> {
        self.data.getuint16(64)
    }

    pub fn get_last_char(&self) -> FontResult<i32> {
        self.data.getuint16(66)
    }

    pub fn get_typo_ascender(&self) -> FontResult<i32> {
        self.data.getint16(68)
    }

    pub fn get_typo_descender(&self) -> FontResult<i32> {
        self.data.getint16(70)
    }

    pub fn get_typo_line_gap(&self) -> FontResult<i32> {
        self.data.getint16(72)
    }

    pub fn get_win_ascent(&self) -> FontResult<i32> {
        self.data.getuint16(74)
    }

    pub fn get_win_descent(&self) -> FontResult<i32> {
        self.data.getuint16(76)
    }

    pub fn get_vendor(&self) -> FontResult<i64> {
        self.data.getuint32(58)
    }

    pub fn get_x_height(&self) -> FontResult<i32> {
        if self.get_table_version()? < 2 {
            return Ok(i32::MAX);
        }
        self.data.getint16(86)
    }

    pub fn get_cap_height(&self) -> FontResult<i32> {
        if self.get_table_version()? < 2 {
            return Ok(i32::MAX);
        }
        self.data.getint16(88)
    }

    pub fn get_raw_fs_type(&self) -> FontResult<i32> {
        self.data.getuint16(8)
    }

    pub fn get_embedding_permission(&self) -> FontResult<Permission> {
        let n = self.get_raw_fs_type()?;
        Ok(EmbeddingPermission::interpret_fs_type(n))
    }

    pub fn subset_and_stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1330851634, OTByteArray::get_ot_byte_array_builder_instance_from(&self.data));
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1330851634, data_as_byte_array(&self.data));
    }
}
