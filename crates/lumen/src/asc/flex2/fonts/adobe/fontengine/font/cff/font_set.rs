//! Port of afe.jar `com.adobe.fontengine.font.cff.FontSet`.

use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::CFFByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_font::AnyCFFFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::char_strings::CharStrings;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cid_keyed_font::CIDKeyedFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::dict::{Dict, Key};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::header::Header;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::index::Index;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::name_index::NameIndex;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::name_keyed_font::NameKeyedFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::string_index::StringIndex;
use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

pub struct FontSet {
    pub data: Rc<CFFByteArray>,
    pub header: Header,
    pub name_index: NameIndex,
    pub top_dict_index: Index,
    pub string_index: Rc<StringIndex>,
    pub global_subrs: CharStrings,
    pub fonts: Vec<Option<AnyCFFFont>>,
}

impl FontSet {
    /// Synthetic fonts (`SyntheticBase`) are not ported and fail as unsupported.
    pub fn new(a: FontByteArray) -> FontResult<FontSet> {
        let size = a.get_size();
        let data = Rc::new(CFFByteArray::from_font_byte_array(a));
        if size < 4 {
            return Err(FontEngineException::invalid_font("CFFFontSet too small"));
        }
        let mut n2 = 0;
        let header = Header::new(data.clone(), n2);
        n2 += header.size()?;
        let n3 = header.get_major_version()?;
        if n3 != 1 {
            return Err(FontEngineException::unsupported_font(format!("CFFFontSet major version {} not supported", n3)));
        }
        let name_index = NameIndex::new(data.clone(), n2)?;
        n2 += name_index.size();
        let top_dict_index = Index::new(data.clone(), n2)?;
        n2 += top_dict_index.size();
        let string_index = Rc::new(StringIndex::new(data.clone(), n2)?);
        n2 += string_index.size();
        let global_subrs = CharStrings::new(data.clone(), n2)?;
        let count = name_index.get_count();
        let mut fonts: Vec<Option<AnyCFFFont>> = (0..count).map(|_| None).collect();
        for n in 0..count {
            let s = name_index.get_nth_name(n)?.unwrap_or_default();
            let first = s.encode_utf16().next().expect("java.lang.StringIndexOutOfBoundsException: empty font name");
            if first == 0 {
                continue;
            }
            let dict = Dict::parse(&data, top_dict_index.offset_of(n), top_dict_index.size_of(n), Some(&string_index))?;
            if dict.get_ros_value(&Key::ROS, false).is_some() {
                fonts[n as usize] = Some(AnyCFFFont::CIDKeyed(CIDKeyedFont::parse(string_index.clone(), global_subrs.clone(), s, dict, &data)?));
                continue;
            }
            if dict.get_integer(&Key::SYNTHETIC_BASE, false).is_some() {
                return Err(FontEngineException::unsupported_font("synthetic CFF fonts are not supported"));
            }
            fonts[n as usize] = Some(AnyCFFFont::NameKeyed(NameKeyedFont::parse(string_index.clone(), global_subrs.clone(), s, dict, &data)?));
        }
        Ok(FontSet { data, header, name_index, top_dict_index, string_index, global_subrs, fonts })
    }
}
