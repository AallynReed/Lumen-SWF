//! Port of JDK 17 `sun.font.CharToGlyphMapper`; the subclasses are the enum variants.

use crate::asc::flex2::fonts::java2d::sun::font::composite_glyph_mapper::CompositeGlyphMapper;
use crate::asc::flex2::fonts::java2d::sun::font::true_type_glyph_mapper::TrueTypeGlyphMapper;

pub const HI_SURROGATE_START: i32 = 0xD800;
pub const HI_SURROGATE_END: i32 = 0xDBFF;
pub const LO_SURROGATE_START: i32 = 0xDC00;
pub const LO_SURROGATE_END: i32 = 0xDFFF;
pub const UNINITIALIZED_GLYPH: i32 = -1;
pub const INVISIBLE_GLYPH_ID: i32 = 0xffff;
pub const INVISIBLE_GLYPHS: i32 = 0xfffe;

pub enum CharToGlyphMapper {
    TrueType(TrueTypeGlyphMapper),
    Composite(CompositeGlyphMapper),
}

impl CharToGlyphMapper {
    pub fn get_missing_glyph_code(&self) -> i32 {
        match self {
            CharToGlyphMapper::TrueType(m) => m.missing_glyph,
            CharToGlyphMapper::Composite(m) => m.missing_glyph,
        }
    }

    pub fn get_num_glyphs(&self) -> i32 {
        match self {
            CharToGlyphMapper::TrueType(m) => m.get_num_glyphs(),
            CharToGlyphMapper::Composite(m) => m.get_num_glyphs(),
        }
    }

    pub fn char_to_glyph(&self, unicode: i32) -> i32 {
        match self {
            CharToGlyphMapper::TrueType(m) => m.char_to_glyph(unicode),
            CharToGlyphMapper::Composite(m) => m.char_to_glyph(unicode),
        }
    }

    pub fn can_display(&self, ch: u16) -> bool {
        self.char_to_glyph(ch as i32) != self.get_missing_glyph_code()
    }

    /// `charsToGlyphs(count, char[], int[])`.
    pub fn chars_to_glyphs(&self, unicodes: &[u16], glyphs: &mut [i32]) {
        let count = unicodes.len();
        let mut i = 0;
        while i < count {
            let mut code = unicodes[i] as i32;
            if (HI_SURROGATE_START..=HI_SURROGATE_END).contains(&code) && i < count - 1 {
                let low = unicodes[i + 1] as i32;
                if (LO_SURROGATE_START..=LO_SURROGATE_END).contains(&low) {
                    code = (code - HI_SURROGATE_START) * 0x400 + low - LO_SURROGATE_START + 0x10000;
                    glyphs[i] = self.char_to_glyph(code);
                    i += 1;
                    glyphs[i] = INVISIBLE_GLYPH_ID;
                    i += 1;
                    continue;
                }
            }
            glyphs[i] = self.char_to_glyph(code);
            i += 1;
        }
    }
}
