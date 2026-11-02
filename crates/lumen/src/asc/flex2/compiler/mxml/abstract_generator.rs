//! Port of `flex2.compiler.mxml.AbstractGenerator`: the state and helpers the interface and implementation AST
//! generators share.

use std::collections::HashSet;
use std::rc::Rc;

use super::lang::standard_defs::{StandardDefs, NAMESPACE_MX_INTERNAL_LOCALNAME};
use super::rep::script::Script;
use crate::asc::flex2::compiler::as3::abstract_syntax_tree_util::{self as ast, Nf};
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::parser::node::NodeRef;
use crate::asc::util::context::ContextRef;

pub const BINDABLE: &str = "Bindable";
pub const MX_INTERNAL: &str = NAMESPACE_MX_INTERNAL_LOCALNAME;

pub struct AbstractGenerator {
    pub standard_defs: Rc<StandardDefs>,
    pub context: Option<ContextRef>,
    pub node_factory: Option<Nf>,
    pub config_namespaces: HashSet<String>,
    pub program_node: Option<NodeRef>,
    pub generate_doc_comments: bool,
}

impl AbstractGenerator {
    pub fn new(defs: Rc<StandardDefs>) -> AbstractGenerator {
        AbstractGenerator { standard_defs: defs, context: None, node_factory: None, config_namespaces: HashSet::new(), program_node: None, generate_doc_comments: false }
    }

    pub fn cx(&self) -> &ContextRef {
        self.context.as_ref().expect("NullPointerException")
    }

    pub fn nf(&self) -> &Nf {
        self.node_factory.as_ref().expect("NullPointerException")
    }

    /// `generateMetaData(programStatementList, scripts)`; `path` is `getPath()`.
    pub fn generate_meta_data(&self, program_statement_list: NodeRef, scripts: &[Script], path: &str) -> NodeRef {
        let mut result = program_statement_list;
        for script in scripts {
            let list = ast::parse(self.cx(), &self.config_namespaces, script.get_text(), script.get_xml_line_number(), self.generate_doc_comments);
            for node in list {
                if node.borrow().as_meta_data().is_some() {
                    result = self.nf().f().statement_list(Some(result), Some(node));
                } else {
                    let input = self.cx().borrow().input.clone().expect("NullPointerException");
                    let pos = node.borrow().pos();
                    let line = input.borrow_mut().get_ln_num(pos) + script.get_xml_line_number() - 1;
                    toolkit::log(CompilerMessage::error("Only metadata is allowed."), Some(path), line, -1, None);
                }
            }
        }
        result
    }

    pub fn generate_mx_internal_attribute(&self) -> NodeRef {
        let nf = self.nf();
        let mx_internal_getter_selector = ast::generate_resolved_getter_selector(&mut nf.f(), self.standard_defs.get_core_package(), MX_INTERNAL);
        let list = nf.f().list(None, Some(mx_internal_getter_selector));
        nf.f().attribute_list(Some(list), None)
    }

    /// `generateScripts(statementList, scripts)`: the parsed nodes are appended as they are, not through
    /// `statementList()`, which would inline include directives a second time.
    pub fn generate_scripts(&self, statement_list: Option<NodeRef>, scripts: &[Script]) -> Option<NodeRef> {
        let result = statement_list;
        for script in scripts {
            let list = ast::parse(self.cx(), &self.config_namespaces, script.get_text(), script.get_xml_line_number(), self.generate_doc_comments);
            let r = result.as_ref().expect("NullPointerException");
            r.borrow_mut().as_statement_list_mut().unwrap().items.extend(list.into_iter().map(Some));
        }
        result
    }

    pub fn get_context(&self) -> ContextRef {
        self.cx().clone()
    }

    pub fn get_syntax_tree(&self) -> Option<NodeRef> {
        self.program_node.clone()
    }
}
