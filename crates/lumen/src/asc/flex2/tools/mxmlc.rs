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
use crate::asc::flex2::compiler::util::{mime_mappings, MultiName, Name};
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
    vec![Box::new(As3Compiler::new(config.clone())), Box::new(AbcCompiler), css_compiler(config)]
}

/// `new CssCompiler(compilerConfig, transcoders, mappings)`: its own `As3Compiler`.
fn css_compiler(config: &Rc<Configuration>) -> Box<dyn SubCompiler> {
    let asc = As3Compiler::new(config.clone());
    let css_config = Rc::new(config.css.compiler_configuration(config.debug, config.generated_directory.clone()));
    let mappings = Rc::new(crate::asc::flex2::compiler::util::name_mappings::get_name_mappings(&config.css.namespaces));
    Box::new(crate::asc::flex2::compiler::css::css_compiler::CssCompiler::new(css_config, Box::new(asc), Some(mappings)))
}

/// `CompilerAPI.compile` with `PreLink`, then `LinkerAPI.link` (with `PostLink` when optimizing a release build).
pub fn mxmlc(mut configuration: Configuration, main: &Path, libraries: Vec<(Rc<crate::asc::flex2::compiler::swc::Swc>, bool)>) -> Compiled {
    let fail = Compiled { sources: Vec::new(), units: Vec::new(), movie: None };
    super::pre_link_css::reset();
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
    swc_context.load_with(&configuration.library_path, &configuration.external_library_path, libraries);
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
        let rerun = super::pre_link::run(&units, &mut api.configuration) | super::pre_link_css::process_main_unit_styles(api);
        if !rerun {
            super::pre_link_css::post_run_styles(api);
        }
        rerun
    });
    let Ok(units) = units else { return Compiled { sources: api.sources, ..fail } };
    api.configuration.add_externs(crate::asc::flex2::compiler::css::css_compiler::take_externs());
    let movie = linker::link(&units, &api.configuration).ok();
    Compiled { sources: api.sources, units, movie }
}

/// The compile half of `mxmlc` for one root source that need not exist on disk, against extra in-memory libraries.
/// The root depends on every script of `depends_on`, so it is generated only after they have been analyzed.
pub fn compile_unit(mut configuration: Configuration, root: crate::asc::flex2::compiler::source::SourceRef, libraries: Vec<(Rc<crate::asc::flex2::compiler::swc::Swc>, bool)>, depends_on: &[Rc<crate::asc::flex2::compiler::swc::Swc>]) -> Compiled {
    let class_path: Vec<String> = configuration.source_path.iter().filter(|p| p.is_dir()).map(|p| canonical_path(p)).collect();
    let source_path = SourcePath::new(&class_path, None, &source_path_mime_types(), configuration.allow_source_path_overlap);
    let mut swc_context = CompilerSwcContext::new();
    swc_context.load_with(&configuration.library_path, &configuration.external_library_path, libraries);
    configuration.add_externs(swc_context.get_externs());
    let mut names = Vec::new();
    for lib in depends_on.iter().flat_map(|s| s.libraries.iter()) {
        for script in lib.get_scripts() {
            names.extend(script.get_definitions().first().map(|d| Name::M(MultiName::parse(d))));
        }
    }
    let config_rc = Rc::new(configuration.clone());
    let compilers = compilers(&config_rc).into_iter().map(|c| Box::new(Depending { inner: c, root: root.clone(), names: names.clone() }) as Box<dyn SubCompiler>).collect();
    let mut api = CompilerApi {
        sources: vec![root],
        units: Vec::new(),
        igraph: DependencyGraph::default(),
        symbol_table: SymbolTable::new(&configuration),
        compilers,
        source_list: None,
        source_path: Some(source_path),
        swc_context,
        configuration,
    };
    let units = api.compile(&mut |api: &mut CompilerApi| {
        let units: Vec<UnitRef> = api.units.iter().flatten().cloned().collect();
        let rerun = super::pre_link::run(&units, &mut api.configuration) | super::pre_link_css::process_main_unit_styles(api);
        if !rerun {
            super::pre_link_css::post_run_styles(api);
        }
        rerun
    });
    Compiled { units: units.unwrap_or_default(), sources: api.sources, movie: None }
}

/// A sub-compiler that gives the root unit extra expression dependencies once it is parsed.
struct Depending {
    inner: Box<dyn SubCompiler>,
    root: crate::asc::flex2::compiler::source::SourceRef,
    names: Vec<Name>,
}

impl SubCompiler for Depending {
    fn get_name(&self) -> &'static str {
        self.inner.get_name()
    }
    fn is_supported(&self, mime_type: &str) -> bool {
        self.inner.is_supported(mime_type)
    }
    fn preprocess(&mut self, source: crate::asc::flex2::compiler::source::SourceRef) -> Option<crate::asc::flex2::compiler::source::SourceRef> {
        self.inner.preprocess(source)
    }
    fn parse1(&mut self, source: &crate::asc::flex2::compiler::source::SourceRef, symbol_table: &mut SymbolTable) -> Option<UnitRef> {
        let unit = self.inner.parse1(source, symbol_table);
        if let Some(u) = unit.as_ref().filter(|_| Rc::ptr_eq(source, &self.root)) {
            for n in &self.names {
                u.borrow_mut().expressions.insert(n.clone());
            }
        }
        unit
    }
    fn parse2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        self.inner.parse2(unit, symbol_table)
    }
    fn analyze1(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        self.inner.analyze1(unit, symbol_table)
    }
    fn analyze2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        self.inner.analyze2(unit, symbol_table)
    }
    fn analyze3(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        self.inner.analyze3(unit, symbol_table)
    }
    fn analyze4(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        self.inner.analyze4(unit, symbol_table)
    }
    fn generate(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        self.inner.generate(unit, symbol_table)
    }
    fn postprocess(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        self.inner.postprocess(unit, symbol_table)
    }
}
