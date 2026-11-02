//! Port of `flex2.compiler.mxml.gen.CodeFragmentList`: generated code fragments, each mapped to an MXML line.

use crate::asc::flex2::compiler::mxml::rep::script::Script;

#[derive(Clone, Default)]
pub struct CodeFragmentList {
    pub list: Vec<Script>,
}

impl CodeFragmentList {
    pub fn new() -> CodeFragmentList {
        CodeFragmentList { list: Vec::new() }
    }

    pub fn with(fragment: &str, line: i32) -> CodeFragmentList {
        let mut l = CodeFragmentList::new();
        l.add(fragment, line);
        l
    }

    pub fn add(&mut self, fragment: &str, line: i32) -> bool {
        self.list.push(Script::new(fragment, line));
        true
    }

    pub fn add2(&mut self, s1: &str, s2: &str, line: i32) -> bool {
        self.add(&format!("{s1}{s2}"), line)
    }

    pub fn add3(&mut self, s1: &str, s2: &str, s3: &str, line: i32) -> bool {
        self.add(&format!("{s1}{s2}{s3}"), line)
    }

    pub fn add4(&mut self, s1: &str, s2: &str, s3: &str, s4: &str, line: i32) -> bool {
        self.add(&format!("{s1}{s2}{s3}{s4}"), line)
    }

    pub fn add5(&mut self, s1: &str, s2: &str, s3: &str, s4: &str, s5: &str, line: i32) -> bool {
        self.add(&format!("{s1}{s2}{s3}{s4}{s5}"), line)
    }

    /// `add(index, fragment, line)`.
    pub fn add_at(&mut self, index: usize, fragment: &str, line: i32) {
        self.list.insert(index, Script::new(fragment, line));
    }

    pub fn size(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn add_all(&mut self, other: &CodeFragmentList) {
        self.list.extend(other.list.iter().cloned());
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Script> {
        self.list.iter()
    }
}
