//! Port of swfutils `flash.fonts.BatikFontManager`: its delegation to the JRE manager, and its own `.ttf` path
//! through Flex's Batik `svggen.font.Font`. Only Batik's table directory, `name` and `OS/2` reading is ported:
//! mxmlc gets here for a font file only when afe.jar finds no font in it (or, before Flex 4, no face of the
//! requested style), and a `BatikFontFace` that is then used reports itself as not ported.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::asc::flex2::flash::swf::tags::TagRef;
use crate::asc::flex2::fonts::cached_font_manager::{self, CachedFontManagerBase, FileKey};
use crate::asc::flex2::fonts::font_description::{FontDescription, JUrl};
use crate::asc::flex2::fonts::cached_font_face::guess_style_from_sub_family_name;
use crate::asc::flex2::fonts::font_face::{FaceRef, FontFace};
use crate::asc::flex2::fonts::fs_type::FSType;
use crate::asc::flex2::fonts::glyph_entry::GlyphEntryRef;
use crate::asc::flex2::fonts::font_manager::{FmResult, FontManager, FontManagerBase, InitMapRef, JavaException};
use crate::asc::flex2::fonts::font_set::FontSet;
use crate::asc::flex2::fonts::jre_font_manager::JREFontManager;

pub struct BatikFontManager {
    base: FontManagerBase,
    cached: CachedFontManagerBase,
    jre_manager: RefCell<Option<Rc<JREFontManager>>>,
    initialize_map: RefCell<InitMapRef>,
}

impl Default for BatikFontManager {
    fn default() -> Self {
        Self::new()
    }
}

/// `processLocation(location)` for a `URL`: its `getFile()` when it is a `file` URL.
pub fn process_location(url: &JUrl) -> Option<String> {
    if url.get_protocol().to_lowercase().contains("file") {
        Some(url.external_form["file:".len()..].to_string())
    } else {
        None
    }
}

impl BatikFontManager {
    pub fn new() -> BatikFontManager {
        BatikFontManager { base: FontManagerBase::default(), cached: CachedFontManagerBase::default(), jre_manager: RefCell::new(None), initialize_map: RefCell::new(None) }
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

    /// `createFontFromLocation(location, requestedStyle, useTwips)`. The face is registered under the style Batik
    /// guesses; only a face that is then looked up is a problem, as its glyph code is not ported.
    fn create_font_from_location(&self, url: &JUrl, requested_style: i32, _use_twips: bool) -> FmResult<Option<String>> {
        let Some(path) = process_location(url) else { return Ok(None) };
        let unexpected = || JavaException::runtime(format!("Unexpected exception encountered while reading font file '{path}'"));
        let font = BatikFont::create(Some(&path));
        if !font.is_complete() {
            return Err(unexpected());
        }
        let family = font.name_record(1).map_err(|_| unexpected())?;
        let sub_family = font.name_record(2).map_err(|_| unexpected())?;
        let mut style = guess_style_from_sub_family_name(Some(&sub_family));
        if self.base.major_compatibility_version.get() > 3 {
            style = requested_style;
        }
        let set = match self.cached.font_cache_get(&family) {
            Some(s) => s,
            None => {
                let s = Rc::new(RefCell::new(FontSet::new(self.cached.max_faces_per_font as i32)));
                self.cached.font_cache_put(&family, s.clone());
                s
            }
        };
        set.borrow_mut().put(style, Rc::new(UnportedBatikFontFace { path, properties: RefCell::new(HashMap::new()) }));
        Ok(Some(family))
    }
}

/// A `BatikFontFace` whose glyph and metric code (Batik's own TrueType reader) is not ported; any use fails.
struct UnportedBatikFontFace {
    path: String,
    properties: RefCell<HashMap<String, Rc<dyn Any>>>,
}

impl UnportedBatikFontFace {
    fn err<T>(&self) -> FmResult<T> {
        Err(JavaException::runtime(format!("BatikFontFace is not ported: '{}' is a TrueType font afe.jar could not load", self.path)))
    }
}

impl FontFace for UnportedBatikFontFace {
    fn get_glyph_entry(&self, _c: u16) -> FmResult<Option<GlyphEntryRef>> {
        self.err()
    }
    fn get_missing_glyph_code(&self) -> i32 {
        0
    }
    fn get_point_size(&self) -> f64 {
        1.0
    }
    fn get_family(&self) -> FmResult<String> {
        self.err()
    }
    fn get_ascent(&self) -> i32 {
        0
    }
    fn get_descent(&self) -> i32 {
        0
    }
    fn get_line_gap(&self) -> i32 {
        0
    }
    fn get_first_char(&self) -> FmResult<i32> {
        self.err()
    }
    fn get_num_glyphs(&self) -> FmResult<i32> {
        self.err()
    }
    fn can_display(&self, _c: u16) -> FmResult<bool> {
        self.err()
    }
    fn get_advance(&self, _c: u16) -> FmResult<i32> {
        self.err()
    }
    fn is_bold(&self) -> bool {
        false
    }
    fn is_italic(&self) -> bool {
        false
    }
    fn get_em_scale(&self) -> f64 {
        1.0
    }
    fn get_copyright(&self) -> Option<String> {
        None
    }
    fn set_copyright(&self, _c: Option<String>) {}
    fn get_trademark(&self) -> Option<String> {
        None
    }
    fn set_trademark(&self, _t: Option<String>) {}
    fn get_fs_type(&self) -> Option<FSType> {
        None
    }
    fn set_fs_type(&self, _t: Option<FSType>) {}
    fn get_postscript_name(&self) -> FmResult<String> {
        self.err()
    }
    fn properties(&self) -> &RefCell<HashMap<String, Rc<dyn Any>>> {
        &self.properties
    }
}

impl FontManager for BatikFontManager {
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

    fn get_entry_from_location(&self, location: &JUrl, requested_style: i32, use_twips: bool) -> FmResult<Option<FaceRef>> {
        if !location.external_form.to_lowercase().ends_with(".ttf") {
            return match self.base.parent() {
                Some(p) => p.get_entry_from_location(location, requested_style, use_twips),
                None => Ok(None),
            };
        }
        let mut entry = None;
        let key = location.external_form.clone();
        let mut fam = self.cached.font_file_cache_get(&FileKey::Str(key.clone()));
        if fam.is_none() {
            fam = self.create_font_from_location(location, requested_style, use_twips)?;
        }
        match fam {
            Some(family) => {
                if let Some(set) = self.cached.font_cache_get(&family) {
                    entry = set.borrow().get(requested_style);
                }
                self.cached.font_file_cache_put(FileKey::Str(key), family);
            }
            None => {
                if let Some(p) = self.base.parent() {
                    return p.get_entry_from_location(location, requested_style, use_twips);
                }
            }
        }
        Ok(entry)
    }
}

/// What JREFontManager and this manager read through Batik's `svggen.font.Font`: the table directory, the `name`
/// records (decoded as Batik's `NameRecord.loadString` does) and the `OS/2` licence type.
pub struct BatikFont {
    tables: Vec<([u8; 4], usize, usize)>,
    data: Vec<u8>,
}

impl BatikFont {
    /// `Font.create(path)`: a missing or unreadable file leaves every table null.
    pub fn create(path: Option<&str>) -> BatikFont {
        let data = path.and_then(|p| std::fs::read(java_file_path(p)).ok()).unwrap_or_default();
        let mut tables = Vec::new();
        if data.len() >= 12 {
            let n = u16::from_be_bytes([data[4], data[5]]) as usize;
            for i in 0..n {
                let o = 12 + 16 * i;
                let Some(e) = data.get(o..o + 16) else {
                    tables.clear();
                    break;
                };
                let off = u32::from_be_bytes([e[8], e[9], e[10], e[11]]) as usize;
                let len = u32::from_be_bytes([e[12], e[13], e[14], e[15]]) as usize;
                if off.checked_add(len).is_none_or(|end| end > data.len()) {
                    tables.clear();
                    break;
                }
                tables.push(([e[0], e[1], e[2], e[3]], off, len));
            }
        }
        BatikFont { tables, data }
    }

    fn table(&self, tag: &[u8; 4]) -> Option<&[u8]> {
        self.tables.iter().find(|t| &t.0 == tag).map(|&(_, o, l)| &self.data[o..o + l])
    }

    /// Whether `Font.read`, `FSType.getFSType` and the `BatikFontFace` constructor get through without a
    /// `NullPointerException` (the tables they dereference are all there).
    pub fn is_complete(&self) -> bool {
        [b"OS/2", b"cmap", b"glyf", b"head", b"hhea", b"hmtx", b"loca", b"maxp", b"name", b"post"].iter().all(|t| self.table(t).is_some())
    }

    /// `getOS2Table().getLicenseType()`; `Err` for the NPE of a missing table.
    pub fn os2_license_type(&self) -> FmResult<i32> {
        let t = self.table(b"OS/2").ok_or_else(JavaException::null_pointer)?;
        Ok(t.get(8..10).map_or(0, |b| i16::from_be_bytes([b[0], b[1]]) as i32))
    }

    /// `getNameTable().getRecord(nameId)`: the first record with that id, or "".
    pub fn name_record(&self, name_id: i16) -> FmResult<String> {
        let t = self.table(b"name").ok_or_else(JavaException::null_pointer)?;
        let rd = |o: usize| t.get(o..o + 2).map_or(0, |b| i16::from_be_bytes([b[0], b[1]]));
        let n = rd(2);
        let storage = rd(4) as i32;
        for i in 0..n.max(0) as usize {
            let r = 6 + 12 * i;
            if rd(r + 6) != name_id {
                continue;
            }
            let platform = rd(r);
            let length = rd(r + 8) as i32;
            let offset = rd(r + 10) as i32;
            let start = (storage + offset) as usize;
            let bytes = t.get(start..).unwrap_or(&[]);
            let mut s = String::new();
            match platform {
                0 | 3 => {
                    let units: Vec<u16> = (0..(length / 2).max(0) as usize).filter_map(|k| bytes.get(2 * k..2 * k + 2)).map(|b| u16::from_be_bytes([b[0], b[1]])).collect();
                    s = String::from_utf16_lossy(&units);
                }
                1 | 2 => {
                    let units: Vec<u16> = (0..length.max(0) as usize).filter_map(|k| bytes.get(k)).map(|&b| b as i8 as i16 as u16).collect();
                    s = String::from_utf16_lossy(&units);
                }
                _ => {}
            }
            return Ok(s);
        }
        Ok(String::new())
    }
}

/// `new File(url.getFile())` on Windows: `/C:/x` names `C:\x`.
fn java_file_path(p: &str) -> String {
    let b = p.as_bytes();
    if b.len() > 2 && b[0] == b'/' && b[2] == b':' {
        p[1..].to_string()
    } else {
        p.to_string()
    }
}
