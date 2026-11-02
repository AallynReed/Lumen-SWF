//! Port of afe.jar `com.adobe.fontengine.font.opentype.Table`.

use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::base::Base;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::cff::Cff;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::cmap::Cmap;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::cvt::Cvt;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::fond::Fond;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::fpgm::Fpgm;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gasp::Gasp;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gdef::Gdef;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::glyf_raw::GlyfRaw;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gpos::Gpos;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gsub::Gsub;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::head::Head;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::hhea::Hhea;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::hmtx_raw::HmtxRaw;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::kern::Kern;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::loca_raw::LocaRaw;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ltsh::Ltsh;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::maxp::Maxp;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::name::Name;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::os2::Os2;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::post::Post;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::prep::Prep;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::vhea::Vhea;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::vmtx_raw::VmtxRaw;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::vorg::Vorg;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[derive(Clone, Debug, Default)]
pub struct Table {
    pub data: OTByteArray,
}

/// A loaded table as `Table.factory` returns it; the faces of a TTC share these.
#[derive(Clone)]
pub enum AnyTable {
    Cmap(Rc<Cmap>),
    Hhea(Rc<Hhea>),
    HmtxRaw(Rc<HmtxRaw>),
    Maxp(Rc<Maxp>),
    Name(Rc<Name>),
    Os2(Rc<Os2>),
    Gpos(Rc<Gpos>),
    Gsub(Rc<Gsub>),
    Gdef(Rc<Gdef>),
    Base(Rc<Base>),
    Head(Rc<Head>),
    Post(Rc<Post>),
    GlyfRaw(Rc<GlyfRaw>),
    LocaRaw(Rc<LocaRaw>),
    Cff(Rc<Cff>),
    Vhea(Rc<Vhea>),
    VmtxRaw(Rc<VmtxRaw>),
    Cvt(Rc<Cvt>),
    Fpgm(Rc<Fpgm>),
    Prep(Rc<Prep>),
    Kern(Rc<Kern>),
    Gasp(Rc<Gasp>),
    Ltsh(Rc<Ltsh>),
    Vorg(Rc<Vorg>),
    Fond(Rc<Fond>),
    Other(Rc<Table>),
}

impl AnyTable {
    /// The table's `data` field; `Cff` never sets it.
    pub fn data(&self) -> Option<&OTByteArray> {
        Some(match self {
            AnyTable::Cmap(t) => &t.data,
            AnyTable::Hhea(t) => &t.data,
            AnyTable::HmtxRaw(t) => &t.data,
            AnyTable::Maxp(t) => &t.data,
            AnyTable::Name(t) => &t.data,
            AnyTable::Os2(t) => &t.data,
            AnyTable::Gpos(t) => &t.data,
            AnyTable::Gsub(t) => &t.data,
            AnyTable::Gdef(t) => &t.data,
            AnyTable::Base(t) => &t.data,
            AnyTable::Head(t) => &t.data,
            AnyTable::Post(t) => &t.data,
            AnyTable::GlyfRaw(t) => &t.data,
            AnyTable::LocaRaw(t) => &t.data,
            AnyTable::Cff(_) => return None,
            AnyTable::Vhea(t) => &t.data,
            AnyTable::VmtxRaw(t) => &t.data,
            AnyTable::Cvt(t) => &t.data,
            AnyTable::Fpgm(t) => &t.data,
            AnyTable::Prep(t) => &t.data,
            AnyTable::Kern(t) => &t.data,
            AnyTable::Gasp(t) => &t.data,
            AnyTable::Ltsh(t) => &t.data,
            AnyTable::Vorg(t) => &t.data,
            AnyTable::Fond(t) => &t.data,
            AnyTable::Other(t) => &t.data,
        })
    }
}

impl Table {
    pub fn factory(n: i32, a: FontByteArray) -> FontResult<AnyTable> {
        Ok(match n {
            1668112752 => AnyTable::Cmap(Rc::new(Cmap::new(a)?)),
            1751672161 => AnyTable::Hhea(Rc::new(Hhea::new(a)?)),
            1752003704 => AnyTable::HmtxRaw(Rc::new(HmtxRaw::new(a))),
            1835104368 => AnyTable::Maxp(Rc::new(Maxp::new(a)?)),
            1851878757 => AnyTable::Name(Rc::new(Name::new(a))),
            1330851634 => AnyTable::Os2(Rc::new(Os2::new(a))),
            1196445523 => AnyTable::Gpos(Rc::new(Gpos::new(a)?)),
            1196643650 => AnyTable::Gsub(Rc::new(Gsub::new(a)?)),
            1195656518 => AnyTable::Gdef(Rc::new(Gdef::new(a)?)),
            1111577413 => AnyTable::Base(Rc::new(Base::new(a)?)),
            1751474532 => AnyTable::Head(Rc::new(Head::new(a)?)),
            1886352244 => AnyTable::Post(Rc::new(Post::new(a)?)),
            1735162214 => AnyTable::GlyfRaw(Rc::new(GlyfRaw::new(a))),
            1819239265 => AnyTable::LocaRaw(Rc::new(LocaRaw::new(a))),
            1128678944 => AnyTable::Cff(Rc::new(Cff::new(a)?)),
            1986553185 => AnyTable::Vhea(Rc::new(Vhea::new(a)?)),
            1986884728 => AnyTable::VmtxRaw(Rc::new(VmtxRaw::new(a))),
            1668707360 => AnyTable::Cvt(Rc::new(Cvt::new(a))),
            1718642541 => AnyTable::Fpgm(Rc::new(Fpgm::new(a))),
            1886545264 => AnyTable::Prep(Rc::new(Prep::new(a))),
            1801810542 => AnyTable::Kern(Rc::new(Kern::new(a)?)),
            1734439792 => AnyTable::Gasp(Rc::new(Gasp::new(a))),
            1280594760 => AnyTable::Ltsh(Rc::new(Ltsh::new(a))),
            1448038983 => AnyTable::Vorg(Rc::new(Vorg::new(a))),
            1718578788 => AnyTable::Fond(Rc::new(Fond::new(a))),
            _ => AnyTable::Other(Rc::new(Table::new(a))),
        })
    }

    pub fn known_table(n: i32) -> bool {
        matches!(
            n,
            1111577413
                | 1128678944
                | 1195656518
                | 1196445523
                | 1196643650
                | 1280594760
                | 1330851634
                | 1448038983
                | 1668112752
                | 1668707360
                | 1718578788
                | 1718642541
                | 1734439792
                | 1735162214
                | 1751474532
                | 1751672161
                | 1752003704
                | 1801810542
                | 1819239265
                | 1835104368
                | 1851878757
                | 1886352244
                | 1886545264
                | 1986553185
                | 1986884728
        )
    }

    pub fn new(a: FontByteArray) -> Table {
        Table { data: OTByteArray::from_font_byte_array(a) }
    }

    pub fn get_data_as_byte_array(&self) -> OTByteArrayBuilder {
        data_as_byte_array(&self.data)
    }

    pub fn checksum(&self, n: i32, n2: i32, l: i64) -> FontResult<i64> {
        self.data.checksum(n, n2, l)
    }
}

/// `Table.getDataAsByteArray()` for a subclass that holds `data`.
pub fn data_as_byte_array(data: &OTByteArray) -> OTByteArrayBuilder {
    OTByteArray::get_ot_byte_array_builder_instance_from(data)
}
