//! Port of flex-fontkit.jar `flash.fonts.AFEFontManager` (+ `AFEFontFace`, `AFE2SWFOutlineAdaptor`,
//! `AFEFMFontException`, `BatikFailsException`): DefineFont3 faces from afe's `SWFFontDescription`.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::asc::flex2::flash::swf::builder::shape_builder::{ShapeBuilder, ShapeIterator};
use crate::asc::flex2::flash::swf::types::Shape;
use crate::asc::flex2::fonts::adobe::fontengine::font::swf_font_description::SWFFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::font_loader::FontLoader;
use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;
use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::permission::Permission;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontEngineException;
use crate::asc::flex2::fonts::cached_font_face::{guess_style_from_sub_family_name, CachedFontFace};
use crate::asc::flex2::fonts::cached_font_manager::{self, CachedFontManagerBase, FileKey};
use crate::asc::flex2::fonts::font_description::{FontDescription, JUrl};
use crate::asc::flex2::fonts::font_face::{FaceRef, FontFace};
use crate::asc::flex2::fonts::font_manager::{FmResult, FontManager, FontManagerBase, InitMapRef, JavaException};
use crate::asc::flex2::fonts::font_set::FontSet;
use crate::asc::flex2::fonts::fs_type::FSType;
use crate::asc::flex2::fonts::glyph_entry::{GlyphEntry, GlyphEntryRef};
use crate::asc::flex2::fonts::jre_font_manager::JREFontManager;
use crate::asc::flex2::flash::swf::tags::TagRef;

pub struct AFEFontManager {
    base: FontManagerBase,
    cached: CachedFontManagerBase,
    loader: RefCell<FontLoader>,
    jre_manager: RefCell<Option<Rc<JREFontManager>>>,
    initialize_map: RefCell<InitMapRef>,
}

impl Default for AFEFontManager {
    fn default() -> Self {
        Self::new()
    }
}

/// `new AFEFMFontException(url, e)`.
fn afefm_font_exception(url: &JUrl) -> JavaException {
    JavaException::new("flash.fonts.AFEFontManager$AFEFMFontException", Some(format!("The font {} is not usable.", url.external_form)))
}

impl AFEFontManager {
    pub fn new() -> AFEFontManager {
        AFEFontManager { base: FontManagerBase::default(), cached: CachedFontManagerBase::default(), loader: RefCell::new(FontLoader::new()), jre_manager: RefCell::new(None), initialize_map: RefCell::new(None) }
    }

    fn jre(&self, initialize: bool) -> Rc<JREFontManager> {
        let mut j = self.jre_manager.borrow_mut();
        if j.is_none() {
            let m = Rc::new(JREFontManager::new());
            if initialize {
                m.initialize(&self.initialize_map.borrow());
            }
            *j = Some(m);
        }
        j.clone().unwrap()
    }

    /// `createFontFromLocation(location, style, useTwips)`.
    fn create_font_from_location(&self, url: &JUrl, n: i32, use_twips: bool) -> FmResult<Option<String>> {
        let mut string = None;
        let fonts = match self.loader.borrow().load_url(std::path::Path::new(&url.path)) {
            Ok(f) => f,
            Err(e) => return Err(unexpected(url, &e)),
        };
        let mut candidate: Option<Rc<dyn SWFFontDescription>> = None;
        for (i, font) in fonts.iter().enumerate() {
            let desc = font.get_swf_font_description().map_err(|e| unexpected(url, &e))?;
            let n3 = afe_font_face_get_style(&*desc).map_err(|e| unexpected(url, &e))?;
            if n3 == n {
                string = Some(self.create_afe_font_face(url, n, desc, use_twips)?);
                break;
            }
            if candidate.is_none() {
                if let Some(sub) = desc.get_sub_family().map_err(|e| unexpected(url, &e))? {
                    if guess_style_from_sub_family_name(Some(&sub)) == n {
                        candidate = Some(desc.clone());
                    }
                }
            }
            if string.is_none() {
                if let Some(c) = &candidate {
                    string = Some(self.create_afe_font_face(url, n, c.clone(), use_twips)?);
                    break;
                }
            }
            let last = i == fonts.len() - 1;
            if !last || string.is_some() || candidate.is_some() || self.base.major_compatibility_version.get() <= 3 {
                continue;
            }
            let d0 = fonts[0].get_swf_font_description().map_err(|e| unexpected(url, &e))?;
            candidate = Some(d0.clone());
            string = Some(self.create_afe_font_face(url, n, d0, use_twips)?);
        }
        Ok(string)
    }

    fn create_afe_font_face(&self, url: &JUrl, n: i32, desc: Rc<dyn SWFFontDescription>, use_twips: bool) -> FmResult<String> {
        let face = AFEFontFace::new_instance(self.cached.max_glyphs_per_face.get(), desc, url.clone(), use_twips)?;
        face.cached.style.set(n);
        let family = face.get_family()?;
        let set = match self.cached.font_cache_get(&family) {
            Some(s) => s,
            None => {
                let s = Rc::new(RefCell::new(FontSet::new(self.cached.max_faces_per_font as i32)));
                self.cached.font_cache_put(&family, s.clone());
                s
            }
        };
        set.borrow_mut().put(n, Rc::new(face));
        Ok(family)
    }
}

/// `catch (Throwable t) { throw new RuntimeException("Unexpected exception encountered while reading font file '" + url + "'", t); }`
fn unexpected(url: &JUrl, _cause: &FontEngineException) -> JavaException {
    JavaException::runtime(format!("Unexpected exception encountered while reading font file '{}'", url.external_form))
}

impl FontManager for AFEFontManager {
    fn base(&self) -> &FontManagerBase {
        &self.base
    }

    fn initialize(&self, map: &InitMapRef) {
        self.base.initialize(map);
        self.cached.initialize(map);
        *self.initialize_map.borrow_mut() = map.clone();
    }

    fn create_define_font(&self, tag_code: i32, desc: &mut FontDescription) -> FmResult<Option<TagRef>> {
        cached_font_manager::create_define_font(self, tag_code, desc)
    }

    fn get_entry_from_system(&self, family_name: &str, style: i32, use_twips: bool) -> FmResult<Option<FaceRef>> {
        match self.base.parent() {
            Some(p) => p.get_entry_from_system(family_name, style, use_twips),
            None => self.jre(true).get_entry_from_system(family_name, style, use_twips),
        }
    }

    fn is_cached(&self) -> bool {
        true
    }

    fn create_set_for_system_font(&self, family: &str, style: i32, use_twips: bool) -> FmResult<Option<FontSet>> {
        match self.base.parent() {
            Some(p) if p.is_cached() => p.create_set_for_system_font(family, style, use_twips),
            _ => self.jre(false).create_set_for_system_font(family, style, use_twips),
        }
    }

    fn get_entry_from_location(&self, url: &JUrl, n: i32, use_twips: bool) -> FmResult<Option<FaceRef>> {
        let mut face = None;
        let key = url.external_form.clone();
        let mut family = self.cached.font_file_cache_get(&FileKey::Str(key.clone()));
        if family.is_none() {
            family = self.create_font_from_location(url, n, use_twips)?;
        }
        match family {
            Some(f) => {
                if let Some(set) = self.cached.font_cache_get(&f) {
                    face = set.borrow().get(n);
                }
                let lower = key.to_lowercase();
                if !lower.ends_with(".ttc") && !lower.ends_with(".dfont") {
                    self.cached.font_file_cache_put(FileKey::Str(key), f);
                }
            }
            None => {
                if let Some(p) = self.base.parent() {
                    return p.get_entry_from_location(url, n, use_twips);
                }
            }
        }
        Ok(face)
    }
}

/// `AFE2SWFOutlineAdaptor`: afe outline calls in, `ShapeIterator` segments out (type, x, y[, cx, cy]).
pub struct AFE2SWFOutlineAdaptor {
    scale: Matrix,
    initial_matrix: Matrix,
    segments: Vec<Vec<f64>>,
    index: usize,
    scaled_start_x: f64,
    scaled_start_y: f64,
    shape_started: bool,
    x1: f64,
    y1: f64,
    variable_quad_approximation: bool,
    throw_when_batik_fails: bool,
    /// A `BatikFailsException` thrown out of the outline callbacks.
    batik_fails: Option<f64>,
}

impl AFE2SWFOutlineAdaptor {
    /// `new AFE2SWFOutlineAdaptor(c, desc, n, reverse, variableQuad, throwWhenBatikFails)`; `reverse` is never set by
    /// the embedding path, so `reversePoints` is not ported.
    pub fn new(c: u16, desc: &dyn SWFFontDescription, n: i32, _reverse: bool, variable_quad: bool, throw_when_batik_fails: bool) -> Result<AFE2SWFOutlineAdaptor, AdaptorError> {
        let mut a = AFE2SWFOutlineAdaptor {
            scale: Matrix::IDENTITY_MATRIX,
            initial_matrix: Matrix::new(n as f64, 0.0, 0.0, -n as f64, 0.0, 0.0),
            segments: Vec::new(),
            index: 0,
            scaled_start_x: 0.0,
            scaled_start_y: 0.0,
            shape_started: false,
            x1: 0.0,
            y1: 0.0,
            variable_quad_approximation: variable_quad,
            throw_when_batik_fails,
            batik_fails: None,
        };
        desc.get_outline(c, &mut a).map_err(AdaptorError::Font)?;
        if let Some(d) = a.batik_fails {
            return Err(AdaptorError::Runtime(JavaException::new("flash.fonts.AFEFontManager$BatikFailsException", Some(format!("Font fails with batik: {}", java_double_to_string(d))))));
        }
        Ok(a)
    }

    fn add_segment(&mut self, x: f64, y: f64, n: i32) {
        self.segments.push(vec![n as f64, x, y]);
    }

    fn add_quadratic(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        self.segments.push(vec![2.0, cx, cy, x, y]);
    }

    fn close(&mut self) {
        self.add_segment(self.scaled_start_x, self.scaled_start_y, 1);
        self.shape_started = false;
    }

    fn approximate_cubic_bezier_atm(&mut self, d: f64, d2: f64, d3: f64, d4: f64, d5: f64, d6: f64) {
        let d7 = -self.x1 + 3.0 * d - 3.0 * d3 + d5;
        let d8 = -self.y1 + 3.0 * d2 - 3.0 * d4 + d6;
        let d9 = 3.0 * self.x1 - 6.0 * d + 3.0 * d3;
        let d10 = 3.0 * self.y1 - 6.0 * d2 + 3.0 * d4;
        let d11 = -3.0 * self.x1 + 3.0 * d;
        let d12 = -3.0 * self.y1 + 3.0 * d2;
        let d13 = self.x1;
        let d14 = self.y1;
        let d15 = d7.abs().max(d8.abs()).ceil();
        let d15 = if d7.is_nan() || d8.is_nan() { f64::NAN } else { d15 };
        let mut n: i32 = 0;
        while d15 > (10i32.wrapping_mul(n).wrapping_mul(n).wrapping_mul(n)) as f64 {
            n += 1;
        }
        let d16 = 1.0 / n as f64;
        let mut d17 = d11;
        let mut d18 = d12;
        for i in 1..=n {
            let d19 = i as f64 * d16;
            let d20 = ((d7 * d19 + d9) * d19 + d11) * d19 + d13;
            let d21 = ((d8 * d19 + d10) * d19 + d12) * d19 + d14;
            let d22 = (3.0 * d7 * d19 + 2.0 * d9) * d19 + d11;
            let d23 = (3.0 * d8 * d19 + 2.0 * d10) * d19 + d12;
            let k = (4 * n) as f64;
            self.add_quadratic((self.x1 + d20) / 2.0 + (d17 - d22) / k, (self.y1 + d21) / 2.0 + (d18 - d23) / k, d20, d21);
            self.x1 = d20;
            self.y1 = d21;
            d17 = d22;
            d18 = d23;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn approximate_cubic_bezier_flex(&mut self, d: f64, d2: f64, d3: f64, d4: f64, d5: f64, d6: f64, d7: f64, d8: f64) {
        let d9 = point_on_segment(d, d3, 0.75);
        let d10 = point_on_segment(d2, d4, 0.75);
        let d11 = point_on_segment(d7, d5, 0.75);
        let d12 = point_on_segment(d8, d6, 0.75);
        let d13 = (d7 - d) / 16.0;
        let d14 = (d8 - d2) / 16.0;
        let d15 = point_on_segment(d, d3, 0.375);
        let d16 = point_on_segment(d2, d4, 0.375);
        let d17 = point_on_segment(d9, d11, 0.375) - d13;
        let d18 = point_on_segment(d10, d12, 0.375) - d14;
        let d19 = point_on_segment(d11, d9, 0.375) + d13;
        let d20 = point_on_segment(d12, d10, 0.375) + d14;
        let d21 = point_on_segment(d7, d5, 0.375);
        let d22 = point_on_segment(d8, d6, 0.375);
        let d23 = (d15 + d17) / 2.0;
        let d24 = (d16 + d18) / 2.0;
        let d25 = (d9 + d11) / 2.0;
        let d26 = (d10 + d12) / 2.0;
        let d27 = (d19 + d21) / 2.0;
        let d28 = (d20 + d22) / 2.0;
        self.add_quadratic(d15, d16, d23, d24);
        self.add_quadratic(d17, d18, d25, d26);
        self.add_quadratic(d19, d20, d27, d28);
        self.add_quadratic(d21, d22, d7, d8);
        self.x1 = d7;
        self.y1 = d8;
    }

    fn check_range(&mut self, d: f64, d2: f64) {
        if self.throw_when_batik_fails && self.batik_fails.is_none() {
            if d.abs() > 32767.0 {
                self.batik_fails = Some(d);
            } else if d2.abs() > 32767.0 {
                self.batik_fails = Some(d2);
            }
        }
    }
}

fn point_on_segment(d: f64, d2: f64, d3: f64) -> f64 {
    d + (d2 - d) * d3
}

/// `Double.toString` for the `BatikFailsException` message.
fn java_double_to_string(d: f64) -> String {
    let s = format!("{d:?}");
    if s.contains('e') {
        s.replace('e', "E")
    } else {
        s
    }
}

pub enum AdaptorError {
    Font(FontEngineException),
    Runtime(JavaException),
}

impl OutlineConsumer for AFE2SWFOutlineAdaptor {
    fn set_matrix(&mut self, m: &Matrix) {
        self.scale = m.multiply(&self.initial_matrix);
    }

    fn moveto(&mut self, x: f64, y: f64) {
        if self.shape_started {
            self.close();
        }
        self.check_range(x, y);
        let sx = self.scale.apply_to_xy_get_x(x, y);
        let sy = self.scale.apply_to_xy_get_y(x, y);
        self.scaled_start_x = sx;
        self.scaled_start_y = sy;
        self.x1 = sx;
        self.y1 = sy;
    }

    fn lineto(&mut self, x: f64, y: f64) {
        if !self.shape_started {
            self.add_segment(self.scaled_start_x, self.scaled_start_y, 0);
            self.shape_started = true;
        }
        self.check_range(x, y);
        let sx = self.scale.apply_to_xy_get_x(x, y);
        let sy = self.scale.apply_to_xy_get_y(x, y);
        self.add_segment(sx, sy, 1);
        self.x1 = sx;
        self.y1 = sy;
    }

    fn curveto(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        if !self.shape_started {
            self.add_segment(self.scaled_start_x, self.scaled_start_y, 0);
            self.shape_started = true;
        }
        let a = self.scale.apply_to_xy_get_x(x1, y1);
        let b = self.scale.apply_to_xy_get_y(x1, y1);
        let c = self.scale.apply_to_xy_get_x(x2, y2);
        let d = self.scale.apply_to_xy_get_y(x2, y2);
        let e = self.scale.apply_to_xy_get_x(x3, y3);
        let f = self.scale.apply_to_xy_get_y(x3, y3);
        if self.variable_quad_approximation {
            self.approximate_cubic_bezier_atm(a, b, c, d, e, f);
        } else {
            self.approximate_cubic_bezier_flex(self.x1, self.y1, a, b, c, d, e, f);
        }
    }

    fn curveto_quad(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        if !self.shape_started {
            self.add_segment(self.scaled_start_x, self.scaled_start_y, 0);
            self.shape_started = true;
        }
        self.check_range(x1, y1);
        self.check_range(x2, y2);
        let a = self.scale.apply_to_xy_get_x(x1, y1);
        let b = self.scale.apply_to_xy_get_y(x1, y1);
        let c = self.scale.apply_to_xy_get_x(x2, y2);
        let d = self.scale.apply_to_xy_get_y(x2, y2);
        self.add_quadratic(a, b, c, d);
    }

    fn endchar(&mut self) {}
}

impl ShapeIterator for AFE2SWFOutlineAdaptor {
    fn current_segment(&mut self, coords: &mut [f64; 6]) -> i16 {
        let s = &self.segments[self.index];
        coords[0] = s[1];
        coords[1] = s[2];
        match s[0] as i32 {
            2 => {
                coords[2] = s[3];
                coords[3] = s[4];
            }
            3 => {
                coords[2] = s[3];
                coords[3] = s[4];
                coords[4] = s[5];
                coords[5] = s[6];
            }
            _ => {}
        }
        s[0] as i32 as i16
    }

    fn is_done(&self) -> bool {
        self.index >= self.segments.len()
    }

    fn next(&mut self) {
        self.index += 1;
    }
}

pub struct AFEFontFace {
    pub cached: CachedFontFace,
    font_description: Rc<dyn SWFFontDescription>,
    font_path: JUrl,
    ascent: i16,
    descent: i16,
    line_gap: i16,
    em_scale: f64,
}

/// `AFEFontFace.getStyle(desc)`.
fn afe_font_face_get_style(desc: &dyn SWFFontDescription) -> Result<i32, FontEngineException> {
    let mut n = 0;
    if desc.is_bold()? {
        n += 1;
    }
    if desc.is_italic()? {
        n += 2;
    }
    Ok(n)
}

/// `AFEFontFace.toFSType(permission)`.
fn to_fs_type(p: Permission) -> FSType {
    match p {
        Permission::Editable => FSType::new(0, "Editable embedding", false, true, false, false),
        Permission::Installable => FSType::new(0, "Installable embedding", true, false, false, false),
        Permission::PreviewAndPrint => FSType::new(0, "Preview and Print embedding", false, false, true, false),
        _ => FSType::new(0, "No embedding allowed", false, false, false, true),
    }
}

/// `(short) Math.rint(d)`.
fn rint_short(d: f64) -> i16 {
    d.round_ties_even() as i32 as i16
}

impl AFEFontFace {
    /// `newInstance(maxGlyphs, desc, url, useTwips)`.
    pub fn new_instance(max_glyphs: i16, desc: Rc<dyn SWFFontDescription>, url: JUrl, use_twips: bool) -> FmResult<AFEFontFace> {
        let wrap = |_e: FontEngineException| -> JavaException { afefm_font_exception(&url) };
        let trademark = desc.get_trademark().map_err(wrap)?.unwrap_or_default();
        let copyright = desc.get_copyright().map_err(wrap)?.unwrap_or_default();
        let fs = to_fs_type(desc.get_permissions().map_err(wrap)?);
        let cached = CachedFontFace::with(max_glyphs as i32, 0, Some(fs), Some(copyright), Some(trademark), use_twips);
        let mut f = AFEFontFace { cached, font_description: desc, font_path: url.clone(), ascent: 0, descent: 0, line_gap: 0, em_scale: 0.0 };
        f.init().map_err(wrap)?;
        Ok(f)
    }

    fn init(&mut self) -> Result<(), FontEngineException> {
        self.cached.style.set(afe_font_face_get_style(&*self.font_description)?);
        let t = if self.cached.use_twips { 20.0 } else { 1.0 };
        self.em_scale = 1024.0 / self.font_description.get_em_scale()?;
        self.ascent = rint_short(self.font_description.get_ascent()? * self.em_scale * t);
        self.descent = rint_short(self.font_description.get_descent()? * self.em_scale * t);
        self.line_gap = rint_short(self.font_description.get_line_gap()? * self.em_scale * t);
        Ok(())
    }

    fn fm(&self, _e: FontEngineException) -> JavaException {
        afefm_font_exception(&self.font_path)
    }

    /// `createGlyphEntry(c, c2, false, true, false)`.
    pub fn create_glyph_entry(&self, c: u16, c2: u16) -> FmResult<GlyphEntryRef> {
        let adaptor = match AFE2SWFOutlineAdaptor::new(c2, &*self.font_description, 1024, false, true, false) {
            Ok(a) => a,
            Err(AdaptorError::Font(e)) => return Err(self.fm(e)),
            Err(AdaptorError::Runtime(e)) => return Err(e),
        };
        let shape = self.get_shape_from_glyph(adaptor);
        let t = if self.cached.use_twips { 20.0 } else { 1.0 };
        let advance = (self.get_advance(c2)? as f64 * self.em_scale * t) as i32;
        Ok(Rc::new(RefCell::new(GlyphEntry { advance, character: c, shape: Some(shape), ..Default::default() })))
    }

    fn get_shape_from_glyph(&self, mut adaptor: AFE2SWFOutlineAdaptor) -> Shape {
        let mut b = ShapeBuilder::new(self.cached.use_twips);
        b.set_current_line_style(0);
        b.set_current_fill_style1(1);
        b.set_use_fill_style1(true);
        b.process_shape(&mut adaptor);
        b.build()
    }

    pub fn get_description(&self) -> Rc<dyn SWFFontDescription> {
        self.font_description.clone()
    }
}

impl FontFace for AFEFontFace {
    fn get_glyph_entry(&self, c: u16) -> FmResult<Option<GlyphEntryRef>> {
        self.cached.cached_glyph(c, || self.can_display(c), || self.create_glyph_entry(c, c))
    }
    fn get_missing_glyph_code(&self) -> i32 {
        0
    }
    fn get_point_size(&self) -> f64 {
        1.0
    }
    fn get_family(&self) -> FmResult<String> {
        self.font_description.get_family().map(|f| f.unwrap_or_default()).map_err(|e| self.fm(e))
    }
    fn get_ascent(&self) -> i32 {
        self.ascent as i32
    }
    fn get_descent(&self) -> i32 {
        self.descent as i32
    }
    fn get_line_gap(&self) -> i32 {
        self.line_gap as i32
    }
    fn get_first_char(&self) -> FmResult<i32> {
        self.font_description.get_first_char().map_err(|e| self.fm(e))
    }
    fn get_num_glyphs(&self) -> FmResult<i32> {
        self.font_description.get_num_glyphs().map_err(|e| self.fm(e))
    }
    fn can_display(&self, c: u16) -> FmResult<bool> {
        self.font_description.can_display(c).map_err(|e| self.fm(e))
    }
    fn get_advance(&self, c: u16) -> FmResult<i32> {
        self.font_description.get_horizontal_advance(c).map(|a| a.round_ties_even() as i32).map_err(|e| self.fm(e))
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
        self.font_description.get_postscript_name().map(|n| n.unwrap_or_default()).map_err(|e| self.fm(e))
    }
    fn properties(&self) -> &RefCell<HashMap<String, Rc<dyn Any>>> {
        &self.cached.properties
    }
}
