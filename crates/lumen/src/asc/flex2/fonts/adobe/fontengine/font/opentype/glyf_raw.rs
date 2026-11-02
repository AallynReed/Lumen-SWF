//! Port of afe.jar `com.adobe.fontengine.font.opentype.GlyfRaw`.

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::OTByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::rect::Rect;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[derive(Clone, Debug, Default)]
pub struct GlyfRaw {
    pub data: OTByteArray,
}

impl GlyfRaw {
    pub const ARG_1_AND_2_ARE_WORDS: i32 = 1;
    pub const ARGS_ARE_XY_VALUES_WORDS: i32 = 2;
    pub const ROUND_XY_TO_GRID: i32 = 4;
    pub const WE_HAVE_A_SCALE: i32 = 8;
    pub const MORE_COMPONENTS: i32 = 32;
    pub const WE_HAVE_AN_X_AND_Y_SCALE: i32 = 64;
    pub const WE_HAVE_A_TWO_BY_TWO: i32 = 128;
    pub const WE_HAVE_INSTRUCTIONS: i32 = 256;
    pub const USE_MY_METRICS: i32 = 512;
    pub const OVERLAP_COMPOUND: i32 = 1024;
    pub const SCALED_COMPONENT_OFFSET: i32 = 2048;
    pub const UNSCALED_COMPONENT_OFFSET: i32 = 4096;

    pub fn new(a: FontByteArray) -> GlyfRaw {
        GlyfRaw { data: OTByteArray::from_font_byte_array(a) }
    }

    pub fn get_glyph_bounding_box(&self, n: i32, n2: i32) -> FontResult<Rect> {
        if n == n2 {
            return Ok(Rect::EMPTY_RECT);
        }
        Ok(Rect::new(self.data.getint16(n + 2)? as f64, self.data.getint16(n + 4)? as f64, self.data.getint16(n + 6)? as f64, self.data.getint16(n + 8)? as f64))
    }

    pub fn is_composite(&self, n: i32, n2: i32) -> FontResult<bool> {
        if n == n2 {
            return Ok(false);
        }
        Ok(self.data.getint16(n)? == -1)
    }
}
