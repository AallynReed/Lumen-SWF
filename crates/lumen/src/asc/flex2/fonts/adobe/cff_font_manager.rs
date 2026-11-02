//! Port of flex-fontkit.jar `flash.fonts.CFFFontManager` (+ `CodePointIterator`, `DefineFont4Cache`,
//! `FontManagerException`): DefineFont4 from afe's `SWFFont4Description`.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

use crate::asc::flex2::flash::swf::tags::{codes, Body, DefineFont4, Tag, TagRef};
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::font::Font;
use crate::asc::flex2::fonts::adobe::fontengine::font::swf_font4_description::{FTESubsetCompletionIterator, SWFFont4Description, TLFSubsetCompletionIterator};
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::font_loader::FontLoader;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::fxg::fxg_font_resolver::FXGFontResolver;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::fxg::fxg_font_resolver_impl::FXGFontResolverImpl;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::fxg::fxg_font_search_attributes::FXGFontSearchAttributes;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform::platform_font_resolver::PlatformFontResolver;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform::platform_font_resolver_impl::PlatformFontResolverImpl;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform::platform_font_search_attributes::PlatformFontSearchAttributes;

type FontRef = Rc<dyn Font>;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontEngineException;
use crate::asc::flex2::fonts::font_description::{FontDescription, FontSource, JUrl};
use crate::asc::flex2::fonts::font_face::FaceRef;
use crate::asc::flex2::fonts::font_manager::{is_bold, is_italic, java_parse_int, FmResult, FontManager, FontManagerBase, InitMapRef, JavaException};
use crate::asc::flex2::fonts::lru_cache::LRUCache;

const MAX_CACHED_FONTS_KEY: &str = "max-cached-fonts";
const DEFAULT_MAX_CACHE_SIZE: i32 = 10;

pub struct CFFFontManager {
    base: FontManagerBase,
    loader: RefCell<FontLoader>,
    platform_resolver: RefCell<PlatformFontResolverImpl>,
    legacy_resolver: RefCell<FXGFontResolverImpl>,
    local_font_paths: RefCell<Option<Vec<String>>>,
    local_paths_loaded: Cell<bool>,
    define_font4_cache: RefCell<Option<LRUCache<FontDescription, Option<TagRef>>>>,
}

impl Default for CFFFontManager {
    fn default() -> Self {
        Self::new()
    }
}

/// `new FontManagerException(message)`.
fn font_manager_exception(message: String) -> JavaException {
    JavaException::new("flash.fonts.CFFFontManager$FontManagerException", Some(message))
}

impl CFFFontManager {
    pub fn new() -> CFFFontManager {
        CFFFontManager {
            base: FontManagerBase::default(),
            loader: RefCell::new(FontLoader::new()),
            platform_resolver: RefCell::new(FontLoader::get_platform_font_resolver_instance()),
            legacy_resolver: RefCell::new(FontLoader::get_fxg_font_resolver_instance()),
            local_font_paths: RefCell::new(None),
            local_paths_loaded: Cell::new(false),
            define_font4_cache: RefCell::new(None),
        }
    }

    /// The `catch (FontLoadingException | UnsupportedFontException | InvalidFontException | IOException)` arms.
    fn not_usable(desc: &FontDescription, e: &FontEngineException) -> JavaException {
        let source = desc.source.as_ref().map_or("null".to_string(), |s| s.to_string());
        font_manager_exception(format!("The font {} is not usable: {}", source, e.get_message().unwrap_or("null")))
    }

    fn create_define_font4(&self, desc: &mut FontDescription, d4: &dyn SWFFont4Description) -> Result<TagRef, RangeOrFont> {
        if desc.alias.is_none() {
            desc.alias = d4.get_family().map_err(RangeOrFont::Font)?;
        }
        let mut out: Vec<u8> = Vec::with_capacity(4096);
        match desc.unicode_ranges.clone() {
            Some(r) => {
                let chars = self.get_code_point_iterator(&r)?;
                d4.stream_font_data(&mut chars.into_iter(), &mut out).map_err(RangeOrFont::Font)?;
            }
            None => d4.stream_font_data_all(&mut out).map_err(RangeOrFont::Font)?,
        }
        let f = DefineFont4 { has_font_data: true, small_text: false, italic: is_italic(desc.style), bold: is_bold(desc.style), lang_code: 0, font_name: desc.alias.clone(), data: out, license: None };
        Ok(Tag::new_ref(codes::DEFINE_FONT4, Body::DefineFont4(f)))
    }

    /// `getCodePointIterator(ranges)`: `CodePointIterator`, then the TLF and FTE completion iterators.
    fn get_code_point_iterator(&self, s: &str) -> Result<Vec<i32>, RangeOrFont> {
        let ranges = self.get_unicode_ranges(Some(s)).map_err(RangeOrFont::Range)?;
        let base = code_point_iterator(ranges.as_deref());
        Ok(FTESubsetCompletionIterator::new(TLFSubsetCompletionIterator::new(base)).collect())
    }

    fn load_local_fonts(&self) {
        let Some(paths) = self.local_font_paths.borrow().clone() else { return };
        self.local_paths_loaded.set(true);
        let mut files = Vec::new();
        for p in &paths {
            get_local_font_files(Path::new(p), &mut files);
        }
        let mut errors = Vec::new();
        for f in &files {
            let fonts = self.loader.borrow().load_file(f, false, Some(&mut errors));
            for font in fonts {
                if self.platform_resolver.borrow_mut().add_font(font.clone()).is_ok() {
                    let _ = self.legacy_resolver.borrow_mut().add_font(font);
                }
            }
        }
    }

    fn find_and_build(&self, desc: &mut FontDescription) -> Result<Option<TagRef>, RangeOrFont> {
        let bold = is_bold(desc.style);
        let italic = is_italic(desc.style);
        match desc.source.clone() {
            Some(FontSource::Name(name)) => {
                if self.local_font_paths.borrow().is_none() {
                    return Err(RangeOrFont::Range(font_manager_exception(format!("Cannot embed local font '{name}' as CFF. The CSS @font-face 'local()' syntax is not supported. Please specify a path directly to a font file using the 'url()' syntax. For [Embed] syntax the 'systemFont' attribute is not supported. Please specify a path directly to a font file using the 'source' attribute."))));
                }
                if !self.local_paths_loaded.get() {
                    self.load_local_fonts();
                }
                let mut font = self.platform_resolver.borrow().find_font(&PlatformFontSearchAttributes::new(name.clone()));
                if font.is_none() {
                    font = self.legacy_resolver.borrow().find_font(&FXGFontSearchAttributes::new(name.clone(), bold, italic));
                }
                match font {
                    Some(f) => {
                        let d4 = f.get_swf_font4_description().map_err(RangeOrFont::Font)?;
                        Ok(Some(self.create_define_font4(desc, &*d4)?))
                    }
                    None => Ok(None),
                }
            }
            Some(FontSource::Url(url)) => {
                let fonts: Vec<FontRef> = self.loader.borrow().load_url(Path::new(&url.path)).map_err(RangeOrFont::Font)?;
                if fonts.is_empty() {
                    return Ok(None);
                }
                let font = if fonts.len() == 1 {
                    fonts[0].clone()
                } else {
                    let mut r = FontLoader::get_fxg_font_resolver_instance();
                    for f in &fonts {
                        r.add_font(f.clone()).map_err(RangeOrFont::Font)?;
                    }
                    let alias = desc.alias.clone().unwrap_or_default();
                    match r.find_font(&FXGFontSearchAttributes::new(alias, bold, italic)) {
                        Some(f) => f,
                        None => {
                            let family = fonts[0].get_swf_font4_description().map_err(RangeOrFont::Font)?.get_family().map_err(RangeOrFont::Font)?.unwrap_or_default();
                            r.find_font(&FXGFontSearchAttributes::new(family, bold, italic)).unwrap_or_else(|| fonts[0].clone())
                        }
                    }
                };
                let d4 = font.get_swf_font4_description().map_err(RangeOrFont::Font)?;
                Ok(Some(self.create_define_font4(desc, &*d4)?))
            }
            None => Err(RangeOrFont::Range(JavaException::null_pointer())),
        }
    }
}

/// A Java runtime exception, or an afe one the catch arms turn into `FontManagerException`.
enum RangeOrFont {
    Range(JavaException),
    Font(FontEngineException),
}

/// `CodePointIterator` over `getUnicodeRanges`' rows, drained.
pub fn code_point_iterator(chars: Option<&[[i32; 2]]>) -> Vec<i32> {
    let mut out = Vec::new();
    let Some(chars) = chars else { return out };
    let (mut i, row_count) = (0usize, chars.len());
    let (mut c, mut row_end) = (0i32, 0i32);
    if row_count > 0 {
        row_end = chars[0][1];
        c = chars[0][0];
    }
    while i < row_count && c <= row_end {
        out.push(c);
        c = c.wrapping_add(1);
        if c > row_end {
            i += 1;
            if i < row_count {
                row_end = chars[i][1];
                c = chars[i][0];
            }
        }
    }
    out
}

fn get_local_font_files(file: &Path, list: &mut Vec<PathBuf>) {
    if file.is_dir() {
        let Ok(rd) = std::fs::read_dir(file) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                get_local_font_files(&p, list);
            } else if accept_file(&e.file_name().to_string_lossy()) {
                list.push(p);
            }
        }
    } else {
        list.push(file.to_path_buf());
    }
}

fn accept_file(s: &str) -> bool {
    let s = s.to_lowercase();
    s.ends_with(".ttf") || s.ends_with(".otf") || s.ends_with(".dfont") || s.ends_with(".ttc")
}

impl FontManager for CFFFontManager {
    fn base(&self) -> &FontManagerBase {
        &self.base
    }

    fn initialize(&self, map: &InitMapRef) {
        let mut n = DEFAULT_MAX_CACHE_SIZE;
        if let Some(m) = map {
            *self.local_font_paths.borrow_mut() = m.local_font_paths.clone();
            let _ = MAX_CACHED_FONTS_KEY;
            if let Some(v) = m.max_cached_fonts.as_deref().and_then(|s| java_parse_int(s, 10)) {
                n = v;
            }
        }
        *self.define_font4_cache.borrow_mut() = Some(LRUCache::new(n / 2, n, 1));
    }

    fn create_define_font(&self, n: i32, desc: &mut FontDescription) -> FmResult<Option<TagRef>> {
        if !desc.compact_font_format {
            return match self.base.parent() {
                Some(p) => p.create_define_font(n, desc),
                None => Ok(None),
            };
        }
        let key = desc.clone();
        let hit = self.define_font4_cache.borrow_mut().as_mut().and_then(|c| c.get_or_fetch(&key, || None));
        if let Some(t) = hit {
            let mut f = match &t.borrow().body {
                Body::DefineFont4(f) => f.clone(),
                _ => unreachable!(),
            };
            if let Some(a) = &desc.alias {
                f.font_name = Some(a.clone());
            }
            return Ok(Some(Tag::new_ref(codes::DEFINE_FONT4, Body::DefineFont4(f))));
        }
        let tag = match self.find_and_build(desc) {
            Ok(t) => t,
            Err(RangeOrFont::Range(j)) => return Err(j),
            Err(RangeOrFont::Font(e)) => return Err(Self::not_usable(desc, &e)),
        };
        if let Some(t) = &tag {
            if let Some(c) = self.define_font4_cache.borrow_mut().as_mut() {
                c.put(key, Some(t.clone()));
            }
        }
        Ok(tag)
    }

    fn get_entry_from_location(&self, url: &JUrl, n: i32, use_twips: bool) -> FmResult<Option<FaceRef>> {
        match self.base.parent() {
            Some(p) => p.get_entry_from_location(url, n, use_twips),
            None => Ok(None),
        }
    }

    fn get_entry_from_system(&self, s: &str, n: i32, use_twips: bool) -> FmResult<Option<FaceRef>> {
        match self.base.parent() {
            Some(p) => p.get_entry_from_system(s, n, use_twips),
            None => Ok(None),
        }
    }
}
