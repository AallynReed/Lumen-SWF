//! Port of Batik's `bridge.CSSUtilities`: the computed CSS values the bridges convert.

use super::bridge_context::BridgeContext;
use crate::asc::flex2::batik::css::engine::value::{Value, V, CSS_IDENT, CSS_RECT};
use crate::asc::flex2::batik::css::engine::*;
use crate::asc::flex2::batik::dom::NodeRef;
use crate::asc::flex2::flash::graphics::g2d::graphic_context::*;
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::{WIND_EVEN_ODD, WIND_NON_ZERO};
use crate::asc::flex2::fonts::java2d::java::awt::paint::AlphaComposite;

/// `getComputedStyle(e, property)`.
pub fn get_computed_style(e: &NodeRef, property: usize, ctx: &BridgeContext) -> V {
    let engine = e.get_css_engine().expect("CSS engine");
    engine.get_computed_style(e, property, ctx)
}

fn sv(e: &NodeRef, p: usize, ctx: &BridgeContext) -> String {
    get_computed_style(e, p, ctx).string_value().to_string()
}

pub fn convert_display(e: &NodeRef, ctx: &BridgeContext) -> bool {
    !sv(e, DISPLAY_INDEX, ctx).starts_with('n')
}

pub fn convert_visibility(e: &NodeRef, ctx: &BridgeContext) -> bool {
    sv(e, VISIBILITY_INDEX, ctx).starts_with('v')
}

/// `convertOpacity(e)`: `SrcOver` (as `None`'s stand-in `Some(SRC_OVER)`), or an alpha composite.
pub fn convert_opacity(e: &NodeRef, ctx: &BridgeContext) -> AlphaComposite {
    let f = get_computed_style(e, OPACITY_INDEX, ctx).float_value();
    if f <= 0.0 {
        AlphaComposite { alpha: 0.0 }
    } else if f >= 1.0 {
        AlphaComposite::SRC_OVER
    } else {
        AlphaComposite { alpha: f }
    }
}

pub fn convert_overflow(e: &NodeRef, ctx: &BridgeContext) -> bool {
    let s = sv(e, OVERFLOW_INDEX, ctx);
    s.starts_with('h') || s.starts_with('s')
}

/// `convertClip(e)`: the `clip: rect(...)` offsets, or `None` for `auto`.
pub fn convert_clip(e: &NodeRef, ctx: &BridgeContext) -> Option<[f32; 4]> {
    let v = get_computed_style(e, CLIP_INDEX, ctx);
    match v.primitive_type() {
        CSS_RECT => {
            let (t, r, b, l) = v.rect();
            Some([t.float_value(), r.float_value(), b.float_value(), l.float_value()])
        }
        CSS_IDENT => None,
        _ => panic!("Error: can't be reached"),
    }
}

pub fn convert_fill_rule(e: &NodeRef, ctx: &BridgeContext) -> i32 {
    if sv(e, FILL_RULE_INDEX, ctx).starts_with('n') {
        WIND_NON_ZERO
    } else {
        WIND_EVEN_ODD
    }
}

pub fn convert_clip_rule(e: &NodeRef, ctx: &BridgeContext) -> i32 {
    if sv(e, CLIP_RULE_INDEX, ctx).starts_with('n') {
        WIND_NON_ZERO
    } else {
        WIND_EVEN_ODD
    }
}

/// `convertShapeRendering(e, hints)`.
pub fn convert_shape_rendering(e: &NodeRef, hints: Option<RenderingHints>, ctx: &BridgeContext) -> Option<RenderingHints> {
    let s = sv(e, SHAPE_RENDERING_INDEX, ctx);
    let len = s.chars().count();
    if len == 4 && s.starts_with('a') {
        return hints;
    }
    if len < 10 {
        return hints;
    }
    let mut h = hints.unwrap_or_default();
    match s.chars().next() {
        Some('o') => {
            h.insert(KEY_RENDERING, "VALUE_RENDER_SPEED");
            h.insert(KEY_ANTIALIASING, "VALUE_ANTIALIAS_OFF");
        }
        Some('c') => {
            h.insert(KEY_RENDERING, "VALUE_RENDER_DEFAULT");
            h.insert(KEY_ANTIALIASING, "VALUE_ANTIALIAS_OFF");
        }
        Some('g') => {
            h.insert(KEY_RENDERING, "VALUE_RENDER_QUALITY");
            h.insert(KEY_ANTIALIASING, "VALUE_ANTIALIAS_ON");
            h.insert(KEY_STROKE_CONTROL, "VALUE_STROKE_PURE");
        }
        _ => {}
    }
    Some(h)
}

/// `convertColorRendering(e, hints)`.
pub fn convert_color_rendering(e: &NodeRef, hints: Option<RenderingHints>, ctx: &BridgeContext) -> Option<RenderingHints> {
    let s = sv(e, COLOR_RENDERING_INDEX, ctx);
    let len = s.chars().count();
    if len == 4 && s.starts_with('a') {
        return hints;
    }
    if len < 13 {
        return hints;
    }
    let mut h = hints.unwrap_or_default();
    match s.chars().nth(8) {
        Some('s') => {
            h.insert(KEY_COLOR_RENDERING, "VALUE_COLOR_RENDER_SPEED");
            h.insert(KEY_ALPHA_INTERPOLATION, "VALUE_ALPHA_INTERPOLATION_SPEED");
        }
        Some('q') => {
            h.insert(KEY_COLOR_RENDERING, "VALUE_COLOR_RENDER_QUALITY");
            h.insert(KEY_ALPHA_INTERPOLATION, "VALUE_ALPHA_INTERPOLATION_QUALITY");
        }
        _ => {}
    }
    Some(h)
}

/// Whether a `filter`/`mask`/`clip-path` property is `none` (an `ident`) or names a URI (its string).
pub fn uri_property(e: &NodeRef, p: usize, ctx: &BridgeContext) -> Option<String> {
    let v = get_computed_style(e, p, ctx);
    if v.primitive_type() == CSS_IDENT {
        None
    } else {
        Some(v.string_value().to_string())
    }
}

pub fn is_linear_rgb_interpolation(e: &NodeRef, ctx: &BridgeContext) -> bool {
    sv(e, COLOR_INTERPOLATION_INDEX, ctx) == "linearrgb"
}

/// The `Value` of a `stop-color`/`fill`-like property, unwrapped.
pub fn computed(e: &NodeRef, p: usize, ctx: &BridgeContext) -> V {
    let v = get_computed_style(e, p, ctx);
    match v.as_ref() {
        Value::Computed { computed, .. } => computed.clone(),
        _ => v,
    }
}
