//! Text writers for decompiled code, without highlighting.

const NEW_LINE: &str = "\r\n";
const INDENT: &str = "   ";
const MAX_LINE: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopType {
    Loop,
    Switch,
}

#[derive(Debug, Clone, Copy)]
pub struct LoopWithType {
    pub id: i64,
    pub ty: LoopType,
    pub used: bool,
}

/// A text writer. In "nul" mode it only measures and tracks loop labels, like `NulWriter`.
pub struct Writer {
    nul: bool,
    /// Count line lengths as the file export does: each append once, indentation not at all.
    stream: bool,
    sb: String,
    new_line: bool,
    indent: i32,
    line_length: usize,
    length: usize,
    loops: Vec<LoopWithType>,
    string_added: bool,
    added_stack: Vec<bool>,
}

impl Writer {
    pub fn new() -> Writer {
        Writer { nul: false, stream: false, sb: String::new(), new_line: true, indent: 0, line_length: 0, length: 0, loops: Vec::new(), string_added: false, added_stack: Vec::new() }
    }

    pub fn nul() -> Writer {
        Writer { nul: true, ..Writer::new() }
    }

    pub fn stream() -> Writer {
        Writer { stream: true, ..Writer::new() }
    }

    pub fn is_nul(&self) -> bool {
        self.nul
    }

    pub fn finish(self) -> String {
        self.sb
    }

    fn put(&mut self, s: &str) {
        if self.new_line {
            self.new_line = false;
            for _ in 0..self.indent.max(0) {
                if !self.stream {
                    self.line_length += INDENT.len();
                }
                self.sb.push_str(INDENT);
            }
        }
        self.sb.push_str(s);
    }

    pub fn append(&mut self, s: &str) -> &mut Self {
        if self.nul {
            self.string_added = true;
            self.length += s.encode_utf16().count();
            self.line_length += s.encode_utf16().count();
            return self;
        }
        self.line_length += if self.stream { 1 } else { 2 } * s.encode_utf16().count();
        self.put(s);
        self
    }

    pub fn append_int(&mut self, v: i64) -> &mut Self {
        self.append(&v.to_string())
    }

    pub fn append_no_hilight(&mut self, s: &str) -> &mut Self {
        if self.nul {
            self.string_added = true;
            self.length += s.encode_utf16().count();
            self.line_length += s.encode_utf16().count();
            return self;
        }
        self.line_length += s.encode_utf16().count();
        self.put(s);
        self
    }

    /// `hilightSpecial`: plain text without the highlight data.
    pub fn special(&mut self, s: &str) -> &mut Self {
        self.append_no_hilight(s)
    }

    pub fn indent(&mut self) -> &mut Self {
        if !self.nul {
            self.indent += 1;
        }
        self
    }

    pub fn unindent(&mut self) -> &mut Self {
        if !self.nul {
            self.indent -= 1;
        }
        self
    }

    pub fn new_line(&mut self) -> &mut Self {
        self.line_length = 0;
        if self.nul {
            return self;
        }
        self.put(NEW_LINE);
        self.new_line = true;
        self
    }

    pub fn len(&self) -> usize {
        if self.nul { self.length } else { self.sb.len() }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn start_block(&mut self) -> &mut Self {
        self.new_line();
        self.append("{").new_line().indent()
    }

    pub fn start_block_no_new_line(&mut self) -> &mut Self {
        self.new_line();
        self.append("{")
    }

    pub fn end_block(&mut self) -> &mut Self {
        self.unindent().append("}")
    }

    pub fn space(&mut self) -> &mut Self {
        self.append(" ")
    }

    /// Takes back the spaces written after the last token on this line.
    pub fn trim_space(&mut self) -> &mut Self {
        if self.nul || self.new_line {
            return self;
        }
        let t = self.sb.trim_end_matches(' ').len();
        if t > 0 && !self.sb[..t].ends_with('\n') {
            self.line_length = self.line_length.saturating_sub(self.sb.len() - t);
            self.sb.truncate(t);
        }
        self
    }

    pub fn allow_wrap_here(&mut self) -> &mut Self {
        if self.line_length > MAX_LINE {
            self.new_line();
        }
        self
    }

    /// Spacing before call parentheses is off in the default formatting.
    pub fn space_before_call_parenthesis(&mut self, _args: usize) -> &mut Self {
        self
    }

    pub fn start_loop(&mut self, id: i64, ty: LoopType) {
        if self.nul {
            self.loops.push(LoopWithType { id, ty, used: false });
        }
    }

    pub fn end_loop(&mut self, id: i64) -> Option<LoopWithType> {
        if !self.nul {
            return None;
        }
        let l = self.loops.pop().expect("loop stack");
        assert_eq!(l.id, id, "LoopId mismatch");
        Some(l)
    }

    pub fn current_loop(&self) -> i64 {
        self.loops.last().map_or(-1, |l| l.id)
    }

    pub fn non_switch_loop(&self) -> i64 {
        match self.loops.iter().rev().find(|l| l.ty != LoopType::Switch) {
            Some(l) => l.id,
            None => -1,
        }
    }

    pub fn set_loop_used(&mut self, id: i64) {
        if let Some(l) = self.loops.iter_mut().rev().find(|l| l.id == id) {
            l.used = true;
        }
    }

    pub fn mark(&mut self) {
        self.added_stack.push(self.string_added);
        self.string_added = false;
    }

    pub fn get_mark(&mut self) -> bool {
        let r = self.string_added;
        self.string_added = self.added_stack.pop().unwrap_or(false) || r;
        r
    }
}

impl Default for Writer {
    fn default() -> Self {
        Writer::new()
    }
}
