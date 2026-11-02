//! Port of afe.jar `com.adobe.fontengine.font.opentype.Post`.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::table::data_as_byte_array;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

pub const MAC_NAMES: [&str; 258] = [
    ".notdef", ".null", "nonmarkingreturn", "space", "exclam", "quotedbl", "numbersign", "dollar", "percent", "ampersand", "quotesingle",
    "parenleft", "parenright", "asterisk", "plus", "comma", "hyphen", "period", "slash", "zero", "one", "two", "three", "four", "five",
    "six", "seven", "eight", "nine", "colon", "semicolon", "less", "equal", "greater", "question", "at", "A", "B", "C", "D", "E", "F", "G",
    "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "bracketleft", "backslash",
    "bracketright", "asciicircum", "underscore", "grave", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p",
    "q", "r", "s", "t", "u", "v", "w", "x", "y", "z", "braceleft", "bar", "braceright", "asciitilde", "Adieresis", "Aring", "Ccedilla",
    "Eacute", "Ntilde", "Odieresis", "Udieresis", "aacute", "agrave", "acircumflex", "adieresis", "atilde", "aring", "ccedilla", "eacute",
    "egrave", "ecircumflex", "edieresis", "iacute", "igrave", "icircumflex", "idieresis", "ntilde", "oacute", "ograve", "ocircumflex",
    "odieresis", "otilde", "uacute", "ugrave", "ucircumflex", "udieresis", "dagger", "degree", "cent", "sterling", "section", "bullet",
    "paragraph", "germandbls", "registered", "copyright", "trademark", "acute", "dieresis", "notequal", "AE", "Oslash", "infinity",
    "plusminus", "lessequal", "greaterequal", "yen", "mu", "partialdiff", "summation", "product", "pi", "integral", "ordfeminine",
    "ordmasculine", "Omega", "ae", "oslash", "questiondown", "exclamdown", "logicalnot", "radical", "florin", "approxequal", "Delta",
    "guillemotleft", "guillemotright", "ellipsis", "nonbreakingspace", "Agrave", "Atilde", "Otilde", "OE", "oe", "endash", "emdash",
    "quotedblleft", "quotedblright", "quoteleft", "quoteright", "divide", "lozenge", "ydieresis", "Ydieresis", "fraction", "currency",
    "guilsinglleft", "guilsinglright", "fi", "fl", "daggerdbl", "periodcentered", "quotesinglbase", "quotedblbase", "perthousand",
    "Acircumflex", "Ecircumflex", "Aacute", "Edieresis", "Egrave", "Iacute", "Icircumflex", "Idieresis", "Igrave", "Oacute", "Ocircumflex",
    "apple", "Ograve", "Uacute", "Ucircumflex", "Ugrave", "dotlessi", "circumflex", "tilde", "macron", "breve", "dotaccent", "ring",
    "cedilla", "hungarumlaut", "ogonek", "caron", "Lslash", "lslash", "Scaron", "scaron", "Zcaron", "zcaron", "brokenbar", "Eth", "eth",
    "Yacute", "yacute", "Thorn", "thorn", "minus", "multiply", "onesuperior", "twosuperior", "threesuperior", "onehalf", "onequarter",
    "threequarters", "franc", "Gbreve", "gbreve", "Idotaccent", "Scedilla", "scedilla", "Cacute", "cacute", "Ccaron", "ccaron", "dcroat",
];

#[derive(Clone, Debug, Default)]
pub struct Post {
    pub data: OTByteArray,
}

impl Post {
    pub fn new(a: FontByteArray) -> FontResult<Post> {
        let size = a.get_size();
        let t = Post { data: OTByteArray::from_font_byte_array(a) };
        if size < 32 {
            return Err(FontEngineException::invalid_font("'post' table must be at least 32 bytes"));
        }
        let n = t.get_table_major_version()?;
        if !(1..=4).contains(&n) {
            return Err(FontEngineException::unsupported_font(format!("'post' tables with major version {} are not supported", n)));
        }
        Ok(t)
    }

    pub fn get_table_major_version(&self) -> FontResult<i32> {
        self.data.getuint16(0)
    }

    pub fn get_table_minor_version(&self) -> FontResult<i32> {
        self.data.getuint16(2)
    }

    pub fn get_italic_angle(&self) -> FontResult<i32> {
        self.data.getint32(4)
    }

    pub fn get_underline_position(&self) -> FontResult<i32> {
        self.data.getint16(8)
    }

    pub fn get_underline_thickness(&self) -> FontResult<i32> {
        self.data.getint16(10)
    }

    pub fn is_fixed_pitch(&self) -> FontResult<bool> {
        Ok(self.data.getuint32(12)? != 0)
    }

    pub fn get_number_of_glyphs(&self) -> FontResult<i32> {
        if self.get_table_major_version()? != 2 {
            return Ok(-1);
        }
        self.data.getuint16(32)
    }

    pub fn get_glyph_name(&self, n: i32) -> Option<String> {
        self.get_glyph_name_impl(n).unwrap_or(None)
    }

    fn get_glyph_name_impl(&self, n: i32) -> FontResult<Option<String>> {
        match self.get_table_major_version()? {
            1 => {
                if n >= MAC_NAMES.len() as i32 {
                    return Ok(None);
                }
                Ok(Some(MAC_NAMES[n as usize].to_string()))
            }
            2 => {
                if self.get_table_minor_version()? < 5 {
                    let n3 = self.data.getuint16(32)?;
                    if n >= n3 {
                        return Ok(None);
                    }
                    let n4 = self.data.getuint16(34 + n * 2)?;
                    if n4 < 258 {
                        return Ok(Some(MAC_NAMES[n4 as usize].to_string()));
                    }
                    let mut n5 = 34 + n3 * 2;
                    for _ in 258..n4 {
                        let n6 = self.data.getuint8(n5)?;
                        n5 += 1 + n6;
                    }
                    let n2 = self.data.getuint8(n5)?;
                    let mut c = Vec::with_capacity(n2 as usize);
                    for _ in 0..n2 {
                        n5 += 1;
                        c.push(self.data.getuint8(n5)? as u16);
                    }
                    return Ok(Some(String::from_utf16_lossy(&c)));
                }
                let n7 = self.data.getint8(34 + n * 2)?;
                Ok(Some(MAC_NAMES[(n + n7) as usize].to_string()))
            }
            _ => Ok(None),
        }
    }

    pub fn glyph_name2gid(&self, s: &str) -> FontResult<i32> {
        match self.get_table_major_version()? {
            1 => Ok(MAC_NAMES.iter().position(|m| *m == s).map(|i| i as i32).unwrap_or(0)),
            2 => {
                let n = self.data.getuint16(32)?;
                for i in 0..n {
                    if self.get_glyph_name(i).as_deref() == Some(s) {
                        return Ok(i);
                    }
                }
                Ok(0)
            }
            _ => Ok(0),
        }
    }

    pub fn subset_and_stream_for_cff(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) -> FontResult<()> {
        let mut b = OTByteArray::get_ot_byte_array_builder_instance_sized(32);
        b.setuint32(0, 196608);
        b.setuint32(4, self.data.getint32(4)?);
        b.setuint16(8, self.data.getint16(8)?);
        b.setuint16(10, self.data.getint16(10)?);
        b.setuint32(12, self.data.getint32(12)?);
        b.setuint32(16, 0);
        b.setuint32(20, 0);
        b.setuint32(24, 0);
        b.setuint32(28, 0);
        map.insert(1886352244, b);
        Ok(())
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1886352244, data_as_byte_array(&self.data));
    }
}
