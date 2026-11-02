//! Port of afe.jar `com.adobe.fontengine.fontmanagement.URLFont`: a font of a file, its data kept once loaded.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_data::FontData;
use crate::asc::flex2::fonts::adobe::fontengine::font::font_impl::FontImpl;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::font_loader::FontLoader;

pub struct URLFont {
    /// The URL's external form (`toString()`).
    pub outline_file_url: String,
    pub path: PathBuf,
    pub metric_file_url: Option<String>,
    pub index: i32,
    pub canonical_path: Option<String>,
    pub length: i64,
    pub last_modified: i64,
    font_ref: RefCell<Option<Rc<dyn FontData>>>,
}

impl URLFont {
    /// `new URLFont(url, index)` (a Type 1 font, loaded on demand).
    pub fn new(outline_file_url: String, path: PathBuf, index: i32) -> URLFont {
        let mut f = URLFont { outline_file_url, path, metric_file_url: None, index, canonical_path: None, length: 0, last_modified: 0, font_ref: RefCell::new(None) };
        f.init_cache_tags();
        f
    }

    /// `new URLFont(url, index, fontData)`.
    pub fn with_data(outline_file_url: String, path: PathBuf, index: i32, data: Rc<dyn FontData>) -> URLFont {
        let f = URLFont::new(outline_file_url, path, index);
        *f.font_ref.borrow_mut() = Some(data);
        f
    }

    fn init_cache_tags(&mut self) {
        match std::fs::canonicalize(&self.path) {
            Ok(p) => {
                let s = p.to_string_lossy().to_string();
                self.canonical_path = Some(s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s));
                let md = std::fs::metadata(&self.path).ok();
                self.length = md.as_ref().map(|m| m.len() as i64).unwrap_or(0);
                self.last_modified = md
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
            }
            Err(_) => {
                self.canonical_path = None;
                self.length = 0;
                self.last_modified = 0;
            }
        }
    }

    pub fn get_index(&self) -> i32 {
        self.index
    }
}

impl FontImpl for URLFont {
    fn retrieve_font_data(&self) -> FontResult<Rc<dyn FontData>> {
        if let Some(d) = &*self.font_ref.borrow() {
            return Ok(d.clone());
        }
        let a = FontLoader::from_url(&self.outline_file_url, &self.path)?;
        let d = a.get(self.index as usize).cloned().ok_or_else(|| {
            crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontEngineException::new(
                crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::ExceptionKind::FontEngine,
                format!("java.lang.ArrayIndexOutOfBoundsException: Index {} out of bounds for length {}", self.index, a.len()),
            )
        })?;
        *self.font_ref.borrow_mut() = Some(d.clone());
        Ok(d)
    }

    fn get_canonical_path(&self) -> Option<String> {
        self.canonical_path.clone()
    }

    fn get_length(&self) -> i64 {
        self.length
    }

    fn get_last_modified(&self) -> i64 {
        self.last_modified
    }

    fn to_java_string(&self) -> String {
        self.outline_file_url.clone()
    }
}
