//! Port of `flex2.compiler.mxml.AbstractSyntaxTreeLogAdapter`: the logger used with AST generation. It reports an
//! ASC error or warning at a given path and line only once, and drops the source line text.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::asc::flex2::compiler::toolkit::{self, LogAdapter, LogCall};

pub struct AbstractSyntaxTreeLogAdapter {
    original: Option<Rc<dyn LogAdapter>>,
    messages: RefCell<HashMap<String, String>>,
}

impl AbstractSyntaxTreeLogAdapter {
    pub fn new(original: Option<Rc<dyn LogAdapter>>) -> AbstractSyntaxTreeLogAdapter {
        AbstractSyntaxTreeLogAdapter { original, messages: RefCell::new(HashMap::new()) }
    }

    fn first_time(&self, path: &str, line: i32, msg: &str) -> bool {
        let key = format!("{path}{line}");
        let mut messages = self.messages.borrow_mut();
        if messages.get(&key).map(String::as_str) == Some(msg) {
            return false;
        }
        messages.insert(key, msg.to_string());
        true
    }
}

impl LogAdapter for AbstractSyntaxTreeLogAdapter {
    fn log(&self, call: LogCall) {
        let call = match call {
            LogCall::Message { m, .. } => LogCall::Message { m, source: None },
            LogCall::PathLine { level, path, line, msg, code: Some(code) } => {
                if !self.first_time(&path, line, &msg) {
                    return;
                }
                LogCall::PathLine { level, path, line, msg, code: Some(code) }
            }
            LogCall::PathLineColSource { level, path, line, msg, code, .. } => {
                if !self.first_time(&path, line, &msg) {
                    return;
                }
                LogCall::PathLine { level, path, line, msg, code }
            }
            other => other,
        };
        toolkit::forward_log(&self.original, call);
    }
}

/// Installs `new AbstractSyntaxTreeLogAdapter(original)` until the guard drops.
pub fn set_ast_log_adapter() -> super::mxml_compiler::LogAdapterGuard {
    let original = toolkit::get_log_adapter();
    toolkit::set_log_adapter(Some(Rc::new(AbstractSyntaxTreeLogAdapter::new(original.clone()))));
    super::mxml_compiler::LogAdapterGuard(original)
}
