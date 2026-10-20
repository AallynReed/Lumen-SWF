//! Type information about classes and properties: FFDec's `AbcIndexing`.

use super::ctx::LocalData;
use super::item::It;

/// `AbcIndexing.isPropertyAmbiguous` for a multiname property of `parent`.
pub fn is_property_ambiguous(_ld: &LocalData, _mn: usize, _parent: &It) -> Option<bool> {
    None
}

pub fn is_property_ambiguous_name(_ld: &LocalData, _name: &str, _parent: &It) -> Option<bool> {
    None
}
