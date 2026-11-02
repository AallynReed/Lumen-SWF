//! Port of `flex2.compiler.mxml.rep.init.EventInitializer`.

use std::collections::HashSet;
use std::rc::Rc;

use crate::asc::flex2::compiler::mxml::gen::code_fragment_list::CodeFragmentList;
use crate::asc::flex2::compiler::mxml::gen::text_gen;
use crate::asc::flex2::compiler::mxml::rep::deps::Type;
use crate::asc::flex2::compiler::mxml::rep::event_handler::EventHandler;
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::flex2::compiler::as3::abstract_syntax_tree_util::{self as ast, Nf};
use crate::asc::parser::node::NodeRef;
use crate::asc::parser::tokens::EMPTY_TOKEN;

pub struct EventInitializer {
    handler: Rc<EventHandler>,
}

impl EventInitializer {
    pub fn new(handler: Rc<EventHandler>) -> EventInitializer {
        EventInitializer { handler }
    }

    pub fn get_handler(&self) -> &Rc<EventHandler> {
        &self.handler
    }

    pub fn get_name(&self) -> String {
        self.handler.get_name()
    }

    pub fn get_lvalue_type(&self) -> Type {
        self.handler.get_type()
    }

    pub fn get_line_ref(&self) -> i32 {
        self.handler.get_xml_line_number()
    }

    pub fn get_handler_text(&self) -> String {
        self.handler.get_event_handler_text().to_string()
    }

    pub fn get_value_expr(&self) -> String {
        self.handler.get_document_function_name()
    }

    /// `generateValueExpr(nodeFactory, ...)`.
    pub fn generate_value_expr(&self, nf: &Nf) -> NodeRef {
        ast::generate_getter_selector(&mut nf.f(), &self.handler.get_document_function_name(), true)
    }

    pub fn get_assign_expr(&self, lvalue_base: &str) -> String {
        format!("{lvalue_base}.addEventListener({}, {})", text_gen::quote_word(&self.get_name()), self.get_value_expr())
    }

    /// `generateAssignExpr(nodeFactory, ..., statementList, lvalueBase)`.
    pub fn generate_assign_expr(&self, nf: &Nf, statement_list: Option<NodeRef>, lvalue_base: NodeRef) -> NodeRef {
        let identifier = nf.f().identifier_intern("addEventListener", false);
        let literal_string = nf.f().literal_string(&self.get_name());
        let argument_list = nf.f().argument_list(None, Some(literal_string));
        let value_expr = self.generate_value_expr(nf);
        let argument_list = nf.f().argument_list(Some(argument_list), Some(value_expr));
        let call_expression = ast::call(nf, identifier, Some(argument_list), false);
        let expression_statement = ast::expr_stmt(nf, Some(lvalue_base), call_expression);
        nf.f().statement_list(statement_list, Some(expression_statement))
    }

    pub fn get_definitions_iterator(&self) -> Vec<CodeFragmentList> {
        vec![self.get_definition_body()]
    }

    fn get_definition_body(&self) -> CodeFragmentList {
        let line = self.get_line_ref();
        let mut list = CodeFragmentList::new();
        list.add("/**", line);
        list.add(" * @private", line);
        list.add(" **/", line);
        list.add5("public function ", &self.handler.get_document_function_name(), "(event:", &name_formatter::to_dot(&self.handler.get_type().get_name().to_string()), "):void", line);
        list.add("{", line);
        list.add2("\t", self.handler.get_event_handler_text(), line);
        list.add("}", line);
        list
    }

    /// `generateDefinitions(context, configNamespaces, generateDocComments, statementList)`.
    pub fn generate_definitions(&self, context: &crate::asc::util::context::ContextRef, config_namespaces: &HashSet<String>, generate_doc_comments: bool, statement_list: Option<NodeRef>) -> NodeRef {
        let nf_owned = Nf::of(context);
        let nf = &nf_owned;
        let mut result = statement_list;
        if generate_doc_comments {
            let doc_comment = ast::generate_private_doc_comment(&mut nf.f());
            result = Some(nf.f().statement_list(result, Some(doc_comment)));
        }
        let type_ = name_formatter::to_dot(&self.handler.get_type().get_name().to_string());
        let parameter = ast::generate_parameter(&mut nf.f(), "event", &type_, true);
        let parameter_list = nf.f().parameter_list(None, Some(parameter));
        let function_signature = nf.f().function_signature(Some(parameter_list), None);
        ast::void_anno(&function_signature);
        let attribute_list = ast::generate_public_attribute(&mut nf.f());
        let identifier = nf.f().identifier(&self.handler.get_document_function_name());
        let function_name = nf.f().function_name(EMPTY_TOKEN, Some(identifier));
        let mut function_statement_list: Option<NodeRef> = None;
        let text = self.handler.get_event_handler_text().to_string();
        let list = ast::parse_expression(context, config_namespaces, &text, self.handler.get_xml_line_number(), generate_doc_comments);
        for node in list {
            function_statement_list = Some(nf.f().statement_list(function_statement_list, Some(node)));
        }
        let position = ast::line_number_to_position(&mut nf.f(), self.get_line_ref());
        let function_common = nf.f().function_common_pos(context, None, Some(function_signature), function_statement_list.clone(), position);
        ast::user_defined_body(&function_common);
        if let Some(l) = &function_statement_list {
            let last = l.borrow().as_statement_list().unwrap().items.last().cloned().flatten().expect("NoSuchElementException");
            assert!(last.borrow().as_return_statement().is_some(), "ClassCastException");
            last.borrow_mut().set_position_terminal(position);
        }
        let function_definition = nf.f().function_definition(context, Some(attribute_list), Some(function_name), Some(function_common));
        let result = nf.f().statement_list(result, Some(function_definition));
        result
    }
}
