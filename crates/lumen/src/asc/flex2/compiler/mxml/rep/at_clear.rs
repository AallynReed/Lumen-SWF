//! Port of `flex2.compiler.mxml.rep.AtClear`.

use std::cell::Cell;

#[derive(Debug)]
pub struct AtClear {
    line_number: Cell<i32>,
}

impl AtClear {
    /// `new AtClear(source, lineNumber)`; the source is unused.
    pub fn new(line_number: i32) -> AtClear {
        AtClear { line_number: Cell::new(line_number) }
    }

    pub fn get_xml_line_number(&self) -> i32 {
        self.line_number.get()
    }

    pub fn set_xml_line_number(&self, xml_line_number: i32) {
        self.line_number.set(xml_line_number);
    }
}
