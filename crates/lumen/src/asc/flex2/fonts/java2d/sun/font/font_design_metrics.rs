//! Port of JDK 17 `sun.font.FontDesignMetrics` (ascent and descent) for an identity-transform `FontRenderContext`,
//! with its static `metricsCache`: keyed by `Font.equals` (name, style and size, not the `Font2D`), so a later font that
//! is equal to an earlier one gets the earlier one's metrics.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::asc::flex2::fonts::java2d::java::awt::font::Font;
use crate::asc::flex2::fonts::java2d::sun::font::font_strike_desc::{FontStrikeDesc, IDENTITY};

const ROUNDING_UP_VALUE: f32 = 0.95;

type MetricsKey = (String, i32, i32, u32, i32, i32);

thread_local! {
    static METRICS_CACHE: RefCell<HashMap<MetricsKey, FontDesignMetrics>> = RefCell::new(HashMap::new());
}

#[derive(Clone, Copy)]
pub struct FontDesignMetrics {
    ascent: f32,
    descent: f32,
    leading: f32,
    max_advance: f32,
}

impl FontDesignMetrics {
    /// `getMetrics(font, frc)` (`initMatrixAndMetrics` through `Font2D.getStrike(font, frc)`); `aa`/`fm` are the
    /// context's `SunHints` values.
    pub fn get_metrics(font: &Font, aa: i32, fm: i32) -> FontDesignMetrics {
        let key = (font.get_name(), font.get_style(), font.get_size(), font.get_size2d().to_bits(), aa, fm);
        if let Some(m) = METRICS_CACHE.with(|c| c.borrow().get(&key).copied()) {
            return m;
        }
        let m = Self::new(font, aa, fm);
        METRICS_CACHE.with(|c| c.borrow_mut().insert(key, m));
        m
    }

    fn new(font: &Font, aa: i32, fm: i32) -> FontDesignMetrics {
        let font2d = font.get_font2d();
        let pt_size = font.get_size2d() as f64;
        let at = [pt_size, 0.0, 0.0, pt_size, 0.0, 0.0];
        let desc = FontStrikeDesc::new(IDENTITY, at, font.get_style(), FontStrikeDesc::get_aa_hint_int_val(aa), FontStrikeDesc::get_fm_hint_int_val(fm));
        let m = font2d.get_strike(&desc).get_font_metrics();
        FontDesignMetrics { ascent: -m.ascent_y, descent: m.descent_y, leading: m.leading_y, max_advance: m.max_advance_x }
    }

    pub fn get_ascent(&self) -> i32 {
        (ROUNDING_UP_VALUE + self.ascent) as i32
    }

    pub fn get_descent(&self) -> i32 {
        (ROUNDING_UP_VALUE + self.descent) as i32
    }

    pub fn get_leading(&self) -> f32 {
        self.leading
    }

    pub fn get_max_advance(&self) -> f32 {
        self.max_advance
    }
}
