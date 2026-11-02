//! Port of the JDK 17 `java.nio.ByteBuffer` (big-endian heap buffer) reads the font code makes; `Err` stands for the
//! runtime exception Java throws (underflow, index or position out of range).

use std::rc::Rc;

pub type JResult<T> = Result<T, ()>;

#[derive(Clone)]
pub struct ByteBuffer {
    data: Rc<Vec<u8>>,
    start: usize,
    limit: usize,
    pos: usize,
}

impl ByteBuffer {
    pub fn wrap(data: Rc<Vec<u8>>) -> ByteBuffer {
        let limit = data.len();
        ByteBuffer { data, start: 0, limit, pos: 0 }
    }

    pub fn capacity(&self) -> usize {
        self.limit
    }

    pub fn remaining(&self) -> usize {
        self.limit - self.pos
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn set_position(&mut self, p: i64) -> JResult<()> {
        if p < 0 || p as usize > self.limit {
            return Err(());
        }
        self.pos = p as usize;
        Ok(())
    }

    /// `slice()`: a buffer over the remaining bytes.
    pub fn slice(&self) -> ByteBuffer {
        ByteBuffer { data: self.data.clone(), start: self.start + self.pos, limit: self.limit - self.pos, pos: 0 }
    }

    fn at(&self, i: i64, n: usize) -> JResult<&[u8]> {
        if i < 0 || i as usize + n > self.limit {
            return Err(());
        }
        let s = self.start + i as usize;
        Ok(&self.data[s..s + n])
    }

    pub fn get_u8_at(&self, i: i64) -> JResult<u8> {
        Ok(self.at(i, 1)?[0])
    }

    pub fn get_char_at(&self, i: i64) -> JResult<u16> {
        let b = self.at(i, 2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    pub fn get_short_at(&self, i: i64) -> JResult<i16> {
        Ok(self.get_char_at(i)? as i16)
    }

    pub fn get_int_at(&self, i: i64) -> JResult<i32> {
        let b = self.at(i, 4)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn take(&mut self, n: usize) -> JResult<&[u8]> {
        if self.remaining() < n {
            return Err(());
        }
        let s = self.start + self.pos;
        self.pos += n;
        Ok(&self.data[s..s + n])
    }

    pub fn get_u8(&mut self) -> JResult<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn get_char(&mut self) -> JResult<u16> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    pub fn get_short(&mut self) -> JResult<i16> {
        Ok(self.get_char()? as i16)
    }

    pub fn get_int(&mut self) -> JResult<i32> {
        let b = self.take(4)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// `get(dst, 0, len)`: fails when `len` exceeds `dst_len` or the remaining bytes.
    pub fn get_bytes(&mut self, dst_len: usize, len: usize) -> JResult<Vec<u8>> {
        if len > dst_len {
            return Err(());
        }
        Ok(self.take(len)?.to_vec())
    }

    /// `asCharBuffer()`/`asShortBuffer()`/`asIntBuffer()`: a view starting at the position, `unit` bytes per element.
    pub fn view(&self, unit: usize) -> ByteBuffer {
        let n = self.remaining() / unit * unit;
        ByteBuffer { data: self.data.clone(), start: self.start + self.pos, limit: n, pos: 0 }
    }
}
