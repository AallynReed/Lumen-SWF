//! The byte streams and readers Xerces-J 2.9.1 parses a document through: `BufferedInputStream`,
//! `XMLEntityManager.RewindableInputStream`, `UTF8Reader`, `Latin1Reader`, `ASCIIReader`, and a `java.io.InputStreamReader`
//! stand-in for the other encodings.

/// A reader failure: `MalformedByteSequenceException` (`key`, `args`) or `CharConversionException`.
#[derive(Clone, Debug)]
pub enum ReadError {
    Malformed { key: &'static str, args: Vec<String> },
    CharConversion,
}

/// The `BufferedInputStream` over the source file.
pub struct ByteStream {
    data: Vec<u8>,
    pos: usize,
}

impl ByteStream {
    pub fn new(data: Vec<u8>) -> ByteStream {
        ByteStream { data, pos: 0 }
    }

    fn read1(&mut self) -> i32 {
        if self.pos < self.data.len() {
            self.pos += 1;
            self.data[self.pos - 1] as i32
        } else {
            -1
        }
    }

    fn read(&mut self, b: &mut [u8], off: usize, len: usize) -> i32 {
        if len == 0 {
            return 0;
        }
        if self.pos >= self.data.len() {
            return -1;
        }
        let n = len.min(self.data.len() - self.pos);
        b[off..off + n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        n as i32
    }

    fn skip(&mut self, n: i64) -> i64 {
        let k = (n.max(0) as usize).min(self.data.len() - self.pos);
        self.pos += k;
        k as i64
    }
}

/// `XMLEntityManager.RewindableInputStream`.
pub struct Rewindable {
    input: ByteStream,
    data: Vec<u8>,
    start_offset: usize,
    end_offset: i64,
    offset: usize,
    length: usize,
}

impl Rewindable {
    pub fn new(input: ByteStream) -> Rewindable {
        Rewindable { input, data: vec![0; 64], start_offset: 0, end_offset: -1, offset: 0, length: 0 }
    }

    pub fn read1(&mut self) -> i32 {
        if self.offset < self.length {
            self.offset += 1;
            return self.data[self.offset - 1] as i32;
        }
        if self.offset as i64 == self.end_offset {
            return -1;
        }
        if self.offset == self.data.len() {
            self.data.resize(self.offset << 1, 0);
        }
        let b = self.input.read1();
        if b == -1 {
            self.end_offset = self.offset as i64;
            return -1;
        }
        self.data[self.length] = b as u8;
        self.length += 1;
        self.offset += 1;
        b & 0xff
    }

    pub fn read(&mut self, b: &mut [u8], off: usize, len: usize, may_read_chunks: bool) -> i32 {
        let bytes_left = self.length - self.offset;
        if bytes_left == 0 {
            if self.offset as i64 == self.end_offset {
                return -1;
            }
            if may_read_chunks {
                return self.input.read(b, off, len);
            }
            let r = self.read1();
            if r == -1 {
                self.end_offset = self.offset as i64;
                return -1;
            }
            b[off] = r as u8;
            return 1;
        }
        let len = if len < bytes_left {
            if len == 0 {
                return 0;
            }
            len
        } else {
            bytes_left
        };
        b[off..off + len].copy_from_slice(&self.data[self.offset..self.offset + len]);
        self.offset += len;
        len as i32
    }

    pub fn skip(&mut self, n: i64) -> i64 {
        if n <= 0 {
            return 0;
        }
        let bytes_left = (self.length - self.offset) as i64;
        if bytes_left == 0 {
            if self.offset as i64 == self.end_offset {
                return 0;
            }
            return self.input.skip(n);
        }
        if n <= bytes_left {
            self.offset += n as usize;
            return n;
        }
        self.offset += bytes_left as usize;
        if self.offset as i64 == self.end_offset {
            return bytes_left;
        }
        self.input.skip(n - bytes_left) + bytes_left
    }

    pub fn reset(&mut self) {
        self.offset = self.start_offset;
    }
}

/// The byte buffer `XMLEntityManager`'s pool lends the readers (`DEFAULT_BUFFER_SIZE` bytes).
const BYTE_BUFFER_SIZE: usize = 2048;

pub enum Reader {
    Utf8 { buffer: Vec<u8>, offset: usize, surrogate: i32 },
    Latin1 { buffer: Vec<u8> },
    Ascii { buffer: Vec<u8> },
    /// `InputStreamReader` with a Java charset (`UTF-16BE`, `UTF-16LE`, `windows-1252`, ...).
    Java { encoding: String, pending: Vec<u8>, started: bool },
}

impl Reader {
    pub fn utf8() -> Reader {
        Reader::Utf8 { buffer: vec![0; BYTE_BUFFER_SIZE], offset: 0, surrogate: -1 }
    }

    pub fn latin1() -> Reader {
        Reader::Latin1 { buffer: vec![0; BYTE_BUFFER_SIZE] }
    }

    pub fn ascii() -> Reader {
        Reader::Ascii { buffer: vec![0; BYTE_BUFFER_SIZE] }
    }

    pub fn java(encoding: &str) -> Reader {
        Reader::Java { encoding: encoding.to_string(), pending: Vec::new(), started: false }
    }

    pub fn read(&mut self, ch: &mut [u16], offset: usize, length: usize, input: &mut Rewindable, chunks: bool) -> Result<i32, ReadError> {
        match self {
            Reader::Utf8 { buffer, offset: f_offset, surrogate } => utf8_read(buffer, f_offset, surrogate, ch, offset, length, input, chunks),
            Reader::Latin1 { buffer } => single_read(buffer, false, ch, offset, length, input, chunks),
            Reader::Ascii { buffer } => single_read(buffer, true, ch, offset, length, input, chunks),
            Reader::Java { encoding, pending, started } => java_read(encoding, pending, started, ch, offset, length, input, chunks),
        }
    }
}

/// `Latin1Reader.read` / `ASCIIReader.read`.
fn single_read(buffer: &mut [u8], ascii: bool, ch: &mut [u16], offset: usize, length: usize, input: &mut Rewindable, chunks: bool) -> Result<i32, ReadError> {
    let length = length.min(buffer.len());
    let count = input.read(buffer, 0, length, chunks);
    for i in 0..count.max(0) as usize {
        let b = buffer[i];
        if ascii && b >= 0x80 {
            return Err(ReadError::Malformed { key: "InvalidASCII", args: vec![(b as i32).to_string()] });
        }
        ch[offset + i] = b as u16;
    }
    Ok(count)
}

#[allow(clippy::too_many_arguments)]
fn utf8_read(f_buffer: &mut [u8], f_offset: &mut usize, f_surrogate: &mut i32, ch: &mut [u16], offset: usize, mut length: usize, input: &mut Rewindable, chunks: bool) -> Result<i32, ReadError> {
    let mut out = offset;
    let mut count: i32;
    if *f_offset == 0 {
        if length > f_buffer.len() {
            length = f_buffer.len();
        }
        if *f_surrogate != -1 {
            ch[out] = *f_surrogate as u16;
            out += 1;
            *f_surrogate = -1;
            length -= 1;
        }
        count = input.read(f_buffer, 0, length, chunks);
        if count == -1 {
            return Ok(-1);
        }
        count += (out - offset) as i32;
    } else {
        count = *f_offset as i32;
        *f_offset = 0;
    }
    let total = count as usize;
    let expected = |p: i32, c: i32| ReadError::Malformed { key: "ExpectedByte", args: vec![p.to_string(), c.to_string()] };
    let invalid = |p: i32, c: i32| ReadError::Malformed { key: "InvalidByte", args: vec![p.to_string(), c.to_string()] };
    let mut i = 0usize;
    while i < total {
        let b = f_buffer[i];
        if b < 0x80 {
            ch[out] = b as u16;
            out += 1;
            i += 1;
        } else {
            break;
        }
    }
    while i < total {
        let byte1 = f_buffer[i];
        if byte1 < 0x80 {
            ch[out] = byte1 as u16;
            out += 1;
            i += 1;
            continue;
        }
        let b0 = byte1 as i32;
        macro_rules! next_byte {
            ($pos:expr, $n:expr, $keep:expr) => {{
                i += 1;
                if i < total {
                    f_buffer[i] as i32
                } else {
                    let b = input.read1();
                    if b == -1 {
                        if out > offset {
                            let keep: &[i32] = $keep;
                            for (k, v) in keep.iter().enumerate() {
                                f_buffer[k] = *v as u8;
                            }
                            *f_offset = keep.len();
                            return Ok((out - offset) as i32);
                        }
                        return Err(expected($pos, $n));
                    }
                    count += 1;
                    b
                }
            }};
        }
        macro_rules! bad {
            ($cond:expr, $pos:expr, $n:expr, $keep:expr) => {
                if $cond {
                    if out > offset {
                        let keep: &[i32] = $keep;
                        for (k, v) in keep.iter().enumerate() {
                            f_buffer[k] = *v as u8;
                        }
                        *f_offset = keep.len();
                        return Ok((out - offset) as i32);
                    }
                    return Err(invalid($pos, $n));
                }
            };
        }
        if (b0 & 0xE0) == 0xC0 {
            let b1 = next_byte!(2, 2, &[b0]);
            bad!((b1 & 0xC0) != 0x80, 2, 2, &[b0, b1]);
            ch[out] = (((b0 << 6) & 0x07C0) | (b1 & 0x003F)) as u16;
            out += 1;
            count -= 1;
            i += 1;
            continue;
        }
        if (b0 & 0xF0) == 0xE0 {
            let b1 = next_byte!(2, 3, &[b0]);
            bad!((b1 & 0xC0) != 0x80 || (b0 == 0xED && b1 >= 0xA0) || ((b0 & 0x0F) == 0 && (b1 & 0x20) == 0), 2, 3, &[b0, b1]);
            let b2 = next_byte!(3, 3, &[b0, b1]);
            bad!((b2 & 0xC0) != 0x80, 3, 3, &[b0, b1, b2]);
            ch[out] = (((b0 << 12) & 0xF000) | ((b1 << 6) & 0x0FC0) | (b2 & 0x003F)) as u16;
            out += 1;
            count -= 2;
            i += 1;
            continue;
        }
        if (b0 & 0xF8) == 0xF0 {
            let b1 = next_byte!(2, 4, &[b0]);
            bad!((b1 & 0xC0) != 0x80 || ((b1 & 0x30) == 0 && (b0 & 0x07) == 0), 2, 4, &[b0, b1]);
            let b2 = next_byte!(3, 4, &[b0, b1]);
            bad!((b2 & 0xC0) != 0x80, 3, 4, &[b0, b1, b2]);
            let b3 = next_byte!(4, 4, &[b0, b1, b2]);
            bad!((b3 & 0xC0) != 0x80, 4, 4, &[b0, b1, b2, b3]);
            let uuuuu = ((b0 << 2) & 0x001C) | ((b1 >> 4) & 0x0003);
            if uuuuu > 0x10 {
                return Err(ReadError::Malformed { key: "InvalidHighSurrogate", args: vec![format!("{uuuuu:x}")] });
            }
            let wwww = uuuuu - 1;
            let zzzz = b1 & 0x000F;
            let yyyyyy = b2 & 0x003F;
            let xxxxxx = b3 & 0x003F;
            let hs = 0xD800 | ((wwww << 6) & 0x03C0) | (zzzz << 2) | (yyyyyy >> 4);
            let ls = 0xDC00 | ((yyyyyy << 6) & 0x03C0) | xxxxxx;
            ch[out] = hs as u16;
            out += 1;
            count -= 2;
            if count as usize <= length {
                ch[out] = ls as u16;
                out += 1;
            } else {
                *f_surrogate = ls;
                count -= 1;
            }
            i += 1;
            continue;
        }
        bad!(true, 1, 1, &[b0]);
    }
    Ok(count)
}

/// `InputStreamReader.read` for the charsets Xerces leaves to Java, decoding what one stream read returns.
#[allow(clippy::too_many_arguments)]
fn java_read(encoding: &str, pending: &mut Vec<u8>, started: &mut bool, ch: &mut [u16], offset: usize, length: usize, input: &mut Rewindable, chunks: bool) -> Result<i32, ReadError> {
    let enc = encoding.to_ascii_uppercase();
    let unit = if enc.starts_with("UTF-16") || enc.starts_with("UNICODE") { 2 } else { 1 };
    let mut produced = 0usize;
    loop {
        while pending.len() >= unit && produced < length {
            let c = if unit == 2 {
                let (a, b) = (pending[0] as u16, pending[1] as u16);
                if enc.contains("LE") { a | (b << 8) } else { (a << 8) | b }
            } else {
                single_byte(&enc, pending[0])
            };
            pending.drain(..unit);
            if !*started {
                *started = true;
                if unit == 2 && c == 0xFEFF {
                    continue;
                }
            }
            ch[offset + produced] = c;
            produced += 1;
        }
        if produced > 0 {
            return Ok(produced as i32);
        }
        let mut buf = vec![0u8; 8192];
        let n = input.read(&mut buf, 0, (length * unit).clamp(1, 8192), chunks);
        if n == -1 {
            return if pending.is_empty() { Ok(-1) } else { Err(ReadError::CharConversion) };
        }
        pending.extend_from_slice(&buf[..n as usize]);
    }
}

fn single_byte(enc: &str, b: u8) -> u16 {
    const CP1252: [u16; 32] = [
        0x20AC, 0xFFFD, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0xFFFD, 0x017D, 0xFFFD,
        0xFFFD, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0xFFFD, 0x017E, 0x0178,
    ];
    if (enc == "CP1252" || enc == "WINDOWS-1252") && (0x80..0xA0).contains(&b) {
        CP1252[(b - 0x80) as usize]
    } else {
        b as u16
    }
}

/// Java's `Cp1252` decoder (undefined bytes become U+FFFD).
pub fn decode_cp1252(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from_u32(single_byte("CP1252", b) as u32).unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
}
