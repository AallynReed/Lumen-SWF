//! Port of afe.jar `com.adobe.fontengine.font.cff.CIDKeyedFont` (the parts the font embedding uses).

use std::io::Write;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::{CFFByteArray, CFFByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_font::{create_subset_charstring_index, CFFFont, CFFFontImpl};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_subrize::CFFSubrize;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::char_strings::CharStrings;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::charset::Charset;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cid_component_font::CIDComponentFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::dict::{Dict, IntegerValue, Key, ROSValue};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::fd_select::FdSelect;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::header::Header;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::index::Index;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::name_index::NameIndex;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::string_index::{index_of, StringIndex};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_outline_parser::{OutlineTarget, Type2OutlineParser};
use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;
use crate::asc::flex2::fonts::adobe::fontengine::font::orig_font_type::OrigFontType;
use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::permission::Permission;
use crate::asc::flex2::fonts::adobe::fontengine::font::rect::Rect;
use crate::asc::flex2::fonts::adobe::fontengine::font::ros::ROS;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

const MAX_CFF_SIZE_FOR_SUBRIZE: i32 = 614400;

pub const TOP_DICT_KEYS_FOR_SUBSET: [Key; 17] = [
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
    Key::CID_FONT_TYPE,
];
pub const FONT_DICT_KEYS_FOR_SUBSET: [Key; 1] = [Key::FONT_MATRIX];
pub const PRIVATE_DICT_KEYS_FOR_SUBSET: [Key; 18] = [
    Key::BLUE_VALUES,
    Key::OTHER_BLUES,
    Key::FAMILY_BLUES,
    Key::FAMILY_OTHER_BLUES,
    Key::STD_HW,
    Key::STD_VW,
    Key::DEFAULT_WIDTH_X,
    Key::NOMINAL_WIDTH_X,
    Key::BLUE_SCALE,
    Key::BLUE_SHIFT,
    Key::BLUE_FUZZ,
    Key::STEM_SNAP_H,
    Key::STEM_SNAP_V,
    Key::FORCE_BOLD,
    Key::FORCE_BOLD_THRESHOLD,
    Key::LANGUAGE_GROUP,
    Key::EXPANSION_FACTOR,
    Key::INITIAL_RANDOM_SEED,
];

pub fn io_err(e: std::io::Error) -> FontEngineException {
    FontEngineException::io(e.to_string())
}

#[derive(Clone, Debug)]
pub struct CIDKeyedFont {
    pub base: CFFFont,
    pub char_strings: CharStrings,
    pub charset: Option<Charset>,
    pub fd_select: Option<FdSelect>,
    pub components: Option<Vec<CIDComponentFont>>,
}

impl CIDKeyedFont {
    /// The constructor `FontSet` uses.
    pub fn parse(si: Rc<StringIndex>, gsubrs: CharStrings, name: String, dict: Dict, data: &Rc<CFFByteArray>) -> FontResult<CIDKeyedFont> {
        let ov = match dict.get_offset(&Key::CHAR_STRINGS, false) {
            None => return Err(FontEngineException::invalid_font("missing TopDICT/CharStrings")),
            Some(v) => v,
        };
        let char_strings = CharStrings::new(data.clone(), ov.offset)?;
        let n = char_strings.get_count();
        let charset = match dict.get_offset(&Key::CHARSET, false) {
            Some(o) => Some(Charset::new(data.clone(), o.offset, n)?),
            None => None,
        };
        let components = match dict.get_offset(&Key::FD_ARRAY, false) {
            Some(o) => {
                let index = Index::new(data.clone(), o.offset)?;
                let mut v = Vec::with_capacity(index.get_count() as usize);
                for i in 0..index.get_count() {
                    v.push(CIDComponentFont::parse(data, index.offset_of(i), index.size_of(i), Some(&si))?);
                }
                Some(v)
            }
            None => None,
        };
        let fd_select = match dict.get_offset(&Key::FD_SELECT, false) {
            Some(o) => Some(FdSelect::new(data.clone(), o.offset, n)?),
            None => None,
        };
        Ok(CIDKeyedFont { base: CFFFont::new(Some(si), Some(gsubrs), dict, name), char_strings, charset, fd_select, components })
    }

    /// The package-private 8-argument constructor (`NameKeyedFont.toCID`).
    #[allow(clippy::too_many_arguments)]
    pub fn new_internal(
        si: Option<Rc<StringIndex>>,
        gsubrs: Option<CharStrings>,
        name: String,
        dict: Dict,
        cs: CharStrings,
        charset: Charset,
        components: Vec<CIDComponentFont>,
        fd_select: FdSelect,
    ) -> CIDKeyedFont {
        CIDKeyedFont { base: CFFFont::new(si, gsubrs, dict, name), char_strings: cs, charset: Some(charset), fd_select: Some(fd_select), components: Some(components) }
    }

    /// The public 6-argument constructor.
    pub fn new6(name: String, top: Dict, cs: CharStrings, num_glyphs: i32, font_dict: Dict, private_dict: Dict) -> FontResult<CIDKeyedFont> {
        let comp = CIDComponentFont::new(font_dict, private_dict, Some(CharStrings::create_empty_charstrings()?));
        Ok(CIDKeyedFont {
            base: CFFFont::new(None, Some(CharStrings::create_empty_charstrings()?), top, name),
            char_strings: cs,
            charset: Some(Charset::identity_charset(num_glyphs)),
            fd_select: Some(FdSelect::single_font(num_glyphs)?),
            components: Some(vec![comp]),
        })
    }

    /// The public 8-argument constructor (`Glyf.subsetAndStreamForCFF`).
    #[allow(clippy::too_many_arguments)]
    pub fn new_synthetic(
        name: String,
        top: Dict,
        cs: CharStrings,
        num_glyphs: i32,
        font_dict: Dict,
        private_dict: Dict,
        lsubrs: Option<CharStrings>,
        gsubrs: Option<CharStrings>,
    ) -> FontResult<CIDKeyedFont> {
        let comp = CIDComponentFont::new(font_dict, private_dict, lsubrs);
        Ok(CIDKeyedFont {
            base: CFFFont::new(None, gsubrs, top, name),
            char_strings: cs,
            charset: Some(Charset::identity_charset(num_glyphs)),
            fd_select: Some(FdSelect::single_font(num_glyphs)?),
            components: Some(vec![comp]),
        })
    }

    fn components(&self) -> &[CIDComponentFont] {
        self.components.as_deref().expect("java.lang.NullPointerException: no FDArray")
    }

    fn fd_select(&self) -> &FdSelect {
        self.fd_select.as_ref().expect("java.lang.NullPointerException: no FDSelect")
    }

    fn charset(&self) -> &Charset {
        self.charset.as_ref().expect("java.lang.NullPointerException: no charset")
    }

    pub fn get_num_glyphs(&self) -> i32 {
        self.char_strings.get_count()
    }

    pub fn get_glyph_name(&self, _n: i32) -> Option<String> {
        None
    }

    pub fn get_ros(&self) -> Option<ROS> {
        self.base.top_dict.get_ros()
    }

    pub fn get_glyph_cid(&self, n: i32) -> FontResult<i32> {
        self.charset().gid2sid(n)
    }

    pub fn cid2gid(&self, n: i32) -> FontResult<i32> {
        self.charset().sid2gid(n)
    }

    pub fn get_glyph_outline(&self, n: i32, oc: &mut dyn OutlineConsumer) -> FontResult<()> {
        if self.base.top_dict.get_numbers(&Key::CHARSTRING_TYPE, true).unwrap().get_first_value_as_double() != 2.0 {
            return Err(FontEngineException::invalid_font("Unsupported Charstring type"));
        }
        self.get_outline(n, &mut Type2OutlineParser::new(false), OutlineTarget::Plain(oc))
    }

    pub fn get_outline(&self, n: i32, p: &mut Type2OutlineParser, oc: OutlineTarget<'_>) -> FontResult<()> {
        let fd = self.fd_select().component_of(n)?;
        let fm = self.base.top_dict.get_numbers(&Key::FONT_MATRIX, false);
        self.components()[fd as usize].get_outline(&self.char_strings, n, self.base.global_subrs.as_ref(), p, oc, fm.as_ref())
    }

    pub fn get_stem_v_for_glyph(&self, n: i32) -> FontResult<f64> {
        Ok(self.components()[self.fd_select().component_of(n)? as usize].get_stem_v())
    }

    pub fn get_font_matrix_fd(&self, n: i32) -> Matrix {
        let nv = self.base.top_dict.get_numbers(&Key::FONT_MATRIX, false);
        let comps = self.components();
        if n >= 0 && (n as usize) < comps.len() {
            comps[n as usize].get_outline_matrix(nv.as_ref())
        } else {
            Matrix::from_array(&nv.expect("java.lang.NullPointerException: FontMatrix").get_values_as_double())
        }
    }

    fn get_font_to_metrics_matrix(&self) -> Matrix {
        let m = self.get_font_matrix();
        let d = self.get_units_per_em_x();
        let d2 = self.get_units_per_em_y();
        Matrix::new(d * m.a, d2 * m.b, d * m.c, d2 * m.d, d * m.tx, d2 * m.ty)
    }

    pub fn get_horizontal_advance(&self, n: i32) -> FontResult<f64> {
        let fd = self.fd_select().component_of(n)?;
        self.components()[fd as usize].get_horizontal_advance(&self.char_strings, n, self.base.global_subrs.as_ref())
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
        self.base.get_embedding_permission_given_ft(bl, OrigFontType::K_CID)
    }

    fn set_cid_count(&self, bl: bool, b: &mut CFFByteArrayBuilder, subset: &dyn Subset, bl2: bool, list: &[String]) -> FontResult<()> {
        if !bl {
            Dict::stream_key_val_int(b, &Key::CID_COUNT, subset.get_num_glyphs());
        } else if bl2 {
            let mut n = -1;
            for i in 0..subset.get_num_glyphs() {
                let n3 = self.charset().gid2sid(subset.get_full_gid(i))?;
                if n3 <= n {
                    continue;
                }
                n = n3;
            }
            Dict::stream_key_val_int(b, &Key::CID_COUNT, n + 1);
        } else {
            self.base.top_dict.stream_value(b, list, &Key::CID_COUNT)?;
        }
        Ok(())
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

    fn check_subrize_limit(&self, subset: &dyn Subset) -> bool {
        let n = self.char_strings.data.get_size();
        if n <= MAX_CFF_SIZE_FOR_SUBRIZE {
            return true;
        }
        n as f64 * subset.get_num_glyphs() as f64 <= 614400.0 * self.get_num_glyphs() as f64
    }

    /// `subsetAndStream(Subset, OutputStream, boolean, Integer)`.
    pub fn subset_and_stream4(&self, subset: &dyn Subset, out: &mut dyn Write, bl: bool, n: Option<i32>) -> FontResult<()> {
        self.subset_and_stream(subset, out, bl, n, false, true)
    }

    /// `subsetAndStream(Subset, OutputStream, boolean, Integer, boolean)`.
    pub fn subset_and_stream5(&self, subset: &dyn Subset, out: &mut dyn Write, bl: bool, n: Option<i32>, bl2: bool) -> FontResult<()> {
        self.subset_and_stream(subset, out, bl, n, bl2, true)
    }

    /// The 6-argument `subsetAndStream`, for a subset that is not a `SubsetDefaultImpl` (an `OTSubset` counts as one,
    /// which only matters when `b2` is false and `b3` true).
    pub fn subset_and_stream(&self, subset: &dyn Subset, out: &mut dyn Write, b1: bool, n: Option<i32>, b2: bool, b3: bool) -> FontResult<()> {
        self.subset_and_stream_sdi(subset, None, out, b1, n, b2, b3)
    }

    /// The 6-argument `subsetAndStream`; `sdi_do_subset` is `doSubset()` when the subset is a `SubsetDefaultImpl`.
    #[allow(clippy::too_many_arguments)]
    pub fn subset_and_stream_sdi(&self, subset: &dyn Subset, sdi_do_subset: Option<bool>, out: &mut dyn Write, bl: bool, n: Option<i32>, mut bl2: bool, bl3: bool) -> FontResult<()> {
        let bl4 = sdi_do_subset.is_none_or(|d| d) || bl2;
        let comps = self.components();
        let mut list = StringIndex::collect_predefined_strings();
        self.base.top_dict.collect_strings(&mut list);
        if let Some(sv) = self.base.top_dict.get_string(&Key::POST_SCRIPT, false) {
            let i = index_of(&list, &sv.value);
            list.remove(i as usize);
        }
        for c in comps {
            c.collect_strings(&mut list);
        }
        let mut b = CFFByteArray::get_cff_byte_array_builder_instance();
        Header::to_binary(&mut b);
        NameIndex::to_binary(&mut b, &self.base.name);
        let mut cursor = Index::start_index(&mut b, 1);
        if bl || !bl3 {
            self.base.top_dict.stream_value(&mut b, &list, &Key::ROS)?;
        } else {
            let rv = ROSValue::new("Adobe".into(), "Identity".into(), 1);
            rv.collect_strings(&mut list);
            rv.stream(&mut b, &list);
            Key::ROS.stream(&mut b);
        }
        self.base.top_dict.stream(&mut b, &list, &TOP_DICT_KEYS_FOR_SUBSET)?;
        self.set_cid_count(bl, &mut b, subset, bl4, &list)?;
        self.set_ps_string(&mut b, n, &mut list);
        let n4 = Key::FD_ARRAY.stream_dummy_value(&mut b);
        let n5 = Key::FD_SELECT.stream_dummy_value(&mut b);
        let n6 = self.charset().predefined_offset();
        let n3 = if bl4 || !bl {
            Key::CHARSET.stream_dummy_value(&mut b)
        } else if n6 != -1 {
            IntegerValue::new(n6).stream(&mut b);
            -1
        } else {
            Key::CHARSET.stream_dummy_value(&mut b)
        };
        let n7 = Key::CHAR_STRINGS.stream_dummy_value(&mut b);
        cursor = Index::element_entered(&mut b, cursor);
        let _ = cursor;
        StringIndex::to_binary(&mut b, &list);
        let mut subrize: Option<CFFSubrize> = None;
        let mut char_strings: Option<CharStrings> = None;
        if !bl4 || !bl3 {
            match &self.base.global_subrs {
                Some(g) => g.stream(&mut b)?,
                None => b.add_card16(0),
            }
        } else {
            if bl2 {
                bl2 = self.check_subrize_limit(subset);
            }
            let mut cs = create_subset_charstring_index(self, subset, bl2)?;
            if bl2 {
                let mut fds = Vec::with_capacity(subset.get_num_glyphs() as usize);
                for i in 0..subset.get_num_glyphs() {
                    fds.push(self.get_fd_for_glyph(subset.get_full_gid(i))? as u8);
                }
                let mut s = CFFSubrize::new();
                cs = s.subrize_fd(cs, Some(fds));
                let g = match s.get_g_subrs() {
                    Some(g) => g,
                    None => CharStrings::create_empty_charstrings()?,
                };
                g.stream(&mut b)?;
                subrize = Some(s);
            } else {
                b.add_card16(0);
            }
            char_strings = Some(cs);
        }
        let size = b.get_size();
        Key::FD_ARRAY.fix_offset(&mut b, n4, size);
        let mut offs = vec![0i32; comps.len()];
        let mut cursor = Index::start_index(&mut b, comps.len() as i32);
        for (i, c) in comps.iter().enumerate() {
            c.font_dict.stream(&mut b, &list, &FONT_DICT_KEYS_FOR_SUBSET)?;
            offs[i] = Key::PRIVATE.stream_dummy_value(&mut b);
            cursor = Index::element_entered(&mut b, cursor);
        }
        for (i, c) in comps.iter().enumerate() {
            let mut cs3: Option<CharStrings> = None;
            if let Some(s) = &subrize {
                cs3 = Some(match s.get_l_subrs_fd(i as i32) {
                    Some(l) => l,
                    None => CharStrings::create_empty_charstrings()?,
                });
            }
            let size = b.get_size();
            Key::PRIVATE.fix_offset(&mut b, offs[i], size);
            let n8 = b.get_size();
            c.private_dict.stream(&mut b, &list, &PRIVATE_DICT_KEYS_FOR_SUBSET)?;
            let mut n9 = 0;
            if cs3.is_some() || (!bl4 || !bl3) && c.local_subrs.is_some() {
                n9 = Key::SUBRS.stream_dummy_value(&mut b);
            }
            let n10 = b.get_size() - n8;
            Key::PRIVATE.fix_size(&mut b, offs[i], n10);
            if cs3.is_none() && (bl4 && bl3 || c.local_subrs.is_none()) {
                continue;
            }
            let v = b.get_size() - n8;
            Key::SUBRS.fix_offset(&mut b, n9, v);
            match &cs3 {
                Some(l) => l.stream(&mut b)?,
                None => c.local_subrs.as_ref().unwrap().stream(&mut b)?,
            }
        }
        let size = b.get_size();
        Key::FD_SELECT.fix_offset(&mut b, n5, size);
        if !bl4 || !bl3 {
            self.fd_select().stream(&mut b)?;
        } else if comps.len() > 1 {
            FdSelect::fd_select_from_subset(self.fd_select(), subset)?.stream(&mut b)?;
        } else {
            FdSelect::single_font(subset.get_num_glyphs())?.stream(&mut b)?;
        }
        if n3 != -1 {
            let size = b.get_size();
            Key::CHARSET.fix_offset(&mut b, n3, size);
            if !bl4 && bl || !bl3 {
                self.charset().stream(&mut b)?;
            } else if !bl {
                Charset::identity_charset(subset.get_num_glyphs()).stream(&mut b)?;
            } else {
                Charset::char_set_from_subset(self.charset(), subset)?.stream(&mut b)?;
            }
        }
        let size = b.get_size();
        Key::CHAR_STRINGS.fix_offset(&mut b, n7, size);
        if !bl4 || !bl3 {
            self.char_strings.stream(&mut b)?;
        } else {
            char_strings.unwrap().stream(&mut b)?;
        }
        let a = b.to_cff_byte_array();
        out.write_all(a.base.as_slice()).map_err(io_err)
    }

    pub fn stream(&self, out: &mut dyn Write, n: Option<i32>) -> FontResult<()> {
        let sdi = crate::asc::flex2::fonts::adobe::fontengine::font::subset_default_impl::SubsetDefaultImpl::new(self.get_num_glyphs(), false);
        self.subset_and_stream_sdi(&sdi, Some(false), out, true, n, false, true)
    }
}

impl CFFFontImpl for CIDKeyedFont {
    fn base(&self) -> &CFFFont {
        &self.base
    }

    fn get_num_fds(&self) -> i32 {
        self.components().len() as i32
    }

    fn get_char_strings(&self) -> &CharStrings {
        &self.char_strings
    }

    fn get_local_subrs_for_fd(&self, n: i32) -> Option<&CharStrings> {
        self.components()[n as usize].local_subrs.as_ref()
    }

    fn get_default_width_for_fd(&self, n: i32) -> f64 {
        self.components()[n as usize].private_dict.get_numbers(&Key::DEFAULT_WIDTH_X, true).unwrap().get_first_value_as_double()
    }

    fn get_nominal_width_for_fd(&self, n: i32) -> f64 {
        self.components()[n as usize].private_dict.get_numbers(&Key::NOMINAL_WIDTH_X, true).unwrap().get_first_value_as_double()
    }

    fn get_fd_for_glyph(&self, n: i32) -> FontResult<i32> {
        self.fd_select().component_of(n)
    }

    fn get_font_matrix(&self) -> Matrix {
        Matrix::from_array(&self.base.top_dict.get_numbers(&Key::FONT_MATRIX, true).unwrap().get_values_as_double())
    }
}
