//! Port of JDK 17 `sun.font.CompositeFont`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::asc::flex2::fonts::java2d::sun::font::font2d::{Font2D, FONT_BOLD, FONT_CONFIG_RANK, FONT_ITALIC, FONT_PLAIN};
use crate::asc::flex2::fonts::java2d::sun::font::sun_font_manager::{SunFontManager, PHYSICAL_FALLBACK};

pub struct CompositeFont {
    deferred_initialisation: RefCell<Vec<bool>>,
    component_file_names: Option<Vec<Option<String>>>,
    component_names: RefCell<Option<Vec<Option<String>>>>,
    components: RefCell<Vec<Option<Rc<Font2D>>>>,
    pub num_slots: usize,
    pub num_metrics_slots: usize,
    pub exclusion_ranges: Option<Vec<i32>>,
    pub max_indices: Option<Vec<i32>>,
    pub num_glyphs: Cell<i32>,
    locale_slot: Cell<i32>,
    is_std_composite: bool,
}

impl CompositeFont {
    /// `CompositeFont(name, compFileNames, compNames, metricsSlotCnt, exclRanges, maxIndexes, defer, fm)`.
    pub fn new(name: &str, comp_file_names: Option<Vec<Option<String>>>, comp_names: Option<Vec<Option<String>>>, metrics_slot_cnt: usize, excl_ranges: Option<Vec<i32>>, max_indexes: Option<Vec<i32>>, defer: bool, eudc: Option<Rc<Font2D>>) -> Rc<Font2D> {
        let mut num_slots = match &comp_names {
            None => comp_file_names.as_ref().map(|v| v.len()).unwrap_or(0),
            Some(n) => n.len(),
        };
        num_slots = num_slots.min(254);
        let num_metrics_slots = metrics_slot_cnt;
        let mut component_names = comp_names.clone();
        let mut component_file_names = comp_file_names.clone();
        let components;
        let mut deferred;
        if let Some(e) = eudc {
            let ms_cnt = num_metrics_slots;
            num_slots += 1;
            if let Some(cn) = &comp_names {
                let mut v = vec![None; num_slots];
                v[..ms_cnt].clone_from_slice(&cn[..ms_cnt]);
                v[ms_cnt] = Some(e.get_font_name(None));
                v[ms_cnt + 1..].clone_from_slice(&cn[ms_cnt..num_slots - 1]);
                component_names = Some(v);
            }
            if let Some(cf) = &comp_file_names {
                let mut v = vec![None; num_slots];
                v[..ms_cnt].clone_from_slice(&cf[..ms_cnt]);
                v[ms_cnt + 1..].clone_from_slice(&cf[ms_cnt..num_slots - 1]);
                component_file_names = Some(v);
            }
            let mut c = vec![None; num_slots];
            c[ms_cnt] = Some(e);
            components = c;
            deferred = vec![false; num_slots];
            if defer {
                for (i, d) in deferred.iter_mut().enumerate() {
                    *d = i != ms_cnt;
                }
            }
        } else {
            components = vec![None; num_slots];
            deferred = vec![defer; num_slots];
        }
        let (family_name, style) = match name.find('.') {
            Some(index) if index > 0 => {
                let style = match &name[index + 1..] {
                    "plain" => FONT_PLAIN,
                    "bold" => FONT_BOLD,
                    "italic" => FONT_ITALIC,
                    "bolditalic" => FONT_BOLD | FONT_ITALIC,
                    _ => FONT_PLAIN,
                };
                (name[..index].to_string(), style)
            }
            _ => (name.to_string(), FONT_PLAIN),
        };
        let cf = CompositeFont {
            deferred_initialisation: RefCell::new(deferred),
            component_file_names,
            component_names: RefCell::new(component_names),
            components: RefCell::new(components),
            num_slots,
            num_metrics_slots,
            exclusion_ranges: excl_ranges,
            max_indices: max_indexes,
            num_glyphs: Cell::new(0),
            locale_slot: Cell::new(-1),
            is_std_composite: true,
        };
        Font2D::new_composite(family_name, name.to_string(), style, FONT_CONFIG_RANK, cf)
    }

    /// `CompositeFont(PhysicalFont physFont, CompositeFont compFont)`.
    pub fn new_with_physical(phys_font: &Rc<Font2D>, comp_font: &Rc<Font2D>) -> Rc<Font2D> {
        let c = comp_font.composite().unwrap();
        let num_slots = c.num_slots + 1;
        let mut components = vec![Some(phys_font.clone())];
        components.extend(c.components.borrow().iter().cloned());
        let component_names = c.component_names.borrow().as_ref().map(|n| {
            let mut v = vec![Some(phys_font.full_name.clone())];
            v.extend(n.iter().cloned());
            v
        });
        let component_file_names = c.component_file_names.as_ref().map(|n| {
            let mut v = vec![None];
            v.extend(n.iter().cloned());
            v
        });
        let mut deferred = vec![false];
        deferred.extend(c.deferred_initialisation.borrow().iter().copied());
        let cf = CompositeFont {
            deferred_initialisation: RefCell::new(deferred),
            component_file_names,
            component_names: RefCell::new(component_names),
            components: RefCell::new(components),
            num_slots,
            num_metrics_slots: 1,
            exclusion_ranges: None,
            max_indices: None,
            num_glyphs: Cell::new(0),
            locale_slot: Cell::new(-1),
            is_std_composite: false,
        };
        Font2D::new_composite(phys_font.family_name.clone(), phys_font.full_name.clone(), phys_font.get_style(), crate::asc::flex2::fonts::java2d::sun::font::font2d::DEFAULT_RANK, cf)
    }

    fn do_deferred_initialisation(&self, this: &Rc<Font2D>, slot: usize) {
        if !self.deferred_initialisation.borrow()[slot] {
            return;
        }
        let fm = SunFontManager::get_instance();
        if self.component_names.borrow().is_none() {
            *self.component_names.borrow_mut() = Some(vec![None; self.num_slots]);
        }
        if self.components.borrow()[slot].is_none() {
            let mut comp = None;
            if let Some(Some(f)) = self.component_file_names.as_ref().map(|v| v[slot].clone()) {
                comp = fm.initialise_deferred_font(&f);
            }
            let comp = match comp {
                Some(c) => c,
                None => fm.get_default_physical_font(),
            };
            self.components.borrow_mut()[slot] = Some(comp.clone());
            let name = comp.get_font_name(None);
            let cn = self.component_names.borrow().as_ref().unwrap()[slot].clone();
            match cn {
                None => self.component_names.borrow_mut().as_mut().unwrap()[slot] = Some(name),
                Some(cn) => {
                    if !cn.eq_ignore_ascii_case(&name) && cn.to_lowercase() != name.to_lowercase() {
                        let f = fm.find_font2d(&cn, this.get_style(), PHYSICAL_FALLBACK);
                        let f = match f {
                            Some(f) if f.is_physical() => f,
                            _ => fm.get_default_physical_font(),
                        };
                        self.components.borrow_mut()[slot] = Some(f);
                    }
                }
            }
        }
        self.deferred_initialisation.borrow_mut()[slot] = false;
    }

    pub fn replace_component_font(&self, old_font: &Rc<Font2D>, new_font: &Rc<Font2D>) {
        for slot in 0..self.num_slots {
            let hit = matches!(&self.components.borrow()[slot], Some(c) if Rc::ptr_eq(c, old_font));
            if hit {
                self.components.borrow_mut()[slot] = Some(new_font.clone());
                if let Some(n) = self.component_names.borrow_mut().as_mut() {
                    n[slot] = Some(new_font.get_font_name(None));
                }
            }
        }
    }

    pub fn is_excluded_char(&self, slot: usize, charcode: i32) -> bool {
        let (Some(ranges), Some(max_indices)) = (&self.exclusion_ranges, &self.max_indices) else { return false };
        if slot >= self.num_metrics_slots {
            return false;
        }
        let min_index = if slot > 0 { max_indices[slot - 1] } else { 0 };
        let max_index = max_indices[slot];
        let mut cur_index = min_index;
        while max_index > cur_index {
            if charcode >= ranges[cur_index as usize] && charcode <= ranges[cur_index as usize + 1] {
                return true;
            }
            cur_index += 2;
        }
        false
    }

    pub fn get_slot_font(&self, this: &Rc<Font2D>, slot: usize) -> Rc<Font2D> {
        if self.deferred_initialisation.borrow()[slot] {
            self.do_deferred_initialisation(this, slot);
        }
        let fm = SunFontManager::get_instance();
        let font = self.components.borrow()[slot].clone();
        match font {
            Some(f) => f,
            None => {
                let name = self.component_names.borrow().as_ref().and_then(|n| n[slot].clone());
                let f = name.and_then(|n| fm.find_font2d(&n, this.get_style(), PHYSICAL_FALLBACK));
                match f {
                    Some(f) if f.is_physical() => {
                        self.components.borrow_mut()[slot] = Some(f.clone());
                        f
                    }
                    _ => fm.get_default_physical_font(),
                }
            }
        }
    }

    pub fn get_validated_glyph_code(&self, this: &Rc<Font2D>, glyph_code: i32) -> i32 {
        let slot = (glyph_code as u32 >> 24) as usize;
        if slot >= self.num_slots {
            return this.get_mapper().get_missing_glyph_code();
        }
        let slotglyph_code = glyph_code & 0xffffff;
        let slot_font = self.get_slot_font(this, slot);
        if slot_font.get_validated_glyph_code(slotglyph_code) == slot_font.get_missing_glyph_code() {
            this.get_mapper().get_missing_glyph_code()
        } else {
            glyph_code
        }
    }

    pub fn is_std_composite(&self) -> bool {
        self.is_std_composite
    }

    pub fn use_aa_for_pt_size(&self, this: &Rc<Font2D>, ptsize: i32) -> bool {
        if self.locale_slot.get() == -1 {
            let mut num_core_slots = self.num_metrics_slots;
            if num_core_slots == 1 && !self.is_std_composite() {
                num_core_slots = self.num_slots;
            }
            for slot in 0..num_core_slots {
                if self.get_slot_font(this, slot).supports_encoding(None) {
                    self.locale_slot.set(slot as i32);
                    break;
                }
            }
            if self.locale_slot.get() == -1 {
                self.locale_slot.set(0);
            }
        }
        self.get_slot_font(this, self.locale_slot.get() as usize).use_aa_for_pt_size(ptsize)
    }
}
