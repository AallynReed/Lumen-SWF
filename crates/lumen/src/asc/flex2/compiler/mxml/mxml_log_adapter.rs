//! Port of `flex2.compiler.mxml.MxmlLogAdapter`: the logger adapter that maps generated-code positions (and renamed
//! variables) back to the MXML document through its line number maps.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::gen::line_number_map::{DualModeLineNumberMap, LineNumberMap};
use crate::asc::flex2::compiler::toolkit::{self, LogAdapter, LogCall};
use crate::asc::util::jhash::JHashMap;

/// A `LineNumberMap` the adapter consults: the document's `DualModeLineNumberMap`, or a source fragment's map.
#[derive(Clone)]
pub enum LineMap {
    Dual(Rc<DualModeLineNumberMap>),
    Plain(Rc<RefCell<LineNumberMap>>),
}

impl LineMap {
    fn get(&self, line: i32) -> i32 {
        match self {
            LineMap::Dual(m) => m.get(line),
            LineMap::Plain(m) => m.borrow().get(line),
        }
    }

    fn get_old_name(&self) -> String {
        match self {
            LineMap::Dual(m) => m.get_old_name().unwrap_or_else(|| "null".into()),
            LineMap::Plain(m) => m.borrow().get_old_name().unwrap_or("null").to_string(),
        }
    }

    fn get_new_name(&self) -> Option<String> {
        match self {
            LineMap::Dual(m) => m.get_new_name(),
            LineMap::Plain(m) => m.borrow().get_new_name().map(str::to_string),
        }
    }
}

pub struct MxmlLogAdapter {
    original: Option<Rc<dyn LogAdapter>>,
    map: Option<LineMap>,
    extras: RefCell<Option<Vec<LineMap>>>,
    renamed_variable_map: RefCell<Option<JHashMap<String, String>>>,
    /// Messages already reported at an MXML line, as binding expressions can report the same error twice.
    messages: RefCell<Option<HashMap<String, String>>>,
}

impl MxmlLogAdapter {
    /// `new MxmlLogAdapter(original, map)`; `original` is `ThreadLocalToolkit.getLogger()` (`toolkit::get_log_adapter()`).
    pub fn new(original: Option<Rc<dyn LogAdapter>>, map: Option<LineMap>) -> MxmlLogAdapter {
        MxmlLogAdapter { original, map, extras: RefCell::new(None), renamed_variable_map: RefCell::new(None), messages: RefCell::new(None) }
    }

    pub fn set_renamed_variable_map(&self, renamed_variable_map: Option<JHashMap<String, String>>) {
        *self.renamed_variable_map.borrow_mut() = renamed_variable_map;
    }

    pub fn add_line_number_map(&self, map: LineMap) {
        self.extras.borrow_mut().get_or_insert_with(|| Vec::with_capacity(5)).push(map);
    }

    pub fn add_line_number_maps(&self, c: Option<Vec<LineMap>>) {
        if let Some(c) = c {
            self.extras.borrow_mut().get_or_insert_with(|| Vec::with_capacity(5)).extend(c);
        }
    }

    fn match_path(&self, path: &str) -> Option<LineMap> {
        if let Some(m) = &self.map {
            if m.get_new_name().as_deref() == Some(path) {
                return Some(m.clone());
            }
        }
        self.extras.borrow().as_ref()?.iter().find(|m| m.get_new_name().as_deref() == Some(path)).cloned()
    }

    fn is_user_defined(map: &LineMap, line: i32) -> bool {
        map.get(line) > 0
    }

    /// `message.replaceAll("'" + new + "'", "'" + old + "'")` for each renamed variable, in `HashMap` order.
    fn map_renamed_variables(&self, message: String) -> String {
        let mut message = message;
        if let Some(m) = self.renamed_variable_map.borrow().as_ref() {
            for (new_name, old_name) in m.iter() {
                let pattern = format!("'{new_name}'");
                let replacement = format!("'{old_name}'");
                message = match regex::Regex::new(&pattern) {
                    Ok(re) => re.replace_all(&message, regex::NoExpand(&replacement)).into_owned(),
                    Err(_) => message.replace(&pattern, &replacement),
                };
            }
        }
        message
    }

    fn forward(&self, call: LogCall) {
        toolkit::forward_log(&self.original, call);
    }

    /// The once-per-line filter of the `source`-carrying overloads; true when the message is new.
    fn first_time(&self, key: String, msg: &str) -> bool {
        let mut messages = self.messages.borrow_mut();
        let messages = messages.get_or_insert_with(HashMap::new);
        if messages.get(&key).map(String::as_str) != Some(msg) {
            messages.insert(key, msg.to_string());
            true
        } else {
            false
        }
    }
}

impl LogAdapter for MxmlLogAdapter {
    fn log(&self, call: LogCall) {
        match call {
            LogCall::Text { .. } => self.forward(call),
            LogCall::Path { level, path, msg, code } => {
                let path = match self.match_path(&path) {
                    Some(map) => map.get_old_name(),
                    None => path,
                };
                self.forward(LogCall::Path { level, path, msg, code });
            }
            LogCall::PathLine { level, path, line, msg, code } => match self.match_path(&path) {
                Some(map) if Self::is_user_defined(&map, line) => self.forward(LogCall::PathLine { level, path: map.get_old_name(), line: map.get(line), msg, code }),
                _ => self.forward(LogCall::PathLine { level, path, line, msg, code }),
            },
            LogCall::PathLineCol { level, path, line, col, msg } => match self.match_path(&path) {
                Some(map) if Self::is_user_defined(&map, line) => self.forward(LogCall::PathLine { level, path: map.get_old_name(), line: map.get(line), msg, code: None }),
                _ => self.forward(LogCall::PathLineCol { level, path, line, col, msg }),
            },
            LogCall::PathLineColSource { level, path, line, col, msg, source, code } => {
                let msg = if level == toolkit::Level::Warning { self.map_renamed_variables(msg) } else { msg };
                match self.match_path(&path) {
                    Some(map) if Self::is_user_defined(&map, line) => {
                        let (old, mapped) = (map.get_old_name(), map.get(line));
                        if self.first_time(format!("{old}{mapped}"), &msg) {
                            self.forward(LogCall::PathLine { level, path: old, line: mapped, msg, code });
                        }
                    }
                    _ => self.forward(LogCall::PathLineColSource { level, path, line, col, msg, source, code }),
                }
            }
            LogCall::Message { mut m, mut source } => {
                if let Some(map) = m.path.as_deref().and_then(|p| self.match_path(p)) {
                    if Self::is_user_defined(&map, m.line) {
                        m.path = Some(map.get_old_name());
                        m.line = map.get(m.line);
                        m.col = -1;
                        source = None;
                    }
                }
                self.forward(LogCall::Message { m, source });
            }
        }
    }
}

#[cfg(test)]
#[path = "mxml_log_adapter_tests.rs"]
mod tests;
