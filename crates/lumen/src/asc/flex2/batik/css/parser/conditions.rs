//! Ports of the Batik parser's conditions (`DefaultAndCondition`, `DefaultAttributeCondition` and its
//! class/one-of/begin-hyphen subclasses, `DefaultIdCondition`, `DefaultLangCondition`, `DefaultPseudoClassCondition`)
//! and `DefaultConditionFactory`.

use std::rc::Rc;

use crate::asc::flex2::batik::css::sac::condition::*;
use crate::asc::flex2::batik::css::sac::{CSSException, CssError, CssResult};

pub type ConditionRef = Rc<Condition>;

#[derive(Debug)]
pub enum Condition {
    And(ConditionRef, ConditionRef),
    Attribute { local_name: Option<String>, namespace_uri: Option<String>, specified: bool, value: Option<String> },
    OneOfAttribute { local_name: Option<String>, namespace_uri: Option<String>, specified: bool, value: Option<String> },
    BeginHyphenAttribute { local_name: Option<String>, namespace_uri: Option<String>, specified: bool, value: Option<String> },
    Class { namespace_uri: Option<String>, value: Option<String> },
    Id { value: Option<String> },
    Lang { lang: Option<String> },
    PseudoClass { namespace_uri: Option<String>, value: Option<String> },
}

fn s(v: &Option<String>) -> &str {
    v.as_deref().unwrap_or("null")
}

impl Condition {
    pub fn get_condition_type(&self) -> i16 {
        match self {
            Condition::And(..) => SAC_AND_CONDITION,
            Condition::Attribute { .. } => SAC_ATTRIBUTE_CONDITION,
            Condition::OneOfAttribute { .. } => SAC_ONE_OF_ATTRIBUTE_CONDITION,
            Condition::BeginHyphenAttribute { .. } => SAC_BEGIN_HYPHEN_ATTRIBUTE_CONDITION,
            Condition::Class { .. } => SAC_CLASS_CONDITION,
            Condition::Id { .. } => SAC_ID_CONDITION,
            Condition::Lang { .. } => SAC_LANG_CONDITION,
            Condition::PseudoClass { .. } => SAC_PSEUDO_CLASS_CONDITION,
        }
    }

    /// `instanceof CombinatorCondition`.
    pub fn is_combinator_condition(&self) -> bool {
        matches!(self, Condition::And(..))
    }

    /// `instanceof AttributeCondition`.
    pub fn is_attribute_condition(&self) -> bool {
        !matches!(self, Condition::And(..) | Condition::Lang { .. })
    }

    /// `AttributeCondition.getValue()`.
    pub fn get_value(&self) -> Option<&str> {
        match self {
            Condition::Attribute { value, .. }
            | Condition::OneOfAttribute { value, .. }
            | Condition::BeginHyphenAttribute { value, .. }
            | Condition::Class { value, .. }
            | Condition::Id { value }
            | Condition::PseudoClass { value, .. } => value.as_deref(),
            _ => None,
        }
    }

    pub fn get_local_name(&self) -> Option<&str> {
        match self {
            Condition::Attribute { local_name, .. } | Condition::OneOfAttribute { local_name, .. } | Condition::BeginHyphenAttribute { local_name, .. } => local_name.as_deref(),
            Condition::Class { .. } => Some("class"),
            Condition::Id { .. } => Some("id"),
            _ => None,
        }
    }

    pub fn get_first_condition(&self) -> Option<&ConditionRef> {
        match self {
            Condition::And(a, _) => Some(a),
            _ => None,
        }
    }

    pub fn get_second_condition(&self) -> Option<&ConditionRef> {
        match self {
            Condition::And(_, b) => Some(b),
            _ => None,
        }
    }
}

impl std::fmt::Display for Condition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Condition::And(a, b) => write!(f, "{a}{b}"),
            Condition::Attribute { local_name, value, .. } => match value {
                None => write!(f, "[{}]", s(local_name)),
                Some(v) => write!(f, "[{}=\"{v}\"]", s(local_name)),
            },
            Condition::OneOfAttribute { local_name, value, .. } => write!(f, "[{}~=\"{}\"]", s(local_name), s(value)),
            Condition::BeginHyphenAttribute { local_name, value, .. } => write!(f, "[{}|=\"{}\"]", s(local_name), s(value)),
            Condition::Class { value, .. } => write!(f, ".{}", s(value)),
            Condition::Id { value } => write!(f, "#{}", s(value)),
            Condition::Lang { lang } => write!(f, ":lang({})", s(lang)),
            Condition::PseudoClass { value, .. } => write!(f, ":{}", s(value)),
        }
    }
}

fn not_implemented<T>() -> CssResult<T> {
    Err(CssError::Css(CSSException::new("Not implemented in CSS2")))
}

/// `DefaultConditionFactory`.
pub struct DefaultConditionFactory;

impl DefaultConditionFactory {
    pub fn create_and_condition(&self, first: ConditionRef, second: ConditionRef) -> CssResult<ConditionRef> {
        Ok(Rc::new(Condition::And(first, second)))
    }

    pub fn create_or_condition(&self, _first: ConditionRef, _second: ConditionRef) -> CssResult<ConditionRef> {
        not_implemented()
    }

    pub fn create_attribute_condition(&self, local_name: Option<String>, namespace_uri: Option<String>, specified: bool, value: Option<String>) -> CssResult<ConditionRef> {
        Ok(Rc::new(Condition::Attribute { local_name, namespace_uri, specified, value }))
    }

    pub fn create_id_condition(&self, value: Option<String>) -> CssResult<ConditionRef> {
        Ok(Rc::new(Condition::Id { value }))
    }

    pub fn create_lang_condition(&self, lang: Option<String>) -> CssResult<ConditionRef> {
        Ok(Rc::new(Condition::Lang { lang }))
    }

    pub fn create_one_of_attribute_condition(&self, local_name: Option<String>, ns_uri: Option<String>, specified: bool, value: Option<String>) -> CssResult<ConditionRef> {
        Ok(Rc::new(Condition::OneOfAttribute { local_name, namespace_uri: ns_uri, specified, value }))
    }

    pub fn create_begin_hyphen_attribute_condition(&self, local_name: Option<String>, namespace_uri: Option<String>, specified: bool, value: Option<String>) -> CssResult<ConditionRef> {
        Ok(Rc::new(Condition::BeginHyphenAttribute { local_name, namespace_uri, specified, value }))
    }

    pub fn create_class_condition(&self, namespace_uri: Option<String>, value: Option<String>) -> CssResult<ConditionRef> {
        Ok(Rc::new(Condition::Class { namespace_uri, value }))
    }

    pub fn create_pseudo_class_condition(&self, namespace_uri: Option<String>, value: Option<String>) -> CssResult<ConditionRef> {
        Ok(Rc::new(Condition::PseudoClass { namespace_uri, value }))
    }
}
