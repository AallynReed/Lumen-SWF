//! Port of `flex2.compiler.mxml.InterfaceGenerator`: direct AST generation of the interface (public signature)
//! skeleton of an MXML class.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashSet};
use std::rc::Rc;

use super::abstract_generator::{AbstractGenerator, BINDABLE};
use super::lang::framework_defs;
use super::rep::document_info::DocumentInfoRef;
use crate::asc::flex2::compiler::as3::abstract_syntax_tree_util::{self as ast, Nf};
use crate::asc::flex2::compiler::as3::as3_compiler;
use crate::asc::flex2::compiler::source::SourceRef;
use crate::asc::parser::node::NodeRef;
use crate::asc::semantics::emitter::EmitterRef;
use crate::asc::util::context::{ConfigVar, ContextRef, ContextStatics};
use crate::asc::util::jstr;

pub struct InterfaceGenerator {
    base: AbstractGenerator,
    doc_info: DocumentInfoRef,
}

/// A `TreeSet<String>` element, ordered by `String.compareTo`.
#[derive(PartialEq, Eq)]
struct Key(String);

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Key {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        jstr::compare(&self.0, &other.0)
    }
}

impl InterfaceGenerator {
    /// `new InterfaceGenerator(docInfo, bogusImports, contextStatics, source, bytecodeEmitter, defines)`;
    /// `bogus_imports` in the set's iteration order.
    pub fn new(doc_info: &DocumentInfoRef, bogus_imports: &[String], context_statics: &Rc<RefCell<ContextStatics>>, source: &SourceRef, bytecode_emitter: &EmitterRef, defines: &[ConfigVar]) -> InterfaceGenerator {
        let mut base = AbstractGenerator::new(doc_info.get_standard_defs());
        let context = ast::generate_context(context_statics, source, bytecode_emitter, defines);
        base.node_factory = Some(Nf::of(&context));
        base.context = Some(context.clone());
        let mut config_namespaces = HashSet::new();
        let config_vars = ast::parse_config_vars(&context, &mut config_namespaces);
        base.config_namespaces = config_namespaces;
        let program_node = ast::generate_program(&context, config_vars, &doc_info.get_package_name());
        base.program_node = Some(program_node.clone());
        let g = InterfaceGenerator { base, doc_info: doc_info.clone() };
        let nf = g.base.nf().clone();
        let program_statement_list = program_node.borrow().program().unwrap().statements.clone().expect("NullPointerException");
        let program_statement_list = g.generate_imports(program_statement_list, bogus_imports);
        let program_statement_list = g.base.generate_meta_data(program_statement_list, &doc_info.get_metadata(), &g.get_path());
        let class_definition = g.generate_class_definition();
        let program_statement_list = nf.f().statement_list(Some(program_statement_list), Some(class_definition));
        program_node.borrow_mut().program_mut().unwrap().statements = Some(program_statement_list.clone());
        let package_definition = nf.f().finish_package(&context, None);
        nf.f().statement_list(Some(program_statement_list), package_definition);
        as3_compiler::clean_node_factory(&context);
        g
    }

    fn create_interface_names(&self) -> Vec<String> {
        let mut result = BTreeSet::new();
        for info in self.doc_info.get_interface_names().iter() {
            result.insert(Key(info.get_name().to_string()));
        }
        result.into_iter().map(|k| k.0).collect()
    }

    fn generate_binding_management_vars(&self, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.base.nf();
        let mut result = statement_list;
        for variable_declaration in framework_defs::binding_management_vars() {
            let qualified_identifier = ast::generate_mx_internal_qualified_identifier(&mut nf.f(), variable_declaration.get_name(), false);
            let attrs = self.base.generate_mx_internal_attribute();
            let variable_definition = ast::generate_variable_attrs(&mut nf.f(), Some(attrs), qualified_identifier, variable_declaration.get_type(), false, None);
            result = Some(nf.f().statement_list(result, Some(variable_definition)));
        }
        result
    }

    fn generate_class_definition(&self) -> NodeRef {
        let nf = self.base.nf();
        let class_name = self.doc_info.get_class_name();
        let constructor = ast::generate_constructor(self.base.cx(), &class_name, None, false, None, -1);
        let mut statement_list = Some(nf.f().statement_list(None, Some(constructor)));
        statement_list = self.generate_instance_variables(statement_list);
        statement_list = self.generate_binding_management_vars(statement_list);
        statement_list = self.base.generate_scripts(statement_list, &self.doc_info.get_scripts());
        ast::generate_class_definition(self.base.cx(), &class_name, &self.doc_info.get_qualified_super_class_name(), &self.create_interface_names(), statement_list)
    }

    fn generate_imports(&self, statement_list: NodeRef, bogus_imports: &[String]) -> NodeRef {
        let cx = self.base.cx();
        let nf = self.base.nf();
        let mut result = statement_list;
        for split_import in self.doc_info.get_split_import_names() {
            let import_directive = ast::generate_import_split(cx, &split_import);
            result = nf.f().statement_list(Some(result), Some(import_directive));
        }
        for name_info in self.doc_info.get_import_names().iter() {
            let import_directive = ast::generate_import(cx, name_info.get_name());
            result = nf.f().statement_list(Some(result), Some(import_directive));
        }
        for name in bogus_imports {
            let import_directive = ast::generate_import(cx, name);
            result = nf.f().statement_list(Some(result), Some(import_directive));
        }
        result
    }

    fn generate_instance_variables(&self, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.base.nf();
        let mut result = statement_list;
        for var_decl in self.doc_info.get_var_decls() {
            let bindable_meta_data = ast::generate_meta_data(&mut nf.f(), BINDABLE);
            result = Some(nf.f().statement_list(result, Some(bindable_meta_data)));
            let position = ast::line_number_to_position(&mut nf.f(), var_decl.line);
            let type_expression = ast::generate_type_expression_pos(&mut nf.f(), &var_decl.class_name, true, position);
            let variable_definition = ast::generate_public_variable(self.base.cx(), type_expression, &var_decl.name);
            result = Some(nf.f().statement_list(result, Some(variable_definition)));
        }
        result
    }

    fn get_path(&self) -> String {
        self.doc_info.get_path().to_string()
    }

    pub fn get_context(&self) -> ContextRef {
        self.base.get_context()
    }

    pub fn get_syntax_tree(&self) -> Option<NodeRef> {
        self.base.get_syntax_tree()
    }
}
