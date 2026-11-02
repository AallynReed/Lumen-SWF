//! Port of afe.jar `com.adobe.fontengine.fontmanagement.fxg.FXGFontSearchAttributes`.

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FXGFontSearchAttributes {
    family_name: String,
    is_bold: bool,
    is_italic: bool,
}

impl FXGFontSearchAttributes {
    pub fn new(family_name: String, is_bold: bool, is_italic: bool) -> FXGFontSearchAttributes {
        FXGFontSearchAttributes { family_name, is_bold, is_italic }
    }

    pub fn get_family_name(&self) -> &str {
        &self.family_name
    }

    pub fn is_bold(&self) -> bool {
        self.is_bold
    }

    pub fn is_italic(&self) -> bool {
        self.is_italic
    }
}
