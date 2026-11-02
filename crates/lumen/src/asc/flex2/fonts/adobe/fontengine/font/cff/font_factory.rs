//! Port of afe.jar `com.adobe.fontengine.font.cff.FontFactory`.

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_font::AnyCFFFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::font_set::FontSet;
use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub struct FontFactory;

impl FontFactory {
    pub fn load(a: FontByteArray) -> FontResult<Vec<Option<AnyCFFFont>>> {
        Ok(FontSet::new(a)?.fonts)
    }

    pub fn get_num_bytes_needed_to_identify() -> i32 {
        4
    }

    pub fn is_cff(a: &[u8]) -> bool {
        a[0] == 1 && (a[3] as i8) > 0 && a[3] <= 4
    }
}
