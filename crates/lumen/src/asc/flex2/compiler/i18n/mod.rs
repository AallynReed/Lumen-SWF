//! Ports of `flex2.compiler.i18n`: `.properties` resource bundles compiled into `ResourceBundle` subclasses.
//! `TranslationFormat`/`TranslationInfo` have one implementation and are folded into `property_translation_format`.

pub mod class_reference;
pub mod i18n_compiler;
pub mod i18n_utils;
pub mod property_text;
pub mod property_translation_format;
pub mod resource_file;

/// `System.getProperty("line.separator")`.
pub const LINE_SEP: &str = if cfg!(windows) { "\r\n" } else { "\n" };
