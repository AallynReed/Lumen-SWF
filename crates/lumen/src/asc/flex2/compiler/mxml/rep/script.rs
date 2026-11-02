//! Port of `flex2.compiler.mxml.rep.Script`.

use std::rc::Rc;

use crate::asc::flex2::velocity::{JType, JavaException, JavaObject, Method, Value};

#[derive(Clone, Debug, Default)]
pub struct Script {
    xml_line_number: i32,
    end_xml_line_number: i32,
    pub(crate) text: String,
    is_embedded: bool,
}

impl Script {
    pub fn new(text: &str, line_number: i32) -> Script {
        Script::with_lines(text, line_number, line_number)
    }

    pub fn with_lines(text: &str, begin_line: i32, end_line: i32) -> Script {
        let mut s = Script::from_text(text);
        s.set_xml_line_number(begin_line);
        s.set_end_xml_line_number(end_line);
        s
    }

    /// `new Script(text)`.
    pub fn from_text(text: &str) -> Script {
        Script { text: text.to_string(), ..Default::default() }
    }

    pub fn get_xml_line_number(&self) -> i32 {
        self.xml_line_number
    }

    pub fn get_end_xml_line_number(&self) -> i32 {
        self.end_xml_line_number
    }

    pub fn get_text(&self) -> &str {
        &self.text
    }

    pub fn set_xml_line_number(&mut self, xml_line_number: i32) {
        self.xml_line_number = xml_line_number;
    }

    pub fn set_end_xml_line_number(&mut self, xml_line_number: i32) {
        self.end_xml_line_number = xml_line_number;
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
    }

    pub fn set_embedded_script(&mut self, b: bool) {
        self.is_embedded = b;
    }

    pub fn is_embedded(&self) -> bool {
        self.is_embedded
    }

    pub fn value(&self) -> Value {
        Value::Object(Rc::new(self.clone()))
    }
}

static SCRIPT_METHODS: &[Method] = &[
    Method::new("getXmlLineNumber", &[], JType::Int),
    Method::new("getEndXmlLineNumber", &[], JType::Int),
    Method::new("getText", &[], JType::String),
    Method::new("setXmlLineNumber", &[JType::Int], JType::Void),
    Method::new("setEndXmlLineNumber", &[JType::Int], JType::Void),
    Method::new("setXmlLineNumber", &[JType::Int, JType::Boolean], JType::Void),
    Method::new("setText", &[JType::String], JType::Void),
    Method::new("setEmbeddedScript", &[JType::Boolean], JType::Void),
    Method::new("isEmbedded", &[], JType::Boolean),
];

impl JavaObject for Script {
    fn class_name(&self) -> &str {
        "flex2.compiler.mxml.rep.Script"
    }

    fn methods(&self) -> &[Method] {
        SCRIPT_METHODS
    }

    fn invoke(&self, method: &Method, _args: &[Value]) -> Result<Value, JavaException> {
        Ok(match &*method.name {
            "getXmlLineNumber" => Value::Integer(self.xml_line_number),
            "getEndXmlLineNumber" => Value::Integer(self.end_xml_line_number),
            "getText" => Value::str(&self.text),
            "isEmbedded" => Value::Boolean(self.is_embedded),
            _ => Value::Null,
        })
    }
}
