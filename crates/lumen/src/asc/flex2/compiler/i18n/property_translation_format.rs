//! Port of `flex2.compiler.i18n.PropertyTranslationFormat` and the `TranslationInfo` it returns.

use std::cell::RefCell;
use std::rc::Rc;

use super::property_text::{PropertyText, PropertyValue};
use super::resource_file::resource_file_of;
use crate::asc::flex2::compiler::css::at_embed::AtEmbed;
use crate::asc::flex2::compiler::source::SourceRef;
use crate::asc::flex2::compiler::util::mime_mappings;
use crate::asc::util::context::ContextStatics;

/// `TranslationInfo`.
pub struct TranslationInfo {
    /// `getClassReferences()`: the `imports` `HashSet`, in its order.
    pub class_references: Vec<String>,
    /// `getEmeds()`.
    pub embeds: Vec<Rc<AtEmbed>>,
    /// `getTranslationSet()`: `entrySet()` in the backing map's order.
    pub translation_set: Vec<(String, PropertyValue)>,
}

pub fn is_supported(mime_type: &str) -> bool {
    mime_type == mime_mappings::PROPERTIES
}

/// `getTranslationSet(configuration, symbolTable, source, locale, standardDefs)`; `Err` is the
/// `TranslationException`'s message.
pub fn get_translation_set(compatibility_version: i32, per_compile_data: Rc<RefCell<ContextStatics>>, source: &SourceRef, locale: &str) -> Result<TranslationInfo, String> {
    let mut p = PropertyText::new(compatibility_version, per_compile_data, source.clone(), locale);
    if let Some(f) = resource_file_of(source) {
        f.set_locale(locale);
        if let Some(bytes) = f.read() {
            let bytes = bytes.map_err(|e| e.to_string())?;
            p.load(&String::from_utf8_lossy(&bytes));
        }
    }
    Ok(TranslationInfo { class_references: p.imports.iter().cloned().collect(), embeds: p.at_embeds.iter().map(|(_, e)| e.clone()).collect(), translation_set: p.props.entry_set() })
}
