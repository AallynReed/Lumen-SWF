//! Port of `flex2.compiler.mxml.lang.FrameworkDefs`.

use crate::asc::flex2::compiler::mxml::rep::variable_declaration::VariableDeclaration;

pub const BUILT_IN_EFFECT_NAMES: [&str; 14] = ["Dissolve", "Fade", "WipeLeft", "WipeRight", "WipeUp", "WipeDown", "Zoom", "Resize", "Move", "Pause", "Rotate", "Iris", "Blur", "Glow"];

pub const REQUIRED_TOP_LEVEL_DESCRIPTOR_PROPERTIES: [&str; 3] = ["height", "width", "creationPolicy"];

pub fn is_builtin_effect_name(name: &str) -> bool {
    BUILT_IN_EFFECT_NAMES.contains(&name)
}

pub fn is_required_top_level_descriptor_property(name: &str) -> bool {
    REQUIRED_TOP_LEVEL_DESCRIPTOR_PROPERTIES.contains(&name)
}

/// `bindingManagementVars`.
pub fn binding_management_vars() -> Vec<VariableDeclaration> {
    vec![
        VariableDeclaration::new("mx_internal", "_bindings", "Array", "[]"),
        VariableDeclaration::new("mx_internal", "_watchers", "Array", "[]"),
        VariableDeclaration::new("mx_internal", "_bindingsByDestination", "Object", "{}"),
        VariableDeclaration::new("mx_internal", "_bindingsBeginWithWord", "Object", "{}"),
    ]
}
