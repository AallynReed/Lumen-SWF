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
    /// `TextFile`, with its own parent and name for reporting.
    Text { text: Rc<String>, name: String, name_for_reporting: String, parent: Option<String>, mime_type: &'static str, last_modified: i64 },
}

impl VirtualFile {
    pub fn local(path: &Path) -> VirtualFile {
        VirtualFile::Local { name: canonical_path(path), last_modified: last_modified(path) }
    }

    pub fn get_name(&self) -> &str {
        match self {
            VirtualFile::Local { name, .. } | VirtualFile::InMemory { name, .. } | VirtualFile::Text { name, .. } => name,
        }
    }

    pub fn get_name_for_reporting(&self) -> &str {
        match self {
            VirtualFile::Text { name_for_reporting, .. } => name_for_reporting,
            _ => self.get_name(),
        }
    }

    /// `new TextFile(text, name, nameForReporting, parent, mimeType, lastModified)`.
    pub fn text(text: String, name: &str, name_for_reporting: Option<&str>, parent: Option<String>, mime_type: &'static str, last_modified: i64) -> VirtualFile {
        VirtualFile::Text { text: Rc::new(text), name: name.to_string(), name_for_reporting: name_for_reporting.unwrap_or(name).to_string(), parent, mime_type, last_modified }
    }

    pub fn is_text_based(&self) -> bool {
        matches!(self, VirtualFile::Text { .. })
    }

    /// `TextFile.toString()`.
    pub fn get_text(&self) -> Option<Rc<String>> {
        match self {
            VirtualFile::Text { text, .. } => Some(text.clone()),
            _ => None,
        }
    }

    pub fn get_parent(&self) -> Option<String> {
        match self {
            VirtualFile::Local { name, .. } => Path::new(name).parent().map(|p| p.to_string_lossy().into_owned()),
            VirtualFile::InMemory { .. } => None,
            VirtualFile::Text { parent, .. } => parent.clone(),
        }
    }

    pub fn get_last_modified(&self) -> i64 {
        match self {
            VirtualFile::Local { name, .. } => last_modified(Path::new(name)),
            VirtualFile::InMemory { last_modified, .. } | VirtualFile::Text { last_modified, .. } => *last_modified,
        }
    }

    pub fn size(&self) -> i64 {
        match self {
            VirtualFile::Local { name, .. } => std::fs::metadata(name).map_or(0, |m| m.len() as i64),
            VirtualFile::InMemory { bytes, .. } => bytes.len() as i64,
            VirtualFile::Text { text, .. } => crate::asc::util::jstr::utf16_len(text) as i64,
        }
    }

    pub fn get_mime_type(&self) -> Option<&'static str> {
        match self {
            VirtualFile::Local { name, .. } => mime_mappings::get_mime_type(name),
            VirtualFile::InMemory { mime_type, .. } | VirtualFile::Text { mime_type, .. } => Some(mime_type),
        }
    }

    pub fn to_byte_array(&self) -> std::io::Result<Vec<u8>> {
        match self {
            VirtualFile::Local { name, .. } => std::fs::read(name),
            VirtualFile::InMemory { bytes, .. } => Ok(bytes.as_ref().clone()),
            VirtualFile::Text { text, .. } => Ok(text.as_bytes().to_vec()),
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
    ResourceBundlePath,
    CompilerSwcContext,
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
    /// `fileIncludeTimes`, by file name.
    file_include_times: crate::asc::util::jhash::JHashMap<String, i64>,
    /// The `BytecodeEmitter` view of this source, made on first use.
    emitter_view: Option<Rc<crate::asc::emit::bytecode_emitter::Source>>,
    delegate: Option<SourceRef>,
    /// `fragments` / `fragmentLineMaps`: MXML's embedded scripts (`VirtualFile`), inline component trees and their maps.
    fragments: Option<crate::asc::util::jhash::JHashMap<String, Rc<dyn std::any::Any>>>,
    fragment_line_maps: Option<crate::asc::util::jhash::JHashMap<String, Rc<dyn std::any::Any>>>,
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
            file_include_times: crate::asc::util::jhash::JHashMap::with_capacity(4),
            emitter_view: None,
            delegate: None,
            fragments: None,
            fragment_line_maps: None,
        }))
    }

    /// `new Source(file, original)`.
    pub fn new_from(file: VirtualFile, original: &SourceRef) -> SourceRef {
        let s = {
            let o = original.borrow();
            Source::new(file, o.path_root.clone(), &o.relative_path, &o.short_name, o.owner.clone(), o.is_internal, o.is_root, o.is_debuggable)
        };
        s.borrow_mut().delegate = Some(original.clone());
        s
    }

    /// `new Source(file, original, shortName, isInternal, isRoot)`.
    pub fn new_derived(file: VirtualFile, original: &SourceRef, short_name: &str, is_internal: bool, is_root: bool) -> SourceRef {
        let s = {
            let o = original.borrow();
            Source::new(file, o.path_root.clone(), &o.relative_path, short_name, Owner::None, is_internal, is_root, true)
        };
        s.borrow_mut().delegate = Some(original.clone());
        s
    }

    pub fn get_delegate(&self) -> Option<SourceRef> {
        self.delegate.clone()
    }

    pub fn is_text_based(&self) -> bool {
        self.file.is_text_based()
    }

    /// `getInputText()`.
    pub fn get_input_text(&self) -> Option<Rc<String>> {
        self.file.get_text()
    }

    /// `addSourceFragment(n, f, m)`.
    pub fn add_source_fragment(&mut self, n: &str, f: Rc<dyn std::any::Any>, m: Option<Rc<dyn std::any::Any>>) {
        if self.fragments.is_none() {
            self.fragments = Some(Default::default());
            self.fragment_line_maps = Some(Default::default());
        }
        self.fragments.as_mut().unwrap().insert(n.to_string(), f);
        if let Some(m) = m {
            self.fragment_line_maps.as_mut().unwrap().insert(n.to_string(), m);
        }
    }

    /// `getSourceFragment(n)`: this source's before the delegate's.
    pub fn get_source_fragment(&self, n: &str) -> Option<Rc<dyn std::any::Any>> {
        let obj = self.fragments.as_ref().and_then(|f| f.get(n).cloned());
        match obj {
            None => self.delegate.as_ref().and_then(|d| d.borrow().get_source_fragment(n)),
            o => o,
        }
    }

    /// `getSourceFragmentLineMaps()`.
    pub fn get_source_fragment_line_maps(&self) -> Option<Vec<Rc<dyn std::any::Any>>> {
        if let Some(m) = &self.fragment_line_maps {
            Some(m.values().cloned().collect())
        } else if let Some(d) = &self.delegate {
            d.borrow().get_source_fragment_line_maps()
        } else {
            None
        }
    }

    pub fn clear_source_fragments(&mut self) {
        self.fragments = None;
    }

    /// `addFileIncludes(source)`.
    pub fn add_file_includes(&mut self, other: &Source) {
        for (f, t) in other.file_include_times.iter() {
            if !self.file_include_times.contains_key(f) {
                self.file_include_times.insert(f.clone(), *t);
            }
        }
    }

    /// `getFileIncludes()` with `getFileIncludeTime(f)`.
    pub fn get_file_includes(&self) -> Vec<(String, i64)> {
        self.file_include_times.iter().map(|(f, t)| (f.clone(), *t)).collect()
    }

    /// `getUpdatedFileIncludes()`.
    pub fn get_updated_file_includes(&self) -> Vec<String> {
        self.file_include_times.iter().filter(|(f, t)| last_modified(Path::new(f.as_str())) != **t).map(|(f, _)| f.clone()).collect()
    }

    /// `Source.populateSource`/`newSource`'s include times.
    pub fn set_file_include_times(&mut self, includes: Vec<(String, i64)>) {
        for (f, t) in includes {
            self.file_include_times.insert(f, t);
        }
    }

    pub fn get_file_time(&self) -> i64 {
        self.file_time
    }

    pub fn set_file_time(&mut self, t: i64) {
        self.file_time = t;
    }

    pub fn set_path_root(&mut self, root: Option<String>) {
        self.path_root = root;
    }

    pub fn set_flags(&mut self, relative_path: &str, short_name: &str, is_internal: bool, is_root: bool, is_debuggable: bool) {
        self.relative_path = relative_path.to_string();
        self.short_name = short_name.to_string();
        self.is_internal = is_internal;
        self.is_root = is_root;
        self.is_debuggable = is_debuggable;
    }

    /// `removeCompilationUnit()`.
    pub fn remove_compilation_unit(&mut self) {
        if self.is_swc_script_owner() {
            if let Some(u) = &self.unit {
                u.borrow_mut().remove_type_info();
            }
        } else {
            self.unit = None;
        }
        self.file_time = self.file.get_last_modified();
        self.logger = None;
        self.is_preprocessed = false;
        self.file_include_times.clear();
        self.delegate = None;
        if self.fragments.is_some() {
            self.fragments = Some(Default::default());
            self.fragment_line_maps = Some(Default::default());
        }
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

    /// Only `ResourceContainer.addResource` sets the owner after construction.
    pub fn set_owner(&mut self, owner: Owner) {
        self.owner = owner;
    }

    /// `getBackingFile()`: the file itself, as no ported source has a delegate.
    pub fn get_backing_file(&self) -> &VirtualFile {
        &self.file
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
        self.file.get_last_modified() != self.file_time || self.file_include_times.iter().any(|(f, t)| last_modified(Path::new(f.as_str())) != *t)
    }

    pub fn exists(&self) -> bool {
        match &self.file {
            VirtualFile::Local { name, .. } => Path::new(name).exists(),
            VirtualFile::InMemory { .. } | VirtualFile::Text { .. } => true,
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

    /// `addFileInclude(VirtualFile)`: a file by name, with its modification time.
    pub fn add_file_include(&mut self, path: String) {
        if !self.file_include_times.contains_key(&path) {
            let t = last_modified(Path::new(&path));
            self.file_include_times.insert(path, t);
        }
    }

    pub fn is_included_file(&self, name: &str) -> bool {
        self.file_include_times.contains_key(name)
    }

    pub fn get_file_include_size(&self) -> usize {
        self.file_include_times.len()
    }

    pub fn is_resource_bundle_path_owner(&self) -> bool {
        matches!(self.owner, Owner::ResourceBundlePath)
    }

    pub fn is_compiler_swc_context_owner(&self) -> bool {
        matches!(self.owner, Owner::CompilerSwcContext)
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

/// `Source.transferDefinitions(from, to)` and the other static `transfer*` helpers the delegating compilers use.
pub fn transfer_definitions(from: &CompilationUnit, to: &mut CompilationUnit) {
    for q in from.top_level_definitions.0.iter() {
        to.top_level_definitions.add(q.clone());
    }
}

pub fn transfer_type_info(from: &CompilationUnit, to: &mut CompilationUnit) {
    to.type_info = from.type_info.clone();
}

pub fn clear_dependencies(unit: &mut CompilationUnit) {
    unit.inheritance.clear();
    unit.types.clear();
    unit.expressions.clear();
    unit.namespaces.clear();
    unit.import_package_statements.clear();
    unit.import_definition_statements.clear();
}

fn put_all(to: &mut super::util::MultiNameMap, from: &super::util::MultiNameMap) {
    for (k, v) in from.iter() {
        to.put(k.clone(), v.clone());
    }
}

pub fn transfer_inheritance(from: &CompilationUnit, to: &mut CompilationUnit) {
    to.inheritance.clear();
    for n in from.inheritance.iter() {
        to.inheritance.insert(n.clone());
    }
    put_all(&mut to.inheritance_history, &from.inheritance_history);
}

pub fn transfer_dependencies(from: &CompilationUnit, to: &mut CompilationUnit) {
    clear_dependencies(to);
    for n in from.inheritance.iter() {
        to.inheritance.insert(n.clone());
    }
    for n in from.types.iter() {
        to.types.insert(n.clone());
    }
    for n in from.expressions.iter() {
        to.expressions.insert(n.clone());
    }
    for n in from.namespaces.iter() {
        to.namespaces.insert(n.clone());
    }
    for n in from.import_package_statements.iter() {
        to.import_package_statements.insert(n.clone());
    }
    for n in from.import_definition_statements.iter() {
        to.import_definition_statements.insert(n.clone());
    }
    put_all(&mut to.inheritance_history, &from.inheritance_history);
    put_all(&mut to.type_history, &from.type_history);
    put_all(&mut to.expression_history, &from.expression_history);
    put_all(&mut to.namespace_history, &from.namespace_history);
}

pub fn transfer_namespaces(from: &CompilationUnit, to: &mut CompilationUnit) {
    to.namespaces.clear();
    for n in from.namespaces.iter() {
        to.namespaces.insert(n.clone());
    }
    put_all(&mut to.namespace_history, &from.namespace_history);
}

pub fn transfer_expressions(from: &CompilationUnit, to: &mut CompilationUnit) {
    to.expressions.clear();
    for n in from.expressions.iter() {
        to.expressions.insert(n.clone());
    }
    put_all(&mut to.expression_history, &from.expression_history);
}

pub fn transfer_assets(from: &CompilationUnit, to: &mut CompilationUnit) {
    if from.has_assets() {
        to.assets.add_all(&from.assets);
    }
}

pub fn transfer_meta_data(from: &CompilationUnit, to: &mut CompilationUnit) {
    for m in &from.metadata {
        if !to.metadata.iter().any(|x| Rc::ptr_eq(x, m)) {
            to.metadata.push(m.clone());
        }
    }
    to.swf_meta_data = from.swf_meta_data.clone();
    to.icon = from.icon.clone();
    to.icon_file = from.icon_file.clone();
    to.signature_checksum = from.signature_checksum;
    to.loader_class = from.loader_class.clone();
    for c in from.extra_classes.iter() {
        to.extra_classes.insert(c.clone());
    }
    if let Some(a) = &from.accessibility_classes {
        for c in a.iter() {
            to.add_accessibility_class(c.clone());
        }
    }
    for (k, v) in from.licensed_class_reqs.iter() {
        to.licensed_class_reqs.insert(k.clone(), v.clone());
    }
    for (k, v) in from.remote_class_aliases.iter() {
        to.remote_class_aliases.insert(k.clone(), v.clone());
    }
    for (k, v) in from.effect_triggers.iter() {
        to.effect_triggers.insert(k.clone(), v.clone());
    }
    for m in from.mixins.iter() {
        to.mixins.insert(m.clone());
    }
    for r in from.resource_bundles.iter() {
        to.resource_bundles.insert(r.clone());
    }
    for r in from.resource_bundle_history.iter() {
        to.resource_bundle_history.insert(r.clone());
    }
}

pub fn transfer_generated_sources(from: &mut CompilationUnit, to: &mut CompilationUnit) {
    if let Some(g) = from.get_generated_sources().cloned() {
        to.add_generated_sources(g);
    }
    from.clear_generated_sources();
}

pub fn transfer_class_table(from: &CompilationUnit, to: &mut CompilationUnit) {
    for (k, v) in from.class_table.iter() {
        to.class_table.insert(k.clone(), v.clone());
    }
}

pub fn transfer_loader_class_base(from: &CompilationUnit, to: &mut CompilationUnit) {
    to.loader_class_base = from.loader_class_base.clone();
}

pub fn transfer_bytecodes(from: &CompilationUnit, to: &mut CompilationUnit) {
    to.bytes.clear();
    to.bytes.extend_from_slice(&from.bytes);
}

pub fn transfer_styles(from: &CompilationUnit, to: &mut CompilationUnit) {
    to.styles = from.styles.clone();
}

/// `source.resolve(include)` (`Source.Resolver`, then the global resolver) as the resolved file's name.
pub fn resolve_from(source: &SourceRef, relative: &str) -> Option<String> {
    let s = source.borrow();
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if !relative.starts_with('/') {
        if let Some(p) = s.get_parent() {
            candidates.push(Path::new(&p).join(relative));
        }
    }
    if let (Some(root), Some(rest)) = (s.get_path_root(), relative.strip_prefix('/')) {
        candidates.push(Path::new(root).join(rest));
    }
    candidates.push(std::path::PathBuf::from(relative));
    candidates.into_iter().find(|c| c.is_file()).map(|c| canonical_path(&c))
}

/// `Source.copy()`: a finished source as its bytecode, with a fresh unit whose dependencies are the histories'
/// multinames, to be resolved again.
pub fn copy(this: &SourceRef) -> Option<SourceRef> {
    let (unit, s) = {
        let b = this.borrow();
        let u = b.unit.clone().filter(|u| u.borrow().is_done())?;
        let f = VirtualFile::InMemory { bytes: Rc::new(u.borrow().get_byte_codes().to_vec()), name: b.get_name().to_string(), mime_type: mime_mappings::ABC, last_modified: b.file_time };
        let s = Source::new(f, b.path_root.clone(), &b.relative_path, &b.short_name, b.owner.clone(), b.is_internal, b.is_root, b.is_debuggable);
        s.borrow_mut().file_time = b.file_time;
        s.borrow_mut().file_include_times = b.file_include_times.clone();
        s.borrow_mut().logger = b.logger.clone();
        (u, s)
    };
    let nu = Source::new_compilation_unit(&s, None);
    copy_compilation_unit(&unit.borrow(), &mut nu.borrow_mut(), true);
    Some(s)
}

/// `Source.copyCompilationUnit(fromUnit, toUnit, useHistories)`.
pub fn copy_compilation_unit(from: &CompilationUnit, to: &mut CompilationUnit, use_histories: bool) {
    for q in from.top_level_definitions.0.iter() {
        to.top_level_definitions.add(q.clone());
    }
    use super::util::Name;
    if use_histories {
        for (m, _) in from.inheritance_history.iter() {
            to.inheritance.insert(Name::M(m.clone()));
        }
        for (m, _) in from.type_history.iter() {
            to.types.insert(Name::M(m.clone()));
        }
        for (m, _) in from.namespace_history.iter() {
            to.namespaces.insert(Name::M(m.clone()));
        }
        for (m, _) in from.expression_history.iter() {
            to.expressions.insert(Name::M(m.clone()));
        }
    } else {
        for n in from.inheritance.iter() {
            to.inheritance.insert(n.clone());
        }
        for n in from.types.iter() {
            to.types.insert(n.clone());
        }
        for n in from.namespaces.iter() {
            to.namespaces.insert(n.clone());
        }
        for n in from.expressions.iter() {
            to.expressions.insert(n.clone());
        }
    }
    put_all(&mut to.inheritance_history, &from.inheritance_history);
    put_all(&mut to.type_history, &from.type_history);
    put_all(&mut to.namespace_history, &from.namespace_history);
    put_all(&mut to.expression_history, &from.expression_history);
    to.signature_checksum = from.signature_checksum;
    if from.has_assets() {
        to.assets.add_all(&from.assets);
    }
    to.aux_generate_info = from.aux_generate_info.clone();
    to.icon = from.icon.clone();
    to.icon_file = from.icon_file.clone();
    if from.has_type_info {
        to.styles = from.styles.clone();
        to.type_info = from.type_info.clone();
        to.has_type_info = true;
        transfer_class_table(from, to);
        to.swf_meta_data = from.swf_meta_data.clone();
        to.loader_class = from.loader_class.clone();
        to.loader_class_base = from.loader_class_base.clone();
        for c in from.extra_classes.iter() {
            to.extra_classes.insert(c.clone());
        }
        if let Some(a) = &from.accessibility_classes {
            for c in a.iter() {
                to.add_accessibility_class(c.clone());
            }
        }
        for (k, v) in from.licensed_class_reqs.iter() {
            to.licensed_class_reqs.insert(k.clone(), v.clone());
        }
        for (k, v) in from.remote_class_aliases.iter() {
            to.remote_class_aliases.insert(k.clone(), v.clone());
        }
        for (k, v) in from.effect_triggers.iter() {
            to.effect_triggers.insert(k.clone(), v.clone());
        }
        for m in from.mixins.iter() {
            to.mixins.insert(m.clone());
        }
        for r in from.resource_bundle_history.iter() {
            to.resource_bundle_history.insert(r.clone());
        }
        to.bytes = from.bytes.clone();
    }
}

/// `Source.equals`: the same owner kind, name and relative path.
pub fn java_equals(a: &SourceRef, b: &SourceRef) -> bool {
    if Rc::ptr_eq(a, b) {
        return true;
    }
    let (a, b) = (a.borrow(), b.borrow());
    std::mem::discriminant(&a.owner) == std::mem::discriminant(&b.owner) && a.get_name() == b.get_name() && a.relative_path == b.relative_path
}
