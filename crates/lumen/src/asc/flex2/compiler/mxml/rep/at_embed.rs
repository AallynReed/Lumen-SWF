//! Port of `flex2.compiler.mxml.rep.AtEmbed`. The class and its CSS factories live in `compiler/css/at_embed.rs`
//! (CSS shares the type); this file adds MXML's `@Embed` factory, `codegenEmbedVar` and the template view.

use std::cell::RefCell;
use std::rc::Rc;

pub use crate::asc::flex2::compiler::css::at_embed::AtEmbed;
use crate::asc::flex2::compiler::css::at_embed::{create_mangled_name_for, parse_meta_data, EmbedParams, EmbedValue};
use crate::asc::flex2::compiler::source::SourceRef;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::velocity::{JType, JavaException, JavaObject, Method, Value};
use crate::asc::util::context::ContextStatics;
use crate::asc::util::jstr;

/// `AtEmbed.create(perCompileData, sourceFile, beginLine, value, strType)`: an MXML `@Embed(...)`; `value` starts
/// with the directive's `@`.
pub fn create(per_compile_data: &Rc<RefCell<ContextStatics>>, source_file: &SourceRef, begin_line: i32, value: &str, str_type: bool) -> Option<Rc<AtEmbed>> {
    let meta_data = parse_meta_data(per_compile_data, source_file, begin_line, &jstr::substring_from(value, 1))?;
    if meta_data.count() == 0 {
        let name = source_file.borrow().get_name_for_reporting().to_string();
        toolkit::log(CompilerMessage::error("No parameters provided for @Embed()."), Some(&name), begin_line, -1, None);
        return None;
    }
    let mut source = meta_data.get_value_key("source").map(str::to_string);
    if source.is_none() && meta_data.get_key(0).is_none() && meta_data.count() == 1 {
        source = Some(meta_data.get_value(0).to_string());
    }
    let mut values = EmbedParams::new();
    for i in 0..meta_data.count() {
        let v = EmbedValue::Str(meta_data.get_value(i).to_string());
        match meta_data.get_key(i) {
            None => values.insert("source".to_string(), v),
            Some(k) => values.insert(k.to_string(), v),
        };
    }
    let embed_name = create_mangled_name_for(source_file, source.as_deref(), str_type, &values, Some("_embed_mxml_"));
    Some(Rc::new(AtEmbed::new(embed_name, begin_line, values, str_type)))
}

/// `codegenEmbedVar()`.
pub fn codegen_embed_var(e: &AtEmbed) -> String {
    let line_sep = if cfg!(windows) { "\r\n" } else { "\n" };
    let attrs: Vec<String> = e.get_attributes().iter().map(|(k, v)| format!("{k}='{v}'")).collect();
    format!("    [Embed({})]{line_sep}    private static var {}:{};{line_sep}", attrs.join(", "), e.get_prop_name(), e.get_type())
}

/// An `AtEmbed` as the MXML templates see it.
pub struct AtEmbedView(pub Rc<AtEmbed>);

static METHODS: &[Method] = &[
    Method::new("getPropName", &[], JType::String),
    Method::new("getAttributes", &[], JType::Class(std::borrow::Cow::Borrowed("java.util.Map"))),
    Method::new("getXmlLineNumber", &[], JType::Int),
    Method::new("getType", &[], JType::String),
    Method::new("codegenEmbedVar", &[], JType::String),
];

impl JavaObject for AtEmbedView {
    fn class_name(&self) -> &str {
        "flex2.compiler.mxml.rep.AtEmbed"
    }

    fn methods(&self) -> &[Method] {
        METHODS
    }

    fn invoke(&self, method: &Method, _args: &[Value]) -> Result<Value, JavaException> {
        Ok(match &*method.name {
            "getPropName" => Value::str(self.0.get_prop_name()),
            "getAttributes" => Value::map_of("java.util.HashMap", self.0.get_attributes().iter().map(|(k, v)| (Value::str(k), Value::str(&v.to_string()))).collect()),
            "getXmlLineNumber" => Value::Integer(self.0.get_xml_line_number()),
            "getType" => Value::str(self.0.get_type()),
            "codegenEmbedVar" => Value::str(&codegen_embed_var(&self.0)),
            _ => Value::Null,
        })
    }

    fn to_java_string(&self) -> String {
        self.0.get_prop_name().to_string()
    }

    fn java_equals(&self, other: &Value) -> bool {
        other.to_java_string() == self.0.get_prop_name()
    }

    fn java_hash_code(&self) -> i32 {
        jstr::java_hash_code(self.0.get_prop_name())
    }
}
