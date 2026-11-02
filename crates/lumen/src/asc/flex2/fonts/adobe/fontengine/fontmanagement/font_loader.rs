//! Port of afe.jar `com.adobe.fontengine.fontmanagement.FontLoader` for files: format sniffing, OpenType/TTC loading
//! and the resolver factories (Type 1, metric files, Mac resource fonts and the font caches are not ported).

use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::font_factory::FontFactory as CffFontFactory;
use crate::asc::flex2::fonts::adobe::fontengine::font::font::Font;
use crate::asc::flex2::fonts::adobe::fontengine::font::font_data::FontData;
use crate::asc::flex2::fonts::adobe::fontengine::font::font_input_stream::FontInputStream;
use crate::asc::flex2::fonts::adobe::fontengine::font::mac::font_factory::FontFactory as MacFontFactory;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::font_factory::FontFactory as OtFontFactory;
use crate::asc::flex2::fonts::adobe::fontengine::font::type1::font_factory::FontFactory as T1FontFactory;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{ExceptionKind, FontEngineException, FontResult};
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::directory_walker::DirectoryWalker;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::fxg::fxg_font_resolver_impl::FXGFontResolverImpl;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform::platform_font_resolver_impl::PlatformFontResolverImpl;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::url_font::URLFont;
use crate::asc::flex2::fonts::adobe::fontengine::inlineformatting::css20::family_name_normalizer::FamilyNameNormalizer;

const K_TYPE1_FONT: i32 = 1;
const K_OPEN_TYPE_FONT: i32 = 2;
const K_CFF_FONT: i32 = 3;
const K_PFM: i32 = 4;
const K_AFM: i32 = 5;
const K_MAC_DFONT: i32 = 6;
const K_MAC_RSRC_FONT: i32 = 7;

fn max_bytes_needed() -> i32 {
    [
        T1FontFactory::get_num_bytes_needed_to_identify_pfm(),
        OtFontFactory::get_num_bytes_needed_to_identify(),
        CffFontFactory::get_num_bytes_needed_to_identify(),
        T1FontFactory::get_num_bytes_needed_to_identify(),
        T1FontFactory::get_num_bytes_needed_to_identify_afm(),
        MacFontFactory::get_num_bytes_needed_to_identify(),
    ]
    .into_iter()
    .max()
    .unwrap()
}

/// `file.toURL()` (`escaped` false) or `file.toURI().toURL()` (`escaped` true) of an absolute path.
pub fn java_file_url(path: &Path, escaped: bool) -> String {
    let abs = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().map(|d| d.join(path)).unwrap_or_else(|_| path.to_path_buf()) };
    let mut s = abs.to_string_lossy().replace('\\', "/");
    if !s.starts_with('/') {
        s.insert(0, '/');
    }
    if abs.is_dir() && !s.ends_with('/') {
        s.push('/');
    }
    if escaped {
        s = s.replace('%', "%25").replace(' ', "%20");
    }
    format!("file:{}", s)
}

/// `url.openStream()` on a file URL; Java reads a directory as its listing, which no format matches.
fn open_stream(path: &Path) -> Result<Vec<u8>, String> {
    if path.is_dir() {
        return Ok(Vec::new());
    }
    std::fs::read(path).map_err(|_| format!("java.io.FileNotFoundException: {} (The system cannot find the file specified)", path.to_string_lossy().replace('/', "\\")))
}

fn not_ported(what: &str) -> FontEngineException {
    FontEngineException::unsupported_font(format!("{} are not supported by this port", what))
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FontLoader;

impl FontLoader {
    pub fn new() -> FontLoader {
        FontLoader
    }

    pub fn get_platform_font_resolver_instance() -> PlatformFontResolverImpl {
        PlatformFontResolverImpl::new()
    }

    pub fn get_platform_font_resolver_instance_with(n: Option<Rc<dyn FamilyNameNormalizer>>) -> PlatformFontResolverImpl {
        PlatformFontResolverImpl::with_normalizer(n)
    }

    pub fn get_fxg_font_resolver_instance() -> FXGFontResolverImpl {
        FXGFontResolverImpl::new()
    }

    fn determine_file_type(fis: &mut FontInputStream, url: Option<(&str, &Path)>) -> FontResult<i32> {
        let mut n = -1;
        let mut b = vec![0u8; max_bytes_needed() as usize];
        let n2 = fis.read_into(&mut b);
        if n2 >= OtFontFactory::get_num_bytes_needed_to_identify() && OtFontFactory::is_open_type(&b) {
            n = K_OPEN_TYPE_FONT;
        } else if n2 >= T1FontFactory::get_num_bytes_needed_to_identify() && T1FontFactory::is_type1(&b) {
            n = K_TYPE1_FONT;
        } else if n2 >= T1FontFactory::get_num_bytes_needed_to_identify_pfm() && T1FontFactory::is_pfm(&b) {
            n = K_PFM;
        } else if n2 >= T1FontFactory::get_num_bytes_needed_to_identify_afm() && T1FontFactory::is_afm(&b) {
            n = K_AFM;
        } else if n2 >= CffFontFactory::get_num_bytes_needed_to_identify() && CffFontFactory::is_cff(&b) {
            n = K_CFF_FONT;
        } else if let Some((u, p)) = url {
            match MacFontFactory::is_resource_font(&b, u, p) {
                2 => n = K_MAC_DFONT,
                1 => n = K_MAC_RSRC_FONT,
                _ => {}
            }
        }
        if n2 > 0 {
            fis.unread(n2);
        }
        Ok(n)
    }

    /// `loadFont(url, ..., fonts, rawData)`: the OpenType faces of the file (or nothing for formats without outlines).
    fn load_font(url: &str, path: &Path, raw: bool) -> FontResult<(Vec<Rc<dyn Font>>, Vec<Rc<dyn FontData>>)> {
        let data = open_stream(path).map_err(|e| FontEngineException::from_cause(ExceptionKind::Io, e))?;
        let mut fis = FontInputStream::new(data);
        let mut fonts: Vec<Rc<dyn Font>> = Vec::new();
        let mut datas: Vec<Rc<dyn FontData>> = Vec::new();
        match Self::determine_file_type(&mut fis, Some((url, path)))? {
            K_TYPE1_FONT => return Err(not_ported("Type 1 fonts")),
            K_OPEN_TYPE_FONT => {
                for (i, f) in OtFontFactory::load(&mut fis)?.into_iter().enumerate() {
                    if raw {
                        datas.push(f);
                    } else {
                        fonts.push(Rc::new(URLFont::with_data(url.to_string(), path.to_path_buf(), i as i32, f)));
                    }
                }
            }
            K_MAC_DFONT | K_MAC_RSRC_FONT => return Err(not_ported("Mac resource fonts")),
            _ => {}
        }
        Ok((fonts, datas))
    }

    /// `load(URL)` for a file's URL: one `Font` per face (TTC faces in order).
    pub fn load_url(&self, path: &Path) -> FontResult<Vec<Rc<dyn Font>>> {
        let url = java_file_url(path, false);
        let data = open_stream(path).map_err(|e| FontEngineException::from_cause(ExceptionKind::FontLoading, e))?;
        let mut fis = FontInputStream::new(data);
        match Self::determine_file_type(&mut fis, Some((&url, path)))? {
            K_OPEN_TYPE_FONT => {
                let a = OtFontFactory::load(&mut fis)?;
                Ok(a.into_iter().enumerate().map(|(i, f)| Rc::new(URLFont::with_data(url.clone(), path.to_path_buf(), i as i32, f)) as Rc<dyn Font>).collect())
            }
            K_TYPE1_FONT => Ok(vec![Rc::new(URLFont::new(url, path.to_path_buf(), 0)) as Rc<dyn Font>]),
            K_MAC_DFONT | K_MAC_RSRC_FONT => Err(not_ported("Mac resource fonts")),
            _ => Ok(Vec::new()),
        }
    }

    /// `load(File, recurse, exceptions)`: every font under `path`; failures go to `errors`.
    pub fn load_file(&self, path: &Path, recurse: bool, mut errors: Option<&mut Vec<FontEngineException>>) -> Vec<Rc<dyn Font>> {
        let mut w = DirectoryWalker::new(path, recurse);
        let mut out = Vec::new();
        while let Some(f) = w.get_next_file() {
            let url = java_file_url(&f, true);
            match Self::load_font(&url, &f, false) {
                Ok((fonts, _)) => out.extend(fonts),
                Err(e) => {
                    if let Some(l) = errors.as_deref_mut() {
                        if e.kind == ExceptionKind::Io {
                            l.push(FontEngineException::from_cause(ExceptionKind::FontLoading, e.to_java_string_io()));
                        } else {
                            l.push(e);
                        }
                    }
                }
            }
        }
        out
    }

    /// `fromURL(url, null)`: the raw font data of a file.
    pub fn from_url(url: &str, path: &Path) -> FontResult<Vec<Rc<dyn FontData>>> {
        match Self::load_font(url, path, true) {
            Ok((_, d)) => Ok(d),
            Err(e) if e.kind == ExceptionKind::Io => Err(FontEngineException::from_cause(ExceptionKind::FontLoading, e.to_java_string_io())),
            Err(e) => Err(e),
        }
    }

    /// The font file paths a directory walk would visit, for callers that list before loading.
    pub fn walk(path: &Path, recurse: bool) -> Vec<PathBuf> {
        let mut w = DirectoryWalker::new(path, recurse);
        let mut out = Vec::new();
        while let Some(f) = w.get_next_file() {
            out.push(f);
        }
        out
    }
}

trait IoText {
    fn to_java_string_io(&self) -> String;
}

impl IoText for FontEngineException {
    /// The `IOException`'s own `toString()`, which `new FontLoadingException(ioException)` takes as its message.
    fn to_java_string_io(&self) -> String {
        self.message.clone().unwrap_or_else(|| "java.io.IOException".to_string())
    }
}
