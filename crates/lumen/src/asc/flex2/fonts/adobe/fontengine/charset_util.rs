//! Port of afe.jar `com.adobe.fontengine.CharsetUtil` with the aglj40 (ICU4J 4.0) charsets the name and cmap tables use.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The decode tables of the stateless aglj40 charsets, dumped from the jar.
static TABLES: &[u8] = include_bytes!("charset_tables.bin");

const ERR: u16 = 0xFFFF;
const MULTI: u16 = 0xFFFE;
const LEAD: u16 = 0xFFFD;

pub struct TableCharset {
    pub name: String,
    single: Vec<u16>,
    leads: HashMap<u8, Vec<u16>>,
    multi: HashMap<u32, Vec<u16>>,
}

/// A `java.nio.charset.Charset` from `CharsetICU.forNameICU`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Charset {
    Utf16Be,
    MacArabic,
    MacHebrew,
    Table(usize),
}

fn tables() -> &'static Vec<TableCharset> {
    static T: OnceLock<Vec<TableCharset>> = OnceLock::new();
    T.get_or_init(|| {
        let b = TABLES;
        let mut p = 0usize;
        let u8_ = |p: &mut usize| {
            let v = b[*p];
            *p += 1;
            v
        };
        let u16_ = |p: &mut usize| {
            let v = (b[*p] as u16) << 8 | b[*p + 1] as u16;
            *p += 2;
            v
        };
        let n = u8_(&mut p);
        let mut out = Vec::new();
        for _ in 0..n {
            let l = u8_(&mut p) as usize;
            let name = String::from_utf8(b[p..p + l].to_vec()).unwrap();
            p += l;
            let single: Vec<u16> = (0..256).map(|_| u16_(&mut p)).collect();
            let nl = u8_(&mut p);
            let mut leads = HashMap::new();
            for _ in 0..nl {
                let lb = u8_(&mut p);
                leads.insert(lb, (0..256).map(|_| u16_(&mut p)).collect());
            }
            let nm = u16_(&mut p);
            let mut multi = HashMap::new();
            for _ in 0..nm {
                let key = (u16_(&mut p) as u32) << 16 | u16_(&mut p) as u32;
                let k = u8_(&mut p) as usize;
                multi.insert(key, (0..k).map(|_| u16_(&mut p)).collect());
            }
            out.push(TableCharset { name, single, leads, multi });
        }
        out
    })
}

thread_local! {
    /// `UConverterDataMacArabic/Hebrew.contextDirectionToUnicode`: aglj40 keeps it in the cached charset, so it lives for the
    /// whole JVM and carries over from one decode to the next.
    static MAC_ARABIC_CONTEXT: Cell<i32> = const { Cell::new(65534) };
    static MAC_HEBREW_CONTEXT: Cell<i32> = const { Cell::new(65534) };
}

pub struct CharsetUtil;

impl CharsetUtil {
    /// `forNameICU`; every name afe asks for exists in aglj40.
    pub fn for_name_icu(name: Option<&str>) -> Option<Charset> {
        let name = name?;
        match name {
            "UTF-16BE" => return Some(Charset::Utf16Be),
            "MacArabic" => return Some(Charset::MacArabic),
            "MacHebrew" => return Some(Charset::MacHebrew),
            _ => {}
        }
        tables().iter().position(|t| t.name == name).map(Charset::Table)
    }

    /// Starts a fresh JVM as far as the MacArabic/MacHebrew decoders' shared state goes.
    pub fn reset_global_state() {
        MAC_ARABIC_CONTEXT.with(|c| c.set(65534));
        MAC_HEBREW_CONTEXT.with(|c| c.set(65534));
    }

    /// `Name.bytesToUnicode(bytes, 0, n, charset)` through `CharsetUtil.decodeLoop`: `None` where Java throws (the caller
    /// skips the record), else the UTF-16 units, including what its overflow retries drop.
    pub fn bytes_to_unicode(bytes: &[u8], cs: Charset) -> Option<Vec<u16>> {
        match cs {
            Charset::Utf16Be => decode_utf16be(bytes),
            Charset::Table(i) => decode_table(&tables()[i], bytes),
            Charset::MacArabic => Some(decode_bidi(bytes, &MAC_ARABIC_TO_U, true)),
            Charset::MacHebrew => Some(decode_bidi(bytes, &MAC_HEBREW_TO_U, false)),
        }
    }

    /// `encoder.canEncode(c)`; the encoders are taken as the inverse of the decode tables (first byte sequence wins).
    pub fn can_encode(cs: Charset, c: u16) -> bool {
        Self::encode(cs, &[c]).is_some()
    }

    /// Encodes one character (one or two units) the way `Cmap.getNonUnicodeMapping` needs it.
    pub fn encode(cs: Charset, u: &[u16]) -> Option<Vec<u8>> {
        match cs {
            Charset::Utf16Be => {
                if u.len() == 1 && (0xD800..0xE000).contains(&u[0]) {
                    return None;
                }
                Some(u.iter().flat_map(|c| [(c >> 8) as u8, *c as u8]).collect())
            }
            Charset::MacArabic | Charset::MacHebrew => {
                if u.len() != 1 {
                    return None;
                }
                let t = if cs == Charset::MacArabic { &MAC_ARABIC_TO_U } else { &MAC_HEBREW_TO_U };
                t.iter().position(|e| e.0 == u[0]).map(|i| vec![i as u8])
            }
            Charset::Table(i) => {
                if u.len() != 1 {
                    return None;
                }
                reverse(i).get(&u[0]).cloned()
            }
        }
    }

    /// `decodeLoopNoExceptions` of a two-byte buffer into a one-char buffer (`Cmap.generateRequiredCmap`): `Err` on a
    /// coding error, else the char written, if any.
    pub fn decode_first_unit(cs: Charset, b: &[u8; 2]) -> Result<Option<u16>, ()> {
        match cs {
            Charset::Utf16Be => {
                let c = (b[0] as u16) << 8 | b[1] as u16;
                if (0xD800..0xE000).contains(&c) {
                    return Err(());
                }
                Ok(if c == 0xFEFF { None } else { Some(c) })
            }
            Charset::MacArabic | Charset::MacHebrew => {
                let arabic = cs == Charset::MacArabic;
                let table = if arabic { &MAC_ARABIC_TO_U } else { &MAC_HEBREW_TO_U };
                let mut d = BidiDecoder { table, arabic, to_unicode_status: 65535, char_error_buffer: Vec::new(), out: Vec::new(), cap: 1 };
                let mut pos = 0;
                d.decode(b, &mut pos, false);
                d.decode(&[], &mut 0, true);
                Ok(d.out.first().cloned())
            }
            Charset::Table(i) => {
                let t = &tables()[i];
                let x = b[0];
                let v = t.single[x as usize];
                let u = if v == LEAD {
                    let v2 = t.leads[&x][b[1] as usize];
                    if v2 == ERR {
                        return Err(());
                    }
                    units(t, v2, (x as u32) << 8 | b[1] as u32)
                } else if v == ERR {
                    return Err(());
                } else {
                    let first = units(t, v, 0x10000 | x as u32);
                    if !first.is_empty() {
                        return Ok(Some(first[0]));
                    }
                    let y = b[1];
                    let w = t.single[y as usize];
                    if w == ERR || w == LEAD {
                        return Err(());
                    }
                    units(t, w, 0x10000 | y as u32)
                };
                Ok(u.first().cloned())
            }
        }
    }

    /// `new String(bytes, name)` with the JDK charsets, approximated by the ICU tables for the table charsets.
    pub fn decode_replacing(name: &str, bytes: &[u8]) -> Option<String> {
        let t = match Self::for_name_icu(Some(name))? {
            Charset::Table(i) => &tables()[i],
            _ => return None,
        };
        let mut out = Vec::new();
        let mut pos = 0;
        while pos < bytes.len() {
            let x = bytes[pos];
            let v = t.single[x as usize];
            if v == LEAD && pos + 1 < bytes.len() {
                let v2 = t.leads[&x][bytes[pos + 1] as usize];
                if v2 != ERR {
                    out.extend(units(t, v2, (x as u32) << 8 | bytes[pos + 1] as u32));
                    pos += 2;
                    continue;
                }
            }
            if v == ERR || v == LEAD {
                out.push(0xFFFD);
            } else {
                out.extend(units(t, v, 0x10000 | x as u32));
            }
            pos += 1;
        }
        Some(String::from_utf16_lossy(&out))
    }
}

fn reverse(i: usize) -> &'static HashMap<u16, Vec<u8>> {
    static R: OnceLock<Vec<HashMap<u16, Vec<u8>>>> = OnceLock::new();
    &R.get_or_init(|| {
        tables()
            .iter()
            .map(|t| {
                let mut m = HashMap::new();
                for b in 0..256usize {
                    let v = t.single[b];
                    if v != ERR && v != LEAD {
                        let u = units(t, v, 0x10000 | b as u32);
                        if u.len() == 1 {
                            m.entry(u[0]).or_insert_with(|| vec![b as u8]);
                        }
                    }
                }
                let mut leads: Vec<&u8> = t.leads.keys().collect();
                leads.sort();
                for &l in leads {
                    for tr in 0..256usize {
                        let v = t.leads[&l][tr];
                        if v != ERR {
                            let u = units(t, v, (l as u32) << 8 | tr as u32);
                            if u.len() == 1 {
                                m.entry(u[0]).or_insert_with(|| vec![l, tr as u8]);
                            }
                        }
                    }
                }
                m
            })
            .collect()
    })[i]
}

fn units(t: &TableCharset, v: u16, key: u32) -> Vec<u16> {
    if v == MULTI {
        t.multi[&key].clone()
    } else {
        vec![v]
    }
}

/// `CharsetUTF16` decoding big-endian: a leading FE FF is dropped, unpaired surrogates and odd lengths fail.
fn decode_utf16be(b: &[u8]) -> Option<Vec<u16>> {
    if b.len() % 2 != 0 {
        return None;
    }
    let mut cu: Vec<u16> = b.chunks(2).map(|c| (c[0] as u16) << 8 | c[1] as u16).collect();
    if cu.first() == Some(&0xFEFF) {
        cu.remove(0);
    }
    let mut i = 0;
    while i < cu.len() {
        let c = cu[i];
        if (0xD800..0xDC00).contains(&c) {
            if i + 1 < cu.len() && (0xDC00..0xE000).contains(&cu[i + 1]) {
                i += 2;
                continue;
            }
            return None;
        }
        if (0xDC00..0xE000).contains(&c) {
            return None;
        }
        i += 1;
    }
    Some(cu)
}

/// The stateless MBCS/SBCS decoders: a sequence whose units overflow the buffer loses them and the decode restarts after
/// it with twice the room; a buffer that fills exactly ends the string there.
fn decode_table(t: &TableCharset, b: &[u8]) -> Option<Vec<u16>> {
    let n = b.len();
    let mut pos = 0;
    let mut cap = n;
    loop {
        let mut out: Vec<u16> = Vec::new();
        let mut overflowed = false;
        while pos < n {
            if out.len() == cap {
                break;
            }
            let x = b[pos];
            let v = t.single[x as usize];
            let u = if v == LEAD {
                if pos + 1 >= n {
                    return None;
                }
                let tr = b[pos + 1];
                let v2 = t.leads[&x][tr as usize];
                if v2 == ERR {
                    return None;
                }
                pos += 2;
                units(t, v2, (x as u32) << 8 | tr as u32)
            } else if v == ERR {
                return None;
            } else {
                pos += 1;
                units(t, v, 0x10000 | x as u32)
            };
            let room = cap - out.len();
            out.extend_from_slice(&u[..u.len().min(room)]);
            if u.len() > room {
                overflowed = true;
                break;
            }
        }
        if overflowed {
            if cap == 0 {
                return Some(out);
            }
            cap *= 2;
            continue;
        }
        return Some(out);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Cr {
    Underflow,
    Overflow,
}

/// One `CharsetDecoderMacArabic/Hebrew` and the `CharsetDecoderICU`/`java.nio` machinery around its `decodeLoop`.
struct BidiDecoder<'a> {
    table: &'a [(u16, u16, i32); 256],
    arabic: bool,
    to_unicode_status: i32,
    char_error_buffer: Vec<u16>,
    out: Vec<u16>,
    cap: usize,
}

impl<'a> BidiDecoder<'a> {
    fn context(&self) -> i32 {
        if self.arabic {
            MAC_ARABIC_CONTEXT.with(|c| c.get())
        } else {
            MAC_HEBREW_CONTEXT.with(|c| c.get())
        }
    }

    fn set_context(&self, v: i32) {
        if self.arabic {
            MAC_ARABIC_CONTEXT.with(|c| c.set(v))
        } else {
            MAC_HEBREW_CONTEXT.with(|c| c.set(v))
        }
    }

    fn write(&mut self, c: i32) -> Cr {
        if self.out.len() < self.cap {
            self.out.push(c as u16);
            Cr::Underflow
        } else {
            self.char_error_buffer.push(c as u16);
            Cr::Overflow
        }
    }

    #[allow(unused_assignments)]
    fn decode_loop(&mut self, src: &[u8], pos: &mut usize, flush: bool) -> Cr {
        let mut cr = Cr::Underflow;
        let strong = if self.arabic { 2 } else { 1 };
        while *pos < src.len() {
            if self.out.len() >= self.cap {
                cr = Cr::Overflow;
                break;
            }
            let source_char = src[*pos] as i32;
            *pos += 1;
            let (u0, kind, dir) = self.table[source_char as usize];
            let mut target_uni_char = u0 as i32;
            match kind {
                0 => {
                    if self.to_unicode_status != 65535 {
                        if self.context() == dir {
                            let s = self.to_unicode_status;
                            cr = self.write(s);
                            self.to_unicode_status = 65535;
                        } else {
                            let tmp = if self.context() == 0 { 8237 } else { 8238 };
                            cr = self.write(tmp);
                            let s = self.to_unicode_status;
                            cr = self.write(s);
                            self.to_unicode_status = 65535;
                            cr = self.write(8236);
                        }
                    } else if self.context() == 8237 || self.context() == 8238 {
                        cr = self.write(8236);
                    }
                    if !self.arabic {
                        match source_char {
                            129 => {
                                cr = self.write(1522);
                                target_uni_char = 1463;
                            }
                            192 => {
                                cr = self.write(63594);
                                cr = self.write(1500);
                                target_uni_char = 1465;
                            }
                            222 => {
                                cr = self.write(1464);
                                target_uni_char = 63615;
                            }
                            _ => {}
                        }
                    }
                    cr = self.write(target_uni_char);
                    if dir == 0 || dir == strong {
                        self.set_context(dir & 0xFFFF);
                    } else {
                        self.set_context(65534);
                    }
                }
                0x202D => {
                    if self.to_unicode_status != 65535 {
                        if self.context() == 0 {
                            cr = self.write(8237);
                            let s = self.to_unicode_status;
                            cr = self.write(s);
                            self.to_unicode_status = 65535;
                        } else {
                            cr = self.write(8238);
                            let s = self.to_unicode_status;
                            cr = self.write(s);
                            self.to_unicode_status = 65535;
                            cr = self.write(8236);
                            cr = self.write(8237);
                        }
                        cr = self.write(target_uni_char);
                        self.set_context(8237);
                        continue;
                    }
                    if self.context() == 0 {
                        self.to_unicode_status = target_uni_char;
                        continue;
                    }
                    if self.context() == 8237 {
                        cr = self.write(target_uni_char);
                        continue;
                    }
                    if self.context() == 8238 {
                        cr = self.write(8236);
                    }
                    cr = self.write(8237);
                    cr = self.write(target_uni_char);
                    self.set_context(8237);
                }
                _ => {
                    if self.to_unicode_status != 65535 {
                        if self.context() == strong {
                            cr = self.write(8238);
                            let s = self.to_unicode_status;
                            cr = self.write(s);
                            self.to_unicode_status = 65535;
                        } else {
                            cr = self.write(8237);
                            let s = self.to_unicode_status;
                            cr = self.write(s);
                            self.to_unicode_status = 65535;
                            cr = self.write(8236);
                            cr = self.write(8238);
                        }
                        cr = self.write(target_uni_char);
                        self.set_context(8238);
                        continue;
                    }
                    if self.context() == 1 {
                        // Java tests `sourceChar < -80 || sourceChar > -71` on a value in 0..255, which always holds.
                        if source_char < -80 || source_char > -71 {
                            self.to_unicode_status = target_uni_char;
                            continue;
                        }
                        cr = self.write(8238);
                        cr = self.write(target_uni_char);
                        self.set_context(8238);
                        continue;
                    }
                    if self.context() == 8238 {
                        cr = self.write(target_uni_char);
                        continue;
                    }
                    if self.context() == 8237 {
                        cr = self.write(8236);
                    }
                    cr = self.write(8238);
                    cr = self.write(target_uni_char);
                    self.set_context(8238);
                }
            }
        }
        if cr != Cr::Overflow && flush && *pos >= src.len() {
            if self.to_unicode_status != 65535 {
                let tmp = if self.context() == 0 { 8237 } else { 8238 };
                cr = self.write(tmp);
                let s = self.to_unicode_status;
                cr = self.write(s);
                cr = self.write(8236);
            } else if self.context() == 8237 || self.context() == 8238 {
                cr = self.write(8236);
            }
        }
        cr
    }

    /// `CharsetDecoderICU.decode(source, target, null, flush)`.
    fn decode(&mut self, src: &[u8], pos: &mut usize, flush: bool) -> Cr {
        if !self.char_error_buffer.is_empty() {
            let pending = std::mem::take(&mut self.char_error_buffer);
            let mut i = 0;
            while i < pending.len() {
                if self.out.len() >= self.cap {
                    self.char_error_buffer = pending[i..].to_vec();
                    return Cr::Overflow;
                }
                self.out.push(pending[i]);
                i += 1;
            }
        }
        if !flush && *pos >= src.len() {
            return Cr::Underflow;
        }
        loop {
            let cr = self.decode_loop(src, pos, flush);
            if cr == Cr::Underflow {
                if *pos < src.len() {
                    continue;
                }
                if flush {
                    self.to_unicode_status = 65535;
                }
                return cr;
            }
            return cr;
        }
    }

    fn reset(&mut self) {
        self.to_unicode_status = 65535;
        self.char_error_buffer.clear();
    }
}

fn decode_bidi(b: &[u8], table: &[(u16, u16, i32); 256], arabic: bool) -> Vec<u16> {
    let mut d = BidiDecoder { table, arabic, to_unicode_status: 65535, char_error_buffer: Vec::new(), out: Vec::new(), cap: b.len() };
    let mut pos = 0;
    loop {
        d.out = Vec::new();
        d.decode(b, &mut pos, false);
        let r = d.decode(&[], &mut 0, true);
        d.reset();
        if r == Cr::Overflow {
            if d.cap == 0 {
                // Java retries with a zero-sized buffer forever here.
                return d.out;
            }
            d.cap *= 2;
            continue;
        }
        return d.out;
    }
}

const MAC_ARABIC_TO_U: [(u16, u16, i32); 256] = [
    (0x0000, 0x0000, 9), (0x0001, 0x0000, 9), (0x0002, 0x0000, 9), (0x0003, 0x0000, 9), (0x0004, 0x0000, 9), (0x0005, 0x0000, 9), (0x0006, 0x0000, 9), (0x0007, 0x0000, 9),
    (0x0008, 0x0000, 9), (0x0009, 0x0000, 11), (0x000a, 0x0000, 10), (0x000b, 0x0000, 11), (0x000c, 0x0000, 12), (0x000d, 0x0000, 10), (0x000e, 0x0000, 9), (0x000f, 0x0000, 9),
    (0x0010, 0x0000, 9), (0x0011, 0x0000, 9), (0x0012, 0x0000, 9), (0x0013, 0x0000, 9), (0x0014, 0x0000, 9), (0x0015, 0x0000, 9), (0x0016, 0x0000, 9), (0x0017, 0x0000, 9),
    (0x0018, 0x0000, 9), (0x0019, 0x0000, 9), (0x001a, 0x0000, 9), (0x001b, 0x0000, 9), (0x001c, 0x0000, 10), (0x001d, 0x0000, 10), (0x001e, 0x0000, 10), (0x001f, 0x0000, 11),
    (0x0020, 0x202d, 12), (0x0021, 0x202d, 13), (0x0022, 0x202d, 13), (0x0023, 0x202d, 5), (0x0024, 0x202d, 5), (0x0025, 0x202d, 5), (0x0026, 0x202d, 13), (0x0027, 0x202d, 13),
    (0x0028, 0x202d, 13), (0x0029, 0x202d, 13), (0x002a, 0x202d, 13), (0x002b, 0x202d, 4), (0x002c, 0x202d, 7), (0x002d, 0x202d, 4), (0x002e, 0x202d, 7), (0x002f, 0x202d, 7),
    (0x0030, 0x0000, 3), (0x0031, 0x0000, 3), (0x0032, 0x0000, 3), (0x0033, 0x0000, 3), (0x0034, 0x0000, 3), (0x0035, 0x0000, 3), (0x0036, 0x0000, 3), (0x0037, 0x0000, 3),
    (0x0038, 0x0000, 3), (0x0039, 0x0000, 3), (0x003a, 0x202d, 7), (0x003b, 0x202d, 13), (0x003c, 0x202d, 13), (0x003d, 0x202d, 13), (0x003e, 0x202d, 13), (0x003f, 0x202d, 13),
    (0x0040, 0x0000, 13), (0x0041, 0x0000, 0), (0x0042, 0x0000, 0), (0x0043, 0x0000, 0), (0x0044, 0x0000, 0), (0x0045, 0x0000, 0), (0x0046, 0x0000, 0), (0x0047, 0x0000, 0),
    (0x0048, 0x0000, 0), (0x0049, 0x0000, 0), (0x004a, 0x0000, 0), (0x004b, 0x0000, 0), (0x004c, 0x0000, 0), (0x004d, 0x0000, 0), (0x004e, 0x0000, 0), (0x004f, 0x0000, 0),
    (0x0050, 0x0000, 0), (0x0051, 0x0000, 0), (0x0052, 0x0000, 0), (0x0053, 0x0000, 0), (0x0054, 0x0000, 0), (0x0055, 0x0000, 0), (0x0056, 0x0000, 0), (0x0057, 0x0000, 0),
    (0x0058, 0x0000, 0), (0x0059, 0x0000, 0), (0x005a, 0x0000, 0), (0x005b, 0x202d, 13), (0x005c, 0x202d, 13), (0x005d, 0x202d, 13), (0x005e, 0x202d, 13), (0x005f, 0x202d, 13),
    (0x0060, 0x0000, 13), (0x0061, 0x0000, 0), (0x0062, 0x0000, 0), (0x0063, 0x0000, 0), (0x0064, 0x0000, 0), (0x0065, 0x0000, 0), (0x0066, 0x0000, 0), (0x0067, 0x0000, 0),
    (0x0068, 0x0000, 0), (0x0069, 0x0000, 0), (0x006a, 0x0000, 0), (0x006b, 0x0000, 0), (0x006c, 0x0000, 0), (0x006d, 0x0000, 0), (0x006e, 0x0000, 0), (0x006f, 0x0000, 0),
    (0x0070, 0x0000, 0), (0x0071, 0x0000, 0), (0x0072, 0x0000, 0), (0x0073, 0x0000, 0), (0x0074, 0x0000, 0), (0x0075, 0x0000, 0), (0x0076, 0x0000, 0), (0x0077, 0x0000, 0),
    (0x0078, 0x0000, 0), (0x0079, 0x0000, 0), (0x007a, 0x0000, 0), (0x007b, 0x202d, 13), (0x007c, 0x202d, 13), (0x007d, 0x202d, 13), (0x007e, 0x0000, 13), (0x007f, 0x0000, 9),
    (0x00c4, 0x0000, 0), (0x00a0, 0x202e, 7), (0x00c7, 0x0000, 0), (0x00c9, 0x0000, 0), (0x00d1, 0x0000, 0), (0x00d6, 0x0000, 0), (0x00dc, 0x0000, 0), (0x00e1, 0x0000, 0),
    (0x00e0, 0x0000, 0), (0x00e2, 0x0000, 0), (0x00e4, 0x0000, 0), (0x06ba, 0x0000, 2), (0x00ab, 0x202e, 13), (0x00e7, 0x0000, 0), (0x00e9, 0x0000, 0), (0x00e8, 0x0000, 0),
    (0x00ea, 0x0000, 0), (0x00eb, 0x0000, 0), (0x00ed, 0x0000, 0), (0x2026, 0x202e, 13), (0x00ee, 0x0000, 0), (0x00ef, 0x0000, 0), (0x00f1, 0x0000, 0), (0x00f3, 0x0000, 0),
    (0x00bb, 0x202e, 13), (0x00f4, 0x0000, 0), (0x00f6, 0x0000, 0), (0x00f7, 0x202e, 13), (0x00fa, 0x0000, 0), (0x00f9, 0x0000, 0), (0x00fb, 0x0000, 0), (0x00fc, 0x0000, 0),
    (0x0020, 0x202e, 12), (0x0021, 0x202e, 13), (0x0022, 0x202e, 13), (0x0023, 0x202e, 5), (0x0024, 0x202e, 5), (0x066a, 0x0000, 5), (0x0026, 0x202e, 13), (0x0027, 0x202e, 13),
    (0x0028, 0x202e, 13), (0x0029, 0x202e, 13), (0x002a, 0x202e, 13), (0x002b, 0x202e, 4), (0x060c, 0x0000, 7), (0x002d, 0x202e, 4), (0x002e, 0x202e, 7), (0x002f, 0x202e, 7),
    (0x0660, 0x202e, 6), (0x0661, 0x202e, 6), (0x0662, 0x202e, 6), (0x0663, 0x202e, 6), (0x0664, 0x202e, 6), (0x0665, 0x202e, 6), (0x0666, 0x202e, 6), (0x0667, 0x202e, 6),
    (0x0668, 0x202e, 6), (0x0669, 0x202e, 6), (0x003a, 0x202e, 7), (0x061b, 0x0000, 2), (0x003c, 0x202e, 13), (0x003d, 0x202e, 13), (0x003e, 0x202e, 13), (0x061f, 0x0000, 2),
    (0x274a, 0x202e, 13), (0x0621, 0x0000, 2), (0x0622, 0x0000, 2), (0x0623, 0x0000, 2), (0x0624, 0x0000, 2), (0x0625, 0x0000, 2), (0x0626, 0x0000, 2), (0x0627, 0x0000, 2),
    (0x0628, 0x0000, 2), (0x0629, 0x0000, 2), (0x062a, 0x0000, 2), (0x062b, 0x0000, 2), (0x062c, 0x0000, 2), (0x062d, 0x0000, 2), (0x062e, 0x0000, 2), (0x062f, 0x0000, 2),
    (0x0630, 0x0000, 2), (0x0631, 0x0000, 2), (0x0632, 0x0000, 2), (0x0633, 0x0000, 2), (0x0634, 0x0000, 2), (0x0635, 0x0000, 2), (0x0636, 0x0000, 2), (0x0637, 0x0000, 2),
    (0x0638, 0x0000, 2), (0x0639, 0x0000, 2), (0x063a, 0x0000, 2), (0x005b, 0x202e, 13), (0x005c, 0x202e, 13), (0x005d, 0x202e, 13), (0x005e, 0x202e, 13), (0x005f, 0x202e, 13),
    (0x0640, 0x0000, 2), (0x0641, 0x0000, 2), (0x0642, 0x0000, 2), (0x0643, 0x0000, 2), (0x0644, 0x0000, 2), (0x0645, 0x0000, 2), (0x0646, 0x0000, 2), (0x0647, 0x0000, 2),
    (0x0648, 0x0000, 2), (0x0649, 0x0000, 2), (0x064a, 0x0000, 2), (0x064b, 0x0000, 8), (0x064c, 0x0000, 8), (0x064d, 0x0000, 8), (0x064e, 0x0000, 8), (0x064f, 0x0000, 8),
    (0x0650, 0x0000, 8), (0x0651, 0x0000, 8), (0x0652, 0x0000, 8), (0x067e, 0x0000, 2), (0x0679, 0x0000, 2), (0x0686, 0x0000, 2), (0x06d5, 0x0000, 2), (0x06a4, 0x0000, 2),
    (0x06af, 0x0000, 2), (0x0688, 0x0000, 2), (0x0691, 0x0000, 2), (0x007b, 0x202e, 13), (0x007c, 0x202e, 13), (0x007d, 0x202e, 13), (0x0698, 0x0000, 2), (0x06d2, 0x0000, 2),
];
const MAC_HEBREW_TO_U: [(u16, u16, i32); 256] = [
    (0x0000, 0x0000, 9), (0x0001, 0x0000, 9), (0x0002, 0x0000, 9), (0x0003, 0x0000, 9), (0x0004, 0x0000, 9), (0x0005, 0x0000, 9), (0x0006, 0x0000, 9), (0x0007, 0x0000, 9),
    (0x0008, 0x0000, 9), (0x0009, 0x0000, 11), (0x000a, 0x0000, 10), (0x000b, 0x0000, 11), (0x000c, 0x0000, 12), (0x000d, 0x0000, 10), (0x000e, 0x0000, 9), (0x000f, 0x0000, 9),
    (0x0010, 0x0000, 9), (0x0011, 0x0000, 9), (0x0012, 0x0000, 9), (0x0013, 0x0000, 9), (0x0014, 0x0000, 9), (0x0015, 0x0000, 9), (0x0016, 0x0000, 9), (0x0017, 0x0000, 9),
    (0x0018, 0x0000, 9), (0x0019, 0x0000, 9), (0x001a, 0x0000, 9), (0x001b, 0x0000, 9), (0x001c, 0x0000, 10), (0x001d, 0x0000, 10), (0x001e, 0x0000, 10), (0x001f, 0x0000, 11),
    (0x0020, 0x202d, 12), (0x0021, 0x202d, 13), (0x0022, 0x202d, 13), (0x0023, 0x202d, 5), (0x0024, 0x202d, 5), (0x0025, 0x202d, 5), (0x0026, 0x0000, 13), (0x0027, 0x202d, 13),
    (0x0028, 0x202d, 13), (0x0029, 0x202d, 13), (0x002a, 0x202d, 13), (0x002b, 0x202d, 4), (0x002c, 0x202d, 7), (0x002d, 0x202d, 4), (0x002e, 0x202d, 7), (0x002f, 0x202d, 7),
    (0x0030, 0x0000, 3), (0x0031, 0x0000, 3), (0x0032, 0x0000, 3), (0x0033, 0x0000, 3), (0x0034, 0x0000, 3), (0x0035, 0x0000, 3), (0x0036, 0x0000, 3), (0x0037, 0x0000, 3),
    (0x0038, 0x0000, 3), (0x0039, 0x0000, 3), (0x003a, 0x202d, 7), (0x003b, 0x202d, 13), (0x003c, 0x202d, 13), (0x003d, 0x202d, 13), (0x003e, 0x202d, 13), (0x003f, 0x202d, 13),
    (0x0040, 0x0000, 13), (0x0041, 0x0000, 0), (0x0042, 0x0000, 0), (0x0043, 0x0000, 0), (0x0044, 0x0000, 0), (0x0045, 0x0000, 0), (0x0046, 0x0000, 0), (0x0047, 0x0000, 0),
    (0x0048, 0x0000, 0), (0x0049, 0x0000, 0), (0x004a, 0x0000, 0), (0x004b, 0x0000, 0), (0x004c, 0x0000, 0), (0x004d, 0x0000, 0), (0x004e, 0x0000, 0), (0x004f, 0x0000, 0),
    (0x0050, 0x0000, 0), (0x0051, 0x0000, 0), (0x0052, 0x0000, 0), (0x0053, 0x0000, 0), (0x0054, 0x0000, 0), (0x0055, 0x0000, 0), (0x0056, 0x0000, 0), (0x0057, 0x0000, 0),
    (0x0058, 0x0000, 0), (0x0059, 0x0000, 0), (0x005a, 0x0000, 0), (0x005b, 0x202d, 13), (0x005c, 0x0000, 13), (0x005d, 0x202d, 13), (0x005e, 0x0000, 13), (0x005f, 0x0000, 13),
    (0x0060, 0x0000, 13), (0x0061, 0x0000, 0), (0x0062, 0x0000, 0), (0x0063, 0x0000, 0), (0x0064, 0x0000, 0), (0x0065, 0x0000, 0), (0x0066, 0x0000, 0), (0x0067, 0x0000, 0),
    (0x0068, 0x0000, 0), (0x0069, 0x0000, 0), (0x006a, 0x0000, 0), (0x006b, 0x0000, 0), (0x006c, 0x0000, 0), (0x006d, 0x0000, 0), (0x006e, 0x0000, 0), (0x006f, 0x0000, 0),
    (0x0070, 0x0000, 0), (0x0071, 0x0000, 0), (0x0072, 0x0000, 0), (0x0073, 0x0000, 0), (0x0074, 0x0000, 0), (0x0075, 0x0000, 0), (0x0076, 0x0000, 0), (0x0077, 0x0000, 0),
    (0x0078, 0x0000, 0), (0x0079, 0x0000, 0), (0x007a, 0x0000, 0), (0x007b, 0x202d, 13), (0x007c, 0x202d, 13), (0x007d, 0x202d, 13), (0x007e, 0x0000, 13), (0x007f, 0x0000, 9),
    (0x00c4, 0x0000, 0), (0xffff, 0x0000, -1), (0x00c7, 0x0000, 0), (0x00c9, 0x0000, 0), (0x00d1, 0x0000, 0), (0x00d6, 0x0000, 0), (0x00dc, 0x0000, 0), (0x00e1, 0x0000, 0),
    (0x00e0, 0x0000, 0), (0x00e2, 0x0000, 0), (0x00e4, 0x0000, 0), (0x00e3, 0x0000, 0), (0x00e5, 0x0000, 0), (0x00e7, 0x0000, 0), (0x00e9, 0x0000, 0), (0x00e8, 0x0000, 0),
    (0x00ea, 0x0000, 0), (0x00eb, 0x0000, 0), (0x00ed, 0x0000, 0), (0x00ec, 0x0000, 0), (0x00ee, 0x0000, 0), (0x00ef, 0x0000, 0), (0x00f1, 0x0000, 0), (0x00f3, 0x0000, 0),
    (0x00f2, 0x0000, 0), (0x00f4, 0x0000, 0), (0x00f6, 0x0000, 0), (0x00f5, 0x0000, 0), (0x00fa, 0x0000, 0), (0x00f9, 0x0000, 0), (0x00fb, 0x0000, 0), (0x00fc, 0x0000, 0),
    (0x0020, 0x202e, 12), (0x0021, 0x202e, 13), (0x0022, 0x202e, 13), (0x0023, 0x202e, 5), (0x0024, 0x202e, 5), (0x0025, 0x202e, 5), (0x20aa, 0x0000, 5), (0x0027, 0x202e, 13),
    (0x0029, 0x202e, 13), (0x0028, 0x202e, 13), (0x002a, 0x202e, 13), (0x002b, 0x202e, 4), (0x002c, 0x202e, 7), (0x002d, 0x202e, 4), (0x002e, 0x202e, 7), (0x002f, 0x202e, 7),
    (0x0030, 0x202e, 3), (0x0031, 0x202e, 3), (0x0032, 0x202e, 3), (0x0033, 0x202e, 3), (0x0034, 0x202e, 3), (0x0035, 0x202e, 3), (0x0036, 0x202e, 3), (0x0037, 0x202e, 3),
    (0x0038, 0x202e, 3), (0x0039, 0x202e, 3), (0x003a, 0x202e, 7), (0x003b, 0x202e, 13), (0x003c, 0x202e, 13), (0x003d, 0x202e, 13), (0x003e, 0x202e, 13), (0x003f, 0x202e, 13),
    (0xffff, 0x0000, -1), (0x201e, 0x202e, 13), (0xf89b, 0x0000, 0), (0xf89c, 0x0000, 0), (0xf89d, 0x0000, 0), (0xf89e, 0x0000, 0), (0x05bc, 0x0000, 8), (0xfb4b, 0x0000, 1),
    (0xfb35, 0x0000, 1), (0x2026, 0x202e, 13), (0x00a0, 0x202e, 7), (0x05b8, 0x0000, 8), (0x05b7, 0x0000, 8), (0x05b5, 0x0000, 8), (0x05b6, 0x0000, 8), (0x05b4, 0x0000, 8),
    (0x2013, 0x202e, 13), (0x2014, 0x202e, 13), (0x201c, 0x202e, 13), (0x201d, 0x202e, 13), (0x2018, 0x202e, 13), (0x2019, 0x202e, 13), (0xfb2a, 0x0000, 1), (0xfb2b, 0x0000, 1),
    (0x05bf, 0x0000, 8), (0x05b0, 0x0000, 8), (0x05b2, 0x0000, 8), (0x05b1, 0x0000, 8), (0x05bb, 0x0000, 8), (0x05b9, 0x0000, 8), (0xffff, 0x0000, -1), (0x05b3, 0x0000, 8),
    (0x05d0, 0x0000, 1), (0x05d1, 0x0000, 1), (0x05d2, 0x0000, 1), (0x05d3, 0x0000, 1), (0x05d4, 0x0000, 1), (0x05d5, 0x0000, 1), (0x05d6, 0x0000, 1), (0x05d7, 0x0000, 1),
    (0x05d8, 0x0000, 1), (0x05d9, 0x0000, 1), (0x05da, 0x0000, 1), (0x05db, 0x0000, 1), (0x05dc, 0x0000, 1), (0x05dd, 0x0000, 1), (0x05de, 0x0000, 1), (0x05df, 0x0000, 1),
    (0x05e0, 0x0000, 1), (0x05e1, 0x0000, 1), (0x05e2, 0x0000, 1), (0x05e3, 0x0000, 1), (0x05e4, 0x0000, 1), (0x05e5, 0x0000, 1), (0x05e6, 0x0000, 1), (0x05e7, 0x0000, 1),
    (0x05e8, 0x0000, 1), (0x05e9, 0x0000, 1), (0x05ea, 0x0000, 1), (0x007d, 0x202e, 13), (0x005d, 0x202e, 13), (0x007b, 0x202e, 13), (0x005b, 0x202e, 13), (0x007c, 0x202e, 13),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn full_bidi(b: &[u8], arabic: bool) {
        let table = if arabic { &MAC_ARABIC_TO_U } else { &MAC_HEBREW_TO_U };
        let mut d = BidiDecoder { table, arabic, to_unicode_status: 65535, char_error_buffer: Vec::new(), out: Vec::new(), cap: 64 };
        let mut pos = 0;
        d.decode(b, &mut pos, false);
        d.decode(&[], &mut 0, true);
    }

    fn hex(u: &Option<Vec<u16>>) -> String {
        match u {
            None => "N".to_string(),
            Some(v) => format!("={}", v.iter().map(|x| format!("{:04x}", x)).collect::<String>()),
        }
    }

    #[test]
    fn charsets_match_aglj40() {
        let path = std::env::var("OTCORE_CS_DUMP").unwrap_or_else(|_| r"S:\Desktop\Projects\Rust-FFDec\target\fonts_ws\otcore\dump\cs2.txt".to_string());
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        CharsetUtil::reset_global_state();
        let (mut total, mut bad) = (0, 0);
        let mut cs = None;
        let mut bidi = None;
        for line in text.lines() {
            let p: Vec<&str> = line.split(' ').collect();
            match p[0] {
                "CS" => {
                    cs = CharsetUtil::for_name_icu(Some(p[1]));
                    assert!(cs.is_some(), "{}", p[1]);
                    bidi = match p[1] {
                        "MacArabic" => Some(true),
                        "MacHebrew" => Some(false),
                        _ => None,
                    };
                }
                "S" => {
                    if let Some(a) = bidi {
                        full_bidi(&[p[1].parse::<u8>().unwrap()], a);
                    }
                }
                "R" => {
                    let b: Vec<u8> = (0..p[1].len() / 2).map(|i| u8::from_str_radix(&p[1][2 * i..2 * i + 2], 16).unwrap()).collect();
                    let got = hex(&CharsetUtil::bytes_to_unicode(&b, cs.unwrap()));
                    if let Some(a) = bidi {
                        full_bidi(&b, a);
                    }
                    total += 1;
                    if got != p[2] {
                        bad += 1;
                        if bad < 10 {
                            eprintln!("{:?} {} java {} rust {}", cs, p[1], p[2], got);
                        }
                    }
                }
                _ => {}
            }
        }
        eprintln!("charset strings: {} compared, {} differ", total, bad);
        assert_eq!(bad, 0);
    }
}
