//! Port of afe.jar `com.adobe.fontengine.font.FontImpl`: `Font` on top of a lazily retrieved `FontData`.

use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::font::Font;
use crate::asc::flex2::fonts::adobe::fontengine::font::font_data::FontData;
use crate::asc::flex2::fonts::adobe::fontengine::font::permission::Permission;
use crate::asc::flex2::fonts::adobe::fontengine::font::swf_font4_description::SWFFont4Description;
use crate::asc::flex2::fonts::adobe::fontengine::font::swf_font_description::SWFFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::fxg::fxg_font_description::FXGFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform::platform_font_description::PlatformFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform_type::Platform;

/// The abstract members of `FontImpl`; every `FontImpl` is a `Font`. The description cache is left out (it only saves work).
pub trait FontImpl {
    fn retrieve_font_data(&self) -> FontResult<Rc<dyn FontData>>;
    fn get_canonical_path(&self) -> Option<String>;
    fn get_length(&self) -> i64;
    fn get_last_modified(&self) -> i64;
    fn was_embedded(&self) -> bool {
        false
    }
    fn to_java_string(&self) -> String;

    fn get_font_data(&self) -> FontResult<Rc<dyn FontData>> {
        self.retrieve_font_data()
    }
}

/// Java's identity filter: a platform must be the same constant, a locale never is.
fn keep(p: Option<Platform>, l: Option<&str>, dp: Platform) -> bool {
    !(p.is_some() && p != Some(dp) || l.is_some())
}

impl<T: FontImpl> Font for T {
    fn get_units_per_em_x(&self) -> FontResult<f64> {
        self.get_font_data()?.get_units_per_em_x()
    }

    fn get_units_per_em_y(&self) -> FontResult<f64> {
        self.get_font_data()?.get_units_per_em_y()
    }

    fn is_symbolic(&self) -> FontResult<bool> {
        self.get_font_data()?.is_symbolic()
    }

    fn can_embed_for_print_and_preview(&self) -> FontResult<bool> {
        Ok(self.get_font_data()?.get_embedding_permission(self.was_embedded())? != Permission::Restricted)
    }

    fn can_embed_for_editting(&self) -> FontResult<bool> {
        let p = self.get_font_data()?.get_embedding_permission(self.was_embedded())?;
        Ok(p != Permission::Restricted && p != Permission::PreviewAndPrint)
    }

    fn get_swf_font_description(&self) -> FontResult<Rc<dyn SWFFontDescription>> {
        self.retrieve_font_data()?.get_swf_font_description(self.was_embedded())
    }

    fn get_swf_font4_description(&self) -> FontResult<Rc<dyn SWFFont4Description>> {
        self.retrieve_font_data()?.get_swf_font4_description(self.was_embedded())
    }

    fn get_fxg_font_description(&self) -> FontResult<Vec<FXGFontDescription>> {
        self.get_fxg_font_description_for(None, None)
    }

    fn get_fxg_font_description_for(&self, platform: Option<Platform>, locale: Option<&str>) -> FontResult<Vec<FXGFontDescription>> {
        let a = self.retrieve_font_data()?.get_fxg_font_description(None, None)?;
        if platform.is_none() && locale.is_none() {
            return Ok(a);
        }
        Ok(a.into_iter().filter(|d| keep(platform, locale, d.get_platform())).collect())
    }

    fn get_platform_font_description(&self) -> FontResult<Vec<PlatformFontDescription>> {
        self.get_platform_font_description_for(None, None)
    }

    fn get_platform_font_description_for(&self, platform: Option<Platform>, locale: Option<&str>) -> FontResult<Vec<PlatformFontDescription>> {
        let a = self.retrieve_font_data()?.get_platform_font_description(None, None)?;
        if platform.is_none() && locale.is_none() {
            return Ok(a);
        }
        Ok(a.into_iter().filter(|d| keep(platform, locale, d.get_platform())).collect())
    }

    fn to_java_string(&self) -> String {
        FontImpl::to_java_string(self)
    }
}
