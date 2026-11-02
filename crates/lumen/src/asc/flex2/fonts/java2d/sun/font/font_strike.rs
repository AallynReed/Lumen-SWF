//! Port of JDK 17 `sun.font.FontStrike`; the subclasses are the enum variants.

use crate::asc::flex2::fonts::java2d::freetype_scaler::StrikeMetrics;
use crate::asc::flex2::fonts::java2d::java::awt::geom::path2d::GeneralPath;
use crate::asc::flex2::fonts::java2d::sun::font::composite_strike::CompositeStrike;
use crate::asc::flex2::fonts::java2d::sun::font::file_font_strike::FileFontStrike;

pub enum FontStrike {
    File(FileFontStrike),
    Composite(CompositeStrike),
}

impl FontStrike {
    pub fn get_font_metrics(&self) -> StrikeMetrics {
        match self {
            FontStrike::File(s) => s.get_font_metrics(),
            FontStrike::Composite(s) => s.get_font_metrics(),
        }
    }

    pub fn get_glyph_metrics(&self, glyph_code: i32) -> (f32, f32) {
        match self {
            FontStrike::File(s) => s.get_glyph_metrics(glyph_code),
            FontStrike::Composite(s) => s.get_glyph_metrics(glyph_code),
        }
    }

    pub fn get_glyph_outline(&self, glyph_code: i32, x: f32, y: f32) -> GeneralPath {
        match self {
            FontStrike::File(s) => s.get_glyph_outline(glyph_code, x, y),
            FontStrike::Composite(s) => s.get_glyph_outline(glyph_code, x, y),
        }
    }
}
