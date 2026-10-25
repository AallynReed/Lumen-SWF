//! The `VelocityContext`s the media transcoders fill and merge with `EmbedClass.vm` and `SkinClass.vm`.

use crate::asc::flex2::velocity::{self, Value};

/// The `VelocityContext` `AbstractTranscoder.generateSource` fills for `EmbedClass.vm`.
pub struct EmbedClassContext {
    pub package_name: String,
    pub base_class: Option<String>,
    pub asset_type: Option<String>,
    pub embed_class: String,
    /// `embedMap.entrySet()`, in `HashMap` order.
    pub embed_map: Vec<(String, String)>,
    pub embed_props: Vec<(String, String)>,
}

/// `template.merge(velocityContext, stringWriter)` over `EmbedClass.vm`; `None` where Java catches an exception.
pub fn merge_embed_class(c: &EmbedClassContext) -> Option<String> {
    let entries = |m: &[(String, String)]| Value::map_of("java.util.HashMap", m.iter().map(|(k, v)| (Value::str(k), Value::str(v))).collect());
    let cx = velocity::get_code_gen_context(None);
    cx.put("packageName", Value::str(&c.package_name));
    cx.put("baseClass", c.base_class.as_deref().map_or(Value::Null, Value::str));
    if let Some(t) = &c.asset_type {
        cx.put("assetType", Value::str(t));
    }
    cx.put("embedClass", Value::str(&c.embed_class));
    cx.put("embedMap", entries(&c.embed_map));
    cx.put("embedProps", entries(&c.embed_props));
    velocity::get_template("flex2/compiler/as3/EmbedClass.vm").ok()?.merge(&cx).ok()
}

/// The `VelocityContext` `SkinTranscoder.generateSource` fills for `SkinClass.vm`.
pub struct SkinClassContext {
    pub package_name: String,
    pub base_class_name: String,
    pub class_name: String,
    pub needs_iborder: bool,
    pub needs_border_metrics: bool,
    pub needs_iflex_display_object: bool,
    pub needs_measured_height: bool,
    pub needs_measured_width: bool,
    pub needs_move: bool,
    pub needs_set_actual_size: bool,
    pub needs_name: bool,
    pub needs_to_string: bool,
}

/// `template.merge(velocityContext, stringWriter)` over `SkinClass.vm`; `None` where Java catches an exception.
pub fn merge_skin_class(c: &SkinClassContext) -> Option<String> {
    let cx = velocity::get_code_gen_context(None);
    cx.put("packageName", Value::str(&c.package_name));
    cx.put("baseClassName", Value::str(&c.base_class_name));
    cx.put("className", Value::str(&c.class_name));
    for (k, v) in [
        ("needsIBorder", c.needs_iborder),
        ("needsBorderMetrics", c.needs_border_metrics),
        ("needsIFlexDisplayObject", c.needs_iflex_display_object),
        ("needsMeasuredHeight", c.needs_measured_height),
        ("needsMeasuredWidth", c.needs_measured_width),
        ("needsMove", c.needs_move),
        ("needsSetActualSize", c.needs_set_actual_size),
        ("needsName", c.needs_name),
        ("needsToString", c.needs_to_string),
    ] {
        cx.put(k, Value::Boolean(v));
    }
    velocity::get_template("flex2/compiler/media/SkinClass.vm").ok()?.merge(&cx).ok()
}
