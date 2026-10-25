//! Port of `flex2.tools.Mxmlc.mxmlc` for one ActionScript root: source list and path, libraries, compile, link.

use std::path::Path;
use std::rc::Rc;

use crate::asc::flex2::compiler::abc::abc_compiler::AbcCompiler;
use crate::asc::flex2::compiler::as3::as3_compiler::{As3Compiler, SubCompiler};
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::compiler_api::{CompilerApi, DependencyGraph};
use crate::asc::flex2::compiler::compiler_swc_context::CompilerSwcContext;
use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::compiler::source::{canonical_path, VirtualFile};
use crate::asc::flex2::compiler::source_path::{SourceList, SourcePath};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::mime_mappings;
use crate::asc::flex2::linker::{self, Movie};

pub struct Compiled {
    pub sources: Vec<crate::asc::flex2::compiler::source::SourceRef>,
    pub units: Vec<UnitRef>,
    pub movie: Option<Movie>,
}

pub fn source_path_mime_types() -> [&'static str; 3] {
    [mime_mappings::AS, mime_mappings::MXML, mime_mappings::FXG]
}

pub fn compilers(config: &Rc<Configuration>) -> Vec<Box<dyn SubCompiler>> {
    vec![Box::new(As3Compiler::new(config.clone())), Box::new(AbcCompiler)]
}

/// `CompilerAPI.compile` with `PreLink`, then `LinkerAPI.link` (with `PostLink` when optimizing a release build).
pub fn mxmlc(mut configuration: Configuration, main: &Path) -> Compiled {
    let fail = Compiled { sources: Vec::new(), units: Vec::new(), movie: None };
    if !main.is_file() {
        toolkit::log(CompilerMessage::error(format!("unable to open '{}'", main.display())), None, -1, -1, None);
        return fail;
    }
    let target_file = VirtualFile::local(main);
    let class_path: Vec<String> = configuration.source_path.iter().filter(|p| p.is_dir()).map(|p| canonical_path(p)).collect();
    let source_list = match SourceList::new(std::slice::from_ref(&target_file), &class_path, Some(&target_file)) {
        Ok(s) => s,
        Err(_) => return fail,
    };
    let source_path = SourcePath::new(&class_path, Some(&target_file), &source_path_mime_types(), configuration.allow_source_path_overlap);
    let mut swc_context = CompilerSwcContext::new();
    swc_context.load(&configuration.library_path, &configuration.external_library_path);
    configuration.add_externs(swc_context.get_externs());
    if toolkit::error_count() > 0 {
        return fail;
    }
    let config_rc = Rc::new(configuration.clone());
    let mut api = CompilerApi {
        sources: Vec::new(),
        units: Vec::new(),
        igraph: DependencyGraph::default(),
        symbol_table: SymbolTable::new(&configuration),
        compilers: compilers(&config_rc),
        source_list: Some(source_list),
        source_path: Some(source_path),
        swc_context,
        configuration,
    };
    let units = api.compile(&mut |api: &mut CompilerApi| {
        let units: Vec<UnitRef> = api.units.iter().flatten().cloned().collect();
        super::pre_link::run(&units, &mut api.configuration)
    });
    let Ok(units) = units else { return Compiled { sources: api.sources, ..fail } };
    let movie = linker::link(&units, &api.configuration).ok();
    Compiled { sources: api.sources, units, movie }
}

/// The compile half of `mxmlc` for one root source that need not exist on disk, against extra in-memory libraries.
pub fn compile_unit(mut configuration: Configuration, root: crate::asc::flex2::compiler::source::SourceRef, libraries: Vec<(Rc<crate::asc::flex2::compiler::swc::Swc>, bool)>) -> Compiled {
    let class_path: Vec<String> = configuration.source_path.iter().filter(|p| p.is_dir()).map(|p| canonical_path(p)).collect();
    let source_path = SourcePath::new(&class_path, None, &source_path_mime_types(), configuration.allow_source_path_overlap);
    let mut swc_context = CompilerSwcContext::new();
    swc_context.load_with(&configuration.library_path, &configuration.external_library_path, libraries);
    configuration.add_externs(swc_context.get_externs());
    let config_rc = Rc::new(configuration.clone());
    let mut api = CompilerApi {
        sources: vec![root],
        units: Vec::new(),
        igraph: DependencyGraph::default(),
        symbol_table: SymbolTable::new(&configuration),
        compilers: compilers(&config_rc),
        source_list: None,
        source_path: Some(source_path),
        swc_context,
        configuration,
    };
    let units = api.compile(&mut |api: &mut CompilerApi| {
        let units: Vec<UnitRef> = api.units.iter().flatten().cloned().collect();
        super::pre_link::run(&units, &mut api.configuration)
    });
    Compiled { units: units.unwrap_or_default(), sources: api.sources, movie: None }
}
