//! Ports of `flash.css.StyleSelector`, `flash.css.StyleCondition` and `flash.css.MediaList`.

use std::cell::RefCell;
use std::rc::Rc;

pub const CLASS_CONDITION: &str = "class";
pub const ID_CONDITION: &str = "id";
pub const PSEUDO_CONDITION: &str = "pseudo";

#[derive(Clone, Debug)]
pub struct StyleCondition {
    kind: &'static str,
    value: Option<String>,
}

impl StyleCondition {
    pub fn new(kind: &'static str, value: Option<String>) -> StyleCondition {
        StyleCondition { kind, value }
    }

    pub fn get_kind(&self) -> &str {
        self.kind
    }

    pub fn get_value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    pub fn set_value(&mut self, value: Option<String>) {
        self.value = value;
    }
}

impl std::fmt::Display for StyleCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let v = self.value.as_deref().unwrap_or("null");
        match self.kind {
            CLASS_CONDITION => write!(f, ".{v}"),
            ID_CONDITION => write!(f, "#{v}"),
            PSEUDO_CONDITION => write!(f, ":{v}"),
            _ => Ok(()),
        }
    }
}

pub type StyleSelectorRef = Rc<RefCell<StyleSelector>>;

#[derive(Debug, Default)]
pub struct StyleSelector {
    ancestor: Option<StyleSelectorRef>,
    conditions: Option<Vec<StyleCondition>>,
    value: Option<String>,
}

impl StyleSelector {
    pub fn new() -> StyleSelectorRef {
        Rc::new(RefCell::new(StyleSelector::default()))
    }

    pub fn get_ancestor(&self) -> Option<StyleSelectorRef> {
        self.ancestor.clone()
    }

    pub fn set_ancestor(&mut self, ancestor: Option<StyleSelectorRef>) {
        self.ancestor = ancestor;
    }

    pub fn get_conditions(&self) -> Option<&Vec<StyleCondition>> {
        self.conditions.as_ref()
    }

    pub fn add_condition(&mut self, condition: StyleCondition) {
        self.conditions.get_or_insert_with(|| Vec::with_capacity(2)).push(condition);
    }

    pub fn get_value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    pub fn set_value(&mut self, value: Option<String>) {
        self.value = value;
    }
}

impl std::fmt::Display for StyleSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(a) = &self.ancestor {
            write!(f, "{} ", a.borrow())?;
        }
        if let Some(v) = &self.value {
            f.write_str(v)?;
        }
        if let Some(c) = &self.conditions {
            for c in c {
                write!(f, "{c}")?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct MediaList {
    queries: Vec<String>,
}

impl MediaList {
    pub fn new() -> MediaList {
        MediaList { queries: Vec::with_capacity(4) }
    }

    pub fn add_query(&mut self, query: String) {
        self.queries.push(query);
    }

    pub fn get_queries(&self) -> &[String] {
        &self.queries
    }

    pub fn to_escaped_string(&self) -> String {
        self.to_string().replace('"', "\\\"")
    }
}

impl std::fmt::Display for MediaList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.queries.join(", "))
    }
}
