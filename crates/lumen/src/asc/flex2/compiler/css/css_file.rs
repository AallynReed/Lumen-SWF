//! The `VirtualFile`s style sheets come from (`LocalFile`, and `VirtualZipFile` for a SWC's `defaults.css`), with
//! the resolution `ThreadLocalToolkit.getPathResolver()` gives them: the style sheets pushed with
//! `addSinglePathResolver(0, file)`, then `LocalFilePathResolver` and `SwcPathResolver`.

use std::cell::RefCell;
use std::io::Read;
use std::path::Path;
use std::rc::Rc;

use crate::asc::flex2::compiler::source::{canonical_path, SourceRef, VirtualFile};
use crate::asc::flex2::compiler::swc::Swc;

#[derive(Clone, Debug)]
pub enum CssFile {
    /// `LocalFile`, by canonical path.
    Local(String),
    /// `VirtualZipFile`: `swc` is the archive's location, `entry` the name in it.
    Zip { swc: String, entry: String },
}

impl PartialEq for CssFile {
    fn eq(&self, other: &CssFile) -> bool {
        self.get_name() == other.get_name()
    }
}

fn zip_entry(swc: &str, entry: &str) -> std::io::Result<Vec<u8>> {
    let f = std::fs::File::open(swc)?;
    let mut z = zip::ZipArchive::new(f).map_err(std::io::Error::other)?;
    let mut e = z.by_name(entry).map_err(std::io::Error::other)?;
    let mut v = Vec::new();
    e.read_to_end(&mut v)?;
    Ok(v)
}

fn zip_has(swc: &str, entry: &str) -> bool {
    std::fs::File::open(swc).ok().and_then(|f| zip::ZipArchive::new(f).ok()).is_some_and(|mut z| z.by_name(entry).is_ok())
}

/// `FileUtils.addPathComponents(a, b, '/')`.
fn add_path_components(a: &str, b: &str) -> String {
    let mut parts: Vec<&str> = a.split('/').filter(|p| !p.is_empty()).collect();
    for p in b.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(p),
        }
    }
    parts.join("/")
}

impl CssFile {
    pub fn local(path: &Path) -> CssFile {
        CssFile::Local(canonical_path(path))
    }

    pub fn get_name(&self) -> String {
        match self {
            CssFile::Local(n) => n.clone(),
            CssFile::Zip { swc, entry } => format!("{swc}${entry}"),
        }
    }

    pub fn is_zip(&self) -> bool {
        matches!(self, CssFile::Zip { .. })
    }

    pub fn to_byte_array(&self) -> std::io::Result<Vec<u8>> {
        match self {
            CssFile::Local(n) => std::fs::read(n),
            CssFile::Zip { swc, entry } => zip_entry(swc, entry),
        }
    }

    pub fn get_last_modified(&self) -> i64 {
        match self {
            CssFile::Local(n) => VirtualFile::local(Path::new(n)).get_last_modified(),
            CssFile::Zip { swc, .. } => VirtualFile::local(Path::new(swc)).get_last_modified(),
        }
    }

    /// `LocalFile.resolve` / `VirtualZipFile.resolve`.
    pub fn resolve(&self, relative: &str) -> Option<CssFile> {
        match self {
            CssFile::Local(n) => {
                let p = Path::new(n);
                let dir = if p.is_dir() {
                    p.to_path_buf()
                } else if p.is_file() {
                    p.parent()?.to_path_buf()
                } else {
                    return None;
                };
                let r = Path::new(relative);
                let target = if r.is_absolute() { dir.join(relative.trim_start_matches(['/', '\\'])) } else { dir.join(relative) };
                target.exists().then(|| CssFile::local(&target))
            }
            CssFile::Zip { swc, entry } => {
                let rel = match entry.rfind('/') {
                    Some(sep) => add_path_components(&entry[..sep], relative),
                    None => relative.to_string(),
                };
                zip_has(swc, &rel).then(|| CssFile::Zip { swc: swc.clone(), entry: rel })
            }
        }
    }

    /// The file as the transcoders' `VirtualFile`.
    pub fn to_virtual_file(&self) -> VirtualFile {
        match self {
            CssFile::Local(n) => VirtualFile::local(Path::new(n)),
            CssFile::Zip { entry, .. } => VirtualFile::InMemory {
                bytes: Rc::new(self.to_byte_array().unwrap_or_default()),
                name: self.get_name(),
                mime_type: crate::asc::flex2::compiler::util::mime_mappings::get_mime_type(entry).unwrap_or(""),
                last_modified: self.get_last_modified(),
            },
        }
    }
}

#[derive(Default)]
struct Resolvers {
    stack: Vec<CssFile>,
    swcs: Vec<Rc<Swc>>,
}

thread_local! {
    static RESOLVERS: RefCell<Resolvers> = RefCell::new(Resolvers::default());
}

/// The SWCs `CompilerSwcContext` added a `SwcPathResolver` for.
pub fn set_swcs(swcs: Vec<Rc<Swc>>) {
    RESOLVERS.with(|r| r.borrow_mut().swcs = swcs);
}

pub fn swcs() -> Vec<Rc<Swc>> {
    RESOLVERS.with(|r| r.borrow().swcs.clone())
}

/// `ThreadLocalToolkit.getPathResolver().addSinglePathResolver(0, file)`.
pub fn push_resolver(file: &CssFile) {
    RESOLVERS.with(|r| r.borrow_mut().stack.insert(0, file.clone()));
}

/// `removeSinglePathResolver(file)`.
pub fn remove_resolver(file: &CssFile) {
    RESOLVERS.with(|r| {
        let mut r = r.borrow_mut();
        if let Some(i) = r.stack.iter().position(|f| f == file) {
            r.stack.remove(i);
        }
    });
}

/// `Swc.getFile(path)` for a file listed in the SWC.
pub fn swc_file(swc: &Swc, path: &str) -> Option<CssFile> {
    zip_has(&swc.location, path).then(|| CssFile::Zip { swc: swc.location.clone(), entry: path.to_string() })
}

/// `SwcPathResolver.resolve`: `swcGroup.getFiles().get(path)` (the newest SWC's file of that name), then
/// `swcGroup.getFile("x.swc$path")`.
fn swc_resolve(path: &str) -> Option<CssFile> {
    let swcs = swcs();
    let mut best: Option<(i64, CssFile)> = None;
    for swc in &swcs {
        if let Some(f) = swc_file(swc, path) {
            let m = swc.last_modified;
            if best.as_ref().is_none_or(|(bm, _)| m > *bm) {
                best = Some((m, f));
            }
        }
    }
    if let Some((_, f)) = best {
        return Some(f);
    }
    let (location, file) = path.split_once('$')?;
    let swc = swcs.iter().find(|s| s.location == location)?;
    swc_file(swc, file)
}

/// `ThreadLocalToolkit.getPathResolver().resolve(path)`.
pub fn toolkit_resolve(path: &str) -> Option<CssFile> {
    let stack = RESOLVERS.with(|r| r.borrow().stack.clone());
    if let Some(f) = stack.iter().find_map(|f| f.resolve(path)) {
        return Some(f);
    }
    let p = Path::new(path);
    if p.is_absolute() && p.exists() {
        return Some(CssFile::local(p));
    }
    swc_resolve(path)
}

/// `Source.resolve(path)`: relative to the source's file, then its path root, then the toolkit's resolver.
pub fn source_resolve(source: &SourceRef, path: &str) -> Option<CssFile> {
    let (name, root) = {
        let s = source.borrow();
        (s.get_name().to_string(), s.get_path_root().map(str::to_string))
    };
    let mut f = None;
    if !path.starts_with('/') {
        f = CssFile::Local(name).resolve(path);
    }
    if f.is_none() && path.starts_with('/') {
        if let Some(root) = root {
            f = CssFile::Local(root).resolve(&path[1..]);
        }
    }
    f.or_else(|| toolkit_resolve(path))
}

/// `ThreadLocalToolkit.addResolvedPath(path, file)`.
pub fn add_resolved_path(path: &str, file: &CssFile) {
    crate::asc::flex2::compiler::transcoder::add_resolved_path(path, &file.to_virtual_file());
}
