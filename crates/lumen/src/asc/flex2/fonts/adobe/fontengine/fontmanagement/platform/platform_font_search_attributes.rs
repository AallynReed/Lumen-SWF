//! Port of afe.jar `com.adobe.fontengine.fontmanagement.platform.PlatformFontSearchAttributes`.

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlatformFontSearchAttributes {
    platform_name: String,
}

impl PlatformFontSearchAttributes {
    pub fn new(platform_name: String) -> PlatformFontSearchAttributes {
        PlatformFontSearchAttributes { platform_name }
    }

    pub fn get_platform_name(&self) -> &str {
        &self.platform_name
    }
}
