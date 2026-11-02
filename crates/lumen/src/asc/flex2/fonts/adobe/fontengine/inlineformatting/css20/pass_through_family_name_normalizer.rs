//! Port of afe.jar `com.adobe.fontengine.inlineformatting.css20.PassThroughFamilyNameNormalizer`.

use crate::asc::flex2::fonts::adobe::fontengine::inlineformatting::css20::family_name_normalizer::FamilyNameNormalizer;

#[derive(Clone, Copy, Debug, Default)]
pub struct PassThroughFamilyNameNormalizer;

impl FamilyNameNormalizer for PassThroughFamilyNameNormalizer {
    fn normalize(&self, s: &str) -> String {
        s.to_string()
    }
}
