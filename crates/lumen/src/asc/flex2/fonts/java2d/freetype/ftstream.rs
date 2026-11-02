//! Port of FreeType 2.14.2 `src/base/ftstream.c` (as built into JDK 17.0.19), for the JDK's read-callback stream over the whole font file.

use std::rc::Rc;

use crate::asc::flex2::fonts::java2d::freetype::fterrors::*;
use crate::asc::flex2::fonts::java2d::freetype::fttypes::FtError;

#[derive(Clone, Default)]
pub struct FtStream {
    pub base: Rc<Vec<u8>>,
    pub size: u32,
    pub pos: u32,
}

pub type FtResult<T> = Result<T, FtError>;

impl FtStream {
    pub fn new(data: Rc<Vec<u8>>) -> FtStream {
        let size = data.len() as u32;
        FtStream { base: data, size, pos: 0 }
    }

    pub fn seek(&mut self, pos: u32) -> FtResult<()> {
        if pos > self.size {
            return Err(FT_ERR_INVALID_STREAM_OPERATION);
        }
        self.pos = pos;
        Ok(())
    }

    pub fn skip(&mut self, distance: i32) -> FtResult<()> {
        if distance < 0 {
            return Err(FT_ERR_INVALID_STREAM_OPERATION);
        }
        self.seek(self.pos.wrapping_add(distance as u32))
    }

    /// `FT_Stream_EnterFrame` + copy: the bytes `[pos, pos + count)`, advancing `pos`.
    pub fn enter_frame(&mut self, count: u32) -> FtResult<Frame> {
        if count > self.size || self.pos > self.size - count {
            return Err(FT_ERR_INVALID_STREAM_OPERATION);
        }
        let f = Frame { base: self.base.clone(), start: self.pos as usize, cursor: self.pos as usize, limit: (self.pos + count) as usize };
        self.pos += count;
        Ok(f)
    }

    pub fn extract(&mut self, count: u32) -> FtResult<Vec<u8>> {
        let f = self.enter_frame(count)?;
        Ok(f.base[f.start..f.limit].to_vec())
    }

    pub fn read_at(&mut self, pos: u32, count: u32) -> FtResult<Vec<u8>> {
        self.seek(pos)?;
        self.extract(count)
    }

    fn read_n<const N: usize>(&mut self) -> FtResult<[u8; N]> {
        let p = self.pos as usize;
        if p + N > self.size as usize {
            return Err(FT_ERR_INVALID_STREAM_OPERATION);
        }
        let mut a = [0u8; N];
        a.copy_from_slice(&self.base[p..p + N]);
        self.pos += N as u32;
        Ok(a)
    }

    pub fn read_byte(&mut self) -> FtResult<u8> {
        Ok(self.read_n::<1>()?[0])
    }

    pub fn read_ushort(&mut self) -> FtResult<u16> {
        Ok(u16::from_be_bytes(self.read_n::<2>()?))
    }

    pub fn read_short(&mut self) -> FtResult<i16> {
        Ok(i16::from_be_bytes(self.read_n::<2>()?))
    }

    pub fn read_ulong(&mut self) -> FtResult<u32> {
        Ok(u32::from_be_bytes(self.read_n::<4>()?))
    }

    pub fn read_long(&mut self) -> FtResult<i32> {
        Ok(i32::from_be_bytes(self.read_n::<4>()?))
    }
}

/// An entered frame; reads past `limit` are the caller's responsibility to check, as in C.
#[derive(Clone)]
pub struct Frame {
    pub base: Rc<Vec<u8>>,
    pub start: usize,
    pub cursor: usize,
    pub limit: usize,
}

impl Frame {
    pub fn get_byte(&mut self) -> u8 {
        let v = self.base[self.cursor];
        self.cursor += 1;
        v
    }

    pub fn get_char(&mut self) -> i8 {
        self.get_byte() as i8
    }

    pub fn get_ushort(&mut self) -> u16 {
        let v = u16::from_be_bytes([self.base[self.cursor], self.base[self.cursor + 1]]);
        self.cursor += 2;
        v
    }

    pub fn get_short(&mut self) -> i16 {
        self.get_ushort() as i16
    }

    pub fn get_ulong(&mut self) -> u32 {
        let b = &self.base[self.cursor..self.cursor + 4];
        let v = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        self.cursor += 4;
        v
    }

    pub fn get_long(&mut self) -> i32 {
        self.get_ulong() as i32
    }

    pub fn remaining(&self) -> usize {
        self.limit - self.cursor
    }
}

#[inline]
pub fn peek_ushort(p: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([p[at], p[at + 1]])
}

#[inline]
pub fn peek_short(p: &[u8], at: usize) -> i16 {
    peek_ushort(p, at) as i16
}

#[inline]
pub fn peek_ulong(p: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([p[at], p[at + 1], p[at + 2], p[at + 3]])
}
