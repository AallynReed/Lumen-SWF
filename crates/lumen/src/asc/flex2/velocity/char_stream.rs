//! Port of `VelocityCharStream` over a fully buffered input; line and column are computed as JavaCC's `UpdateLineColumn` does.

pub struct VelocityCharStream {
    buffer: Vec<u16>,
    bufline: Vec<i32>,
    bufcolumn: Vec<i32>,
    pub bufpos: isize,
    token_begin: isize,
}

pub struct Eof;

impl VelocityCharStream {
    pub fn new(input: &[u16]) -> VelocityCharStream {
        let mut bufline = Vec::with_capacity(input.len());
        let mut bufcolumn = Vec::with_capacity(input.len());
        let (mut line, mut column) = (1, 0);
        let (mut prev_cr, mut prev_lf) = (false, false);
        for &c in input {
            column += 1;
            if prev_lf {
                prev_lf = false;
                column = 1;
                line += 1;
            } else if prev_cr {
                prev_cr = false;
                if c == b'\n' as u16 {
                    prev_lf = true;
                } else {
                    column = 1;
                    line += 1;
                }
            }
            match c {
                0x0D => prev_cr = true,
                0x0A => prev_lf = true,
                0x09 => {
                    column -= 1;
                    column += 8 - (column & 7);
                }
                _ => {}
            }
            bufline.push(line);
            bufcolumn.push(column);
        }
        VelocityCharStream { buffer: input.to_vec(), bufline, bufcolumn, bufpos: -1, token_begin: 0 }
    }

    pub fn begin_token(&mut self) -> Result<u16, Eof> {
        self.token_begin = -1;
        let c = self.read_char()?;
        self.token_begin = self.bufpos;
        Ok(c)
    }

    pub fn read_char(&mut self) -> Result<u16, Eof> {
        if self.bufpos + 1 >= self.buffer.len() as isize {
            if self.token_begin == -1 {
                self.token_begin = self.bufpos;
            }
            return Err(Eof);
        }
        self.bufpos += 1;
        Ok(self.buffer[self.bufpos as usize])
    }

    pub fn backup(&mut self, amount: isize) {
        self.bufpos -= amount;
    }

    /// The text from the token start through the current position; past an empty input Java reads a buffer slot holding `\0`.
    pub fn get_image(&self) -> String {
        if self.token_begin < 0 || self.bufpos < 0 {
            return "\0".to_string();
        }
        crate::asc::util::jstr::from_utf16(&self.buffer[self.token_begin as usize..=self.bufpos as usize])
    }

    fn at(v: &[i32], i: isize) -> i32 {
        if i < 0 {
            0
        } else {
            v[i as usize]
        }
    }

    pub fn get_begin_line(&self) -> i32 {
        Self::at(&self.bufline, self.token_begin)
    }

    pub fn get_begin_column(&self) -> i32 {
        Self::at(&self.bufcolumn, self.token_begin)
    }

    pub fn get_end_line(&self) -> i32 {
        Self::at(&self.bufline, self.bufpos)
    }

    pub fn get_end_column(&self) -> i32 {
        Self::at(&self.bufcolumn, self.bufpos)
    }

    /// The input from the current character on.
    pub fn remaining(&self) -> &[u16] {
        &self.buffer[(self.bufpos.max(0) as usize).min(self.buffer.len())..]
    }
}
