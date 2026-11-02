//! Port of `flex2.compiler.mxml.lang.AttributeHandler`; implementors keep `type`, `text` and `line` in an
//! [`AttributeHandlerState`].

use super::declaration_handler::DeclarationHandler;
use super::text_parser;
use crate::asc::flex2::compiler::mxml::dom::node::NodeRef;
use crate::asc::flex2::compiler::mxml::reflect::TypeRef;
use crate::asc::flex2::compiler::util::QName;

#[derive(Default)]
pub struct AttributeHandlerState {
    pub type_: Option<TypeRef>,
    pub text: Option<String>,
    pub line: i32,
}

pub trait AttributeHandler: DeclarationHandler {
    fn ah(&mut self) -> &mut AttributeHandlerState;

    fn process_scoped_names(&self) -> bool {
        false
    }
    fn qualified_attribute(&mut self, node: &NodeRef, t: &TypeRef, namespace: &str, local_part: &str);
    fn is_special(&mut self, namespace: &str, local_part: &str) -> bool;
    fn special(&mut self, t: &TypeRef, namespace: &str, local_part: &str);
    fn unknown_namespace(&mut self, namespace: &str, local_part: &str);

    /// `invoke(node, type, qname)`.
    fn invoke_attribute(&mut self, node: &NodeRef, t: &TypeRef, qname: &QName) {
        {
            let n = node.borrow();
            let text = n.get_attribute_value_qname(qname).and_then(|v| v.as_str().map(str::to_string));
            let line = n.get_line_number_qname(qname);
            let st = self.ah();
            st.type_ = Some(t.clone());
            st.text = text;
            st.line = line;
        }
        let namespace = qname.get_namespace().to_string();
        let mut local_part = qname.get_local_part().to_string();
        if self.process_scoped_names() && text_parser::is_scoped_name(&local_part) {
            let stateful_name = text_parser::analyze_scoped_name(&local_part).expect("NullPointerException");
            local_part = stateful_name[0].clone();
        }
        if self.is_special(&namespace, &local_part) {
            self.special(t, &namespace, &local_part);
        } else if !namespace.is_empty() {
            self.qualified_attribute(node, t, &namespace, &local_part);
        } else {
            self.invoke(t, &namespace, &local_part);
        }
    }
}
