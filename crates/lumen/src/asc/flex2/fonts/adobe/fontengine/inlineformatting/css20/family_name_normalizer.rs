//! Port of afe.jar `com.adobe.fontengine.inlineformatting.css20.FamilyNameNormalizer`.

pub trait FamilyNameNormalizer {
    fn normalize(&self, s: &str) -> String;
}
