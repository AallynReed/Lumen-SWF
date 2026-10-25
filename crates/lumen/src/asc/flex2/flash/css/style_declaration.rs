//! Ports of `flash.css.StyleDeclaration`, `flash.css.StyleDeclarationBlock` and `flash.css.StyleProperty`.

use std::cell::RefCell;
use std::rc::Rc;

use super::descriptor::Descriptor;
use super::style_selector::{MediaList, StyleSelector, StyleSelectorRef};
use crate::asc::flex2::batik::css::parser::css_lexical_unit::LexicalUnitRef;
use crate::asc::flex2::compiler::css::at_embed::AtEmbed;
use crate::asc::flex2::compiler::css::reference::Reference;
use crate::asc::util::jhash::JHashMap;

/// The `Object` a `StyleProperty` holds.
#[derive(Clone)]
pub enum PropertyValue {
    Str(String),
    AtEmbed(Rc<AtEmbed>),
    Reference(Rc<Reference>),
}

impl std::fmt::Display for PropertyValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PropertyValue::Str(s) => f.write_str(s),
            PropertyValue::AtEmbed(e) => f.write_str(e.get_prop_name()),
            PropertyValue::Reference(r) => f.write_str(r.get_value()),
        }
    }
}

pub struct StyleProperty {
    name: String,
    value: PropertyValue,
    path: String,
    line_number: i32,
}

impl StyleProperty {
    pub fn new(name: String, value: PropertyValue, path: String, line_number: i32) -> Rc<StyleProperty> {
        Rc::new(StyleProperty { name, value, path, line_number })
    }

    pub fn get_path(&self) -> &str {
        &self.path
    }

    pub fn get_line_number(&self) -> i32 {
        self.line_number
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_value(&self) -> &PropertyValue {
        &self.value
    }
}

impl std::fmt::Display for StyleProperty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{};", self.name, self.value)
    }
}

pub type Properties = JHashMap<String, Rc<StyleProperty>>;

pub type StyleDeclarationBlockRef = Rc<RefCell<StyleDeclarationBlock>>;

pub struct StyleDeclarationBlock {
    media_list: Option<Rc<MediaList>>,
    properties: Properties,
    effect_styles: Vec<String>,
}

impl StyleDeclarationBlock {
    pub fn new(media_list: Option<Rc<MediaList>>) -> StyleDeclarationBlockRef {
        Rc::new(RefCell::new(StyleDeclarationBlock { media_list, properties: JHashMap::new(), effect_styles: Vec::new() }))
    }

    pub fn has_media_list(&self) -> bool {
        self.media_list.is_some()
    }

    pub fn get_media_list(&self) -> Option<&Rc<MediaList>> {
        self.media_list.as_ref()
    }

    pub fn get_properties(&self) -> &Properties {
        &self.properties
    }

    pub fn get_properties_mut(&mut self) -> &mut Properties {
        &mut self.properties
    }

    pub fn set_properties(&mut self, properties: Properties) {
        self.properties = properties;
    }

    pub fn get_effect_styles(&self) -> &[String] {
        &self.effect_styles
    }

    pub fn get_effect_styles_mut(&mut self) -> &mut Vec<String> {
        &mut self.effect_styles
    }

    pub fn add_effect_style(&mut self, property_name: String) {
        self.effect_styles.push(property_name);
    }
}

/// `StyleDeclaration`'s `LinkedHashMap<String, Descriptor>`, shared by shallow copies.
pub type Descriptors = Rc<RefCell<Vec<(String, Rc<Descriptor>)>>>;

pub type StyleDeclarationRef = Rc<RefCell<StyleDeclaration>>;

pub struct StyleDeclaration {
    line_number: i32,
    path: String,
    subject: Option<String>,
    selector: Option<StyleSelectorRef>,
    block_cache: JHashMap<String, StyleDeclarationBlockRef>,
    declaration_blocks: Vec<StyleDeclarationBlockRef>,
    descriptors: Descriptors,
}

impl StyleDeclaration {
    pub fn new(path: String, line_number: i32) -> StyleDeclarationRef {
        Rc::new(RefCell::new(StyleDeclaration {
            line_number,
            path,
            subject: None,
            selector: None,
            block_cache: JHashMap::with_capacity(2),
            declaration_blocks: Vec::with_capacity(2),
            descriptors: Rc::new(RefCell::new(Vec::new())),
        }))
    }

    pub fn get_path(&self) -> &str {
        &self.path
    }

    pub fn get_line_number(&self) -> i32 {
        self.line_number
    }

    pub fn set_line_number(&mut self, line_number: i32) {
        self.line_number = line_number;
    }

    pub fn get_subject(&self) -> Option<&str> {
        self.subject.as_deref()
    }

    pub fn set_subject(&mut self, subject: Option<String>) {
        self.subject = subject;
    }

    pub fn get_selector(&self) -> Option<StyleSelectorRef> {
        self.selector.clone()
    }

    pub fn set_selector(&mut self, selector: Option<StyleSelectorRef>) {
        self.selector = selector;
    }

    pub fn has_properties(&self) -> bool {
        self.declaration_blocks.iter().any(|b| !b.borrow().get_properties().is_empty())
    }

    pub fn has_effect_styles(&self) -> bool {
        self.declaration_blocks.iter().any(|b| !b.borrow().get_effect_styles().is_empty())
    }

    pub fn get_declaration_blocks(&self) -> &[StyleDeclarationBlockRef] {
        &self.declaration_blocks
    }

    pub fn get_declaration_block(&mut self, selector: &StyleSelector, media_list: Option<Rc<MediaList>>) -> StyleDeclarationBlockRef {
        if media_list.is_some() {
            let block = StyleDeclarationBlock::new(media_list);
            self.declaration_blocks.push(block.clone());
            block
        } else {
            let key = Self::get_declaration_block_key(selector, None);
            match self.block_cache.get(&key) {
                Some(b) => b.clone(),
                None => {
                    let block = StyleDeclarationBlock::new(None);
                    self.block_cache.insert(key, block.clone());
                    self.declaration_blocks.push(block.clone());
                    block
                }
            }
        }
    }

    fn get_declaration_block_key(selector: &StyleSelector, media_list: Option<&MediaList>) -> String {
        let mut sb = format!("({})(", selector);
        if let Some(m) = media_list {
            for q in m.get_queries() {
                sb.push_str(q);
            }
        }
        sb.push(')');
        sb
    }

    pub fn get_descriptor_value(&self, name: &str) -> Option<Rc<Descriptor>> {
        self.descriptors.borrow().iter().find(|(k, _)| k == name).map(|(_, d)| d.clone())
    }

    /// `LinkedHashMap.put`: a replaced key keeps its position.
    pub fn set_descriptor(&mut self, property_name: &str, value: &LexicalUnitRef, _priority: Option<&str>) {
        let d = Rc::new(Descriptor::new(property_name.to_string(), value.clone(), self.path.clone()));
        let mut ds = self.descriptors.borrow_mut();
        match ds.iter_mut().find(|(k, _)| k == property_name) {
            Some(e) => e.1 = d,
            None => ds.push((property_name.to_string(), d)),
        }
    }

    pub fn remove_descriptor(&mut self, name: &str) -> Option<Rc<Descriptor>> {
        let mut ds = self.descriptors.borrow_mut();
        let pos = ds.iter().position(|(k, _)| k == name)?;
        Some(ds.remove(pos).1)
    }

    /// `iterator()` over the descriptors, in insertion order.
    pub fn descriptors(&self) -> Vec<(String, Rc<Descriptor>)> {
        self.descriptors.borrow().clone()
    }

    pub fn shallow_copy(&self) -> StyleDeclarationRef {
        let decl = StyleDeclaration::new(self.path.clone(), self.line_number);
        {
            let mut d = decl.borrow_mut();
            d.subject = self.subject.clone();
            d.selector = self.selector.clone();
            d.descriptors = self.descriptors.clone();
        }
        decl
    }
}
