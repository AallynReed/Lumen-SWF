//! Port of `flex2.compiler.css.Styles` and `StyleConflictException`: the `[Style]` declarations of a unit or of the
//! whole compilation.

use std::rc::{Rc, Weak};
use std::cell::RefCell;

use crate::asc::flex2::compiler::source::Source;
use crate::asc::semantics::meta_data::MetaData;
use crate::asc::util::jhash::JHashMap;

/// `StyleConflictException`: "Declaration of style '${style}' conflicts with previous declaration in ${source}."
#[derive(Clone, Debug)]
pub struct StyleConflictException {
    pub style: String,
    pub source: String,
}

impl StyleConflictException {
    pub fn get_localized_message(&self) -> String {
        format!("Declaration of style '{}' conflicts with previous declaration in {}.", self.style, self.source)
    }
}

#[derive(Clone)]
pub struct Styles {
    decl_map: JHashMap<String, MetaData>,
    location_map: JHashMap<String, Weak<RefCell<Source>>>,
}

impl Default for Styles {
    fn default() -> Self {
        Styles::new(16)
    }
}

fn is_inherit(md: &MetaData) -> bool {
    md.get_value_key("inherit").as_deref() == Some("yes")
}

impl Styles {
    pub fn new(preferred_size: usize) -> Styles {
        Styles { decl_map: JHashMap::with_capacity(preferred_size), location_map: JHashMap::with_capacity(preferred_size) }
    }

    pub fn size(&self) -> usize {
        self.decl_map.len()
    }

    pub fn add_style(&mut self, name: &str, md: MetaData, source: Option<&Rc<RefCell<Source>>>) -> Result<(), StyleConflictException> {
        let conflict = if is_inherit(&md) { self.is_non_inheriting_style(name) } else { self.is_inheriting_style(name) };
        if conflict {
            let source = self.get_location(name).map(|s| s.borrow().get_name_for_reporting().to_string()).unwrap_or_default();
            return Err(StyleConflictException { style: name.to_string(), source });
        }
        self.decl_map.insert(name.to_string(), md);
        self.location_map.insert(name.to_string(), source.map(Rc::downgrade).unwrap_or_default());
        Ok(())
    }

    pub fn add_styles(&mut self, styles: &Styles) -> Result<(), StyleConflictException> {
        for (name, md) in styles.decl_map.iter() {
            let loc = styles.get_location(name);
            self.add_style(name, md.clone(), loc.as_ref())?;
        }
        Ok(())
    }

    pub fn get_location(&self, name: &str) -> Option<Rc<RefCell<Source>>> {
        self.location_map.get(name).and_then(|w| w.upgrade())
    }

    pub fn get_style(&self, name: &str) -> Option<&MetaData> {
        self.decl_map.get(name)
    }

    pub fn is_inheriting_style(&self, name: &str) -> bool {
        self.get_style(name).is_some_and(is_inherit)
    }

    pub fn is_non_inheriting_style(&self, name: &str) -> bool {
        self.get_style(name).is_some_and(|md| !is_inherit(md))
    }

    pub fn get_style_names(&self) -> Vec<String> {
        self.decl_map.keys().cloned().collect()
    }

    /// `getInheritingStyles()`, a `HashSet` in Java's order.
    pub fn get_inheriting_styles(&self) -> crate::asc::util::jhash::JHashSet<String> {
        let mut result = crate::asc::util::jhash::JHashSet::new();
        for name in self.decl_map.keys() {
            if self.is_inheriting_style(name) {
                result.insert(name.clone());
            }
        }
        result
    }

    pub fn clear(&mut self) {
        self.decl_map.clear();
    }
}
