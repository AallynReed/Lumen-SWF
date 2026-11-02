//! Port of JDK 17 `sun.font.CompositeStrike` (metrics and outlines), with `StrikeMetrics.merge`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::asc::flex2::fonts::java2d::freetype_scaler::StrikeMetrics;
use crate::asc::flex2::fonts::java2d::java::awt::geom::path2d::GeneralPath;
use crate::asc::flex2::fonts::java2d::sun::font::font2d::Font2D;
use crate::asc::flex2::fonts::java2d::sun::font::font_strike::FontStrike;
use crate::asc::flex2::fonts::java2d::sun::font::font_strike_desc::FontStrikeDesc;

const SLOTMASK: i32 = 0xffffff;

pub struct CompositeStrike {
    comp_font: Rc<Font2D>,
    pub desc: FontStrikeDesc,
    strikes: RefCell<Vec<Option<Rc<FontStrike>>>>,
    strike_metrics: RefCell<Option<StrikeMetrics>>,
}

impl CompositeStrike {
    pub fn new(font2d: Rc<Font2D>, desc: FontStrikeDesc) -> CompositeStrike {
        let n = font2d.composite().unwrap().num_slots;
        CompositeStrike { comp_font: font2d, desc, strikes: RefCell::new(vec![None; n]), strike_metrics: RefCell::new(None) }
    }

    fn get_strike_for_glyph(&self, glyph_code: i32) -> Rc<FontStrike> {
        self.get_strike_for_slot((glyph_code as u32 >> 24) as usize)
    }

    fn get_strike_for_slot(&self, slot: usize) -> Rc<FontStrike> {
        let slot = if slot >= self.strikes.borrow().len() { 0 } else { slot };
        if let Some(s) = &self.strikes.borrow()[slot] {
            return s.clone();
        }
        let s = self.comp_font.composite().unwrap().get_slot_font(&self.comp_font, slot).get_strike(&self.desc);
        self.strikes.borrow_mut()[slot] = Some(s.clone());
        s
    }

    pub fn get_font_metrics(&self) -> StrikeMetrics {
        if let Some(m) = *self.strike_metrics.borrow() {
            return m;
        }
        let mut comp_metrics = StrikeMetrics {
            ascent_x: i32::MAX as f32,
            ascent_y: i32::MAX as f32,
            descent_x: i32::MIN as f32,
            descent_y: i32::MIN as f32,
            leading_x: i32::MIN as f32,
            leading_y: i32::MIN as f32,
            baseline_x: i32::MIN as f32,
            baseline_y: i32::MIN as f32,
            max_advance_x: i32::MIN as f32,
            max_advance_y: i32::MIN as f32,
        };
        for s in 0..self.comp_font.composite().unwrap().num_metrics_slots {
            merge(&mut comp_metrics, &self.get_strike_for_slot(s).get_font_metrics());
        }
        *self.strike_metrics.borrow_mut() = Some(comp_metrics);
        comp_metrics
    }

    pub fn get_glyph_metrics(&self, glyph_code: i32) -> (f32, f32) {
        self.get_strike_for_glyph(glyph_code).get_glyph_metrics(glyph_code & SLOTMASK)
    }

    pub fn get_glyph_outline(&self, glyph_code: i32, x: f32, y: f32) -> GeneralPath {
        self.get_strike_for_glyph(glyph_code).get_glyph_outline(glyph_code & SLOTMASK, x, y)
    }
}

fn merge(m: &mut StrikeMetrics, other: &StrikeMetrics) {
    if other.ascent_x < m.ascent_x {
        m.ascent_x = other.ascent_x;
    }
    if other.ascent_y < m.ascent_y {
        m.ascent_y = other.ascent_y;
    }
    if other.descent_x > m.descent_x {
        m.descent_x = other.descent_x;
    }
    if other.descent_y > m.descent_y {
        m.descent_y = other.descent_y;
    }
    if other.baseline_x > m.baseline_x {
        m.baseline_x = other.baseline_x;
    }
    if other.baseline_y > m.baseline_y {
        m.baseline_y = other.baseline_y;
    }
    if other.leading_x > m.leading_x {
        m.leading_x = other.leading_x;
    }
    if other.leading_y > m.leading_y {
        m.leading_y = other.leading_y;
    }
    if other.max_advance_x > m.max_advance_x {
        m.max_advance_x = other.max_advance_x;
    }
    if other.max_advance_y > m.max_advance_y {
        m.max_advance_y = other.max_advance_y;
    }
}
