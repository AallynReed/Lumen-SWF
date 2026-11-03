//! Ports of `flex2.compiler.SourcePath` (with `SourcePathBase`) and `flex2.compiler.SourceList`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf, MAIN_SEPARATOR};

use super::source::{canonical_path, Owner, Source, SourceRef, VirtualFile};
use super::toolkit::{self, CompilerMessage};
use super::util::{construct_class_name, mime_mappings, name_formatter};

const NO_DIR: &str = "";

/// `CompilerException` thrown by `findFile` when two mime types match one name.
pub struct MoreThanOneComponentOfTheSameName {
    pub message: String,
}

/// `FileUtil.isSubdirectoryOf(String, VirtualFile[])`.
fn is_subdirectory_of_paths(directory: &str, directories: &[String]) -> bool {
    let parent = Path::new(directory).parent().map(|p| p.to_path_buf());
    directories.iter().any(|dir| {
        let dir_parent = Path::new(dir).parent().map(|p| p.to_path_buf());
        parent != dir_parent && directory.len() > dir.len() && directory.starts_with(dir.as_str())
    })
}

/// `FileUtil.isSubdirectoryOf(File, List)`.
fn is_subdirectory_of(path: &str, directories: &[PathBuf]) -> Option<usize> {
    let parent = Path::new(path).parent().map(|p| p.to_path_buf());
    directories.iter().position(|d| {
        let dir = canonical_path(d);
        let dir_parent = Path::new(&dir).parent().map(|p| p.to_path_buf());
        parent != dir_parent && ((path.len() > dir.len() && path.starts_with(&dir)) || (dir.len() > path.len() && dir.starts_with(path)))
    })
}

/// `SourcePathBase.addApplicationParentToSourcePath`.
pub(crate) fn add_application_parent(app_path: Option<&VirtualFile>, class_path: &[String], directories: &mut Vec<PathBuf>) {
    if let Some(parent) = app_path.and_then(|a| a.get_parent()) {
        if Path::new(&parent).is_dir() && !is_subdirectory_of_paths(&parent, class_path) {
            directories.push(PathBuf::from(parent));
        }
    }
}

fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// `SourcePathBase.addPathElements`; returns the overlap warnings.
pub(crate) fn add_path_elements(class_path: &[String], directories: &mut Vec<PathBuf>, allow_overlap: bool) -> Vec<(String, String)> {
    let mut warnings = Vec::new();
    let mut bad_paths = false;
    for path in class_path {
        let f = PathBuf::from(path);
        if f.is_dir() {
            if !allow_overlap && !bad_paths {
                if let Some(index) = is_subdirectory_of(path, directories) {
                    let dir_path = std::path::absolute(&directories[index]).unwrap_or_else(|_| directories[index].clone()).to_string_lossy().into_owned();
                    let package_path = if path.len() > dir_path.len() { &path[dir_path.len()..] } else { &dir_path[path.len()..] };
                    if path == &dir_path || package_path.split(MAIN_SEPARATOR).filter(|t| !t.is_empty()).all(is_valid_identifier) {
                        warnings.push((path.clone(), dir_path));
                        bad_paths = true;
                    }
                }
            }
            directories.push(f);
        }
    }
    warnings
}

/// `SourcePathBase` state shared by the lookups.
pub struct SourcePath {
    directories: Vec<PathBuf>,
    mime_types: Vec<&'static str>,
    sources: crate::asc::util::jhash::JHashMap<String, SourceRef>,
    hits: HashSet<String>,
    misses: HashSet<String>,
    dirs: HashMap<String, String>,
    warnings: Vec<(String, String)>,
}

impl SourcePath {
    pub fn new(class_path: &[String], app_path: Option<&VirtualFile>, mime_types: &[&'static str], allow_overlap: bool) -> SourcePath {
        let mut directories = Vec::new();
        add_application_parent(app_path, class_path, &mut directories);
        let warnings = add_path_elements(class_path, &mut directories, allow_overlap);
        SourcePath {
            directories,
            mime_types: mime_types.to_vec(),
            sources: crate::asc::util::jhash::JHashMap::new(),
            hits: HashSet::new(),
            misses: HashSet::with_capacity(1024),
            dirs: HashMap::new(),
            warnings,
        }
    }

    pub fn display_warnings(&self) {
        for (cpath, directory) in &self.warnings {
            toolkit::log(
                CompilerMessage::warning(format!("Source path entry, '{cpath}', is a subdirectory of source path entry, '{directory}'.")),
                None,
                -1,
                -1,
                None,
            );
        }
    }

    pub fn get_paths(&self) -> &[PathBuf] {
        &self.directories
    }

    pub fn get_mime_types(&self) -> &[&'static str] {
        &self.mime_types
    }

    /// `sources()`: class name to source.
    pub fn sources(&self) -> &crate::asc::util::jhash::JHashMap<String, SourceRef> {
        &self.sources
    }

    pub fn sources_mut(&mut self) -> &mut crate::asc::util::jhash::JHashMap<String, SourceRef> {
        &mut self.sources
    }

    fn new_source(&self, file: &Path, path_root: &Path, namespace_uri: &str, local_part: &str) -> SourceRef {
        Source::new(
            VirtualFile::local(file),
            Some(canonical_path(path_root)),
            &namespace_uri.replace('.', "/"),
            local_part,
            Owner::SourcePath,
            false,
            false,
            true,
        )
    }

    fn find_file_class(&mut self, class_name: &str, namespace_uri: &str, local_part: &str) -> Result<Option<SourceRef>, MoreThanOneComponentOfTheSameName> {
        let p = class_name.replace(':', ".").replace('.', MAIN_SEPARATOR.to_string().as_str());
        for i in 0..self.directories.len() {
            let d = self.directories[i].clone();
            if let Some(f) = self.find_file(&d, &p)? {
                let s = self.new_source(&f, &d, namespace_uri, local_part);
                self.sources.insert(class_name.to_string(), s.clone());
                return Ok(Some(s));
            }
        }
        Ok(None)
    }

    pub fn has_package(&mut self, package_name: &str) -> bool {
        for i in 0..self.directories.len() {
            let d = self.directories[i].clone();
            if self.has_directory(&d, package_name) {
                return true;
            }
        }
        false
    }

    pub fn has_definition(&mut self, namespace: &str, local_part: &str) -> bool {
        let class_name = construct_class_name(namespace, local_part);
        if self.misses.contains(&class_name) {
            return false;
        }
        if self.hits.contains(&class_name) {
            return true;
        }
        let p = class_name.replace(':', ".").replace('.', MAIN_SEPARATOR.to_string().as_str());
        for i in 0..self.directories.len() {
            let d = self.directories[i].clone();
            if let Ok(Some(_)) = self.find_file(&d, &p) {
                self.hits.insert(class_name);
                return true;
            }
        }
        self.misses.insert(class_name);
        false
    }

    fn has_directory(&mut self, dir: &Path, package_name: &str) -> bool {
        if package_name.is_empty() {
            return true;
        }
        let relative_path = package_name.replace('.', MAIN_SEPARATOR.to_string().as_str());
        let full_path = format!("{}{}{}", dir.display(), MAIN_SEPARATOR, relative_path);
        if self.dirs.get(&full_path).is_some_and(|v| v == NO_DIR) {
            return false;
        }
        let result = dir.join(&relative_path).is_dir();
        self.dirs.insert(full_path.clone(), if result { full_path } else { NO_DIR.to_string() });
        result
    }

    /// `findSource(namespaceURI, localPart)`.
    pub fn find_source(&mut self, namespace_uri: &str, local_part: &str) -> Result<Option<SourceRef>, MoreThanOneComponentOfTheSameName> {
        let class_name = construct_class_name(namespace_uri, local_part);
        if self.misses.contains(&class_name) {
            return Ok(None);
        }
        let mut s = self.sources.get(&class_name).cloned();
        if s.is_none() {
            if let Some(found) = self.find_file_class(&class_name, namespace_uri, local_part)? {
                return Ok(Some(found));
            }
        }
        if s.as_ref().is_some_and(|src| !src.borrow().exists()) {
            self.sources.remove(&class_name);
            s = None;
        }
        if let Some(src) = &s {
            let u = src.borrow().get_compilation_unit();
            if let Some(u) = u {
                let u = u.borrow();
                if u.top_level_definitions.len() == 1 && !u.top_level_definitions.contains(namespace_uri, local_part) {
                    let real_name = u.top_level_definitions.first().unwrap().to_string();
                    self.sources.insert(real_name.clone(), src.clone());
                    self.misses.remove(&real_name);
                    s = None;
                }
            }
        }
        if let Some(src) = s.clone() {
            let (busy, has_unit) = {
                let b = src.borrow();
                let u = b.get_compilation_unit();
                (u.as_ref().is_some_and(|u| !u.borrow().is_done()) || b.is_updated(), u.is_some())
            };
            if !busy && has_unit {
                s = super::source::copy(&src);
            }
        }
        if s.is_none() {
            self.misses.insert(class_name);
        }
        Ok(s)
    }

    /// `checkPreference(s)`: the source is still what `findFile` finds first.
    pub fn check_preference(&mut self, s: &SourceRef) -> bool {
        let (name, root) = {
            let b = s.borrow();
            (b.get_name().to_string(), b.get_path_root().unwrap_or("").to_string())
        };
        let Some(mut relative_path) = name.get(root.len() + 1..).map(str::to_string) else { return true };
        match self.mime_types.iter().map(|m| mime_mappings::get_extension(m)).find(|ext| relative_path.ends_with(ext)) {
            Some(ext) => relative_path.truncate(relative_path.len() - ext.len()),
            None => return true,
        }
        let mut this_path = false;
        for i in 0..self.directories.len() {
            let d = self.directories[i].clone();
            if root == canonical_path(&d) {
                this_path = true;
            }
            match self.find_file(&d, &relative_path) {
                Err(_) => {
                    self.remove_source(s);
                    return false;
                }
                Ok(Some(_)) if !this_path => {
                    self.remove_source(s);
                    return false;
                }
                _ => {}
            }
        }
        true
    }

    /// `removeSource(s)`.
    pub fn remove_source(&mut self, s: &SourceRef) {
        let key = self.sources.iter().find(|(_, v)| std::rc::Rc::ptr_eq(v, s)).map(|(k, _)| k.clone());
        if let Some(k) = key {
            self.sources.remove(&k);
        }
    }

    fn find_file(&mut self, directory: &Path, relative_path: &str) -> Result<Option<PathBuf>, MoreThanOneComponentOfTheSameName> {
        let mut found: Option<PathBuf> = None;
        for k in 0..self.mime_types.len() {
            let f = self.find_file_mime(directory, relative_path, self.mime_types[k]);
            if let Some(f) = f {
                match &found {
                    None => found = Some(f),
                    Some(prev) => {
                        return Err(MoreThanOneComponentOfTheSameName {
                            message: format!("{} and {} can't co-exist in the same directory.", canonical_path(prev), canonical_path(&f)),
                        })
                    }
                }
            }
        }
        Ok(found)
    }

    fn find_file_mime(&mut self, directory: &Path, relative_path: &str, mime_type: &str) -> Option<PathBuf> {
        let full_path = format!("{}{}{}", directory.display(), MAIN_SEPARATOR, relative_path);
        let dir = full_path.rfind(MAIN_SEPARATOR).map(|i| full_path[..i].to_string());
        if let Some(dir) = &dir {
            if self.dirs.get(dir).is_some_and(|v| v == NO_DIR) {
                return None;
            }
        }
        let path = format!("{}{}", relative_path, mime_mappings::get_extension(mime_type));
        let f = directory.join(&path);
        if f.is_file() && canonical_path(&f).ends_with(&path) {
            return Some(f);
        } else if let Some(dir) = dir {
            if !self.dirs.contains_key(&dir) {
                let is_dir = f.parent().is_some_and(|p| p.is_dir());
                self.dirs.insert(dir.clone(), if is_dir { dir } else { NO_DIR.to_string() });
            }
        }
        None
    }

    /// `checkClassNameFileName(s)`.
    pub fn check_class_name_file_name(def_local_part: &str, short_name: &str) -> Option<(String, String)> {
        if def_local_part == short_name {
            None
        } else {
            Some((short_name.to_string(), def_local_part.to_string()))
        }
    }

    /// `checkPackageNameDirectoryName(s)`.
    pub fn check_package_name_directory_name(def_namespace: &str, relative_path: &str) -> Option<(String, String)> {
        let def_package = name_formatter::normalize_package_name(def_namespace);
        let path_package = name_formatter::to_dot_delim(relative_path, '/');
        if def_package == path_package {
            None
        } else {
            Some((path_package, def_package))
        }
    }
}

/// `SourceList`: the files named on the command line, the last one being the root.
pub struct SourceList {
    sources: Vec<(String, SourceRef)>,
    directories: Vec<PathBuf>,
}

pub struct FileNotInSourcePath(pub String);

impl SourceList {
    pub fn new(files: &[VirtualFile], class_path: &[String], app_path: Option<&VirtualFile>) -> Result<SourceList, FileNotInSourcePath> {
        let mut directories = Vec::new();
        add_application_parent(app_path, class_path, &mut directories);
        add_path_elements(class_path, &mut directories, true);
        let mut list = SourceList { sources: Vec::new(), directories };
        let length = files.len();
        for (i, file) in files.iter().enumerate() {
            let name = file.get_name().to_string();
            let Some(path_root) = list.calculate_path_root(&name) else {
                let m = format!("{name} must be in the source path.");
                toolkit::log(CompilerMessage::error(m), None, -1, -1, None);
                return Err(FileNotInSourcePath(name));
            };
            let relative_path = list.calculate_relative_path(&name);
            let namespace_uri = relative_path.replace('/', ".");
            let local_part = calculate_local_part(&name);
            let source = Source::new(file.clone(), Some(path_root), &relative_path, &local_part, Owner::SourceList, false, i == length - 1, true);
            let class_name = construct_class_name(&namespace_uri, &local_part);
            match list.sources.iter_mut().find(|(k, _)| *k == class_name) {
                Some(e) => e.1 = source,
                None => list.sources.push((class_name, source)),
            }
        }
        Ok(list)
    }

    fn calculate_path_root(&self, name: &str) -> Option<String> {
        for d in &self.directories {
            let dir = std::path::absolute(d).unwrap_or_else(|_| d.clone()).to_string_lossy().into_owned();
            if name.starts_with(&dir) {
                return Some(canonical_path(Path::new(&dir)));
            }
        }
        None
    }

    fn calculate_relative_path(&self, name: &str) -> String {
        for d in &self.directories {
            let dir = format!("{}{}", std::path::absolute(d).unwrap_or_else(|_| d.clone()).display(), MAIN_SEPARATOR);
            if let Some(rest) = name.strip_prefix(&dir) {
                return match rest.rfind(MAIN_SEPARATOR) {
                    Some(index) => rest[..index].replace(MAIN_SEPARATOR, "/"),
                    None => String::new(),
                };
            }
        }
        String::new()
    }

    /// `retrieveSources()`: a finished, unchanged source is handed out as a copy.
    pub fn retrieve_sources(&self) -> Vec<SourceRef> {
        let mut out = Vec::new();
        for (_, s) in &self.sources {
            if !s.borrow().exists() {
                continue;
            }
            let u = s.borrow().get_compilation_unit();
            if u.as_ref().is_some_and(|u| !u.borrow().is_done()) || s.borrow().is_updated() || u.is_none() {
                out.push(s.clone());
            } else if let Some(c) = super::source::copy(s) {
                out.push(c);
            }
        }
        out
    }

    pub fn find_source(&mut self, namespace_uri: &str, local_part: &str) -> Option<SourceRef> {
        if self.sources.is_empty() {
            return None;
        }
        let class_name = construct_class_name(namespace_uri, local_part);
        let mut s = self.sources.iter().find(|(k, _)| *k == class_name).map(|(_, s)| s.clone());
        if s.as_ref().is_some_and(|src| !src.borrow().exists()) {
            s = None;
        }
        if let Some(src) = s.clone() {
            let u = src.borrow().get_compilation_unit();
            if let Some(u) = u {
                let u = u.borrow();
                if u.top_level_definitions.len() == 1 && !u.top_level_definitions.contains(namespace_uri, local_part) {
                    let real_name = u.top_level_definitions.first().unwrap().to_string();
                    match self.sources.iter_mut().find(|(k, _)| *k == real_name) {
                        Some(e) => e.1 = src.clone(),
                        None => self.sources.push((real_name, src.clone())),
                    }
                    s = None;
                }
            }
        }
        if let Some(src) = s.clone() {
            let u = src.borrow().get_compilation_unit();
            let busy = u.as_ref().is_some_and(|u| !u.borrow().is_done()) || src.borrow().is_updated();
            if !busy && u.is_some() {
                s = super::source::copy(&src);
            }
        }
        s
    }

    pub fn get_paths(&self) -> &[PathBuf] {
        &self.directories
    }

    /// `sources().values()`, in insertion order.
    pub fn sources(&self) -> Vec<SourceRef> {
        self.sources.iter().map(|(_, s)| s.clone()).collect()
    }
}

fn calculate_local_part(name: &str) -> String {
    let leaf = &name[name.rfind(MAIN_SEPARATOR).map_or(0, |i| i + 1)..];
    leaf[..leaf.rfind('.').unwrap_or(leaf.len())].to_string()
}
