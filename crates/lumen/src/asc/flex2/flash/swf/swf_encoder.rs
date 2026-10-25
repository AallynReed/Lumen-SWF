//! Port of `flash.swf.SwfEncoder` over `flash.swf.RandomAccessBuffer`: a byte buffer written at a movable position,
//! with SWF bit fields and an in-place zlib section.

/// `CompressionLevel`: `BestSpeed` once a `DebugID` was written, else `BestCompression`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompressionLevel {
    BestSpeed,
    BestCompression,
}

pub struct SwfEncoder {
    buf: Vec<u8>,
    pos: usize,
    bit_pos: i32,
    current_byte: u8,
    compress_pos: Option<usize>,
    pub swf_version: i32,
}

/// `java.util.zip.Deflater` at `level` through a `DeflaterOutputStream`.
pub fn deflate(data: &[u8], level: CompressionLevel) -> Vec<u8> {
    let level = match level {
        CompressionLevel::BestSpeed => 1,
        CompressionLevel::BestCompression => 9,
    };
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(level));
    std::io::Write::write_all(&mut z, data).expect("in-memory deflate");
    z.finish().expect("in-memory deflate")
}

impl SwfEncoder {
    pub fn new(version: i32) -> SwfEncoder {
        SwfEncoder { buf: Vec::with_capacity(1024), pos: 0, bit_pos: 8, current_byte: 0, compress_pos: None, swf_version: version }
    }

    pub fn get_pos(&self) -> usize {
        self.pos
    }

    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// `RandomAccessBuffer.write(int)`.
    fn put(&mut self, b: u8) {
        if self.pos < self.buf.len() {
            self.buf[self.pos] = b;
        } else {
            self.buf.push(b);
        }
        self.pos += 1;
    }

    /// `RandomAccessBuffer.write(byte[])`.
    pub fn write(&mut self, b: &[u8]) {
        if self.pos > self.buf.len() {
            self.buf.resize(self.pos, 0);
        }
        let overlap = (self.buf.len() - self.pos).min(b.len());
        self.buf[self.pos..self.pos + overlap].copy_from_slice(&b[..overlap]);
        self.buf.extend_from_slice(&b[overlap..]);
        self.pos += b.len();
    }

    pub fn write_ui8(&mut self, c: i32) {
        self.put(c as u8);
    }

    pub fn write_fixed8(&mut self, v: f32) {
        let f8 = ((v * 256.0) as i32) & 0xffff;
        self.write_ui16(f8);
    }

    pub fn write_ui16(&mut self, c: i32) {
        let c = if c == -1 { 65535 } else { c };
        self.put(c as u8);
        self.put((c >> 8) as u8);
    }

    pub fn write_si16(&mut self, c: i32) {
        self.put(c as u8);
        self.put((c >> 8) as u8);
    }

    pub fn write32(&mut self, c: i32) {
        self.put(c as u8);
        self.put((c >> 8) as u8);
        self.put((c >> 16) as u8);
        self.put((c >> 24) as u8);
    }

    pub fn write64(&mut self, c: i64) {
        self.write32(c as i32);
        self.write32((c >> 32) as i32);
    }

    pub fn write_float(&mut self, f: f32) {
        self.write32(f.to_bits() as i32);
    }

    pub fn mark_comp(&mut self) {
        self.compress_pos = Some(self.pos);
    }

    /// `compress(level)`: deflates the marked section in place.
    pub fn compress(&mut self, level: CompressionLevel) {
        if let Some(cp) = self.compress_pos.take() {
            let z = deflate(&self.buf[cp..], level);
            self.pos = cp;
            self.write(&z);
        }
    }

    /// `writeTo(out, level)`: the buffer up to the position, the marked tail deflated.
    pub fn write_to(&mut self, out: &mut SwfEncoder, level: CompressionLevel) {
        match self.compress_pos {
            None => out.write(&self.buf[..self.pos]),
            Some(cp) => {
                out.write(&self.buf[..cp]);
                let z = deflate(&self.buf[cp..self.pos], level);
                out.write(&z);
            }
        }
    }

    /// `toByteArray()`.
    pub fn to_byte_array(&self) -> Vec<u8> {
        self.buf[..self.pos.min(self.buf.len())].to_vec()
    }

    pub fn write_bit(&mut self, data: bool) {
        self.write_bits(data as i32, 1);
    }

    fn write_bits(&mut self, data: i32, mut size: i32) {
        while size > 0 {
            if size > self.bit_pos {
                self.current_byte |= ((data << (32 - size)) as u32 >> (32 - self.bit_pos)) as u8;
                let b = self.current_byte;
                self.put(b);
                size -= self.bit_pos;
                self.current_byte = 0;
                self.bit_pos = 8;
            } else {
                self.current_byte |= ((data << (32 - size)) as u32 >> (32 - self.bit_pos)) as u8;
                self.bit_pos -= size;
                size = 0;
                if self.bit_pos == 0 {
                    let b = self.current_byte;
                    self.put(b);
                    self.current_byte = 0;
                    self.bit_pos = 8;
                }
            }
        }
    }

    pub fn write_ubits(&mut self, data: i32, size: i32) {
        self.write_bits(data, size);
    }

    pub fn write_sbits(&mut self, data: i32, size: i32) {
        self.write_bits(data, size);
    }

    pub fn flush_bits(&mut self) {
        if self.bit_pos != 8 {
            let b = self.current_byte;
            self.put(b);
            self.current_byte = 0;
            self.bit_pos = 8;
        }
    }

    pub fn reset(&mut self) {
        self.buf.clear();
        self.pos = 0;
        self.compress_pos = None;
    }

    pub fn write_ui16_at(&mut self, pos: usize, value: i32) {
        let old = self.pos;
        self.pos = pos;
        self.write_ui16(value);
        self.pos = old;
    }

    pub fn write32_at(&mut self, pos: usize, value: i32) {
        let old = self.pos;
        self.pos = pos;
        self.write32(value);
        self.pos = old;
    }

    /// `writeString`: UTF-8 for SWF 6 and later, NUL-terminated.
    pub fn write_string(&mut self, s: &str) {
        let b = if self.swf_version >= 6 { crate::asc::util::jstr::to_java_utf8_bytes(s) } else { s.bytes().collect() };
        self.write(&b);
        self.put(0);
    }

    pub fn write_length_string(&mut self, name: &str) {
        let b = if self.swf_version >= 6 { crate::asc::util::jstr::to_java_utf8_bytes(name) } else { name.bytes().collect() };
        self.put((b.len() + 1) as u8);
        self.write(&b);
        self.put(0);
    }
}

/// `SwfEncoder.maxNum`.
pub fn max_num(a: i32, b: i32, c: i32, d: i32) -> i32 {
    let (a, b, c, d) = (a.wrapping_abs(), b.wrapping_abs(), c.wrapping_abs(), d.wrapping_abs());
    if a > b {
        if a > c {
            if a > d { a } else { d }
        } else if c > d {
            c
        } else {
            d
        }
    } else if b > c {
        if b > d { b } else { d }
    } else if c > d {
        c
    } else {
        d
    }
}

/// `SwfEncoder.minBits`.
pub fn min_bits(number: i32, mut bits: i32) -> i32 {
    let mut val: i32 = 1;
    let mut x: i32 = 1;
    while val <= number && bits <= 32 {
        val |= x;
        bits += 1;
        x = x.wrapping_shl(1);
    }
    bits
}
