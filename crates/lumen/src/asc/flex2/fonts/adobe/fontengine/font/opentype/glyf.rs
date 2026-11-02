//! Port of afe.jar `com.adobe.fontengine.font.opentype.Glyf` (without the `TTOutline` builders of the hinting scaler).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::auto_color::AutoColor;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_subrize::CFFSubrize;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::char_strings::CharStrings;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cid_keyed_font::CIDKeyedFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::dict::Dict;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::non_overlapping_outline_consumer::NonOverlappingOutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_cstring_generator::Type2CStringGenerator;
use crate::asc::flex2::fonts::adobe::fontengine::font::font_data::FontData;
use crate::asc::flex2::fonts::adobe::fontengine::font::hinted_outline_consumer::HintedOutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::glyf_raw::GlyfRaw;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::loca_raw::LocaRaw;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::open_type_font::OpenTypeFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder, OTByteArrayBuilderOutputStreamAdaptor};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::table::data_as_byte_array;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::zone_hint::ZoneHint;
use crate::asc::flex2::fonts::adobe::fontengine::font::orig_font_type::OrigFontType;
use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::pdf_font_description::PDFFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::font::rect::Rect;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset_default_impl::SubsetDefaultImpl;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};
use crate::asc::flex2::fonts::adobe::fontengine::math::f16_dot16::F16Dot16;

const MAX_GLYF_SIZE_FOR_SUBRIZE: i32 = 614400;

#[derive(Clone, Debug)]
pub struct Glyf {
    pub loca: Rc<LocaRaw>,
    pub glyf: Rc<GlyfRaw>,
    pub index_to_loc_format: i32,
}

impl Glyf {
    pub fn new(glyf: Rc<GlyfRaw>, loca: Rc<LocaRaw>, n: i32) -> Glyf {
        Glyf { glyf, loca, index_to_loc_format: n }
    }

    pub fn get_glyph_location(&self, n: i32) -> FontResult<i32> {
        self.loca.get_glyph_location(n, self.index_to_loc_format)
    }

    pub fn get_glyph_bounding_box(&self, n: i32) -> FontResult<Rect> {
        self.glyf.get_glyph_bounding_box(self.get_glyph_location(n)?, self.get_glyph_location(n + 1)?)
    }

    pub fn is_composite(&self, n: i32) -> FontResult<bool> {
        self.glyf.is_composite(self.get_glyph_location(n)?, self.get_glyph_location(n + 1)?)
    }

    /// The TrueType subset (`subsetAndStream(Subset, Map)`): `subset` maps the components through `get_subset_gid`.
    pub fn subset_and_stream(&self, subset: &mut dyn Subset, map: &mut BTreeMap<i32, OTByteArrayBuilder>) -> FontResult<()> {
        let mut b = OTByteArray::get_ot_byte_array_builder_instance_sized((subset.get_num_glyphs() + 1) * 4);
        let mut n2: i32 = 0;
        for i in 0..subset.get_num_glyphs() {
            let n = subset.get_full_gid(i);
            let l2 = self.get_glyph_location(n)? as i64;
            let l = self.get_glyph_location(n + 1)? as i64;
            if l < l2 {
                return Err(FontEngineException::invalid_font(format!("gid {} starts at {} and ends at {}!", n, l2, l)));
            }
            n2 = (n2 as i64 + (l - l2)) as i32;
        }
        let mut b2 = OTByteArray::get_ot_byte_array_builder_instance_sized(n2);
        let mut n: i32 = 0;
        for i in 0..subset.get_num_glyphs() {
            b.setuint32(i * 4, n);
            let n3 = subset.get_full_gid(i);
            let l = self.get_glyph_location(n3)? as i64;
            let l3 = self.get_glyph_location(n3 + 1)? as i64;
            b2.replace_from(n, &self.glyf.data.base, l as i32, (l3 - l) as i32)?;
            if l3 - l > 0 {
                self.fix_component_glyphs(subset, l as i32, &mut b2, n)?;
            }
            n = (n as i64 + (l3 - l)) as i32;
        }
        b.setuint32(subset.get_num_glyphs() * 4, n);
        map.insert(1819239265, b);
        map.insert(1735162214, b2);
        Ok(())
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1819239265, data_as_byte_array(&self.loca.data));
        map.insert(1735162214, data_as_byte_array(&self.glyf.data));
    }

    pub fn pull_component_glyphs(&self, subset: &mut dyn Subset, n: i32) -> FontResult<()> {
        let mut n3 = self.get_glyph_location(n)?;
        let n2 = self.get_glyph_location(n + 1)?;
        if n3 == n2 {
            return Ok(());
        }
        if self.glyf.data.getint16(n3)? != -1 {
            return Ok(());
        }
        n3 += 10;
        let mut bl = true;
        while bl {
            subset.get_subset_gid(self.glyf.data.getuint16(n3 + 2)?)?;
            let n4 = self.glyf.data.getuint16(n3)?;
            let n5 = Glyf::component_size(n4);
            n3 += n5;
            bl = n4 & 0x20 != 0;
        }
        Ok(())
    }

    fn component_size(n4: i32) -> i32 {
        let mut n5 = 4;
        n5 += if n4 & 1 != 0 { 4 } else { 2 };
        if n4 & 8 != 0 {
            n5 += 2;
        } else if n4 & 0x40 != 0 {
            n5 += 4;
        } else if n4 & 0x80 != 0 {
            n5 += 8;
        }
        n5
    }

    pub fn fix_component_glyphs(&self, subset: &mut dyn Subset, mut n: i32, b: &mut OTByteArrayBuilder, mut n2: i32) -> FontResult<()> {
        if self.glyf.data.getint16(n)? != -1 {
            return Ok(());
        }
        n += 10;
        n2 += 10;
        let mut bl = true;
        while bl {
            let g = subset.get_subset_gid(self.glyf.data.getuint16(n + 2)?)?;
            b.setuint16(n2 + 2, g);
            let n3 = self.glyf.data.getuint16(n)?;
            let n4 = Glyf::component_size(n3);
            n += n4;
            n2 += n4;
            bl = n3 & 0x20 != 0;
        }
        Ok(())
    }

    /// `nameGet`: Latin-1 with (C), (R), (TM) and '?' for anything wider.
    fn name_get(s: Option<String>) -> Option<String> {
        let s = s?;
        let mut out = String::new();
        for c in s.encode_utf16() {
            match c {
                0x00A9 => out.push_str("(C)"),
                0x00AE => out.push_str("(R)"),
                0x2122 => out.push_str("(TM)"),
                _ => {
                    if c & 0xFF00 != 0 {
                        out.push('?');
                    } else {
                        out.push(char::from_u32(c as u32).unwrap());
                    }
                }
            }
        }
        Some(out)
    }

    fn create_top_dict(&self, font: &OpenTypeFont, bl: bool) -> FontResult<Dict> {
        let registry = if bl { Some("Adobe".to_string()) } else { None };
        let ordering = if bl { Some("Identity".to_string()) } else { None };
        let matrix = if bl { None } else { Some(Matrix::new(1.0 / font.get_units_per_em_x()?, 0.0, 0.0, 1.0 / font.get_units_per_em_y()?, 0.0, 0.0)) };
        let postscript = format!("/OrigFontType /{} def", OrigFontType::K_TRUETYPE.to_java_string());
        let mut font_name = font.get_xdc_font_description().get_postscript_name()?;
        if font_name.as_deref().is_none_or(|s| s.is_empty() || s.starts_with('\u{0}')) {
            font_name = Some("unknown".to_string());
        }
        let name = font.name.as_ref().ok_or_else(npe)?;
        let version = Glyf::name_get(name.select_name(5)?);
        let notice = Glyf::name_get(name.select_name(7)?);
        let copyright = Glyf::name_get(name.select_name(13)?);
        let full_name = Glyf::name_get(name.select_name(4)?);
        let family_name = Glyf::name_get(name.select_name(1)?);
        let weight = Glyf::name_get(name.select_name(2)?);
        let (fixed, italic, upos, uthick) = match &font.post {
            Some(post) => (
                post.is_fixed_pitch()?,
                F16Dot16::to_double(post.get_italic_angle()?.wrapping_neg()),
                post.get_underline_position()? as f64,
                post.get_underline_thickness()? as f64,
            ),
            None => (false, 0.0, -100.0, 50.0),
        };
        let rect = font.head.as_ref().ok_or_else(npe)?.get_font_bbox()?;
        Dict::new_top(
            registry,
            ordering,
            0,
            version,
            notice,
            copyright,
            full_name,
            font_name,
            family_name,
            weight,
            Some(fixed),
            Some(italic),
            Some(upos),
            Some(uthick),
            None,
            None,
            Some(rect),
            None,
            None,
            Some(postscript),
            None,
            None,
            None,
            matrix,
        )
    }

    fn create_private_dict(d: f64, d2: f64, zone_hint: Option<&ZoneHint>) -> Dict {
        let mut blue_values = None;
        let mut other_blues = None;
        let mut blue_scale = None;
        let mut blue_fuzz = None;
        let mut std_vw = None;
        if let Some(z) = zone_hint {
            blue_values = z.get_blue_values();
            if blue_values.is_some() {
                other_blues = z.get_other_blues();
                blue_scale = Some(z.get_blue_scale());
                blue_fuzz = Some(z.get_blue_fuzz());
            }
            std_vw = Some(z.get_std_v_stem());
        }
        Dict::new_private(blue_values, other_blues, None, None, blue_scale, None, blue_fuzz, None, std_vw, None, None, None, None, None, None, Some(d), Some(d2))
    }

    pub fn stream_for_cff(&self, font: &OpenTypeFont, map: &mut BTreeMap<i32, OTByteArrayBuilder>, bl: bool) -> FontResult<()> {
        self.subset_and_stream_for_cff(None, font, map, bl)
    }

    fn check_subrize_limit(&self, subset: &dyn Subset, font: &OpenTypeFont) -> FontResult<bool> {
        let n = self.glyf.data.get_size();
        if n <= MAX_GLYF_SIZE_FOR_SUBRIZE {
            return Ok(true);
        }
        Ok(n as f64 * subset.get_num_glyphs() as f64 <= 614400.0 * font.get_num_glyphs()? as f64)
    }

    /// `subsetAndStreamForCFF(subset, font, map, hint)`: converts the TrueType outlines to a CID-keyed CFF, hinted
    /// with AutoColor and its zones when `bl`. A `None` subset is the whole font.
    pub fn subset_and_stream_for_cff(&self, subset: Option<&dyn Subset>, font: &OpenTypeFont, map: &mut BTreeMap<i32, OTByteArrayBuilder>, bl: bool) -> FontResult<()> {
        let whole;
        let subset: &dyn Subset = match subset {
            Some(s) => s,
            None => {
                whole = SubsetDefaultImpl::new(font.get_num_glyphs()?, false);
                &whole
            }
        };
        let bl3 = self.check_subrize_limit(subset, font)?;
        let mut zone_hint = None;
        let generator;
        if !bl {
            generator = Rc::new(RefCell::new(Type2CStringGenerator::new(subset.get_num_glyphs(), 1, bl3)));
            for n in 0..subset.get_num_glyphs() {
                let d = font.get_horizontal_advance(subset.get_full_gid(n))?;
                generator.borrow_mut().new_glyph(n, 0, d, 0.0);
                let mut g = generator.borrow_mut();
                font.get_glyph_outline_with(subset.get_full_gid(n), &mut *g, 2)?;
            }
        } else {
            let z = ZoneHint::new(font)?;
            generator = Rc::new(RefCell::new(Type2CStringGenerator::new(subset.get_num_glyphs(), 1, bl3)));
            let gen_dyn: Rc<RefCell<dyn HintedOutlineConsumer>> = generator.clone();
            let auto_color = Rc::new(RefCell::new(AutoColor::new(Some(gen_dyn), font.get_units_per_em_y()?, 6, true, false, &z.get_top_zones(), &z.get_bottom_zones())));
            let ac_dyn: Rc<RefCell<dyn OutlineConsumer>> = auto_color.clone();
            let mut non_overlapping = NonOverlappingOutlineConsumer::new(ac_dyn, font.get_units_per_em_y()?);
            zone_hint = Some(z);
            for n in 0..subset.get_num_glyphs() {
                let d = font.get_horizontal_advance(subset.get_full_gid(n))?;
                auto_color.borrow_mut().new_glyph(n, d, 0);
                generator.borrow_mut().new_glyph(n, 0, d, 0.0);
                font.get_glyph_outline_with(subset.get_full_gid(n), &mut non_overlapping, 2)?;
            }
        }
        let mut s = font.get_xdc_font_description().get_postscript_name()?;
        if s.as_deref().is_none_or(|s| s.is_empty() || s.starts_with('\u{0}')) {
            s = Some("unknown".to_string());
        }
        let (d, d2, mut char_strings) = {
            let mut g = generator.borrow_mut();
            let d = g.calculate_nominal_width(0);
            let d2 = g.calculate_default_width(0);
            (d, d2, g.get_charstring_index()?)
        };
        let mut lsubrs: Option<CharStrings> = None;
        let mut gsubrs: Option<CharStrings> = None;
        if bl3 {
            let mut sub = CFFSubrize::new();
            char_strings = sub.subrize(char_strings);
            lsubrs = sub.get_l_subrs();
            gsubrs = sub.get_g_subrs();
        }
        let top = self.create_top_dict(font, true)?;
        let private = Glyf::create_private_dict(d, d2, zone_hint.as_ref());
        let mut out = OTByteArrayBuilderOutputStreamAdaptor::new();
        let font_dict = Dict::new_font_matrix(Matrix::new(1.0 / font.get_units_per_em_x()?, 0.0, 0.0, 1.0 / font.get_units_per_em_y()?, 0.0, 0.0));
        let cid = CIDKeyedFont::new_synthetic(s.unwrap(), top, char_strings, subset.get_num_glyphs(), font_dict, private, lsubrs, gsubrs)?;
        cid.subset_and_stream(subset, &mut out, false, None, false, false)?;
        map.insert(1128678944, out.builder);
        Ok(())
    }
}

fn npe() -> FontEngineException {
    FontEngineException::new(crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::ExceptionKind::FontEngine, "java.lang.NullPointerException")
}
