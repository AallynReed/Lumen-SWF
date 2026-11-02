//! Port of afe.jar `com.adobe.fontengine.font.cff.CFFByteArray` and `CFFByteArrayBuilder`.

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::{FontByteArray, FontByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CFFByteArray {
    pub base: FontByteArray,
}

impl CFFByteArray {
    pub fn new(n: i32) -> CFFByteArray {
        CFFByteArray { base: FontByteArray::new(n) }
    }

    /// `new CFFByteArray(fontByteArray)`, which takes over the data of the other array.
    pub fn from_font_byte_array(a: FontByteArray) -> CFFByteArray {
        CFFByteArray { base: a }
    }

    pub fn get_cff_byte_array_builder_instance() -> CFFByteArrayBuilder {
        CFFByteArrayBuilder::new()
    }

    pub fn get_cff_byte_array_builder_instance_sized(n: i32) -> CFFByteArrayBuilder {
        CFFByteArrayBuilder::with_size(n)
    }

    pub fn get_size(&self) -> i32 {
        self.base.get_size()
    }

    pub fn getcard8(&self, n: i32) -> FontResult<i32> {
        if n < 0 || n >= self.get_size() {
            return Err(FontEngineException::invalid_font("Invalid index"));
        }
        self.base.get_raw_byte(n)
    }

    pub fn getint8(&self, n: i32) -> FontResult<i32> {
        if n < 0 || n >= self.get_size() {
            return Err(FontEngineException::invalid_font("Invalid index"));
        }
        self.base.get_signed_raw_byte(n)
    }

    pub fn getcard16(&self, n: i32) -> FontResult<i32> {
        Ok(self.getcard8(n)? << 8 | self.getcard8(n + 1)?)
    }

    pub fn getint16(&self, n: i32) -> FontResult<i32> {
        Ok(self.getint8(n)? << 8 | self.getcard8(n + 1)?)
    }

    pub fn getcard24(&self, n: i32) -> FontResult<i32> {
        Ok(self.getcard8(n)? << 16 | self.getcard8(n + 1)? << 8 | self.getcard8(n + 2)?)
    }

    /// Java ORs as ints and then widens, so a set high bit comes back negative.
    pub fn getcard32(&self, n: i32) -> FontResult<i64> {
        Ok((self.getcard8(n)? << 24 | self.getcard8(n + 1)? << 16 | self.getcard8(n + 2)? << 8 | self.getcard8(n + 3)?) as i64)
    }

    pub fn getint32(&self, n: i32) -> FontResult<i32> {
        Ok(self.getint8(n)? << 24 | self.getcard8(n + 1)? << 16 | self.getcard8(n + 2)? << 8 | self.getcard8(n + 3)?)
    }

    pub fn get_off_size(&self, n: i32) -> FontResult<i32> {
        self.getcard8(n)
    }

    pub fn get_offset(&self, n: i32, off_size: i32, message: &str) -> FontResult<i32> {
        match off_size {
            1 => self.getcard8(n),
            2 => self.getcard16(n),
            3 => self.getcard24(n),
            _ => {
                if self.getcard8(n)? > 127 {
                    return Err(FontEngineException::unsupported_font(message));
                }
                Ok(self.getcard32(n)? as i32)
            }
        }
    }
}

/// `CFFByteArray.CFFByteArrayBuilder`.
#[derive(Clone, Debug, Default)]
pub struct CFFByteArrayBuilder {
    pub base: FontByteArrayBuilder,
}

impl CFFByteArrayBuilder {
    pub fn new() -> CFFByteArrayBuilder {
        CFFByteArrayBuilder { base: FontByteArrayBuilder::new(FontByteArray::new(1024)) }
    }

    pub fn with_size(n: i32) -> CFFByteArrayBuilder {
        CFFByteArrayBuilder { base: FontByteArrayBuilder::new(FontByteArray::new(n)) }
    }

    pub fn get_size(&self) -> i32 {
        self.base.get_size()
    }

    pub fn add_card32(&mut self, n: i32) {
        self.base.append_raw_byte(n >> 24 & 0xff);
        self.base.append_raw_byte(n >> 16 & 0xff);
        self.base.append_raw_byte(n >> 8 & 0xff);
        self.base.append_raw_byte(n & 0xff);
    }

    pub fn add_card24(&mut self, n: i32) {
        self.base.append_raw_byte(n >> 16 & 0xff);
        self.base.append_raw_byte(n >> 8 & 0xff);
        self.base.append_raw_byte(n & 0xff);
    }

    pub fn add_card16(&mut self, n: i32) {
        self.base.append_raw_byte(n >> 8 & 0xff);
        self.base.append_raw_byte(n & 0xff);
    }

    pub fn add_card8(&mut self, n: i32) {
        self.base.append_raw_byte(n & 0xff);
    }

    pub fn add_offset(&mut self, off_size: i32, n: i32) {
        match off_size {
            1 => self.add_card8(n),
            2 => self.add_card16(n),
            3 => self.add_card24(n),
            _ => self.add_card32(n),
        }
    }

    pub fn set_card32(&mut self, n: i32, v: i32) {
        self.base.set_raw_byte(n, v >> 24 & 0xff);
        self.base.set_raw_byte(n + 1, v >> 16 & 0xff);
        self.base.set_raw_byte(n + 2, v >> 8 & 0xff);
        self.base.set_raw_byte(n + 3, v & 0xff);
    }

    pub fn add_bytes(&mut self, bytes: &[u8], offset: i32, len: i32) {
        self.base.append_bytes(bytes, offset, len)
    }

    pub fn add_bytes_from(&mut self, src: &CFFByteArray, offset: i32, len: i32) -> FontResult<()> {
        self.base.append_from(&src.base, offset, len)
    }

    pub fn to_cff_byte_array(self) -> CFFByteArray {
        CFFByteArray { base: self.base.byte_array }
    }
}
