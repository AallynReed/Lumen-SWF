//! Port of afe.jar `com.adobe.fontengine.font.opentype.Fond`.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::charset_util::CharsetUtil;
use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::name::Name;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::table::data_as_byte_array;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

const SCRIPTID_TO_ULOCALE: [Option<&str>; 33] = [
    Some("en"), Some("ja"), Some("zh-Hant"), Some("ko"), Some("ar"), Some("he"), Some("el"), Some("ru"), Some("MacSymbol"), Some("hi"), Some("pa"), Some("gu"),
    None, None, None, None, None, None, None, None, None, Some("th"), None, None, None, Some("zh-Hans"), None, None, None, Some("MacCentralEuropean"), None, None, None,
];

#[derive(Clone, Debug, Default)]
pub struct Fond {
    pub data: OTByteArray,
}

impl Fond {
    pub fn new(a: FontByteArray) -> Fond {
        Fond { data: OTByteArray::from_font_byte_array(a) }
    }

    pub fn subset_and_stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1718578788, OTByteArray::get_ot_byte_array_builder_instance_from(&self.data));
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1718578788, data_as_byte_array(&self.data));
    }

    pub fn get_data(&self) -> &OTByteArray {
        &self.data
    }

    /// Java decodes with the JDK charset of that name (replacing bad bytes); the ICU tables stand in for it here.
    pub fn get_name(&self) -> FontResult<Option<String>> {
        let n = self.data.getint16(16)?;
        let n2 = self.data.getint8(28)?;
        let bytes = self.data.base.get_bytes(29, n2);
        let mut s = Name::get_charset_name(1, n);
        if s.is_none() {
            s = Name::get_charset_name(1, 0);
        }
        Ok(CharsetUtil::decode_replacing(s.unwrap(), &bytes))
    }

    pub fn is_bold(&self) -> FontResult<bool> {
        Ok(self.data.getint16(14)? & 1 != 0)
    }

    pub fn is_italic(&self) -> FontResult<bool> {
        Ok(self.data.getint16(14)? & 2 != 0)
    }

    /// `ULocale` stands as its locale string.
    pub fn get_locale(&self) -> FontResult<String> {
        let n = self.data.getint16(16)?.min(SCRIPTID_TO_ULOCALE.len() as i32 - 1);
        Ok(SCRIPTID_TO_ULOCALE[n as usize].unwrap_or(SCRIPTID_TO_ULOCALE[0].unwrap()).to_string())
    }
}
