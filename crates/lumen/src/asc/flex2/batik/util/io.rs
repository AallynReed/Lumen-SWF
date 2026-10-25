//! Port of `org.apache.flex.forks.batik.util.io.StreamNormalizingReader` over a `Reader` (`GenericDecoder`), the
//! only form the CSS parser is given: CR and CRLF read as LF, with Batik's line and column counting.

pub struct StreamNormalizingReader {
    chars: Vec<u16>,
    pos: usize,
    next_char: i32,
    line: i32,
    column: i32,
}

impl StreamNormalizingReader {
    pub fn new(chars: Vec<u16>) -> StreamNormalizingReader {
        StreamNormalizingReader { chars, pos: 0, next_char: -1, line: 1, column: 0 }
    }

    fn read_char(&mut self) -> i32 {
        match self.chars.get(self.pos) {
            Some(&c) => {
                self.pos += 1;
                c as i32
            }
            None => -1,
        }
    }

    pub fn read(&mut self) -> i32 {
        let result = self.next_char;
        if result != -1 {
            self.next_char = -1;
            if result == 13 {
                self.column = 0;
                self.line += 1;
            } else {
                self.column += 1;
            }
            return result;
        }
        let result = self.read_char();
        match result {
            13 => {
                self.column = 0;
                self.line += 1;
                let c = self.read_char();
                if c == 10 {
                    return 10;
                }
                self.next_char = c;
                return 10;
            }
            10 => {
                self.column = 0;
                self.line += 1;
            }
            _ => {}
        }
        result
    }

    pub fn get_line(&self) -> i32 {
        self.line
    }

    pub fn get_column(&self) -> i32 {
        self.column
    }
}
