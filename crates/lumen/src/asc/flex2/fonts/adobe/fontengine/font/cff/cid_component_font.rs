//! Port of afe.jar `com.adobe.fontengine.font.cff.CIDComponentFont`.

use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::CFFByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::char_strings::CharStrings;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::dict::{Dict, Key, NumbersValue};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::string_index::StringIndex;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_consumer::Type2Consumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_outline_parser::{OutlineTarget, Type2OutlineParser};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_parser::Type2Parser;
use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

#[derive(Clone, Debug)]
pub struct CIDComponentFont {
    pub font_dict: Dict,
    pub private_dict: Dict,
    pub local_subrs: Option<CharStrings>,
}

impl CIDComponentFont {
    pub fn parse(data: &Rc<CFFByteArray>, n: i32, n2: i32, si: Option<&StringIndex>) -> FontResult<CIDComponentFont> {
        let font_dict = Dict::parse(data, n, n2, si)?;
        let osv = match font_dict.get_offset_size(&Key::PRIVATE, false) {
            None => return Err(FontEngineException::invalid_font("Missing privateDict in CID component font")),
            Some(v) => v,
        };
        let private_dict = Dict::parse(data, osv.offset, osv.size, si)?;
        let local_subrs = match private_dict.get_offset(&Key::SUBRS, false) {
            Some(ov) => Some(CharStrings::new(data.clone(), osv.offset + ov.offset)?),
            None => None,
        };
        Ok(CIDComponentFont { font_dict, private_dict, local_subrs })
    }

    pub fn new(font_dict: Dict, private_dict: Dict, local_subrs: Option<CharStrings>) -> CIDComponentFont {
        CIDComponentFont { font_dict, private_dict, local_subrs }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn get_outline(&self, cs: &CharStrings, n: i32, gsubrs: Option<&CharStrings>, p: &mut Type2OutlineParser, oc: OutlineTarget<'_>, nv: Option<&NumbersValue>) -> FontResult<()> {
        let m = self.get_outline_matrix(nv);
        p.parse(cs, n, self.local_subrs.as_ref(), gsubrs, oc, &m, None)
    }

    pub fn get_stem_v(&self) -> f64 {
        if let Some(nv) = self.private_dict.get_numbers(&Key::STD_VW, false) {
            return nv.get_first_value_as_double();
        }
        if let Some(nv) = self.private_dict.get_numbers(&Key::STEM_SNAP_V, false) {
            return nv.get_first_value_as_double();
        }
        0.0
    }

    pub fn get_outline_matrix(&self, nv: Option<&NumbersValue>) -> Matrix {
        let nv2 = self.font_dict.get_numbers(&Key::FONT_MATRIX, false);
        match (nv, nv2) {
            (None, None) => Matrix::from_array(&self.font_dict.get_numbers(&Key::FONT_MATRIX, true).unwrap().get_values_as_double()),
            (None, Some(b)) => Matrix::from_array(&b.get_values_as_double()),
            (Some(a), None) => Matrix::from_array(&a.get_values_as_double()),
            (Some(a), Some(b)) => Matrix::from_array(&b.get_values_as_double()).multiply(&Matrix::from_array(&a.get_values_as_double())),
        }
    }

    pub fn get_horizontal_advance(&self, cs: &CharStrings, n: i32, gsubrs: Option<&CharStrings>) -> FontResult<f64> {
        let mut p = Type2Parser::new();
        let mut wc = WidthConsumer::default();
        p.parse(cs, n, self.local_subrs.as_ref(), gsubrs, &mut wc, None)?;
        if wc.width_seen {
            let nv = self.private_dict.get_numbers(&Key::NOMINAL_WIDTH_X, true).unwrap();
            return Ok(nv.get_first_value_as_double() + wc.width);
        }
        Ok(self.private_dict.get_numbers(&Key::DEFAULT_WIDTH_X, true).unwrap().get_first_value_as_double())
    }

    pub fn collect_strings(&self, list: &mut Vec<String>) {
        self.font_dict.collect_strings(list);
        self.private_dict.collect_strings(list);
    }
}

/// `CIDComponentFont.WidthConsumer` (also `NameKeyedFont.WidthConsumer`).
#[derive(Clone, Debug, Default)]
pub struct WidthConsumer {
    pub width: f64,
    pub width_seen: bool,
}

impl Type2Consumer for WidthConsumer {
    fn width(&mut self, d: f64) -> bool {
        self.width_seen = true;
        self.width = d;
        false
    }
}
