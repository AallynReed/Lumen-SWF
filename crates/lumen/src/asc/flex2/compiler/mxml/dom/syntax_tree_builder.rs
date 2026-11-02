//! Port of `flex2.compiler.mxml.dom.SyntaxTreeBuilder`: the visitor that attaches each parsed tag's children.

use crate::asc::flex2::compiler::mxml::parser::MxmlVisitor;
use crate::asc::flex2::compiler::mxml::token::Tok;

#[derive(Default)]
pub struct SyntaxTreeBuilder;

fn nodes(children: Option<Vec<Tok>>) -> Option<Vec<super::node::NodeRef>> {
    children.map(|c| c.iter().map(|t| t.as_node()).collect())
}

fn add_children(t: &Tok, children: Option<Vec<Tok>>) {
    t.as_node().borrow_mut().add_children(nodes(children));
}

fn add_children_if(t: &Tok, children: Option<Vec<Tok>>) {
    if children.is_some() {
        add_children(t, children);
    }
}

fn add_child_if(t: &Tok, child: Option<&Tok>) {
    if let Some(c) = child {
        t.as_node().borrow_mut().add_child(c.as_node());
    }
}

impl MxmlVisitor for SyntaxTreeBuilder {
    fn parse_application(&mut self, t: &Tok, components: Option<Vec<Tok>>) {
        add_children(t, components);
    }
    fn parse_component(&mut self, t: &Tok, components: Option<Vec<Tok>>) {
        add_children(t, components);
    }
    fn parse_style(&mut self, t: &Tok, text: Option<&Tok>) {
        add_child_if(t, text);
    }
    fn parse_script(&mut self, t: &Tok, text: Option<&Tok>) {
        add_child_if(t, text);
    }
    fn parse_meta_data(&mut self, t: &Tok, text: Option<&Tok>) {
        add_child_if(t, text);
    }
    fn parse_model(&mut self, t: &Tok, objects: Option<Vec<Tok>>) {
        add_children_if(t, objects);
    }
    fn parse_xml(&mut self, t: &Tok, objects: Option<Vec<Tok>>) {
        add_children_if(t, objects);
    }
    fn parse_xml_list(&mut self, t: &Tok, objects: Option<Vec<Tok>>) {
        add_children_if(t, objects);
    }
    fn parse_array(&mut self, t: &Tok, elements: Option<Vec<Tok>>) {
        add_children_if(t, elements);
    }
    fn parse_vector(&mut self, t: &Tok, elements: Option<Vec<Tok>>) {
        add_children_if(t, elements);
    }
    fn parse_binding(&mut self, _t: &Tok) {}
    fn parse_anonymous_object(&mut self, t: &Tok, objects: Option<Vec<Tok>>) {
        add_children_if(t, objects);
    }
    fn parse_web_service(&mut self, t: &Tok, children: Option<Vec<Tok>>) {
        add_children_if(t, children);
    }
    fn parse_http_service(&mut self, t: &Tok, children: Option<Vec<Tok>>) {
        add_children_if(t, children);
    }
    fn parse_remote_object(&mut self, t: &Tok, children: Option<Vec<Tok>>) {
        add_children_if(t, children);
    }
    fn parse_operation(&mut self, t: &Tok, children: Option<Vec<Tok>>) {
        add_children_if(t, children);
    }
    fn parse_request(&mut self, t: &Tok, children: Option<Vec<Tok>>) {
        add_children_if(t, children);
    }
    fn parse_method(&mut self, t: &Tok, children: Option<Vec<Tok>>) {
        add_children_if(t, children);
    }
    fn parse_arguments(&mut self, t: &Tok, children: Option<Vec<Tok>>) {
        add_children_if(t, children);
    }
    fn parse_string(&mut self, t: &Tok, data: Option<&Tok>) {
        add_child_if(t, data);
    }
    fn parse_number(&mut self, t: &Tok, data: Option<&Tok>) {
        add_child_if(t, data);
    }
    fn parse_int(&mut self, t: &Tok, data: Option<&Tok>) {
        add_child_if(t, data);
    }
    fn parse_uint(&mut self, t: &Tok, data: Option<&Tok>) {
        add_child_if(t, data);
    }
    fn parse_boolean(&mut self, t: &Tok, data: Option<&Tok>) {
        add_child_if(t, data);
    }
    fn parse_class(&mut self, t: &Tok, data: Option<&Tok>) {
        add_child_if(t, data);
    }
    fn parse_function(&mut self, t: &Tok, data: Option<&Tok>) {
        add_child_if(t, data);
    }
    fn parse_inline_component(&mut self, t: &Tok, child: Option<&Tok>) {
        add_child_if(t, child);
    }
    fn parse_declarations(&mut self, t: &Tok, declarations: Option<Vec<Tok>>) {
        add_children_if(t, declarations);
    }
    fn parse_reparent(&mut self, _t: &Tok) {}
    fn parse_state(&mut self, t: &Tok, overrides: Option<Vec<Tok>>) {
        add_children(t, overrides);
    }
    fn parse_library(&mut self, t: &Tok, children: Option<Vec<Tok>>) {
        add_children_if(t, children);
    }
    fn parse_definition(&mut self, t: &Tok, child: Option<&Tok>) {
        add_child_if(t, child);
    }
    fn parse_design_layer(&mut self, t: &Tok, children: Option<Vec<Tok>>) {
        add_children_if(t, children);
    }
}
