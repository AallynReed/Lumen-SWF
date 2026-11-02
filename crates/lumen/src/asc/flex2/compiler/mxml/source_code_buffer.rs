//! Port of `flex2.compiler.mxml.SourceCodeBuffer`: a `StringWriter` that counts the lines written to it.

use std::fmt;

pub struct SourceCodeBuffer {
    buf: String,
    current_line: i32,
}

impl Default for SourceCodeBuffer {
    fn default() -> Self {
        SourceCodeBuffer::new()
    }
}

impl SourceCodeBuffer {
    pub fn new() -> SourceCodeBuffer {
        SourceCodeBuffer::with_capacity(1024)
    }

    pub fn with_capacity(initial_size: usize) -> SourceCodeBuffer {
        SourceCodeBuffer { buf: String::with_capacity(initial_size), current_line: 1 }
    }

    /// `write(int c)`.
    pub fn write_char(&mut self, c: char) {
        self.buf.push(c);
        if c == '\n' {
            self.current_line += 1;
        }
    }

    /// `write(String)` and `write(char[], 0, len)`.
    pub fn write_str(&mut self, s: &str) {
        self.buf.push_str(s);
        self.current_line += s.bytes().filter(|&b| b == b'\n').count() as i32;
    }

    /// `write(String str, int off, int len)`, in UTF-16 units.
    pub fn write_str_range(&mut self, s: &str, off: i32, len: i32) {
        self.write_str(&crate::asc::util::jstr::substring(s, off, off + len));
    }

    pub fn get_line_number(&self) -> i32 {
        self.current_line
    }

    pub fn as_str(&self) -> &str {
        &self.buf
    }

    pub fn into_string(self) -> String {
        self.buf
    }
}

impl fmt::Write for SourceCodeBuffer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        SourceCodeBuffer::write_str(self, s);
        Ok(())
    }
}

impl fmt::Display for SourceCodeBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.buf)
    }
}
