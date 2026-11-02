//! Port of afe.jar `com.adobe.fontengine.font.opentype.Name` with `MacintoshLanguageCode`, `MicrosoftLCID` and the name selectors.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::charset_util::{Charset, CharsetUtil};
use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{ExceptionKind, FontEngineException, FontResult};
use crate::asc::util::jhash::{JHashMap, JavaHash};

pub const TAG_NAME: i32 = 1851878757;

/// `MicrosoftLCID.ENGLISH_UNITED_STATES.getLanguageCode()`.
pub const ENGLISH_UNITED_STATES: i32 = 1033;

/// `LanguageCodeEntry`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageCodeEntry {
    pub language_code: i32,
    pub language: &'static str,
    pub script: Option<&'static str>,
    pub region: Option<&'static str>,
    pub variant: Option<&'static str>,
}

impl LanguageCodeEntry {
    pub fn get_language_code(&self) -> i32 {
        self.language_code
    }

    pub fn get_locale_string(&self) -> String {
        let mut s = self.language.to_string();
        for p in [self.script, self.region, self.variant].into_iter().flatten() {
            s.push('_');
            s.push_str(p);
        }
        s
    }

    /// `ULocale` stands as its name.
    pub fn get_locale(&self) -> String {
        self.get_locale_string()
    }
}

fn lookup(t: &[(i32, &'static str, Option<&'static str>, Option<&'static str>, Option<&'static str>)], n: i32) -> Option<LanguageCodeEntry> {
    t.binary_search_by(|e| e.0.cmp(&n)).ok().map(|i| {
        let e = t[i];
        LanguageCodeEntry { language_code: e.0, language: e.1, script: e.2, region: e.3, variant: e.4 }
    })
}

/// `MacintoshLanguageCode.getCodeEntry`.
pub fn macintosh_language_code_get_code_entry(n: i32) -> Option<LanguageCodeEntry> {
    lookup(&MACINTOSH_LANGUAGE_CODES, n)
}

/// `MicrosoftLCID.getCodeEntry`.
pub fn microsoft_lcid_get_code_entry(n: i32) -> Option<LanguageCodeEntry> {
    lookup(&MICROSOFT_LCIDS, n)
}

/// `Name.NameSelector`.
pub trait NameSelector {
    fn name_found(&mut self, s: &str, name_id: i32, platform: i32, encoding: i32, language: i32) -> FontResult<bool>;
}

/// The selector `getFirstName` passes.
struct FirstName;

impl NameSelector for FirstName {
    fn name_found(&mut self, _s: &str, _n: i32, _p: i32, _e: i32, _l: i32) -> FontResult<bool> {
        Ok(true)
    }
}

/// `NameEntry`: equal on language and platform alone (`nameIdDifferentiates` is always false here).
#[derive(Clone, Debug)]
pub struct NameEntry {
    name: String,
    name_id: i32,
    platform_id: i32,
    language: i32,
    name_id_differentiates: bool,
}

impl NameEntry {
    pub fn new(name: String, name_id: i32, platform_id: i32, language: i32, _encoding: i32, b: bool) -> NameEntry {
        NameEntry { name, name_id, platform_id, language, name_id_differentiates: b }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_name_id(&self) -> i32 {
        self.name_id
    }

    pub fn get_platform_id(&self) -> i32 {
        self.platform_id
    }

    pub fn get_language(&self) -> i32 {
        self.language
    }
}

impl PartialEq for NameEntry {
    fn eq(&self, o: &Self) -> bool {
        self.language == o.language
            && !(self.name_id_differentiates && self.name_id != o.name_id)
            && self.name_id_differentiates == o.name_id_differentiates
            && self.platform_id == o.platform_id
    }
}

impl Eq for NameEntry {}

impl JavaHash for NameEntry {
    fn java_hash(&self) -> i32 {
        let mut n: i32 = 1;
        n = n.wrapping_mul(31).wrapping_add(self.language);
        if self.name_id_differentiates {
            n = n.wrapping_mul(31).wrapping_add(self.name_id);
        }
        n = n.wrapping_mul(31).wrapping_add(if self.name_id_differentiates { 1231 } else { 1237 });
        n.wrapping_mul(31).wrapping_add(self.platform_id)
    }
}

/// The parts of a `ULocale` name that `isContainedWithin` compares.
fn locale_parts(s: &str) -> (String, String, String, String) {
    let p: Vec<&str> = s.split(|c| c == '_' || c == '-').collect();
    let mut i = 0;
    let lang = p.first().map(|x| x.to_ascii_lowercase()).unwrap_or_default();
    i += 1;
    let mut script = String::new();
    if i < p.len() && p[i].len() == 4 && p[i].chars().all(|c| c.is_ascii_alphabetic()) {
        script = p[i].to_string();
        i += 1;
    }
    let mut country = String::new();
    if i < p.len() && (p[i].len() == 2 && p[i].chars().all(|c| c.is_ascii_alphabetic()) || p[i].len() == 3 && p[i].chars().all(|c| c.is_ascii_digit())) {
        country = p[i].to_ascii_uppercase();
        i += 1;
    }
    let variant = if i < p.len() { p[i..].join("_").to_ascii_uppercase() } else { String::new() };
    (lang, script, country, variant)
}

/// `NameEntrySet`.
pub struct NameEntrySet {
    set: JHashMap<NameEntry, NameEntry>,
    platform_id: i32,
    locale: Option<String>,
    name_ids: Option<Vec<i32>>,
}

impl Default for NameEntrySet {
    fn default() -> Self {
        Self::new()
    }
}

impl NameEntrySet {
    pub fn new() -> NameEntrySet {
        NameEntrySet { set: JHashMap::new(), platform_id: -1, locale: None, name_ids: None }
    }

    /// `getSet()`: the map's keys, which keep the first entry stored for each language and platform.
    pub fn get_set(&self) -> Option<Vec<NameEntry>> {
        if self.set.is_empty() {
            return None;
        }
        Some(self.set.keys().cloned().collect())
    }

    pub fn set_platform(&mut self, n: i32) {
        self.platform_id = n;
    }

    pub fn set_locale(&mut self, l: Option<String>) {
        self.locale = l;
    }

    pub fn set_name_ids(&mut self, a: Option<Vec<i32>>) {
        self.name_ids = a;
    }

    pub fn is_contained_within(l: &str, l2: Option<&str>) -> bool {
        let Some(l2) = l2 else { return true };
        let (a, b, c, d) = locale_parts(l);
        let (a2, b2, c2, d2) = locale_parts(l2);
        (a.is_empty() || a == a2) && (c.is_empty() || c == c2) && (b.is_empty() || b == b2) && (d.is_empty() || d == d2)
    }
}

impl NameSelector for NameEntrySet {
    fn name_found(&mut self, s: &str, n: i32, n2: i32, n3: i32, n4: i32) -> FontResult<bool> {
        if self.platform_id != -1 && n2 != self.platform_id {
            return Ok(false);
        }
        if let Some(l) = &self.locale {
            if !NameEntrySet::is_contained_within(l, Name::get_locale_for_locale_id(n2, n4).as_deref()) {
                return Ok(false);
            }
        }
        let mut object = NameEntry::new(s.to_string(), n, n2, n4, n3, false);
        if let Some(ids) = &self.name_ids {
            if let Some(existing) = self.set.get(&object).cloned() {
                let mut n5 = i32::MAX;
                let mut n6 = i32::MAX;
                for (i, &id) in ids.iter().enumerate() {
                    // Java compares the name ids with the languages.
                    if id == object.get_language() {
                        n5 = i as i32;
                    }
                    if id == existing.get_language() {
                        n6 = i as i32;
                    }
                }
                if n6 < n5 {
                    object = existing;
                } else {
                    self.set.remove(&object);
                }
            }
        }
        self.set.insert(object.clone(), object);
        Ok(false)
    }
}

/// `OpenTypeFont.SingleNameSelector` and its `SinglePostscriptName` / `SingleFamilyName`.
#[derive(Clone, Debug)]
pub struct SingleNameSelector {
    pub current_selection: Option<String>,
    pub current_platform_id: i32,
    pub current_encoding: i32,
    pub current_language: i32,
    family: bool,
}

impl SingleNameSelector {
    pub fn single_postscript_name() -> SingleNameSelector {
        SingleNameSelector { current_selection: None, current_platform_id: -1, current_encoding: 0, current_language: 0, family: false }
    }

    pub fn single_family_name() -> SingleNameSelector {
        SingleNameSelector { family: true, ..Self::single_postscript_name() }
    }

    fn pick_ms_language(&self, n: i32) -> bool {
        self.current_language != ENGLISH_UNITED_STATES && n == ENGLISH_UNITED_STATES
    }

    fn pick_mac_name(&self, n: i32) -> bool {
        n == 0 && self.current_encoding != 0
    }

    fn pick_ms_name(&self, n: i32, n2: i32) -> bool {
        if self.current_encoding == n {
            return self.pick_ms_language(n2);
        }
        if self.current_encoding == 10 {
            return false;
        }
        if n == 10 {
            return true;
        }
        if self.current_encoding == 1 {
            return false;
        }
        n == 1
    }

    fn take(&mut self, s: &str, n2: i32, n3: i32, n4: i32) {
        self.current_selection = Some(s.to_string());
        self.current_platform_id = n2;
        self.current_encoding = n3;
        self.current_language = n4;
    }

    pub fn get_selection(&self) -> Option<String> {
        self.current_selection.clone()
    }
}

impl NameSelector for SingleNameSelector {
    fn name_found(&mut self, s: &str, _n: i32, n2: i32, n3: i32, n4: i32) -> FontResult<bool> {
        if s.is_empty() || s.starts_with('\u{0}') {
            return Ok(false);
        }
        if self.current_platform_id == -1 {
            self.take(s, n2, n3, n4);
            return Ok(false);
        }
        if self.family {
            if self.current_platform_id == 3 && n2 == 1 {
                return Ok(false);
            }
            if n2 == 3 && self.current_platform_id == 1 {
                self.take(s, n2, n3, n4);
                return Ok(false);
            }
            if self.current_platform_id == 3 {
                if self.pick_ms_name(n3, n4) {
                    self.take(s, n2, n3, n4);
                }
                return Ok(false);
            }
        } else {
            if self.current_platform_id == 3 && n2 == 1 && (self.current_language == ENGLISH_UNITED_STATES || n3 != 0)
                || self.current_platform_id == 1 && n2 == 3 && n4 != ENGLISH_UNITED_STATES && self.current_encoding == 0
            {
                return Ok(false);
            }
            if self.current_platform_id != n2 {
                self.take(s, n2, n3, n4);
                return Ok(false);
            }
            if self.current_platform_id == 3 {
                if self.pick_ms_language(n4) {
                    self.take(s, n2, n3, n4);
                }
                return Ok(false);
            }
        }
        if self.pick_mac_name(n3) {
            self.take(s, n2, n3, n4);
        }
        Ok(false)
    }
}

/// `UnsupportedEncodingException`, an `IOException`.
fn unsupported_encoding() -> FontEngineException {
    FontEngineException::bare(ExceptionKind::Io)
}

#[derive(Clone, Debug, Default)]
pub struct Name {
    pub data: OTByteArray,
}

impl Name {
    pub fn get_locale_for_locale_id(n: i32, n2: i32) -> Option<String> {
        let e = match n {
            1 => macintosh_language_code_get_code_entry(n2),
            3 => microsoft_lcid_get_code_entry(n2),
            _ => None,
        };
        e.map(|e| e.get_locale())
    }

    pub fn new(a: FontByteArray) -> Name {
        Name { data: OTByteArray::from_font_byte_array(a) }
    }

    pub fn get_charset_name(n: i32, n2: i32) -> Option<&'static str> {
        match n {
            3 => match n2 {
                0 | 1 | 10 => Some("UTF-16BE"),
                2 => Some("windows-932"),
                3 => Some("windows-936"),
                4 => Some("windows-950"),
                5 => Some("windows-949"),
                6 => Some("ms1361"),
                _ => None,
            },
            1 => match n2 {
                0 => Some("MacRoman"),
                1 => Some("MacJapanese"),
                2 => Some("MacChineseTraditional"),
                3 => Some("MacKorean"),
                4 => Some("MacArabic"),
                5 => Some("MacHebrew"),
                6 => Some("MacGreek"),
                7 => Some("MacCyrillic"),
                8 => Some("MacSymbol"),
                21 => Some("MacThai"),
                25 => Some("MacChineseSimplified"),
                _ => None,
            },
            0 => Some("UTF-16BE"),
            _ => None,
        }
    }

    pub fn get_charset(s: Option<&str>) -> Option<Charset> {
        CharsetUtil::for_name_icu(s)
    }

    /// `bytesToUnicode(bytes, 0, n, charset)`: `None` where Java throws one of the decoding exceptions its callers skip.
    pub fn bytes_to_unicode(b: &[u8], cs: Charset) -> Option<String> {
        CharsetUtil::bytes_to_unicode(b, cs).map(|u| String::from_utf16_lossy(&u))
    }

    /// `this.data.getBytes(offset, len)`, `None` where Java's copy throws `ArrayIndexOutOfBoundsException`.
    fn get_bytes(&self, offset: i32, len: i32) -> Option<Vec<u8>> {
        if len <= 0 {
            return Some(Vec::new());
        }
        if offset < 0 || offset as i64 + len as i64 > self.data.base.size as i64 {
            return None;
        }
        Some(self.data.base.get_bytes(offset, len))
    }

    /// `getName(nameId, language)` on the Windows Unicode records.
    pub fn get_name_ms(&self, n: i32, n2: i32) -> FontResult<Option<String>> {
        match self.get_name(3, 1, n, n2) {
            Err(e) if e.kind == ExceptionKind::Io => Ok(None),
            r => r,
        }
    }

    /// `getName(platform, encoding, language, nameId)`; language -1 matches any. `UnsupportedEncodingException` is `Io`.
    pub fn get_name(&self, n: i32, n2: i32, n3: i32, n4: i32) -> FontResult<Option<String>> {
        let cs = Name::get_charset(Name::get_charset_name(n, n2)).ok_or_else(unsupported_encoding)?;
        let n5 = self.data.getuint16(2)?;
        let n6 = self.data.getuint16(4)?;
        for i in 0..n5 {
            if self.data.getuint16(6 + 12 * i)? != n
                || self.data.getuint16(6 + 12 * i + 2)? != n2
                || self.data.getuint16(6 + 12 * i + 4)? != n3 && -1 != n3
                || self.data.getuint16(6 + 12 * i + 6)? != n4
            {
                continue;
            }
            let n7 = self.data.getuint16(6 + 12 * i + 8)?;
            let n8 = n6 + self.data.getuint16(6 + 12 * i + 10)?;
            let Some(bytes) = self.get_bytes(n8, n7) else { continue };
            if let Some(s) = Name::bytes_to_unicode(&bytes, cs) {
                return Ok(Some(s));
            }
        }
        Ok(None)
    }

    pub fn has_name(&self, n: i32, n2: i32, n3: i32, n4: i32) -> FontResult<bool> {
        let n5 = self.data.getuint16(2)?;
        for i in 0..n5 {
            if self.data.getuint16(6 + 12 * i)? != n && -1 != n
                || self.data.getuint16(6 + 12 * i + 2)? != n2 && -1 != n2
                || self.data.getuint16(6 + 12 * i + 4)? != n3 && -1 != n3
                || self.data.getuint16(6 + 12 * i + 6)? != n4 && -1 != n4
            {
                continue;
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn look_for_names(&self, sel: &mut dyn NameSelector, ids: &[i32], n: i32, n2: i32) -> FontResult<Option<String>> {
        for i in 0..n {
            let n3 = self.data.getuint16(6 + 12 * i + 6)?;
            for &id in ids {
                if n3 != id {
                    continue;
                }
                let Some(cs_name) = Name::get_charset_name(self.data.getuint16(6 + 12 * i)?, self.data.getuint16(6 + 12 * i + 2)?) else { continue };
                let n4 = self.data.getuint16(6 + 12 * i + 8)?;
                let n5 = n2 + self.data.getuint16(6 + 12 * i + 10)?;
                let Some(bytes) = self.get_bytes(n5, n4) else { continue };
                let cs = CharsetUtil::for_name_icu(Some(cs_name)).expect("aglj40 has every name charset");
                let Some(s) = Name::bytes_to_unicode(&bytes, cs) else { continue };
                if !sel.name_found(&s, n3, self.data.getuint16(6 + 12 * i)?, self.data.getuint16(6 + 12 * i + 2)?, self.data.getuint16(6 + 12 * i + 4)?)? {
                    continue;
                }
                return Ok(Some(s));
            }
        }
        Ok(None)
    }

    fn pick_family_name_selector(&self, n: i32) -> FontResult<i32> {
        for i in 0..n {
            if self.data.getuint16(6 + 12 * i + 6)? == 16 {
                return Ok(16);
            }
        }
        Ok(1)
    }

    pub fn get_first_name(&self, n: i32) -> FontResult<Option<String>> {
        let n2 = self.data.getuint16(2)?;
        let n3 = self.data.getuint16(4)?;
        self.look_for_names(&mut FirstName, &[n], n2, n3)
    }

    pub fn get_mac_fxg_family_names(&self, locale: Option<&str>) -> FontResult<Option<Vec<NameEntry>>> {
        let ids = vec![18, 4];
        let mut s = NameEntrySet::new();
        s.set_name_ids(Some(ids.clone()));
        s.set_platform(1);
        s.set_locale(locale.map(str::to_string));
        self.get_names_extended(&ids, &mut s)?;
        let mut set = s.get_set();
        if set.is_none() {
            if locale.is_some() {
                s.set_locale(None);
                self.get_names_extended(&ids, &mut s)?;
                if s.get_set().is_some() {
                    return Ok(None);
                }
            }
            s.set_locale(locale.map(str::to_string));
            s.set_platform(3);
            self.get_names_extended(&ids, &mut s)?;
            set = s.get_set();
        }
        Ok(set)
    }

    pub fn get_windows_fxg_family_names(&self, locale: Option<&str>) -> FontResult<Option<Vec<NameEntry>>> {
        let mut s = NameEntrySet::new();
        s.set_platform(3);
        s.set_locale(locale.map(str::to_string));
        self.get_names_extended(&[1], &mut s)?;
        Ok(s.get_set())
    }

    pub fn get_mac_platform_names(&self, locale: Option<&str>) -> FontResult<Option<Vec<NameEntry>>> {
        let mut s = NameEntrySet::new();
        s.set_platform(1);
        s.set_locale(locale.map(str::to_string));
        self.get_names_extended(&[4], &mut s)?;
        let mut set = s.get_set();
        if set.is_none() {
            if locale.is_some() {
                s.set_locale(None);
                self.get_names_extended(&[4], &mut s)?;
                if s.get_set().is_some() {
                    return Ok(None);
                }
            }
            s.set_locale(locale.map(str::to_string));
            s.set_platform(3);
            self.get_names_extended(&[4], &mut s)?;
            set = s.get_set();
        }
        Ok(set)
    }

    pub fn get_windows_platform_names(&self, locale: Option<&str>) -> FontResult<Option<Vec<NameEntry>>> {
        let mut s = NameEntrySet::new();
        s.set_platform(3);
        s.set_locale(locale.map(str::to_string));
        self.get_names_extended(&[4], &mut s)?;
        Ok(s.get_set())
    }

    fn get_names_extended(&self, ids: &[i32], s: &mut NameEntrySet) -> FontResult<()> {
        let n = self.data.getuint16(2)?;
        let n2 = self.data.getuint16(4)?;
        self.look_for_names(s, ids, n, n2)?;
        Ok(())
    }

    pub fn select_family_name_id(&self) -> FontResult<i32> {
        self.pick_family_name_selector(self.data.getuint16(2)?)
    }

    pub fn enumerate_names(&self, sel: &mut dyn NameSelector, n: i32) -> FontResult<()> {
        let n2 = self.data.getuint16(2)?;
        let n3 = self.data.getuint16(4)?;
        self.look_for_names(sel, &[n], n2, n3)?;
        Ok(())
    }

    fn keep_name_id(n: i32, bl: bool, a: Option<&[i32]>) -> bool {
        if n == 0 || bl && n == 7 {
            return true;
        }
        a.is_some_and(|a| a.contains(&n))
    }

    pub fn subset_and_stream(&self, bl: bool, a: Option<&[i32]>, map: &mut BTreeMap<i32, OTByteArrayBuilder>) -> FontResult<()> {
        let mut n2 = 0;
        let mut n3 = 0;
        let n4 = self.data.getuint16(2)?;
        let n5 = self.data.getuint16(4)?;
        for n in 0..n4 {
            let n6 = self.data.getuint16(6 + 12 * n + 6)?;
            if !Name::keep_name_id(n6, bl, a) {
                continue;
            }
            n2 += 1;
            n3 += self.data.getuint16(6 + 12 * n + 8)?;
        }
        let n = 6 + 12 * n2;
        let mut b = OTByteArray::get_ot_byte_array_builder_instance_sized(n + n3);
        b.setuint32(0, 0);
        b.setuint16(2, n2);
        b.setuint16(4, n);
        let mut n7 = 6;
        let mut n8 = 0;
        for i in 0..n4 {
            let n9 = self.data.getuint16(6 + 12 * i + 6)?;
            if !Name::keep_name_id(n9, bl, a) {
                continue;
            }
            let n10 = self.data.getuint16(6 + 12 * i + 8)?;
            b.setuint16(n7, self.data.getuint16(6 + 12 * i)?);
            b.setuint16(n7 + 2, self.data.getuint16(6 + 12 * i + 2)?);
            b.setuint16(n7 + 4, self.data.getuint16(6 + 12 * i + 4)?);
            b.setuint16(n7 + 6, n9);
            b.setuint16(n7 + 8, n10);
            b.setuint16(n7 + 10, n8);
            n7 += 12;
            let src = n5 + self.data.getuint16(6 + 12 * i + 10)?;
            if n10 > 0 && (src < 0 || src as i64 + n10 as i64 > self.data.base.size as i64) {
                return Err(FontEngineException::invalid_font(format!("string {} in name table extends beyond the end of the table", i)));
            }
            b.replace_from(n + n8, &self.data.base, src, n10)?;
            n8 += n10;
        }
        map.insert(TAG_NAME, b);
        Ok(())
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>, bl: bool, a: Option<&[i32]>) -> FontResult<()> {
        self.subset_and_stream(bl, a, map)
    }

    /// Java drops the Macintosh lookup's result and returns null there.
    pub fn select_name(&self, n: i32) -> FontResult<Option<String>> {
        let s = self.get_name(3, 1, ENGLISH_UNITED_STATES, n)?;
        if s.is_some() {
            return Ok(s);
        }
        let s = self.get_name(3, 0, ENGLISH_UNITED_STATES, n)?;
        if s.is_some() {
            return Ok(s);
        }
        self.get_name(1, 0, 0, n)?;
        Ok(s)
    }
}

/// `MacintoshLanguageCode.LANGUAGE_MAP`: code, language, script, region, variant.
const MACINTOSH_LANGUAGE_CODES: [(i32, &str, Option<&str>, Option<&str>, Option<&str>); 117] = [
    (0, "en", None, None, None),
    (1, "fr", None, None, None),
    (2, "de", None, None, None),
    (3, "it", None, None, None),
    (4, "nl", None, None, None),
    (5, "sv", None, None, None),
    (6, "es", None, None, None),
    (7, "da", None, None, None),
    (8, "pt", None, None, None),
    (9, "no", None, None, None),
    (10, "he", None, None, None),
    (11, "ja", None, None, None),
    (12, "ar", None, None, None),
    (13, "fi", None, None, None),
    (14, "iu", None, None, None),
    (15, "is", None, None, None),
    (16, "mt", None, None, None),
    (17, "tr", None, None, None),
    (18, "hr", None, None, None),
    (19, "zh", Some("Hant"), None, None),
    (20, "ur", None, None, None),
    (21, "hi", None, None, None),
    (22, "th", None, None, None),
    (23, "ko", None, None, None),
    (24, "lt", None, None, None),
    (25, "pl", None, None, None),
    (26, "hu", None, None, None),
    (27, "et", None, None, None),
    (28, "lv", None, None, None),
    (29, "se", None, None, None),
    (30, "fo", None, None, None),
    (31, "fa", None, None, None),
    (32, "ru", None, None, None),
    (33, "zh", Some("Hans"), None, None),
    (34, "nl", None, None, None),
    (35, "ga", None, None, None),
    (36, "sq", None, None, None),
    (37, "ro", None, None, None),
    (38, "cs", None, None, None),
    (39, "sk", None, None, None),
    (40, "sl", None, None, None),
    (41, "yi", None, None, None),
    (42, "sr", None, None, None),
    (43, "mk", None, None, None),
    (44, "bg", None, None, None),
    (45, "uk", None, None, None),
    (46, "be", None, None, None),
    (47, "uz", None, None, None),
    (48, "kk", None, None, None),
    (49, "az", Some("Cyrl"), None, None),
    (50, "az", Some("Arab"), None, None),
    (51, "hy", None, None, None),
    (52, "ka", None, None, None),
    (53, "mo", None, None, None),
    (54, "ky", None, None, None),
    (55, "tg", None, None, None),
    (56, "tk", None, None, None),
    (57, "mn", Some("Mong"), None, None),
    (58, "mn", Some("Cyrl"), None, None),
    (59, "ps", None, None, None),
    (60, "ku", None, None, None),
    (61, "ks", None, None, None),
    (62, "sd", None, None, None),
    (63, "bo", None, None, None),
    (64, "ne", None, None, None),
    (65, "sa", None, None, None),
    (66, "mr", None, None, None),
    (67, "bn", None, None, None),
    (68, "as", None, None, None),
    (69, "gu", None, None, None),
    (70, "pa", None, None, None),
    (71, "or", None, None, None),
    (72, "ml", None, None, None),
    (73, "kn", None, None, None),
    (74, "ta", None, None, None),
    (75, "te", None, None, None),
    (76, "si", None, None, None),
    (77, "my", None, None, None),
    (78, "km", None, None, None),
    (79, "lo", None, None, None),
    (80, "vi", None, None, None),
    (81, "id", None, None, None),
    (82, "tl", None, None, None),
    (83, "ms", Some("Latn"), None, None),
    (84, "ms", Some("Arab"), None, None),
    (85, "am", None, None, None),
    (86, "ti", None, None, None),
    (87, "om", None, None, None),
    (88, "so", None, None, None),
    (89, "sw", None, None, None),
    (90, "rw", None, None, None),
    (91, "rn", None, None, None),
    (92, "ny", None, None, None),
    (93, "mg", None, None, None),
    (94, "eo", None, None, None),
    (128, "cy", None, None, None),
    (129, "eu", None, None, None),
    (130, "ca", None, None, None),
    (131, "la", None, None, None),
    (132, "qu", None, None, None),
    (133, "gn", None, None, None),
    (134, "ay", None, None, None),
    (135, "tt", None, None, None),
    (136, "ug", None, None, None),
    (137, "dz", None, None, None),
    (138, "jv", Some("Latn"), None, None),
    (139, "su", Some("Latn"), None, None),
    (140, "gl", None, None, None),
    (141, "af", None, None, None),
    (142, "br", None, None, None),
    (144, "gd", None, None, None),
    (145, "gv", None, None, None),
    (146, "ga", Some("Latg"), None, None),
    (147, "to", None, None, None),
    (148, "grc", None, None, None),
    (149, "kl", None, None, None),
    (150, "az", Some("Latn"), None, None),
];

/// `MicrosoftLCID.LCID_MAP`: code, language, script, region, variant.
const MICROSOFT_LCIDS: [(i32, &str, Option<&str>, Option<&str>, Option<&str>); 202] = [
    (1025, "ar", None, Some("SA"), None),
    (1026, "bg", None, Some("BG"), None),
    (1027, "ca", None, Some("ES"), None),
    (1028, "zh", None, Some("TW"), None),
    (1029, "cs", None, Some("CZ"), None),
    (1030, "da", None, Some("DK"), None),
    (1031, "de", None, Some("DE"), None),
    (1032, "el", None, Some("GR"), None),
    (1033, "en", None, Some("US"), None),
    (1034, "es", None, Some("ES"), Some("TRADITIONAL")),
    (1035, "fil", None, Some("FI"), None),
    (1036, "fr", None, Some("FR"), None),
    (1037, "he", None, Some("IL"), None),
    (1038, "hu", None, Some("HU"), None),
    (1039, "is", None, Some("IS"), None),
    (1040, "it", None, Some("IT"), None),
    (1041, "ja", None, Some("JP"), None),
    (1042, "ko", None, Some("KR"), None),
    (1043, "nl", None, Some("NL"), None),
    (1044, "nb", None, Some("NO"), None),
    (1045, "pl", None, Some("PL"), None),
    (1046, "pt", None, Some("BR"), None),
    (1047, "rm", None, Some("CH"), None),
    (1048, "ro", None, Some("RO"), None),
    (1049, "ru", None, Some("RU"), None),
    (1050, "hr", None, Some("HR"), None),
    (1051, "sk", None, Some("SK"), None),
    (1052, "sq", None, Some("AL"), None),
    (1053, "sv", None, Some("SE"), None),
    (1054, "th", None, Some("TH"), None),
    (1055, "tr", None, Some("TR"), None),
    (1056, "ur", None, Some("PK"), None),
    (1057, "id", None, Some("ID"), None),
    (1058, "uk", None, Some("UA"), None),
    (1059, "be", None, Some("BY"), None),
    (1060, "sl", None, Some("SI"), None),
    (1061, "et", None, Some("EE"), None),
    (1062, "lv", None, Some("LV"), None),
    (1063, "lt", None, Some("LT"), None),
    (1064, "tg", Some("Cyrl"), Some("TJ"), None),
    (1066, "vi", None, Some("VN"), None),
    (1067, "hy", None, Some("AM"), None),
    (1068, "az", Some("Latn"), Some("AZ"), None),
    (1069, "eu", None, Some("ES"), None),
    (1070, "wen", None, Some("DE"), None),
    (1071, "mk", None, Some("MK"), None),
    (1074, "tn", None, Some("ZA"), None),
    (1076, "xh", None, Some("ZA"), None),
    (1077, "zu", None, Some("ZA"), None),
    (1078, "af", None, Some("ZA"), None),
    (1079, "ka", None, Some("GE"), None),
    (1080, "fo", None, Some("FO"), None),
    (1081, "hi", None, Some("IN"), None),
    (1082, "mt", None, Some("MT"), None),
    (1083, "se", None, Some("NO"), None),
    (1086, "ms", None, Some("MY"), None),
    (1087, "kk", None, Some("KZ"), None),
    (1088, "ky", None, Some("KG"), None),
    (1089, "sw", None, Some("KE"), None),
    (1090, "tk", None, Some("TM"), None),
    (1091, "uz", Some("Latn"), Some("UZ"), None),
    (1092, "tt", None, Some("RU"), None),
    (1093, "bn", None, Some("IN"), None),
    (1094, "pa", None, Some("IN"), None),
    (1095, "gu", None, Some("IN"), None),
    (1096, "wo", None, Some("SN"), None),
    (1097, "ta", None, Some("IN"), None),
    (1098, "te", None, Some("IN"), None),
    (1099, "kn", None, Some("IN"), None),
    (1100, "ml", None, Some("IN"), None),
    (1101, "as", None, Some("IN"), None),
    (1102, "mr", None, Some("IN"), None),
    (1103, "sa", None, Some("IN"), None),
    (1104, "mn", None, Some("MN"), None),
    (1105, "bo", None, Some("CN"), None),
    (1106, "cy", None, Some("GB"), None),
    (1107, "km", None, Some("KH"), None),
    (1108, "lo", None, Some("LA"), None),
    (1110, "gl", None, Some("ES"), None),
    (1111, "kok", None, Some("IN"), None),
    (1114, "syr", None, Some("SY"), None),
    (1115, "si", None, Some("LK"), None),
    (1117, "iu", None, Some("CA"), None),
    (1118, "am", None, Some("ET"), None),
    (1121, "ne", None, Some("NP"), None),
    (1122, "fy", None, Some("NL"), None),
    (1123, "ps", None, Some("AF"), None),
    (1124, "fil", None, Some("PH"), None),
    (1125, "dv", None, Some("MV"), None),
    (1128, "ha", None, Some("NG"), None),
    (1131, "qu", None, Some("BO"), None),
    (1132, "nso", None, Some("ZA"), None),
    (1133, "ba", None, Some("RU"), None),
    (1134, "lb", None, Some("LU"), None),
    (1135, "kl", None, Some("GL"), None),
    (1136, "ig", None, Some("NG"), None),
    (1146, "arn", None, Some("CL"), None),
    (1148, "moh", None, Some("CA"), None),
    (1150, "br", None, Some("FR"), None),
    (1152, "ug", None, Some("CN"), None),
    (1153, "mi", None, Some("NZ"), None),
    (1154, "oc", None, Some("FR"), None),
    (1155, "co", None, Some("FR"), None),
    (1156, "gsw", None, Some("FR"), None),
    (1157, "sah", None, Some("RU"), None),
    (1158, "qut", None, Some("GT"), None),
    (1159, "rw", None, Some("RW"), None),
    (1164, "fa", None, Some("AF"), None),
    (2049, "ar", None, Some("IQ"), None),
    (2052, "zh", None, Some("CN"), None),
    (2055, "de", None, Some("CH"), None),
    (2057, "en", None, Some("GB"), None),
    (2058, "es", None, Some("MX"), None),
    (2060, "fr", None, Some("BE"), None),
    (2064, "it", None, Some("CH"), None),
    (2067, "nl", None, Some("BE"), None),
    (2068, "nn", None, Some("NO"), None),
    (2070, "pt", None, Some("PT"), None),
    (2074, "sr", Some("Latn"), Some("RS"), None),
    (2077, "sv", None, Some("FI"), None),
    (2092, "az", Some("Cyrl"), Some("AZ"), None),
    (2094, "dsb", None, Some("DE"), None),
    (2107, "se", None, Some("SE"), None),
    (2108, "ga", None, Some("IE"), None),
    (2110, "ms", None, Some("BN"), None),
    (2115, "uz", Some("Cyrl"), Some("UZ"), None),
    (2117, "bn", None, Some("BD"), None),
    (2128, "mn", None, Some("CN"), None),
    (2141, "iu", Some("Latn"), Some("CA"), None),
    (2143, "ber", Some("Latn"), Some("DZ"), None),
    (2155, "qu", None, Some("EC"), None),
    (3073, "ar", None, Some("EG"), None),
    (3076, "zh", None, Some("HK"), None),
    (3079, "de", None, Some("AT"), None),
    (3081, "en", None, Some("AU"), None),
    (3082, "es", None, Some("ES"), None),
    (3084, "fr", None, Some("CA"), None),
    (3098, "sr", Some("Cyrl"), Some("RS"), None),
    (3131, "se", None, Some("FI"), None),
    (3179, "qu", None, Some("PE"), None),
    (4097, "ar", None, Some("LY"), None),
    (4100, "zh", None, Some("SG"), None),
    (4103, "de", None, Some("LU"), None),
    (4105, "en", None, Some("CA"), None),
    (4106, "es", None, Some("GT"), None),
    (4108, "fr", None, Some("CH"), None),
    (4122, "hr", None, Some("BA"), None),
    (4155, "smj", None, Some("NO"), None),
    (5121, "ar", None, Some("DZ"), None),
    (5124, "zh", None, Some("MO"), None),
    (5127, "de", None, Some("LI"), None),
    (5129, "en", None, Some("NZ"), None),
    (5130, "es", None, Some("CR"), None),
    (5132, "fr", None, Some("LU"), None),
    (5146, "bs", Some("Latn"), Some("BA"), None),
    (5179, "smj", None, Some("SE"), None),
    (6145, "ar", None, Some("MA"), None),
    (6153, "en", None, Some("IE"), None),
    (6154, "es", None, Some("PA"), None),
    (6156, "fr", None, Some("MC"), None),
    (6170, "sr", Some("Latn"), Some("BA"), None),
    (6203, "sma", None, Some("NO"), None),
    (7169, "ar", None, Some("TN"), None),
    (7177, "en", None, Some("ZA"), None),
    (7178, "es", None, Some("DO"), None),
    (7194, "sr", Some("Cyrl"), Some("BA"), None),
    (7227, "sma", None, Some("SE"), None),
    (8193, "ar", None, Some("OM"), None),
    (8201, "en", None, Some("JM"), None),
    (8202, "es", None, Some("VE"), None),
    (8218, "bs", Some("Cyrl"), Some("BA"), None),
    (8251, "sms", None, Some("FI"), None),
    (9217, "ar", None, Some("YE"), None),
    (9225, "en", None, Some("029"), None),
    (9226, "es", None, Some("CO"), None),
    (9275, "smn", None, Some("FI"), None),
    (10241, "ar", None, Some("SY"), None),
    (10249, "en", None, Some("BZ"), None),
    (10250, "es", None, Some("PE"), None),
    (11265, "ar", None, Some("JO"), None),
    (11273, "en", None, Some("TT"), None),
    (11274, "es", None, Some("AR"), None),
    (12289, "ar", None, Some("LB"), None),
    (12297, "en", None, Some("ZW"), None),
    (12298, "es", None, Some("EC"), None),
    (13313, "ar", None, Some("KW"), None),
    (13321, "en", None, Some("PH"), None),
    (13322, "es", None, Some("CL"), None),
    (14337, "ar", None, Some("AE"), None),
    (14346, "es", None, Some("UY"), None),
    (15361, "ar", None, Some("BH"), None),
    (15370, "es", None, Some("PY"), None),
    (16385, "ar", None, Some("QA"), None),
    (16393, "en", None, Some("IN"), None),
    (16394, "es", None, Some("BO"), None),
    (17417, "en", None, Some("MY"), None),
    (17418, "es", None, Some("SV"), None),
    (18441, "en", None, Some("SG"), None),
    (18442, "es", None, Some("HN"), None),
    (19466, "es", None, Some("NI"), None),
    (20490, "es", None, Some("PR"), None),
    (21514, "es", None, Some("US"), None),
];

