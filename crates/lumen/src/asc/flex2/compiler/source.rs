//! Ports of `flex2.compiler.Source` and the parts of `flex2.compiler.io` (`LocalFile`, `InMemoryFile`) it reads through.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use super::compilation_unit::{CompilationUnit, UnitRef};
use super::swc::SwcScript;
use super::toolkit::LocalLogger;
use super::util::mime_mappings;

pub type SourceRef = Rc<RefCell<Source>>;

/// `FileUtil.getCanonicalPath`, as Java prints it on Windows.
pub fn canonical_path(p: &Path) -> String {
    let c = std::fs::canonicalize(p).unwrap_or_else(|_| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()));
    let s = c.to_string_lossy().into_owned();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s
    }
}

fn last_modified(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as i64)
}

#[derive(Clone, Debug)]
pub enum VirtualFile {
    /// `LocalFile`: `name` is the canonical path.
    Local { name: String, last_modified: i64 },
    /// `InMemoryFile`; `TextFile` sources are this with an AS mime type.
    InMemory { bytes: Rc<Vec<u8>>, name: String, mime_type: &'static str, last_modified: i64 },
}

impl VirtualFile {
    pub fn local(path: &Path) -> VirtualFile {
        VirtualFile::Local { name: canonical_path(path), last_modified: last_modified(path) }
    }

    pub fn get_name(&self) -> &str {
        match self {
            VirtualFile::Local { name, .. } | VirtualFile::InMemory { name, .. } => name,
        }
    }

    pub fn get_name_for_reporting(&self) -> &str {
        self.get_name()
    }

    pub fn get_parent(&self) -> Option<String> {
        match self {
            VirtualFile::Local { name, .. } => Path::new(name).parent().map(|p| p.to_string_lossy().into_owned()),
            VirtualFile::InMemory { .. } => None,
        }
    }

    pub fn get_last_modified(&self) -> i64 {
        match self {
            VirtualFile::Local { name, .. } => last_modified(Path::new(name)),
            VirtualFile::InMemory { last_modified, .. } => *last_modified,
        }
    }

    pub fn size(&self) -> i64 {
        match self {
            VirtualFile::Local { name, .. } => std::fs::metadata(name).map_or(0, |m| m.len() as i64),
            VirtualFile::InMemory { bytes, .. } => bytes.len() as i64,
        }
    }

    pub fn get_mime_type(&self) -> Option<&'static str> {
        match self {
            VirtualFile::Local { name, .. } => mime_mappings::get_mime_type(name),
            VirtualFile::InMemory { mime_type, .. } => Some(mime_type),
        }
    }

    pub fn to_byte_array(&self) -> std::io::Result<Vec<u8>> {
        match self {
            VirtualFile::Local { name, .. } => std::fs::read(name),
            VirtualFile::InMemory { bytes, .. } => Ok(bytes.as_ref().clone()),
        }
    }
}

/// `Source.getOwner()`, by the owner's class.
#[derive(Clone)]
pub enum Owner {
    None,
    SourcePath,
    SourceList,
    ResourceContainer,
    SwcScript(Rc<SwcScript>),
}

pub struct Source {
    file: VirtualFile,
    path_root: Option<String>,
    relative_path: String,
    short_name: String,
    owner: Owner,
    is_internal: bool,
    is_root: bool,
    is_debuggable: bool,
    is_preprocessed: bool,
    unit: Option<UnitRef>,
    file_time: i64,
    logger: Option<Rc<LocalLogger>>,
    file_includes: Vec<String>,
    /// The `BytecodeEmitter` view of this source, made on first use.
    emitter_view: Option<Rc<crate::asc::emit::bytecode_emitter::Source>>,
}

impl Source {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        file: VirtualFile,
        path_root: Option<String>,
        relative_path: &str,
        short_name: &str,
        owner: Owner,
        is_internal: bool,
        is_root: bool,
        is_debuggable: bool,
    ) -> SourceRef {
        let file_time = file.get_last_modified();
        Rc::new(RefCell::new(Source {
            file,
            path_root,
            relative_path: relative_path.to_string(),
            short_name: short_name.to_string(),
            owner,
            is_internal,
            is_root,
            is_debuggable,
            is_preprocessed: false,
            unit: None,
            file_time,
            logger: None,
            file_includes: Vec::new(),
            emitter_view: None,
        }))
    }

    pub fn new_compilation_unit(this: &SourceRef, syntax_tree: Option<crate::asc::parser::node::NodeRef>) -> UnitRef {
        let unit = CompilationUnit::new(this, syntax_tree);
        this.borrow_mut().unit = Some(unit.clone());
        unit
    }

    pub fn get_compilation_unit(&self) -> Option<UnitRef> {
        self.unit.clone()
    }

    pub fn set_preprocessed(&mut self) {
        self.is_preprocessed = true;
    }

    pub fn is_preprocessed(&self) -> bool {
        self.is_preprocessed
    }

    pub fn set_logger(&mut self, logger: Option<Rc<LocalLogger>>) {
        self.logger = logger;
    }

    pub fn get_logger(&self) -> Option<Rc<LocalLogger>> {
        self.logger.clone()
    }

    pub fn disconnect_logger(&mut self) {
        if let Some(l) = &self.logger {
            if l.warning_count() == 0 && l.error_count() == 0 {
                self.logger = None;
            } else {
                l.disconnect();
            }
        }
    }

    pub fn has_error(&self) -> bool {
        self.logger.as_ref().is_some_and(|l| l.error_count() > 0)
    }

    pub fn get_owner(&self) -> &Owner {
        &self.owner
    }

    pub fn is_source_path_owner(&self) -> bool {
        matches!(self.owner, Owner::SourcePath)
    }

    pub fn is_source_list_owner(&self) -> bool {
        matches!(self.owner, Owner::SourceList)
    }

    pub fn is_swc_script_owner(&self) -> bool {
        matches!(self.owner, Owner::SwcScript(_))
    }

    pub fn is_resource_container_owner(&self) -> bool {
        matches!(self.owner, Owner::ResourceContainer)
    }

    pub fn swc_script(&self) -> Option<&Rc<SwcScript>> {
        match &self.owner {
            Owner::SwcScript(s) => Some(s),
            _ => None,
        }
    }

    pub fn is_internal(&self) -> bool {
        self.is_internal
    }

    /// `isEntryPoint()`: only `FileSpec` sources, which mxmlc never has.
    pub fn is_entry_point(&self) -> bool {
        false
    }

    pub fn is_root(&self) -> bool {
        self.is_root
    }

    pub fn is_debuggable(&self) -> bool {
        self.is_debuggable
    }

    pub fn is_compiled(&self) -> bool {
        self.unit.as_ref().is_some_and(|u| u.borrow().is_bytecode_available())
    }

    pub fn is_updated(&self) -> bool {
        self.file.get_last_modified() != self.file_time
    }

    pub fn exists(&self) -> bool {
        match &self.file {
            VirtualFile::Local { name, .. } => Path::new(name).exists(),
            VirtualFile::InMemory { .. } => true,
        }
    }

    pub fn get_name(&self) -> &str {
        self.file.get_name()
    }

    pub fn get_name_for_reporting(&self) -> &str {
        self.file.get_name_for_reporting()
    }

    pub fn get_parent(&self) -> Option<String> {
        self.file.get_parent()
    }

    pub fn size(&self) -> i64 {
        self.file.size()
    }

    pub fn to_byte_array(&self) -> std::io::Result<Vec<u8>> {
        self.file.to_byte_array()
    }

    pub fn get_mime_type(&self) -> Option<&'static str> {
        self.file.get_mime_type()
    }

    pub fn get_last_modified(&self) -> i64 {
        self.file_time
    }

    pub fn get_relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn get_short_name(&self) -> &str {
        &self.short_name
    }

    pub fn get_path_root(&self) -> Option<&str> {
        self.path_root.as_deref()
    }

    pub fn add_file_include(&mut self, path: String) {
        if !self.file_includes.contains(&path) {
            self.file_includes.push(path);
        }
    }

    pub fn is_included_file(&self, name: &str) -> bool {
        self.file_includes.iter().any(|f| f == name)
    }

    /// The fields `BytecodeEmitter` reads, as its own `Source` stub.
    pub fn emitter_view(&mut self) -> Rc<crate::asc::emit::bytecode_emitter::Source> {
        if self.emitter_view.is_none() {
            self.emitter_view = Some(Rc::new(crate::asc::emit::bytecode_emitter::Source {
                name: self.get_name().to_string(),
                debuggable: self.is_debuggable,
                source_path_owner: self.is_source_path_owner(),
                source_list_owner: self.is_source_list_owner(),
                resource_container_owner: self.is_resource_container_owner(),
                root: self.is_root,
                relative_path: self.relative_path.clone(),
                line_count: Default::default(),
            }));
        }
        self.emitter_view.clone().unwrap()
    }
}
