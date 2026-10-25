//! Port of `flex2.compiler.css.StyleModule`: the style definitions of one style sheet, by subject.

use std::cell::RefCell;
use std::rc::Rc;

use super::at_embed::{AtEmbed, AtEmbedKey, EmbedValue};
use super::import::Import;
use super::style_def::{StyleDef, StyleDefRef, GLOBAL, UNIVERSAL};
use crate::asc::flex2::batik::css::parser::conditions::Condition;
use crate::asc::flex2::batik::css::parser::selectors::{Kind, Selector};
use crate::asc::flex2::batik::css::sac::condition::*;
use crate::asc::flex2::batik::css::sac::selector::*;
use crate::asc::flex2::compiler::source::SourceRef;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::name_mappings::NameMappings;
use crate::asc::flex2::flash::css::rule::{self, RuleRef};
use crate::asc::flex2::flash::css::style_declaration::StyleDeclarationRef;
use crate::asc::flex2::flash::css::style_selector::{MediaList, StyleCondition, StyleSelector, StyleSelectorRef, CLASS_CONDITION, ID_CONDITION, PSEUDO_CONDITION};
use crate::asc::flex2::flash::css::style_sheet::StyleSheet;
use crate::asc::util::context::ContextStatics;
use crate::asc::util::jhash::JHashSet;

/// A `null` subject (`*` without conditions in a legacy sheet), which Java keys its `StyleDef` under.
pub const NULL_KEY: &str = "\u{0}";

pub struct StyleModule {
    /// `LinkedHashMap<String, AtEmbed>`.
    pub(super) at_embeds: Vec<(String, Rc<AtEmbed>)>,
    pub(super) font_face_rules: Vec<RuleRef>,
    pub(super) advanced: bool,
    pub(super) qualified_type_selectors: bool,
    name: Option<String>,
    pub(super) per_compile_data: Rc<RefCell<ContextStatics>>,
    pub(super) source: SourceRef,
    /// `LinkedHashMap<String, StyleDef>`.
    pub(super) style_defs: Vec<(String, StyleDefRef)>,
    pub(super) name_mappings: Option<Rc<NameMappings>>,
}

fn unsupported_selector(selector: &Selector, path: &str, line_number: i32) {
    toolkit::log(CompilerMessage::warning(format!("CSS selector type is not supported: '{selector}'")), Some(path), line_number, -1, None);
}

fn unsupported_condition(condition: &Condition, path: &str, line_number: i32) {
    toolkit::log(CompilerMessage::warning(format!("CSS selector condition type is not supported: '{condition}'")), Some(path), line_number, -1, None);
}

impl StyleModule {
    pub fn new(source: SourceRef, per_compile_data: Rc<RefCell<ContextStatics>>) -> StyleModule {
        StyleModule { at_embeds: Vec::new(), font_face_rules: Vec::new(), advanced: true, qualified_type_selectors: true, name: None, per_compile_data, source, style_defs: Vec::new(), name_mappings: None }
    }

    /// `getAtEmbeds()`, a `HashSet` in Java's order.
    pub fn get_at_embeds(&self) -> JHashSet<AtEmbedKey> {
        let mut result = JHashSet::from_collection(self.at_embeds.iter().map(|(_, e)| AtEmbedKey(e.clone())), self.at_embeds.len());
        for (_, style_def) in &self.style_defs {
            for e in style_def.borrow().get_at_embeds().iter() {
                if !result.contains(e) {
                    result.insert(e.clone());
                }
            }
        }
        result
    }

    pub fn get_font_face_rules(&self) -> &[RuleRef] {
        &self.font_face_rules
    }

    pub fn is_advanced(&self) -> bool {
        self.advanced
    }

    pub fn set_advanced(&mut self, value: bool) {
        self.advanced = value;
    }

    pub fn get_qualified_type_selectors(&self) -> bool {
        self.qualified_type_selectors
    }

    pub fn set_qualified_type_selectors(&mut self, value: bool) {
        self.qualified_type_selectors = value;
    }

    pub fn get_imports(&self) -> JHashSet<Import> {
        let mut result = JHashSet::new();
        for (_, style_def) in &self.style_defs {
            for i in style_def.borrow().get_imports().iter() {
                result.insert(i.clone());
            }
        }
        result
    }

    pub fn get_name_mappings(&self) -> Option<Rc<NameMappings>> {
        self.name_mappings.clone()
    }

    pub fn set_name_mappings(&mut self, mappings: Option<Rc<NameMappings>>) {
        self.name_mappings = mappings;
    }

    pub fn get_name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn set_name(&mut self, name: String) {
        self.name = Some(name);
    }

    pub fn get_source(&self) -> &SourceRef {
        &self.source
    }

    pub fn get_style_defs(&self) -> Vec<StyleDefRef> {
        self.style_defs.iter().map(|(_, d)| d.clone()).collect()
    }

    pub fn get_style_def(&self, name: &str) -> Option<StyleDefRef> {
        self.style_defs.iter().find(|(k, _)| k == name).map(|(_, d)| d.clone())
    }

    pub(super) fn put_style_def(&mut self, key: String, def: StyleDefRef) {
        match self.style_defs.iter_mut().find(|(k, _)| *k == key) {
            Some(e) => e.1 = def,
            None => self.style_defs.push((key, def)),
        }
    }

    pub(super) fn get_path_for_reporting(&self, declaration: &StyleDeclarationRef) -> String {
        declaration.borrow().get_path().to_string()
    }

    pub(super) fn get_selector_line_number(&self, selector: &Selector, declaration: &StyleDeclarationRef) -> i32 {
        let mut line_number = declaration.borrow().get_line_number();
        if selector.get_line_number() > 0 {
            line_number = selector.get_line_number();
        }
        line_number
    }

    fn get_qualified_class_name(&self, uri: Option<&str>, name: &str) -> Option<String> {
        let uri = uri?;
        let class_name = self.name_mappings.as_ref().expect("nameMappings").resolve_class_name(uri, name)?;
        Some(class_name.replace(':', "."))
    }

    fn convert_selector(&self, selector: &Rc<Selector>, declaration: &StyleDeclarationRef) -> Option<StyleSelectorRef> {
        let mut simple_type_selector = None;
        match &selector.kind {
            Kind::Element { .. } | Kind::PseudoElement { .. } => {
                simple_type_selector = self.convert_type_selector(selector, declaration);
            }
            Kind::Conditional { simple_selector, condition } => {
                if simple_selector.is_element_selector() {
                    simple_type_selector = self.convert_type_selector(simple_selector, declaration);
                }
                match &simple_type_selector {
                    Some(s) => {
                        if !convert_condition(s, condition) {
                            let line_number = self.get_selector_line_number(selector, declaration);
                            unsupported_condition(condition, &self.get_path_for_reporting(declaration), line_number);
                            return None;
                        }
                    }
                    None => {
                        let line_number = self.get_selector_line_number(selector, declaration);
                        unsupported_selector(selector, &self.get_path_for_reporting(declaration), line_number);
                    }
                }
            }
            Kind::Descendant { ancestor_selector, simple_selector } | Kind::Child { ancestor_selector, simple_selector } => {
                simple_type_selector = self.convert_selector(simple_selector, declaration);
                if let Some(s) = &simple_type_selector {
                    let ancestor = self.convert_selector(ancestor_selector, declaration);
                    s.borrow_mut().set_ancestor(ancestor);
                }
            }
            Kind::DirectAdjacent { .. } => {
                let line_number = self.get_selector_line_number(selector, declaration);
                unsupported_selector(selector, &self.get_path_for_reporting(declaration), line_number);
            }
        }
        simple_type_selector
    }

    fn convert_type_selector(&self, selector: &Rc<Selector>, declaration: &StyleDeclarationRef) -> Option<StyleSelectorRef> {
        if selector.get_selector_type() != SAC_ELEMENT_NODE_SELECTOR {
            return None;
        }
        let mut name = selector.get_local_name().map(str::to_string);
        let uri = selector.get_namespace_uri();
        if name.is_none() && selector.to_string() == "*" {
            name = Some("*".to_string());
        } else if name.as_deref() != Some(GLOBAL) && self.qualified_type_selectors {
            if uri.is_some() {
                match self.get_qualified_class_name(uri, name.as_deref().unwrap_or("null")) {
                    Some(c) => name = Some(c),
                    None => {
                        let line_number = self.get_selector_line_number(selector, declaration);
                        let n = name.as_deref().unwrap_or("null");
                        toolkit::log(CompilerMessage::warning(format!("Cannot resolve namespace qualified type '{n}' in CSS selector '{selector}'")), Some(&self.get_path_for_reporting(declaration)), line_number, -1, None);
                    }
                }
            } else {
                let line_number = self.get_selector_line_number(selector, declaration);
                let n = name.as_deref().unwrap_or("null");
                toolkit::log(CompilerMessage::warning(format!("Type '{n}' in CSS selector '{selector}' must be qualified with a namespace.")), Some(&self.get_path_for_reporting(declaration)), line_number, -1, None);
            }
        }
        let name = name?;
        let result = StyleSelector::new();
        result.borrow_mut().set_value(Some(name));
        Some(result)
    }
}

fn convert_condition(selector: &StyleSelectorRef, condition: &Condition) -> bool {
    if condition.is_combinator_condition() {
        let mut supported = convert_condition(selector, condition.get_first_condition().unwrap());
        if supported {
            supported = convert_condition(selector, condition.get_second_condition().unwrap());
        }
        supported
    } else if condition.is_attribute_condition() {
        let value = condition.get_value().map(str::to_string);
        let kind = match condition.get_condition_type() {
            SAC_CLASS_CONDITION => Some(CLASS_CONDITION),
            SAC_ID_CONDITION => Some(ID_CONDITION),
            SAC_PSEUDO_CLASS_CONDITION => Some(PSEUDO_CONDITION),
            _ => None,
        };
        match kind {
            Some(k) => {
                selector.borrow_mut().add_condition(StyleCondition::new(k, value));
                true
            }
            None => false,
        }
    } else {
        false
    }
}

/// The overridable part of `StyleModule` (`StylesContainer` replaces these), and the extraction that calls it.
pub trait StyleModuleOps {
    fn module(&self) -> std::cell::Ref<'_, StyleModule>;
    fn module_mut(&self) -> std::cell::RefMut<'_, StyleModule>;

    fn add_selector_to_style_def(&self, subject: &str, declaration: &StyleDeclarationRef, is_type_selector: bool, _is_local: bool, line_number: i32) {
        let existing = self.module().get_style_def(subject);
        let style_def = match existing {
            Some(d) => d,
            None => {
                let m = self.module();
                let d = StyleDef::new_legacy(subject.to_string(), is_type_selector, None, None, m.source.clone(), line_number, m.per_compile_data.clone());
                drop(m);
                self.module_mut().put_style_def(subject.to_string(), d.clone());
                d
            }
        };
        style_def.borrow_mut().add_declaration(declaration);
    }

    fn add_advanced_selector_to_style_def(&self, declaration: &StyleDeclarationRef, media_list: Option<Rc<MediaList>>, _is_local: bool, line_number: i32) {
        let subject = declaration.borrow().get_subject().unwrap_or("null").to_string();
        let selector = declaration.borrow().get_selector().expect("NullPointerException");
        let mut style_def_key = subject.clone();
        if subject == UNIVERSAL {
            style_def_key = GLOBAL.to_string();
            if selector.borrow().get_conditions().is_some_and(|c| !c.is_empty()) {
                selector.borrow_mut().set_value(Some(String::new()));
            }
        }
        let existing = self.module().get_style_def(&style_def_key);
        let style_def = match existing {
            Some(d) => d,
            None => {
                let m = self.module();
                let d = StyleDef::new_advanced(subject, None, None, m.source.clone(), line_number, m.per_compile_data.clone());
                drop(m);
                self.module_mut().put_style_def(style_def_key, d.clone());
                d
            }
        };
        style_def.borrow_mut().add_advanced_declaration(declaration, media_list);
    }

    fn add_at_embed(&self, at_embed: Rc<AtEmbed>) {
        let mut m = self.module_mut();
        let key = at_embed.get_prop_name().to_string();
        match m.at_embeds.iter_mut().find(|(k, _)| *k == key) {
            Some(e) => e.1 = at_embed,
            None => m.at_embeds.push((key, at_embed)),
        }
    }

    /// `extractStyles`; an `Err` is the `RuntimeException` (by message) a `@font-face` rule can throw, which ends it.
    fn extract_styles(&self, style_sheet: Option<&StyleSheet>, is_local: bool) -> Result<(), Option<String>> {
        let Some(style_sheet) = style_sheet else { return Ok(()) };
        let Some(sheet_rules) = style_sheet.get_css_rules() else { return Ok(()) };
        for rule in sheet_rules.clone() {
            let (is_style, is_font_face, is_media) = {
                let r = rule.borrow();
                (r.is_style_rule(), r.font_face().is_some(), r.get_media_list().is_some())
            };
            if is_style {
                self.add_style_rule(&rule, None, is_local);
            } else if is_font_face {
                self.add_font_face_rule(&rule)?;
            } else if is_media && self.module().advanced {
                self.add_media_rule(&rule, is_local);
            }
        }
        Ok(())
    }

    fn add_media_rule(&self, media_rule: &RuleRef, is_local: bool) {
        let (child_rules, media_list) = {
            let r = media_rule.borrow();
            (r.get_rules(), r.get_media_list())
        };
        for rule in child_rules {
            if rule.borrow().is_style_rule() {
                self.add_style_rule(&rule, media_list.clone(), is_local);
            }
        }
    }

    fn add_style_rule(&self, rule: &RuleRef, media_list: Option<Rc<MediaList>>, is_local: bool) {
        let (selectors, base_declaration) = {
            let r = rule.borrow();
            (r.get_selector_list().unwrap(), r.get_style_declaration())
        };
        for i in 0..selectors.get_length() {
            let selector = selectors.item(i).unwrap().clone();
            let declaration = if i > 0 { base_declaration.borrow().shallow_copy() } else { base_declaration.clone() };
            match selector.get_selector_type() {
                SAC_ELEMENT_NODE_SELECTOR => self.add_simple_type_selector(&selector, &declaration, media_list.clone(), is_local),
                SAC_CONDITIONAL_SELECTOR => self.add_conditional_selector(&selector, &declaration, media_list.clone()),
                SAC_DESCENDANT_SELECTOR => self.add_descendant_selector(&selector, &declaration, media_list.clone()),
                _ => {
                    let m = self.module();
                    let line_number = m.get_selector_line_number(&selector, &declaration);
                    let path = m.get_path_for_reporting(&declaration);
                    unsupported_selector(&selector, &path, line_number);
                }
            }
        }
    }

    fn add_conditional_selector(&self, selector: &Rc<Selector>, declaration: &StyleDeclarationRef, media_list: Option<Rc<MediaList>>) {
        let line_number = self.module().get_selector_line_number(selector, declaration);
        if self.module().advanced {
            let base = self.module().convert_selector(selector, declaration);
            if let Some(base) = base {
                let subject = base.borrow().get_value().map(str::to_string);
                declaration.borrow_mut().set_subject(subject);
                declaration.borrow_mut().set_selector(Some(base));
                self.add_advanced_selector_to_style_def(declaration, media_list, false, line_number);
            }
        } else {
            let condition = selector.get_condition().unwrap();
            if condition.get_condition_type() == SAC_CLASS_CONDITION {
                let name = condition.get_value().unwrap_or("null").to_string();
                self.add_selector_to_style_def(&name, declaration, false, false, line_number);
            } else {
                let path = self.module().get_path_for_reporting(declaration);
                unsupported_condition(condition, &path, line_number);
            }
        }
    }

    fn add_descendant_selector(&self, selector: &Rc<Selector>, declaration: &StyleDeclarationRef, media_list: Option<Rc<MediaList>>) {
        let line_number = self.module().get_selector_line_number(selector, declaration);
        if self.module().advanced {
            let base = self.module().convert_selector(selector, declaration);
            if let Some(base) = base {
                let subject = base.borrow().get_value().map(str::to_string);
                declaration.borrow_mut().set_subject(subject);
                declaration.borrow_mut().set_selector(Some(base));
                self.add_advanced_selector_to_style_def(declaration, media_list, false, line_number);
            }
        } else {
            let path = self.module().get_path_for_reporting(declaration);
            unsupported_selector(selector, &path, line_number);
        }
    }

    fn add_simple_type_selector(&self, selector: &Rc<Selector>, declaration: &StyleDeclarationRef, media_list: Option<Rc<MediaList>>, is_local: bool) {
        let name = selector.get_local_name().map(str::to_string);
        if name.is_some() || selector.to_string() == UNIVERSAL {
            let line_number = self.module().get_selector_line_number(selector, declaration);
            if self.module().advanced {
                let base = self.module().convert_selector(selector, declaration);
                if let Some(base) = base {
                    let subject = base.borrow().get_value().map(str::to_string);
                    declaration.borrow_mut().set_selector(Some(base));
                    declaration.borrow_mut().set_subject(subject);
                    self.add_advanced_selector_to_style_def(declaration, media_list, is_local, line_number);
                }
            } else {
                self.add_selector_to_style_def(name.as_deref().unwrap_or(NULL_KEY), declaration, true, is_local, line_number);
            }
        }
    }

    fn add_font_face_rule(&self, font_face_rule: &RuleRef) -> Result<(), Option<String>> {
        let (family, bold, italic) = {
            let mut r = font_face_rule.borrow_mut();
            let family = r.get_family();
            (family, r.is_bold()?, r.is_italic()?)
        };
        let existing = {
            let m = self.module();
            rule::get_rule(&m.font_face_rules, family.as_deref(), bold, italic)?
        };
        if existing.is_none() {
            self.module_mut().font_face_rules.push(font_face_rule.clone());
            let prop_name = format!("_embed__font_{}_{}_{}", family.as_deref().unwrap_or("null"), if bold { "bold" } else { "medium" }, if italic { "italic" } else { "normal" });
            let (mut embed_params, path, line) = {
                let r = font_face_rule.borrow();
                let d = r.get_style_declaration();
                let d = d.borrow();
                (r.get_embed_params().cloned().unwrap_or_default(), d.get_path().to_string(), d.get_line_number())
            };
            if path.contains('\\') {
                embed_params.insert("_file".into(), EmbedValue::Str(path.replace('\\', "/")));
                embed_params.insert("_pathsep".into(), EmbedValue::Str("true".into()));
            } else {
                embed_params.insert("_file".into(), EmbedValue::Str(path.clone()));
            }
            embed_params.insert("_line".into(), EmbedValue::Str(line.to_string()));
            let source = self.module().source.clone();
            let at_embed = AtEmbed::create_named(&prop_name, &source, &path, line, embed_params, false);
            self.add_at_embed(at_embed);
        }
        Ok(())
    }
}

/// A bare `StyleModule`, as `CssCompiler` makes one.
pub struct PlainStyleModule(pub RefCell<StyleModule>);

impl StyleModuleOps for PlainStyleModule {
    fn module(&self) -> std::cell::Ref<'_, StyleModule> {
        self.0.borrow()
    }
    fn module_mut(&self) -> std::cell::RefMut<'_, StyleModule> {
        self.0.borrow_mut()
    }
}
