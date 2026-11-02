//! Port of JDK 17 `sun.font.FontFamily`; the static maps are per thread, like the rest of the font manager state.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::asc::flex2::fonts::java2d::java::util::locale::Locale;
use crate::asc::flex2::fonts::java2d::sun::font::font2d::{Font2D, FONT_BOLD, FONT_ITALIC, FONT_PLAIN, FWEIGHT_BOLD, FWEIGHT_NORMAL, FWIDTH_NORMAL};

thread_local! {
    static FAMILY_NAME_MAP: RefCell<HashMap<String, Rc<FontFamily>>> = RefCell::new(HashMap::new());
    static ALL_LOCALE_NAMES: RefCell<Option<HashMap<String, Rc<FontFamily>>>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Members {
    plain: Option<Rc<Font2D>>,
    bold: Option<Rc<Font2D>>,
    italic: Option<Rc<Font2D>>,
    bolditalic: Option<Rc<Font2D>>,
}

pub struct FontFamily {
    pub family_name: String,
    m: RefCell<Members>,
    pub logical_font: bool,
    family_rank: i32,
    family_width: Cell<i32>,
}

fn same(a: &Option<Rc<Font2D>>, b: &Rc<Font2D>) -> bool {
    matches!(a, Some(x) if Rc::ptr_eq(x, b))
}

impl FontFamily {
    pub fn get_family(name: &str) -> Option<Rc<FontFamily>> {
        FAMILY_NAME_MAP.with(|m| m.borrow().get(&name.to_lowercase()).cloned())
    }

    pub fn remove(font2d: &Rc<Font2D>) {
        let name = font2d.get_family_name(Some(&Locale::english()));
        let Some(family) = Self::get_family(&name) else { return };
        let empty = {
            let mut m = family.m.borrow_mut();
            if same(&m.plain, font2d) {
                m.plain = None;
            }
            if same(&m.bold, font2d) {
                m.bold = None;
            }
            if same(&m.italic, font2d) {
                m.italic = None;
            }
            if same(&m.bolditalic, font2d) {
                m.bolditalic = None;
            }
            m.plain.is_none() && m.bold.is_none() && m.italic.is_none() && m.bolditalic.is_none()
        };
        if empty {
            FAMILY_NAME_MAP.with(|m| m.borrow_mut().remove(&name));
        }
    }

    pub fn new(name: &str, is_log_font: bool, rank: i32) -> Rc<FontFamily> {
        let f = Rc::new(FontFamily { family_name: name.to_string(), m: RefCell::new(Members::default()), logical_font: is_log_font, family_rank: rank, family_width: Cell::new(0) });
        FAMILY_NAME_MAP.with(|m| m.borrow_mut().insert(name.to_lowercase(), f.clone()));
        f
    }

    pub fn get_rank(&self) -> i32 {
        self.family_rank
    }

    fn is_from_same_source(&self, font: &Rc<Font2D>) -> bool {
        let Some(new_plat) = font.plat_name() else { return false };
        let existing = {
            let m = self.m.borrow();
            let e = [&m.plain, &m.bold, &m.italic, &m.bolditalic].into_iter().flatten().find(|f| f.is_physical()).cloned();
            e
        };
        let Some(existing) = existing else { return false };
        let dir = |p: &str| canonical_parent(p);
        dir(existing.plat_name().unwrap()) == dir(new_plat)
    }

    fn preferred_width(&self, font: &Rc<Font2D>) -> bool {
        let new_width = font.get_width();
        if self.family_width.get() == 0 {
            self.family_width.set(new_width);
            return true;
        }
        if new_width == self.family_width.get() {
            return true;
        }
        if (FWIDTH_NORMAL - new_width).abs() < (FWIDTH_NORMAL - self.family_width.get()).abs() {
            self.family_width.set(new_width);
            *self.m.borrow_mut() = Members::default();
            return true;
        }
        false
    }

    fn closer_weight(&self, curr_font: &Option<Rc<Font2D>>, font: &Rc<Font2D>, style: i32) -> bool {
        if self.family_width.get() != font.get_width() {
            return false;
        }
        let Some(curr_font) = curr_font else { return true };
        let new_weight = font.get_weight();
        match style {
            FONT_PLAIN | FONT_ITALIC => new_weight <= FWEIGHT_NORMAL && new_weight > curr_font.get_weight(),
            3 | FONT_BOLD => (new_weight - FWEIGHT_BOLD).abs() < (curr_font.get_weight() - FWEIGHT_BOLD).abs(),
            _ => false,
        }
    }

    pub fn set_font(&self, font: &Rc<Font2D>, style: i32) {
        if font.get_rank() > self.family_rank && !self.is_from_same_source(font) {
            return;
        }
        let cur = |f: fn(&Members) -> Option<Rc<Font2D>>| f(&self.m.borrow());
        match style {
            FONT_PLAIN => {
                if self.preferred_width(font) && self.closer_weight(&cur(|m| m.plain.clone()), font, style) {
                    self.m.borrow_mut().plain = Some(font.clone());
                }
            }
            FONT_BOLD => {
                if self.preferred_width(font) && self.closer_weight(&cur(|m| m.bold.clone()), font, style) {
                    self.m.borrow_mut().bold = Some(font.clone());
                }
            }
            FONT_ITALIC => {
                if self.preferred_width(font) && self.closer_weight(&cur(|m| m.italic.clone()), font, style) {
                    self.m.borrow_mut().italic = Some(font.clone());
                }
            }
            3 => {
                if self.preferred_width(font) && self.closer_weight(&cur(|m| m.bolditalic.clone()), font, style) {
                    self.m.borrow_mut().bolditalic = Some(font.clone());
                }
            }
            _ => {}
        }
    }

    pub fn get_font_with_exact_style_match(&self, style: i32) -> Option<Rc<Font2D>> {
        let m = self.m.borrow();
        match style {
            FONT_PLAIN => m.plain.clone(),
            FONT_BOLD => m.bold.clone(),
            FONT_ITALIC => m.italic.clone(),
            3 => m.bolditalic.clone(),
            _ => None,
        }
    }

    pub fn get_font(&self, style: i32) -> Option<Rc<Font2D>> {
        let m = self.m.borrow();
        let can = |f: &Option<Rc<Font2D>>| matches!(f, Some(x) if x.can_do_style(style));
        match style {
            FONT_PLAIN => m.plain.clone(),
            FONT_BOLD | FONT_ITALIC => {
                let own = if style == FONT_BOLD { &m.bold } else { &m.italic };
                if own.is_some() {
                    own.clone()
                } else if can(&m.plain) {
                    m.plain.clone()
                } else {
                    None
                }
            }
            3 => {
                if m.bolditalic.is_some() {
                    m.bolditalic.clone()
                } else if can(&m.bold) {
                    m.bold.clone()
                } else if can(&m.italic) {
                    m.italic.clone()
                } else if can(&m.plain) {
                    m.plain.clone()
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn get_closest_style(&self, style: i32) -> Option<Rc<Font2D>> {
        let m = self.m.borrow();
        let first = |a: &Option<Rc<Font2D>>, b: &Option<Rc<Font2D>>, c: &Option<Rc<Font2D>>| a.clone().or_else(|| b.clone()).or_else(|| c.clone());
        match style {
            FONT_PLAIN => first(&m.bold, &m.italic, &m.bolditalic),
            FONT_BOLD => first(&m.plain, &m.bolditalic, &m.italic),
            FONT_ITALIC => first(&m.bolditalic, &m.plain, &m.bold),
            3 => first(&m.italic, &m.bold, &m.plain),
            _ => None,
        }
    }

    pub fn add_locale_names(family: &Rc<FontFamily>, names: &[String]) {
        ALL_LOCALE_NAMES.with(|m| {
            let mut m = m.borrow_mut();
            let m = m.get_or_insert_with(HashMap::new);
            for n in names {
                m.insert(n.to_lowercase(), family.clone());
            }
        });
    }

    pub fn get_locale_family(name: &str) -> Option<Rc<FontFamily>> {
        ALL_LOCALE_NAMES.with(|m| m.borrow().as_ref().and_then(|m| m.get(&name.to_lowercase()).cloned()))
    }
}

/// `new File(p).getParentFile().getCanonicalFile()`, compared as Windows does (case-insensitively).
fn canonical_parent(p: &str) -> Option<String> {
    let parent = std::path::Path::new(p).parent()?;
    let c = std::fs::canonicalize(parent).map(|c| c.to_string_lossy().to_string()).unwrap_or_else(|_| parent.to_string_lossy().to_string());
    Some(c.trim_start_matches(r"\\?\").to_lowercase())
}
