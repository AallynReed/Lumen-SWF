//! Port of `flex2.compiler.mxml.InterfaceCompiler`: the first MXML pass, which parses the document, generates the
//! class skeleton (as an AST, or through `InterfaceDef.vm`), runs ASC over it and collects the component types to
//! load.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use super::attribute::Attribute;
use super::dom::analyzer::Analyzer;
use super::dom::analyzer_adapter::AnalyzerAdapter;
use super::dom::mxml_scanner::{self, MxmlScanner, ScannerFailure};
use super::dom::node::{self as dom_node, Node, NodeClass, NodeRef, CLASS_NAME_ATTR, DEFINITION_NAME_ATTR};
use super::dom::scanner_error::ScannerError;
use super::dom::syntax_tree_builder::SyntaxTreeBuilder;
use super::gen::line_number_map::LineNumberMap;
use super::lang::attribute_handler::{AttributeHandler, AttributeHandlerState};
use super::lang::child_node_handler::{ChildNodeHandler, ChildNodeHandlerState};
use super::lang::declaration_handler::DeclarationHandler;
use super::lang::framework_defs;
use super::lang::standard_defs::{self, StandardDefs};
use super::lang::text_parser::{self, TextParser};
use super::mxml_compiler::{self, TypeTableSlot};
use super::mxml_configuration::MxmlConfiguration;
use super::mxml_log_adapter::LineMap;
use super::mxml_namespaces::{FXG_2008_NAMESPACE, MXML_1_NAMESPACE, MXML_2009_NAMESPACE, MXML_2_NAMESPACE};
use super::parser::{Parser, ParserError, LINE_SEPARATOR};
use super::reflect::{EffectRef, EventRef, PropertyRef, StyleRef, TypeRef, TypeTable};
use super::rep::binding_expression::BindingExpressionRef;
use super::rep::document_info::{DocumentInfo, DocumentInfoRef};
use super::rep::model::RValue;
use super::rep::script::Script;
use crate::asc::flex2::compiler::as3::as3_compiler::{As3Compiler, SubCompiler};
use crate::asc::flex2::compiler::as3::syntax_tree_evaluator;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::compiler::source::{self, Source, SourceRef, VirtualFile};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage, Level};
use crate::asc::flex2::compiler::util::name_mappings::NameMappings;
use crate::asc::flex2::compiler::util::{mime_mappings, name_formatter, MultiName, Name, QName};
use crate::asc::flex2::velocity::{self, JavaException, JavaObject, Method, Value};
use crate::asc::util::jhash::JHashSet;

const TEMPLATE_PATH: &str = "flex2/compiler/mxml/gen/";
const ATTR_TYPE_REQUESTS: &str = "TypeRequests";
const ATTR_INLINE_COMPONENT_SYNTAX_TREE: &str = "InlineComponentSyntaxTree";
pub const DOCUMENT_NODE: &str = "DocumentNode";
const PRIVATE_AST_COMMENT: &str = "<description><![CDATA[]]></description><private><![CDATA[]]></private>";

/// The nodes `postprocess` revisits (`CHECK_NODES`), in insertion order.
pub type CheckNodes = RefCell<Vec<NodeRef>>;
/// `AttrTypeRequests`.
pub type TypeRequests = RefCell<JHashSet<MultiName>>;

fn msg_error(text: String) -> CompilerMessage {
    CompilerMessage::error(text)
}

pub struct InterfaceCompiler {
    mxml_configuration: Rc<MxmlConfiguration>,
    defines: Vec<crate::asc::util::context::ConfigVar>,
    name_mappings: Rc<NameMappings>,
    type_table: TypeTableSlot,
    asc: As3Compiler,
    process_comments: bool,
}

impl InterfaceCompiler {
    pub fn new(mxml_configuration: Rc<MxmlConfiguration>, configuration: Rc<Configuration>, mappings: Rc<NameMappings>, type_table: TypeTableSlot, process_comments: bool) -> InterfaceCompiler {
        let mut c = (*configuration).clone();
        c.optimize = true;
        c.verbose_stacktraces = false;
        c.debug = false;
        c.warnings = false;
        c.doc = false;
        c.show_deprecation_warnings = false;
        c.keep_embed_metadata = false;
        let mut asc = As3Compiler::new(Rc::new(c));
        asc.add_compiler_extension(Box::new(crate::asc::flex2::compiler::as3::host_component_extension::HostComponentExtension::new(mxml_configuration.report_missing_required_skin_parts_as_warnings())));
        InterfaceCompiler { mxml_configuration, defines: configuration.defines.clone(), name_mappings: mappings, type_table, asc, process_comments }
    }

    pub fn get_as_compiler(&mut self) -> &mut As3Compiler {
        &mut self.asc
    }

    pub fn preprocess(&mut self, source: SourceRef) -> Option<SourceRef> {
        let component_name = source.borrow().get_short_name().to_string();
        if !text_parser::is_valid_identifier(&component_name) {
            let path = source.borrow().get_name_for_reporting().to_string();
            toolkit::log(msg_error(format!("Invalid component name '{component_name}': component name must be legal ActionScript class name.")), Some(&path), -1, -1, None);
        }
        Some(source)
    }

    fn type_table(&self, symbol_table: &SymbolTable) -> Rc<TypeTable> {
        let existing = self.type_table.borrow().clone();
        match existing {
            Some(t) => t,
            None => {
                let theme_names: HashSet<String> = self.mxml_configuration.get_theme_names().clone();
                let t = TypeTable::new(symbol_table, self.name_mappings.clone(), StandardDefs::get_standard_defs("halo"), Some(theme_names));
                *self.type_table.borrow_mut() = Some(t.clone());
                t
            }
        }
    }

    /// `parseMXML(source)`.
    pub fn parse_mxml(&self, source: &SourceRef) -> Option<NodeRef> {
        let (name, bytes) = {
            let s = source.borrow();
            (s.get_name_for_reporting().to_string(), s.to_byte_array())
        };
        let bytes = match bytes {
            Ok(b) => b,
            Err(e) => {
                toolkit::log_path(Level::Error, &name, &e.to_string(), -1);
                return None;
            }
        };
        let mut s = match MxmlScanner::with_comments(bytes, self.mxml_configuration.enable_runtime_design_layers(), self.process_comments) {
            Ok(s) => s,
            Err(ScannerFailure::Scanner(err)) => {
                scanner_error(&name, err);
                return None;
            }
            Err(ScannerFailure::Runtime(m)) => {
                toolkit::log_path(Level::Error, &name, &m.unwrap_or_else(|| "null".into()), -1);
                return None;
            }
        };
        let result = {
            let mut p = Parser::new(&mut s);
            p.set_visitor(Box::new(SyntaxTreeBuilder));
            p.parse_application()
        };
        match result {
            Ok(t) => {
                let app = t.as_node();
                if !check_mxml_namespace(&name, &app) {
                    return None;
                }
                Some(app)
            }
            Err(ParserError::Parse(ex)) => {
                let token = ex.next.clone();
                let mut msg = ex.get_message().to_string();
                if let Some(was) = msg.find(&format!("{LINE_SEPARATOR}Was expecting")).filter(|&w| w > 0) {
                    msg.truncate(was);
                    if regex::Regex::new("^Encountered \"(?s:.*)\" at line.*$").unwrap().is_match(&msg) {
                        msg = format!("Parse error: '{}' is not allowed to follow '{}'.", ex.next.image(), ex.current_token.image());
                    }
                }
                log_error_col(&name, token.begin_line(), token.begin_column(), &msg);
                None
            }
            Err(ParserError::Scanner(err)) => {
                scanner_error(&name, err);
                None
            }
            Err(ParserError::Runtime(m)) => {
                toolkit::log_path(Level::Error, &name, &m, -1);
                None
            }
        }
    }

    /// `createDocumentInfo(unit, app, source)`.
    fn create_document_info(&self, app: &NodeRef, source: &SourceRef) -> Option<DocumentInfoRef> {
        let standard_defs = StandardDefs::get_standard_defs("halo");
        let (name, short_name, rel) = {
            let s = source.borrow();
            (s.get_name_for_reporting().to_string(), s.get_short_name().to_string(), s.get_relative_path().replace('/', "."))
        };
        let info = DocumentInfo::new(&name, standard_defs.clone());
        let (begin_line, ns, local, image) = {
            let a = app.borrow();
            (a.begin_line, a.get_namespace().to_string(), a.get_local_part().to_string(), a.image.clone())
        };
        info.set_root_node(app.clone(), begin_line);
        info.set_class_name(&short_name);
        info.set_package_name(&rel);
        let mut super_class_name = info.get_local_class(&ns, &local);
        if super_class_name.is_none() {
            super_class_name = self.name_mappings.resolve_class_name(&ns, &local);
        }
        match super_class_name {
            Some(sc) => info.set_qualified_super_class_name(&name_formatter::to_dot(&sc), begin_line),
            None => {
                toolkit::log(msg_error(format!("Could not resolve {image} to a component implementation.")), Some(&name), begin_line, -1, None);
                return None;
            }
        }
        let (interface_names, implements_line) = {
            let a = app.borrow();
            (a.get_attribute_str("implements").map(str::to_string), a.get_line_number("implements"))
        };
        if let Some(interface_names) = interface_names {
            for t in interface_names.split(',').filter(|t| !t.is_empty()) {
                info.add_interface_name(&java_trim(t), implements_line);
            }
        }
        if self.mxml_configuration.get_generate_abstract_syntax_tree() {
            let split = |names: Vec<String>| -> Vec<(String, Vec<String>)> { names.into_iter().map(|n| { let parts = n.split('.').map(str::to_string).collect(); (n, parts) }).collect() };
            info.add_split_import_names(split(standard_defs::implicit_imports().iter().cloned().collect()));
            if info.get_version() >= 4 {
                info.remove_split_import_name(&name_formatter::to_dot_star(standard_defs::PACKAGE_FLASH_FILTERS));
                let mx_filters = name_formatter::to_dot_star(standard_defs::PACKAGE_MX_FILTERS);
                let parts = mx_filters.split('.').map(str::to_string).collect();
                info.add_split_import_name(&mx_filters, parts);
            }
            if self.mxml_configuration.get_compiling_for_air() {
                info.add_split_import_names(split(standard_defs::air_only_implicit_imports().iter().cloned().collect()));
            }
            info.add_split_import_names(standard_defs.get_split_standard_mxml_imports().iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        } else {
            let implicit: Vec<String> = standard_defs::implicit_imports().iter().cloned().collect();
            info.add_import_names(&implicit, begin_line);
            if info.get_version() >= 4 {
                info.remove_import_name(&name_formatter::to_dot_star(standard_defs::PACKAGE_FLASH_FILTERS));
                info.add_import_name(&name_formatter::to_dot_star(standard_defs::PACKAGE_MX_FILTERS), begin_line);
            }
            if self.mxml_configuration.get_compiling_for_air() {
                let air: Vec<String> = standard_defs::air_only_implicit_imports().iter().cloned().collect();
                info.add_import_names(&air, begin_line);
            }
            let std_imports: Vec<String> = standard_defs.get_standard_mxml_imports().iter().cloned().collect();
            info.add_import_names(&std_imports, begin_line);
        }
        Some(info)
    }

    /// `generateSkeleton(info, bogusImports, map, source)`.
    fn generate_skeleton(&self, info: &DocumentInfoRef, bogus_imports: &[String], map: &Rc<RefCell<LineNumberMap>>, source: &SourceRef) -> Option<SourceRef> {
        let path = source.borrow().get_name().to_string();
        let template_name = format!("{TEMPLATE_PATH}{}", info.get_standard_defs().get_interface_def_template());
        let template = match velocity::get_template(&template_name) {
            Ok(t) => t,
            Err(e) => {
                toolkit::log_path(Level::Error, &path, &e.to_string(), -1);
                return None;
            }
        };
        let codes = SourceCodes::default();
        let cx = velocity::get_code_gen_context(None);
        let mut script_set = Vec::new();
        for script in info.get_scripts() {
            if !script.is_embedded() {
                script_set.push(codes.add(script.get_text(), script.get_xml_line_number()));
            } else {
                let (src_name, parent, last_modified, reporting) = {
                    let s = source.borrow();
                    (s.get_name().replace('\\', "/"), s.get_parent(), s.get_last_modified(), s.get_name_for_reporting().to_string())
                };
                let n = format!("{src_name}:{},{}", script.get_xml_line_number(), script.get_end_xml_line_number());
                let f = VirtualFile::text(script.get_text().to_string(), &n, None, parent, mime_mappings::AS, last_modified);
                let mut m = LineNumberMap::new(Some(&reporting), Some(&n));
                m.put(script.get_xml_line_number(), 1, script.get_end_xml_line_number() - script.get_xml_line_number());
                m.put(script.get_end_xml_line_number(), script.get_end_xml_line_number() - script.get_xml_line_number() + 1, 1);
                source.borrow_mut().add_source_fragment(&n, Rc::new(f), Some(Rc::new(RefCell::new(m))));
                script_set.push(codes.add(&format!("include \"{n}\";"), script.get_xml_line_number()));
            }
        }
        let mut metadata_set = Vec::new();
        for script in info.get_metadata() {
            metadata_set.push(codes.add(script.get_text(), script.get_xml_line_number()));
        }
        let mut var_decl_entries = Vec::new();
        for var_decl in info.get_var_decls() {
            var_decl_entries.push(Value::Entry(Rc::new(velocity::introspection::JEntry { key: Value::str(&var_decl.name), value: codes.add(&var_decl.class_name, var_decl.line) })));
        }
        let mut super_class_line_number = 1;
        let mut import_name_set = Vec::new();
        for import_name in info.get_import_names().iter() {
            import_name_set.push(codes.add(import_name.get_name(), import_name.get_line()));
            if import_name.get_name() == info.get_qualified_super_class_name() {
                super_class_line_number = import_name.get_line();
            }
        }
        for import_name in bogus_imports {
            import_name_set.push(codes.add(import_name, 1));
        }
        let mut interface_name_set = Vec::new();
        for interface_name in info.get_interface_names().iter() {
            interface_name_set.push(codes.add(interface_name.get_name(), interface_name.get_line()));
        }
        let binding_vars: Vec<Value> = framework_defs::binding_management_vars().iter().map(|v| v.value()).collect();
        cx.put("imports", Value::list_of("java.util.LinkedHashSet", import_name_set));
        cx.put("variables", Value::list_of("java.util.LinkedHashMap$EntrySet", var_decl_entries));
        cx.put("scripts", Value::list_of("java.util.LinkedHashSet", script_set));
        cx.put("classMetaData", Value::list_of("java.util.LinkedHashSet", metadata_set));
        cx.put("bindingManagementVariables", Value::list(binding_vars));
        cx.put("superClassName", codes.add(&info.get_qualified_super_class_name(), super_class_line_number));
        cx.put("interfaceNames", Value::list_of("java.util.LinkedHashSet", interface_name_set));
        cx.put("className", Value::str(&info.get_class_name()));
        cx.put("packageName", Value::str(&info.get_package_name()));
        let out = match template.merge(&cx) {
            Ok(o) => o,
            Err(e) => {
                toolkit::log_path(Level::Error, &path, &e.to_string(), -1);
                return None;
            }
        };
        let generated = codes.resolve(&out, map).replace("\r\n", "\n");
        let filename = self.write_generated(info, &generated);
        let (parent, last_modified) = {
            let s = source.borrow();
            (s.get_parent(), s.get_last_modified())
        };
        let text_file = VirtualFile::text(generated, &filename, None, parent, mime_mappings::AS, last_modified);
        Some(Source::new_from(text_file, source))
    }

    /// `generateSkeletonAST(info, bogusImports, source, symbolTable)`.
    fn generate_skeleton_ast(&self, info: &DocumentInfoRef, bogus_imports: &[String], source: &SourceRef, symbol_table: &SymbolTable) -> SourceRef {
        let file_name = mxml_compiler::get_generated_name(&self.mxml_configuration, &info.get_package_name(), &info.get_class_name(), "-interface.as");
        let (name, parent, last_modified) = {
            let s = source.borrow();
            (s.get_name().to_string(), s.get_parent(), s.get_last_modified())
        };
        let text_file = VirtualFile::text(String::new(), &file_name, Some(&name), parent, mime_mappings::MXML, last_modified);
        let result = Source::new_from(text_file, source);
        let interface_generator = super::interface_generator::InterfaceGenerator::new(info, bogus_imports, &symbol_table.per_compile_data, &result, &symbol_table.emitter, &self.defines);
        let syntax_tree = interface_generator.get_syntax_tree();
        let unit = Source::new_compilation_unit(&result, syntax_tree);
        unit.borrow_mut().context.set_asc_context(Some(interface_generator.get_context()));
        result
    }

    fn write_generated(&self, info: &DocumentInfoRef, generated: &str) -> String {
        let filename = mxml_compiler::get_generated_name(&self.mxml_configuration, &info.get_package_name(), &info.get_class_name(), "-interface.as");
        if self.mxml_configuration.keep_generated_action_script() {
            if let Some(parent) = std::path::Path::new(&filename).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&filename, generated);
        }
        filename
    }

    /// `compileInterface(newSource, origSource, info, map, symbolTable)`.
    fn compile_interface(&mut self, new_source: &SourceRef, info: &DocumentInfoRef, map: &Rc<RefCell<LineNumberMap>>, symbol_table: &mut SymbolTable) -> Option<UnitRef> {
        let interface_unit = self.asc.parse1(new_source, symbol_table);
        if let Some(u) = &interface_unit {
            syntax_tree_evaluator::remove_non_api_content(u);
            strip_redeclared_management_vars(u, &info.get_qname().to_string(), symbol_table);
            let md = info.get_metadata();
            if !self.mxml_configuration.get_generate_abstract_syntax_tree() && !md.is_empty() {
                let begin_lines: Vec<i32> = md.iter().map(|s| s.get_xml_line_number()).collect();
                let end_lines: Vec<i32> = md.iter().map(|s| s.get_end_xml_line_number()).collect();
                let m = map.clone();
                crate::asc::flex2::compiler::as3::reflect::node_magic::meta_data_only(u, &move |l| m.borrow().get(l), &begin_lines, &end_lines);
            }
            syntax_tree_evaluator::ensure_meta_data_has_definition(u);
        }
        interface_unit
    }

    /// `setLogAdapter(unit)`.
    fn set_log_adapter(&self, unit: &UnitRef) -> mxml_compiler::LogAdapterGuard {
        if self.mxml_configuration.get_generate_abstract_syntax_tree() {
            return super::abstract_syntax_tree_log_adapter::set_ast_log_adapter();
        }
        let source = unit.borrow().get_source();
        mxml_compiler::set_mxml_log_adapter(mxml_compiler::line_number_map(unit), mxml_compiler::fragment_line_maps(&source), None)
    }

    fn delegate(unit: &UnitRef) -> UnitRef {
        unit.borrow().context.get_attribute_as::<UnitRef>(mxml_compiler::DELEGATE_UNIT).map(|u| (*u).clone()).expect("DelegateUnit")
    }
}

/// `SyntaxTreeEvaluator.stripRedeclaredManagementVars(ascUnit, className, symbolTable)`: binding management variables
/// the base class already defines are removed again.
fn strip_redeclared_management_vars(unit: &UnitRef, class_name: &str, symbol_table: &mut SymbolTable) {
    if toolkit::error_count() > 0 {
        return;
    }
    let (program, cx) = {
        let u = unit.borrow();
        (u.get_syntax_tree(), u.context.get_asc_context())
    };
    let Some(program) = program else { return };
    let Some(class_definition) = syntax_tree_evaluator::get_first_class_definition(Some(&program)) else { return };
    let cx = cx.expect("NullPointerException");
    let type_analyzer = symbol_table.get_type_analyzer();
    type_analyzer.borrow_mut().evaluate(&cx, &program, symbol_table);
    let class_info = type_analyzer.borrow().get_class_info(class_name);
    let Some(class_info) = class_info else { return };
    let base = class_info.borrow().get_base_class_info();
    let vars = framework_defs::binding_management_vars();
    if let Some(base) = base {
        if !vars.is_empty() && base.borrow().defines_variable(vars[0].get_name()) {
            remove_variables(&class_definition, &vars);
        }
    }
}

/// `removeVariables(classDefinition, variableDeclarations)`: by name only.
fn remove_variables(class_definition: &crate::asc::parser::node::NodeRef, variable_declarations: &[super::rep::variable_declaration::VariableDeclaration]) {
    let Some(statements) = class_definition.borrow().class_definition().and_then(|c| c.statements.clone()) else { return };
    let mut st = statements.borrow_mut();
    let Some(list) = st.as_statement_list_mut() else { return };
    list.items.retain(|node| {
        let Some(node) = node else { return true };
        let n = node.borrow();
        let Some(vd) = n.as_variable_definition() else { return true };
        let Some(l) = vd.list.as_ref() else { return true };
        let l = l.borrow();
        let Some(items) = l.list().map(|l| &l.items) else { return true };
        if items.len() != 1 {
            return true;
        }
        let Some(binding) = &items[0] else { return true };
        let b = binding.borrow();
        let Some(vb) = b.as_variable_binding() else { return true };
        let name = vb.variable.as_ref().and_then(|v| v.borrow().as_typed_identifier().and_then(|t| t.identifier.clone())).and_then(|i| i.borrow().as_identifier().map(|i| i.name.clone()));
        let Some(name) = name else { return true };
        !variable_declarations.iter().any(|v| v.get_name() == name)
    });
}

fn java_trim(s: &str) -> String {
    crate::asc::flex2::flash::util::string_utils::java_trim(s)
}

/// `ThreadLocalToolkit.logError(path, line, col, msg)`.
fn log_error_col(path: &str, line: i32, col: i32, msg: &str) {
    let source = toolkit::line_text(path, line);
    toolkit::log_asc(Level::Error, path, line, col, msg, &source, -1);
}

fn scanner_error(name: &str, err: ScannerError) {
    let msg = err.get_reason().to_string();
    let (line, col) = (err.get_line_number(), err.get_column_number());
    let logged = if msg == mxml_scanner::MARKUP_NOT_RECOGNIZED_IN_CONTENT {
        Some("Invalid character or markup found. Try surrounding your code with a CDATA block.")
    } else if msg == mxml_scanner::RESERVED_PI_TARGET {
        Some("Whitespace is not allowed before an XML Processing Instruction (<? ... ?>).")
    } else if msg == mxml_scanner::MARKUP_NOT_RECOGNIZED_IN_MISC {
        Some("The markup in the document following the root element must be well-formed.")
    } else {
        None
    };
    match logged {
        Some(text) => toolkit::log(CompilerMessage::error(text), Some(name), line, col, None),
        None => {
            let re = regex::Regex::new("^The element type \"(.*)\" must be terminated by the matching end-tag \"(.*)\".$").unwrap();
            let msg = match re.captures(&msg) {
                Some(c) => format!("The element type '{}' must be terminated by the matching end-tag '{}'.", &c[1], &c[2]),
                None => msg.clone(),
            };
            log_error_col(name, line, col, &msg);
        }
    }
}

/// `checkMxmlNamespace(source, node)`.
fn check_mxml_namespace(name: &str, node: &NodeRef) -> bool {
    let ns = node.borrow().get_namespace().to_string();
    if ns == MXML_1_NAMESPACE || ns == MXML_2_NAMESPACE {
        let text = format!("unsupported MXML namespace found ('{ns}'). Please use the MXML namespace '{MXML_2009_NAMESPACE}' for MXML 2 source code.");
        toolkit::log(CompilerMessage::error(text), Some(name), -1, -1, None);
        false
    } else {
        true
    }
}

fn transfer_document_dependencies(info: &DocumentInfoRef, unit: &UnitRef) {
    let mut u = unit.borrow_mut();
    u.inheritance.insert(Name::M(name_formatter::to_multi_name(&info.get_qualified_super_class_name())));
    for name_info in info.get_interface_names().iter() {
        u.inheritance.insert(Name::M(name_formatter::to_multi_name(name_info.get_name())));
    }
}

impl SubCompiler for InterfaceCompiler {
    fn get_name(&self) -> &'static str {
        "mxml"
    }

    fn is_supported(&self, mime_type: &str) -> bool {
        mime_type == mime_mappings::MXML
    }

    fn preprocess(&mut self, source: SourceRef) -> Option<SourceRef> {
        InterfaceCompiler::preprocess(self, source)
    }

    fn parse1(&mut self, source: &SourceRef, _symbol_table: &mut SymbolTable) -> Option<UnitRef> {
        let preparsed = source.borrow().get_source_fragment(ATTR_INLINE_COMPONENT_SYNTAX_TREE);
        let (app, unit) = match preparsed.and_then(|p| p.downcast::<NodeRef>().ok()) {
            None => {
                let app = self.parse_mxml(source)?;
                let unit = Source::new_compilation_unit(source, None);
                let mut a = super::analyzer::syntax_analyzer::SyntaxAnalyzer::new(unit.clone(), self.mxml_configuration.clone());
                dom_node::analyze(&app, &mut a);
                if toolkit::error_count() > 0 {
                    return None;
                }
                (app, unit)
            }
            Some(app) => {
                let app = (*app).clone();
                let unit = Source::new_compilation_unit(source, None);
                (app, unit)
            }
        };
        unit.borrow_mut().context.set_attribute(DOCUMENT_NODE, Rc::new(app.clone()));
        let doc_info = self.create_document_info(&app, source);
        if toolkit::error_count() > 0 {
            return None;
        }
        let doc_info = doc_info?;
        unit.borrow_mut().context.set_attribute(mxml_compiler::DOCUMENT_INFO, Rc::new(doc_info.clone()));
        unit.borrow_mut().top_level_definitions.add(QName::new(&doc_info.get_package_name(), &doc_info.get_class_name()));
        transfer_document_dependencies(&doc_info, &unit);
        Some(unit)
    }

    fn parse2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let doc_info = unit.borrow().context.get_attribute_as::<DocumentInfoRef>(mxml_compiler::DOCUMENT_INFO).map(|d| (*d).clone()).expect("DocumentInfo");
        let source = unit.borrow().get_source();
        let super_class_name = doc_info.get_qualified_super_class_name();
        let base_class_info = get_class_info(&source, symbol_table, &super_class_name);
        if base_class_info.is_none() {
            let qualified_class_name = name_formatter::to_dot_ns(&doc_info.get_package_name(), &doc_info.get_class_name());
            let name = source.borrow().get_name_for_reporting().to_string();
            toolkit::log_for(msg_error(format!("Unable to locate specified base class '{super_class_name}' for component class '{qualified_class_name}'.")), &name);
            return;
        }
        let mut analyzer = InterfaceAnalyzer::new(unit.clone(), doc_info.clone(), base_class_info, self.name_mappings.clone(), self.mxml_configuration.get_generate_abstract_syntax_tree());
        let app = unit.borrow().context.get_attribute_as::<NodeRef>(DOCUMENT_NODE).map(|n| (*n).clone()).expect("DocumentNode");
        dom_node::analyze(&app, &mut analyzer);
        if toolkit::error_count() > 0 {
            return;
        }
        let map = Rc::new(RefCell::new(LineNumberMap::new(Some(source.borrow().get_name()), None)));
        let bogus: Vec<String> = analyzer.bogus_imports.iter().cloned().collect();
        let ast_mode = self.mxml_configuration.get_generate_abstract_syntax_tree();
        let (guard, new_source) = if ast_mode {
            let guard = super::abstract_syntax_tree_log_adapter::set_ast_log_adapter();
            (guard, self.generate_skeleton_ast(&doc_info, &bogus, &source, symbol_table))
        } else {
            let guard = mxml_compiler::set_mxml_log_adapter(Some(LineMap::Plain(map.clone())), mxml_compiler::fragment_line_maps(&source), None);
            let Some(new_source) = self.generate_skeleton(&doc_info, &bogus, &map, &source) else { return };
            map.borrow_mut().set_new_name(new_source.borrow().get_name());
            (guard, new_source)
        };
        let interface_unit = {
            let _inner = if ast_mode {
                super::abstract_syntax_tree_log_adapter::set_ast_log_adapter()
            } else {
                mxml_compiler::set_mxml_log_adapter(Some(LineMap::Plain(map.clone())), mxml_compiler::fragment_line_maps(&source), None)
            };
            self.compile_interface(&new_source, &doc_info, &map, symbol_table)
        };
        let Some(interface_unit) = interface_unit else { return };
        {
            let isrc = interface_unit.borrow().get_source();
            unit.borrow().get_source().borrow_mut().add_file_includes(&isrc.borrow());
        }
        unit.borrow_mut().context.set_attribute(mxml_compiler::LINE_NUMBER_MAP, Rc::new(LineMap::Plain(map)));
        unit.borrow_mut().context.set_attribute(mxml_compiler::DELEGATE_SOURCE, Rc::new(new_source.clone()));
        unit.borrow_mut().context.set_attribute(mxml_compiler::DELEGATE_UNIT, Rc::new(interface_unit.clone()));
        source::transfer_meta_data(&interface_unit.borrow(), &mut unit.borrow_mut());
        source::transfer_inheritance(&unit.borrow(), &mut interface_unit.borrow_mut());
        self.asc.parse2(&interface_unit, symbol_table);
        drop(guard);
    }

    fn analyze1(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let interface_unit = Self::delegate(unit);
        let guard = self.set_log_adapter(unit);
        self.asc.analyze1(&interface_unit, symbol_table);
        drop(guard);
        source::transfer_type_info(&interface_unit.borrow(), &mut unit.borrow_mut());
        source::transfer_namespaces(&interface_unit.borrow(), &mut unit.borrow_mut());
    }

    fn analyze2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let interface_unit = Self::delegate(unit);
        source::transfer_dependencies(&unit.borrow(), &mut interface_unit.borrow_mut());
        let guard = self.set_log_adapter(unit);
        self.asc.analyze2(&interface_unit, symbol_table);
        drop(guard);
        source::transfer_dependencies(&interface_unit.borrow(), &mut unit.borrow_mut());
    }

    fn analyze3(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let interface_unit = Self::delegate(unit);
        let import_definition_statements = interface_unit.borrow().import_definition_statements.clone();
        source::transfer_dependencies(&unit.borrow(), &mut interface_unit.borrow_mut());
        for q in import_definition_statements.iter() {
            interface_unit.borrow_mut().import_definition_statements.insert(q.clone());
        }
        let guard = self.set_log_adapter(unit);
        self.asc.analyze3(&interface_unit, symbol_table);
        drop(guard);
    }

    fn analyze4(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let interface_unit = Self::delegate(unit);
        let guard = self.set_log_adapter(unit);
        self.asc.analyze4(&interface_unit, symbol_table);
        drop(guard);
        for c in interface_unit.borrow().class_table.values() {
            c.freeze();
        }
        source::transfer_dependencies(&interface_unit.borrow(), &mut unit.borrow_mut());
        source::transfer_loader_class_base(&interface_unit.borrow(), &mut unit.borrow_mut());
        source::transfer_generated_sources(&mut interface_unit.borrow_mut(), &mut unit.borrow_mut());
        source::transfer_class_table(&interface_unit.borrow(), &mut unit.borrow_mut());
        source::transfer_styles(&interface_unit.borrow(), &mut unit.borrow_mut());
    }

    fn generate(&mut self, _unit: &UnitRef, _symbol_table: &mut SymbolTable) {}

    fn postprocess(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let type_table = self.type_table(symbol_table);
        let info = unit.borrow().context.get_attribute_as::<DocumentInfoRef>(mxml_compiler::DOCUMENT_INFO).map(|d| (*d).clone()).expect("DocumentInfo");
        let mut check_nodes = unit.borrow().context.get_attribute_as::<CheckNodes>(mxml_compiler::CHECK_NODES);
        let mut all_type_requests = unit.borrow().context.get_attribute_as::<TypeRequests>(ATTR_TYPE_REQUESTS);
        if check_nodes.is_none() {
            check_nodes = Some(Rc::new(RefCell::new(vec![info.get_root_node()])));
            let a: Rc<TypeRequests> = Rc::new(RefCell::new(JHashSet::new()));
            unit.borrow_mut().context.set_attribute(ATTR_TYPE_REQUESTS, a.clone());
            all_type_requests = Some(a);
        }
        let check_nodes = check_nodes.unwrap();
        let all_type_requests = all_type_requests.unwrap();
        if !check_nodes.borrow().is_empty() {
            let mut analyzer = DependencyAnalyzer::new(unit.clone(), type_table, info, self.name_mappings.clone(), all_type_requests.clone());
            let nodes: Vec<NodeRef> = check_nodes.borrow().clone();
            for n in &nodes {
                dom_node::analyze(n, &mut analyzer);
            }
            let new_check_nodes: Rc<CheckNodes> = Rc::new(RefCell::new(analyzer.check_nodes.clone()));
            unit.borrow_mut().context.set_attribute(mxml_compiler::CHECK_NODES, new_check_nodes);
            if !analyzer.new_type_requests.is_empty() {
                for r in analyzer.new_type_requests.iter() {
                    all_type_requests.borrow_mut().insert(r.clone());
                }
                for r in analyzer.new_type_requests.iter() {
                    unit.borrow_mut().expressions.insert(Name::M(r.clone()));
                }
            }
        }
    }
}

/// `getClassInfo(source, symbolTable, className)`: whether the base class already defines a member, as
/// `InterfaceAnalyzer.registerVariable` asks.
pub struct BaseClassInfo {
    defines: Box<dyn Fn(&str) -> bool>,
}

fn get_class_info(source: &SourceRef, symbol_table: &mut SymbolTable, class_name: &str) -> Option<BaseClassInfo> {
    let handler = Rc::new(RefCell::new(crate::asc::flex2::compiler::as3::as3_compiler::As3CompilerHandler::default()));
    let cx = crate::asc::flex2::compiler::as3::as3_compiler::new_unit_context(source, symbol_table, handler);
    let type_analyzer = symbol_table.get_type_analyzer();
    let info = type_analyzer.borrow_mut().analyze_class(&cx, &MultiName::parse(&name_formatter::to_colon(class_name)), symbol_table)?;
    Some(BaseClassInfo {
        defines: Box::new(move |id| {
            let i = info.borrow();
            i.defines_variable(id) || i.defines_getter_inherited(id, true) || i.defines_setter_inherited(id, true)
        }),
    })
}

/// `SourceCode`: a text that records where in the generated file the template writes it.
struct SourceCode {
    index: usize,
    text: String,
}

const MARK_START: char = '\u{FDD2}';
const MARK_END: char = '\u{FDD3}';

impl JavaObject for SourceCode {
    fn class_name(&self) -> &str {
        "flex2.compiler.mxml.InterfaceCompiler$SourceCode"
    }
    fn methods(&self) -> &[Method] {
        &[]
    }
    fn invoke(&self, _method: &Method, _args: &[Value]) -> Result<Value, JavaException> {
        Ok(Value::Null)
    }
    fn to_java_string(&self) -> String {
        format!("{MARK_START}{}{MARK_END}{}", self.index, self.text)
    }
}

#[derive(Default)]
struct SourceCodes {
    items: RefCell<Vec<(i32, i32)>>,
}

impl SourceCodes {
    fn add(&self, text: &str, begin_line: i32) -> Value {
        let mut items = self.items.borrow_mut();
        items.push((begin_line, crate::asc::flex2::compiler::mxml::gen::velocity_util::count_lines(text)));
        Value::object(Rc::new(SourceCode { index: items.len() - 1, text: text.to_string() }))
    }

    /// Strips the markers, mapping each item's lines at the output line it starts on.
    fn resolve(&self, out: &str, map: &Rc<RefCell<LineNumberMap>>) -> String {
        let items = self.items.borrow();
        let mut result = String::with_capacity(out.len());
        let mut current_line = 1;
        let mut rest = out;
        while let Some(i) = rest.find(MARK_START) {
            let before = &rest[..i];
            current_line += before.matches('\n').count() as i32;
            result.push_str(before);
            let after = &rest[i + MARK_START.len_utf8()..];
            let end = after.find(MARK_END).unwrap_or(after.len());
            if let Ok(n) = after[..end].parse::<usize>() {
                let (begin_line, line_count) = items[n];
                map.borrow_mut().put(begin_line, current_line, line_count + 1);
            }
            rest = &after[(end + MARK_END.len_utf8()).min(after.len())..];
        }
        result.push_str(rest);
        result
    }
}

/// `InterfaceAnalyzer`.
struct InterfaceAnalyzer {
    adapter: AnalyzerAdapter,
    doc_info: DocumentInfoRef,
    base_class_info: Option<BaseClassInfo>,
    name_mappings: Rc<NameMappings>,
    standard_defs: Rc<StandardDefs>,
    repeater_num: i32,
    inner_class_count: i32,
    inner_class_names: HashSet<String>,
    bogus_imports: JHashSet<String>,
    declared_layers: Vec<NodeRef>,
    generate_ast: bool,
}

impl InterfaceAnalyzer {
    fn new(unit: UnitRef, doc_info: DocumentInfoRef, base_class_info: Option<BaseClassInfo>, name_mappings: Rc<NameMappings>, generate_ast: bool) -> InterfaceAnalyzer {
        InterfaceAnalyzer {
            adapter: AnalyzerAdapter::new(unit),
            doc_info,
            base_class_info,
            name_mappings,
            standard_defs: StandardDefs::get_standard_defs("halo"),
            repeater_num: 0,
            inner_class_count: 0,
            inner_class_names: HashSet::new(),
            bogus_imports: JHashSet::new(),
            declared_layers: Vec::new(),
            generate_ast,
        }
    }

    fn unit(&self) -> UnitRef {
        self.adapter.unit.clone()
    }

    fn add_bogus_import(&mut self, name: String) {
        self.bogus_imports.insert(name);
    }

    fn register_variable_for_id(&mut self, node: &NodeRef, class_name: Option<String>) {
        let mut id = None;
        let mut line = 0;
        if let Some(attr) = self.get_language_attribute(node, standard_defs::PROP_ID) {
            id = attr.get_value().as_str().map(str::to_string);
            line = attr.get_line();
        }
        self.register_variable(node, id, line, class_name);
    }

    fn register_variable(&mut self, node: &NodeRef, id: Option<String>, line: i32, mut class_name: Option<String>) {
        let (ns, local, image) = {
            let n = node.borrow();
            (n.get_namespace().to_string(), n.get_local_part().to_string(), n.image.clone())
        };
        match id {
            Some(id) => {
                if !text_parser::is_valid_identifier(&id) {
                    self.adapter.log_node_line(node, line, msg_error(format!("'{id}' is not a valid identifier.")));
                } else if self.doc_info.contains_var_decl(&id) {
                    self.adapter.log_line(line, msg_error(format!("identifier '{id}' used more than once.")));
                } else if self.doc_info.get_class_name() == id {
                    self.adapter.log_line(line, msg_error(format!("'{id}': identifier and class may not have the same name.")));
                } else {
                    if class_name.is_none() {
                        class_name = self.name_mappings.resolve_class_name(&ns, &local);
                    }
                    if self.repeater_num > 0 {
                        class_name = Some("Array".to_string());
                    }
                    match class_name {
                        Some(class_name) => {
                            let defined = self.base_class_info.as_ref().is_some_and(|b| (b.defines)(&id));
                            if !defined {
                                if class_name == standard_defs::CLASS_VECTOR {
                                    let t = node.borrow().get_attribute(standard_defs::PROP_TYPE).and_then(|a| a.get_value().as_str().map(str::to_string)).expect("NullPointerException");
                                    self.doc_info.add_vector_var_decl(&id, line, &t);
                                } else {
                                    self.doc_info.add_var_decl(&id, &name_formatter::to_dot(&class_name), line);
                                }
                            }
                        }
                        None => self.adapter.log_line(line, msg_error(format!("Could not resolve {image} to a component implementation."))),
                    }
                }
            }
            None => {
                if class_name.is_none() {
                    class_name = self.name_mappings.resolve_class_name(&ns, &local);
                }
                if let Some(c) = class_name.filter(|c| !c.contains('*')) {
                    self.add_bogus_import(name_formatter::to_dot(&c));
                }
            }
        }
    }

    /// `createInlineComponentUnit(node)`.
    fn create_inline_component_unit(&mut self, node: &NodeRef) {
        let component_root = node.borrow().get_child_at(0).expect("NullPointerException");
        let class_attr = node.borrow().get_attribute(CLASS_NAME_ATTR).cloned();
        let class_name = self.get_inner_class_name(class_attr.as_ref());
        let class_qname = QName::new(&self.doc_info.get_package_name(), &class_name);
        let begin_line = node.borrow().begin_line;
        self.doc_info.add_import_name(&name_formatter::to_dot_ns(class_qname.get_namespace(), class_qname.get_local_part()), begin_line);
        node.borrow_mut().set_class_qname(class_qname.clone());
        let unit = self.unit();
        let usrc = unit.borrow().get_source();
        let inline = {
            let r = component_root.borrow();
            let mut d = Node::inline_document_node(r.get_namespace(), r.get_local_part(), &name_formatter::to_dot_ns(&self.doc_info.get_package_name(), &self.doc_info.get_class_name()));
            d.begin_line = r.begin_line;
            d.begin_column = r.begin_column;
            d.end_line = r.end_line;
            d.end_column = r.end_column;
            d.image = r.image.clone();
            d.comment = Some(if self.generate_ast { PRIVATE_AST_COMMENT } else { "@private" }.to_string());
            r.copy(&mut d);
            d
        };
        let inline = inline.into_ref();
        {
            let mut d = inline.borrow_mut();
            d.set_local_class_mappings(self.doc_info.get_local_class_mappings());
            d.set_language_namespace(self.doc_info.get_language_namespace().as_deref());
            d.set_version(self.doc_info.get_version());
        }
        add_exclude_class_node(&inline, &component_root);
        let source = new_inline_source(&usrc, &class_name);
        source.borrow_mut().add_source_fragment(ATTR_INLINE_COMPONENT_SYNTAX_TREE, Rc::new(inline), None);
        unit.borrow_mut().add_generated_source(class_qname, source);
    }

    /// `createDefinitionUnit(node)`.
    fn create_definition_unit(&mut self, node: &NodeRef) {
        let definition_root = node.borrow().get_child_at(0).expect("NullPointerException");
        let name_attr = node.borrow().get_attribute_str(DEFINITION_NAME_ATTR).map(str::to_string);
        let class_name = format!("{}_definition{}", self.doc_info.get_class_name(), self.inner_class_count);
        self.inner_class_count += 1;
        let class_qname = QName::new(&self.doc_info.get_package_name(), &class_name);
        let ns = node.borrow().get_namespace().to_string();
        self.doc_info.add_local_class(&ns, name_attr.as_deref().unwrap_or("null"), &class_qname.to_string());
        node.borrow_mut().set_name(class_qname.clone());
        let unit = self.unit();
        let usrc = unit.borrow().get_source();
        let def = {
            let r = definition_root.borrow();
            let mut d = Node::new_document(r.get_namespace(), r.get_local_part(), 0);
            d.begin_line = r.begin_line;
            d.begin_column = r.begin_column;
            d.end_line = r.end_line;
            d.end_column = r.end_column;
            d.image = r.image.clone();
            d.comment = Some(if self.generate_ast { PRIVATE_AST_COMMENT } else { "@private" }.to_string());
            r.copy(&mut d);
            d
        };
        let def = def.into_ref();
        {
            let mut d = def.borrow_mut();
            d.set_local_class_mappings(self.doc_info.get_local_class_mappings());
            d.set_language_namespace(self.doc_info.get_language_namespace().as_deref());
            d.set_version(self.doc_info.get_version());
        }
        let source = new_inline_source(&usrc, &class_name);
        source.borrow_mut().add_source_fragment(ATTR_INLINE_COMPONENT_SYNTAX_TREE, Rc::new(def), None);
        unit.borrow_mut().add_generated_source(class_qname, source);
    }

    fn get_inner_class_name(&mut self, attribute: Option<&Attribute>) -> String {
        let mut result = None;
        if let Some(attribute) = attribute {
            let r = attribute.get_value().as_str().unwrap_or_default().to_string();
            if !text_parser::is_valid_identifier(&r) {
                self.adapter.log_line(attribute.get_line(), msg_error("class name must be a valid ActionScript identifier.".to_string()));
            } else if self.inner_class_names.contains(&r) {
                self.adapter.log_line(attribute.get_line(), msg_error("class name specified more than once in document.".to_string()));
            } else {
                self.inner_class_names.insert(r.clone());
                result = Some(r);
            }
        }
        result.unwrap_or_else(|| {
            let r = format!("{}InnerClass{}", self.doc_info.get_class_name(), self.inner_class_count);
            self.inner_class_count += 1;
            r
        })
    }

    /// `processStateNode(node)`.
    fn process_state_node(&mut self, node: &NodeRef) {
        let state_name_attr = self.get_language_attribute(node, standard_defs::PROP_STATE_NAME);
        let exclude = self.get_language_attribute_value(node, standard_defs::PROP_EXCLUDE_STATES);
        let include = self.get_language_attribute_value(node, standard_defs::PROP_INCLUDE_STATES);
        let mut state_name = None;
        let mut line = 0;
        if let Some(a) = &state_name_attr {
            state_name = a.get_value().as_str().map(str::to_string);
            line = a.get_line();
        }
        if text_parser::is_valid_state_identifier(state_name.as_deref()) || self.get_document_version() < 4 {
            if include.is_none() && exclude.is_none() {
                self.doc_info.add_state_name(state_name.as_deref().unwrap_or("null"), line);
                let groups = node.borrow().get_attribute_str(standard_defs::PROP_STATE_GROUPS).map(str::to_string);
                for group_name in text_parser::parse_string_list(groups.as_deref()) {
                    if text_parser::is_valid_state_identifier(Some(&group_name)) {
                        self.doc_info.add_state_group(&group_name, state_name.as_deref().unwrap_or("null"), line);
                    } else {
                        self.adapter.log_node_line(node, line, msg_error(format!("'{group_name}' is not a valid identifier.")));
                    }
                }
            } else {
                let local = node.borrow().get_local_part().to_string();
                self.adapter.log_node_line(node, line, msg_error(format!("The includeIn and excludeFrom attributes are not allowed on MXML language tag '{local}'.")));
            }
        } else {
            match state_name {
                None => self.adapter.log_node_line(node, line, msg_error("'name' is a required attribute for the State tag.".to_string())),
                Some(s) => self.adapter.log_node_line(node, line, msg_error(format!("'{s}' is not a valid identifier."))),
            }
        }
    }
}

fn new_inline_source(usrc: &SourceRef, class_name: &str) -> SourceRef {
    let (name, parent, last_modified) = {
        let s = usrc.borrow();
        (s.get_name().to_string(), s.get_parent(), s.get_last_modified())
    };
    let virtual_file = VirtualFile::text(String::new(), &format!("{name}${class_name}"), Some(&name), parent, mime_mappings::MXML, last_modified);
    Source::new_derived(virtual_file, usrc, class_name, false, false)
}

/// `addExcludeClassNode(inlineDocumentNode, componentRoot)`.
fn add_exclude_class_node(inline_document_node: &NodeRef, component_root: &NodeRef) {
    let (ns, local, image) = {
        let r = component_root.borrow();
        (r.get_namespace().to_string(), r.get_local_part().to_string(), r.image.clone())
    };
    let mut exclude = Node::with_class(NodeClass::MetaData, &ns, &local, 0);
    exclude.image = image;
    let mut text = Node::new_cdata();
    text.image = "[ExcludeClass]".to_string();
    exclude.add_child(text.into_ref());
    inline_document_node.borrow_mut().add_child(exclude.into_ref());
}

impl Analyzer for InterfaceAnalyzer {
    fn adapter(&self) -> &AnalyzerAdapter {
        &self.adapter
    }
    fn adapter_mut(&mut self) -> &mut AnalyzerAdapter {
        &mut self.adapter
    }
    fn get_document_version(&self) -> i32 {
        self.doc_info.get_version()
    }
    fn get_language_namespace(&self) -> Option<String> {
        self.doc_info.get_language_namespace()
    }

    fn analyze_node(&mut self, node: &NodeRef) {
        let mut in_repeater = false;
        let (ns, local) = {
            let n = node.borrow();
            (n.get_namespace().to_string(), n.get_local_part().to_string())
        };
        if let Some(class_name) = self.name_mappings.resolve_class_name(&ns, &local) {
            if self.standard_defs.is_repeater_name(&class_name) {
                self.repeater_num += 1;
                in_repeater = true;
            }
        }
        self.register_variable_for_id(node, None);
        self.traverse(node);
        if in_repeater {
            self.repeater_num -= 1;
        }
        if node.borrow().is_document_node() {
            let layers: Vec<NodeRef> = node.borrow().layer_declaration_nodes().to_vec();
            for l in layers {
                self.register_variable_for_id(&l, None);
            }
        }
    }

    fn analyze_layered_node(&mut self, node: &NodeRef) {
        self.analyze_node(node);
        let mut layer_parent = node.borrow().get_layer_parent();
        while let Some(lp) = layer_parent {
            if !self.declared_layers.iter().any(|d| Rc::ptr_eq(d, &lp)) {
                self.register_variable_for_id(&lp, None);
                self.declared_layers.push(lp.clone());
            }
            layer_parent = lp.borrow().get_layer_parent();
        }
    }

    fn analyze_script_node(&mut self, node: &NodeRef) {
        if node.borrow().get_source_file().is_none() {
            let cdata = node.borrow().get_child_at(0);
            if let Some(cdata) = cdata {
                let c = cdata.borrow();
                let mut script = Script::with_lines(&c.image, c.begin_line, c.end_line);
                script.set_embedded_script(true);
                self.doc_info.add_script(script);
            }
        } else {
            let (source, line) = {
                let n = node.borrow();
                (n.get_attribute_str("source").map(str::to_string), n.begin_line)
            };
            if let Some(source) = source {
                self.doc_info.add_script(Script::new(&format!("include \"{source}\";"), line));
            }
        }
    }

    fn analyze_meta_data_node(&mut self, node: &NodeRef) {
        let cdata = node.borrow().get_child_at(0);
        if let Some(cdata) = cdata {
            let c = cdata.borrow();
            self.doc_info.add_metadata(Script::with_lines(&c.image, c.begin_line, c.end_line));
        }
    }

    fn analyze_model_node(&mut self, node: &NodeRef) {
        let c = name_formatter::to_dot(&self.standard_defs.class_objectproxy);
        self.register_variable_for_id(node, Some(c));
    }

    fn analyze_xml_node(&mut self, node: &NodeRef) {
        let (e4x, line) = {
            let n = node.borrow();
            (n.is_e4x(), n.begin_line)
        };
        if !e4x {
            self.doc_info.add_import_name(&name_formatter::to_dot(&self.standard_defs.class_xmlutil), line);
        }
        let c = name_formatter::to_dot(self.standard_defs.get_xml_backing_class_name(e4x));
        self.register_variable_for_id(node, Some(c));
    }

    fn analyze_xml_list_node(&mut self, node: &NodeRef) {
        self.register_variable_for_id(node, Some(name_formatter::to_dot(standard_defs::CLASS_XMLLIST)));
    }

    fn analyze_array_node(&mut self, node: &NodeRef) {
        self.register_variable_for_id(node, Some(standard_defs::CLASS_ARRAY.to_string()));
        self.traverse(node);
    }

    fn analyze_vector_node(&mut self, node: &NodeRef) {
        if self.get_language_attribute(node, standard_defs::PROP_TYPE).is_some() {
            self.register_variable_for_id(node, Some(standard_defs::CLASS_VECTOR.to_string()));
            self.traverse(node);
        } else {
            let line = node.borrow().begin_line;
            self.adapter.log_node_line(node, line, msg_error("'type' is a required attribute for the Vector tag.".to_string()));
        }
    }

    fn analyze_binding_node(&mut self, _node: &NodeRef) {}
    fn analyze_private_node(&mut self, _node: &NodeRef) {}

    fn analyze_string_node(&mut self, node: &NodeRef) {
        self.register_variable_for_id(node, None);
    }
    fn analyze_number_node(&mut self, node: &NodeRef) {
        self.register_variable_for_id(node, None);
    }
    fn analyze_int_node(&mut self, node: &NodeRef) {
        self.register_variable_for_id(node, None);
    }
    fn analyze_uint_node(&mut self, node: &NodeRef) {
        self.register_variable_for_id(node, None);
    }
    fn analyze_boolean_node(&mut self, node: &NodeRef) {
        self.register_variable_for_id(node, None);
    }
    fn analyze_web_service_node(&mut self, node: &NodeRef) {
        self.register_variable_for_id(node, None);
        self.traverse(node);
    }
    fn analyze_http_service_node(&mut self, node: &NodeRef) {
        self.register_variable_for_id(node, None);
        self.traverse(node);
    }
    fn analyze_remote_object_node(&mut self, node: &NodeRef) {
        self.register_variable_for_id(node, None);
        self.traverse(node);
    }
    fn analyze_inline_component_node(&mut self, node: &NodeRef) {
        self.create_inline_component_unit(node);
        let c = self.standard_defs.interface_ifactory.clone();
        self.register_variable_for_id(node, Some(c));
    }
    fn analyze_definition_node(&mut self, node: &NodeRef) {
        self.create_definition_unit(node);
    }
    fn analyze_state_node(&mut self, node: &NodeRef) {
        self.process_state_node(node);
        self.register_variable_for_id(node, None);
        self.traverse(node);
    }

    fn traverse(&mut self, node: &NodeRef) {
        let mut i = 0;
        while i < node.borrow().get_child_count() {
            let child = node.borrow().get_child_at(i).expect("NullPointerException");
            dom_node::analyze(&child, self);
            if child.borrow().is_script_node() {
                node.borrow_mut().replace_node(i, Vec::new());
                continue;
            }
            i += 1;
        }
    }
}

/// `DependencyAnalyzer`.
struct DependencyAnalyzer {
    adapter: AnalyzerAdapter,
    type_table: Rc<TypeTable>,
    info: DocumentInfoRef,
    name_mappings: Rc<NameMappings>,
    standard_defs: Rc<StandardDefs>,
    check_nodes: Vec<NodeRef>,
    new_type_requests: JHashSet<MultiName>,
    all_type_requests: Rc<TypeRequests>,
}

impl DependencyAnalyzer {
    fn new(unit: UnitRef, type_table: Rc<TypeTable>, info: DocumentInfoRef, name_mappings: Rc<NameMappings>, all_type_requests: Rc<TypeRequests>) -> DependencyAnalyzer {
        DependencyAnalyzer {
            adapter: AnalyzerAdapter::new(unit),
            type_table,
            info,
            name_mappings,
            standard_defs: StandardDefs::get_standard_defs("halo"),
            check_nodes: Vec::new(),
            new_type_requests: JHashSet::new(),
            all_type_requests,
        }
    }

    fn add_check_node(&mut self, node: &NodeRef) {
        if !self.check_nodes.iter().any(|n| Rc::ptr_eq(n, node)) {
            self.check_nodes.push(node.clone());
        }
    }

    fn add_type_request(&mut self, name: &str) {
        self.new_type_requests.insert(name_formatter::to_multi_name(name));
    }

    fn unit_source_name(&self) -> String {
        self.adapter.unit.borrow().get_source().borrow().get_name_for_reporting().to_string()
    }

    fn register_dependencies(&mut self, node: &NodeRef) {
        let local = node.borrow().get_local_part().to_string();
        self.register_dependencies_as(node, &local);
    }

    fn register_dependencies_as(&mut self, node: &NodeRef, local_part: &str) {
        let (ns, image, line) = {
            let n = node.borrow();
            (n.get_namespace().to_string(), n.image.clone(), n.begin_line)
        };
        let mut class_name = self.info.get_local_class(&ns, local_part);
        if class_name.is_none() {
            class_name = self.name_mappings.resolve_class_name(&ns, local_part);
        }
        let Some(class_name) = class_name else {
            toolkit::log(msg_error(format!("Could not resolve {image} to a component implementation.")), Some(&self.unit_source_name()), line, -1, None);
            return;
        };
        if let Some(t) = self.request_type(&class_name, node) {
            let names = node.borrow().get_attribute_names();
            for qname in names {
                if toolkit::error_count() != 0 {
                    break;
                }
                let mut scanner = AttributeDependencyScanner { da: self, st: AttributeHandlerState::default() };
                scanner.invoke_attribute(node, &t, &qname);
            }
            let tt = self.type_table.clone();
            let is_fxg = self.info.get_language_namespace().as_deref() == Some(FXG_2008_NAMESPACE);
            let mut scanner = ChildNodeDependencyScanner { da: self, st: ChildNodeHandlerState::new(tt, is_fxg) };
            scanner.scan_child_nodes(node, &t);
        }
    }

    fn request_type(&mut self, class_name: &str, node: &NodeRef) -> Option<TypeRef> {
        let line = node.borrow().begin_line;
        self.info.add_import_name(&name_formatter::to_dot(class_name), line);
        let t = self.type_table.get_type(class_name);
        if t.is_none() {
            let multi_name = name_formatter::to_multi_name(class_name);
            if self.all_type_requests.borrow().contains(&multi_name) {
                let tag = if node.borrow().is_vector_node() { class_name.to_string() } else { node.borrow().image.clone() };
                toolkit::log(msg_error(format!("Could not resolve {tag} to a component implementation.")), Some(&self.unit_source_name()), line, -1, None);
            } else {
                self.add_check_node(node);
                self.new_type_requests.insert(multi_name);
            }
        }
        t
    }

    fn request_event_type(&mut self, event: &EventRef) {
        if event.get_type().is_none() {
            let n = event.get_type_name().to_string();
            self.add_type_request(&n);
        }
    }

    fn parse_class_initializer(&mut self, text: Option<&str>, t: &TypeRef) {
        let is_fxg = self.get_language_namespace().as_deref() == Some(FXG_2008_NAMESPACE);
        let mut flags = 0;
        if is_fxg {
            flags |= text_parser::FLAG_IGNORE_BINDING;
        }
        let tt = self.type_table.clone();
        let mut p = ClassInitializerTextParser { da: self, type_table: tt };
        p.parse(text.expect("NullPointerException"), t, None, flags);
    }
}

impl Analyzer for DependencyAnalyzer {
    fn adapter(&self) -> &AnalyzerAdapter {
        &self.adapter
    }
    fn adapter_mut(&mut self) -> &mut AnalyzerAdapter {
        &mut self.adapter
    }
    fn get_document_version(&self) -> i32 {
        self.info.get_version()
    }
    fn get_language_namespace(&self) -> Option<String> {
        self.info.get_language_namespace()
    }

    fn analyze_node(&mut self, node: &NodeRef) {
        self.register_dependencies(node);
    }
    fn analyze_layered_node(&mut self, node: &NodeRef) {
        self.analyze_node(node);
    }
    fn analyze_cdata_node(&mut self, _node: &NodeRef) {}
    fn analyze_style_node(&mut self, _node: &NodeRef) {}
    fn analyze_script_node(&mut self, _node: &NodeRef) {}
    fn analyze_meta_data_node(&mut self, _node: &NodeRef) {}
    fn analyze_model_node(&mut self, node: &NodeRef) {
        let c = self.standard_defs.class_objectproxy.clone();
        self.request_type(&c, node);
    }
    fn analyze_xml_node(&mut self, node: &NodeRef) {
        let e4x = node.borrow().is_e4x();
        if !e4x {
            let c = self.standard_defs.class_xmlutil.clone();
            self.request_type(&c, node);
        }
        self.request_type(self.standard_defs.get_xml_backing_class_name(e4x), node);
    }
    fn analyze_xml_list_node(&mut self, node: &NodeRef) {
        self.request_type(standard_defs::CLASS_XMLLIST, node);
    }
    fn analyze_binding_node(&mut self, _node: &NodeRef) {}
    fn analyze_string_node(&mut self, _node: &NodeRef) {}
    fn analyze_number_node(&mut self, _node: &NodeRef) {}
    fn analyze_int_node(&mut self, _node: &NodeRef) {}
    fn analyze_uint_node(&mut self, _node: &NodeRef) {}
    fn analyze_boolean_node(&mut self, _node: &NodeRef) {}
    fn analyze_web_service_node(&mut self, node: &NodeRef) {
        self.register_dependencies(node);
    }
    fn analyze_state_node(&mut self, node: &NodeRef) {
        self.register_dependencies(node);
    }
    fn analyze_http_service_node(&mut self, node: &NodeRef) {
        self.register_dependencies(node);
    }
    fn analyze_remote_object_node(&mut self, node: &NodeRef) {
        self.register_dependencies(node);
    }
    fn analyze_operation_node(&mut self, node: &NodeRef) {
        let n = self.standard_defs.get_converted_tag_name(node);
        self.register_dependencies_as(node, &n);
    }
    fn analyze_request_node(&mut self, _node: &NodeRef) {}
    fn analyze_method_node(&mut self, node: &NodeRef) {
        let n = self.standard_defs.get_converted_tag_name(node);
        self.register_dependencies_as(node, &n);
    }
    fn analyze_arguments_node(&mut self, _node: &NodeRef) {}
    fn analyze_inline_component_node(&mut self, node: &NodeRef) {
        let class_qname = node.borrow().get_class_qname();
        match class_qname {
            None => self.adapter.log(node, msg_error("internal error processing inline component.".to_string())),
            Some(q) => {
                let cf = self.standard_defs.class_classfactory.clone();
                self.add_type_request(&cf);
                self.new_type_requests.insert(MultiName::with_namespaces(vec![q.get_namespace().to_string()], q.get_local_part()));
                let line = node.borrow().begin_line;
                self.info.add_import_name(&name_formatter::to_dot(&cf), line);
                self.info.add_import_name(&name_formatter::to_dot_ns(q.get_namespace(), q.get_local_part()), line);
            }
        }
    }
    fn analyze_vector_node(&mut self, node: &NodeRef) {
        let class_name = self.get_language_attribute_value(node, standard_defs::PROP_TYPE).and_then(|v| v.as_str().map(str::to_string)).expect("NullPointerException");
        self.request_type(&name_formatter::to_colon(&class_name), node);
    }
    fn analyze_definition_node(&mut self, node: &NodeRef) {
        if node.borrow().get_name().is_none() {
            self.adapter.log(node, msg_error("internal error processing Definition node.".to_string()));
        }
    }
    fn analyze_reparent_node(&mut self, _node: &NodeRef) {}
}

struct AttributeDependencyScanner<'a> {
    da: &'a mut DependencyAnalyzer,
    st: AttributeHandlerState,
}

impl DeclarationHandler for AttributeDependencyScanner<'_> {
    fn event(&mut self, event: &EventRef) {
        self.da.request_event_type(event);
    }
    fn property(&mut self, property: &PropertyRef) {
        let Some(t) = property.get_type() else { return };
        if *t == *t.get_type_table().class_type() || self.da.standard_defs.is_instance_generator(&t) {
            let text = self.st.text.clone();
            self.da.parse_class_initializer(text.as_deref(), &t);
        }
    }
    fn states(&mut self, _property: &PropertyRef) {}
    fn effect(&mut self, _effect: &EffectRef) {}
    fn style(&mut self, _style: &StyleRef) {}
    fn dynamic_property(&mut self, _name: &str, _state: Option<&str>) {}
    fn unknown(&mut self, _namespace: &str, _local_part: &str) {}
}

impl AttributeHandler for AttributeDependencyScanner<'_> {
    fn ah(&mut self) -> &mut AttributeHandlerState {
        &mut self.st
    }
    fn process_scoped_names(&self) -> bool {
        true
    }
    fn qualified_attribute(&mut self, _node: &NodeRef, _t: &TypeRef, _namespace: &str, _local_part: &str) {}
    fn is_special(&mut self, _namespace: &str, _local_part: &str) -> bool {
        false
    }
    fn special(&mut self, _t: &TypeRef, _namespace: &str, _local_part: &str) {}
    fn unknown_namespace(&mut self, _namespace: &str, _local_part: &str) {}
}

struct ChildNodeDependencyScanner<'a> {
    da: &'a mut DependencyAnalyzer,
    st: ChildNodeHandlerState,
}

impl ChildNodeDependencyScanner<'_> {
    fn scan_grandchildren(&mut self) {
        let child = self.st.child();
        let children = child.borrow().get_children();
        for n in children {
            dom_node::analyze(&n, self.da);
        }
    }

    fn scan_child(&mut self) {
        let child = self.st.child();
        dom_node::analyze(&child, self.da);
    }
}

impl ChildNodeHandler for ChildNodeDependencyScanner<'_> {
    fn cnh(&mut self) -> &mut ChildNodeHandlerState {
        &mut self.st
    }
    fn event(&mut self, event: &EventRef) {
        self.da.request_event_type(event);
        self.scan_grandchildren();
    }
    fn states(&mut self, property: &PropertyRef) {
        if self.da.info.get_version() >= 4 {
            let sd = self.da.standard_defs.clone();
            self.da.add_type_request(&sd.class_setproperty);
            self.da.add_type_request(&sd.class_seteventhandler);
            self.da.add_type_request(&sd.class_setstyle);
            self.da.add_type_request(&sd.class_additems);
            self.da.add_type_request(&sd.interface_ioverride);
            self.scan_grandchildren();
        } else {
            self.property(property);
        }
    }
    fn property(&mut self, property: &PropertyRef) {
        if let Some(t) = property.get_type() {
            if *t == *t.get_type_table().class_type() || self.da.standard_defs.is_instance_generator(&t) {
                let children = self.st.child().borrow().get_children();
                let cdata = self.da.get_text_content(&children, true);
                if let Some(cdata) = cdata {
                    let image = cdata.borrow().image.clone();
                    self.da.parse_class_initializer(Some(&image), &t);
                }
            }
        }
        self.scan_grandchildren();
    }
    fn effect(&mut self, _effect: &EffectRef) {
        self.scan_grandchildren();
    }
    fn style(&mut self, _style: &StyleRef) {
        self.scan_grandchildren();
    }
    fn dynamic_property(&mut self, _name: &str, _state: Option<&str>) {
        if self.st.parent().borrow().is_document_node() {
            self.scan_child();
        } else {
            self.scan_grandchildren();
        }
    }
    fn default_property_element(&mut self, _loc_error: bool) {
        self.scan_child();
    }
    fn nested_declaration(&mut self) {
        self.scan_child();
    }
    fn text_content(&mut self) {}
    fn language_node(&mut self) {
        self.scan_child();
    }
}

/// `ClassInitializerTextParser`: only picks up class names.
struct ClassInitializerTextParser<'a> {
    da: &'a mut DependencyAnalyzer,
    type_table: Rc<TypeTable>,
}

impl TextParser for ClassInitializerTextParser<'_> {
    fn type_table(&self) -> Rc<TypeTable> {
        self.type_table.clone()
    }
    fn ignore_two_way_binding(&self) -> bool {
        false
    }
    fn context_root(&mut self, _text: &str) -> Option<RValue> {
        None
    }
    fn clear(&mut self) -> Option<RValue> {
        None
    }
    fn embed(&mut self, _text: &str, t: &TypeRef) -> Option<RValue> {
        if self.da.standard_defs.is_ifactory(t) {
            let cf = self.da.standard_defs.class_classfactory.clone();
            self.da.add_type_request(&cf);
        }
        None
    }
    fn resource(&mut self, _text: &str, _t: &TypeRef) -> Option<RValue> {
        None
    }
    fn binding_expression(&mut self, _converted: &str) -> Option<BindingExpressionRef> {
        None
    }
    fn binding_expression_two_way(&mut self, _converted: &str, _is_two_way: bool) -> Option<BindingExpressionRef> {
        None
    }
    fn parse_binding_expression_line(&mut self, _text: &str, _line: i32) -> Option<BindingExpressionRef> {
        None
    }
    fn percentage(&mut self, _pct: &str) -> Option<RValue> {
        None
    }
    fn array(&mut self, _entries: Vec<RValue>, _array_element_type: Option<&TypeRef>) -> Option<RValue> {
        None
    }
    fn function_text(&mut self, _name: &str) -> Option<RValue> {
        None
    }
    fn class_name(&mut self, name: &str, t: &TypeRef) -> Option<RValue> {
        if self.da.standard_defs.is_ifactory(t) {
            let cf = self.da.standard_defs.class_classfactory.clone();
            self.da.add_type_request(&cf);
        }
        self.da.add_type_request(name);
        None
    }
    fn error(&mut self, _error: i32, _text: &str, _t: Option<&TypeRef>, _array_element_type: Option<&TypeRef>) {}
}

