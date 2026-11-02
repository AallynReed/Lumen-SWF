//! Port of JDK 17 `sun.font.TrueTypeFont` (with the `FileFont`/`PhysicalFont` state it carries).

use std::cell::RefCell;
use std::io::{Read, Seek, SeekFrom};
use std::rc::Rc;

use crate::asc::flex2::fonts::java2d::freetype_scaler::FreetypeFontScaler;
use crate::asc::flex2::fonts::java2d::java::nio::byte_buffer::{ByteBuffer, JResult};
use crate::asc::flex2::fonts::java2d::java::util::locale::Locale;
use crate::asc::flex2::fonts::java2d::sun::font::font2d::{FONT_BOLD, FONT_ITALIC};

pub const CMAP_TAG: i32 = 0x636D6170;
pub const HEAD_TAG: i32 = 0x68656164;
pub const HHEA_TAG: i32 = 0x68686561;
pub const HMTX_TAG: i32 = 0x686D7478;
pub const MAXP_TAG: i32 = 0x6D617870;
pub const NAME_TAG: i32 = 0x6E616D65;
pub const OS_2_TAG: i32 = 0x4F532F32;
pub const CFF_TAG: i32 = 0x43464620;
pub const GLYF_TAG: i32 = 0x676C7966;
pub const TTCF_TAG: i32 = 0x74746366;
pub const V1TT_TAG: i32 = 0x00010000;
pub const TRUE_TAG: i32 = 0x74727565;
pub const OTTO_TAG: i32 = 0x4f54544f;

pub const MAC_PLATFORM_ID: i16 = 1;
pub const MACROMAN_SPECIFIC_ID: i16 = 0;
pub const MACROMAN_ENGLISH_LANG: i16 = 0;
pub const MS_PLATFORM_ID: i16 = 3;
pub const ENGLISH_LOCALE_ID: i16 = 0x0409;
pub const FAMILY_NAME_ID: i32 = 1;
pub const FULL_NAME_ID: i32 = 4;
pub const POSTSCRIPT_NAME_ID: i32 = 6;
const US_LCID: i16 = 0x0409;

const TTCHEADERSIZE: i64 = 12;
const DIRECTORYHEADERSIZE: i32 = 12;
const DIRECTORYENTRYSIZE: i32 = 16;

/// `java.awt.FontFormatException`.
#[derive(Debug, Clone, PartialEq)]
pub struct FontFormatException(pub String);

#[derive(Clone, Copy)]
struct DirectoryEntry {
    tag: i32,
    offset: i32,
    length: i32,
}

/// `FontScaler` as a `FileFont` holds it: the FreeType scaler, or `NullFontScaler` after a failure.
pub enum FontScaler {
    Freetype(FreetypeFontScaler),
    Null,
}

pub struct TrueTypeFont {
    pub plat_name: String,
    pub use_java_rasterizer: bool,
    pub file_size: i32,
    pub font_index: i32,
    pub directory_count: i32,
    table_directory: Vec<DirectoryEntry>,
    pub supports_cjk: bool,
    name_locale: Option<Locale>,
    pub family_name: Option<String>,
    pub full_name: Option<String>,
    locale_family_name: Option<String>,
    locale_full_name: Option<String>,
    pub font_data_size: i32,
    pub style: i32,
    pub font_width: i32,
    pub font_weight: i32,
    language_compatible_lcids: &'static [i16],
    pub scaler: RefCell<Option<Rc<RefCell<FontScaler>>>>,
    pub file_data: RefCell<Option<Rc<Vec<u8>>>>,
}

impl TrueTypeFont {
    pub fn new(platname: &str, f_index: i32, java_rasterizer: bool) -> Result<TrueTypeFont, FontFormatException> {
        let mut f = TrueTypeFont {
            plat_name: platname.to_string(),
            use_java_rasterizer: java_rasterizer,
            file_size: 0,
            font_index: 0,
            directory_count: 1,
            table_directory: Vec::new(),
            supports_cjk: false,
            name_locale: None,
            family_name: None,
            full_name: None,
            locale_family_name: None,
            locale_full_name: None,
            font_data_size: 0,
            style: 0,
            font_width: 0,
            font_weight: 0,
            language_compatible_lcids: &[],
            scaler: RefCell::new(None),
            file_data: RefCell::new(None),
        };
        f.verify()?;
        f.init(f_index)?;
        Ok(f)
    }

    fn verify(&mut self) -> Result<(), FontFormatException> {
        match std::fs::metadata(&self.plat_name) {
            Ok(m) if m.is_file() => {
                self.file_size = m.len() as i32;
                Ok(())
            }
            Ok(_) => Err(FontFormatException(format!("java.io.FileNotFoundException: {} (Access is denied)", self.plat_name))),
            Err(e) => Err(FontFormatException(format!("java.io.FileNotFoundException: {} ({})", self.plat_name, e))),
        }
    }

    /// `readBlock(offset, length)`: null past the end of the file, truncated at it.
    fn read_block(&self, offset: i64, length: i64) -> Option<ByteBuffer> {
        if length < 0 {
            return None;
        }
        let mut length = length;
        if offset + length > self.file_size as i64 {
            if offset > self.file_size as i64 {
                return None;
            }
            length = self.file_size as i64 - offset;
        }
        let v = self.read_file(offset, length as usize).ok()?;
        Some(ByteBuffer::wrap(Rc::new(v)))
    }

    fn read_file(&self, offset: i64, length: usize) -> std::io::Result<Vec<u8>> {
        if let Some(d) = self.file_data.borrow().as_ref() {
            let s = (offset as usize).min(d.len());
            let e = (s + length).min(d.len());
            return Ok(d[s..e].to_vec());
        }
        let mut file = std::fs::File::open(&self.plat_name)?;
        file.seek(SeekFrom::Start(offset as u64))?;
        let mut v = vec![0u8; length];
        let mut n = 0;
        while n < length {
            let r = file.read(&mut v[n..])?;
            if r == 0 {
                break;
            }
            n += r;
        }
        v.truncate(n);
        Ok(v)
    }

    /// The whole file, as the scaler reads it through `readBlock`.
    pub fn file_bytes(&self) -> Option<Rc<Vec<u8>>> {
        if let Some(d) = self.file_data.borrow().as_ref() {
            return Some(d.clone());
        }
        let d = Rc::new(std::fs::read(&self.plat_name).ok()?);
        *self.file_data.borrow_mut() = Some(d.clone());
        Some(d)
    }

    fn init(&mut self, f_index: i32) -> Result<(), FontFormatException> {
        let r = self.init_tables(f_index);
        match r {
            Ok(()) => {}
            Err(Some(e)) => return Err(e),
            Err(None) => return Err(FontFormatException("java.lang.RuntimeException".into())),
        }
        if self.family_name.is_none() || self.full_name.is_none() {
            return Err(FontFormatException("Font name not found".into()));
        }
        let os2 = self.get_table_buffer(OS_2_TAG);
        self.set_style_os2(os2.as_ref());
        self.set_cjk_support(os2.as_ref());
        Ok(())
    }

    fn init_tables(&mut self, f_index: i32) -> Result<(), Option<FontFormatException>> {
        let mut header_offset: i64 = 0;
        let mut buffer = self.read_block(0, TTCHEADERSIZE).ok_or(None)?;
        match buffer.get_int().map_err(|_| None)? {
            TTCF_TAG => {
                buffer.get_int().map_err(|_| None)?;
                self.directory_count = buffer.get_int().map_err(|_| None)?;
                if f_index >= self.directory_count {
                    return Err(Some(FontFormatException("Bad collection index".into())));
                }
                self.font_index = f_index;
                let mut b = self.read_block(TTCHEADERSIZE + 4 * f_index as i64, 4).ok_or(None)?;
                header_offset = b.get_int().map_err(|_| None)? as i64;
                self.font_data_size = 0.max(self.file_size.wrapping_sub(header_offset as i32));
            }
            V1TT_TAG | TRUE_TAG | OTTO_TAG => self.font_data_size = self.file_size,
            _ => return Err(Some(FontFormatException(format!("Unsupported sfnt {}", self.plat_name)))),
        }
        let mut b = self.read_block(header_offset + 4, 2).ok_or(None)?;
        let num_tables = b.get_short().map_err(|_| None)? as i32;
        let directory_offset = header_offset as i32 + DIRECTORYHEADERSIZE;
        if num_tables * DIRECTORYENTRYSIZE < 0 {
            return Err(None);
        }
        let bbuffer = self.read_block(directory_offset as i64, (num_tables * DIRECTORYENTRYSIZE) as i64).ok_or(None)?;
        let mut ibuffer = bbuffer.view(4);
        self.table_directory.clear();
        for _ in 0..num_tables {
            let tag = ibuffer.get_int().map_err(|_| None)?;
            ibuffer.get_int().map_err(|_| None)?;
            let offset = ibuffer.get_int().map_err(|_| None)? & 0x7FFFFFFF;
            let length = ibuffer.get_int().map_err(|_| None)? & 0x7FFFFFFF;
            self.table_directory.push(DirectoryEntry { tag, offset, length });
            if offset.wrapping_add(length) < length || offset.wrapping_add(length) > self.file_size {
                return Err(Some(FontFormatException(format!("bad table, tag={}", tag))));
            }
        }
        if self.get_directory_entry(HEAD_TAG).is_none() {
            return Err(Some(FontFormatException("missing head table".into())));
        }
        if self.get_directory_entry(MAXP_TAG).is_none() {
            return Err(Some(FontFormatException("missing maxp table".into())));
        }
        if self.get_directory_entry(HMTX_TAG).is_some() && self.get_directory_entry(HHEA_TAG).is_none() {
            return Err(Some(FontFormatException("missing hhea table".into())));
        }
        let maxp = self.get_table_buffer(MAXP_TAG).ok_or(None)?;
        if maxp.get_char_at(4).map_err(|_| None)? == 0 {
            return Err(Some(FontFormatException("zero glyphs".into())));
        }
        self.init_names().map_err(|_| None)?;
        Ok(())
    }

    fn set_cjk_support(&mut self, os2: Option<&ByteBuffer>) {
        let Some(os2) = os2 else { return };
        if os2.capacity() < 50 {
            return;
        }
        let range2 = os2.get_int_at(46).unwrap_or(0);
        self.supports_cjk = (range2 & 0x29bf0000) != 0;
    }

    pub fn get_table_buffer(&self, tag: i32) -> Option<ByteBuffer> {
        let entry = self.table_directory.iter().find(|e| e.tag == tag)?;
        if entry.length == 0 || entry.offset.wrapping_add(entry.length) < entry.length || entry.offset.wrapping_add(entry.length) > self.file_size {
            return None;
        }
        let v = self.read_file(entry.offset as i64, entry.length as usize).ok()?;
        if v.len() < entry.length as usize {
            return None;
        }
        Some(ByteBuffer::wrap(Rc::new(v)))
    }

    pub fn get_table_size(&self, tag: i32) -> i32 {
        self.table_directory.iter().find(|e| e.tag == tag).map(|e| e.length).unwrap_or(0)
    }

    fn get_directory_entry(&self, tag: i32) -> Option<DirectoryEntry> {
        self.table_directory.iter().find(|e| e.tag == tag).copied()
    }

    /// A CFF-flavoured font (FreeType's CFF driver, which this port's scaler lacks).
    pub fn is_cff(&self) -> bool {
        self.get_directory_entry(CFF_TAG).is_some() && self.get_directory_entry(GLYF_TAG).is_none()
    }

    fn set_style_os2(&mut self, os_2_table: Option<&ByteBuffer>) {
        let Some(t) = os_2_table else { return };
        if t.capacity() >= 8 {
            self.font_weight = t.get_char_at(4).unwrap_or(0) as i32;
            self.font_width = t.get_char_at(6).unwrap_or(0) as i32;
        }
        if t.capacity() < 64 {
            self.set_style_from_name();
            return;
        }
        let fs_selection = t.get_char_at(62).unwrap_or(0) as i32;
        let italic = fs_selection & 0x1;
        let bold = fs_selection & 0x20;
        let regular = fs_selection & 0x40;
        if regular != 0 && (italic | bold) != 0 {
            self.set_style_from_name();
            return;
        } else if (regular | italic | bold) == 0 {
            self.set_style_from_name();
            return;
        }
        match bold | italic {
            0x1 => self.style = FONT_ITALIC,
            0x20 => self.style = FONT_BOLD,
            0x21 => self.style = FONT_BOLD | FONT_ITALIC,
            _ => {}
        }
    }

    /// `Font2D.setStyle()`.
    fn set_style_from_name(&mut self) {
        if let Some(fname) = &self.full_name {
            self.style = crate::asc::flex2::fonts::java2d::sun::font::font2d::style_from_name(fname, self.style);
        }
    }

    fn init_names(&mut self) -> JResult<()> {
        let Some(buffer) = self.get_table_buffer(NAME_TAG) else { return Ok(()) };
        let mut buffer = buffer;
        let mut sbuffer = buffer.view(2);
        sbuffer.get_short()?;
        let num_records = sbuffer.get_short()?;
        let string_ptr = sbuffer.get_short()? as u16 as i64;
        let name_locale = Locale::startup();
        let name_locale_id = get_lcid_from_locale(&name_locale);
        self.language_compatible_lcids = get_language_compatible_lcids_from_locale(&name_locale);
        self.name_locale = Some(name_locale);
        for _ in 0..num_records as i32 {
            let platform_id = sbuffer.get_short()?;
            if platform_id != MS_PLATFORM_ID && platform_id != MAC_PLATFORM_ID {
                let p = sbuffer.position() as i64 + 10;
                sbuffer.set_position(p)?;
                continue;
            }
            let encoding_id = sbuffer.get_short()?;
            let lang_id = sbuffer.get_short()?;
            let name_id = sbuffer.get_short()? as i32;
            let name_len = sbuffer.get_short()? as u16 as usize;
            let name_ptr = sbuffer.get_short()? as u16 as i64 + string_ptr;
            if platform_id == MAC_PLATFORM_ID && (encoding_id != MACROMAN_SPECIFIC_ID || lang_id != MACROMAN_ENGLISH_LANG) {
                continue;
            }
            match name_id {
                FAMILY_NAME_ID => {
                    let mut compatible = false;
                    if self.family_name.is_none() || lang_id == ENGLISH_LOCALE_ID || lang_id == name_locale_id || (self.locale_family_name.is_none() && {
                        compatible = self.is_language_compatible(lang_id);
                        compatible
                    }) {
                        buffer.set_position(name_ptr)?;
                        let name = buffer.get_bytes(256, name_len)?;
                        let tmp_name = make_string(&name, platform_id, encoding_id);
                        if self.family_name.is_none() || lang_id == ENGLISH_LOCALE_ID {
                            self.family_name = tmp_name.clone();
                        }
                        if lang_id == name_locale_id || (self.locale_family_name.is_none() && compatible) {
                            self.locale_family_name = tmp_name;
                        }
                    }
                }
                FULL_NAME_ID => {
                    let mut compatible = false;
                    if self.full_name.is_none() || lang_id == ENGLISH_LOCALE_ID || lang_id == name_locale_id || (self.locale_full_name.is_none() && {
                        compatible = self.is_language_compatible(lang_id);
                        compatible
                    }) {
                        buffer.set_position(name_ptr)?;
                        let name = buffer.get_bytes(256, name_len)?;
                        let tmp_name = make_string(&name, platform_id, encoding_id);
                        if self.full_name.is_none() || lang_id == ENGLISH_LOCALE_ID {
                            self.full_name = tmp_name.clone();
                        }
                        if lang_id == name_locale_id || (self.locale_full_name.is_none() && compatible) {
                            self.locale_full_name = tmp_name;
                        }
                    }
                }
                _ => {}
            }
        }
        if self.locale_family_name.is_none() {
            self.locale_family_name = self.family_name.clone();
        }
        if self.locale_full_name.is_none() {
            self.locale_full_name = self.full_name.clone();
        }
        Ok(())
    }

    fn lookup_name(&self, find_locale_id: i16, find_name_id: i32) -> Option<String> {
        self.lookup_name_impl(find_locale_id, find_name_id).unwrap_or(None)
    }

    fn lookup_name_impl(&self, find_locale_id: i16, find_name_id: i32) -> JResult<Option<String>> {
        let mut found_name: Option<String> = None;
        let Some(mut buffer) = self.get_table_buffer(NAME_TAG) else { return Ok(None) };
        let mut sbuffer = buffer.view(2);
        sbuffer.get_short()?;
        let num_records = sbuffer.get_short()?;
        let string_ptr = sbuffer.get_short()? as u16 as i64;
        for _ in 0..num_records as i32 {
            let platform_id = sbuffer.get_short()?;
            if platform_id != MS_PLATFORM_ID {
                let p = sbuffer.position() as i64 + 10;
                sbuffer.set_position(p)?;
                continue;
            }
            let encoding_id = sbuffer.get_short()?;
            let lang_id = sbuffer.get_short()?;
            let name_id = sbuffer.get_short()? as i32;
            let name_len = sbuffer.get_short()? as u16 as usize;
            let name_ptr = sbuffer.get_short()? as u16 as i64 + string_ptr;
            if name_id == find_name_id && ((found_name.is_none() && lang_id == ENGLISH_LOCALE_ID) || lang_id == find_locale_id) {
                buffer.set_position(name_ptr)?;
                let name = buffer.get_bytes(1024, name_len)?;
                found_name = make_string(&name, platform_id, encoding_id);
                if lang_id == find_locale_id {
                    return Ok(found_name);
                }
            }
        }
        Ok(found_name)
    }

    pub fn get_font_count(&self) -> i32 {
        self.directory_count
    }

    pub fn get_postscript_name(&self) -> String {
        match self.lookup_name(ENGLISH_LOCALE_ID, POSTSCRIPT_NAME_ID) {
            None => self.full_name.clone().unwrap_or_default(),
            Some(n) => n,
        }
    }

    pub fn get_font_name(&self, locale: Option<&Locale>) -> String {
        match locale {
            None => self.full_name.clone().unwrap_or_default(),
            Some(l) if Some(l) == self.name_locale.as_ref() && self.locale_full_name.is_some() => self.locale_full_name.clone().unwrap(),
            Some(l) => self.lookup_name(get_lcid_from_locale(l), FULL_NAME_ID).unwrap_or_else(|| self.full_name.clone().unwrap_or_default()),
        }
    }

    pub fn get_family_name(&self, locale: Option<&Locale>) -> String {
        match locale {
            None => self.family_name.clone().unwrap_or_default(),
            Some(l) if Some(l) == self.name_locale.as_ref() && self.locale_family_name.is_some() => self.locale_family_name.clone().unwrap(),
            Some(l) => self.lookup_name(get_lcid_from_locale(l), FAMILY_NAME_ID).unwrap_or_else(|| self.family_name.clone().unwrap_or_default()),
        }
    }

    /// `initAllNames`.
    fn init_all_names(&self, requested_id: i32, names: &mut Vec<String>) -> JResult<()> {
        let Some(mut buffer) = self.get_table_buffer(NAME_TAG) else { return Ok(()) };
        let mut sbuffer = buffer.view(2);
        sbuffer.get_short()?;
        let num_records = sbuffer.get_short()?;
        let string_ptr = sbuffer.get_short()? as u16 as i64;
        for _ in 0..num_records as i32 {
            let platform_id = sbuffer.get_short()?;
            if platform_id != MS_PLATFORM_ID {
                let p = sbuffer.position() as i64 + 10;
                sbuffer.set_position(p)?;
                continue;
            }
            let encoding_id = sbuffer.get_short()?;
            sbuffer.get_short()?;
            let name_id = sbuffer.get_short()? as i32;
            let name_len = sbuffer.get_short()? as u16 as usize;
            let name_ptr = sbuffer.get_short()? as u16 as i64 + string_ptr;
            if name_id == requested_id {
                buffer.set_position(name_ptr)?;
                let name = buffer.get_bytes(256, name_len)?;
                if let Some(s) = make_string(&name, platform_id, encoding_id) {
                    if !names.contains(&s) {
                        names.push(s);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn get_all_family_names(&self) -> Vec<String> {
        let mut v = Vec::new();
        let _ = self.init_all_names(FAMILY_NAME_ID, &mut v);
        v
    }

    pub fn get_all_full_names(&self) -> Vec<String> {
        let mut v = Vec::new();
        let _ = self.init_all_names(FULL_NAME_ID, &mut v);
        v
    }

    fn is_language_compatible(&self, lcid: i16) -> bool {
        self.language_compatible_lcids.contains(&lcid)
    }

    /// `supportsEncoding(null)` with the Windows code page (`file.encoding`).
    pub fn supports_encoding(&self, encoding: Option<&str>) -> bool {
        let encoding = encoding.unwrap_or(CODE_PAGE).to_lowercase();
        if encoding.is_empty() {
            return false;
        }
        let encoding = match encoding.as_str() {
            "gb18030" => "gbk".to_string(),
            "ms950_hkscs" => "ms950".to_string(),
            _ => encoding,
        };
        let Some(buffer) = self.get_table_buffer(OS_2_TAG) else { return false };
        if buffer.capacity() < 86 {
            return false;
        }
        let range1 = buffer.get_int_at(78).unwrap_or(0);
        for (em, m) in ENCODING_MAPPING.iter().enumerate() {
            if *m == encoding && ((1 << em) & range1) != 0 {
                return true;
            }
        }
        false
    }

    /// `useAAForPtSize`, through the `gasp` table.
    pub fn use_aa_for_pt_size(&self, ptsize: i32, style: i32) -> bool {
        let gasp = self.get_gasp_table();
        if !gasp.is_empty() {
            for i in (0..gasp.len()).step_by(2) {
                if ptsize <= gasp[i] as i32 {
                    return (gasp[i + 1] & 0x2) != 0;
                }
            }
            return true;
        }
        if style == FONT_BOLD {
            true
        } else {
            ptsize <= 8 || ptsize >= 18
        }
    }

    fn get_gasp_table(&self) -> Vec<u16> {
        const GASP_TAG: i32 = 0x67617370;
        let Some(buffer) = self.get_table_buffer(GASP_TAG) else { return Vec::new() };
        let mut c = buffer.view(2);
        let Ok(format) = c.get_char() else { return Vec::new() };
        if format > 1 {
            return Vec::new();
        }
        let Ok(num_ranges) = c.get_char() else { return Vec::new() };
        if 4 + num_ranges as i32 * 4 > self.get_table_size(GASP_TAG) {
            return Vec::new();
        }
        let mut v = Vec::with_capacity(2 * num_ranges as usize);
        for _ in 0..2 * num_ranges as usize {
            match c.get_char() {
                Ok(x) => v.push(x),
                Err(_) => return Vec::new(),
            }
        }
        v
    }
}

const CODE_PAGE: &str = "Cp1252";

static ENCODING_MAPPING: [&str; 32] = [
    "cp1252", "cp1250", "cp1251", "cp1253", "cp1254", "cp1255", "cp1256", "cp1257", "", "", "", "", "", "", "", "", "ms874", "ms932", "gbk", "ms949", "ms950", "ms1361", "", "", "", "", "", "", "", "", "", "",
];

/// `makeString`; the platform-3 CJK encodings 2..6 (SJIS, GBK, MS950, EUC_KR, Johab) decode bytes as Latin-1 here.
fn make_string(bytes: &[u8], platform_id: i16, encoding: i16) -> Option<String> {
    let encoding = if platform_id == MAC_PLATFORM_ID { -1 } else { encoding };
    if (2..=6).contains(&encoding) {
        let b: Vec<u8> = bytes.iter().copied().filter(|&b| b != 0).collect();
        return Some(b.iter().map(|&c| c as char).collect());
    }
    if encoding == -1 {
        return Some(bytes.iter().map(|&c| if c < 0x80 { c as char } else { '\u{fffd}' }).collect());
    }
    Some(decode_utf16(bytes))
}

/// `new String(bytes, "UTF-16")`: BOM-detected, big-endian by default, malformed input replaced.
pub fn decode_utf16(bytes: &[u8]) -> String {
    let mut i = 0;
    let mut le = false;
    if bytes.len() >= 2 {
        if bytes[0] == 0xFE && bytes[1] == 0xFF {
            i = 2;
        } else if bytes[0] == 0xFF && bytes[1] == 0xFE {
            i = 2;
            le = true;
        }
    }
    let mut units = Vec::with_capacity(bytes.len() / 2);
    while i + 1 < bytes.len() {
        units.push(if le { u16::from_le_bytes([bytes[i], bytes[i + 1]]) } else { u16::from_be_bytes([bytes[i], bytes[i + 1]]) });
        i += 2;
    }
    let mut s: String = char::decode_utf16(units.iter().copied()).map(|r| r.unwrap_or('\u{fffd}')).collect();
    if i < bytes.len() {
        s.push('\u{fffd}');
    }
    s
}

fn get_lcid_from_locale(locale: &Locale) -> i16 {
    if *locale == Locale::us() {
        return US_LCID;
    }
    let mut key = locale.to_string();
    while !key.is_empty() {
        if let Some(&(_, v)) = LCID_MAP.iter().find(|(k, _)| *k == key) {
            return v;
        }
        match key.rfind('_') {
            Some(pos) if pos >= 1 => key.truncate(pos),
            _ => return US_LCID,
        }
    }
    US_LCID
}

fn get_language_compatible_lcids_from_locale(locale: &Locale) -> &'static [i16] {
    match locale.language.as_str() {
        "de" => &[1031, 3079, 5127, 2055, 4103],
        "no" => &[1044, 2068],
        "ru" => &[1049, 2073],
        "sv" => &[1053, 2077],
        "pt" => &[1046, 2070],
        "qu" => &[1131, 3179, 2155],
        "ms" => &[1086, 2110],
        "en" => &[11273, 3081, 12297, 8201, 10249, 4105, 13321, 6153, 7177, 5129, 2057],
        "hr" => &[1050, 4122],
        "it" => &[1040, 2064],
        "fr" => &[1036, 5132, 6156, 2060, 3084, 4108],
        "es" => &[1034, 12298, 14346, 2058, 8202, 19466, 17418, 9226, 13322, 5130, 7178, 11274, 16394, 4106, 10250, 6154, 18442, 20490, 15370],
        "zh" => &[1028, 3076, 5124, 4100, 2052],
        "ar" => &[1025, 8193, 16385, 9217, 2049, 14337, 15361, 11265, 13313, 10241, 7169, 12289, 4097, 5121, 6145, 3073],
        "se" => &[1083, 3131, 2107],
        "ro" => &[1048, 2072],
        "nl" => &[1043, 2067],
        "sr" => &[7194, 3098],
        _ => &[],
    }
}

static LCID_MAP: &[(&str, i16)] = &[
    ("ar", 0x0401), ("bg", 0x0402), ("ca", 0x0403), ("zh", 0x0404), ("cs", 0x0405), ("da", 0x0406), ("de", 0x0407), ("el", 0x0408),
    ("es", 0x040a), ("fi", 0x040b), ("fr", 0x040c), ("iw", 0x040d), ("hu", 0x040e), ("is", 0x040f), ("it", 0x0410), ("ja", 0x0411),
    ("ko", 0x0412), ("nl", 0x0413), ("no", 0x0414), ("pl", 0x0415), ("pt", 0x0416), ("rm", 0x0417), ("ro", 0x0418), ("ru", 0x0419),
    ("hr", 0x041a), ("sk", 0x041b), ("sq", 0x041c), ("sv", 0x041d), ("th", 0x041e), ("tr", 0x041f), ("ur", 0x0420), ("in", 0x0421),
    ("uk", 0x0422), ("be", 0x0423), ("sl", 0x0424), ("et", 0x0425), ("lv", 0x0426), ("lt", 0x0427), ("fa", 0x0429), ("vi", 0x042a),
    ("hy", 0x042b), ("eu", 0x042d), ("mk", 0x042f), ("tn", 0x0432), ("xh", 0x0434), ("zu", 0x0435), ("af", 0x0436), ("ka", 0x0437),
    ("fo", 0x0438), ("hi", 0x0439), ("mt", 0x043a), ("se", 0x043b), ("gd", 0x043c), ("ms", 0x043e), ("kk", 0x043f), ("ky", 0x0440),
    ("sw", 0x0441), ("tt", 0x0444), ("bn", 0x0445), ("pa", 0x0446), ("gu", 0x0447), ("ta", 0x0449), ("te", 0x044a), ("kn", 0x044b),
    ("ml", 0x044c), ("mr", 0x044e), ("sa", 0x044f), ("mn", 0x0450), ("cy", 0x0452), ("gl", 0x0456), ("dv", 0x0465), ("qu", 0x046b),
    ("mi", 0x0481), ("ar_IQ", 0x0801), ("zh_CN", 0x0804), ("de_CH", 0x0807), ("en_GB", 0x0809), ("es_MX", 0x080a), ("fr_BE", 0x080c),
    ("it_CH", 0x0810), ("nl_BE", 0x0813), ("no_NO_NY", 0x0814), ("pt_PT", 0x0816), ("ro_MD", 0x0818), ("ru_MD", 0x0819), ("sr_CS", 0x081a),
    ("sv_FI", 0x081d), ("az_AZ", 0x082c), ("se_SE", 0x083b), ("ga_IE", 0x083c), ("ms_BN", 0x083e), ("uz_UZ", 0x0843), ("qu_EC", 0x086b),
    ("ar_EG", 0x0c01), ("zh_HK", 0x0c04), ("de_AT", 0x0c07), ("en_AU", 0x0c09), ("fr_CA", 0x0c0c), ("sr_CS", 0x0c1a), ("se_FI", 0x0c3b),
    ("qu_PE", 0x0c6b), ("ar_LY", 0x1001), ("zh_SG", 0x1004), ("de_LU", 0x1007), ("en_CA", 0x1009), ("es_GT", 0x100a), ("fr_CH", 0x100c),
    ("hr_BA", 0x101a), ("ar_DZ", 0x1401), ("zh_MO", 0x1404), ("de_LI", 0x1407), ("en_NZ", 0x1409), ("es_CR", 0x140a), ("fr_LU", 0x140c),
    ("bs_BA", 0x141a), ("ar_MA", 0x1801), ("en_IE", 0x1809), ("es_PA", 0x180a), ("fr_MC", 0x180c), ("sr_BA", 0x181a), ("ar_TN", 0x1c01),
    ("en_ZA", 0x1c09), ("es_DO", 0x1c0a), ("sr_BA", 0x1c1a), ("ar_OM", 0x2001), ("en_JM", 0x2009), ("es_VE", 0x200a), ("ar_YE", 0x2401),
    ("es_CO", 0x240a), ("ar_SY", 0x2801), ("en_BZ", 0x2809), ("es_PE", 0x280a), ("ar_JO", 0x2c01), ("en_TT", 0x2c09), ("es_AR", 0x2c0a),
    ("ar_LB", 0x3001), ("en_ZW", 0x3009), ("es_EC", 0x300a), ("ar_KW", 0x3401), ("en_PH", 0x3409), ("es_CL", 0x340a), ("ar_AE", 0x3801),
    ("es_UY", 0x380a), ("ar_BH", 0x3c01), ("es_PY", 0x3c0a), ("ar_QA", 0x4001), ("es_BO", 0x400a), ("es_SV", 0x440a), ("es_HN", 0x480a),
    ("es_NI", 0x4c0a), ("es_PR", 0x500a),
];

