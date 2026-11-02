//! Port of afe.jar `com.adobe.fontengine.font.cff.CFFFont` (the parts the font embedding uses).

use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::char_strings::CharStrings;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::dict::{Dict, Key};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cid_keyed_font::CIDKeyedFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::name_keyed_font::NameKeyedFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::string_index::StringIndex;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_cstring_generator::Type2CStringGenerator;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_outline_parser::{OutlineTarget, Type2OutlineParser};
use crate::asc::flex2::fonts::adobe::fontengine::font::embedding_permission::EmbeddingPermission;
use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;
use crate::asc::flex2::fonts::adobe::fontengine::font::orig_font_type::OrigFontType;
use crate::asc::flex2::fonts::adobe::fontengine::font::permission::Permission;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

/// The fields of the abstract class.
#[derive(Clone, Debug)]
pub struct CFFFont {
    pub string_index: Option<Rc<StringIndex>>,
    pub global_subrs: Option<CharStrings>,
    pub name: String,
    pub top_dict: Dict,
}

impl CFFFont {
    pub fn new(string_index: Option<Rc<StringIndex>>, global_subrs: Option<CharStrings>, top_dict: Dict, name: String) -> CFFFont {
        CFFFont { string_index, global_subrs, name, top_dict }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_preferred_css_family_name(&self) -> String {
        match self.top_dict.get_string(&Key::FAMILY_NAME, false) {
            Some(v) => v.value,
            None => self.name.clone(),
        }
    }

    pub fn is_fixed_pitch(&self) -> bool {
        self.top_dict.get_numbers(&Key::IS_FIXED_PITCH, true).is_some_and(|v| v.get_first_value_as_double() != 0.0)
    }

    pub fn is_css_style_italic(&self) -> bool {
        self.top_dict.get_numbers(&Key::ITALIC_ANGLE, true).is_some_and(|v| v.get_first_value_as_double() != 0.0)
    }

    pub fn get_notice(&self) -> Option<String> {
        self.top_dict.get_string(&Key::NOTICE, true).map(|v| v.value)
    }

    pub fn get_copyright(&self) -> Option<String> {
        self.top_dict.get_string(&Key::COPYRIGHT, true).map(|v| v.value)
    }

    pub fn get_full_name(&self) -> Option<String> {
        self.top_dict.get_string(&Key::FULL_NAME, true).map(|v| v.value)
    }

    pub fn get_xuid(&self) -> Option<Vec<i32>> {
        self.top_dict.get_numbers(&Key::XUID, true).map(|v| v.get_values_as_int())
    }

    pub fn get_fs_type(&self) -> Option<i32> {
        self.top_dict.get_fs_type()
    }

    pub fn get_orig_font_type(&self) -> Option<OrigFontType> {
        self.top_dict.get_orig_font_type()
    }

    pub fn get_italic_angle(&self) -> f64 {
        match self.top_dict.get_numbers(&Key::ITALIC_ANGLE, false) {
            Some(v) => v.get_first_value_as_double(),
            None => 0.0,
        }
    }

    pub fn get_embedding_permission_given_ft(&self, bl: bool, oft: OrigFontType) -> Permission {
        if let Some(n) = self.get_fs_type() {
            return EmbeddingPermission::interpret_fs_type(n);
        }
        let oft2 = self.get_orig_font_type();
        if oft2 == Some(OrigFontType::K_TYPE1) || oft2.is_none() && oft == OrigFontType::K_TYPE1 {
            return EmbeddingPermission::get_type1_default_permission(self.get_notice().as_deref(), Some(&self.name));
        }
        if bl {
            return EmbeddingPermission::get_default_was_embedded_permission();
        }
        if oft2 == Some(OrigFontType::K_CID) || oft2.is_none() && oft == OrigFontType::K_CID {
            return EmbeddingPermission::get_cid_default_permission(self.get_xuid().as_deref());
        }
        if oft2 == Some(OrigFontType::K_TRUETYPE) {
            return EmbeddingPermission::get_true_type_default_permission();
        }
        EmbeddingPermission::get_ocf_default_permission()
    }
}

/// The abstract methods `createSubsetCharstringIndex` relies on.
pub trait CFFFontImpl {
    fn base(&self) -> &CFFFont;
    fn get_num_fds(&self) -> i32;
    fn get_char_strings(&self) -> &CharStrings;
    fn get_local_subrs_for_fd(&self, n: i32) -> Option<&CharStrings>;
    fn get_default_width_for_fd(&self, n: i32) -> f64;
    fn get_nominal_width_for_fd(&self, n: i32) -> f64;
    fn get_fd_for_glyph(&self, n: i32) -> FontResult<i32>;
    fn get_font_matrix(&self) -> Matrix;
    /// `this instanceof NameKeyedFont ? (NameKeyedFont)this : null`.
    fn as_name_keyed(&self) -> Option<&NameKeyedFont> {
        None
    }

    fn get_units_per_em_x(&self) -> f64 {
        1.0 / self.get_font_matrix().a
    }

    fn get_units_per_em_y(&self) -> f64 {
        1.0 / self.get_font_matrix().d
    }
}

pub fn create_subset_charstring_index(f: &dyn CFFFontImpl, subset: &dyn Subset, bl: bool) -> FontResult<CharStrings> {
    let mut g = Type2CStringGenerator::new(subset.get_num_glyphs(), f.get_num_fds(), bl);
    let mut p = Type2OutlineParser::new(true);
    for i in 0..subset.get_num_glyphs() {
        let n = subset.get_full_gid(i);
        let n2 = f.get_fd_for_glyph(n)?;
        g.new_glyph(i, n2, f.get_default_width_for_fd(n2), f.get_nominal_width_for_fd(n2));
        p.parse(f.get_char_strings(), n, f.get_local_subrs_for_fd(n2), f.base().global_subrs.as_ref(), OutlineTarget::Hinted(&mut g), &Matrix::IDENTITY_MATRIX, f.as_name_keyed())?;
    }
    g.get_charstring_index()
}

/// A `CFFFont` reference: the concrete classes a font set can hold.
#[derive(Clone, Debug)]
pub enum AnyCFFFont {
    NameKeyed(NameKeyedFont),
    CIDKeyed(CIDKeyedFont),
}

impl AnyCFFFont {
    pub fn base(&self) -> &CFFFont {
        match self {
            AnyCFFFont::NameKeyed(f) => &f.base,
            AnyCFFFont::CIDKeyed(f) => &f.base,
        }
    }
}
