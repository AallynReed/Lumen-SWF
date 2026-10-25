//! Port of `flex2.compiler.css.StyleDef`: the style declarations of one subject (a type, a class, or `global`).

use std::cell::RefCell;
use std::rc::Rc;

use super::at_embed::{AtEmbed, AtEmbedKey};
use super::document::{is_builtin_effect_name, CssConfiguration, CssDocument};
use super::import::Import;
use super::reference::Reference;
use crate::asc::flex2::batik::css::sac::lexical_unit::SAC_IDENT;
use crate::asc::flex2::compiler::source::SourceRef;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::flash::css::descriptor::Descriptor;
use crate::asc::flex2::flash::css::style_declaration::{Properties, PropertyValue, StyleDeclarationRef, StyleProperty};
use crate::asc::flex2::flash::css::style_selector::MediaList;
use crate::asc::util::context::ContextStatics;
use crate::asc::util::jhash::{JHashMap, JHashSet};
use crate::asc::util::jstr;

const CLASS_REFERENCE: &str = "ClassReference(";
const EMBED: &str = "Embed(";
pub const GLOBAL: &str = "global";
pub const UNIVERSAL: &str = "*";
const PROPERTY_REFERENCE: &str = "PropertyReference(";

pub type StyleDefRef = Rc<RefCell<StyleDef>>;

pub struct StyleDef {
    subject: String,
    effect_styles: Vec<String>,
    /// `LinkedHashMap<String, StyleDeclaration>`.
    declarations: Option<Vec<(String, StyleDeclarationRef)>>,
    advanced: bool,
    is_type_selector: bool,
    styles: Properties,
    mxml_document: Option<Rc<dyn CssDocument>>,
    mxml_configuration: Option<Rc<CssConfiguration>>,
    source: SourceRef,
    line_number: i32,
    per_compile_data: Rc<RefCell<ContextStatics>>,
}

impl StyleDef {
    /// The advanced (Flex 4) constructor.
    pub fn new_advanced(subject: String, mxml_document: Option<Rc<dyn CssDocument>>, mxml_configuration: Option<Rc<CssConfiguration>>, source: SourceRef, line_number: i32, per_compile_data: Rc<RefCell<ContextStatics>>) -> StyleDefRef {
        let is_type_selector = subject != GLOBAL && subject != UNIVERSAL;
        Rc::new(RefCell::new(StyleDef {
            subject,
            effect_styles: Vec::new(),
            declarations: None,
            advanced: true,
            is_type_selector,
            styles: JHashMap::new(),
            mxml_document,
            mxml_configuration,
            source,
            line_number,
            per_compile_data,
        }))
    }

    /// The legacy (Flex 3) constructor.
    pub fn new_legacy(name: String, is_type_selector: bool, mxml_document: Option<Rc<dyn CssDocument>>, mxml_configuration: Option<Rc<CssConfiguration>>, source: SourceRef, line_number: i32, per_compile_data: Rc<RefCell<ContextStatics>>) -> StyleDefRef {
        Rc::new(RefCell::new(StyleDef {
            subject: name,
            effect_styles: Vec::new(),
            declarations: None,
            advanced: false,
            is_type_selector,
            styles: JHashMap::new(),
            mxml_document,
            mxml_configuration,
            source,
            line_number,
            per_compile_data,
        }))
    }

    pub fn add_advanced_declaration(&mut self, declaration: &StyleDeclarationRef, media_list: Option<Rc<MediaList>>) {
        let selector = declaration.borrow().get_selector().expect("NullPointerException");
        let selector_string = selector.borrow().to_string();
        let decls = self.declarations.get_or_insert_with(Vec::new);
        let existing_declaration = match decls.iter().find(|(k, _)| *k == selector_string) {
            Some((_, d)) => d.clone(),
            None => {
                decls.push((selector_string, declaration.clone()));
                declaration.clone()
            }
        };
        let is_media = media_list.is_some();
        let block = existing_declaration.borrow_mut().get_declaration_block(&selector.borrow(), media_list);
        {
            let mut b = block.borrow_mut();
            let mut properties = std::mem::take(b.get_properties_mut());
            let mut effects = std::mem::take(b.get_effect_styles_mut());
            self.extract_properties(declaration, &mut properties, &mut effects);
            b.set_properties(properties);
            *b.get_effect_styles_mut() = effects;
        }
        if !is_media {
            let properties: Vec<(String, Rc<StyleProperty>)> = block.borrow().get_properties().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            let existing_blocks = existing_declaration.borrow().get_declaration_blocks().to_vec();
            for existing_block in existing_blocks {
                if !Rc::ptr_eq(&existing_block, &block) {
                    let mut eb = existing_block.borrow_mut();
                    for (property, value) in &properties {
                        if eb.get_properties().get(property).is_some() {
                            eb.get_properties_mut().insert(property.clone(), value.clone());
                        }
                    }
                }
            }
        }
    }

    pub fn add_declaration(&mut self, declaration: &StyleDeclarationRef) {
        let mut styles = std::mem::take(&mut self.styles);
        let mut effects = std::mem::take(&mut self.effect_styles);
        self.extract_properties(declaration, &mut styles, &mut effects);
        self.styles = styles;
        self.effect_styles = effects;
    }

    pub fn is_advanced(&self) -> bool {
        self.advanced
    }

    pub fn set_advanced(&mut self, value: bool) {
        self.advanced = value;
    }

    pub fn get_allow_duplicate_default_style_declarations(&self) -> bool {
        self.mxml_configuration.as_ref().is_some_and(|c| c.allow_duplicate_default_style_declarations)
    }

    fn property_values(&self) -> Vec<Rc<StyleProperty>> {
        let mut out = Vec::new();
        if self.advanced {
            for (_, d) in self.declarations.iter().flatten() {
                for block in d.borrow().get_declaration_blocks() {
                    out.extend(block.borrow().get_properties().values().cloned());
                }
            }
        } else {
            out.extend(self.styles.values().cloned());
        }
        out
    }

    pub fn get_at_embeds(&self) -> JHashSet<AtEmbedKey> {
        let mut result = JHashSet::new();
        for p in self.property_values() {
            if let PropertyValue::AtEmbed(e) = p.get_value() {
                result.insert(AtEmbedKey(e.clone()));
            }
        }
        result
    }

    pub fn get_declarations(&self) -> Option<&Vec<(String, StyleDeclarationRef)>> {
        self.declarations.as_ref()
    }

    pub fn get_effect_styles(&self) -> &[String] {
        &self.effect_styles
    }

    pub fn get_imports(&self) -> JHashSet<Import> {
        let mut result = JHashSet::new();
        for p in self.property_values() {
            if let PropertyValue::Reference(r) = p.get_value() {
                if r.is_class_reference() && r.get_value() != "null" {
                    result.insert(Import::new(r.get_value().to_string(), r.get_line_number()));
                }
            }
        }
        result
    }

    pub fn get_line_number(&self) -> i32 {
        self.line_number
    }

    pub fn get_styles(&self) -> &Properties {
        &self.styles
    }

    pub fn get_subject(&self) -> &str {
        &self.subject
    }

    pub fn get_type_name(&self) -> String {
        if self.subject == UNIVERSAL {
            GLOBAL.to_string()
        } else {
            create_type_name(&self.subject)
        }
    }

    pub fn is_type_selector(&self) -> bool {
        self.is_type_selector
    }

    fn extract_properties(&self, declaration: &StyleDeclarationRef, properties: &mut Properties, effects_styles: &mut Vec<String>) {
        let descriptors = declaration.borrow().descriptors();
        for (_, descriptor) in descriptors {
            let property_name = dehyphenize(descriptor.get_name());
            let r = if property_name == "fontFamily" {
                self.process_font_family(properties, &descriptor);
                Ok(())
            } else {
                self.process_property_descriptor(property_name, &descriptor, properties, effects_styles)
            };
            if let Err(e) = r {
                toolkit::log(e, Some(descriptor.get_path()), descriptor.get_line_number(), -1, None);
            }
        }
    }

    fn process_reference(&self, value: &str, style_sheet_path: &str, line: i32, is_class_reference: bool) -> Option<Rc<Reference>> {
        let prefix_length = if is_class_reference { CLASS_REFERENCE.len() } else { PROPERTY_REFERENCE.len() } as i32;
        let parameter = jstr::substring(value, prefix_length, jstr::utf16_len(value) - 1);
        let parameter = parameter.trim_matches(|c: char| c <= ' ').to_string();
        let plen = jstr::utf16_len(&parameter);
        if jstr::char_at(&parameter, 0) == b'"' as u16 && jstr::index_of_unit(&parameter, b'"' as u16, 1) == plen - 1 {
            let substring = jstr::substring(&parameter, 1, plen - 1);
            if !is_class_reference && self.mxml_document.is_none() {
                toolkit::log(CompilerMessage::error("PropertyReference() can only be used in the context of a document."), Some(style_sheet_path), line, -1, None);
                None
            } else {
                Some(Rc::new(Reference::new(substring, is_class_reference, style_sheet_path.to_string(), line)))
            }
        } else if parameter == "null" {
            Some(Rc::new(Reference::new(parameter, is_class_reference, style_sheet_path.to_string(), line)))
        } else {
            let t = if is_class_reference { "Class" } else { "Property" };
            toolkit::log(CompilerMessage::error(format!("Invalid {t}Reference directive in stylesheet.  It must contain a single String parameter.")), Some(style_sheet_path), line, -1, None);
            None
        }
    }

    fn process_font_family(&self, properties: &mut Properties, descriptor: &Descriptor) {
        let property_name = "fontFamily".to_string();
        let font_family = descriptor.get_ident_as_string();
        let styles_property = StyleProperty::new(property_name.clone(), PropertyValue::Str(format!("\"{font_family}\"")), descriptor.get_path().to_string(), descriptor.get_line_number());
        properties.insert(property_name, styles_property);
    }

    fn process_property_descriptor(&self, property_name: String, descriptor: &Descriptor, properties: &mut Properties, effect_styles: &mut Vec<String>) -> Result<(), CompilerMessage> {
        if property_name.ends_with("Effect") {
            effect_styles.push(property_name.clone());
        }
        if let Some(value) = self.process_property_value(descriptor)? {
            let style_property = StyleProperty::new(property_name.clone(), value, descriptor.get_path().to_string(), descriptor.get_line_number());
            properties.insert(property_name, style_property);
        }
        Ok(())
    }

    fn process_property_value(&self, descriptor: &Descriptor) -> Result<Option<PropertyValue>, CompilerMessage> {
        let value = descriptor.get_value_as_string()?;
        let mut result = Some(PropertyValue::Str(value.clone()));
        if value.starts_with(EMBED) {
            result = AtEmbed::create(&self.per_compile_data, &self.source, &value, descriptor.get_path(), descriptor.get_line_number(), "_embed_css_").map(PropertyValue::AtEmbed);
        } else if value.starts_with(CLASS_REFERENCE) {
            result = self.process_reference(&value, descriptor.get_path(), descriptor.get_line_number(), true).map(PropertyValue::Reference);
        } else if value.starts_with(PROPERTY_REFERENCE) {
            result = self.process_reference(&value, descriptor.get_path(), descriptor.get_line_number(), false).map(PropertyValue::Reference);
        }
        let len = jstr::utf16_len(&value);
        if let (false, Some(doc)) = (self.advanced, &self.mxml_document) {
            let quote = |c: u16| c == b'\'' as u16 || c == b'"' as u16;
            if descriptor.get_value().get_lexical_unit_type() == SAC_IDENT && len > 2 && quote(jstr::char_at(&value, 0)) && quote(jstr::char_at(&value, len - 1)) {
                let potential_property = jstr::substring(&value, 1, len - 1);
                if doc.root_type_has_property(&potential_property) {
                    result = Some(PropertyValue::Str(potential_property.clone()));
                }
                if is_builtin_effect_name(&potential_property) {
                    doc.add_type_ref(&format!("{}.{potential_property}", doc.effects_package()), descriptor.get_line_number());
                }
            }
        }
        Ok(result)
    }
}

pub fn create_type_name(subject: &str) -> String {
    subject.replace('.', "_")
}

/// `StyleDef.dehyphenize`.
pub fn dehyphenize(string: &str) -> String {
    let mut sb = String::new();
    let mut start = 0;
    let mut end = jstr::index_of_unit(string, b'-' as u16, 0);
    while end >= 0 {
        sb.push_str(&jstr::substring(string, start, end));
        let next = jstr::char_at(string, end + 1);
        sb.push_str(&jstr::from_unit(crate::asc::flex2::batik::css::parser::scanner::java_to_upper(next)));
        start = end + 2;
        end = jstr::index_of_unit(string, b'-' as u16, start);
    }
    sb.push_str(&jstr::substring_from(string, start));
    sb
}
