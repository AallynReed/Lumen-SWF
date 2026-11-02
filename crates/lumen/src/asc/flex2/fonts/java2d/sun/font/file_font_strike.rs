//! Port of JDK 17 `sun.font.FileFontStrike` (the metrics and outline paths; glyph images are not needed), with the
//! `FileFont` scaler calls it makes.

use std::cell::RefCell;
use std::rc::Rc;

use crate::asc::flex2::fonts::java2d::freetype_scaler::{FreetypeFontScaler, ScalerContext, StrikeMetrics, INVISIBLE_GLYPHS};
use crate::asc::flex2::fonts::java2d::java::awt::geom::path2d::GeneralPath;
use crate::asc::flex2::fonts::java2d::sun::font::font2d::{Font2D, FONT_BOLD, FONT_ITALIC};
use crate::asc::flex2::fonts::java2d::sun::font::font_strike_desc::FontStrikeDesc;
use crate::asc::flex2::fonts::java2d::sun::font::sun_font_manager::SunFontManager;
use crate::asc::flex2::fonts::java2d::sun::font::true_type_font::FontScaler;

pub struct FileFontStrike {
    file_font: Rc<Font2D>,
    pub desc: FontStrikeDesc,
    pub algo_style: bool,
    pub boldness: f32,
    pub italic: f32,
    p_scaler_context: ScalerContext,
    strike_metrics: RefCell<Option<StrikeMetrics>>,
}

impl FileFontStrike {
    pub fn new(file_font: Rc<Font2D>, desc: FontStrikeDesc) -> FileFontStrike {
        let (mut algo_style, mut boldness, mut italic) = (false, 1.0f32, 0.0f32);
        let fstyle = file_font.get_style();
        if desc.style != fstyle {
            if (desc.style & FONT_ITALIC) == FONT_ITALIC && (fstyle & FONT_ITALIC) == 0 {
                algo_style = true;
                italic = 0.7;
            }
            if (desc.style & FONT_BOLD) == FONT_BOLD && (fstyle & FONT_BOLD) == 0 {
                algo_style = true;
                boldness = 1.33;
            }
        }
        let at = desc.glyph_tx;
        let matrix = [at[0], at[1], at[2], at[3]];
        let ctx = if matrix.iter().any(|m| m.is_nan()) {
            ScalerContext::null()
        } else {
            let scaler = get_scaler(&file_font);
            let c = scaler.borrow().create_scaler_context(matrix, desc.aa_hint, desc.fm_hint, boldness, italic);
            c.unwrap_or_else(ScalerContext::null)
        };
        FileFontStrike { file_font, desc, algo_style, boldness, italic, p_scaler_context: ctx, strike_metrics: RefCell::new(None) }
    }

    pub fn get_font_metrics(&self) -> StrikeMetrics {
        if let Some(m) = *self.strike_metrics.borrow() {
            return m;
        }
        let m = get_scaler(&self.file_font).borrow_mut().get_font_metrics(&self.p_scaler_context);
        *self.strike_metrics.borrow_mut() = Some(m);
        m
    }

    /// `getGlyphMetrics(glyphCode)`: the advance as a point.
    pub fn get_glyph_metrics(&self, glyph_code: i32) -> (f32, f32) {
        if glyph_code >= INVISIBLE_GLYPHS {
            return (0.0, 0.0);
        }
        get_scaler(&self.file_font).borrow_mut().get_glyph_metrics(&self.p_scaler_context, glyph_code)
    }

    pub fn get_glyph_outline(&self, glyph_code: i32, x: f32, y: f32) -> GeneralPath {
        let mut gp = get_scaler(&self.file_font).borrow_mut().get_glyph_outline(&self.p_scaler_context, glyph_code, 0.0, 0.0);
        if x != 0.0 || y != 0.0 {
            gp.translate(x, y);
        }
        gp
    }
}

impl FontScaler {
    pub fn create_scaler_context(&self, matrix: [f64; 4], aa: i32, fm: i32, boldness: f32, italic: f32) -> Option<ScalerContext> {
        match self {
            FontScaler::Freetype(s) => s.create_scaler_context(matrix, aa, fm, boldness, italic),
            FontScaler::Null => Some(ScalerContext::null()),
        }
    }

    pub fn get_font_metrics(&mut self, ctx: &ScalerContext) -> StrikeMetrics {
        match self {
            FontScaler::Freetype(s) => s.get_font_metrics(ctx),
            FontScaler::Null => StrikeMetrics::default(),
        }
    }

    pub fn get_glyph_metrics(&mut self, ctx: &ScalerContext, glyph_code: i32) -> (f32, f32) {
        match self {
            FontScaler::Freetype(s) => s.get_glyph_metrics(ctx, glyph_code),
            FontScaler::Null => (0.0, 0.0),
        }
    }

    pub fn get_glyph_outline(&mut self, ctx: &ScalerContext, glyph_code: i32, x: f32, y: f32) -> GeneralPath {
        match self {
            FontScaler::Freetype(s) => GeneralPath::from_data(s.get_glyph_outline(ctx, glyph_code, x, y)),
            FontScaler::Null => GeneralPath::empty(),
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(self, FontScaler::Freetype(s) if s.is_valid())
    }
}

/// `TrueTypeFont.getScaler()` (`FontScaler.getScaler`): a failed scaler becomes the null scaler and deregisters the font,
/// except for CFF-flavoured fonts, which FreeType reads but this port's scaler does not.
pub fn get_scaler(font: &Rc<Font2D>) -> Rc<RefCell<FontScaler>> {
    let ttf = font.ttf().unwrap();
    if let Some(s) = ttf.scaler.borrow().as_ref() {
        return s.clone();
    }
    let created = ttf.file_bytes().and_then(|d| FreetypeFontScaler::new(d, ttf.font_index));
    let fm = SunFontManager::get_instance();
    let s = match created {
        Some(s) => Rc::new(RefCell::new(FontScaler::Freetype(s))),
        None => {
            let s = Rc::new(RefCell::new(FontScaler::Null));
            *ttf.scaler.borrow_mut() = Some(s.clone());
            if !ttf.is_cff() {
                fm.de_register_bad_font(font);
            }
            return s;
        }
    };
    *ttf.scaler.borrow_mut() = Some(s.clone());
    fm.add_to_pool(font);
    s
}
