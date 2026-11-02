//! Port of swfutils `flash.fonts.JREFontManager` (+ `JREFontFace`): system fonts, and compatibility-3 font files,
//! through Java2D (`fonts::java2d`).

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::asc::flex2::flash::swf::builder::shape_builder::{PathIteratorWrapper, ShapeBuilder, TWIPS_PER_PIXEL};
use crate::asc::flex2::flash::swf::tags::TagRef;
use crate::asc::flex2::fonts::batik_font_manager::{process_location, BatikFont};
use crate::asc::flex2::fonts::cached_font_face::{guess_style_from_sub_family_name, is_bold, is_italic, CachedFontFace};
use crate::asc::flex2::fonts::cached_font_manager::{self, CachedFontManagerBase, FileKey, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE_STRING};
use crate::asc::flex2::fonts::font_description::{FontDescription, JUrl};
use crate::asc::flex2::fonts::font_face::{FaceRef, FontFace, SWF_EM_SQUARE};
use crate::asc::flex2::fonts::font_manager::{FmResult, FontManager, FontManagerBase, InitMapRef};
use crate::asc::flex2::fonts::font_set::FontSet;
use crate::asc::flex2::fonts::fs_type::FSType;
use crate::asc::flex2::fonts::glyph_entry::{GlyphEntry, GlyphEntryRef};
use crate::asc::flex2::fonts::java2d_api::{get_available_font_family_names, Font};
use crate::asc::flex2::fonts::local_font::{read_local_fonts, LocalFont};

pub const LOCAL_FONTS_SNAPSHOT: &str = "flex.fonts-snapshot";

thread_local! {
    /// `JREFontManager.systemFontNames`, a static the first lookup fills; per thread, like the Java2D state it reads.
    static SYSTEM_FONT_NAMES: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

pub struct JREFontManager {
    base: FontManagerBase,
    cached: CachedFontManagerBase,
    read_local_fonts: Cell<bool>,
    local_fonts: RefCell<HashMap<String, LocalFont>>,
    local_fonts_file: RefCell<Option<String>>,
}

impl Default for JREFontManager {
    fn default() -> Self {
        Self::new()
    }
}

impl JREFontManager {
    pub fn new() -> JREFontManager {
        JREFontManager { base: FontManagerBase::default(), cached: CachedFontManagerBase::default(), read_local_fonts: Cell::new(false), local_fonts: RefCell::new(HashMap::new()), local_fonts_file: RefCell::new(None) }
    }

    /// `createFontFromLocation(location, requestedStyle, useTwips)`.
    fn create_font_from_location(&self, url: &JUrl, requested_style: i32, use_twips: bool) -> FmResult<Option<String>> {
        if !url.get_protocol().to_lowercase().contains("file") {
            return Ok(None);
        }
        let Ok(font) = Font::create_font_truetype(std::path::Path::new(&url.path)) else { return Ok(None) };
        let family = font.get_family();
        let font_name = font.get_font_name_english();
        if self.base.major_compatibility_version.get() < 4 && requested_style != guess_style_from_sub_family_name(Some(&font_name)) {
            return Ok(None);
        }
        let location_str = process_location(url);
        let batik_font = BatikFont::create(location_str.as_deref());
        let fs = FSType::get_fs_type(batik_font.os2_license_type()?);
        let copyright = batik_font.name_record(0)?;
        let trademark = batik_font.name_record(7)?;
        let font = font.derive_font_attrs(&family, DEFAULT_FONT_SIZE, is_italic(requested_style), is_bold(requested_style));
        let set = match self.cached.font_cache_get(&family) {
            Some(s) => s,
            None => {
                let s = Rc::new(RefCell::new(FontSet::new(self.cached.max_faces_per_font as i32)));
                self.cached.font_cache_put(&family, s.clone());
                s
            }
        };
        let face = JREFontFace::new(font, requested_style, self.cached.max_glyphs_per_face.get() as i32, Some(fs), Some(copyright), Some(trademark), use_twips);
        set.borrow_mut().put(requested_style, Rc::new(face));
        Ok(Some(family))
    }

    fn read_local_fonts(&self) {
        self.read_local_fonts.set(true);
        let mut m = self.local_fonts.borrow_mut();
        m.clear();
        if let Some(f) = self.local_fonts_file.borrow().as_deref() {
            if let Ok(d) = std::fs::read(f) {
                if let Ok(fonts) = read_local_fonts(&d) {
                    m.extend(fonts);
                }
            }
        }
    }
}

fn get_style_as_string(style: i32) -> &'static str {
    match style {
        1 => "bold",
        2 => "italic",
        3 => "bolditalic",
        _ => "plain",
    }
}

fn initialize_system_fonts() -> Vec<String> {
    get_available_font_family_names().iter().map(|f| crate::asc::flex2::fonts::font_manager::java_trim(f).to_lowercase()).collect()
}

impl FontManager for JREFontManager {
    fn base(&self) -> &FontManagerBase {
        &self.base
    }

    fn initialize(&self, map: &InitMapRef) {
        self.base.initialize(map);
        self.cached.initialize(map);
        let mut f = map.as_ref().and_then(|m| m.local_fonts_snapshot.clone());
        if f.is_none() {
            f = Some("localFonts.ser".to_string());
        }
        *self.local_fonts_file.borrow_mut() = f;
    }

    fn create_define_font(&self, tag_code: i32, desc: &mut FontDescription) -> FmResult<Option<TagRef>> {
        cached_font_manager::create_define_font(self, tag_code, desc)
    }

    fn get_entry_from_location(&self, location: &JUrl, style: i32, use_twips: bool) -> FmResult<Option<FaceRef>> {
        let mut entry = None;
        let mut fam = self.cached.font_file_cache_get(&FileKey::Url(location.external_form.clone()));
        if fam.is_none() {
            fam = self.create_font_from_location(location, style, use_twips)?;
        }
        if let Some(family) = fam {
            if let Some(set) = self.cached.font_cache_get(&family) {
                entry = set.borrow().get(style);
            }
        }
        Ok(entry)
    }

    fn is_cached(&self) -> bool {
        true
    }

    fn create_set_for_system_font(&self, family: &str, _style: i32, use_twips: bool) -> FmResult<Option<FontSet>> {
        let style = _style;
        if SYSTEM_FONT_NAMES.with(|n| n.borrow().is_none()) {
            let names = initialize_system_fonts();
            SYSTEM_FONT_NAMES.with(|n| *n.borrow_mut() = Some(names));
        }
        let known = SYSTEM_FONT_NAMES.with(|n| n.borrow().as_ref().unwrap().contains(&crate::asc::flex2::fonts::font_manager::java_trim(family).to_lowercase()));
        if !known {
            return Ok(None);
        }
        let font = Font::decode(&format!("{}-{}-{}", family, get_style_as_string(style), DEFAULT_FONT_SIZE_STRING));
        let mut set = FontSet::new(self.cached.max_faces_per_font as i32);
        let s = font.get_style();
        set.put(s, Rc::new(JREFontFace::new(font, s, self.cached.max_glyphs_per_face.get() as i32, None, None, None, use_twips)));
        Ok(Some(set))
    }

    fn get_entry_from_system(&self, family_name: &str, style: i32, use_twips: bool) -> FmResult<Option<FaceRef>> {
        if !self.read_local_fonts.get() {
            self.read_local_fonts();
        }
        let mut entry = None;
        let set = match self.cached.font_cache_get(family_name) {
            Some(s) => Some(s.borrow().clone()),
            None => self.create_set_for_system_font(family_name, style, use_twips)?,
        };
        if let Some(set) = set {
            entry = set.get(style);
        }
        if let Some(e) = &entry {
            if let Some(f) = self.local_fonts.borrow().get(&e.get_postscript_name()?) {
                e.set_copyright(f.copyright.clone());
                e.set_trademark(f.trademark.clone());
                e.set_fs_type(Some(FSType::get_fs_type(f.fs_type)));
            }
        }
        Ok(entry)
    }
}

pub struct JREFontFace {
    pub cached: CachedFontFace,
    font: Font,
    ascent: i32,
    descent: i32,
    line_gap: i32,
    em_scale: f64,
}

impl JREFontFace {
    pub fn new(font: Font, style: i32, max_glyph: i32, fs_type: Option<FSType>, copyright: Option<String>, trademark: Option<String>, use_twips: bool) -> JREFontFace {
        let cached = CachedFontFace::with(max_glyph, style, fs_type, copyright, trademark, use_twips);
        let mut f = JREFontFace { cached, font, ascent: 0, descent: 0, line_gap: 0, em_scale: 0.0 };
        f.init();
        f
    }

    fn init(&mut self) {
        self.em_scale = SWF_EM_SQUARE as f64 / self.get_point_size();
        let (a, d) = self.font.font_metrics_ascent_descent();
        let t = if self.cached.use_twips { TWIPS_PER_PIXEL as f64 } else { 1.0 };
        self.ascent = (a as f64 * self.em_scale * t).round_ties_even() as i32;
        self.descent = (d as f64 * self.em_scale * t).round_ties_even() as i32;
        self.line_gap = (self.get_point_size() - self.ascent as f64 - self.descent as f64).abs().round_ties_even() as i32;
    }

    pub fn create_glyph_entry(&self, c: u16, reference_char: u16) -> GlyphEntryRef {
        let (outline, advance) = self.font.glyph_outline_and_advance(reference_char);
        let advance = (advance as f64).round_ties_even() as i32;
        let segments = outline
            .iter()
            .map(|(t, c)| {
                let mut d = [0.0f64; 6];
                for i in 0..6 {
                    d[i] = c[i] as f64 * self.em_scale;
                }
                (*t, d)
            })
            .collect();
        let mut b = ShapeBuilder::new(self.cached.use_twips);
        b.set_current_line_style(0);
        b.set_current_fill_style1(1);
        b.set_use_fill_style1(true);
        b.process_shape(&mut PathIteratorWrapper::new(segments));
        let t = if self.cached.use_twips { TWIPS_PER_PIXEL as f64 } else { 1.0 };
        Rc::new(RefCell::new(GlyphEntry { advance: (advance as f64 * self.em_scale * t) as i32, character: c, shape: Some(b.build()), ..Default::default() }))
    }

    pub fn get_font(&self) -> &Font {
        &self.font
    }
}

impl FontFace for JREFontFace {
    fn get_glyph_entry(&self, c: u16) -> FmResult<Option<GlyphEntryRef>> {
        self.cached.cached_glyph(c, || self.can_display(c), || Ok(self.create_glyph_entry(c, c)))
    }
    fn get_missing_glyph_code(&self) -> i32 {
        self.font.get_missing_glyph_code()
    }
    fn get_point_size(&self) -> f64 {
        self.font.get_size2d() as f64
    }
    fn get_family(&self) -> FmResult<String> {
        Ok(self.font.get_name())
    }
    fn get_ascent(&self) -> i32 {
        self.ascent
    }
    fn get_descent(&self) -> i32 {
        self.descent
    }
    fn get_line_gap(&self) -> i32 {
        self.line_gap
    }
    fn get_first_char(&self) -> FmResult<i32> {
        Ok(0)
    }
    fn get_num_glyphs(&self) -> FmResult<i32> {
        Ok(self.font.get_num_glyphs())
    }
    fn can_display(&self, c: u16) -> FmResult<bool> {
        Ok(self.font.can_display(c))
    }
    fn get_advance(&self, _c: u16) -> FmResult<i32> {
        Ok(0)
    }
    fn is_bold(&self) -> bool {
        self.cached.is_bold()
    }
    fn is_italic(&self) -> bool {
        self.cached.is_italic()
    }
    fn get_em_scale(&self) -> f64 {
        self.em_scale
    }
    fn get_copyright(&self) -> Option<String> {
        self.cached.copyright.borrow().clone()
    }
    fn set_copyright(&self, c: Option<String>) {
        *self.cached.copyright.borrow_mut() = c;
    }
    fn get_trademark(&self) -> Option<String> {
        self.cached.trademark.borrow().clone()
    }
    fn set_trademark(&self, t: Option<String>) {
        *self.cached.trademark.borrow_mut() = t;
    }
    fn get_fs_type(&self) -> Option<FSType> {
        self.cached.fs_type.borrow().clone()
    }
    fn set_fs_type(&self, t: Option<FSType>) {
        *self.cached.fs_type.borrow_mut() = t;
    }
    fn get_postscript_name(&self) -> FmResult<String> {
        Ok(self.font.get_ps_name())
    }
    fn properties(&self) -> &RefCell<HashMap<String, Rc<dyn Any>>> {
        &self.cached.properties
    }
}
