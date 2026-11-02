//! Port of afe.jar `com.adobe.fontengine.font.cff.StringIndex`.

use std::ops::Deref;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::{CFFByteArray, CFFByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::index::Index;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::name_index::NameIndex;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[derive(Clone, Debug)]
pub struct StringIndex {
    pub base: NameIndex,
}

impl Deref for StringIndex {
    type Target = NameIndex;
    fn deref(&self) -> &NameIndex {
        &self.base
    }
}

impl StringIndex {
    pub fn new(data: Rc<CFFByteArray>, n: i32) -> FontResult<StringIndex> {
        Ok(StringIndex { base: NameIndex::new(data, n)? })
    }

    pub fn get_string(&self, n: i32) -> FontResult<Option<String>> {
        if n < PREDEFINED_STRINGS.len() as i32 {
            // Java indexes the array with a negative n and throws.
            assert!(n >= 0, "java.lang.ArrayIndexOutOfBoundsException: Index {} out of bounds for length 391", n);
            return Ok(Some(PREDEFINED_STRINGS[n as usize].to_string()));
        }
        self.base.get_nth_name(n - PREDEFINED_STRINGS.len() as i32)
    }

    pub fn to_binary(b: &mut CFFByteArrayBuilder, list: &[String]) {
        let n = list.len() as i32 - PREDEFINED_STRINGS.len() as i32;
        let mut cursor = Index::start_index(b, n);
        for s in &list[PREDEFINED_STRINGS.len()..] {
            for c in s.encode_utf16() {
                b.add_card8(c as i32);
            }
            cursor = Index::element_entered(b, cursor);
        }
    }

    pub fn collect_predefined_strings() -> Vec<String> {
        PREDEFINED_STRINGS.iter().map(|s| s.to_string()).collect()
    }
}

/// `List.indexOf(string)`.
pub fn index_of(list: &[String], s: &str) -> i32 {
    list.iter().position(|x| x == s).map_or(-1, |p| p as i32)
}

pub static PREDEFINED_STRINGS: [&str; 391] = [
    ".notdef", "space", "exclam", "quotedbl", "numbersign", "dollar", "percent", "ampersand", "quoteright", "parenleft",
    "parenright", "asterisk", "plus", "comma", "hyphen", "period", "slash", "zero", "one", "two",
    "three", "four", "five", "six", "seven", "eight", "nine", "colon", "semicolon", "less",
    "equal", "greater", "question", "at", "A", "B", "C", "D", "E", "F",
    "G", "H", "I", "J", "K", "L", "M", "N", "O", "P",
    "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
    "bracketleft", "backslash", "bracketright", "asciicircum", "underscore", "quoteleft", "a", "b", "c", "d",
    "e", "f", "g", "h", "i", "j", "k", "l", "m", "n",
    "o", "p", "q", "r", "s", "t", "u", "v", "w", "x",
    "y", "z", "braceleft", "bar", "braceright", "asciitilde", "exclamdown", "cent", "sterling", "fraction",
    "yen", "florin", "section", "currency", "quotesingle", "quotedblleft", "guillemotleft", "guilsinglleft", "guilsinglright", "fi",
    "fl", "endash", "dagger", "daggerdbl", "periodcentered", "paragraph", "bullet", "quotesinglbase", "quotedblbase", "quotedblright",
    "guillemotright", "ellipsis", "perthousand", "questiondown", "grave", "acute", "circumflex", "tilde", "macron", "breve",
    "dotaccent", "dieresis", "ring", "cedilla", "hungarumlaut", "ogonek", "caron", "emdash", "AE", "ordfeminine",
    "Lslash", "Oslash", "OE", "ordmasculine", "ae", "dotlessi", "lslash", "oslash", "oe", "germandbls",
    "onesuperior", "logicalnot", "mu", "trademark", "Eth", "onehalf", "plusminus", "Thorn", "onequarter", "divide",
    "brokenbar", "degree", "thorn", "threequarters", "twosuperior", "registered", "minus", "eth", "multiply", "threesuperior",
    "copyright", "Aacute", "Acircumflex", "Adieresis", "Agrave", "Aring", "Atilde", "Ccedilla", "Eacute", "Ecircumflex",
    "Edieresis", "Egrave", "Iacute", "Icircumflex", "Idieresis", "Igrave", "Ntilde", "Oacute", "Ocircumflex", "Odieresis",
    "Ograve", "Otilde", "Scaron", "Uacute", "Ucircumflex", "Udieresis", "Ugrave", "Yacute", "Ydieresis", "Zcaron",
    "aacute", "acircumflex", "adieresis", "agrave", "aring", "atilde", "ccedilla", "eacute", "ecircumflex", "edieresis",
    "egrave", "iacute", "icircumflex", "idieresis", "igrave", "ntilde", "oacute", "ocircumflex", "odieresis", "ograve",
    "otilde", "scaron", "uacute", "ucircumflex", "udieresis", "ugrave", "yacute", "ydieresis", "zcaron", "exclamsmall",
    "Hungarumlautsmall", "dollaroldstyle", "dollarsuperior", "ampersandsmall", "Acutesmall", "parenleftsuperior", "parenrightsuperior", "twodotenleader", "onedotenleader", "zerooldstyle",
    "oneoldstyle", "twooldstyle", "threeoldstyle", "fouroldstyle", "fiveoldstyle", "sixoldstyle", "sevenoldstyle", "eightoldstyle", "nineoldstyle", "commasuperior",
    "threequartersemdash", "periodsuperior", "questionsmall", "asuperior", "bsuperior", "centsuperior", "dsuperior", "esuperior", "isuperior", "lsuperior",
    "msuperior", "nsuperior", "osuperior", "rsuperior", "ssuperior", "tsuperior", "ff", "ffi", "ffl", "parenleftinferior",
    "parenrightinferior", "Circumflexsmall", "hyphensuperior", "Gravesmall", "Asmall", "Bsmall", "Csmall", "Dsmall", "Esmall", "Fsmall",
    "Gsmall", "Hsmall", "Ismall", "Jsmall", "Ksmall", "Lsmall", "Msmall", "Nsmall", "Osmall", "Psmall",
    "Qsmall", "Rsmall", "Ssmall", "Tsmall", "Usmall", "Vsmall", "Wsmall", "Xsmall", "Ysmall", "Zsmall",
    "colonmonetary", "onefitted", "rupiah", "Tildesmall", "exclamdownsmall", "centoldstyle", "Lslashsmall", "Scaronsmall", "Zcaronsmall", "Dieresissmall",
    "Brevesmall", "Caronsmall", "Dotaccentsmall", "Macronsmall", "figuredash", "hypheninferior", "Ogoneksmall", "Ringsmall", "Cedillasmall", "questiondownsmall",
    "oneeighth", "threeeighths", "fiveeighths", "seveneighths", "onethird", "twothirds", "zerosuperior", "foursuperior", "fivesuperior", "sixsuperior",
    "sevensuperior", "eightsuperior", "ninesuperior", "zeroinferior", "oneinferior", "twoinferior", "threeinferior", "fourinferior", "fiveinferior", "sixinferior",
    "seveninferior", "eightinferior", "nineinferior", "centinferior", "dollarinferior", "periodinferior", "commainferior", "Agravesmall", "Aacutesmall", "Acircumflexsmall",
    "Atildesmall", "Adieresissmall", "Aringsmall", "AEsmall", "Ccedillasmall", "Egravesmall", "Eacutesmall", "Ecircumflexsmall", "Edieresissmall", "Igravesmall",
    "Iacutesmall", "Icircumflexsmall", "Idieresissmall", "Ethsmall", "Ntildesmall", "Ogravesmall", "Oacutesmall", "Ocircumflexsmall", "Otildesmall", "Odieresissmall",
    "OEsmall", "Oslashsmall", "Ugravesmall", "Uacutesmall", "Ucircumflexsmall", "Udieresissmall", "Yacutesmall", "Thornsmall", "Ydieresissmall", "001.000",
    "001.001", "001.002", "001.003", "Black", "Bold", "Book", "Light", "Medium", "Regular", "Roman",
    "Semibold",
];
