//! Port of afe.jar `com.adobe.fontengine.font.cff.NameKeyedFont` (the parts the font embedding uses).

use std::io::Write;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::{CFFByteArray, CFFByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_font::{create_subset_charstring_index, CFFFont, CFFFontImpl};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_subrize::CFFSubrize;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::char_strings::CharStrings;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::charset::Charset;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cid_component_font::{CIDComponentFont, WidthConsumer};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cid_keyed_font::{io_err, CIDKeyedFont, PRIVATE_DICT_KEYS_FOR_SUBSET};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::dict::{Dict, IntegerValue, Key, ROSValue, StringValue, Value};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::encoding::Encoding;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::fd_select::FdSelect;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::header::Header;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::index::Index;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::name_index::NameIndex;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::string_index::{index_of, StringIndex};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_outline_parser::{OutlineTarget, Type2OutlineParser};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_parser::Type2Parser;
use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;
use crate::asc::flex2::fonts::adobe::fontengine::font::orig_font_type::OrigFontType;
use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::permission::Permission;
use crate::asc::flex2::fonts::adobe::fontengine::font::postscript::postscript_token_parser;
use crate::asc::flex2::fonts::adobe::fontengine::font::rect::Rect;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset_default_impl::SubsetDefaultImpl;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

const MAX_CFF_SIZE_FOR_SUBRIZE: i32 = 614400;

pub const TOP_DICT_KEYS_FOR_SUBSET: [Key; 20] = [
    Key::NOTICE,
    Key::FULL_NAME,
    Key::FAMILY_NAME,
    Key::FONT_NAME,
    Key::BASE_FONT_NAME,
    Key::BASE_FONT_BLEND,
    Key::WEIGHT,
    Key::FONT_BBOX,
    Key::COPYRIGHT,
    Key::IS_FIXED_PITCH,
    Key::ITALIC_ANGLE,
    Key::UNDERLINE_POSITION,
    Key::UNDERLINE_THICKNESS,
    Key::PAINT_TYPE,
    Key::CHARSTRING_TYPE,
    Key::STROKE_WIDTH,
    Key::UNIQUE_ID,
    Key::XUID,
    Key::UID_BASE,
    Key::FONT_MATRIX,
];

/// `NameKeyedFont.GlyphNameFetcher`.
pub trait GlyphNameFetcher {
    fn get_glyph_name(&self, subset: &dyn Subset, n: i32) -> String;
}

#[derive(Clone, Debug)]
pub struct NameKeyedFont {
    pub base: CFFFont,
    pub private_dict: Option<Dict>,
    pub charset: Option<Charset>,
    pub encoding: Option<Encoding>,
    pub char_strings: CharStrings,
    pub local_subrs: Option<CharStrings>,
}

impl NameKeyedFont {
    /// The constructor `FontSet` uses.
    pub fn parse(si: Rc<StringIndex>, gsubrs: CharStrings, name: String, dict: Dict, data: &Rc<CFFByteArray>) -> FontResult<NameKeyedFont> {
        let ov = match dict.get_offset(&Key::CHAR_STRINGS, false) {
            None => return Err(FontEngineException::invalid_font("missing TopDICT/CharStrings")),
            Some(v) => v,
        };
        let char_strings = CharStrings::new(data.clone(), ov.offset)?;
        let n = char_strings.get_count();
        let cso = dict.get_offset(&Key::CHARSET, true).unwrap();
        let charset = Charset::new(data.clone(), cso.offset, n)?;
        let eo = dict.get_offset(&Key::ENCODING, true).unwrap();
        let encoding = Encoding::new(data.clone(), eo.offset);
        let mut private_dict = None;
        let mut local_subrs = None;
        if let Some(osv) = dict.get_offset_size(&Key::PRIVATE, false) {
            let pd = Dict::parse(data, osv.offset, osv.size, Some(&si))?;
            if let Some(ov) = pd.get_offset(&Key::SUBRS, false) {
                local_subrs = Some(CharStrings::new(data.clone(), osv.offset + ov.offset)?);
            }
            private_dict = Some(pd);
        }
        Ok(NameKeyedFont { base: CFFFont::new(Some(si), Some(gsubrs), dict, name), private_dict, charset: Some(charset), encoding: Some(encoding), char_strings, local_subrs })
    }

    fn charset(&self) -> &Charset {
        self.charset.as_ref().expect("java.lang.NullPointerException: no charset")
    }

    fn private_dict(&self) -> &Dict {
        self.private_dict.as_ref().expect("java.lang.NullPointerException: no Private DICT")
    }

    fn string_index(&self) -> &StringIndex {
        self.base.string_index.as_deref().expect("java.lang.NullPointerException: no StringIndex")
    }

    pub fn get_num_glyphs(&self) -> i32 {
        self.char_strings.get_count()
    }

    pub fn glyph_name2gid(&self, s: &str) -> FontResult<i32> {
        for i in 0..self.get_num_glyphs() {
            if self.string_index().get_string(self.charset().gid2sid(i)?)?.as_deref() == Some(s) {
                return Ok(i);
            }
        }
        Ok(0)
    }

    pub fn char_code2gid(&self, n: i32) -> FontResult<i32> {
        self.encoding.as_ref().expect("java.lang.NullPointerException: no encoding").char_code2gid(n, self.charset())
    }

    pub fn get_glyph_name(&self, n: i32) -> FontResult<Option<String>> {
        self.string_index().get_string(self.charset().gid2sid(n)?)
    }

    pub fn get_glyph_cid(&self, _n: i32) -> i32 {
        -1
    }

    pub fn get_glyph_outline(&self, n: i32, oc: &mut dyn OutlineConsumer) -> FontResult<()> {
        self.get_outline(n, &mut Type2OutlineParser::new(false), OutlineTarget::Plain(oc))
    }

    pub fn get_outline(&self, n: i32, p: &mut Type2OutlineParser, oc: OutlineTarget<'_>) -> FontResult<()> {
        p.parse(&self.char_strings, n, self.local_subrs.as_ref(), self.base.global_subrs.as_ref(), oc, &self.get_font_matrix(), Some(self))
    }

    pub fn get_stem_v_for_glyph(&self, _n: i32) -> f64 {
        let pd = self.private_dict();
        if let Some(nv) = pd.get_numbers(&Key::STD_VW, false) {
            return nv.get_first_value_as_double();
        }
        if let Some(nv) = pd.get_numbers(&Key::STEM_SNAP_V, false) {
            return nv.get_first_value_as_double();
        }
        0.0
    }

    pub fn get_horizontal_advance(&self, n: i32) -> FontResult<f64> {
        let mut p = Type2Parser::new();
        let mut wc = WidthConsumer::default();
        p.parse(&self.char_strings, n, self.local_subrs.as_ref(), self.base.global_subrs.as_ref(), &mut wc, Some(self))?;
        if wc.width_seen {
            return Ok(self.private_dict().get_numbers(&Key::NOMINAL_WIDTH_X, true).unwrap().get_first_value_as_double() + wc.width);
        }
        Ok(self.private_dict().get_numbers(&Key::DEFAULT_WIDTH_X, true).unwrap().get_first_value_as_double())
    }

    fn get_font_to_metrics_matrix(&self) -> Matrix {
        let m = self.get_font_matrix();
        let d = self.get_units_per_em_x();
        let d2 = self.get_units_per_em_y();
        Matrix::new(d * m.a, d2 * m.b, d * m.c, d2 * m.d, d * m.tx, d2 * m.ty)
    }

    pub fn get_raw_font_bbox(&self) -> Option<Rect> {
        let nv = self.base.top_dict.get_numbers(&Key::FONT_BBOX, false)?;
        if nv.values.len() == 4 {
            return Some(Rect::from_array(&nv.get_values_as_double()));
        }
        None
    }

    pub fn get_font_bbox(&self) -> Option<Rect> {
        let r = self.get_raw_font_bbox()?;
        Some(r.apply_matrix(&self.get_font_to_metrics_matrix()))
    }

    pub fn get_embedding_permission(&self, bl: bool) -> Permission {
        self.base.get_embedding_permission_given_ft(bl, OrigFontType::K_TYPE1)
    }

    pub fn to_cid(&self, subset: &dyn Subset) -> FontResult<CIDKeyedFont> {
        let td = &self.base.top_dict;
        let mut m: Vec<(Key, Value)> = Vec::new();
        m.push((Key::ROS, Value::Ros(ROSValue::new("Adobe".into(), "Identity".into(), 0))));
        m.push((Key::CID_COUNT, Value::Integer(IntegerValue::new(subset.get_num_glyphs()))));
        if let Some(v) = td.get_string(&Key::NOTICE, false) {
            m.push((Key::NOTICE, Value::String(v)));
        }
        if let Some(v) = td.get_string(&Key::FULL_NAME, false) {
            m.push((Key::FULL_NAME, Value::String(v)));
        }
        m.push((Key::FONT_NAME, Value::String(StringValue::new(self.base.name.clone()))));
        if let Some(v) = td.get_string(&Key::FAMILY_NAME, false) {
            m.push((Key::FAMILY_NAME, Value::String(v)));
        }
        if let Some(v) = td.get_string(&Key::WEIGHT, false) {
            m.push((Key::WEIGHT, Value::String(v)));
        }
        if let Some(v) = td.get_numbers(&Key::FONT_BBOX, false) {
            m.push((Key::FONT_BBOX, Value::Numbers(v)));
        }
        if let Some(v) = td.get_string(&Key::COPYRIGHT, false) {
            m.push((Key::COPYRIGHT, Value::String(v)));
        }
        for k in [Key::IS_FIXED_PITCH, Key::ITALIC_ANGLE, Key::UNDERLINE_POSITION, Key::UNDERLINE_THICKNESS, Key::PAINT_TYPE, Key::CHARSTRING_TYPE, Key::STROKE_WIDTH] {
            if let Some(v) = td.get_numbers(&k, false) {
                m.push((k, Value::Numbers(v)));
            }
        }
        match td.get_string(&Key::POST_SCRIPT, false) {
            Some(sv) => {
                if postscript_token_parser::get_orig_font_type(Some(&sv.value)).is_some() {
                    m.push((Key::POST_SCRIPT, Value::String(sv)));
                } else {
                    m.push((Key::POST_SCRIPT, Value::String(StringValue::new(format!("/OrigFontType /Type1 def {}", sv.value)))));
                }
            }
            None => m.push((Key::POST_SCRIPT, Value::String(StringValue::new("/OrigFontType /Type1 def")))),
        }
        if let Some(v) = td.get_numbers(&Key::CID_FONT_TYPE, false) {
            m.push((Key::CID_FONT_TYPE, Value::Numbers(v)));
        }
        if let Some(v) = td.get_numbers(&Key::UID_BASE, false) {
            m.push((Key::UID_BASE, Value::Numbers(v)));
        }
        let top = Dict::from_map(m)?;
        let mut m2: Vec<(Key, Value)> = Vec::new();
        if let Some(v) = td.get_numbers(&Key::FONT_MATRIX, false) {
            m2.push((Key::FONT_MATRIX, Value::Numbers(v)));
        }
        let fd = Dict::from_map(m2)?;
        let pd = self.private_dict.clone().expect("java.lang.NullPointerException: no Private DICT");
        let comps = vec![CIDComponentFont::new(fd, pd, None)];
        let cs = create_subset_charstring_index(self, subset, false)?;
        Ok(CIDKeyedFont::new_internal(
            None,
            None,
            self.base.name.clone(),
            top,
            cs,
            Charset::identity_charset(subset.get_num_glyphs()),
            comps,
            FdSelect::single_font(subset.get_num_glyphs())?,
        ))
    }

    /// `subsetAndStream(Subset, OutputStream, boolean, Integer)`.
    pub fn subset_and_stream4(&self, subset: &dyn Subset, out: &mut dyn Write, _bl: bool, n: Option<i32>) -> FontResult<()> {
        let sdi = SubsetDefaultImpl::new(subset.get_num_glyphs(), false);
        self.to_cid(subset)?.subset_and_stream_sdi(&sdi, Some(false), out, false, n, false, true)
    }

    /// `subsetAndStream(Subset, OutputStream, boolean, Integer, boolean)`.
    pub fn subset_and_stream5(&self, subset: &dyn Subset, out: &mut dyn Write, _bl: bool, n: Option<i32>, bl2: bool) -> FontResult<()> {
        let sdi = SubsetDefaultImpl::new(subset.get_num_glyphs(), false);
        self.to_cid(subset)?.subset_and_stream_sdi(&sdi, Some(false), out, false, n, bl2, true)
    }

    fn set_ps_string(&self, b: &mut CFFByteArrayBuilder, n: Option<i32>, list: &mut Vec<String>) {
        let mut s: Option<String> = None;
        let n = n.or_else(|| self.base.top_dict.get_fs_type());
        if let Some(n) = n {
            s = Some(format!("/FSType {} def ", n));
        }
        if let Some(oft) = self.base.top_dict.get_orig_font_type() {
            s = Some(match s {
                Some(x) => format!("{} /OrigFontType /{} def", x, oft.to_java_string()),
                None => format!("/OrigFontType /{} def", oft.to_java_string()),
            });
        }
        if let Some(s) = s {
            if index_of(list, &s) == -1 {
                list.push(s.clone());
            }
            Dict::stream_key_val_string(b, &Key::POST_SCRIPT, &s, list);
        }
    }

    pub fn stream(&self, out: &mut dyn Write, n: Option<i32>) -> FontResult<()> {
        let sdi = SubsetDefaultImpl::new(self.get_num_glyphs(), false);
        self.subset_and_stream_fetcher(&sdi, out, n, false, None)
    }

    fn build_charset_data(&self, subset: &dyn Subset, f: Option<&dyn GlyphNameFetcher>, list: &mut Vec<String>) -> FontResult<Vec<i32>> {
        let mut a = vec![0i32; subset.get_num_glyphs() as usize];
        for i in 0..a.len() {
            let s = match f {
                None => self.get_glyph_name(subset.get_full_gid(i as i32))?.unwrap_or_default(),
                Some(f) => f.get_glyph_name(subset, i as i32),
            };
            let mut n = index_of(list, &s);
            if n < 0 {
                list.push(s);
                n = list.len() as i32 - 1;
            }
            a[i] = n;
        }
        Ok(a)
    }

    fn check_subrize_limit(&self, subset: &dyn Subset) -> bool {
        let n = self.char_strings.data.get_size();
        if n <= MAX_CFF_SIZE_FOR_SUBRIZE {
            return true;
        }
        n as f64 * subset.get_num_glyphs() as f64 <= 614400.0 * self.get_num_glyphs() as f64
    }

    /// `subsetAndStream(Subset, OutputStream, Integer, boolean, GlyphNameFetcher)`.
    pub fn subset_and_stream_fetcher(&self, subset: &dyn Subset, out: &mut dyn Write, n: Option<i32>, bl: bool, f: Option<&dyn GlyphNameFetcher>) -> FontResult<()> {
        self.subset_and_stream_fetcher6(subset, out, n, bl, f, false)
    }

    /// `subsetAndStream(Subset, OutputStream, Integer, boolean, GlyphNameFetcher, boolean)`.
    pub fn subset_and_stream_fetcher6(&self, subset: &dyn Subset, out: &mut dyn Write, n: Option<i32>, bl: bool, f: Option<&dyn GlyphNameFetcher>, mut bl2: bool) -> FontResult<()> {
        let mut list = StringIndex::collect_predefined_strings();
        let charset_data = self.build_charset_data(subset, f, &mut list)?;
        self.base.top_dict.collect_strings(&mut list);
        if let Some(sv) = self.base.top_dict.get_string(&Key::POST_SCRIPT, false) {
            let i = index_of(&list, &sv.value);
            list.remove(i as usize);
        }
        let mut b = CFFByteArray::get_cff_byte_array_builder_instance();
        Header::to_binary(&mut b);
        NameIndex::to_binary(&mut b, &self.base.name);
        let cursor = Index::start_index(&mut b, 1);
        self.base.top_dict.stream(&mut b, &list, &TOP_DICT_KEYS_FOR_SUBSET)?;
        self.set_ps_string(&mut b, n, &mut list);
        let n2 = Key::CHARSET.stream_dummy_value(&mut b);
        let n3 = Key::CHAR_STRINGS.stream_dummy_value(&mut b);
        let n4 = Key::PRIVATE.stream_dummy_value(&mut b);
        Index::element_entered(&mut b, cursor);
        StringIndex::to_binary(&mut b, &list);
        let mut lsubrs: Option<CharStrings> = None;
        let cs_owned: Option<CharStrings>;
        if bl {
            if bl2 {
                bl2 = self.check_subrize_limit(subset);
            }
            let mut cs = create_subset_charstring_index(self, subset, bl2)?;
            if bl2 {
                let mut s = CFFSubrize::new();
                cs = s.subrize(cs);
                lsubrs = s.get_l_subrs();
                let g = match s.get_g_subrs() {
                    Some(g) => g,
                    None => CharStrings::create_empty_charstrings()?,
                };
                g.stream(&mut b)?;
            } else {
                b.add_card16(0);
            }
            cs_owned = Some(cs);
        } else {
            cs_owned = None;
            self.base.global_subrs.as_ref().expect("java.lang.NullPointerException: global subrs").stream(&mut b)?;
        }
        let char_strings = cs_owned.as_ref().unwrap_or(&self.char_strings);
        let size = b.get_size();
        Key::CHARSET.fix_offset(&mut b, n2, size);
        Charset::stream_char_set(&mut b, &charset_data);
        let size = b.get_size();
        Key::CHAR_STRINGS.fix_offset(&mut b, n3, size);
        char_strings.stream(&mut b)?;
        let n5 = b.get_size();
        Key::PRIVATE.fix_offset(&mut b, n4, n5);
        self.private_dict().stream(&mut b, &list, &PRIVATE_DICT_KEYS_FOR_SUBSET)?;
        let mut n6 = 0;
        let has_subrs = bl && lsubrs.is_some() || !bl && self.local_subrs.is_some();
        if has_subrs {
            n6 = Key::SUBRS.stream_dummy_value(&mut b);
        }
        let v = b.get_size() - n5;
        Key::PRIVATE.fix_size(&mut b, n4, v);
        if has_subrs {
            let v = b.get_size() - n5;
            Key::SUBRS.fix_offset(&mut b, n6, v);
            if bl {
                lsubrs.as_ref().unwrap().stream(&mut b)?;
            } else {
                self.local_subrs.as_ref().unwrap().stream(&mut b)?;
            }
        }
        let a = b.to_cff_byte_array();
        out.write_all(a.base.as_slice()).map_err(io_err)
    }
}

impl CFFFontImpl for NameKeyedFont {
    fn base(&self) -> &CFFFont {
        &self.base
    }

    fn get_num_fds(&self) -> i32 {
        1
    }

    fn get_char_strings(&self) -> &CharStrings {
        &self.char_strings
    }

    fn get_local_subrs_for_fd(&self, _n: i32) -> Option<&CharStrings> {
        self.local_subrs.as_ref()
    }

    fn get_default_width_for_fd(&self, _n: i32) -> f64 {
        self.private_dict().get_numbers(&Key::DEFAULT_WIDTH_X, true).unwrap().get_first_value_as_double()
    }

    fn get_nominal_width_for_fd(&self, _n: i32) -> f64 {
        self.private_dict().get_numbers(&Key::NOMINAL_WIDTH_X, true).unwrap().get_first_value_as_double()
    }

    fn get_fd_for_glyph(&self, _n: i32) -> FontResult<i32> {
        Ok(0)
    }

    fn get_font_matrix(&self) -> Matrix {
        Matrix::from_array(&self.base.top_dict.get_numbers(&Key::FONT_MATRIX, true).unwrap().get_values_as_double())
    }

    fn as_name_keyed(&self) -> Option<&NameKeyedFont> {
        Some(self)
    }
}
