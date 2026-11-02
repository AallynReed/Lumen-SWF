//! Port of `flex2.compiler.mxml.rep.AtResource`.

use std::cell::Cell;
use std::rc::Rc;

use super::deps::{Type, TypeTable};
use crate::asc::flex2::compiler::as3::abstract_syntax_tree_util as ast;
use crate::asc::flex2::compiler::css::at_embed::parse_meta_data;
use crate::asc::flex2::compiler::source::SourceRef;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::velocity::{JType, JavaException, JavaObject, Method, Value};

#[derive(Debug)]
pub struct AtResource {
    bundle: String,
    key: String,
    method_name: String,
    line_number: Cell<i32>,
}

impl AtResource {
    pub fn new(bundle: &str, key: &str, method_name: &str, line_number: i32) -> AtResource {
        AtResource { bundle: bundle.into(), key: key.into(), method_name: method_name.into(), line_number: Cell::new(line_number) }
    }

    pub fn get_bundle(&self) -> &str {
        &self.bundle
    }

    pub fn get_value_expression(&self) -> String {
        format!("ResourceManager.getInstance().{}(\"{}\", \"{}\")", self.method_name, self.bundle, self.key)
    }

    /// `getValueExpression(nodeFactory)`.
    pub fn generate_value_expression(&self, nf: &ast::Nf) -> crate::asc::parser::node::NodeRef {
        let resource_manager_member_expression = ast::generate_getter_selector(&mut nf.f(), "ResourceManager", false);
        let get_instance_identifier = nf.f().identifier_intern("getInstance", false);
        let get_instance_call_expression = ast::call(nf, get_instance_identifier, None, false);
        let base = nf.f().member_expression(Some(resource_manager_member_expression), Some(get_instance_call_expression));
        let method_name_identifier = nf.f().identifier(&self.method_name);
        let bundle_literal_string = nf.f().literal_string(&self.bundle);
        let argument_list = nf.f().argument_list(None, Some(bundle_literal_string));
        let key_literal_string = nf.f().literal_string(&self.key);
        let argument_list = nf.f().argument_list(Some(argument_list), Some(key_literal_string));
        let selector = ast::call(nf, method_name_identifier, Some(argument_list), false);
        nf.f().member_expression(Some(base), Some(selector))
    }

    pub fn get_xml_line_number(&self) -> i32 {
        self.line_number.get()
    }

    pub fn set_xml_line_number(&self, xml_line_number: i32) {
        self.line_number.set(xml_line_number);
    }

    /// `AtResource.create(typeTable, sourceFile, beginLine, value, type)`; `value` starts with `@Resource`'s `@`.
    pub fn create(type_table: &TypeTable, source_file: &SourceRef, begin_line: i32, value: &str, type_: &Type) -> Option<Rc<AtResource>> {
        let method_name = if type_.is_assignable_to(Some(&type_table.string_type())) {
            "getString"
        } else if type_.is_assignable_to(Some(&type_table.boolean_type())) {
            "getBoolean"
        } else if type_.is_assignable_to(Some(&type_table.number_type())) {
            "getNumber"
        } else if type_.is_assignable_to(Some(&type_table.int_type())) {
            "getInt"
        } else if type_.is_assignable_to(Some(&type_table.uint_type())) {
            "getUint"
        } else if type_.is_assignable_to(Some(&type_table.class_type())) {
            "getClass"
        } else if type_.is_assignable_to(Some(&type_table.array_type())) {
            "getStringArray"
        } else {
            "getObject"
        };
        let md = crate::asc::util::jstr::substring_from(value, 1);
        let meta_data = parse_meta_data(&type_table.get_per_compile_data(), source_file, begin_line, &md)?;
        let name_for_reporting = source_file.borrow().get_name_for_reporting().to_string();
        if meta_data.count() == 0 {
            toolkit::log(CompilerMessage::error("No parameters provided for @Resource()."), Some(&name_for_reporting), begin_line, -1, None);
            return None;
        }
        let key = match meta_data.get_value_key("key") {
            Some(k) => k.to_string(),
            None => {
                if meta_data.get_key(0).is_none() && meta_data.count() == 1 {
                    meta_data.get_value(0).to_string()
                } else {
                    toolkit::log(CompilerMessage::error("No key parameter given for @Resource()."), Some(&name_for_reporting), begin_line, -1, None);
                    return None;
                }
            }
        };
        let bundle = match meta_data.get_value_key("bundle") {
            Some(b) => b.to_string(),
            None => {
                if meta_data.count() > 1 {
                    toolkit::log(CompilerMessage::error("No bundle parameter given for @Resource()."), Some(&name_for_reporting), begin_line, -1, None);
                    return None;
                }
                q_name_from_source(source_file)
            }
        };
        Some(Rc::new(AtResource::new(&bundle, &key, method_name, begin_line)))
    }
}

/// `NameFormatter.qNameFromSource(src).toString()` for a source-path or source-list source.
fn q_name_from_source(src: &SourceRef) -> String {
    let s = src.borrow();
    let rel = s.get_relative_path();
    let prefix = if rel.is_empty() { String::new() } else { format!("{}:", rel.replace(['/', '\\'], ".")) };
    prefix + s.get_short_name()
}

static METHODS: &[Method] = &[
    Method::new("getBundle", &[], JType::String),
    Method::new("getValueExpression", &[], JType::String),
    Method::new("getXmlLineNumber", &[], JType::Int),
    Method::new("setXmlLineNumber", &[JType::Int], JType::Void),
];

impl JavaObject for AtResource {
    fn class_name(&self) -> &str {
        "flex2.compiler.mxml.rep.AtResource"
    }

    fn methods(&self) -> &[Method] {
        METHODS
    }

    fn invoke(&self, method: &Method, args: &[Value]) -> Result<Value, JavaException> {
        Ok(match &*method.name {
            "getBundle" => Value::str(&self.bundle),
            "getValueExpression" => Value::str(&self.get_value_expression()),
            "getXmlLineNumber" => Value::Integer(self.get_xml_line_number()),
            "setXmlLineNumber" => {
                if let Some(Value::Integer(i)) = args.first() {
                    self.set_xml_line_number(*i);
                }
                Value::Null
            }
            _ => Value::Null,
        })
    }
}
