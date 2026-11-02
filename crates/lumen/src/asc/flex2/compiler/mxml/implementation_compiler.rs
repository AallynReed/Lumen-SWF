//! Port of `flex2.compiler.mxml.ImplementationCompiler`: the second MXML pass, which builds the `MxmlDocument`,
//! generates the full class (as an AST, or through `ClassDef.vm`/`ClassDefLib.vm`) and compiles it with ASC.

use std::rc::Rc;

use super::builder::document_builder::DocumentBuilder;
use super::dom::node::{self as dom_node, NodeRef};
use super::gen::line_number_map::DualModeLineNumberMap;
use super::gen::template_values;
use super::gen::velocity_util::{self, VelocityUtil};
use super::implementation_generator::ImplementationGenerator;
use super::interface_compiler::{CheckNodes, DOCUMENT_NODE};
use super::lang::standard_defs::StandardDefs;
use super::mxml_compiler::{self, TypeTableSlot};
use super::mxml_configuration::MxmlConfiguration;
use super::mxml_log_adapter::LineMap;
use super::reflect::TypeTable;
use super::rep::document_info::DocumentInfoRef;
use super::rep::mxml_document::{MxmlDocument, MxmlDocumentRef};
use crate::asc::flex2::compiler::as3::as3_compiler::{As3Compiler, SubCompiler};
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::compiler::source::{self, Source, SourceRef, VirtualFile};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::transcoder::Transcoders;
use crate::asc::flex2::compiler::util::mime_mappings;
use crate::asc::flex2::compiler::util::name_mappings::NameMappings;
use crate::asc::flex2::velocity::{self, Value};

const DOC_KEY: &str = "doc";
const COMMENTS_KEY: &str = "processcomment";
const CLASSDEF_TEMPLATE_PATH: &str = "flex2/compiler/mxml/gen/";

pub struct ImplementationCompiler {
    configuration: Rc<Configuration>,
    mxml_configuration: Rc<MxmlConfiguration>,
    name_mappings: Rc<NameMappings>,
    type_table: TypeTableSlot,
    asc: As3Compiler,
    process_comments: bool,
}

impl ImplementationCompiler {
    pub fn new(mxml_configuration: Rc<MxmlConfiguration>, configuration: Rc<Configuration>, mappings: Rc<NameMappings>, transcoders: Transcoders, type_table: TypeTableSlot, process_comments: bool) -> ImplementationCompiler {
        let mut asc = As3Compiler::new(configuration.clone());
        let gendir = if mxml_configuration.keep_generated_action_script() { mxml_configuration.get_generated_directory().map(str::to_string) } else { None };
        let generate_ast = mxml_configuration.get_generate_abstract_syntax_tree();
        asc.add_compiler_extension(Box::new(crate::asc::flex2::compiler::as3::embed_extension::EmbedExtension::new(transcoders, gendir.clone(), mxml_configuration.show_deprecation_warnings())));
        asc.add_compiler_extension(Box::new(crate::asc::flex2::compiler::as3::style_evaluator::StyleExtension));
        asc.add_compiler_extension(Box::new(crate::asc::flex2::compiler::as3::host_component_extension::HostComponentExtension::new(mxml_configuration.report_missing_required_skin_parts_as_warnings())));
        asc.add_compiler_extension(Box::new(crate::asc::flex2::compiler::as3::skin_part_extension::SkinPartExtension));
        if !process_comments {
            asc.add_compiler_extension(Box::new(crate::asc::flex2::compiler::as3::binding::bindable_extension::BindableExtension::new(gendir.clone(), generate_ast, false)));
        }
        asc.add_compiler_extension(Box::new(crate::asc::flex2::compiler::as3::binding::data_binding_extension::DataBindingExtension::new(gendir.clone(), mxml_configuration.show_binding_warnings(), generate_ast, configuration.defines.clone())));
        asc.add_compiler_extension(Box::new(crate::asc::flex2::compiler::as3::managed::managed_extension_error::ManagedExtensionError));
        ImplementationCompiler { configuration, mxml_configuration, name_mappings: mappings, type_table, asc, process_comments }
    }

    pub fn get_as_compiler(&mut self) -> &mut As3Compiler {
        &mut self.asc
    }

    pub fn preprocess(&mut self, source: SourceRef) -> Option<SourceRef> {
        Some(source)
    }

    fn type_table(&self, symbol_table: &SymbolTable) -> Rc<TypeTable> {
        let existing = self.type_table.borrow().clone();
        match existing {
            Some(t) => t,
            None => {
                let theme_names = self.mxml_configuration.get_theme_names().clone();
                let t = TypeTable::new(symbol_table, self.name_mappings.clone(), StandardDefs::get_standard_defs("halo"), Some(theme_names));
                *self.type_table.borrow_mut() = Some(t.clone());
                t
            }
        }
    }

    /// `hasUnresolvedNodes(unit)`.
    fn has_unresolved_nodes(unit: &UnitRef) -> bool {
        let check_nodes = unit.borrow_mut().context.remove_attribute(mxml_compiler::CHECK_NODES).and_then(|a| a.downcast::<CheckNodes>().ok());
        if let Some(check_nodes) = check_nodes {
            let name = unit.borrow().get_source().borrow().get_name_for_reporting().to_string();
            for node in check_nodes.borrow().iter() {
                let image = node.borrow().image.clone();
                toolkit::log_for(CompilerMessage::error(format!("Could not resolve {image} to a component implementation.")), &name);
            }
        }
        toolkit::error_count() > 0
    }

    /// `generateImplementation(doc)`.
    fn generate_implementation(&self, doc: &MxmlDocumentRef) -> Option<VirtualFile> {
        let standard_defs = doc.get_standard_defs();
        let class_def_template = format!("{CLASSDEF_TEMPLATE_PATH}{}", standard_defs.get_class_def_template());
        let class_def_lib_template = format!("{CLASSDEF_TEMPLATE_PATH}{}", standard_defs.get_class_def_lib_template());
        let template = match velocity::get_template_with_lib(&class_def_template, Some(&class_def_lib_template)) {
            Ok(t) => t,
            Err(_) => {
                toolkit::log(CompilerMessage::error(format!("unable to load template '{class_def_template}'")), None, -1, -1, None);
                return None;
            }
        };
        let gen_file_name = mxml_compiler::get_generated_name(&self.mxml_configuration, &doc.get_package_name(), &doc.get_class_name(), "-generated.as");
        let source = doc.get_compilation_unit().borrow().get_source();
        let line_map = Rc::new(DualModeLineNumberMap::new(Some(source.borrow().get_name_for_reporting()), Some(&gen_file_name)));
        doc.set_line_number_map(Some(line_map.clone()));
        let util = VelocityUtil::new(CLASSDEF_TEMPLATE_PATH, self.mxml_configuration.debug(), Some(line_map));
        let (vc, util) = velocity_util::get_code_gen_context(util, None);
        vc.put(DOC_KEY, template_values::document(doc));
        vc.put(COMMENTS_KEY, Value::Boolean(self.process_comments));
        let code = match velocity_util::merge(&template, &vc, &util) {
            Ok(c) => c,
            Err(e) => {
                toolkit::log(CompilerMessage::error(format!("Exception while generating from {}: {}", doc.get_source_path(), e)), None, -1, -1, None);
                return None;
            }
        };
        if self.mxml_configuration.keep_generated_action_script() {
            if let Some(parent) = std::path::Path::new(&gen_file_name).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&gen_file_name, &code) {
                toolkit::log(CompilerMessage::error(format!("unable to write generated file '{gen_file_name}': {e}")), None, -1, -1, None);
            }
        }
        let (parent, last_modified) = {
            let s = source.borrow();
            (s.get_parent(), s.get_last_modified())
        };
        Some(VirtualFile::text(code, &gen_file_name, None, parent, mime_mappings::AS, last_modified))
    }

    /// `generateImplementationAST(mxmlDocument, contextStatics, bytecodeEmitter)`.
    fn generate_implementation_ast(&self, mxml_document: &MxmlDocumentRef, symbol_table: &SymbolTable) -> SourceRef {
        let gen_file_name = mxml_compiler::get_generated_name(&self.mxml_configuration, &mxml_document.get_package_name(), &mxml_document.get_class_name(), "-generated.as");
        let source = mxml_document.get_compilation_unit().borrow().get_source();
        let (parent, last_modified) = {
            let s = source.borrow();
            (s.get_parent(), s.get_last_modified())
        };
        let empty_file = VirtualFile::text(String::new(), &gen_file_name, None, parent, mime_mappings::AS, last_modified);
        let result = Source::new_from(empty_file, &source);
        let implementation_generator = ImplementationGenerator::new(mxml_document, false, &symbol_table.per_compile_data, &result, &symbol_table.emitter, &self.configuration.defines, self.process_comments);
        let syntax_tree = implementation_generator.get_syntax_tree();
        let unit = Source::new_compilation_unit(&result, syntax_tree);
        unit.borrow_mut().context.set_asc_context(Some(implementation_generator.get_context()));
        result
    }

    /// `setLogAdapter(unit)`.
    fn set_log_adapter(&self, unit: &UnitRef) -> mxml_compiler::LogAdapterGuard {
        if self.mxml_configuration.get_generate_abstract_syntax_tree() {
            return super::abstract_syntax_tree_log_adapter::set_ast_log_adapter();
        }
        mxml_compiler::set_mxml_log_adapter(mxml_compiler::line_number_map(unit), None, None)
    }

    fn delegate(unit: &UnitRef) -> UnitRef {
        unit.borrow().context.get_attribute_as::<UnitRef>(mxml_compiler::DELEGATE_UNIT).map(|u| (*u).clone()).expect("DelegateUnit")
    }
}

impl SubCompiler for ImplementationCompiler {
    fn get_name(&self) -> &'static str {
        "mxml"
    }

    fn is_supported(&self, mime_type: &str) -> bool {
        mime_type == mime_mappings::MXML
    }

    fn preprocess(&mut self, source: SourceRef) -> Option<SourceRef> {
        ImplementationCompiler::preprocess(self, source)
    }

    fn parse1(&mut self, source: &SourceRef, symbol_table: &mut SymbolTable) -> Option<UnitRef> {
        let unit = source.borrow().get_compilation_unit()?;
        let type_table = self.type_table(symbol_table);
        if Self::has_unresolved_nodes(&unit) {
            return None;
        }
        let app = unit.borrow().context.get_attribute_as::<NodeRef>(DOCUMENT_NODE).map(|n| (*n).clone()).expect("DocumentNode");
        let info = unit.borrow_mut().context.remove_attribute(mxml_compiler::DOCUMENT_INFO).and_then(|a| a.downcast::<DocumentInfoRef>().ok()).map(|d| (*d).clone()).expect("DocumentInfo");
        let document = MxmlDocument::new(&unit, &type_table, &info, &self.mxml_configuration);
        {
            let mut builder = DocumentBuilder::new(unit.clone(), type_table.clone(), self.mxml_configuration.clone(), document.clone());
            dom_node::analyze(&app, &mut builder);
        }
        if toolkit::error_count() > 0 {
            return None;
        }
        if self.mxml_configuration.get_generate_abstract_syntax_tree() {
            let binding_expressions = document.get_binding_expressions();
            let guard = super::abstract_syntax_tree_log_adapter::set_ast_log_adapter();
            let gen_source = self.generate_implementation_ast(&document, symbol_table);
            document.get_styles_container().set_mxml_document(None);
            drop(document);
            unit.borrow_mut().context.remove_attribute(DOCUMENT_NODE);
            let asc_unit = self.asc.parse1(&gen_source, symbol_table);
            if toolkit::error_count() > 0 {
                // Java returns here with the adapter still installed.
                std::mem::forget(guard);
                return None;
            }
            let asc_unit = asc_unit?;
            drop(guard);
            unit.borrow_mut().context.set_attribute(mxml_compiler::DELEGATE_SOURCE, Rc::new(gen_source.clone()));
            asc_unit.borrow_mut().context.set_attribute(crate::asc::flex2::compiler::as3::binding::data_binding_extension::BINDING_EXPRESSIONS, Rc::new(std::cell::RefCell::new(binding_expressions)));
            unit.borrow_mut().context.set_attribute(mxml_compiler::DELEGATE_UNIT, Rc::new(asc_unit.clone()));
            {
                let asrc = asc_unit.borrow().get_source();
                unit.borrow().get_source().borrow_mut().add_file_includes(&asrc.borrow());
            }
            source::transfer_meta_data(&asc_unit.borrow(), &mut unit.borrow_mut());
            source::transfer_generated_sources(&mut asc_unit.borrow_mut(), &mut unit.borrow_mut());
            source::transfer_definitions(&asc_unit.borrow(), &mut unit.borrow_mut());
            source::transfer_inheritance(&asc_unit.borrow(), &mut unit.borrow_mut());
            source::transfer_expressions(&asc_unit.borrow(), &mut unit.borrow_mut());
            return Some(unit);
        }
        let gen_file = self.generate_implementation(&document);
        let binding_expressions = document.get_binding_expressions();
        let line_map = document.get_line_number_map();
        let gen_source = match gen_file {
            Some(f) if toolkit::error_count() == 0 => {
                let s = Source::new_from(f, source);
                s.borrow_mut().add_file_includes(&source.borrow());
                s
            }
            _ => return None,
        };
        let guard = mxml_compiler::set_mxml_log_adapter(line_map.clone().map(LineMap::Dual), None, None);
        document.get_styles_container().set_mxml_document(None);
        drop(document);
        unit.borrow_mut().context.remove_attribute(DOCUMENT_NODE);
        let asc_unit = self.asc.parse1(&gen_source, symbol_table);
        if toolkit::error_count() > 0 {
            return None;
        }
        let asc_unit = asc_unit?;
        if let Some(m) = line_map {
            unit.borrow_mut().context.set_attribute(mxml_compiler::LINE_NUMBER_MAP, Rc::new(LineMap::Dual(m.clone())));
            asc_unit.borrow_mut().context.set_attribute(mxml_compiler::LINE_NUMBER_MAP, Rc::new(LineMap::Dual(m)));
        }
        drop(guard);
        unit.borrow_mut().context.set_attribute(mxml_compiler::DELEGATE_SOURCE, Rc::new(gen_source.clone()));
        asc_unit.borrow_mut().context.set_attribute(crate::asc::flex2::compiler::as3::binding::data_binding_extension::BINDING_EXPRESSIONS, Rc::new(std::cell::RefCell::new(binding_expressions)));
        unit.borrow_mut().context.set_attribute(mxml_compiler::DELEGATE_UNIT, Rc::new(asc_unit.clone()));
        {
            let asrc = asc_unit.borrow().get_source();
            unit.borrow().get_source().borrow_mut().add_file_includes(&asrc.borrow());
        }
        source::transfer_meta_data(&asc_unit.borrow(), &mut unit.borrow_mut());
        source::transfer_generated_sources(&mut asc_unit.borrow_mut(), &mut unit.borrow_mut());
        source::transfer_definitions(&asc_unit.borrow(), &mut unit.borrow_mut());
        source::transfer_inheritance(&asc_unit.borrow(), &mut unit.borrow_mut());
        source::transfer_expressions(&asc_unit.borrow(), &mut unit.borrow_mut());
        Some(unit)
    }

    fn parse2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let asc_unit = Self::delegate(unit);
        source::transfer_inheritance(&unit.borrow(), &mut asc_unit.borrow_mut());
        let guard = self.set_log_adapter(unit);
        self.asc.parse2(&asc_unit, symbol_table);
        drop(guard);
        source::transfer_assets(&asc_unit.borrow(), &mut unit.borrow_mut());
        source::transfer_generated_sources(&mut asc_unit.borrow_mut(), &mut unit.borrow_mut());
    }

    fn analyze1(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let asc_unit = Self::delegate(unit);
        let guard = self.set_log_adapter(unit);
        self.asc.analyze1(&asc_unit, symbol_table);
        drop(guard);
        source::transfer_type_info(&asc_unit.borrow(), &mut unit.borrow_mut());
        source::transfer_namespaces(&asc_unit.borrow(), &mut unit.borrow_mut());
    }

    fn analyze2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let asc_unit = Self::delegate(unit);
        source::transfer_dependencies(&unit.borrow(), &mut asc_unit.borrow_mut());
        let guard = self.set_log_adapter(unit);
        self.asc.analyze2(&asc_unit, symbol_table);
        drop(guard);
        source::transfer_dependencies(&asc_unit.borrow(), &mut unit.borrow_mut());
    }

    fn analyze3(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let asc_unit = Self::delegate(unit);
        source::transfer_dependencies(&unit.borrow(), &mut asc_unit.borrow_mut());
        let guard = self.set_log_adapter(unit);
        self.asc.analyze3(&asc_unit, symbol_table);
        drop(guard);
    }

    fn analyze4(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let asc_unit = Self::delegate(unit);
        let guard = if self.mxml_configuration.get_generate_abstract_syntax_tree() {
            super::abstract_syntax_tree_log_adapter::set_ast_log_adapter()
        } else {
            let renamed = asc_unit.borrow().context.get_attribute_as::<crate::asc::flex2::compiler::as3::genext::generative_second_pass_evaluator::RenamedVariableMap>(crate::asc::flex2::compiler::as3::genext::generative_second_pass_evaluator::RENAMED_VARIABLE_MAP).map(|m| m.borrow().clone());
            mxml_compiler::set_mxml_log_adapter(mxml_compiler::line_number_map(unit), None, renamed)
        };
        self.asc.analyze4(&asc_unit, symbol_table);
        drop(guard);
        if toolkit::error_count() > 0 {
            return;
        }
        source::transfer_expressions(&asc_unit.borrow(), &mut unit.borrow_mut());
        source::transfer_meta_data(&asc_unit.borrow(), &mut unit.borrow_mut());
        source::transfer_loader_class_base(&asc_unit.borrow(), &mut unit.borrow_mut());
        source::transfer_class_table(&asc_unit.borrow(), &mut unit.borrow_mut());
        source::transfer_styles(&asc_unit.borrow(), &mut unit.borrow_mut());
    }

    fn generate(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let asc_unit = Self::delegate(unit);
        let guard = if self.mxml_configuration.get_generate_abstract_syntax_tree() {
            super::abstract_syntax_tree_log_adapter::set_ast_log_adapter()
        } else {
            let map = mxml_compiler::line_number_map(unit);
            if let Some(LineMap::Dual(m)) = &map {
                m.flush_temp();
            }
            mxml_compiler::set_mxml_log_adapter(map, None, None)
        };
        self.asc.generate(&asc_unit, symbol_table);
        drop(guard);
        if toolkit::error_count() > 0 {
            return;
        }
        source::transfer_generated_sources(&mut asc_unit.borrow_mut(), &mut unit.borrow_mut());
        source::transfer_bytecodes(&asc_unit.borrow(), &mut unit.borrow_mut());
    }

    fn postprocess(&mut self, _unit: &UnitRef, _symbol_table: &mut SymbolTable) {}
}
