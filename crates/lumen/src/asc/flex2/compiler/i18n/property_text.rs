//! Port of `flex2.compiler.i18n.PropertyText`: a `.properties` file whose `ClassReference(...)` and `Embed(...)`
//! values become class references and embeds.

use std::cell::RefCell;
use std::rc::Rc;

use super::class_reference::ClassReference;
use super::resource_file::resource_file_of;
use crate::asc::flex2::compiler::css::at_embed::AtEmbed;
use crate::asc::flex2::compiler::source::SourceRef;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::ordered_properties::{OrderedProperties, PropertiesHooks};
use crate::asc::util::context::ContextStatics;
use crate::asc::util::jhash::JHashSet;

pub const CLASS_REFERENCE: &str = "ClassReference(";
pub const EMBED: &str = "Embed(";

/// A bundle value: a `String`, a `ClassReference` or an `AtEmbed`.
#[derive(Clone)]
pub enum PropertyValue {
    Str(String),
    ClassReference(ClassReference),
    Embed(Rc<AtEmbed>),
}

pub struct PropertyText {
    pub props: OrderedProperties<PropertyValue>,
    flex2_compatible: bool,
    per_compile_data: Rc<RefCell<ContextStatics>>,
    source: SourceRef,
    locale: String,
    pub imports: JHashSet<String>,
    /// `HashMap<String, AtEmbed>` by property name.
    pub at_embeds: Vec<(String, Rc<AtEmbed>)>,
}

struct Hooks {
    flex2_compatible: bool,
}

impl PropertiesHooks for Hooks {
    fn get_terminators(&self) -> &'static str {
        if self.flex2_compatible {
            "="
        } else {
            "=: \t"
        }
    }

    fn remove_bad_chars(&self, prop_line: &str, string: String, is_value: bool) -> String {
        if !self.flex2_compatible {
            return string;
        }
        let mut s = remove_bad_chars(prop_line, &string, "\\n");
        s = remove_bad_chars(prop_line, &s, "\\r");
        if is_value && !s.starts_with(CLASS_REFERENCE) && !s.starts_with(EMBED) {
            s = remove_bad_chars(prop_line, &s, "\"");
        }
        s
    }
}

/// `removeBadChars(propLine, str, bad)`.
fn remove_bad_chars(prop_line: &str, s: &str, bad: &str) -> String {
    let Some(mut index) = s.find(bad) else { return s.to_string() };
    let mut ignore = index != 0 && s[..index].ends_with('\\');
    let mut buf = String::with_capacity(s.len());
    let mut last_index = 0;
    let mut error_message = false;
    loop {
        buf.push_str(&s[last_index..index]);
        if ignore {
            buf.push_str(bad);
        } else if !error_message {
            error_message = true;
            toolkit::log(CompilerMessage::warning(format!("'{bad}' can not be contained in a property and was removed from '{prop_line}'")), None, -1, -1, None);
        }
        last_index = index + bad.len();
        match s[last_index..].find(bad) {
            Some(i) => {
                index = last_index + i;
                ignore = index != 0 && s[..index].ends_with('\\');
            }
            None => break,
        }
    }
    buf.push_str(&s[last_index..]);
    buf
}

impl PropertyText {
    pub fn new(compatibility_version: i32, per_compile_data: Rc<RefCell<ContextStatics>>, source: SourceRef, locale: &str) -> PropertyText {
        let mut imports = JHashSet::new();
        imports.insert("mx.resources.ResourceBundle".to_string());
        PropertyText {
            props: OrderedProperties::default(),
            flex2_compatible: compatibility_version < crate::asc::flex2::compiler::css::document::VERSION_3_0,
            per_compile_data,
            source,
            locale: locale.to_string(),
            imports,
            at_embeds: Vec::new(),
        }
    }

    /// `load(reader)`.
    pub fn load(&mut self, text: &str) {
        let hooks = Hooks { flex2_compatible: self.flex2_compatible };
        self.props.load(text, &hooks, &PropertyValue::Str);
        let keys: Vec<String> = self.props.map.iter().map(|(k, _)| k.to_string()).collect();
        for key in keys {
            let Some(PropertyValue::Str(value)) = self.props.get(&key).cloned() else { continue };
            let value_object = if value.starts_with(CLASS_REFERENCE) {
                self.process_class_reference(&key, &value).map(PropertyValue::ClassReference)
            } else if value.starts_with(EMBED) {
                self.process_embed(&key, &value).map(PropertyValue::Embed)
            } else {
                Some(PropertyValue::Str(value))
            };
            if let Some(v) = value_object {
                self.props.put(&key, v);
            }
        }
    }

    fn line(&self, key: &str) -> i32 {
        *self.props.lines.get(key).expect("NullPointerException")
    }

    fn process_class_reference(&mut self, key: &str, value: &str) -> Option<ClassReference> {
        let line_number = self.line(key);
        let parameter = value[CLASS_REFERENCE.len()..value.len() - 1].trim_matches(|c: char| c <= ' ');
        let quoted = parameter.starts_with('"') && parameter[1..].find('"').map(|i| i + 1) == Some(parameter.len() - 1);
        if quoted {
            let p = parameter[1..parameter.len() - 1].to_string();
            self.imports.insert(p.clone());
            Some(ClassReference::new(Some(p), line_number))
        } else if parameter == "null" {
            Some(ClassReference::new(None, line_number))
        } else {
            let path = self.get_property_file_name();
            toolkit::log(CompilerMessage::error("Invalid ClassReference directive in resource bundle.  It must contain a single String parameter."), Some(&path), line_number, -1, None);
            None
        }
    }

    fn process_embed(&mut self, key: &str, value: &str) -> Option<Rc<AtEmbed>> {
        let line_number = self.line(key);
        let properties_path = self.get_property_file_name();
        let at_embed = AtEmbed::create(&self.per_compile_data, &self.source, value, &properties_path, line_number, "_embed_properties_");
        if let Some(e) = &at_embed {
            let name = e.get_prop_name().to_string();
            match self.at_embeds.iter_mut().find(|(k, _)| *k == name) {
                Some(slot) => slot.1 = e.clone(),
                None => self.at_embeds.push((name, e.clone())),
            }
        }
        at_embed
    }

    fn get_property_file_name(&self) -> String {
        match resource_file_of(&self.source) {
            Some(f) => {
                f.set_locale(&self.locale);
                f.get_name_for_reporting()
            }
            None => self.source.borrow().get_name_for_reporting().to_string(),
        }
    }
}
