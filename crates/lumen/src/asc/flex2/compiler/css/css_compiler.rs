//! Port of `flex2.compiler.css.CssCompiler` (a `.css` source compiled as a style module through `StyleModule.vm`) on
//! `flex2.compiler.AbstractDelegatingSubCompiler` and the `Source.transfer*` helpers it uses. `MxmlLogAdapter`'s line
//! remapping of the generated source's messages is not ported.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::document::{CssConfiguration, VERSION_3_0};
use super::style_module::{PlainStyleModule, StyleModule, StyleModuleOps};
use super::template_context;
use crate::asc::flex2::compiler::as3::as3_compiler::SubCompiler;
use crate::asc::flex2::compiler::compilation_unit::{CompilationUnit, UnitRef};
use crate::asc::flex2::compiler::source::{Source, SourceRef, VirtualFile};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::mime_mappings;
use crate::asc::flex2::compiler::util::name_mappings::NameMappings;
use crate::asc::flex2::flash::css::style_sheet::StyleSheet;

const TEMPLATE_PATH: &str = "flex2/compiler/css/";

/// The `StandardDefs` classes `preprocess` adds as externs: a style module uses the application's.
pub const MODULE_EXTERNS: [&str; 23] = [
    "mx.styles:CSSStyleDeclaration",
    "mx.preloaders:DownloadProgressBar",
    "mx.events:FlexEvent",
    "mx.core:FlexSprite",
    "mx.messaging.config:LoaderConfig",
    "mx.events:ModuleEvent",
    "mx.modules:ModuleManager",
    "mx.preloaders:Preloader",
    "mx.events:StyleEvent",
    "mx.managers:SystemChildrenList",
    "mx.managers:SystemManager",
    "mx.managers:SystemRawChildrenList",
    "mx.core:IChildList",
    "mx.core:IFlexDisplayObject",
    "mx.managers:IFocusManagerContainer",
    "mx.core:IInvalidating",
    "mx.managers:ILayoutManagerClient",
    "mx.modules:IModuleInfo",
    "mx.core:IRawChildrenContainer",
    "mx.styles:ISimpleStyleClient",
    "mx.styles:IStyleClient",
    "mx.managers:ISystemManager",
    "mx.core:IUIComponent",
];

const RESERVED: [&str; 48] = [
    "as", "break", "case", "catch", "class", "continue", "default", "do", "else", "extends", "false", "final", "finally", "for", "function", "goto", "if", "implements", "import", "in", "include", "instanceof", "interface", "is", "namespace", "new", "null", "package", "private", "protected", "public", "return", "static", "super", "switch", "synchronized", "this", "throw", "transient", "true", "try", "typeof", "use", "var", "void", "volatile", "while", "with",
];

/// `TextParser.isValidIdentifier`: Java identifier characters, and not an AS3 reserved word.
pub fn is_valid_identifier(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else { return false };
    if !(first.is_alphabetic() || first == '_' || first == '$' || first.is_ascii_alphabetic()) {
        return false;
    }
    if !chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
        return false;
    }
    !RESERVED.contains(&id)
}

/// `Source.transfer*`: what a delegate unit hands back to its owner.
pub mod transfer {
    use super::*;

    pub fn definitions(from: &CompilationUnit, to: &mut CompilationUnit) {
        for q in from.top_level_definitions.iter() {
            to.top_level_definitions.add(q.clone());
        }
    }

    pub fn type_info(from: &CompilationUnit, to: &mut CompilationUnit) {
        to.type_info = from.type_info.clone();
    }

    fn clear_dependencies(unit: &mut CompilationUnit) {
        unit.inheritance.clear();
        unit.types.clear();
        unit.expressions.clear();
        unit.namespaces.clear();
        unit.import_package_statements.clear();
        unit.import_definition_statements.clear();
    }

    pub fn inheritance(from: &CompilationUnit, to: &mut CompilationUnit) {
        to.inheritance.clear();
        for n in from.inheritance.iter() {
            to.inheritance.insert(n.clone());
        }
        for (k, v) in from.inheritance_history.iter() {
            to.inheritance_history.put(k.clone(), v.clone());
        }
    }

    pub fn dependencies(from: &CompilationUnit, to: &mut CompilationUnit) {
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
        for (k, v) in from.inheritance_history.iter() {
            to.inheritance_history.put(k.clone(), v.clone());
        }
        for (k, v) in from.type_history.iter() {
            to.type_history.put(k.clone(), v.clone());
        }
        for (k, v) in from.expression_history.iter() {
            to.expression_history.put(k.clone(), v.clone());
        }
        for (k, v) in from.namespace_history.iter() {
            to.namespace_history.put(k.clone(), v.clone());
        }
    }

    pub fn namespaces(from: &CompilationUnit, to: &mut CompilationUnit) {
        to.namespaces.clear();
        for n in from.namespaces.iter() {
            to.namespaces.insert(n.clone());
        }
        for (k, v) in from.namespace_history.iter() {
            to.namespace_history.put(k.clone(), v.clone());
        }
    }

    pub fn expressions(from: &CompilationUnit, to: &mut CompilationUnit) {
        to.expressions.clear();
        for n in from.expressions.iter() {
            to.expressions.insert(n.clone());
        }
        for (k, v) in from.expression_history.iter() {
            to.expression_history.put(k.clone(), v.clone());
        }
    }

    pub fn assets(from: &CompilationUnit, to: &mut CompilationUnit) {
        if from.has_assets() {
            to.assets.add_all(&from.assets);
        }
    }

    pub fn meta_data(from: &CompilationUnit, to: &mut CompilationUnit) {
        to.metadata.extend(from.metadata.iter().cloned());
        to.swf_meta_data = from.swf_meta_data.clone();
        to.icon = from.icon.clone();
        to.loader_class = from.loader_class.clone();
        for c in from.extra_classes.iter() {
            to.extra_classes.insert(c.clone());
        }
        to.licensed_class_reqs.extend(from.licensed_class_reqs.iter().map(|(k, v)| (k.clone(), v.clone())));
        to.remote_class_aliases.extend(from.remote_class_aliases.iter().map(|(k, v)| (k.clone(), v.clone())));
        to.effect_triggers.extend(from.effect_triggers.iter().map(|(k, v)| (k.clone(), v.clone())));
        for c in from.mixins.iter() {
            to.mixins.insert(c.clone());
        }
        for c in from.resource_bundles.iter() {
            to.resource_bundles.insert(c.clone());
        }
        for c in from.resource_bundle_history.iter() {
            to.resource_bundle_history.insert(c.clone());
        }
    }

    pub fn generated_sources(from: &mut CompilationUnit, to: &mut CompilationUnit) {
        if let Some(g) = from.get_generated_sources().cloned() {
            to.add_generated_sources(g);
        }
        from.clear_generated_sources();
    }

    pub fn class_table(from: &CompilationUnit, to: &mut CompilationUnit) {
        to.class_table.extend(from.class_table.iter().map(|(k, v)| (k.clone(), v.clone())));
    }

    pub fn loader_class_base(from: &CompilationUnit, to: &mut CompilationUnit) {
        to.loader_class_base = from.loader_class_base.clone();
    }

    pub fn bytecodes(from: &CompilationUnit, to: &mut CompilationUnit) {
        to.bytes = from.bytes.clone();
    }

    pub fn styles(from: &CompilationUnit, to: &mut CompilationUnit) {
        to.styles = from.styles.clone();
    }
}

/// `CssCompiler`: an `As3Compiler` delegate (with its `EmbedExtension`) behind `.css` sources.
pub struct CssCompiler {
    configuration: Rc<CssConfiguration>,
    name_mappings: Option<Rc<NameMappings>>,
    delegate: Box<dyn SubCompiler>,
    /// `CompilerContext.DELEGATE_UNIT`, by the CSS unit, with the generated source that owns it.
    delegates: HashMap<usize, (UnitRef, SourceRef)>,
}

thread_local! {
    /// The externs `preprocess` added (`configuration.addExtern`), for the driver to take before linking.
    static PENDING_EXTERNS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

pub fn take_externs() -> Vec<String> {
    PENDING_EXTERNS.with(|e| std::mem::take(&mut *e.borrow_mut()))
}

fn key(u: &UnitRef) -> usize {
    Rc::as_ptr(u) as usize
}

impl CssCompiler {
    pub fn new(configuration: Rc<CssConfiguration>, delegate: Box<dyn SubCompiler>, name_mappings: Option<Rc<NameMappings>>) -> CssCompiler {
        CssCompiler { configuration, name_mappings, delegate, delegates: HashMap::new() }
    }

    fn delegate_unit(&self, unit: &UnitRef) -> UnitRef {
        self.delegates.get(&key(unit)).map(|d| d.0.clone()).expect("DelegateUnit")
    }

    /// `generateStyleName(source)`.
    fn generate_style_name(source: &Source) -> String {
        let mut result = source.get_name().to_string();
        if let Some(last_separator) = result.rfind(std::path::MAIN_SEPARATOR) {
            result = result[last_separator + 1..].to_string();
            if let Some(extension) = result.find(".css") {
                result.truncate(extension);
            }
        }
        result
    }

    /// `generateSourceCodeFile(compilationUnit, styleModule)`.
    fn generate_source_code_file(&self, style_module: &StyleModule) -> Option<VirtualFile> {
        let template_name = format!("{TEMPLATE_PATH}{}", if !self.configuration.archive_classes_and_assets { "StyleModule.vm" } else { "StyleLibrary.vm" });
        let gen_file_name = format!("{}{}{}-generated.as", self.configuration.generated_directory.as_deref().unwrap_or("null"), std::path::MAIN_SEPARATOR, style_module.get_name().unwrap_or("null"));
        let name = style_module.get_name().unwrap_or("null").to_string();
        let source_code = template_context::merge_mapped(&template_name, self.configuration.debug, true, vec![("styleModule", template_context::style_module(style_module))], &name)?;
        if self.configuration.keep_generated_actionscript {
            if let Err(e) = std::fs::write(&gen_file_name, &source_code) {
                toolkit::log(CompilerMessage::error(format!("Unable to write generated file ({gen_file_name}): {e}.")), None, -1, -1, None);
            }
        }
        Some(VirtualFile::InMemory { bytes: Rc::new(source_code.into_bytes()), name: gen_file_name, mime_type: mime_mappings::AS, last_modified: i64::MAX })
    }
}

impl SubCompiler for CssCompiler {
    fn get_name(&self) -> &'static str {
        "css"
    }

    fn is_supported(&self, mime_type: &str) -> bool {
        mime_type == mime_mappings::CSS
    }

    fn preprocess(&mut self, source: SourceRef) -> Option<SourceRef> {
        if !self.configuration.archive_classes_and_assets {
            PENDING_EXTERNS.with(|e| {
                let mut e = e.borrow_mut();
                for c in MODULE_EXTERNS {
                    if !e.iter().any(|x| x == c) {
                        e.push(c.to_string());
                    }
                }
            });
        }
        let (component_name, reporting) = {
            let s = source.borrow();
            (s.get_short_name().to_string(), s.get_name_for_reporting().to_string())
        };
        if !is_valid_identifier(&component_name) {
            toolkit::log(CompilerMessage::error(format!("Invalid component name '{component_name}': component name must be legal ActionScript class name.")), Some(&reporting), -1, -1, None);
        }
        Some(source)
    }

    fn parse1(&mut self, source: &SourceRef, symbol_table: &mut SymbolTable) -> Option<UnitRef> {
        if let Some(u) = source.borrow().get_compilation_unit() {
            if u.borrow().has_type_info {
                return Some(u);
            }
        }
        let name = source.borrow().get_name().to_string();
        let bytes = match source.borrow().to_byte_array() {
            Ok(b) => b,
            Err(e) => {
                toolkit::log(CompilerMessage::error(e.to_string()), Some(&name), -1, -1, None);
                return None;
            }
        };
        let mut style_sheet = StyleSheet::new();
        style_sheet.check_deprecation(self.configuration.show_deprecation_warnings);
        style_sheet.parse(&name, &bytes);
        if style_sheet.errors_exist() {
            toolkit::log_plain(toolkit::Level::Error, &format!("Error parsing stylesheet: {name}"));
        }
        let mut module = StyleModule::new(source.clone(), symbol_table.per_compile_data.clone());
        if self.configuration.compatibility_version <= VERSION_3_0 {
            module.set_advanced(false);
            module.set_qualified_type_selectors(false);
        } else {
            module.set_qualified_type_selectors(self.configuration.qualified_type_selectors);
        }
        module.set_name_mappings(self.name_mappings.clone());
        module.set_name(Self::generate_style_name(&source.borrow()));
        let module = PlainStyleModule(RefCell::new(module));
        if let Err(m) = module.extract_styles(Some(&style_sheet), false) {
            toolkit::log(CompilerMessage::error(m.unwrap_or_else(|| "null".into())), Some(&name), -1, -1, None);
            return None;
        }
        let css_compilation_unit = Source::new_compilation_unit(source, None);
        let generated_file = self.generate_source_code_file(&module.module())?;
        let generated_source = {
            let s = source.borrow();
            Source::new(generated_file, s.get_path_root().map(str::to_string), s.get_relative_path(), s.get_short_name(), s.get_owner().clone(), s.is_internal(), s.is_root(), s.is_debuggable())
        };
        let asc_compilation_unit = self.delegate.parse1(&generated_source, symbol_table)?;
        let generated_name = generated_source.borrow().get_name().to_string();
        source.borrow_mut().add_file_include(generated_name);
        {
            let mut a = asc_compilation_unit.borrow_mut();
            let mut c = css_compilation_unit.borrow_mut();
            transfer::meta_data(&a, &mut c);
            transfer::generated_sources(&mut a, &mut c);
            transfer::definitions(&a, &mut c);
            transfer::inheritance(&a, &mut c);
        }
        self.delegates.insert(key(&css_compilation_unit), (asc_compilation_unit, generated_source));
        Some(css_compilation_unit)
    }

    fn parse2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        let asc = self.delegate_unit(unit);
        transfer::inheritance(&unit.borrow(), &mut asc.borrow_mut());
        self.delegate.parse2(&asc, symbol_table);
        let mut a = asc.borrow_mut();
        let mut u = unit.borrow_mut();
        transfer::assets(&a, &mut u);
        transfer::generated_sources(&mut a, &mut u);
    }

    fn analyze1(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        let asc = self.delegate_unit(unit);
        self.delegate.analyze1(&asc, symbol_table);
        let a = asc.borrow();
        let mut u = unit.borrow_mut();
        transfer::type_info(&a, &mut u);
        transfer::namespaces(&a, &mut u);
    }

    fn analyze2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        let asc = self.delegate_unit(unit);
        transfer::dependencies(&unit.borrow(), &mut asc.borrow_mut());
        self.delegate.analyze2(&asc, symbol_table);
        transfer::dependencies(&asc.borrow(), &mut unit.borrow_mut());
    }

    fn analyze3(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        let asc = self.delegate_unit(unit);
        transfer::dependencies(&unit.borrow(), &mut asc.borrow_mut());
        self.delegate.analyze3(&asc, symbol_table);
    }

    fn analyze4(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let Some(asc) = self.delegates.get(&key(unit)).map(|d| d.0.clone()) else { return };
        self.delegate.analyze4(&asc, symbol_table);
        if toolkit::error_count() > 0 {
            return;
        }
        let a = asc.borrow();
        let mut u = unit.borrow_mut();
        transfer::expressions(&a, &mut u);
        transfer::meta_data(&a, &mut u);
        transfer::loader_class_base(&a, &mut u);
        transfer::class_table(&a, &mut u);
        transfer::styles(&a, &mut u);
    }

    fn generate(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        let asc = self.delegate_unit(unit);
        self.delegate.generate(&asc, symbol_table);
        if toolkit::error_count() > 0 {
            return;
        }
        let mut a = asc.borrow_mut();
        let mut u = unit.borrow_mut();
        transfer::generated_sources(&mut a, &mut u);
        transfer::bytecodes(&a, &mut u);
    }

    fn postprocess(&mut self, _unit: &UnitRef, _symbol_table: &mut SymbolTable) {}
}
