//! Port of JDK 17 `sun.font.StandardGlyphVector` (with its `GlyphStrike`) for a glyph vector made from chars with a
//! `FontRenderContext(null, aa, fm)` and an untransformed font: glyph outlines and glyph metrics advances.

use std::rc::Rc;

use crate::asc::flex2::fonts::java2d::freetype_scaler::JAVA_WIND_NON_ZERO;
use crate::asc::flex2::fonts::java2d::java::awt::font::Font;
use crate::asc::flex2::fonts::java2d::java::awt::geom::path2d::GeneralPath;
use crate::asc::flex2::fonts::java2d::sun::font::font2d::Font2D;
use crate::asc::flex2::fonts::java2d::sun::font::font_strike::FontStrike;
use crate::asc::flex2::fonts::java2d::sun::font::font_strike_desc::{FontStrikeDesc, IDENTITY};

pub struct StandardGlyphVector {
    glyphs: Vec<i32>,
    positions: Option<Vec<f32>>,
    strike: Rc<FontStrike>,
}

impl StandardGlyphVector {
    /// `new StandardGlyphVector(font, text, frc)`; `aa`/`fm` are the context's `SunHints` values.
    pub fn new(font: &Font, text: &[u16], aa: i32, fm: i32) -> StandardGlyphVector {
        let font2d = font.get_font2d();
        let mut glyphs = vec![0i32; text.len()];
        font2d.get_mapper().chars_to_glyphs(text, &mut glyphs);
        let strike = Self::create_strike(font, &font2d, aa, fm);
        StandardGlyphVector { glyphs, positions: None, strike }
    }

    /// `GlyphStrike.create(sgv, dtx, null)` with an identity device transform.
    fn create_strike(font: &Font, font2d: &Rc<Font2D>, aa: i32, fm: i32) -> Rc<FontStrike> {
        let s = font.get_size2d() as f64;
        let ftx = [s, 0.0, 0.0, s, 0.0, 0.0];
        let aa = FontStrikeDesc::get_aa_hint_int_val(aa);
        let fm = FontStrikeDesc::get_fm_hint_int_val(fm);
        let desc = FontStrikeDesc::new(IDENTITY, ftx, font.get_style(), aa, fm);
        font2d.handle().get().get_strike(&desc)
    }

    fn init_positions(&mut self) {
        if self.positions.is_some() {
            return;
        }
        let mut positions = vec![0f32; self.glyphs.len() * 2 + 2];
        let (mut x, mut y) = (0f32, 0f32);
        for (i, &g) in self.glyphs.iter().enumerate() {
            let adv = self.strike.get_glyph_metrics(g);
            x += adv.0 + 0.0;
            y += adv.1 + 0.0;
            positions[2 * i + 2] = x;
            positions[2 * i + 3] = y;
        }
        self.positions = Some(positions);
    }

    pub fn get_num_glyphs(&self) -> usize {
        self.glyphs.len()
    }

    pub fn get_glyph_code(&self, ix: usize) -> i32 {
        self.glyphs[ix]
    }

    /// `getGlyphOutline(ix)`: the strike's outline appended to a new `GeneralPath(WIND_NON_ZERO)`.
    pub fn get_glyph_outline(&mut self, ix: usize) -> GeneralPath {
        self.init_positions();
        let p = self.positions.as_ref().unwrap();
        let (px, py) = (0.0 + p[ix * 2], 0.0 + p[ix * 2 + 1]);
        let mut result = GeneralPath::new(JAVA_WIND_NON_ZERO);
        let gp = self.strike.get_glyph_outline(self.glyphs[ix], px, py);
        result.append(&gp, false);
        result
    }

    /// `getGlyphMetrics(ix).getAdvance()`.
    pub fn get_glyph_metrics_advance(&mut self, ix: usize) -> f32 {
        self.strike.get_glyph_metrics(self.glyphs[ix]).0
    }
}
