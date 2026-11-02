//! Port of `flex2.compiler.mxml.dom.ScannerError`.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScannerError {
    line: i32,
    col: i32,
    reason: String,
}

impl ScannerError {
    pub(crate) fn new(line: i32, col: i32, reason: &str) -> ScannerError {
        ScannerError { line, col, reason: reason.to_string() }
    }

    pub fn get_line_number(&self) -> i32 {
        self.line
    }

    pub fn get_column_number(&self) -> i32 {
        self.col
    }

    pub fn get_reason(&self) -> &str {
        &self.reason
    }
}
