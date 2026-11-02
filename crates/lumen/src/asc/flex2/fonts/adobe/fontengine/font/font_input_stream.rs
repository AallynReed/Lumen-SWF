//! Port of afe.jar `com.adobe.fontengine.font.FontInputStream` over the bytes of a whole file.

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

#[derive(Clone, Debug, Default)]
pub struct FontInputStream {
    data: Vec<u8>,
    /// Where the underlying stream is; a skip may take it past the end, as `FileInputStream.skip` does.
    pos: usize,
    cur_offset: i64,
}

impl FontInputStream {
    pub fn new(data: Vec<u8>) -> FontInputStream {
        FontInputStream { data, pos: 0, cur_offset: 0 }
    }

    pub fn get_current_offset(&self) -> i64 {
        self.cur_offset
    }

    pub fn skip_to(&mut self, l: i64) -> FontResult<i64> {
        if self.cur_offset > l {
            return Err(FontEngineException::invalid_font(format!(
                "trying to read the same byte twice (read up to {:x}, want to go back to {:x})",
                self.cur_offset, l
            )));
        }
        let start = self.cur_offset;
        self.pos += (l - self.cur_offset) as usize;
        self.cur_offset = l;
        Ok(self.cur_offset - start)
    }

    /// `read(byte[], off, len)`: -1 at the end.
    pub fn read_into(&mut self, b: &mut [u8]) -> i32 {
        if b.is_empty() {
            return 0;
        }
        if self.pos >= self.data.len() {
            return -1;
        }
        let n = b.len().min(self.data.len() - self.pos);
        b[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        self.cur_offset += n as i64;
        n as i32
    }

    pub fn read(&mut self) -> i32 {
        if self.pos >= self.data.len() {
            return -1;
        }
        let v = self.data[self.pos] as i32;
        self.pos += 1;
        self.cur_offset += 1;
        v
    }

    /// `unread(b, 0, n)` of bytes just read.
    pub fn unread(&mut self, n: i32) {
        self.pos -= n as usize;
        self.cur_offset -= n as i64;
    }

    /// `new FontByteArray(this, n)`.
    pub fn read_font_byte_array(&mut self, n: i32) -> FontResult<FontByteArray> {
        if n < 0 {
            return Err(FontEngineException::from_cause(crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::ExceptionKind::FontEngine, format!("java.lang.NegativeArraySizeException: {}", n)));
        }
        let mut a = FontByteArray::new(n);
        self.add_bytes(&mut a, n, 0)?;
        Ok(a)
    }

    /// `FontByteArray.addBytes(this, n, n2)`: reads `n` bytes into `a` at `n2`.
    pub fn add_bytes(&mut self, a: &mut FontByteArray, n: i32, n2: i32) -> FontResult<()> {
        let want = n.min(a.size - n2).max(0) as usize;
        let mut done = 0usize;
        while done < want {
            let start = n2 as usize + done;
            let r = self.read_into(&mut a.data[start..n2 as usize + want]);
            if r == -1 {
                a.used += done as i32;
                return Err(FontEngineException::invalid_font("not enough data"));
            }
            done += r as usize;
        }
        a.used += done as i32;
        Ok(())
    }

    /// `getDigest()` reads the rest of the stream; the SHA digest itself is not kept.
    pub fn get_digest(&mut self) -> Vec<u8> {
        self.pos = self.pos.max(self.data.len());
        Vec::new()
    }
}
