//! Port of afe.jar `com.adobe.fontengine.fontmanagement.Platform` (named apart from the `platform` package).

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Platform {
    Unknown,
    MacOsx,
    Windows,
}

impl Platform {
    pub fn to_java_string(&self) -> &'static str {
        match self {
            Platform::Unknown => "Unknown",
            Platform::MacOsx => "Mac OS X",
            Platform::Windows => "Windows",
        }
    }

    pub fn parse(s: &str) -> Option<Platform> {
        [Platform::MacOsx, Platform::Windows].into_iter().find(|p| p.to_java_string().eq_ignore_ascii_case(s))
    }
}
