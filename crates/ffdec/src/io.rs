use crate::error::{Error, Result};

#[derive(Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_pos: u8,
    bit_buf: u8,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, bit_pos: 0, bit_buf: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn remaining_bits(&self) -> usize {
        self.remaining() * 8 + self.bit_pos as usize
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub fn rest(&mut self) -> &'a [u8] {
        self.align();
        let r = &self.data[self.pos.min(self.data.len())..];
        self.pos = self.data.len();
        r
    }

    pub fn align(&mut self) {
        self.bit_pos = 0;
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.align();
        if self.remaining() < n {
            return Err(Error::Eof);
        }
        let r = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(r)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n).map(|_| ())
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }

    pub fn u16(&mut self) -> Result<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }

    pub fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    pub fn f64(&mut self) -> Result<f64> {
        let b = self.bytes(8)?;
        Ok(f64::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn fixed8(&mut self) -> Result<f32> {
        Ok(self.i16()? as f32 / 256.0)
    }

    pub fn fixed(&mut self) -> Result<f64> {
        Ok(self.i32()? as f64 / 65536.0)
    }

    pub fn encoded_u32(&mut self) -> Result<u32> {
        let mut result = 0u32;
        for i in 0..5 {
            let b = self.u8()?;
            result |= ((b & 0x7f) as u32) << (7 * i);
            if b & 0x80 == 0 {
                break;
            }
        }
        Ok(result)
    }

    pub fn string(&mut self) -> Result<String> {
        self.align();
        let start = self.pos;
        while self.pos < self.data.len() && self.data[self.pos] != 0 {
            self.pos += 1;
        }
        let s = String::from_utf8_lossy(&self.data[start..self.pos]).into_owned();
        if self.pos < self.data.len() {
            self.pos += 1;
        }
        Ok(s)
    }

    pub fn bit(&mut self) -> Result<bool> {
        Ok(self.ub(1)? == 1)
    }

    pub fn ub(&mut self, n: u32) -> Result<u32> {
        let mut v = 0u32;
        for _ in 0..n {
            if self.bit_pos == 0 {
                if self.pos >= self.data.len() {
                    return Err(Error::Eof);
                }
                self.bit_buf = self.data[self.pos];
                self.pos += 1;
                self.bit_pos = 8;
            }
            self.bit_pos -= 1;
            v = (v << 1) | ((self.bit_buf >> self.bit_pos) & 1) as u32;
        }
        Ok(v)
    }

    pub fn sb(&mut self, n: u32) -> Result<i32> {
        if n == 0 {
            return Ok(0);
        }
        let v = self.ub(n)?;
        let shift = 32 - n;
        Ok(((v << shift) as i32) >> shift)
    }

    pub fn fb(&mut self, n: u32) -> Result<f64> {
        Ok(self.sb(n)? as f64 / 65536.0)
    }
}

#[derive(Default)]
pub struct Writer {
    pub data: Vec<u8>,
    bit_pos: u8,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn flush_bits(&mut self) {
        self.bit_pos = 0;
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.flush_bits();
        self.data.extend_from_slice(b);
    }

    pub fn u8(&mut self, v: u8) {
        self.bytes(&[v]);
    }

    pub fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn i16(&mut self, v: i16) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn string(&mut self, s: &str) {
        self.bytes(s.as_bytes());
        self.u8(0);
    }

    pub fn ub(&mut self, n: u32, v: u32) {
        for i in (0..n).rev() {
            if self.bit_pos == 0 {
                self.data.push(0);
                self.bit_pos = 8;
            }
            self.bit_pos -= 1;
            if (v >> i) & 1 == 1 {
                *self.data.last_mut().unwrap() |= 1 << self.bit_pos;
            }
        }
    }

    pub fn sb(&mut self, n: u32, v: i32) {
        self.ub(n, v as u32);
    }
}

pub fn bits_needed_signed(values: &[i32]) -> u32 {
    values
        .iter()
        .map(|&v| {
            let m = if v < 0 { !v } else { v } as u32;
            33 - m.leading_zeros()
        })
        .max()
        .unwrap_or(0)
        .max(1)
}
