//! Port of `flex2.compiler.io.ResourceFile` (one bundle's file per locale), the SWC side of resource bundle lookup
//! (`SwcGroup.updateFiles`, `CompilerSwcContext.updateResourceBundles`/`getResourceBundle`) and the source-path side
//! (`ResourceBundlePath`). A resource bundle `Source` is backed by an `InMemory` file under the bundle's name; the
//! `ResourceFile` itself is kept in a per-compile registry by that name.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::asc::flex2::compiler::source::{canonical_path, Owner, Source, SourceRef, VirtualFile};
use crate::asc::flex2::compiler::swc::Swc;
use crate::asc::flex2::compiler::util::{mime_mappings, name_formatter, QName};
use crate::asc::util::jhash::JHashMap;

/// A bundle file: a SWC entry (`VirtualZipFile`, named `swc$entry`) or a `LocalFile`.
#[derive(Clone, Debug)]
pub enum RbFile {
    Zip { swc: String, entry: String, last_modified: i64 },
    Local(String),
}

impl RbFile {
    pub fn get_name(&self) -> String {
        match self {
            RbFile::Zip { swc, entry, .. } => format!("{swc}${entry}"),
            RbFile::Local(p) => p.clone(),
        }
    }

    pub fn get_last_modified(&self) -> i64 {
        match self {
            RbFile::Zip { last_modified, .. } => *last_modified,
            RbFile::Local(p) => VirtualFile::local(Path::new(p)).get_last_modified(),
        }
    }

    pub fn get_parent(&self) -> Option<String> {
        match self {
            RbFile::Zip { swc, .. } => Some(swc.clone()),
            RbFile::Local(p) => Path::new(p).parent().map(|p| p.to_string_lossy().into_owned()),
        }
    }

    pub fn to_byte_array(&self) -> std::io::Result<Vec<u8>> {
        match self {
            RbFile::Zip { swc, entry, .. } => {
                let f = std::fs::File::open(swc)?;
                let mut z = zip::ZipArchive::new(f).map_err(std::io::Error::other)?;
                let mut e = z.by_name(entry).map_err(std::io::Error::other)?;
                let mut v = Vec::new();
                e.read_to_end(&mut v)?;
                Ok(v)
            }
            RbFile::Local(p) => std::fs::read(p),
        }
    }
}

pub struct ResourceFile {
    name: String,
    locales: Vec<String>,
    /// `LinkedHashMap`s by locale.
    files: RefCell<Vec<(String, Option<RbFile>)>>,
    roots: RefCell<Vec<(String, Option<RbFile>)>>,
    current_locale: RefCell<Option<String>>,
    default_file: RefCell<Option<RbFile>>,
}

impl ResourceFile {
    pub fn new(name: String, locales: &[String], files: Vec<Option<RbFile>>, path_roots: Vec<Option<RbFile>>) -> ResourceFile {
        let mut fm = Vec::new();
        let mut rm = Vec::new();
        for (i, l) in locales.iter().enumerate() {
            fm.push((l.clone(), files.get(i).cloned().flatten()));
            rm.push((l.clone(), path_roots.get(i).cloned().flatten()));
        }
        let r = ResourceFile { name, locales: locales.to_vec(), files: RefCell::new(fm), roots: RefCell::new(rm), current_locale: RefCell::new(None), default_file: RefCell::new(None) };
        r.set_default();
        r
    }

    fn file(&self, locale: &str) -> Option<RbFile> {
        self.files.borrow().iter().find(|(l, _)| l == locale).and_then(|(_, f)| f.clone())
    }

    fn set_default(&self) {
        *self.default_file.borrow_mut() = None;
        for l in &self.locales {
            if let Some(f) = self.file(l) {
                *self.default_file.borrow_mut() = Some(f);
                break;
            }
        }
        *self.current_locale.borrow_mut() = self.locales.first().cloned();
    }

    /// `merge(f)`.
    pub fn merge(&self, f: Option<&ResourceFile>) {
        let Some(f) = f else { return };
        for l in &self.locales {
            if self.file(l).is_none() {
                if let Some(o) = f.file(l) {
                    for e in self.files.borrow_mut().iter_mut().filter(|(k, _)| k == l) {
                        e.1 = Some(o.clone());
                    }
                    for e in self.roots.borrow_mut().iter_mut().filter(|(k, _)| k == l) {
                        e.1 = Some(o.clone());
                    }
                }
            }
        }
        self.set_default();
    }

    pub fn set_locale(&self, locale: &str) {
        *self.current_locale.borrow_mut() = Some(locale.to_string());
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    fn get_virtual_file(&self) -> Option<RbFile> {
        let cur = self.current_locale.borrow().clone();
        match cur {
            None => self.default_file.borrow().clone(),
            Some(l) => self.file(&l).or_else(|| self.default_file.borrow().clone()),
        }
    }

    pub fn get_name_for_reporting(&self) -> String {
        self.get_virtual_file().map_or_else(|| self.name.clone(), |f| f.get_name())
    }

    pub fn get_parent(&self) -> Option<String> {
        self.get_virtual_file().and_then(|f| f.get_parent())
    }

    /// `getInputStream()`'s bytes; `None` when there is no file.
    pub fn read(&self) -> Option<std::io::Result<Vec<u8>>> {
        self.get_virtual_file().map(|f| f.to_byte_array())
    }

    pub fn get_last_modified(&self) -> i64 {
        let mut ts = -1;
        for (_, f) in self.files.borrow().iter() {
            if let Some(f) = f {
                ts = ts.max(f.get_last_modified());
            }
        }
        ts
    }

    pub fn complete(&self) -> bool {
        self.locales.iter().all(|l| self.file(l).is_some())
    }
}

thread_local! {
    static REGISTRY: RefCell<HashMap<String, Rc<ResourceFile>>> = RefCell::new(HashMap::new());
}

/// A new compile's registry.
pub fn reset() {
    REGISTRY.with(|r| r.borrow_mut().clear());
}

/// `(ResourceFile) source.getBackingFile()`.
pub fn resource_file_of(source: &SourceRef) -> Option<Rc<ResourceFile>> {
    let name = source.borrow().get_name().to_string();
    REGISTRY.with(|r| r.borrow().get(&name).cloned())
}

/// `new Source(new ResourceFile(...), pathRoot, relativePath, shortName, owner, false, false, false)`.
fn new_resource_source(rf: ResourceFile, path_root: Option<String>, relative_path: &str, short_name: &str) -> SourceRef {
    let name = rf.get_name().to_string();
    let file = VirtualFile::InMemory { bytes: Rc::new(Vec::new()), name: name.clone(), mime_type: mime_mappings::PROPERTIES, last_modified: rf.get_last_modified() };
    let s = Source::new(file, path_root, relative_path, short_name, Owner::None, false, false, false);
    REGISTRY.with(|r| r.borrow_mut().insert(name, Rc::new(rf)));
    s
}

/// `CompilerSwcContext`'s resource bundle state: `rb2file` from the SWC group's files, and `rb2source`.
#[derive(Default)]
pub struct SwcResourceBundles {
    rb2file: JHashMap<QName, HashMap<String, RbFile>>,
    rb2source: JHashMap<QName, Option<SourceRef>>,
    /// `ResourceBundlePath`, made on first use.
    pub bundle_path: Option<ResourceBundlePath>,
}

/// DOS date and time as a monotonic stand-in for `ZipEntry.getTime()`.
fn zip_time<R: Read>(f: &zip::read::ZipFile<'_, R>) -> i64 {
    match f.last_modified() {
        Some(t) => ((((t.year() as i64 * 13 + t.month() as i64) * 32 + t.day() as i64) * 24 + t.hour() as i64) * 60 + t.minute() as i64) * 60 + t.second() as i64,
        None => 0,
    }
}

impl SwcResourceBundles {
    /// `SwcGroup.updateFiles()` then `updateResourceBundles(files, format)` over the `.properties` files.
    pub fn update(&mut self, swcs: &[Rc<Swc>]) {
        let mut files: HashMap<String, RbFile> = HashMap::new();
        for swc in swcs {
            let Ok(f) = std::fs::File::open(&swc.location) else { continue };
            let Ok(mut z) = zip::ZipArchive::new(f) else { continue };
            for i in 0..z.len() {
                let Ok(e) = z.by_index(i) else { continue };
                let name = e.name().to_string();
                let t = zip_time(&e);
                let newer = files.get(&name).is_none_or(|cur| t > cur.get_last_modified());
                if newer {
                    files.insert(name.clone(), RbFile::Zip { swc: swc.location.clone(), entry: name, last_modified: t });
                }
            }
        }
        for (name, file) in files {
            let Some(rest) = name.strip_prefix("locale/") else { continue };
            let Some(index) = rest.find('/') else { continue };
            if !name.to_ascii_lowercase().ends_with(mime_mappings::get_extension(mime_mappings::PROPERTIES)) {
                continue;
            }
            let locale = &rest[..index];
            let path = &rest[index + 1..rest.len() - ".properties".len()];
            let rb_name = QName::parse(&name_formatter::to_colon(&path.replace('/', ".")));
            if self.rb2file.get(&rb_name).is_none() {
                self.rb2file.insert(rb_name.clone(), HashMap::new());
            }
            self.rb2file.get_mut(&rb_name).unwrap().insert(locale.to_string(), file);
        }
    }

    /// `getVirtualFiles(locales, namespaceURI, localPart)`.
    pub fn get_virtual_files(&self, locales: &[String], namespace_uri: &str, local_part: &str) -> Option<Vec<Option<RbFile>>> {
        let rb_files = self.rb2file.get(&QName::new(namespace_uri, local_part))?;
        if locales.is_empty() {
            return None;
        }
        Some(locales.iter().map(|l| rb_files.get(l).cloned()).collect())
    }

    /// `getResourceBundle(locales, namespaceURI, localPart)`.
    pub fn get_resource_bundle(&mut self, locales: &[String], namespace_uri: &str, local_part: &str) -> Option<SourceRef> {
        if locales.is_empty() {
            return None;
        }
        let key = QName::new(namespace_uri, local_part);
        if let Some(s) = self.rb2source.get(&key) {
            return s.clone();
        }
        let rb_list = self.get_virtual_files(locales, namespace_uri, local_part)?;
        let name = rb_list.iter().flatten().next().map(|f| f.get_name())?;
        let n = rb_list.len();
        let s = new_resource_source(ResourceFile::new(name, locales, rb_list, vec![None; n]), None, &namespace_uri.replace('.', "/"), local_part);
        self.rb2source.insert(key, Some(s.clone()));
        Some(s)
    }
}

/// `ResourceBundlePath`: per locale, the application's directory and the source path, searched for
/// `<package path>/<name>.properties`.
pub struct ResourceBundlePath {
    locales: Vec<String>,
    rb_directories: Vec<(String, Vec<PathBuf>)>,
    sources: HashMap<String, SourceRef>,
}

impl ResourceBundlePath {
    pub fn new(locales: &[String], source_path: &[PathBuf], app_path: Option<&Path>) -> ResourceBundlePath {
        let mut rb_directories = Vec::new();
        for l in locales {
            let mut dirs: Vec<PathBuf> = Vec::new();
            let class_path: Vec<PathBuf> = source_path.iter().map(|p| PathBuf::from(p.to_string_lossy().replace("{locale}", l))).collect();
            if let Some(parent) = app_path.and_then(|a| a.parent()) {
                let pc = canonical_path(parent);
                if !class_path.iter().any(|c| canonical_path(c) == pc) {
                    dirs.push(PathBuf::from(pc));
                }
            }
            for c in class_path {
                if c.is_dir() {
                    let cc = PathBuf::from(canonical_path(&c));
                    if !dirs.contains(&cc) {
                        dirs.push(cc);
                    }
                }
            }
            rb_directories.push((l.clone(), dirs));
        }
        ResourceBundlePath { locales: locales.to_vec(), rb_directories, sources: HashMap::new() }
    }

    /// `findSource(namespaceURI, localPart)`.
    pub fn find_source(&mut self, namespace_uri: &str, local_part: &str) -> Option<SourceRef> {
        let class_name = if namespace_uri.is_empty() { local_part.to_string() } else { format!("{namespace_uri}:{local_part}") };
        if let Some(s) = self.sources.get(&class_name) {
            return Some(s.clone());
        }
        let p = class_name.replace(':', ".").replace('.', std::path::MAIN_SEPARATOR_STR);
        let length = self.locales.len();
        let mut files: Option<Vec<Option<RbFile>>> = None;
        let mut roots: Option<Vec<Option<RbFile>>> = None;
        let mut name = None;
        let mut path_root = None;
        for (i, (_, directories)) in self.rb_directories.iter().enumerate() {
            for d in directories {
                let f = d.join(format!("{p}.properties"));
                if f.is_file() {
                    let files = files.get_or_insert_with(|| vec![None; length]);
                    let roots = roots.get_or_insert_with(|| vec![None; length]);
                    let cp = canonical_path(&f);
                    if name.is_none() {
                        path_root = Some(canonical_path(d));
                        name = Some(cp.clone());
                    }
                    files[i] = Some(RbFile::Local(cp));
                    roots[i] = Some(RbFile::Local(canonical_path(d)));
                    break;
                }
            }
        }
        let files = files?;
        let s = new_resource_source(ResourceFile::new(name.unwrap(), &self.locales, files, roots.unwrap()), path_root, &namespace_uri.replace('.', "/"), local_part);
        self.sources.insert(class_name, s.clone());
        Some(s)
    }
}
