//! Port of afe.jar `com.adobe.fontengine.font.OrigFontType`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrigFontType {
    literal: &'static str,
}

impl OrigFontType {
    pub const K_TYPE1: OrigFontType = OrigFontType { literal: "Type1" };
    pub const K_TRUETYPE: OrigFontType = OrigFontType { literal: "TrueType" };
    pub const K_CID: OrigFontType = OrigFontType { literal: "CID" };
    pub const K_OCF: OrigFontType = OrigFontType { literal: "OCF" };

    pub fn to_java_string(&self) -> String {
        self.literal.to_string()
    }
}
