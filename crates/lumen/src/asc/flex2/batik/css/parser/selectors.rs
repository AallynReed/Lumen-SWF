//! Ports of the Batik parser's selectors (`AbstractSelector` with Flex's line number, `DefaultElementSelector`,
//! `DefaultPseudoElementSelector`, `DefaultConditionalSelector`, `DefaultDescendantSelector`, `DefaultChildSelector`,
//! `DefaultDirectAdjacentSelector`) and `DefaultSelectorFactory`.

use std::cell::Cell;
use std::rc::Rc;

use super::conditions::ConditionRef;
use crate::asc::flex2::batik::css::sac::selector::*;
use crate::asc::flex2::batik::css::sac::{CSSException, CssError, CssResult};

pub type SelectorRef = Rc<Selector>;

#[derive(Debug)]
pub enum Kind {
    Element { namespace_uri: Option<String>, local_name: Option<String> },
    PseudoElement { namespace_uri: Option<String>, local_name: Option<String> },
    Conditional { simple_selector: SelectorRef, condition: ConditionRef },
    Descendant { ancestor_selector: SelectorRef, simple_selector: SelectorRef },
    Child { ancestor_selector: SelectorRef, simple_selector: SelectorRef },
    DirectAdjacent { node_type: i16, selector: SelectorRef, simple_selector: SelectorRef },
}

#[derive(Debug)]
pub struct Selector {
    line_number: Cell<i32>,
    pub kind: Kind,
}

impl Selector {
    fn new(kind: Kind) -> SelectorRef {
        Rc::new(Selector { line_number: Cell::new(0), kind })
    }

    pub fn get_line_number(&self) -> i32 {
        self.line_number.get()
    }

    pub fn set_line_number(&self, line_number: i32) {
        self.line_number.set(line_number)
    }

    pub fn get_selector_type(&self) -> i16 {
        match self.kind {
            Kind::Element { .. } => SAC_ELEMENT_NODE_SELECTOR,
            Kind::PseudoElement { .. } => SAC_PSEUDO_ELEMENT_SELECTOR,
            Kind::Conditional { .. } => SAC_CONDITIONAL_SELECTOR,
            Kind::Descendant { .. } => SAC_DESCENDANT_SELECTOR,
            Kind::Child { .. } => SAC_CHILD_SELECTOR,
            Kind::DirectAdjacent { .. } => SAC_DIRECT_ADJACENT_SELECTOR,
        }
    }

    /// `instanceof ElementSelector`.
    pub fn is_element_selector(&self) -> bool {
        matches!(self.kind, Kind::Element { .. } | Kind::PseudoElement { .. })
    }

    /// `instanceof DescendantSelector`.
    pub fn is_descendant_selector(&self) -> bool {
        matches!(self.kind, Kind::Descendant { .. } | Kind::Child { .. })
    }

    pub fn get_local_name(&self) -> Option<&str> {
        match &self.kind {
            Kind::Element { local_name, .. } | Kind::PseudoElement { local_name, .. } => local_name.as_deref(),
            _ => None,
        }
    }

    pub fn get_namespace_uri(&self) -> Option<&str> {
        match &self.kind {
            Kind::Element { namespace_uri, .. } | Kind::PseudoElement { namespace_uri, .. } => namespace_uri.as_deref(),
            _ => None,
        }
    }

    pub fn get_simple_selector(&self) -> Option<&SelectorRef> {
        match &self.kind {
            Kind::Conditional { simple_selector, .. } | Kind::Descendant { simple_selector, .. } | Kind::Child { simple_selector, .. } => Some(simple_selector),
            _ => None,
        }
    }

    pub fn get_ancestor_selector(&self) -> Option<&SelectorRef> {
        match &self.kind {
            Kind::Descendant { ancestor_selector, .. } | Kind::Child { ancestor_selector, .. } => Some(ancestor_selector),
            _ => None,
        }
    }

    pub fn get_condition(&self) -> Option<&ConditionRef> {
        match &self.kind {
            Kind::Conditional { condition, .. } => Some(condition),
            _ => None,
        }
    }
}

impl std::fmt::Display for Selector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            Kind::Element { local_name, .. } => write!(f, "{}", local_name.as_deref().unwrap_or("*")),
            Kind::PseudoElement { local_name, .. } => write!(f, ":{}", local_name.as_deref().unwrap_or("null")),
            Kind::Conditional { simple_selector, condition } => write!(f, "{simple_selector}{condition}"),
            Kind::Descendant { ancestor_selector, simple_selector } => write!(f, "{ancestor_selector} {simple_selector}"),
            Kind::Child { ancestor_selector, simple_selector } => {
                if simple_selector.get_selector_type() == SAC_PSEUDO_ELEMENT_SELECTOR {
                    write!(f, "{ancestor_selector}{simple_selector}")
                } else {
                    write!(f, "{ancestor_selector} > {simple_selector}")
                }
            }
            Kind::DirectAdjacent { selector, simple_selector, .. } => write!(f, "{selector} + {simple_selector}"),
        }
    }
}

fn not_implemented<T>() -> CssResult<T> {
    Err(CssError::Css(CSSException::new("Not implemented in CSS2")))
}

/// `DefaultSelectorFactory`.
pub struct DefaultSelectorFactory;

impl DefaultSelectorFactory {
    pub fn create_conditional_selector(&self, selector: SelectorRef, condition: ConditionRef) -> CssResult<SelectorRef> {
        Ok(Selector::new(Kind::Conditional { simple_selector: selector, condition }))
    }

    pub fn create_any_node_selector(&self) -> CssResult<SelectorRef> {
        not_implemented()
    }

    pub fn create_root_node_selector(&self) -> CssResult<SelectorRef> {
        not_implemented()
    }

    pub fn create_negative_selector(&self, _selector: SelectorRef) -> CssResult<SelectorRef> {
        not_implemented()
    }

    pub fn create_element_selector(&self, namespace_uri: Option<String>, tag_name: Option<String>) -> CssResult<SelectorRef> {
        Ok(Selector::new(Kind::Element { namespace_uri, local_name: tag_name }))
    }

    pub fn create_pseudo_element_selector(&self, namespace_uri: Option<String>, pseudo_name: Option<String>) -> CssResult<SelectorRef> {
        Ok(Selector::new(Kind::PseudoElement { namespace_uri, local_name: pseudo_name }))
    }

    pub fn create_descendant_selector(&self, parent: SelectorRef, descendant: SelectorRef) -> CssResult<SelectorRef> {
        Ok(Selector::new(Kind::Descendant { ancestor_selector: parent, simple_selector: descendant }))
    }

    pub fn create_child_selector(&self, parent: SelectorRef, child: SelectorRef) -> CssResult<SelectorRef> {
        Ok(Selector::new(Kind::Child { ancestor_selector: parent, simple_selector: child }))
    }

    pub fn create_direct_adjacent_selector(&self, node_type: i16, child: SelectorRef, direct_adjacent: SelectorRef) -> CssResult<SelectorRef> {
        Ok(Selector::new(Kind::DirectAdjacent { node_type, selector: child, simple_selector: direct_adjacent }))
    }
}
