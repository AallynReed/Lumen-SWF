//! Port of `flex2.tools.Mxmlc.mxmlc` for one ActionScript root: source list and path, libraries, the incremental
//! cache check, compile, link.

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
use crate::asc::flex2::compiler::i18n::resource_file::ResourceBundlePath;
use crate::asc::flex2::compiler::incremental;
use crate::asc::flex2::compiler::persistence_store::{self, ReadResult};
use crate::asc::flex2::tools::swc_checksums::SwcChecksums;

pub struct Compiled {
    pub sources: Vec<crate::asc::flex2::compiler::source::SourceRef>,
    pub units: Vec<UnitRef>,
    pub movie: Option<Movie>,
    /// `-incremental`: the cache to write beside the output.
    pub cache: Option<Vec<u8>>,
    /// `-incremental` and nothing changed (`NoUpdateMessage`).
    pub up_to_date: bool,
}

pub fn source_path_mime_types() -> [&'static str; 3] {
    [mime_mappings::AS, mime_mappings::MXML, mime_mappings::FXG]
}

/// `WebTierAPI.getCompilers`, with the ported extensions.
pub fn compilers(config: &Rc<Configuration>) -> Vec<Box<dyn SubCompiler>> {
    let transcoders: crate::asc::flex2::compiler::transcoder::Transcoders = Rc::new(std::cell::RefCell::new(crate::asc::flex2::compiler::media::get_transcoders(config)));
    let mut asc = As3Compiler::new(config.clone());
    if !config.disable_incremental_optimizations {
        asc.add_compiler_extension(Box::new(signature_extension(config)));
    }
    asc.add_compiler_extension(Box::new(crate::asc::flex2::compiler::as3::embed_extension::EmbedExtension::new(transcoders.clone(), config.generated_directory.clone(), config.show_deprecation_warnings)));
    asc.add_compiler_extension(Box::new(crate::asc::flex2::compiler::as3::style_evaluator::StyleExtension));
    asc.add_compiler_extension(Box::new(crate::asc::flex2::compiler::as3::host_component_extension::HostComponentExtension::new(config.flex.report_missing_required_skin_parts_as_warnings)));
    asc.add_compiler_extension(Box::new(crate::asc::flex2::compiler::as3::binding::bindable_extension::BindableExtension::new(config.generated_directory.clone(), config.get_generate_abstract_syntax_tree(), false)));
    asc.add_compiler_extension(Box::new(crate::asc::flex2::compiler::as3::skin_part_extension::SkinPartExtension));
    let mxmlc = {
        let mappings = Rc::new(crate::asc::flex2::compiler::util::name_mappings::get_name_mappings(&config.css.namespaces));
        crate::asc::flex2::compiler::mxml::mxml_compiler::MxmlCompiler::new(config.clone(), mappings, transcoders.clone())
    };
    let i18n = crate::asc::flex2::compiler::i18n::i18n_compiler::I18nCompiler::new(config.clone(), transcoders);
    let mut compilers: Vec<Box<dyn SubCompiler>> = vec![Box::new(asc), Box::new(AbcCompiler), Box::new(i18n), css_compiler(config)];
    compilers.insert(1, Box::new(mxmlc));
    compilers
}

/// `SignatureExtension.init(compilerConfig)` and `getInstance()`.
pub fn signature_extension(config: &Configuration) -> crate::asc::flex2::compiler::as3::signature_extension::SignatureExtension {
    crate::asc::flex2::compiler::as3::signature_extension::SignatureExtension::new(config.signature_directory.is_some(), config.signature_directory.clone())
}

/// `new CssCompiler(compilerConfig, transcoders, mappings)`: its own `As3Compiler` with an `EmbedExtension`.
fn css_compiler(config: &Rc<Configuration>) -> Box<dyn SubCompiler> {
    let transcoders: crate::asc::flex2::compiler::transcoder::Transcoders = Rc::new(std::cell::RefCell::new(crate::asc::flex2::compiler::media::get_transcoders(config)));
    let mut asc = As3Compiler::new(config.clone());
    asc.add_compiler_extension(Box::new(crate::asc::flex2::compiler::as3::embed_extension::EmbedExtension::new(transcoders, config.generated_directory.clone(), config.show_deprecation_warnings)));
    let css_config = Rc::new(config.css.compiler_configuration(config.debug, config.generated_directory.clone()));
    let mappings = Rc::new(crate::asc::flex2::compiler::util::name_mappings::get_name_mappings(&config.css.namespaces));
    Box::new(crate::asc::flex2::compiler::css::css_compiler::CssCompiler::new(css_config, Box::new(asc), Some(mappings)))
}

/// The per-run `ThreadLocalToolkit` state transcoding uses, as a new mxmlc process starts with it.
fn reset_transcoding(configuration: &Configuration) {
    super::pre_link_css::reset();
    crate::asc::flex2::compiler::i18n::resource_file::reset();
    crate::asc::flex2::compiler::transcoder::reset();
    crate::asc::flex2::compiler::transcoder::set_source_path_dirs(configuration.source_path.iter().filter(|p| p.is_dir()).map(|p| canonical_path(p)).collect());
}

/// `CompilerAPI.compile` with `PreLink`, then `LinkerAPI.link` (with `PostLink` when optimizing a release build).
pub fn mxmlc(mut configuration: Configuration, main: &Path, libraries: Vec<(Rc<crate::asc::flex2::compiler::swc::Swc>, bool)>) -> Compiled {
    let fail = Compiled { sources: Vec::new(), units: Vec::new(), movie: None, cache: None, up_to_date: false };
    if !main.is_file() {
        toolkit::log(CompilerMessage::error(format!("unable to open '{}'", main.display())), None, -1, -1, None);
        return fail;
    }
    let target_file = VirtualFile::local(main);
    let class_path: Vec<String> = configuration.source_path.iter().filter(|p| p.is_dir()).map(|p| canonical_path(p)).collect();
    let new_source_list = || SourceList::new(std::slice::from_ref(&target_file), &class_path, Some(&target_file));
    let overlap = configuration.allow_source_path_overlap;
    let new_source_path = || SourcePath::new(&class_path, Some(&target_file), &source_path_mime_types(), overlap);
    let main_name = std::path::PathBuf::from(target_file.get_name());
    let (locales, source_dirs) = (configuration.flex.locales.clone(), configuration.source_path.clone());
    let new_bundle_path = || ResourceBundlePath::new(&locales, &source_dirs, Some(&main_name));
    let Ok(mut source_list) = new_source_list() else { return fail };
    let mut source_path = new_source_path();
    let mut bundle_path = new_bundle_path();
    let mut swc_context = CompilerSwcContext::new();
    swc_context.load_with(&configuration.library_path, &configuration.external_library_path, libraries);
    swc_context.load_themes(&configuration.css.theme_files().0);
    configuration.add_externs(swc_context.get_externs());
    if toolkit::error_count() > 0 {
        return fail;
    }
    let config_rc = Rc::new(configuration.clone());
    // Mxmlc's incremental branch: the cache beside the output decides whether anything needs compiling.
    let mut recompile = true;
    let mut swc_checksums = None;
    if configuration.incremental {
        let mut sc = SwcChecksums::new(&swc_context, configuration.checksum_ts, configuration.link_checksum_ts, configuration.swc_checksum);
        let output = configuration.output.clone().unwrap_or_default();
        let cache_name = format!("{}.cache", output.display());
        if output.exists() {
            let read = std::fs::read(&cache_name);
            if let Err(e) = &read {
                // `FileNotFoundException.getLocalizedMessage()` through `logDebug`, which mxmlc's console prints.
                let reason = e.to_string();
                let reason = reason.split(" (os error").next().unwrap_or("").trim_end_matches('.');
                toolkit::log_info(&format!("{cache_name} ({reason})"));
            }
            if let Ok(bytes) = read {
                let mut loaded = sc.checksums;
                match persistence_store::read(&bytes, loaded[1]) {
                    Err(e) => toolkit::log_info(e.0.as_deref().unwrap_or("null")),
                    Ok(ReadResult::Obsolete) => toolkit::log_info(persistence_store::OBSOLETE_CACHE_FILE_FORMAT),
                    Ok(ReadResult::ChecksumMismatch(_)) => toolkit::log_info(&incremental::failed_to_match(&cache_name)),
                    Ok(ReadResult::Data(data)) => {
                        loaded = data.checksums;
                        if incremental::load_compilation_units(&data, &mut source_list, &mut source_path, &mut bundle_path, &mut sc) < 0 {
                            toolkit::log_info(&incremental::failed_to_match(&cache_name));
                        } else if !(sc.is_recompilation_needed(&loaded, &mut swc_context) && !sc.is_relink_needed(&loaded)) {
                            recompile = false;
                        }
                    }
                }
                if recompile {
                    let Ok(l) = new_source_list() else { return fail };
                    source_list = l;
                    source_path = new_source_path();
                    bundle_path = new_bundle_path();
                }
            }
        }
        let count = incremental::validate_compilation_units(&source_list, &mut source_path, Some(&bundle_path), &swc_context, &config_rc);
        recompile = recompile || count > 0;
        swc_checksums = Some(sc);
    }
    if !recompile {
        return Compiled { up_to_date: true, ..fail };
    }
    swc_context.resource_bundles.bundle_path = Some(bundle_path);
    reset_transcoding(&configuration);
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
        super::pre_link::pre_link(api)
    });
    api.swc_context.close();
    let Ok(units) = units else { return Compiled { sources: api.sources, ..fail } };
    api.configuration.add_externs(crate::asc::flex2::compiler::css::css_compiler::take_externs());
    let movie = linker::link(&units, &api.configuration).ok();
    let cache = swc_checksums.map(|mut sc| {
        sc.save_checksums(&units, &api.swc_context);
        let (sl, sp) = (api.source_list.as_ref().unwrap(), api.source_path.as_ref().unwrap());
        incremental::persist_compilation_units(sl, sp, api.swc_context.resource_bundles.bundle_path.as_ref(), &api.sources, &sc, api.configuration.target_player_major)
    });
    Compiled { sources: api.sources, units, movie, cache, up_to_date: false }
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
    reset_transcoding(&configuration);
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
        super::pre_link::pre_link(api)
    });
    Compiled { units: units.unwrap_or_default(), sources: api.sources, movie: None, cache: None, up_to_date: false }
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
