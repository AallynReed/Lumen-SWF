//! Port of JDK 17 `sun.font.SunFontManager`, as the Windows `sun.awt.Win32FontManager` instance runs it (the Windows
//! overrides live in `sun::awt::win32_font_manager`). One instance per thread stands for the JVM-wide singleton; the
//! `ConcurrentHashMap`s whose iteration order matters keep insertion order here.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::{Rc, Weak};

use indexmap::IndexMap;

use crate::asc::flex2::fonts::java2d::java::util::locale::Locale;
use crate::asc::flex2::fonts::java2d::sun::awt::font_configuration::FontConfiguration;
use crate::asc::flex2::fonts::java2d::sun::awt::win32_font_manager as win32;
use crate::asc::flex2::fonts::java2d::sun::font::composite_font::CompositeFont;
use crate::asc::flex2::fonts::java2d::sun::font::font2d::{Font2D, Font2DHandle, FONT_CONFIG_RANK, FONT_PLAIN, TTF_RANK, TYPE1_RANK, UNKNOWN_RANK};
use crate::asc::flex2::fonts::java2d::sun::font::font_family::FontFamily;
use crate::asc::flex2::fonts::java2d::sun::font::true_type_font::{FontFormatException, TrueTypeFont};
use crate::asc::util::jhash::{JHashMap, JHashSet};

pub const NO_FALLBACK: i32 = 0;
pub const PHYSICAL_FALLBACK: i32 = 1;
pub const LOGICAL_FALLBACK: i32 = 2;

pub const FONTFORMAT_NONE: i32 = -1;
pub const FONTFORMAT_TRUETYPE: i32 = 0;
pub const FONTFORMAT_TYPE1: i32 = 1;
pub const FONTFORMAT_NATIVE: i32 = 4;

const CHANNELPOOLSIZE: usize = 20;
pub const SEPARATOR: &str = "\\";
pub const PATH_SEPARATOR: &str = ";";
/// `jreFontDirName`: `java.home\lib\fonts`, absent from the reference JDK.
pub const JRE_FONT_DIR_NAME: &str = r"C:\Program Files\Eclipse Adoptium\jdk-17.0.19.10-hotspot\lib\fonts";

fn ends_with_any(name: &str, sfx: &[&str]) -> bool {
    let n = name.encode_utf16().count();
    n > 4 && sfx.iter().any(|s| name.ends_with(s))
}

/// `TTFilter.accept`.
pub fn tt_filter(name: &str) -> bool {
    ends_with_any(name, &[".ttf", ".TTF", ".ttc", ".TTC", ".otf", ".OTF"])
}

/// `T1Filter.accept` (`sun.java2d.noType1Font` unset).
pub fn t1_filter(name: &str) -> bool {
    ends_with_any(name, &[".pfa", ".pfb", ".PFA", ".PFB"])
}

pub fn is_absolute(p: &str) -> bool {
    std::path::Path::new(p).is_absolute()
}

pub fn exists(p: &str) -> bool {
    std::path::Path::new(p).exists()
}

#[derive(Clone)]
struct FontRegistrationInfo {
    font_file_path: String,
    font_format: i32,
    java_rasterizer: bool,
    font_rank: i32,
}

#[derive(Clone, Default)]
pub struct FamilyDescription {
    pub family_name: String,
    pub plain_full_name: Option<String>,
    pub bold_full_name: Option<String>,
    pub italic_full_name: Option<String>,
    pub bold_italic_full_name: Option<String>,
    pub plain_file_name: Option<String>,
    pub bold_file_name: Option<String>,
    pub italic_file_name: Option<String>,
    pub bold_italic_file_name: Option<String>,
    pub failed: bool,
}

thread_local! {
    static INSTANCE: OnceCell<Rc<SunFontManager>> = const { OnceCell::new() };
}

pub struct SunFontManager {
    comp_fonts: RefCell<Vec<Rc<Font2D>>>,
    composite_fonts: RefCell<HashMap<String, Rc<Font2D>>>,
    physical_fonts: RefCell<IndexMap<String, Rc<Font2D>>>,
    registered_fonts: RefCell<HashMap<String, Rc<Font2D>>>,
    full_name_to_font: RefCell<HashMap<String, Rc<Font2D>>>,
    locale_full_names_to_font: RefCell<Option<HashMap<String, Rc<Font2D>>>>,
    default_physical_font: RefCell<Option<Rc<Font2D>>>,
    pub loaded_all_fonts: Cell<bool>,
    pub loaded_all_font_files: Cell<bool>,
    jre_other_font_files: RefCell<Option<Vec<Option<String>>>>,
    no_other_jre_font_files: Cell<bool>,
    missing_font_files: RefCell<Option<HashSet<String>>>,
    default_font_name: RefCell<Option<String>>,
    default_font_file_name: RefCell<Option<String>>,
    pub registered_font_files: RefCell<HashSet<String>>,
    pub font_path: RefCell<Option<String>>,
    font_config: RefCell<Option<Rc<FontConfiguration>>>,
    discovered_all_fonts: Cell<bool>,
    all_families: RefCell<Option<Vec<String>>>,
    last_default_locale: RefCell<Option<Locale>>,
    deferred_font_files: RefCell<IndexMap<String, FontRegistrationInfo>>,
    initialised_fonts: RefCell<HashMap<String, Option<Rc<Font2DHandle>>>>,
    font_to_file_map: RefCell<Option<JHashMap<String, String>>>,
    font_to_family_name_map: RefCell<Option<JHashMap<String, String>>>,
    family_to_font_list_map: RefCell<Option<JHashMap<String, Vec<String>>>>,
    path_dirs: RefCell<Option<Vec<String>>>,
    have_checked_unreferenced_font_files: Cell<bool>,
    font_name_cache: RefCell<HashMap<String, Rc<Font2D>>>,
    platform_font_map: RefCell<Option<HashMap<String, FamilyDescription>>>,
    eudc_font: RefCell<Option<Rc<Font2D>>>,
    pool: RefCell<VecDeque<Weak<Font2D>>>,
}

impl SunFontManager {
    /// `SunFontManager.getInstance()`: built on first use, as `FontManagerFactory` does.
    pub fn get_instance() -> Rc<SunFontManager> {
        let (fm, fresh) = INSTANCE.with(|i| {
            let mut fresh = false;
            let fm = i
                .get_or_init(|| {
                    fresh = true;
                    Rc::new(SunFontManager::empty())
                })
                .clone();
            (fm, fresh)
        });
        if fresh {
            fm.init();
        }
        fm
    }

    fn empty() -> SunFontManager {
        SunFontManager {
            comp_fonts: RefCell::new(Vec::new()),
            composite_fonts: RefCell::new(HashMap::new()),
            physical_fonts: RefCell::new(IndexMap::new()),
            registered_fonts: RefCell::new(HashMap::new()),
            full_name_to_font: RefCell::new(HashMap::new()),
            locale_full_names_to_font: RefCell::new(None),
            default_physical_font: RefCell::new(None),
            loaded_all_fonts: Cell::new(false),
            loaded_all_font_files: Cell::new(false),
            jre_other_font_files: RefCell::new(None),
            no_other_jre_font_files: Cell::new(false),
            missing_font_files: RefCell::new(None),
            default_font_name: RefCell::new(None),
            default_font_file_name: RefCell::new(None),
            registered_font_files: RefCell::new(HashSet::new()),
            font_path: RefCell::new(None),
            font_config: RefCell::new(None),
            discovered_all_fonts: Cell::new(false),
            all_families: RefCell::new(None),
            last_default_locale: RefCell::new(None),
            deferred_font_files: RefCell::new(IndexMap::new()),
            initialised_fonts: RefCell::new(HashMap::new()),
            font_to_file_map: RefCell::new(None),
            font_to_family_name_map: RefCell::new(None),
            family_to_font_list_map: RefCell::new(None),
            path_dirs: RefCell::new(None),
            have_checked_unreferenced_font_files: Cell::new(false),
            font_name_cache: RefCell::new(HashMap::new()),
            platform_font_map: RefCell::new(None),
            eudc_font: RefCell::new(None),
            pool: RefCell::new(VecDeque::new()),
        }
    }

    /// The `Win32FontManager` static initialiser and the `SunFontManager()`/`Win32FontManager()` constructors.
    fn init(&self) {
        *self.eudc_font.borrow_mut() = win32::eudc_font();
        self.register_fonts_in_dir(JRE_FONT_DIR_NAME, true, crate::asc::flex2::fonts::java2d::sun::font::font2d::JRE_RANK, true, false);
        let fc = Rc::new(FontConfiguration::new());
        *self.font_config.borrow_mut() = Some(fc.clone());
        let font_info = win32::get_default_platform_font(self);
        *self.default_font_name.borrow_mut() = Some(font_info.0);
        *self.default_font_file_name.borrow_mut() = Some(font_info.1);
        if let Some(extra) = fc.get_extra_font_path() {
            self.register_font_dirs(&extra);
        }
        self.init_composite_fonts(&fc);
    }

    pub fn get_eudc_font(&self) -> Option<Rc<Font2D>> {
        self.eudc_font.borrow().clone()
    }

    pub fn get_font_configuration(&self) -> Rc<FontConfiguration> {
        self.font_config.borrow().clone().unwrap()
    }

    pub fn get_new_composite(&self, family: Option<&str>, style: i32, handle: &Rc<Font2DHandle>) -> Rc<Font2DHandle> {
        let f = handle.get();
        let Some(old_comp) = f.composite() else { return handle.clone() };
        let old_font = old_comp.get_slot_font(&f, 0);
        let family = family.map(|s| s.to_string()).unwrap_or_else(|| old_font.get_family_name(None));
        let style = if style == -1 { f.get_style() } else { style };
        let new_font = match self.find_font2d(&family, style, NO_FALLBACK) {
            Some(n) if n.is_physical() => n,
            _ => old_font,
        };
        let Some(dialog2d) = self.find_font2d("dialog", style, NO_FALLBACK).filter(|d| d.composite().is_some()) else { return handle.clone() };
        let comp_font = CompositeFont::new_with_physical(&new_font, &dialog2d);
        comp_font.handle()
    }

    fn register_composite_font(&self, composite_name: &str, component_file_names: Vec<Option<String>>, component_names: Vec<Option<String>>, num_metrics_slots: usize, exclusion_ranges: Vec<i32>, exclusion_max_index: Vec<i32>, defer: bool) {
        let cf = CompositeFont::new(composite_name, Some(component_file_names), Some(component_names), num_metrics_slots, Some(exclusion_ranges), Some(exclusion_max_index), defer, self.get_eudc_font());
        self.add_composite_to_font_list(&cf, FONT_CONFIG_RANK);
        self.comp_fonts.borrow_mut().push(cf);
    }

    fn add_composite_to_font_list(&self, f: &Rc<Font2D>, rank: i32) {
        f.set_rank(rank);
        self.composite_fonts.borrow_mut().insert(f.full_name.clone(), f.clone());
        self.full_name_to_font.borrow_mut().insert(f.full_name.to_lowercase(), f.clone());
        let family = FontFamily::get_family(&f.family_name).unwrap_or_else(|| FontFamily::new(&f.family_name, true, rank));
        family.set_font(f, f.get_style());
    }

    fn add_to_font_list(&self, f: &Rc<Font2D>, rank: i32) -> Option<Rc<Font2D>> {
        let font_name = f.full_name.clone();
        let family_name = f.family_name.clone();
        if font_name.is_empty() {
            return None;
        }
        if self.composite_fonts.borrow().contains_key(&font_name) {
            return None;
        }
        f.set_rank(rank);
        let old = self.physical_fonts.borrow().get(&font_name).cloned();
        match old {
            None => {
                self.physical_fonts.borrow_mut().insert(font_name.clone(), f.clone());
                let family = FontFamily::get_family(&family_name).unwrap_or_else(|| FontFamily::new(&family_name, false, rank));
                family.set_font(f, f.get_style());
                self.full_name_to_font.borrow_mut().insert(font_name.to_lowercase(), f.clone());
                Some(f.clone())
            }
            Some(old_font) => {
                let new_font = f;
                if old_font.get_rank() >= rank {
                    if old_font.get_rank() == rank {
                        match (old_font.ttf(), new_font.ttf()) {
                            (Some(o), Some(n)) => {
                                if o.file_size >= n.file_size {
                                    return Some(old_font);
                                }
                            }
                            _ => return Some(old_font),
                        }
                    }
                    if old_font.plat_name().map(|p| p.starts_with(JRE_FONT_DIR_NAME)).unwrap_or(false) {
                        return Some(old_font);
                    }
                    self.replace_font(&old_font, new_font);
                    self.physical_fonts.borrow_mut().insert(font_name.clone(), new_font.clone());
                    self.full_name_to_font.borrow_mut().insert(font_name.to_lowercase(), new_font.clone());
                    let family = FontFamily::get_family(&family_name).unwrap_or_else(|| FontFamily::new(&family_name, false, rank));
                    family.set_font(new_font, new_font.get_style());
                    Some(new_font.clone())
                } else {
                    Some(old_font)
                }
            }
        }
    }

    pub fn get_registered_fonts(&self) -> Vec<Rc<Font2D>> {
        let mut v = self.comp_fonts.borrow().clone();
        v.extend(self.get_physical_fonts());
        v
    }

    pub fn get_physical_fonts(&self) -> Vec<Rc<Font2D>> {
        self.physical_fonts.borrow().values().cloned().collect()
    }

    fn initialise_deferred_fonts(&self) {
        let keys: Vec<String> = self.deferred_font_files.borrow().keys().cloned().collect();
        for file_name in keys {
            self.initialise_deferred_font(&file_name);
        }
    }

    fn register_deferred_jre_fonts(&self, jre_dir: &str) {
        let infos: Vec<FontRegistrationInfo> = self.deferred_font_files.borrow().values().cloned().collect();
        for info in infos {
            if info.font_file_path.starts_with(jre_dir) {
                self.initialise_deferred_font(&info.font_file_path);
            }
        }
    }

    fn find_jre_deferred_font(&self, name: &str, style: i32) -> Option<Rc<Font2D>> {
        if self.no_other_jre_font_files.get() {
            return None;
        }
        if self.jre_other_font_files.borrow().is_none() {
            let other: Vec<Option<String>> = self
                .deferred_font_files
                .borrow()
                .keys()
                .filter(|f| std::path::Path::new(f.as_str()).parent().map(|d| d.to_string_lossy() == JRE_FONT_DIR_NAME).unwrap_or(false))
                .map(|f| Some(f.clone()))
                .collect();
            if other.is_empty() {
                self.no_other_jre_font_files.set(true);
            }
            *self.jre_other_font_files.borrow_mut() = Some(other);
        }
        let n = self.jre_other_font_files.borrow().as_ref().unwrap().len();
        for i in 0..n {
            let Some(file_name) = self.jre_other_font_files.borrow_mut().as_mut().unwrap()[i].take() else { continue };
            if let Some(pf) = self.initialise_deferred_font(&file_name) {
                if (equals_ignore_case(&pf.get_font_name(None), name) || equals_ignore_case(&pf.get_family_name(None), name)) && pf.get_style() == style {
                    return Some(pf);
                }
            }
        }
        None
    }

    fn find_other_deferred_font(&self, name: &str, style: i32) -> Option<Rc<Font2D>> {
        let keys: Vec<String> = self.deferred_font_files.borrow().keys().cloned().collect();
        for file_name in keys {
            if let Some(pf) = self.initialise_deferred_font(&file_name) {
                if (equals_ignore_case(&pf.get_font_name(None), name) || equals_ignore_case(&pf.get_family_name(None), name)) && pf.get_style() == style {
                    return Some(pf);
                }
            }
        }
        None
    }

    fn find_deferred_font(&self, name: &str, style: i32) -> Option<Rc<Font2D>> {
        self.find_jre_deferred_font(name, style).or_else(|| self.find_other_deferred_font(name, style))
    }

    pub fn register_deferred_font(&self, file_name_key: &str, full_path_name: &str, font_format: i32, use_java_rasterizer: bool, font_rank: i32) {
        self.deferred_font_files.borrow_mut().insert(file_name_key.to_string(), FontRegistrationInfo { font_file_path: full_path_name.to_string(), font_format, java_rasterizer: use_java_rasterizer, font_rank });
    }

    pub fn initialise_deferred_font(&self, file_name_key: &str) -> Option<Rc<Font2D>> {
        let reg_info = self.deferred_font_files.borrow().get(file_name_key).cloned();
        let mut physical_font = None;
        match reg_info {
            Some(reg_info) => {
                self.deferred_font_files.borrow_mut().shift_remove(file_name_key);
                physical_font = self.register_font_file_full(&reg_info.font_file_path, reg_info.font_format, reg_info.java_rasterizer, reg_info.font_rank);
                self.initialised_fonts.borrow_mut().insert(file_name_key.to_string(), physical_font.as_ref().map(|p| p.handle()));
            }
            None => {
                let handle = self.initialised_fonts.borrow().get(file_name_key).cloned();
                match handle {
                    None => {
                        self.initialised_fonts.borrow_mut().insert(file_name_key.to_string(), None);
                    }
                    Some(h) => physical_font = h.map(|h| h.get()),
                }
            }
        }
        physical_font
    }

    /// `registerFontFile(fileName, nativeNames, fontFormat, useJavaRasterizer, fontRank)`.
    pub fn register_font_file_full(&self, file_name: &str, font_format: i32, use_java_rasterizer: bool, font_rank: i32) -> Option<Rc<Font2D>> {
        if let Some(r) = self.registered_fonts.borrow().get(file_name) {
            return Some(r.clone());
        }
        let mut physical_font: Option<Rc<Font2D>> = None;
        if font_format == FONTFORMAT_TRUETYPE {
            let mut fnum = 0;
            loop {
                let ttf = match TrueTypeFont::new(file_name, fnum, use_java_rasterizer) {
                    Ok(t) => t,
                    Err(_) => break,
                };
                fnum += 1;
                let count = ttf.get_font_count();
                let f = Font2D::new_true_type(ttf, TTF_RANK);
                let pf = self.add_to_font_list(&f, font_rank);
                if physical_font.is_none() {
                    physical_font = pf;
                }
                if fnum >= count {
                    break;
                }
            }
        }
        if let Some(p) = &physical_font {
            if font_format != FONTFORMAT_NATIVE {
                self.registered_fonts.borrow_mut().insert(file_name.to_string(), p.clone());
            }
        }
        physical_font
    }

    pub fn get_default_physical_font(&self) -> Rc<Font2D> {
        if self.default_physical_font.borrow().is_none() {
            let name = self.get_default_font_face_name();
            if let Some(f) = self.find_font2d(&name, FONT_PLAIN, NO_FALLBACK) {
                if f.is_physical() {
                    *self.default_physical_font.borrow_mut() = Some(f);
                }
            }
            if self.default_physical_font.borrow().is_none() {
                let first = self.physical_fonts.borrow().values().next().cloned();
                *self.default_physical_font.borrow_mut() = Some(first.expect("Probable fatal error: No physical fonts found."));
            }
        }
        self.default_physical_font.borrow().clone().unwrap()
    }

    pub fn get_default_logical_font(&self, style: i32) -> Option<Rc<Font2D>> {
        self.find_font2d("dialog", style, NO_FALLBACK)
    }

    fn get_font_files_from_path(&self, no_type1: bool) -> Vec<String> {
        let filter = |n: &str| if no_type1 { tt_filter(n) } else { tt_filter(n) || t1_filter(n) };
        let dirs = self.path_dirs.borrow().clone().unwrap_or_default();
        let mut out = Vec::new();
        for d in &dirs {
            for f in list_dir(d) {
                if filter(&f) {
                    out.push(f.to_lowercase());
                }
            }
        }
        out
    }

    fn resolve_windows_fonts(&self) {
        let mut unmapped_font_names: Option<Vec<String>> = None;
        let keys: Vec<String> = self.font_to_family_name_map.borrow().as_ref().unwrap().keys().cloned().collect();
        for font in keys {
            let file = self.font_to_file_map.borrow().as_ref().unwrap().get(&font).cloned();
            if file.is_none() {
                if font.find("  ").map(|i| i > 0).unwrap_or(false) {
                    let new_name = font.replacen("  ", " ", 1);
                    let file = self.font_to_file_map.borrow().as_ref().unwrap().get(&new_name).cloned();
                    if let Some(file) = file {
                        if !self.font_to_family_name_map.borrow().as_ref().unwrap().contains_key(&new_name) {
                            let mut m = self.font_to_file_map.borrow_mut();
                            let m = m.as_mut().unwrap();
                            m.remove(&new_name);
                            m.insert(font.clone(), file);
                        }
                    }
                } else if font == "marlett" {
                    self.font_to_file_map.borrow_mut().as_mut().unwrap().insert(font.clone(), "marlett.ttf".into());
                } else if font == "david" {
                    let file = self.font_to_file_map.borrow().as_ref().unwrap().get("david regular").cloned();
                    if let Some(file) = file {
                        let mut m = self.font_to_file_map.borrow_mut();
                        let m = m.as_mut().unwrap();
                        m.remove("david regular");
                        m.insert("david".into(), file);
                    }
                } else {
                    unmapped_font_names.get_or_insert_with(Vec::new).push(font.clone());
                }
            }
        }
        let Some(mut unmapped_font_names) = unmapped_font_names else { return };
        let mut unmapped_font_files: JHashSet<String> = JHashSet::new();
        let ffmap_copy: JHashMap<String, String> = {
            let m = self.font_to_file_map.borrow();
            let m = m.as_ref().unwrap();
            let mut c = JHashMap::new();
            c.put_all(m.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>(), m.len());
            c
        };
        let mut ffmap_copy = ffmap_copy;
        for key in self.font_to_family_name_map.borrow().as_ref().unwrap().keys() {
            ffmap_copy.remove(key);
        }
        for (key, value) in ffmap_copy.iter() {
            unmapped_font_files.insert(value.clone());
            self.font_to_file_map.borrow_mut().as_mut().unwrap().remove(key);
        }
        self.resolve_font_files(&unmapped_font_files, &mut unmapped_font_names);
        if !unmapped_font_names.is_empty() {
            let registry_files: Vec<String> = self.font_to_file_map.borrow().as_ref().unwrap().values().map(|v| v.to_lowercase()).collect();
            for path_file in self.get_font_files_from_path(true) {
                if !registry_files.contains(&path_file) {
                    unmapped_font_files.insert(path_file);
                }
            }
            self.resolve_font_files(&unmapped_font_files, &mut unmapped_font_names);
        }
        if !unmapped_font_names.is_empty() {
            for name in unmapped_font_names.clone() {
                let family_name = self.font_to_family_name_map.borrow().as_ref().unwrap().get(&name).cloned();
                if let Some(family_name) = family_name {
                    let size = self.family_to_font_list_map.borrow().as_ref().unwrap().get(&family_name).map(|f| f.len());
                    if let Some(size) = size {
                        if size <= 1 {
                            self.family_to_font_list_map.borrow_mut().as_mut().unwrap().remove(&family_name);
                        }
                    }
                }
                self.font_to_family_name_map.borrow_mut().as_mut().unwrap().remove(&name);
            }
        }
    }

    fn check_for_unreferenced_font_files(&self) {
        if self.have_checked_unreferenced_font_files.get() {
            return;
        }
        self.have_checked_unreferenced_font_files.set(true);
        let registry_files: Vec<String> = self.font_to_file_map.borrow().as_ref().unwrap().values().map(|v| v.to_lowercase()).collect();
        let mut maps2: Option<(JHashMap<String, String>, JHashMap<String, String>, JHashMap<String, Vec<String>>)> = None;
        for path_file in self.get_font_files_from_path(false) {
            if registry_files.contains(&path_file) {
                continue;
            }
            let Some(f) = self.register_font_file_path(&self.get_path_name(&path_file)) else { continue };
            if maps2.is_none() {
                let copy = |m: &JHashMap<String, String>| {
                    let mut c = JHashMap::new();
                    c.put_all(m.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>(), m.len());
                    c
                };
                let a = copy(self.font_to_file_map.borrow().as_ref().unwrap());
                let b = copy(self.font_to_family_name_map.borrow().as_ref().unwrap());
                let mut c3 = JHashMap::new();
                {
                    let m = self.family_to_font_list_map.borrow();
                    let m = m.as_ref().unwrap();
                    c3.put_all(m.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>(), m.len());
                }
                maps2 = Some((a, b, c3));
            }
            let (ff2, fam2, list2) = maps2.as_mut().unwrap();
            let font_name = f.get_font_name(None);
            let family = f.get_family_name(None);
            let family_lc = family.to_lowercase();
            fam2.insert(font_name.clone(), family);
            ff2.insert(font_name.clone(), path_file.clone());
            let mut fonts = list2.get(&family_lc).cloned().unwrap_or_default();
            fonts.push(font_name);
            list2.insert(family_lc, fonts);
        }
        if let Some((a, b, c)) = maps2 {
            *self.font_to_file_map.borrow_mut() = Some(a);
            *self.family_to_font_list_map.borrow_mut() = Some(c);
            *self.font_to_family_name_map.borrow_mut() = Some(b);
        }
    }

    fn resolve_font_files(&self, unmapped_files: &JHashSet<String>, unmapped_fonts: &mut Vec<String>) {
        let l = Locale::startup();
        for file in unmapped_files.iter() {
            let full_path = self.get_path_name(file);
            let mut fnum = 0;
            loop {
                let Ok(ttf) = TrueTypeFont::new(&full_path, fnum, false) else { break };
                fnum += 1;
                let font_name = ttf.get_font_name(Some(&l)).to_lowercase();
                if let Some(i) = unmapped_fonts.iter().position(|n| *n == font_name) {
                    self.font_to_file_map.borrow_mut().as_mut().unwrap().insert(font_name, file.clone());
                    unmapped_fonts.remove(i);
                }
                if fnum >= ttf.get_font_count() {
                    break;
                }
            }
        }
    }

    fn find_font_from_platform_map(&self, lc_name: &str, style: i32) -> Option<Rc<Font2D>> {
        if self.platform_font_map.borrow().is_none() {
            *self.platform_font_map.borrow_mut() = Some(win32::populate_hardcoded_file_name_map());
        }
        if self.platform_font_map.borrow().as_ref().unwrap().is_empty() {
            return None;
        }
        let first_word = match lc_name.find(' ') {
            Some(i) if i > 0 => &lc_name[..i],
            _ => lc_name,
        };
        let fd = self.platform_font_map.borrow().as_ref().unwrap().get(first_word).cloned();
        let Some(fd) = fd else { return None };
        if fd.failed {
            return None;
        }
        let eq = |a: &Option<String>| a.as_deref().map(|a| equals_ignore_case(lc_name, a)).unwrap_or(false);
        let style_index = if eq(&fd.plain_full_name) {
            0
        } else if eq(&fd.bold_full_name) {
            1
        } else if eq(&fd.italic_full_name) {
            2
        } else if eq(&fd.bold_italic_full_name) {
            3
        } else {
            -1
        };
        if style_index == -1 && !equals_ignore_case(lc_name, &fd.family_name) {
            return None;
        }
        self.get_platform_font_dirs();
        let mut failure = false;
        let path = |f: &Option<String>| -> Option<String> { f.as_ref().map(|f| self.get_path_name(f)) };
        let files = [path(&fd.plain_file_name), path(&fd.bold_file_name), path(&fd.italic_file_name), path(&fd.bold_italic_file_name)];
        for f in files.iter().flatten() {
            if !exists(f) {
                failure = true;
            }
        }
        if failure {
            if let Some(m) = self.platform_font_map.borrow_mut().as_mut() {
                if let Some(d) = m.get_mut(first_word) {
                    d.failed = true;
                }
            }
            return None;
        }
        let mut font = None;
        for (f, file) in files.iter().enumerate() {
            let Some(file) = file else { continue };
            let pf = self.register_font_file_full(file, FONTFORMAT_TRUETYPE, false, TTF_RANK);
            if f as i32 == style_index {
                font = pf;
            }
        }
        let mut style = style;
        if let Some(font_family) = FontFamily::get_family(&fd.family_name) {
            match &font {
                None => {
                    font = font_family.get_font(style).or_else(|| font_family.get_closest_style(style));
                }
                Some(f) => {
                    if style > 0 && style != f.get_style() {
                        style |= f.get_style();
                        font = font_family.get_font(style).or_else(|| font_family.get_closest_style(style));
                    }
                }
            }
        }
        font
    }

    fn get_full_name_to_file_map_size(&self) -> usize {
        if self.font_to_file_map.borrow().is_none() {
            let dirs = self.get_platform_font_dirs();
            *self.path_dirs.borrow_mut() = Some(dirs);
            let mut ff = JHashMap::with_capacity(100);
            let mut fam = JHashMap::with_capacity(100);
            let mut list = JHashMap::with_capacity(50);
            win32::populate_font_file_name_map(&mut ff, &mut fam, &mut list);
            *self.font_to_file_map.borrow_mut() = Some(ff);
            *self.font_to_family_name_map.borrow_mut() = Some(fam);
            *self.family_to_font_list_map.borrow_mut() = Some(list);
            self.resolve_windows_fonts();
        }
        self.font_to_file_map.borrow().as_ref().unwrap().len()
    }

    pub fn got_fonts_from_platform(&self) -> bool {
        self.get_full_name_to_file_map_size() != 0
    }

    /// `registerFontFile(String file)`.
    fn register_font_file_path(&self, file: &str) -> Option<Rc<Font2D>> {
        if is_absolute(file) && !self.registered_fonts.borrow().contains_key(file) {
            let (format, rank) = if tt_filter(file) {
                (FONTFORMAT_TRUETYPE, TTF_RANK)
            } else if t1_filter(file) {
                (FONTFORMAT_TYPE1, TYPE1_RANK)
            } else {
                (FONTFORMAT_NONE, UNKNOWN_RANK)
            };
            if format == FONTFORMAT_NONE {
                return None;
            }
            return self.register_font_file_full(file, format, false, rank);
        }
        None
    }

    fn register_other_font_files(&self) {
        if self.get_full_name_to_file_map_size() == 0 {
            return;
        }
        let files: Vec<String> = self.font_to_file_map.borrow().as_ref().unwrap().values().cloned().collect();
        for file in files {
            self.register_font_file_path(&file);
        }
    }

    fn get_family_names_from_platform(&self, family_names: &mut Vec<(String, String)>, _requested_locale: &Locale) -> bool {
        if self.get_full_name_to_file_map_size() == 0 {
            return false;
        }
        self.check_for_unreferenced_font_files();
        let names: Vec<String> = self.font_to_family_name_map.borrow().as_ref().unwrap().values().cloned().collect();
        for name in names {
            tree_put(family_names, name.to_lowercase(), name);
        }
        true
    }

    pub fn get_path_name(&self, s: &str) -> String {
        if is_absolute(s) {
            return s.to_string();
        }
        let dirs = self.path_dirs.borrow().clone().unwrap_or_default();
        if dirs.len() == 1 {
            return format!("{}{}{}", dirs[0], SEPARATOR, s);
        }
        for d in &dirs {
            let p = format!("{}{}{}", d, SEPARATOR, s);
            if exists(&p) {
                return std::path::absolute(&p).map(|a| a.to_string_lossy().to_string()).unwrap_or(p);
            }
        }
        s.to_string()
    }

    fn find_font_from_platform(&self, lc_name: &str, style: i32) -> Option<Rc<Font2D>> {
        if self.get_full_name_to_file_map_size() == 0 {
            return None;
        }
        let mut family: Option<Vec<String>>;
        let mut font_file = None;
        let mut family_name = self.font_to_family_name_map.borrow().as_ref().unwrap().get(lc_name).cloned();
        if let Some(fname) = &family_name {
            font_file = self.font_to_file_map.borrow().as_ref().unwrap().get(lc_name).cloned();
            family = self.family_to_font_list_map.borrow().as_ref().unwrap().get(&fname.to_lowercase()).cloned();
        } else {
            family = self.family_to_font_list_map.borrow().as_ref().unwrap().get(lc_name).cloned();
            if let Some(f) = &family {
                if !f.is_empty() {
                    let lc_font_name = f[0].to_lowercase();
                    family_name = self.font_to_family_name_map.borrow().as_ref().unwrap().get(&lc_font_name).cloned();
                }
            }
        }
        let (Some(font_list), Some(family_name)) = (family.take(), family_name) else { return None };
        if font_list.is_empty() {
            return None;
        }
        for f in &font_list {
            if self.font_to_file_map.borrow().as_ref().unwrap().get(&f.to_lowercase()).is_none() {
                return None;
            }
        }
        let mut physical_font = None;
        if let Some(ff) = &font_file {
            physical_font = self.register_font_file_full(&self.get_path_name(ff), FONTFORMAT_TRUETYPE, false, TTF_RANK);
        }
        for f in &font_list {
            let file_name = self.font_to_file_map.borrow().as_ref().unwrap().get(&f.to_lowercase()).cloned().unwrap();
            if font_file.as_deref() == Some(file_name.as_str()) {
                continue;
            }
            self.register_font_file_full(&self.get_path_name(&file_name), FONTFORMAT_TRUETYPE, false, TTF_RANK);
        }
        let mut style = style;
        if let Some(p) = &physical_font {
            style |= p.get_style();
        }
        FontFamily::get_family(&family_name).and_then(|ff| ff.get_font(style).or_else(|| ff.get_closest_style(style)))
    }

    pub fn find_font2d(&self, name: &str, style: i32, fallback: i32) -> Option<Rc<Font2D>> {
        let lower_case_name = name.to_lowercase();
        let map_name = format!("{}{}", lower_case_name, dot_style_str(style));
        if let Some(f) = self.font_name_cache.borrow().get(&map_name) {
            return Some(f.clone());
        }
        let mut name = name.to_string();
        if lower_case_name == "ms sans serif" {
            name = "sansserif".into();
        } else if lower_case_name == "ms serif" {
            name = "serif".into();
        }
        if lower_case_name == "default" {
            name = "dialog".into();
        }
        let cache = |f: &Rc<Font2D>| {
            self.font_name_cache.borrow_mut().insert(map_name.clone(), f.clone());
        };
        if let Some(family) = FontFamily::get_family(&name) {
            let mut font = family.get_font_with_exact_style_match(style);
            if font.is_none() {
                font = self.find_deferred_font(&name, style);
            }
            if font.is_none() {
                font = self.find_font_from_platform(&lower_case_name, style);
            }
            if font.is_none() {
                font = family.get_font(style);
            }
            if font.is_none() {
                font = family.get_closest_style(style);
            }
            if let Some(f) = font {
                cache(&f);
                return Some(f);
            }
        }
        let font = self.full_name_to_font.borrow().get(&lower_case_name).cloned();
        if let Some(font) = font {
            if font.get_style() == style || style == FONT_PLAIN {
                cache(&font);
                return Some(font);
            } else if let Some(family) = FontFamily::get_family(&font.get_family_name(None)) {
                let s = style | font.get_style();
                if let Some(ff) = family.get_font(s) {
                    cache(&ff);
                    return Some(ff);
                } else if let Some(ff) = family.get_closest_style(s) {
                    if ff.can_do_style(s) {
                        cache(&ff);
                        return Some(ff);
                    }
                }
            }
        }
        if let Some(f) = self.find_font_from_platform_map(&lower_case_name, style) {
            cache(&f);
            return Some(f);
        }
        if !self.deferred_font_files.borrow().is_empty() {
            if let Some(f) = self.find_jre_deferred_font(&lower_case_name, style) {
                cache(&f);
                return Some(f);
            }
        }
        if let Some(f) = self.find_font_from_platform(&lower_case_name, style) {
            cache(&f);
            return Some(f);
        }
        if !self.deferred_font_files.borrow().is_empty() {
            if let Some(f) = self.find_deferred_font(&name, style) {
                cache(&f);
                return Some(f);
            }
        }
        if !self.loaded_all_fonts.get() {
            self.load_fonts();
            self.loaded_all_fonts.set(true);
            return self.find_font2d(&name, style, fallback);
        }
        if !self.loaded_all_font_files.get() {
            self.load_font_files();
            self.loaded_all_font_files.set(true);
            return self.find_font2d(&name, style, fallback);
        }
        if let Some(f) = self.find_font2d_all_locales(&name, style) {
            cache(&f);
            return Some(f);
        }
        let compat_name = self.get_font_configuration().get_fallback_family_name(&name, None);
        if let Some(compat_name) = compat_name {
            let font = self.find_font2d(&compat_name, style, fallback);
            if let Some(f) = &font {
                cache(f);
            }
            return font;
        }
        match fallback {
            PHYSICAL_FALLBACK => Some(self.get_default_physical_font()),
            LOGICAL_FALLBACK => self.get_default_logical_font(style),
            _ => None,
        }
    }

    /// `createFont2D(fontFile, TRUETYPE_FONT, all, isCopy, tracker)` (soft/weak strike references do not matter here).
    pub fn create_font2d(&self, font_file_path: &str, all: bool) -> Result<Vec<Rc<Font2D>>, FontFormatException> {
        let ttf = TrueTypeFont::new(font_file_path, 0, true)?;
        let cnt = ttf.get_font_count();
        let mut f_list = vec![Font2D::new_true_type(ttf, TTF_RANK)];
        if all {
            for index in 1..cnt {
                let t = TrueTypeFont::new(font_file_path, index, true)?;
                f_list.push(Font2D::new_true_type(t, TTF_RANK));
            }
        }
        Ok(f_list)
    }

    pub fn de_register_bad_font(&self, font2d: &Rc<Font2D>) {
        if !font2d.is_physical() {
            return;
        }
        let d = self.get_default_physical_font();
        self.replace_font(font2d, &d);
    }

    pub fn replace_font(&self, old_font: &Rc<Font2D>, new_font: &Rc<Font2D>) {
        if !Rc::ptr_eq(&old_font.handle().get(), old_font) {
            return;
        }
        let mut new_font = new_font.clone();
        if Rc::ptr_eq(old_font, &new_font) {
            let other = self.get_physical_fonts().into_iter().find(|p| !Rc::ptr_eq(p, &new_font));
            match other {
                Some(o) => new_font = o,
                None => return,
            }
        }
        *old_font.handle().font2d.borrow_mut() = new_font.clone();
        self.physical_fonts.borrow_mut().shift_remove(&old_font.full_name);
        self.full_name_to_font.borrow_mut().remove(&old_font.full_name.to_lowercase());
        FontFamily::remove(old_font);
        if let Some(m) = self.locale_full_names_to_font.borrow_mut().as_mut() {
            for v in m.values_mut() {
                if Rc::ptr_eq(v, old_font) {
                    *v = new_font.clone();
                }
            }
        }
        let comps = self.comp_fonts.borrow().clone();
        for c in comps {
            if new_font.get_rank() > FONT_CONFIG_RANK {
                c.composite().unwrap().replace_component_font(old_font, &new_font);
            }
        }
    }

    fn load_locale_names(&self) {
        if self.locale_full_names_to_font.borrow().is_some() {
            return;
        }
        let mut m = HashMap::new();
        for f in self.get_registered_fonts() {
            if let Some(ttf) = f.ttf() {
                for n in ttf.get_all_full_names() {
                    m.insert(n, f.clone());
                }
                if let Some(family) = FontFamily::get_family(&f.family_name) {
                    FontFamily::add_locale_names(&family, &ttf.get_all_family_names());
                }
            }
        }
        *self.locale_full_names_to_font.borrow_mut() = Some(m);
    }

    fn find_font2d_all_locales(&self, name: &str, style: i32) -> Option<Rc<Font2D>> {
        if self.locale_full_names_to_font.borrow().is_none() {
            self.load_locale_names();
        }
        let lower_case_name = name.to_lowercase();
        if let Some(family) = FontFamily::get_locale_family(&lower_case_name) {
            if let Some(f) = family.get_font(style).or_else(|| family.get_closest_style(style)) {
                return Some(f);
            }
        }
        let font = self.locale_full_names_to_font.borrow().as_ref().unwrap().get(name).cloned();
        if let Some(font) = &font {
            if font.get_style() == style || style == FONT_PLAIN {
                return Some(font.clone());
            }
            if let Some(family) = FontFamily::get_family(&font.get_family_name(None)) {
                if let Some(ff) = family.get_font(style) {
                    return Some(ff);
                }
                if let Some(ff) = family.get_closest_style(style) {
                    return if ff.can_do_style(style) { Some(ff) } else { None };
                }
            }
        }
        font
    }

    pub fn get_platform_font_dirs(&self) -> Vec<String> {
        if let Some(p) = self.path_dirs.borrow().as_ref() {
            return p.clone();
        }
        let path = self.get_platform_font_path();
        let dirs: Vec<String> = path.split(PATH_SEPARATOR).filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
        *self.path_dirs.borrow_mut() = Some(dirs.clone());
        dirs
    }

    fn add_dir_fonts(&self, dir_name: &str, filter: fn(&str) -> bool, font_format: i32, use_java_rasterizer: bool, font_rank: i32, defer: bool, resolve_sym_links: bool) {
        let ls: Vec<String> = list_dir(dir_name).into_iter().filter(|n| filter(n)).collect();
        if ls.is_empty() {
            return;
        }
        let mut font_names = Vec::new();
        for f in ls {
            let the_file = format!("{}{}{}", dir_name, SEPARATOR, f);
            let mut full_name = None;
            if resolve_sym_links {
                full_name = canonical_path(&the_file);
            }
            let full_name = full_name.unwrap_or(the_file);
            if self.registered_font_files.borrow().contains(&full_name) {
                continue;
            }
            self.registered_font_files.borrow_mut().insert(full_name.clone());
            font_names.push(full_name);
        }
        for f in font_names {
            if defer {
                self.register_deferred_font(&f, &f, font_format, use_java_rasterizer, font_rank);
            } else if font_format == FONTFORMAT_TRUETYPE {
                self.register_font_file_full(&f, font_format, use_java_rasterizer, font_rank);
            }
        }
    }

    pub fn get_platform_font_path(&self) -> String {
        if self.font_path.borrow().is_none() {
            *self.font_path.borrow_mut() = Some(win32::get_font_path(false));
        }
        self.font_path.borrow().clone().unwrap()
    }

    fn load_fonts(&self) {
        if self.discovered_all_fonts.get() {
            return;
        }
        self.initialise_deferred_fonts();
        if self.font_path.borrow().is_none() {
            let p = self.get_platform_font_path();
            self.register_font_dirs(&p);
        }
        if self.font_path.borrow().is_some() && !self.got_fonts_from_platform() {
            let p = self.font_path.borrow().clone().unwrap();
            self.register_fonts_on_path(&p, false, UNKNOWN_RANK, false, true);
            self.loaded_all_font_files.set(true);
        }
        self.register_other_font_files();
        self.discovered_all_fonts.set(true);
    }

    fn register_font_dirs(&self, _path_name: &str) {}

    fn register_fonts_on_path(&self, path_name: &str, use_java_rasterizer: bool, font_rank: i32, defer: bool, resolve_sym_links: bool) {
        for d in path_name.split(PATH_SEPARATOR).filter(|s| !s.is_empty()) {
            self.register_fonts_in_dir(d, use_java_rasterizer, font_rank, defer, resolve_sym_links);
        }
    }

    fn register_fonts_in_dir(&self, dir_name: &str, use_java_rasterizer: bool, font_rank: i32, defer: bool, resolve_sym_links: bool) {
        self.add_dir_fonts(dir_name, tt_filter, FONTFORMAT_TRUETYPE, use_java_rasterizer, if font_rank == UNKNOWN_RANK { TTF_RANK } else { font_rank }, defer, resolve_sym_links);
        self.add_dir_fonts(dir_name, t1_filter, FONTFORMAT_TYPE1, use_java_rasterizer, if font_rank == UNKNOWN_RANK { TYPE1_RANK } else { font_rank }, defer, resolve_sym_links);
    }

    pub fn get_default_font_file(&self) -> Option<String> {
        self.default_font_file_name.borrow().clone()
    }

    pub fn get_default_font_face_name(&self) -> String {
        self.default_font_name.borrow().clone().unwrap_or_default()
    }

    fn load_font_files(&self) {
        self.load_fonts();
        if self.loaded_all_font_files.get() {
            return;
        }
        let p = self.get_platform_font_path();
        self.register_fonts_on_path(&p, false, UNKNOWN_RANK, false, true);
        self.loaded_all_font_files.set(true);
    }

    fn init_composite_fonts(&self, font_config: &Rc<FontConfiguration>) {
        font_config.get_number_core_fonts();
        let fc_fonts = font_config.get_platform_font_names();
        for platform_font_name in fc_fonts.iter() {
            let mut font_file_name = font_config.get_file_name_from_platform_name(platform_font_name);
            if font_file_name.is_none() || font_file_name.as_deref() == Some(platform_font_name.as_str()) {
                font_file_name = Some(platform_font_name.clone());
            }
            win32::register_font_file(self, &font_file_name.unwrap(), FONT_CONFIG_RANK, true);
        }
        for descriptor in font_config.get_2d_composite_font_info() {
            let mut component_file_names = descriptor.component_file_names;
            let mut component_face_names = descriptor.component_face_names;
            if let Some(missing) = self.missing_font_files.borrow().as_ref() {
                for ii in 0..component_file_names.len() {
                    if component_file_names[ii].as_ref().map(|f| missing.contains(f)).unwrap_or(false) {
                        component_file_names[ii] = self.get_default_font_file();
                        component_face_names[ii] = Some(self.get_default_font_face_name());
                    }
                }
            }
            self.register_composite_font(&descriptor.face_name, component_file_names, component_face_names, descriptor.core_component_count, descriptor.exclusion_ranges, descriptor.exclusion_range_limits, true);
        }
    }

    pub fn add_to_missing_font_file_list(&self, file_name: &str) {
        self.missing_font_files.borrow_mut().get_or_insert_with(HashSet::new).insert(file_name.to_string());
    }

    pub fn get_installed_font_family_names(&self, requested_locale: Option<&Locale>) -> Vec<String> {
        let requested_locale = requested_locale.cloned().unwrap_or_else(Locale::get_default);
        if let (Some(all), Some(last)) = (self.all_families.borrow().as_ref(), self.last_default_locale.borrow().as_ref()) {
            if requested_locale == *last {
                return all.clone();
            }
        }
        let mut family_names: Vec<(String, String)> = Vec::new();
        for s in ["Serif", "SansSerif", "Monospaced", "Dialog", "DialogInput"] {
            tree_put(&mut family_names, s.to_lowercase(), s.to_string());
        }
        if requested_locale == Locale::startup() && self.get_family_names_from_platform(&mut family_names, &requested_locale) {
            self.get_jre_font_family_names(&mut family_names, &requested_locale);
        } else {
            self.load_font_files();
            for p in self.get_physical_fonts() {
                let name = p.get_family_name(Some(&requested_locale));
                tree_put(&mut family_names, name.to_lowercase(), name);
            }
        }
        let retval: Vec<String> = family_names.into_iter().map(|(_, v)| v).collect();
        if requested_locale == Locale::get_default() {
            *self.last_default_locale.borrow_mut() = Some(requested_locale);
            *self.all_families.borrow_mut() = Some(retval.clone());
        }
        retval
    }

    fn get_jre_font_family_names(&self, family_names: &mut Vec<(String, String)>, requested_locale: &Locale) {
        self.register_deferred_jre_fonts(JRE_FONT_DIR_NAME);
        for p in self.get_physical_fonts() {
            let name = p.get_family_name(Some(requested_locale));
            tree_put(family_names, name.to_lowercase(), name);
        }
    }

    /// `addToPool`: keeps at most 20 fonts' file data and scalers loaded, dropping the oldest (a fresh scaler on the
    /// same bytes gives the same results, as the JDK's reopened file channel does).
    pub fn add_to_pool(&self, font: &Rc<Font2D>) {
        let mut pool = self.pool.borrow_mut();
        pool.retain(|w| w.upgrade().map(|f| !Rc::ptr_eq(&f, font)).unwrap_or(false));
        pool.push_back(Rc::downgrade(font));
        while pool.len() > CHANNELPOOLSIZE {
            if let Some(old) = pool.pop_front().and_then(|w| w.upgrade()) {
                if let Some(t) = old.ttf() {
                    let live = t.scaler.borrow().as_ref().map(|s| s.borrow().is_live()).unwrap_or(false);
                    if live {
                        *t.scaler.borrow_mut() = None;
                    }
                    *t.file_data.borrow_mut() = None;
                }
            }
        }
    }
}

fn dot_style_str(num: i32) -> &'static str {
    match num {
        1 => ".bold",
        2 => ".italic",
        3 => ".bolditalic",
        _ => ".plain",
    }
}

/// `String.equalsIgnoreCase`.
pub fn equals_ignore_case(a: &str, b: &str) -> bool {
    let (a, b): (Vec<u16>, Vec<u16>) = (a.encode_utf16().collect(), b.encode_utf16().collect());
    a.len() == b.len()
        && a.iter().zip(b.iter()).all(|(&x, &y)| {
            if x == y {
                return true;
            }
            let (cx, cy) = (char::from_u32(x as u32), char::from_u32(y as u32));
            match (cx, cy) {
                (Some(cx), Some(cy)) => {
                    let (ux, uy) = (cx.to_uppercase().next().unwrap(), cy.to_uppercase().next().unwrap());
                    ux == uy || ux.to_lowercase().next() == uy.to_lowercase().next()
                }
                _ => false,
            }
        })
}

/// `TreeMap<String,String>.put` on a sorted vector (Java `String.compareTo` order).
pub fn tree_put(m: &mut Vec<(String, String)>, k: String, v: String) {
    match m.binary_search_by(|(e, _)| crate::asc::util::jstr::compare(e, &k)) {
        Ok(i) => m[i].1 = v,
        Err(i) => m.insert(i, (k, v)),
    }
}

/// `new File(dir).list()`: the names in the directory's own enumeration order (`FindFirstFile` on Windows).
pub fn list_dir(dir: &str) -> Vec<String> {
    match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect(),
        Err(_) => Vec::new(),
    }
}

/// `File.getCanonicalPath()` on Windows: the on-disk spelling of every component.
pub fn canonical_path(p: &str) -> Option<String> {
    let c = std::fs::canonicalize(p).ok()?;
    let s = c.to_string_lossy().to_string();
    Some(s.strip_prefix(r"\\?\").map(|s| s.to_string()).unwrap_or(s))
}
