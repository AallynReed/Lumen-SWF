//! Port of `flash.css.StyleDocumentHandler`: SAC events into `flash.css` rules.

use std::rc::Rc;

use super::rule::{Rule, RuleRef};
use super::style_parser::StyleParser;
use super::style_selector::MediaList;
use crate::asc::flex2::batik::css::parser::css_lexical_unit::LexicalUnitRef;
use crate::asc::flex2::batik::css::parser::css_sac_media_list::CSSSACMediaList;
use crate::asc::flex2::batik::css::parser::css_selector_list::CSSSelectorList;
use crate::asc::flex2::batik::css::parser::selectors::Kind;
use crate::asc::flex2::batik::css::sac::{CssError, CssResult, DocumentHandler};

pub struct StyleDocumentHandler {
    style_parser: Rc<StyleParser>,
    current_media_rule: Option<RuleRef>,
    current_rule: Option<RuleRef>,
}

impl StyleDocumentHandler {
    pub fn new(style_parser: Rc<StyleParser>) -> StyleDocumentHandler {
        StyleDocumentHandler { style_parser, current_media_rule: None, current_rule: None }
    }

    fn push_rule(&mut self, rule: RuleRef) {
        if let Some(cur) = &self.current_rule {
            rule.borrow_mut().set_parent_rule(cur);
        }
        self.current_rule = Some(rule);
    }

    fn pop_rule(&mut self) {
        if let Some(cur) = self.current_rule.take() {
            self.current_rule = cur.borrow().get_parent_rule();
        }
    }
}

impl DocumentHandler for StyleDocumentHandler {
    fn comment(&mut self, _text: &str) -> CssResult<()> {
        Ok(())
    }

    fn end_document(&mut self) -> CssResult<()> {
        Ok(())
    }

    fn end_font_face(&mut self) -> CssResult<()> {
        if let Some(cur) = self.current_rule.clone() {
            let is_font_face = cur.borrow().font_face().is_some();
            if is_font_face {
                cur.borrow_mut().initialize(&self.style_parser).map_err(CssError::Runtime)?;
            }
            self.current_rule = cur.borrow().get_parent_rule();
        }
        Ok(())
    }

    fn end_media(&mut self, _media: &CSSSACMediaList) -> CssResult<()> {
        self.current_media_rule = None;
        self.pop_rule();
        Ok(())
    }

    fn end_page(&mut self, _name: Option<&str>, _pseudo_page: Option<&str>) -> CssResult<()> {
        Ok(())
    }

    fn end_selector(&mut self, _selectors: &CSSSelectorList) -> CssResult<()> {
        self.pop_rule();
        Ok(())
    }

    fn ignorable_at_rule(&mut self, _at_rule: &str) -> CssResult<()> {
        Ok(())
    }

    fn import_style(&mut self, _uri: Option<&str>, _media: &CSSSACMediaList, _ns: Option<&str>) -> CssResult<()> {
        Ok(())
    }

    fn namespace_declaration(&mut self, _prefix: Option<&str>, _uri: Option<&str>) -> CssResult<()> {
        Ok(())
    }

    fn property(&mut self, name: &str, value: &LexicalUnitRef, important: bool) -> CssResult<()> {
        let Some(cur) = &self.current_rule else {
            return Err(CssError::Runtime(Some("Cannot invoke \"flash.css.Rule.getStyleDeclaration()\" because \"this.currentRule\" is null".to_string())));
        };
        cur.borrow().get_style_declaration().borrow_mut().set_descriptor(name, value, if important { Some("important") } else { None });
        if name == "flashType" || name == "flash-type" {
            let line_number = value.get_line_number();
            self.style_parser.warn_deprecation(name, if name == "flashType" { "advancedAntiAliasing" } else { "advanced-anti-aliasing" }, line_number);
        }
        Ok(())
    }

    fn start_document(&mut self) -> CssResult<()> {
        Ok(())
    }

    fn start_font_face(&mut self) -> CssResult<()> {
        let rule = Rule::new_font_face_rule(self.style_parser.get_path(), self.style_parser.get_line_number());
        self.push_rule(rule.clone());
        self.style_parser.add_rule(rule);
        Ok(())
    }

    fn start_media(&mut self, media: &CSSSACMediaList) -> CssResult<()> {
        let mut media_list = MediaList::new();
        for i in 0..media.get_length() {
            media_list.add_query(media.item(i).unwrap_or_else(|| "null".to_string()));
        }
        let rule = Rule::new_media_rule(media_list, self.style_parser.get_path(), self.style_parser.get_line_number());
        self.current_media_rule = Some(rule.clone());
        self.push_rule(rule.clone());
        self.style_parser.add_rule(rule);
        Ok(())
    }

    fn start_page(&mut self, _name: Option<&str>, _pseudo_page: Option<&str>) -> CssResult<()> {
        Ok(())
    }

    fn start_selector(&mut self, selectors: &CSSSelectorList) -> CssResult<()> {
        let mut line_number = self.style_parser.get_line_number();
        if let Some(selector) = selectors.item(0) {
            if matches!(selector.kind, Kind::Conditional { .. } | Kind::Element { .. } | Kind::Descendant { .. }) {
                line_number = selector.get_line_number();
            }
        }
        let list = Rc::new(selectors.clone());
        let rule = Rule::new_style_rule(list, self.style_parser.get_path(), line_number);
        self.push_rule(rule.clone());
        match &self.current_media_rule {
            Some(m) => m.borrow_mut().add_rule(&rule),
            None => self.style_parser.add_rule(rule),
        }
        Ok(())
    }
}
