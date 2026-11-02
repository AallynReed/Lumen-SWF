//! The state and logging helpers of `flex2.compiler.mxml.dom.AnalyzerAdapter`; its visiting defaults are the
//! `Analyzer` trait's. Implementors hold `standardDefs` and `mxmlConfiguration` themselves.

use super::node::NodeRef;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage, Level};

pub struct AnalyzerAdapter {
    pub unit: UnitRef,
    pub(crate) current_node: Option<NodeRef>,
}

impl AnalyzerAdapter {
    pub fn new(unit: UnitRef) -> AnalyzerAdapter {
        AnalyzerAdapter { unit, current_node: None }
    }

    pub fn current_node(&self) -> Option<NodeRef> {
        self.current_node.clone()
    }

    fn path(&self) -> String {
        self.unit.borrow().get_source().borrow().get_name_for_reporting().to_string()
    }

    fn current_line(&self) -> i32 {
        self.current_node.as_ref().map_or(0, |n| n.borrow().begin_line)
    }

    /// `getLineNumber()`.
    pub fn get_line_number(&self) -> i32 {
        self.current_line()
    }

    /// `log(node, msg)`.
    pub fn log(&self, node: &NodeRef, msg: CompilerMessage) {
        let line = node.borrow().begin_line;
        let col = msg.col;
        toolkit::log(msg, Some(&self.path()), line, col, None);
    }

    /// `log(node, line, msg)`.
    pub fn log_node_line(&self, node: &NodeRef, line: i32, msg: CompilerMessage) {
        let line = if line == 0 { node.borrow().begin_line } else { line };
        let col = msg.col;
        toolkit::log(msg, Some(&self.path()), line, col, None);
    }

    /// `log(msg)` against the current node.
    pub fn log_current(&self, msg: CompilerMessage) {
        if let Some(n) = self.current_node.clone() {
            self.log(&n, msg);
        }
    }

    /// `log(line, msg)` against the current node.
    pub fn log_line(&self, line: i32, msg: CompilerMessage) {
        if let Some(n) = self.current_node.clone() {
            self.log_node_line(&n, line, msg);
        }
    }

    fn log_text(&self, level: Level, node: &NodeRef, line: i32, message: &str) {
        let line = if line == 0 { node.borrow().begin_line } else { line };
        match level {
            Level::Info => toolkit::log(CompilerMessage { level: Level::Info, text: message.to_string(), path: None, line: -1, col: -1 }, Some(&self.path()), line, -1, None),
            _ => toolkit::log_path_line(level, &self.path(), line, message),
        }
    }

    /// `logInfo(node, line, message)`.
    pub fn log_info(&self, node: &NodeRef, line: i32, message: &str) {
        self.log_text(Level::Info, node, line, message);
    }

    /// `logWarning(node, line, message)`.
    pub fn log_warning(&self, node: &NodeRef, line: i32, message: &str) {
        self.log_text(Level::Warning, node, line, message);
    }

    /// `logError(node, line, message)`.
    pub fn log_error(&self, node: &NodeRef, line: i32, message: &str) {
        self.log_text(Level::Error, node, line, message);
    }

    /// `logError(message)` against the current node.
    pub fn log_error_current(&self, message: &str) {
        if let Some(n) = self.current_node.clone() {
            self.log_error(&n, 0, message);
        }
    }

    /// `logWarning(message)` against the current node.
    pub fn log_warning_current(&self, message: &str) {
        if let Some(n) = self.current_node.clone() {
            self.log_warning(&n, 0, message);
        }
    }
}
