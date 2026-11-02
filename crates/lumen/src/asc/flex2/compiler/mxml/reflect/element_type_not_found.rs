//! Port of `flex2.compiler.mxml.reflect.ElementTypeNotFound`.

/// The message's text.
pub fn message(metadata: &str, element_type_name: &str) -> String {
    format!("[{metadata}({element_type_name})]: type {element_type_name} not found.")
}
