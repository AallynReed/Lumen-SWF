//! Ports of `flex2.compiler.Transcoder`, `TranscoderException`, the `PathResolver`s an embed is resolved through
//! (`flex2.compiler.common`), and the `ThreadLocalToolkit` state transcoding shares (resolved paths, `StandardDefs`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::source::{canonical_path, VirtualFile};
use super::symbol_table::SymbolTable;
use crate::asc::flex2::flash::swf::tags::TagRef;
use crate::asc::util::jhash::JHashMap;

pub const FILE: &str = "_file";
pub const PATHSEP: &str = "_pathsep";
pub const LINE: &str = "_line";
pub const COLUMN: &str = "_column";
pub const SOURCE: &str = "source";
pub const SYMBOL: &str = "symbol";
pub const NEWNAME: &str = "exportSymbol";
pub const MIMETYPE: &str = "mimeType";
pub const RESOLVED_SOURCE: &str = "_resolvedSource";
pub const ORIGINAL: &str = "original";
pub const SKINCLASS: &str = "skinClass";

/// The `Map<String, Object>` of Embed parameters; every value mxmlc puts in it is a `String`.
pub type Args = JHashMap<String, String>;

pub type ResultsRef = Rc<RefCell<TranscodingResults>>;

/// `Transcoder.TranscodingResults`.
#[derive(Default)]
pub struct TranscodingResults {
    pub define_tag: Option<TagRef>,
    pub generated_code: Option<String>,
    pub asset_source: Option<VirtualFile>,
    pub class_name: Option<String>,
    pub additional_assets: Option<Vec<ResultsRef>>,
    pub modified: i64,
    pub require_codegen: bool,
}

impl TranscodingResults {
    /// `new TranscodingResults(assetSource)`.
    pub fn with_source(asset_source: VirtualFile) -> TranscodingResults {
        let modified = asset_source.get_last_modified();
        TranscodingResults { asset_source: Some(asset_source), modified, ..Default::default() }
    }
}

/// `TranscoderException` and its subclasses, as the English message `ThreadLocalToolkit.log` prints.
#[derive(Clone, Debug)]
pub struct TranscoderException {
    pub message: String,
    /// A Java runtime exception (`NumberFormatException`, ...), which `EmbedUtil` lets through to `EmbedEvaluator`.
    pub runtime: bool,
    pub path: Option<String>,
    pub line: i32,
    pub column: i32,
}

impl TranscoderException {
    pub fn new(message: impl Into<String>) -> TranscoderException {
        TranscoderException { message: message.into(), runtime: false, path: None, line: -1, column: -1 }
    }

    pub fn unrecognized_extension(source: &str) -> Self {
        Self::new(format!("'{source}' does not have a recognized extension, and a mimeType was not provided"))
    }
    pub fn no_matching_transcoder(mime_type: &str) -> Self {
        Self::new(format!("no transcoder registered for mimeType '{mime_type}'"))
    }
    pub fn unsupported_attribute(attribute: &str, class_name: &str) -> Self {
        Self::new(format!("transcoding parameter '{attribute}' is not supported by '{class_name}'"))
    }
    pub fn unable_to_resolve(source: &str) -> Self {
        Self::new(format!("unable to resolve '{source}' for transcoding"))
    }
    pub fn network_transcoding_source(url: &str) -> Self {
        Self::new(format!("network resource {url} cannot be embedded at compile time; please use a local file or load the resource at runtime"))
    }
    pub fn missing_source() -> Self {
        Self::new("no source provided to transcode")
    }
    pub fn unable_to_generate_source(class_name: &str) -> Self {
        Self::new(format!("unable to generate source for class '{class_name}'"))
    }
    pub fn unable_to_read_source(source: &str) -> Self {
        Self::new(format!("unable to read transcoding source '{source}'"))
    }
    pub fn exception_while_transcoding(exception: &str) -> Self {
        Self::new(format!("exception during transcoding: {exception}"))
    }
    pub fn embed_requires_codegen(source: &str, class_name: &str) -> Self {
        Self::new(format!("'{source}' results in generated code, cannot use existing class '{class_name}'"))
    }
    pub fn incompatible_transcoder_parameters(param1: &str, param2: &str) -> Self {
        Self::new(format!("transcoding parameters '{param1}' and '{param2}' are incompatible"))
    }
}

/// `flex2.compiler.Transcoder`.
pub trait Transcoder {
    fn is_supported(&self, mime_type: &str) -> bool;
    fn transcode(&mut self, context: &PathResolver, symbol_table: Option<&SymbolTable>, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException>;
    /// `getAssociatedClass(tag)`.
    fn get_associated_class(&self, tag: Option<&TagRef>) -> Option<String>;
    fn clear(&mut self);
}

/// The `SinglePathResolver`s an embed source is resolved against.
#[derive(Clone, Debug)]
pub enum SinglePathResolver {
    /// `LocalFile.resolve`: relative to a directory, or to a file's directory.
    LocalFile(String),
    /// `SourcePath.resolve`: a leading-slash path under one of the source path roots.
    SourcePath(Vec<String>),
    /// `LocalFilePathResolver`: absolute paths only.
    LocalFilePath,
    /// `ThreadLocalToolkit.getPathResolver()` as `CompilerAPI.usePathResolver()` makes it.
    Toolkit,
}

/// `java.io.File(parent, child)`, which keeps a child's leading separator under the parent.
fn child_of(parent: &Path, child: &str) -> PathBuf {
    let trimmed = child.trim_start_matches(['/', '\\']);
    if child.len() != trimmed.len() || !Path::new(child).is_absolute() {
        parent.join(trimmed)
    } else {
        parent.join(child)
    }
}

fn local_file(p: &Path) -> Option<VirtualFile> {
    if p.exists() {
        Some(VirtualFile::local(p))
    } else {
        None
    }
}

impl SinglePathResolver {
    pub fn resolve(&self, path: &str) -> Option<VirtualFile> {
        match self {
            SinglePathResolver::LocalFile(f) => {
                let f = Path::new(f);
                let dir = if f.is_dir() {
                    f.to_path_buf()
                } else if f.is_file() {
                    f.parent()?.to_path_buf()
                } else {
                    return None;
                };
                local_file(&child_of(&dir, path))
            }
            SinglePathResolver::SourcePath(dirs) => {
                let rel = path.strip_prefix('/')?;
                dirs.iter().find_map(|d| local_file(&child_of(Path::new(d), rel)))
            }
            SinglePathResolver::LocalFilePath | SinglePathResolver::Toolkit => {
                let p = Path::new(path);
                if p.is_absolute() {
                    local_file(p)
                } else {
                    None
                }
            }
        }
    }
}

/// `flex2.compiler.common.PathResolver`.
#[derive(Clone, Debug, Default)]
pub struct PathResolver {
    bases: Vec<SinglePathResolver>,
}

impl PathResolver {
    pub fn new() -> PathResolver {
        PathResolver::default()
    }

    pub fn add_single_path_resolver(&mut self, r: SinglePathResolver) {
        self.bases.push(r);
    }

    pub fn resolve(&self, path: &str) -> Option<VirtualFile> {
        self.bases.iter().find_map(|b| b.resolve(path))
    }
}

/// `LocalFilePathResolver.getSingleton().resolve(path)`.
pub fn resolve_local_file_path(path: &str) -> Option<VirtualFile> {
    SinglePathResolver::LocalFilePath.resolve(path)
}

thread_local! {
    static RESOLVED_PATHS: RefCell<HashMap<String, VirtualFile>> = RefCell::new(HashMap::new());
}

/// `ThreadLocalToolkit.addResolvedPath`.
pub fn add_resolved_path(path: &str, file: &VirtualFile) {
    RESOLVED_PATHS.with(|m| m.borrow_mut().insert(path.to_string(), file.clone()));
}

/// `ThreadLocalToolkit.getResolvedPath`.
pub fn get_resolved_path(path: &str) -> Option<VirtualFile> {
    RESOLVED_PATHS.with(|m| m.borrow().get(path).cloned())
}

/// `CompilerAPI.removePathResolver()`'s `resetResolvedPaths`, and the per-JVM state a new mxmlc run starts without.
pub fn reset() {
    RESOLVED_PATHS.with(|m| m.borrow_mut().clear());
    crate::asc::flex2::flash::graphics::images::reset();
}

/// `StandardDefs.getCorePackage()`, the same for the Spark and MX defs.
pub const CORE_PACKAGE: &str = "mx.core";

/// A local file's canonical name, as `LocalFile.getName()` gives it.
pub fn local_name(p: &Path) -> String {
    canonical_path(p)
}

/// The `Transcoder[]` `WebTierAPI.getTranscoders` makes once and every compiler shares.
pub type Transcoders = Rc<RefCell<Vec<Box<dyn Transcoder>>>>;

thread_local! {
    static SOURCE_PATH_DIRS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// The source path directories `SourcePath.resolve` searches for a leading-slash embed.
pub fn set_source_path_dirs(dirs: Vec<String>) {
    SOURCE_PATH_DIRS.with(|d| *d.borrow_mut() = dirs);
}

pub fn source_path_dirs() -> Vec<String> {
    SOURCE_PATH_DIRS.with(|d| d.borrow().clone())
}
