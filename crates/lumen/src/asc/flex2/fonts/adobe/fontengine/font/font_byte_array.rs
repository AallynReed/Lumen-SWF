//! Port of afe.jar `com.adobe.fontengine.font.FontByteArray` and its `FontByteArrayBuilder`, kept as one buffer.

use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

#[derive(Clone, Debug, Default)]
pub struct FontByteArray {
    /// At least `size` bytes; the 262143-byte chunks of the Java class are not observable.
    pub data: Vec<u8>,
    /// The allocated length that `getRawByte` checks against.
    pub size: i32,
    pub used: i32,
}

impl FontByteArray {
    pub fn new(n: i32) -> FontByteArray {
        FontByteArray { data: vec![0; n.max(0) as usize], size: n, used: 0 }
    }

    /// `new FontByteArray(stream, n)` over bytes already read: fails like Java when fewer than `n` are there.
    pub fn from_bytes(bytes: &[u8], n: i32) -> FontResult<FontByteArray> {
        let mut a = FontByteArray::new(n);
        if (bytes.len() as i64) < n as i64 {
            return Err(FontEngineException::invalid_font("not enough data"));
        }
        a.data[..n as usize].copy_from_slice(&bytes[..n as usize]);
        a.used = n;
        Ok(a)
    }

    pub fn get_size(&self) -> i32 {
        self.used
    }

    pub fn get_raw_byte(&self, n: i32) -> FontResult<i32> {
        if n < 0 || n >= self.size {
            return Err(FontEngineException::invalid_font(format!("Invalid index = {}", n)));
        }
        Ok(self.data[n as usize] as i32)
    }

    pub fn get_signed_raw_byte(&self, n: i32) -> FontResult<i32> {
        if n < 0 || n >= self.size {
            return Err(FontEngineException::invalid_font(format!("Invalid index = {}", n)));
        }
        Ok(self.data[n as usize] as i8 as i32)
    }

    /// `getBytes(dest, destOffset, srcOffset, len)`: copies into the buffer of `dest` without touching its `used`.
    pub fn get_bytes_into(&self, dest: &mut FontByteArray, dest_offset: i32, src_offset: i32, len: i32) {
        if len <= 0 {
            return;
        }
        let (d, s, l) = (dest_offset as usize, src_offset as usize, len as usize);
        assert!(s + l <= self.data.len(), "Source buffer is too small");
        dest.data[d..d + l].copy_from_slice(&self.data[s..s + l]);
    }

    pub fn get_bytes(&self, offset: i32, len: i32) -> Vec<u8> {
        self.data[offset as usize..(offset + len) as usize].to_vec()
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.data[..self.used as usize]);
    }

    /// The bytes in use.
    pub fn as_slice(&self) -> &[u8] {
        &self.data[..self.used as usize]
    }
}

impl PartialEq for FontByteArray {
    fn eq(&self, o: &Self) -> bool {
        self.as_slice() == o.as_slice()
    }
}

/// `FontByteArray.FontByteArrayBuilder`.
#[derive(Clone, Debug, Default)]
pub struct FontByteArrayBuilder {
    pub byte_array: FontByteArray,
}

impl FontByteArrayBuilder {
    pub fn new(byte_array: FontByteArray) -> FontByteArrayBuilder {
        FontByteArrayBuilder { byte_array }
    }

    pub fn set_raw_byte(&mut self, n: i32, v: i32) {
        self.ensure_capacity(n + 1);
        self.byte_array.data[n as usize] = (v & 0xff) as u8;
        self.byte_array.used = self.byte_array.used.max(n + 1);
    }

    /// Java returns the signed byte.
    pub fn get_raw_byte(&self, n: i32) -> i32 {
        self.byte_array.data[n as usize] as i8 as i32
    }

    pub fn append_raw_byte(&mut self, v: i32) {
        let n = self.byte_array.used;
        self.ensure_capacity(n + 1);
        self.set_raw_byte(n, v);
    }

    pub fn get_size(&self) -> i32 {
        self.byte_array.get_size()
    }

    pub fn replace_from(&mut self, at: i32, src: &FontByteArray, src_offset: i32, len: i32) -> FontResult<()> {
        self.ensure_capacity(at + len);
        if len > 0 {
            src.get_bytes_into(&mut self.byte_array, at, src_offset, len);
            self.byte_array.used = self.byte_array.used.max(at + len);
        }
        Ok(())
    }

    pub fn replace_bytes(&mut self, at: i32, src: &[u8], src_offset: i32, len: i32) {
        self.ensure_capacity(at + len);
        for i in 0..len {
            self.set_raw_byte(at + i, src[(i + src_offset) as usize] as i32);
        }
    }

    pub fn append_from(&mut self, src: &FontByteArray, src_offset: i32, len: i32) -> FontResult<()> {
        let at = self.byte_array.used;
        self.replace_from(at, src, src_offset, len)
    }

    pub fn append_bytes(&mut self, src: &[u8], src_offset: i32, len: i32) {
        let at = self.byte_array.used;
        self.replace_bytes(at, src, src_offset, len)
    }

    pub fn ensure_capacity(&mut self, n: i32) {
        if n > self.byte_array.size {
            self.grow(n);
        }
    }

    pub fn ensure_free_space(&mut self, n: i32) {
        if self.byte_array.size - self.byte_array.used < n {
            let want = self.byte_array.used + n;
            self.grow(want);
        }
    }

    /// Java leaves `size == n` (its last chunk may be longer, which only `getRawByte` can see through `size`).
    fn grow(&mut self, n: i32) {
        if self.byte_array.data.len() < n as usize {
            self.byte_array.data.resize(n as usize, 0);
        }
        self.byte_array.size = n;
    }
}
