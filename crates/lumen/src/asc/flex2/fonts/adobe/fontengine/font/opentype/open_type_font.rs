//! Port of afe.jar `com.adobe.fontengine.font.opentype.OpenTypeFont` (the parts SWF embedding and the font
//! resolvers reach: metrics, outlines, the SWF streaming, names and descriptions; not the CoolType/PDF/CSS logic).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_font::AnyCFFFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::embedding_permission::EmbeddingPermission;
use crate::asc::flex2::fonts::adobe::fontengine::font::font_data::FontData;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::base::Base;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::cff::Cff;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::cmap::Cmap;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::cvt::Cvt;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::fond::Fond;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::fpgm::Fpgm;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gasp::Gasp;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gdef::Gdef;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::glyf::Glyf;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gpos::Gpos;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gpos_harvester::GposHarvester;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gsub::Gsub;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::head::Head;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::hhea::Hhea;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::hmtx::Hmtx;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::kern::Kern;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ltsh::Ltsh;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::maxp::Maxp;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::name::{Name, NameSelector, SingleNameSelector};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::no_vorg_vert_orig_fetcher::NoVORGVertOrigFetcher;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::optical_size_data::OpticalSizeData;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::os2::Os2;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_subset::OTSubset;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::otf_swf_font3_description::OTFSWFFont3Description;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::otf_swf_font4_description::OTFSWFFont4Description;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::post::Post;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::prep::Prep;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::table::AnyTable;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::tt_parser::TTParser;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::vertical_origin_fetcher::VerticalOriginFetcher;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::vhea::Vhea;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::vmtx::Vmtx;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::vorg::Vorg;
use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::pdf_font_description::PDFFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::font::permission::Permission;
use crate::asc::flex2::fonts::adobe::fontengine::font::rect::Rect;
use crate::asc::flex2::fonts::adobe::fontengine::font::ros::ROS;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset_default_impl::SubsetDefaultImpl;
use crate::asc::flex2::fonts::adobe::fontengine::font::swf_font4_description::SWFFont4Description;
use crate::asc::flex2::fonts::adobe::fontengine::font::swf_font_description::SWFFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::font::xdc_font_description::XDCFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{ExceptionKind, FontEngineException, FontResult};
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::fxg::fxg_font_description::FXGFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform::platform_font_description::PlatformFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform_type::Platform;

fn npe() -> FontEngineException {
    FontEngineException::new(ExceptionKind::FontEngine, "java.lang.NullPointerException")
}

fn io(e: std::io::Error) -> FontEngineException {
    FontEngineException::from_cause(ExceptionKind::Io, e.to_string())
}

pub struct OpenTypeFont {
    container_fingerprint: Vec<u8>,
    pub head: Option<Rc<Head>>,
    pub hhea: Option<Rc<Hhea>>,
    pub hmtx: Option<Hmtx>,
    pub maxp: Option<Rc<Maxp>>,
    pub name: Option<Rc<Name>>,
    pub os2: Option<Rc<Os2>>,
    pub cmap: Option<Rc<Cmap>>,
    pub post: Option<Rc<Post>>,
    pub gpos: Option<Rc<Gpos>>,
    pub gsub: Option<Rc<Gsub>>,
    pub gdef: Option<Rc<Gdef>>,
    pub base: Option<Rc<Base>>,
    pub glyf: Option<Glyf>,
    pub cff: Option<Rc<Cff>>,
    pub vhea: Option<Rc<Vhea>>,
    pub vmtx: Option<Vmtx>,
    pub cvt: Option<Rc<Cvt>>,
    pub fpgm: Option<Rc<Fpgm>>,
    pub prep: Option<Rc<Prep>>,
    pub kern: Option<Rc<Kern>>,
    pub gasp: Option<Rc<Gasp>>,
    pub ltsh: Option<Rc<Ltsh>>,
    pub vorg: Option<Rc<Vorg>>,
    pub fond: Option<Rc<Fond>>,
    /// `invertedcmap` once computed (`None` inside when there is no (3,1) cmap).
    invertedcmap: RefCell<Option<Option<Vec<i32>>>>,
    base14_css_name: Option<String>,
    base14_ps_name: Option<String>,
}

macro_rules! take {
    ($map:expr, $tag:expr, $variant:ident) => {
        match $map.get(&$tag) {
            Some(AnyTable::$variant(t)) => Some(t.clone()),
            _ => None,
        }
    };
}

impl OpenTypeFont {
    /// `new OpenTypeFont(map, fingerprint, base14CSSName, base14PSName)`; a tag whose table failed to load is absent.
    pub fn new(map: &HashMap<i32, AnyTable>, fingerprint: Vec<u8>, base14_css_name: Option<String>, base14_ps_name: Option<String>) -> FontResult<OpenTypeFont> {
        let head = take!(map, 1751474532, Head);
        let hhea = take!(map, 1751672161, Hhea);
        let vhea = take!(map, 1986553185, Vhea);
        let hmtx = match (take!(map, 1752003704, HmtxRaw), &hhea) {
            (Some(t), Some(h)) => Some(Hmtx::new(t, h.get_number_of_h_metrics()?)),
            _ => None,
        };
        let vmtx = match (take!(map, 1986884728, VmtxRaw), &vhea) {
            (Some(t), Some(v)) => Some(Vmtx::new(t, v.get_number_of_v_metrics()?)),
            _ => None,
        };
        let glyf = match (take!(map, 1735162214, GlyfRaw), take!(map, 1819239265, LocaRaw), &head) {
            (Some(g), Some(l), Some(h)) => Some(Glyf::new(g, l, h.get_index_to_loc_format()?)),
            _ => None,
        };
        Ok(OpenTypeFont {
            container_fingerprint: fingerprint,
            maxp: take!(map, 1835104368, Maxp),
            name: take!(map, 1851878757, Name),
            os2: take!(map, 1330851634, Os2),
            cmap: take!(map, 1668112752, Cmap),
            post: take!(map, 1886352244, Post),
            cff: take!(map, 1128678944, Cff),
            gpos: take!(map, 1196445523, Gpos),
            gsub: take!(map, 1196643650, Gsub),
            gdef: take!(map, 1195656518, Gdef),
            base: take!(map, 1111577413, Base),
            cvt: take!(map, 1668707360, Cvt),
            fpgm: take!(map, 1718642541, Fpgm),
            prep: take!(map, 1886545264, Prep),
            kern: take!(map, 1801810542, Kern),
            gasp: take!(map, 1734439792, Gasp),
            ltsh: take!(map, 1280594760, Ltsh),
            vorg: take!(map, 1448038983, Vorg),
            fond: take!(map, 1718578788, Fond),
            head,
            hhea,
            hmtx,
            vhea,
            vmtx,
            glyf,
            invertedcmap: RefCell::new(None),
            base14_css_name,
            base14_ps_name,
        })
    }

    pub fn get_cool_type_units_per_em(&self) -> FontResult<f64> {
        if let Some(h) = &self.head {
            return Ok(h.get_units_per_em()? as f64);
        }
        if self.glyf.is_some() {
            return Ok(2048.0);
        }
        Ok(1000.0)
    }

    pub fn get_cool_type_glyph_for_char(&self, n: i32) -> FontResult<i32> {
        match &self.cmap {
            None => Ok(0),
            Some(c) => c.cool_type_unicode_char2glyph(n),
        }
    }

    pub fn get_char_for_glyph(&self, n: i32) -> FontResult<i32> {
        if self.invertedcmap.borrow().is_none() {
            let cmap = self.cmap.as_ref().ok_or_else(npe)?;
            let n2 = cmap.get_cmap_subtable_index(3, 1)?;
            let v = if n2 != -1 { Some(cmap.glyph2char(self.get_num_glyphs()?, n2)?) } else { None };
            *self.invertedcmap.borrow_mut() = Some(v);
        }
        match &*self.invertedcmap.borrow() {
            Some(Some(a)) => a.get(n as usize).cloned().ok_or_else(|| FontEngineException::new(ExceptionKind::FontEngine, format!("java.lang.ArrayIndexOutOfBoundsException: Index {} out of bounds for length {}", n, a.len()))),
            _ => Ok(-1),
        }
    }

    /// `getGlyphOutline(gid, consumer, pathType)`.
    pub fn get_glyph_outline_with(&self, n: i32, oc: &mut dyn OutlineConsumer, n2: i32) -> FontResult<()> {
        if let Some(cff) = &self.cff {
            return cff.get_outline(n, oc);
        }
        TTParser::new().parse_with(self, n, oc, n2)
    }

    pub fn get_glyph_name(&self, n: i32) -> FontResult<String> {
        if n == 0 {
            return Ok(".notdef".to_string());
        }
        let mut s = None;
        if let Some(p) = &self.post {
            s = p.get_glyph_name(n);
        }
        if s.is_none() {
            if let Some(c) = &self.cff {
                s = c.get_glyph_name(n)?;
            }
        }
        Ok(s.unwrap_or_else(|| format!("g{}", n)))
    }

    pub fn get_ros(&self) -> Option<ROS> {
        self.cff.as_ref().and_then(|c| c.get_ros())
    }

    pub fn get_glyph_cid(&self, n: i32) -> FontResult<i32> {
        match &self.cff {
            Some(c) => c.get_glyph_cid(n),
            None => Ok(-1),
        }
    }

    pub fn get_xdc_font_description(&self) -> OTXDCFontDescription<'_> {
        OTXDCFontDescription { font: self }
    }

    pub fn create_subset(&self) -> FontResult<OTSubset<'_>> {
        OTSubset::new(self, self.cff.is_some())
    }

    fn stream_tables_for_editting(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) -> FontResult<()> {
        if let Some(t) = &self.hmtx {
            t.stream(map);
        }
        if let Some(t) = &self.hhea {
            t.stream(map);
        }
        if let Some(t) = &self.os2 {
            t.stream(map);
        }
        if let Some(t) = &self.gsub {
            t.stream(map);
        }
        if let Some(t) = &self.gpos {
            t.stream(map);
        }
        if let Some(t) = &self.gdef {
            t.stream(map);
        }
        if let Some(t) = &self.base {
            t.stream(map);
        }
        if let Some(t) = &self.cff {
            t.stream(map, None)?;
        }
        Ok(())
    }

    /// `streamSFNTForSWFEditting`: the whole font for DefineFont4, TrueType outlines converted to CFF.
    pub fn stream_sfnt_for_swf_editting(&self, out: &mut dyn Write) -> FontResult<()> {
        let mut map = BTreeMap::new();
        self.stream_tables_for_editting(&mut map)?;
        if let Some(name) = &self.name {
            let osd = self.get_optical_size_data()?;
            let ids = osd.map(|o| vec![o.name_id]);
            name.stream(&mut map, false, ids.as_deref())?;
        }
        if let Some(cmap) = &self.cmap {
            cmap.stream_for_swf(&mut map, self.get_num_glyphs()?)?;
        }
        if let Some(maxp) = &self.maxp {
            if self.glyf.is_some() {
                maxp.subset_and_stream_for_cff(self.get_num_glyphs()?, &mut map);
            } else {
                maxp.stream(&mut map);
            }
        }
        if let Some(glyf) = &self.glyf {
            glyf.stream_for_cff(self, &mut map, true)?;
        }
        if let Some(post) = &self.post {
            if self.glyf.is_some() {
                post.subset_and_stream_for_cff(&mut map)?;
            } else {
                post.stream(&mut map);
            }
        }
        if let Some(vorg) = &self.vorg {
            vorg.stream(&mut map);
        } else if self.vmtx.is_some() {
            let s = SubsetDefaultImpl::new(self.get_num_glyphs()?, false);
            Vorg::subset_and_stream(&s, &mut map, &NoVORGVertOrigFetcher::new(self))?;
        }
        if let Some(kern) = &self.kern {
            kern.stream(&mut map);
        }
        if let Some(vhea) = &self.vhea {
            vhea.stream(&mut map);
        }
        if let Some(vmtx) = &self.vmtx {
            vmtx.stream(&mut map);
        }
        let head = if let Some(h) = &self.head { Some(h.stream(&mut map)) } else { None };
        self.finalize_stream_data(head, &mut map, out, false)
    }

    /// `finalizeStreamData`: the table directory, head's checksum adjustment, then the padded tables.
    pub fn finalize_stream_data(&self, head: Option<i32>, map: &mut BTreeMap<i32, OTByteArrayBuilder>, out: &mut dyn Write, bl: bool) -> FontResult<()> {
        let size = map.len() as i32;
        let n = 12 + 16 * size;
        let mut n2 = 1;
        let mut n3 = 0;
        while n2 <= size {
            n2 *= 2;
            n3 += 1;
        }
        n3 -= 1;
        n2 *= 8;
        let n4 = size * 16 - n2;
        let mut dir = OTByteArray::get_ot_byte_array_builder_instance_sized(n);
        if bl {
            dir.set_fixed(0, 1, 0);
        } else {
            dir.set_fixed(0, 20308, 21583);
        }
        dir.setuint16(4, size);
        dir.setuint16(6, n2);
        dir.setuint16(8, n3);
        dir.setuint16(10, n4);
        let mut l: i64 = n as i64;
        let mut n5 = 12;
        let mut l2: i64 = 0;
        if self.head.is_some() {
            Head::clear_checksum_adjust(map.get_mut(&head.ok_or_else(npe)?).ok_or_else(npe)?);
        }
        for (&tag, b) in map.iter() {
            let l3 = b.checksum(0, b.get_size(), 0)?;
            l2 = (l2 + l3) & 0xFFFF_FFFF;
            dir.setuint32(n5, tag);
            dir.setuint32(n5 + 4, l3 as i32);
            dir.setuint32(n5 + 8, l as i32);
            dir.setuint32(n5 + 12, b.get_size());
            n5 += 16;
            l += b.get_size() as i64;
            if b.get_size() % 4 != 0 {
                l += (4 - b.get_size() % 4) as i64;
            }
        }
        l2 = (l2 + dir.checksum(0, dir.get_size(), 0)?) & 0xFFFF_FFFF;
        let l4 = (2981146554i64 - l2) & 0xFFFF_FFFF;
        Head::set_checksum_adjust(map.get_mut(&head.ok_or_else(npe)?).ok_or_else(npe)?, l4);
        let dir = dir.to_ot_byte_array();
        out.write_all(dir.base.as_slice()).map_err(io)?;
        for b in map.values() {
            let a = &b.base.byte_array;
            out.write_all(a.as_slice()).map_err(io)?;
            if a.get_size() % 4 != 0 {
                let n8 = (a.get_size() / 4 + 1) * 4 - a.get_size();
                out.write_all(&vec![0u8; n8 as usize]).map_err(io)?;
            }
        }
        Ok(())
    }

    /// `subsetAndStreamForSWFEmbedding(subset, out, gsubLookups, codePoints, variationSequences)`.
    pub fn subset_and_stream_for_swf_embedding(&self, subset: &mut dyn Subset, out: &mut dyn Write, tree_map: Option<&BTreeMap<i32, Vec<i32>>>, tree_set: &BTreeSet<i32>, bl: bool) -> FontResult<()> {
        let mut map = BTreeMap::new();
        if let Some(t) = &self.hmtx {
            t.subset_and_stream(subset, &mut map)?;
        }
        if let Some(t) = &self.hhea {
            t.subset_and_stream(subset, &mut map);
        }
        if let Some(t) = &self.os2 {
            t.subset_and_stream(&mut map);
        }
        if let Some(t) = &self.gsub {
            let empty = BTreeMap::new();
            t.subset_and_stream(subset, tree_map.unwrap_or(&empty), &mut map, self.get_num_glyphs()?)?;
        }
        if let Some(t) = &self.kern {
            t.subset_and_stream_for_swf(subset, &mut map)?;
        }
        if let Some(t) = &self.gpos {
            let mut h = GposHarvester::new(t, self.get_num_glyphs()?);
            let lookups = h.gather_possible_lookups(subset)?;
            let has_kern = map.contains_key(&1801810542);
            t.subset_and_stream(subset, &lookups, &mut map, self.get_num_glyphs()?, has_kern)?;
        }
        if let Some(t) = &self.gdef {
            t.subset_and_stream(subset, self.get_num_glyphs()?, &mut map)?;
        }
        if let Some(t) = &self.base {
            t.subset_and_stream(subset, &mut map)?;
        }
        if let Some(cff) = &self.cff {
            cff.subset_and_stream(subset, &mut map, false, None, true)?;
        } else if let Some(glyf) = &self.glyf {
            glyf.subset_and_stream_for_cff(Some(&*subset), self, &mut map, true)?;
        } else {
            return Err(FontEngineException::invalid_font("Either cff or glyf must be present"));
        }
        if self.vmtx.is_some() {
            match &self.vorg {
                Some(v) => Vorg::subset_and_stream(subset, &mut map, &**v)?,
                None => Vorg::subset_and_stream(subset, &mut map, &NoVORGVertOrigFetcher::new(self))?,
            }
        }
        if let Some(t) = &self.cmap {
            t.subset_and_stream_for_swf(tree_set, subset, &mut map, bl)?;
        }
        if let Some(t) = &self.name {
            t.subset_and_stream(false, None, &mut map)?;
        }
        if let Some(t) = &self.maxp {
            if self.cff.is_some() {
                t.subset_and_stream(subset, &mut map);
            } else {
                t.subset_and_stream_for_cff(subset.get_num_glyphs(), &mut map);
            }
        }
        if let Some(t) = &self.post {
            if self.cff.is_some() {
                t.stream(&mut map);
            } else {
                t.subset_and_stream_for_cff(&mut map)?;
            }
        }
        if let Some(t) = &self.vhea {
            t.subset_and_stream(subset, &mut map);
        }
        if let Some(t) = &self.vmtx {
            t.subset_and_stream(subset, &mut map)?;
        }
        let head = if let Some(h) = &self.head { Some(h.subset_and_stream(&mut map)) } else { None };
        self.finalize_stream_data(head, &mut map, out, false)
    }

    /// `getOpticalSizeData()`.
    pub fn get_optical_size_data(&self) -> FontResult<Option<OpticalSizeData>> {
        let Some(gpos) = &self.gpos else { return Ok(None) };
        let Some(o) = gpos.get_optical_size_data(false)? else { return Ok(None) };
        let ok = if o.design_size == 0 {
            false
        } else if o.subfamily_id == 0 && o.name_id == 0 && o.min_size == 0 && o.max_size == 0 {
            true
        } else {
            o.design_size >= o.min_size
                && o.max_size >= o.design_size
                && o.name_id >= 256
                && o.name_id <= i16::MAX as i32
                && match &self.name {
                    None => true,
                    Some(n) => n.get_name_ms(-1, o.name_id)?.is_some(),
                }
        };
        if !ok {
            return gpos.get_optical_size_data(true);
        }
        Ok(Some(o))
    }

    pub fn get_cff_font(&self) -> Option<&AnyCFFFont> {
        self.cff.as_ref().map(|c| c.get_cff_font())
    }
}

impl FontData for OpenTypeFont {
    fn get_container_fingerprint(&self) -> Vec<u8> {
        self.container_fingerprint.clone()
    }

    fn get_num_glyphs(&self) -> FontResult<i32> {
        if let Some(m) = &self.maxp {
            return m.get_num_glyphs();
        }
        match self.get_cff_font() {
            Some(AnyCFFFont::NameKeyed(f)) => Ok(f.get_num_glyphs()),
            Some(AnyCFFFont::CIDKeyed(f)) => Ok(f.get_num_glyphs()),
            None => Err(FontEngineException::invalid_font("required table (maxp) is missing")),
        }
    }

    fn is_symbolic(&self) -> FontResult<bool> {
        Ok(match &self.cmap {
            Some(c) => c.is_symbolic(),
            None => true,
        })
    }

    fn get_units_per_em_x(&self) -> FontResult<f64> {
        if let Some(h) = &self.head {
            return Ok(h.get_units_per_em()? as f64);
        }
        match &self.cff {
            Some(c) => Ok(c.get_units_per_em_x()),
            None => Err(FontEngineException::invalid_font("No way to know units per em")),
        }
    }

    fn get_units_per_em_y(&self) -> FontResult<f64> {
        if let Some(h) = &self.head {
            return Ok(h.get_units_per_em()? as f64);
        }
        match &self.cff {
            Some(c) => Ok(c.get_units_per_em_y()),
            None => Err(FontEngineException::invalid_font("No way to know units per em")),
        }
    }

    fn get_font_bbox(&self) -> FontResult<Rect> {
        if let Some(h) = &self.head {
            return h.get_font_bbox();
        }
        match self.cff.as_ref().and_then(|c| c.get_font_bbox()) {
            Some(r) => Ok(r),
            None if self.cff.is_some() => Err(npe()),
            None => Err(FontEngineException::invalid_font("No bbox in font")),
        }
    }

    fn get_glyph_for_char(&self, n: i32) -> FontResult<i32> {
        match &self.cmap {
            None => Ok(0),
            Some(c) => c.unicode_char2glyph(n),
        }
    }

    fn get_horizontal_advance(&self, n: i32) -> FontResult<f64> {
        if let Some(h) = &self.hmtx {
            return Ok(h.get_horizontal_advance(n)? as f64);
        }
        match &self.cff {
            Some(c) => c.get_horizontal_advance(n),
            None => Err(FontEngineException::invalid_font("No way to know horizontal advance")),
        }
    }

    fn get_glyph_outline(&self, n: i32, oc: &mut dyn OutlineConsumer) -> FontResult<()> {
        if let Some(cff) = &self.cff {
            return cff.get_outline(n, oc);
        }
        TTParser::new().parse(self, n, oc)
    }

    /// The CFF branch needs `Type2GlyphBBoxCalculator`, which is not ported.
    fn get_glyph_bbox(&self, n: i32) -> FontResult<Rect> {
        if self.cff.is_some() {
            return Err(FontEngineException::unsupported_font("CFF glyph bounding boxes are not ported"));
        }
        self.glyf.as_ref().ok_or_else(npe)?.get_glyph_bounding_box(n)
    }

    fn get_swf_font_description(self: Rc<Self>, was_embedded: bool) -> FontResult<Rc<dyn SWFFontDescription>> {
        Ok(Rc::new(OTFSWFFont3Description::new(self, was_embedded)))
    }

    fn get_swf_font4_description(self: Rc<Self>, was_embedded: bool) -> FontResult<Rc<dyn SWFFont4Description>> {
        Ok(Rc::new(OTFSWFFont4Description::new(self, was_embedded)))
    }

    fn get_embedding_permission(&self, was_embedded: bool) -> FontResult<Permission> {
        match &self.os2 {
            None => match &self.cff {
                Some(c) => Ok(c.get_embedding_permission(was_embedded)),
                None => Ok(EmbeddingPermission::get_true_type_default_permission()),
            },
            Some(o) => o.get_embedding_permission(),
        }
    }

    fn get_fxg_font_description(&self, platform: Option<Platform>, locale: Option<&str>) -> FontResult<Vec<FXGFontDescription>> {
        let mut list = Vec::new();
        if let Some(name) = &self.name {
            let mut mac = None;
            if platform.is_none() || platform == Some(Platform::MacOsx) {
                mac = name.get_mac_fxg_family_names(locale)?;
            }
            let mut win = None;
            if (platform.is_none() || platform == Some(Platform::Windows)) && self.os2.is_some() {
                win = name.get_windows_fxg_family_names(locale)?;
            }
            if let Some(set) = mac {
                for e in set {
                    if let Some(l) = Name::get_locale_for_locale_id(e.get_platform_id(), e.get_language()) {
                        list.push(FXGFontDescription::new(Platform::MacOsx, l, e.get_name().to_string(), false, false));
                    }
                }
            }
            if let Some(set) = win {
                let n = self.os2.as_ref().unwrap().get_selection()?;
                for e in set {
                    if let Some(l) = Name::get_locale_for_locale_id(e.get_platform_id(), e.get_language()) {
                        list.push(FXGFontDescription::new(Platform::Windows, l, e.get_name().to_string(), n & 0x20 > 0, n & 1 > 0));
                    }
                }
            }
        }
        if let Some(fond) = &self.fond {
            let l = fond.get_locale()?;
            if (platform == Some(Platform::MacOsx) || platform.is_none()) && crate::asc::flex2::fonts::adobe::fontengine::font::opentype::name::NameEntrySet::is_contained_within(&l, locale) {
                let d = FXGFontDescription::new(Platform::MacOsx, l, fond.get_name()?.ok_or_else(npe)?, fond.is_bold()?, fond.is_italic()?);
                let s = d.to_java_string();
                if !list.iter().any(|x: &FXGFontDescription| x.to_java_string() == s) {
                    list.push(d);
                }
            }
        }
        Ok(list)
    }

    fn get_platform_font_description(&self, platform: Option<Platform>, locale: Option<&str>) -> FontResult<Vec<PlatformFontDescription>> {
        let Some(name) = &self.name else { return Ok(Vec::new()) };
        let mut mac = None;
        if platform.is_none() || platform == Some(Platform::MacOsx) {
            mac = name.get_mac_platform_names(locale)?;
        }
        let mut win = None;
        if platform.is_none() || platform == Some(Platform::Windows) {
            win = name.get_windows_platform_names(locale)?;
        }
        let mut list = Vec::new();
        for (set, p) in [(mac, Platform::MacOsx), (win, Platform::Windows)] {
            if let Some(set) = set {
                for e in set {
                    if let Some(l) = Name::get_locale_for_locale_id(e.get_platform_id(), e.get_language()) {
                        list.push(PlatformFontDescription::new(p, l, e.get_name().to_string()));
                    }
                }
            }
        }
        Ok(list)
    }
}

/// `OpenTypeFont.OTXDCFontDescription`, the members SWF embedding reaches.
pub struct OTXDCFontDescription<'a> {
    font: &'a OpenTypeFont,
}

impl<'a> OTXDCFontDescription<'a> {
    pub fn get_glyph_cid(&self, n: i32) -> FontResult<i32> {
        self.font.get_glyph_cid(n)
    }

    pub fn get_glyph_name(&self, n: i32) -> FontResult<String> {
        self.font.get_glyph_name(n)
    }

    pub fn get_ros(&self) -> Option<ROS> {
        if self.font.cff.is_some() {
            return self.font.get_ros();
        }
        None
    }

    pub fn get_italic_angle(&self) -> FontResult<f64> {
        let Some(hhea) = &self.font.hhea else { return Ok(0.0) };
        let n = hhea.get_rise()?;
        let n2 = hhea.get_run()?;
        let mut d = 0.0;
        if n != 0 && n2 != 0 {
            d = ((n2.wrapping_neg()) as f64 / n as f64).atan() / std::f64::consts::PI * 180.0;
            if n2 < 0 {
                d += 180.0;
            }
        }
        Ok(d)
    }
}

impl<'a> PDFFontDescription for OTXDCFontDescription<'a> {
    fn get_postscript_name(&self) -> FontResult<Option<String>> {
        if let Some(b) = self.get_base14_name() {
            return Ok(Some(b));
        }
        if let Some(name) = &self.font.name {
            let mut sel = SingleNameSelector::single_postscript_name();
            name.enumerate_names(&mut sel, 6)?;
            if let Some(s) = sel.get_selection() {
                return Ok(Some(s));
            }
        }
        if let Some(cff) = &self.font.cff {
            return Ok(Some(cff.get_cff_font().base().get_name().to_string()));
        }
        Ok(None)
    }

    fn get_font_family(&self) -> FontResult<Option<String>> {
        let Some(name) = &self.font.name else { return Ok(None) };
        let mut sel = SingleNameSelector::single_family_name();
        name.enumerate_names(&mut sel, name.select_family_name_id()?)?;
        Ok(sel.get_selection())
    }

    fn get_font_bbox(&self) -> FontResult<Option<Rect>> {
        let Some(head) = &self.font.head else { return Ok(None) };
        let r = head.get_font_bbox()?;
        let n = head.get_units_per_em()? as f64;
        Ok(Some(Rect::new(r.xmin * 1000.0 / n, r.ymin * 1000.0 / n, r.xmax * 1000.0 / n, r.ymax * 1000.0 / n)))
    }

    fn get_num_glyphs(&self) -> FontResult<i32> {
        self.font.get_num_glyphs()
    }

    fn get_advance(&self, n: i32) -> FontResult<f64> {
        Ok(self.font.get_horizontal_advance(n)? * 1000.0 / self.font.get_units_per_em_x()?)
    }

    fn get_base14_name(&self) -> Option<String> {
        self.font.base14_ps_name.clone()
    }

    fn pdf_font_is_true_type(&self) -> bool {
        self.font.cff.is_none()
    }
}

impl<'a> XDCFontDescription for OTXDCFontDescription<'a> {}

#[allow(dead_code)]
fn unused(_: &dyn VerticalOriginFetcher, _: &dyn NameSelector) {}
