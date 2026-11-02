//! Port of afe.jar `com.adobe.fontengine.font.opentype.OTByteArray` and `OTByteArrayBuilder`.

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::{FontByteArray, FontByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OTByteArray {
    pub base: FontByteArray,
}

impl OTByteArray {
    pub fn new(n: i32) -> OTByteArray {
        OTByteArray { base: FontByteArray::new(n) }
    }

    /// `new OTByteArray(fontByteArray)`, which takes over the data of the other array.
    pub fn from_font_byte_array(a: FontByteArray) -> OTByteArray {
        OTByteArray { base: a }
    }

    pub fn get_ot_byte_array_builder_instance() -> OTByteArrayBuilder {
        OTByteArrayBuilder::new()
    }

    pub fn get_ot_byte_array_builder_instance_sized(n: i32) -> OTByteArrayBuilder {
        OTByteArrayBuilder::with_size(n)
    }

    /// `getOTByteArrayBuilderInstance(OTByteArray)`, which copies.
    pub fn get_ot_byte_array_builder_instance_from(a: &OTByteArray) -> OTByteArrayBuilder {
        OTByteArrayBuilder { base: FontByteArrayBuilder::new(a.base.clone()) }
    }

    pub fn get_size(&self) -> i32 {
        self.base.get_size()
    }

    pub fn getuint8(&self, n: i32) -> FontResult<i32> {
        Ok(self.base.get_raw_byte(n)? & 0xff)
    }

    pub fn getint8(&self, n: i32) -> FontResult<i32> {
        self.base.get_signed_raw_byte(n)
    }

    pub fn getuint16(&self, n: i32) -> FontResult<i32> {
        Ok(self.getuint8(n)? << 8 | self.getuint8(n + 1)?)
    }

    pub fn getint16(&self, n: i32) -> FontResult<i32> {
        Ok(self.getint8(n)? << 8 | self.getuint8(n + 1)?)
    }

    pub fn getint32(&self, n: i32) -> FontResult<i32> {
        Ok(self.getint8(n)? << 24 | self.getuint8(n + 1)? << 16 | self.getuint8(n + 2)? << 8 | self.getuint8(n + 3)?)
    }

    /// Java widens the high byte before shifting and ORs the others as ints.
    pub fn getuint32(&self, n: i32) -> FontResult<i64> {
        Ok((self.getuint8(n)? as i64) << 24 | ((self.getuint8(n + 1)? << 16) as i64) | ((self.getuint8(n + 2)? << 8) as i64) | self.getuint8(n + 3)? as i64)
    }

    pub fn getuint24(&self, n: i32) -> FontResult<i32> {
        Ok(self.getuint8(n)? << 16 | self.getuint8(n + 1)? << 8 | self.getuint8(n + 2)?)
    }

    pub fn getuint32asint(&self, n: i32, message: &str) -> FontResult<i32> {
        if self.getuint8(n)? > 127 {
            return Err(FontEngineException::unsupported_font(message));
        }
        Ok(self.getuint32(n)? as i32)
    }

    pub fn get_fixed(&self, n: i32) -> FontResult<[i32; 2]> {
        Ok([self.getuint16(n)?, self.getuint16(n + 2)?])
    }

    /// Java reads the same word twice: `getuint32(n) << 32 | getuint32(n)`.
    pub fn get_longdatetime(&self, n: i32) -> FontResult<i64> {
        let v = self.getuint32(n)?;
        Ok(v << 32 | v)
    }

    pub fn get_offset(&self, n: i32, at: i32) -> FontResult<i32> {
        let o = self.getuint16(n + at)?;
        Ok(if o != 0 { n + o } else { 0 })
    }

    pub fn checksum_all(&self) -> FontResult<i64> {
        self.checksum(0, self.get_size(), 0)
    }

    pub fn checksum(&self, mut n: i32, end: i32, mut l: i64) -> FontResult<i64> {
        while n < end {
            let b0 = self.getuint8(n)?;
            n += 1;
            let mut b1 = 0;
            if n < end {
                b1 = self.getuint8(n)?;
                n += 1;
            }
            let mut b2 = 0;
            if n < end {
                b2 = self.getuint8(n)?;
                n += 1;
            }
            let mut b3 = 0;
            if n < end {
                b3 = self.getuint8(n)?;
                n += 1;
            }
            let v = b0 << 24 | b1 << 16 | b2 << 8 | b3;
            l = (l + v as i64) & 0xFFFF_FFFF;
        }
        Ok(l)
    }
}

/// `OTByteArray.OTByteArrayBuilder`.
#[derive(Clone, Debug, Default)]
pub struct OTByteArrayBuilder {
    pub base: FontByteArrayBuilder,
}

impl OTByteArrayBuilder {
    pub fn new() -> OTByteArrayBuilder {
        OTByteArrayBuilder { base: FontByteArrayBuilder::new(FontByteArray::new(1024)) }
    }

    pub fn with_size(n: i32) -> OTByteArrayBuilder {
        OTByteArrayBuilder { base: FontByteArrayBuilder::new(FontByteArray::new(n)) }
    }

    pub fn get_size(&self) -> i32 {
        self.base.get_size()
    }

    pub fn setuint8(&mut self, n: i32, v: i32) {
        self.base.set_raw_byte(n, v & 0xff);
    }

    pub fn setuint16(&mut self, n: i32, v: i32) {
        self.base.set_raw_byte(n, v >> 8 & 0xff);
        self.base.set_raw_byte(n + 1, v & 0xff);
    }

    pub fn getuint16(&self, n: i32) -> i32 {
        (self.base.get_raw_byte(n) & 0xff) << 8 | (self.base.get_raw_byte(n + 1) & 0xff)
    }

    pub fn setint16(&mut self, n: i32, v: i32) {
        self.setuint16(n, v);
    }

    pub fn setuint24(&mut self, n: i32, v: i32) {
        self.base.set_raw_byte(n, v >> 16 & 0xff);
        self.base.set_raw_byte(n + 1, v >> 8 & 0xff);
        self.base.set_raw_byte(n + 2, v & 0xff);
    }

    pub fn setuint32(&mut self, n: i32, v: i32) {
        self.base.set_raw_byte(n, v >> 24 & 0xff);
        self.base.set_raw_byte(n + 1, v >> 16 & 0xff);
        self.base.set_raw_byte(n + 2, v >> 8 & 0xff);
        self.base.set_raw_byte(n + 3, v & 0xff);
    }

    pub fn set_fixed(&mut self, n: i32, hi: i32, lo: i32) {
        self.setuint16(n, hi);
        self.setuint16(n + 2, lo);
    }

    /// Java ignores the range and seed and sums the whole array.
    pub fn checksum(&self, _n: i32, _end: i32, _l: i64) -> FontResult<i64> {
        OTByteArray { base: self.base.byte_array.clone() }.checksum_all()
    }

    pub fn to_ot_byte_array(self) -> OTByteArray {
        OTByteArray { base: self.base.byte_array }
    }

    pub fn replace_from(&mut self, at: i32, src: &FontByteArray, src_offset: i32, len: i32) -> FontResult<()> {
        self.base.replace_from(at, src, src_offset, len)
    }

    pub fn replace_bytes(&mut self, at: i32, src: &[u8], src_offset: i32, len: i32) {
        self.base.replace_bytes(at, src, src_offset, len)
    }

    pub fn append_from(&mut self, src: &FontByteArray, src_offset: i32, len: i32) -> FontResult<()> {
        self.base.append_from(src, src_offset, len)
    }

    pub fn append_bytes(&mut self, src: &[u8], src_offset: i32, len: i32) {
        self.base.append_bytes(src, src_offset, len)
    }

    pub fn ensure_capacity(&mut self, n: i32) {
        self.base.ensure_capacity(n)
    }

    pub fn ensure_free_space(&mut self, n: i32) {
        self.base.ensure_free_space(n)
    }
}

/// `OTByteArrayBuilder.OTByteArrayBuilderOutputStreamAdaptor`.
#[derive(Clone, Debug, Default)]
pub struct OTByteArrayBuilderOutputStreamAdaptor {
    pub builder: OTByteArrayBuilder,
    pub position: i32,
}

impl OTByteArrayBuilderOutputStreamAdaptor {
    pub fn new() -> Self {
        OTByteArrayBuilderOutputStreamAdaptor { builder: OTByteArrayBuilder::new(), position: 0 }
    }

    pub fn write(&mut self, b: i32) {
        self.builder.ensure_capacity(self.position + 1);
        self.builder.setuint8(self.position, b);
        self.position += 1;
    }

    pub fn write_all(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write(b as i32);
        }
    }

    pub fn get_builder(&mut self) -> &mut OTByteArrayBuilder {
        &mut self.builder
    }
}

impl std::io::Write for OTByteArrayBuilderOutputStreamAdaptor {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write_all(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
