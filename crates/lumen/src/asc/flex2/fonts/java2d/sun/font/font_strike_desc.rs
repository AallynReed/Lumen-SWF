//! Port of JDK 17 `sun.font.FontStrikeDesc` (with the `SunHints` integer values it uses).

pub const INTVAL_TEXT_ANTIALIAS_DEFAULT: i32 = 0;
pub const INTVAL_TEXT_ANTIALIAS_OFF: i32 = 1;
pub const INTVAL_TEXT_ANTIALIAS_ON: i32 = 2;
pub const INTVAL_TEXT_ANTIALIAS_GASP: i32 = 3;
pub const INTVAL_TEXT_ANTIALIAS_LCD_HRGB: i32 = 4;
pub const INTVAL_TEXT_ANTIALIAS_LCD_HBGR: i32 = 5;
pub const INTVAL_TEXT_ANTIALIAS_LCD_VRGB: i32 = 6;
pub const INTVAL_TEXT_ANTIALIAS_LCD_VBGR: i32 = 7;
pub const INTVAL_FRACTIONALMETRICS_DEFAULT: i32 = 0;
pub const INTVAL_FRACTIONALMETRICS_OFF: i32 = 1;
pub const INTVAL_FRACTIONALMETRICS_ON: i32 = 2;

const AA_ON: i32 = 0x0010;
const AA_LCD_H: i32 = 0x0020;
const AA_LCD_V: i32 = 0x0040;
const FRAC_METRICS_ON: i32 = 0x0100;

/// An `AffineTransform` as `{m00, m10, m01, m11, m02, m12}`.
pub type Matrix = [f64; 6];
pub const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

#[derive(Clone, Debug)]
pub struct FontStrikeDesc {
    pub dev_tx: Matrix,
    pub glyph_tx: Matrix,
    pub style: i32,
    pub aa_hint: i32,
    pub fm_hint: i32,
    valuemask: i32,
}

impl PartialEq for FontStrikeDesc {
    fn eq(&self, desc: &FontStrikeDesc) -> bool {
        desc.valuemask == self.valuemask && desc.glyph_tx == self.glyph_tx && desc.dev_tx == self.dev_tx
    }
}

impl FontStrikeDesc {
    /// `getAAHintIntVal` for a `FontRenderContext` hint that is not GASP (all this port is given).
    pub fn get_aa_hint_int_val(aa: i32) -> i32 {
        match aa {
            INTVAL_TEXT_ANTIALIAS_ON => INTVAL_TEXT_ANTIALIAS_ON,
            INTVAL_TEXT_ANTIALIAS_LCD_HRGB | INTVAL_TEXT_ANTIALIAS_LCD_HBGR => INTVAL_TEXT_ANTIALIAS_LCD_HRGB,
            INTVAL_TEXT_ANTIALIAS_LCD_VRGB | INTVAL_TEXT_ANTIALIAS_LCD_VBGR => INTVAL_TEXT_ANTIALIAS_LCD_VRGB,
            _ => INTVAL_TEXT_ANTIALIAS_OFF,
        }
    }

    pub fn get_fm_hint_int_val(fm: i32) -> i32 {
        if fm == INTVAL_FRACTIONALMETRICS_OFF || fm == INTVAL_FRACTIONALMETRICS_DEFAULT {
            INTVAL_FRACTIONALMETRICS_OFF
        } else {
            INTVAL_FRACTIONALMETRICS_ON
        }
    }

    pub fn new(dev_at: Matrix, at: Matrix, f_style: i32, aa: i32, fm: i32) -> FontStrikeDesc {
        let mut valuemask = f_style;
        match aa {
            INTVAL_TEXT_ANTIALIAS_ON => valuemask |= AA_ON,
            INTVAL_TEXT_ANTIALIAS_LCD_HRGB | INTVAL_TEXT_ANTIALIAS_LCD_HBGR => valuemask |= AA_LCD_H,
            INTVAL_TEXT_ANTIALIAS_LCD_VRGB | INTVAL_TEXT_ANTIALIAS_LCD_VBGR => valuemask |= AA_LCD_V,
            _ => {}
        }
        if fm == INTVAL_FRACTIONALMETRICS_ON {
            valuemask |= FRAC_METRICS_ON;
        }
        FontStrikeDesc { dev_tx: dev_at, glyph_tx: at, style: f_style, aa_hint: aa, fm_hint: fm, valuemask }
    }
}
