//! Ports of Batik's `bridge.PaintServer`, `AbstractSVGGradientElementBridge` (with its `SVGStopElementBridge`),
//! `SVGLinearGradientElementBridge` and `SVGRadialGradientElementBridge`; `SVGPatternElementBridge`'s paint is
//! the opaque `PatternPaint` the SWF builders skip.

use std::rc::Rc;

use super::bridge_context::BridgeContext;
use super::css_utilities::{computed, get_computed_style, is_linear_rgb_interpolation};
use super::svg_utilities::{self as su, OBJECT_BOUNDING_BOX};
use super::unit_processor::create_context;
use super::{bridge_exception, BResult, BridgeError, ERR_ATTRIBUTE_MISSING, ERR_ATTRIBUTE_VALUE_MALFORMED, ERR_CSS_URI_BAD_TARGET, ERR_URI_MALFORMED, ERR_XLINK_HREF_CIRCULAR_DEPENDENCIES};
use crate::asc::flex2::batik::css::engine::value::{Value, V, CSS_IDENT, CSS_NUMBER, CSS_PERCENTAGE, CSS_RGBCOLOR, CSS_URI, CSS_VALUE_LIST};
use crate::asc::flex2::batik::css::engine::*;
use crate::asc::flex2::batik::dom::{self, NodeRef, SVG_NAMESPACE_URI};
use crate::asc::flex2::batik::ext::awt::{self as ext, LinearGradientPaint, MultipleGradientPaint, RadialGradientPaint};
use crate::asc::flex2::batik::gvt::{GraphicsNode, Kind, ShapePainter};
use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::AffineTransform;
use crate::asc::flex2::fonts::java2d::java::awt::paint::*;

fn runtime(m: String) -> BridgeError {
    BridgeError::Runtime(Some(m))
}

/// `resolveColorComponent(v)`.
fn resolve_color_component(v: &V) -> i32 {
    match v.primitive_type() {
        CSS_PERCENTAGE => {
            let f = v.float_value();
            let f = if f > 100.0 { 100.0 } else if f < 0.0 { 0.0 } else { f };
            java_round(255.0f32 * f / 100.0f32)
        }
        CSS_NUMBER => {
            let f = v.float_value();
            let f = if f > 255.0 { 255.0 } else if f < 0.0 { 0.0 } else { f };
            java_round(f)
        }
        _ => panic!("Error: can't be reached"),
    }
}

/// `Math.round(float)`.
fn java_round(f: f32) -> i32 {
    if f.is_nan() {
        return 0;
    }
    (f as f64 + 0.5).floor() as i32
}

/// `convertColor(c, opacity)`.
pub fn convert_color(c: &V, opacity: f32) -> BResult<Color> {
    let (r, g, b) = c.rgb();
    Color::new(resolve_color_component(&r), resolve_color_component(&g), resolve_color_component(&b), java_round(opacity * 255.0f32)).map_err(runtime)
}

/// `convertOpacity(v)`.
pub fn convert_opacity(v: &V) -> f32 {
    let r = v.float_value();
    if r < 0.0 {
        0.0
    } else if r > 1.0 {
        1.0
    } else {
        r
    }
}

/// `convertStroke(e)`.
pub fn convert_stroke(e: &NodeRef, ctx: &BridgeContext) -> Option<BasicStroke> {
    let width = get_computed_style(e, STROKE_WIDTH_INDEX, ctx).float_value();
    if width == 0.0 {
        return None;
    }
    let cap = match get_computed_style(e, STROKE_LINECAP_INDEX, ctx).string_value().as_bytes()[0] {
        b'b' => CAP_BUTT,
        b'r' => CAP_ROUND,
        _ => CAP_SQUARE,
    };
    let join = match get_computed_style(e, STROKE_LINEJOIN_INDEX, ctx).string_value().as_bytes()[0] {
        b'm' => JOIN_MITER,
        b'r' => JOIN_ROUND,
        _ => JOIN_BEVEL,
    };
    let ml = get_computed_style(e, STROKE_MITERLIMIT_INDEX, ctx).float_value();
    let miterlimit = if ml < 1.0 { 1.0 } else { ml };
    let v = get_computed_style(e, STROKE_DASHARRAY_INDEX, ctx);
    let mut dash = None;
    if v.css_value_type() == CSS_VALUE_LIST {
        let d: Vec<f32> = (0..v.length()).map(|i| v.item(i).float_value()).collect();
        let sum: f32 = d.iter().fold(0.0f32, |a, b| a + b);
        if sum != 0.0 {
            dash = Some(d);
        }
    }
    let mut dash_phase = 0.0f32;
    if let Some(d) = &dash {
        dash_phase = get_computed_style(e, STROKE_DASHOFFSET_INDEX, ctx).float_value();
        if dash_phase < 0.0 {
            let mut len = d.iter().fold(0.0f32, |a, b| a + b);
            if d.len() % 2 != 0 {
                len *= 2.0;
            }
            if len == 0.0 {
                dash_phase = 0.0;
            } else {
                while dash_phase < 0.0 {
                    dash_phase += len;
                }
            }
        }
    }
    Some(BasicStroke { width, cap, join, miterlimit, dash, dash_phase })
}

/// `convertFillPaint`/`convertStrokePaint` through `convertPaint`.
pub fn convert_paint_property(e: &NodeRef, node: &GraphicsNode, opacity_idx: usize, paint_idx: usize, ctx: &BridgeContext) -> BResult<Option<Paint>> {
    let opacity = convert_opacity(&get_computed_style(e, opacity_idx, ctx));
    let v = computed(e, paint_idx, ctx);
    convert_paint(e, node, &v, opacity, ctx)
}

/// `convertPaint(paintedElement, paintedNode, paintDef, opacity, ctx)`.
pub fn convert_paint(e: &NodeRef, node: &GraphicsNode, def: &V, opacity: f32, ctx: &BridgeContext) -> BResult<Option<Paint>> {
    if def.css_value_type() != CSS_VALUE_LIST {
        return match def.primitive_type() {
            CSS_IDENT => Ok(None),
            CSS_RGBCOLOR => Ok(Some(Paint::Color(convert_color(def, opacity)?))),
            CSS_URI => convert_uri_paint(e, node, def.string_value(), opacity, ctx),
            _ => panic!("Error: can't be reached"),
        };
    }
    let v = def.item(0);
    match v.primitive_type() {
        CSS_RGBCOLOR => Ok(Some(Paint::Color(convert_color(&v, opacity)?))),
        CSS_URI => {
            match convert_uri_paint(e, node, v.string_value(), opacity, ctx) {
                Ok(Some(p)) => return Ok(Some(p)),
                Ok(None) | Err(BridgeError::Bridge(_)) => {}
                Err(err) => return Err(err),
            }
            let v1 = def.item(1);
            match v1.primitive_type() {
                CSS_IDENT => Ok(None),
                CSS_RGBCOLOR => Ok(Some(Paint::Color(convert_color(&v1, opacity)?))),
                _ => panic!("Error: can't be reached"),
            }
        }
        t => Err(runtime(format!("Unallowed Value: {t}"))),
    }
}

/// `convertURIPaint(...)`.
fn convert_uri_paint(e: &NodeRef, node: &GraphicsNode, uri: &str, opacity: f32, ctx: &BridgeContext) -> BResult<Option<Paint>> {
    let paint_element = ctx.get_referenced_element(e, uri)?;
    let is_svg = paint_element.namespace_uri() == Some(SVG_NAMESPACE_URI);
    match paint_element.local_name() {
        Some("linearGradient") | Some("radialGradient") if is_svg => create_gradient_paint(ctx, &paint_element, e, node, opacity),
        Some("pattern") if is_svg => Ok(Some(Paint::Pattern)),
        _ => Err(bridge_exception(e, ERR_CSS_URI_BAD_TARGET, &[uri])),
    }
}

/// `convertFillAndStroke(e, node, ctx)`.
pub fn convert_fill_and_stroke(e: &NodeRef, node: &GraphicsNode, ctx: &BridgeContext) -> BResult<Option<ShapePainter>> {
    let Kind::Shape { shape: Some(_), .. } = &node.kind else { return Ok(None) };
    let fill = convert_paint_property(e, node, FILL_OPACITY_INDEX, FILL_INDEX, ctx)?;
    let fp = ShapePainter::Fill { paint: fill };
    let Some(stroke) = convert_stroke(e, ctx) else { return Ok(Some(fp)) };
    let sp = ShapePainter::Stroke { stroke: Some(stroke), paint: convert_paint_property(e, node, STROKE_OPACITY_INDEX, STROKE_INDEX, ctx)? };
    Ok(Some(ShapePainter::Composite(vec![fp, sp])))
}

/// `convertStrokePainter(e, node, ctx)`.
pub fn convert_stroke_painter(e: &NodeRef, node: &GraphicsNode, ctx: &BridgeContext) -> BResult<Option<ShapePainter>> {
    let Kind::Shape { shape: Some(_), .. } = &node.kind else { return Ok(None) };
    let Some(stroke) = convert_stroke(e, ctx) else { return Ok(None) };
    Ok(Some(ShapePainter::Stroke { stroke: Some(stroke), paint: convert_paint_property(e, node, STROKE_OPACITY_INDEX, STROKE_INDEX, ctx)? }))
}

struct Stop {
    color: Color,
    offset: f32,
}

/// `AbstractSVGGradientElementBridge.createPaint(...)`.
fn create_gradient_paint(ctx: &BridgeContext, paint_element: &NodeRef, painted_element: &NodeRef, node: &GraphicsNode, opacity: f32) -> BResult<Option<Paint>> {
    let Some(stops) = extract_stop(paint_element, opacity, ctx)? else { return Ok(None) };
    if stops.len() == 1 {
        return Ok(Some(Paint::Color(stops[0].color)));
    }
    let offsets: Vec<f32> = stops.iter().map(|s| s.offset).collect();
    let colors: Vec<Color> = stops.iter().map(|s| s.color).collect();
    let mut spread = ext::NO_CYCLE;
    let s = su::get_chainable_attribute_ns(paint_element, "spreadMethod", ctx)?;
    if !s.is_empty() {
        spread = match s.as_str() {
            "repeat" => ext::REPEAT,
            "reflect" => ext::REFLECT,
            "pad" => ext::NO_CYCLE,
            _ => return Err(bridge_exception(paint_element, ERR_ATTRIBUTE_VALUE_MALFORMED, &["spreadMethod", &s])),
        };
    }
    let color_space = if is_linear_rgb_interpolation(paint_element, ctx) { ext::LINEAR_RGB } else { ext::SRGB };
    let s = su::get_chainable_attribute_ns(paint_element, "gradientTransform", ctx)?;
    let transform = if !s.is_empty() { su::convert_transform(paint_element, "gradientTransform", &s)? } else { AffineTransform::new() };
    let _ = painted_element;
    if paint_element.local_name() == Some("linearGradient") {
        build_linear(paint_element, node, spread, color_space, transform, &colors, &offsets, ctx)
    } else {
        build_radial(paint_element, node, spread, color_space, transform, &colors, &offsets, ctx)
    }
}

fn chain(e: &NodeRef, attr: &str, default: &str, ctx: &BridgeContext) -> BResult<String> {
    let s = su::get_chainable_attribute_ns(e, attr, ctx)?;
    Ok(if s.is_empty() { default.to_string() } else { s })
}

fn units(e: &NodeRef, ctx: &BridgeContext) -> BResult<i16> {
    let s = su::get_chainable_attribute_ns(e, "gradientUnits", ctx)?;
    if s.is_empty() {
        Ok(OBJECT_BOUNDING_BOX)
    } else {
        su::parse_coordinate_system(e, "gradientUnits", &s)
    }
}

#[allow(clippy::too_many_arguments)]
fn build_linear(e: &NodeRef, node: &GraphicsNode, spread: i32, cs: i32, mut transform: AffineTransform, colors: &[Color], offsets: &[f32], ctx: &BridgeContext) -> BResult<Option<Paint>> {
    let x1 = chain(e, "x1", "0%", ctx)?;
    let y1 = chain(e, "y1", "0%", ctx)?;
    let x2 = chain(e, "x2", "100%", ctx)?;
    let y2 = chain(e, "y2", "0%", ctx)?;
    let units = units(e, ctx)?;
    if units == OBJECT_BOUNDING_BOX {
        transform = su::to_object_bbox(&transform, node);
    }
    let uctx = create_context(ctx, e);
    let p1 = su::convert_point(&x1, "x1", &y1, "y1", units, &uctx)?;
    let p2 = su::convert_point(&x2, "x2", &y2, "y2", units, &uctx)?;
    if p1 == p2 {
        return Ok(Some(Paint::Color(colors[colors.len() - 1])));
    }
    let base = MultipleGradientPaint::new(offsets, colors, spread, cs, transform).map_err(runtime)?;
    Ok(Some(Paint::LinearGradient(Rc::new(LinearGradientPaint::new(p1, p2, base).map_err(runtime)?))))
}

/// `SVGRadialGradientElementBridge.buildGradient`, which always passes `SRGB` whatever `color-interpolation` says.
#[allow(clippy::too_many_arguments)]
fn build_radial(e: &NodeRef, node: &GraphicsNode, spread: i32, _cs: i32, mut transform: AffineTransform, colors: &[Color], offsets: &[f32], ctx: &BridgeContext) -> BResult<Option<Paint>> {
    let cx = chain(e, "cx", "50%", ctx)?;
    let cy = chain(e, "cy", "50%", ctx)?;
    let r = chain(e, "r", "50%", ctx)?;
    let fx = chain(e, "fx", &cx, ctx)?;
    let fy = chain(e, "fy", &cy, ctx)?;
    let units = units(e, ctx)?;
    if units == OBJECT_BOUNDING_BOX {
        transform = su::to_object_bbox(&transform, node);
    }
    let uctx = create_context(ctx, e);
    let r = su::convert_length(&r, "r", units, &uctx)?;
    if r == 0.0 {
        return Ok(Some(Paint::Color(colors[colors.len() - 1])));
    }
    let c = su::convert_point(&cx, "cx", &cy, "cy", units, &uctx)?;
    let f = su::convert_point(&fx, "fx", &fy, "fy", units, &uctx)?;
    let base = MultipleGradientPaint::new(offsets, colors, spread, ext::SRGB, transform).map_err(runtime)?;
    Ok(Some(Paint::RadialGradient(Rc::new(RadialGradientPaint::new(c, r, f, base).map_err(runtime)?))))
}

/// `extractStop(paintElement, opacity, ctx)`.
fn extract_stop(paint_element: &NodeRef, opacity: f32, ctx: &BridgeContext) -> BResult<Option<Vec<Stop>>> {
    let mut refs: Vec<String> = Vec::new();
    let mut pe = paint_element.clone();
    loop {
        if let Some(stops) = extract_local_stop(&pe, opacity, ctx)? {
            return Ok(Some(stops));
        }
        let uri = su::get_xlink_href(&pe);
        if uri.is_empty() {
            return Ok(None);
        }
        let purl = dom::resolve_url(pe.get_url().as_deref(), &uri);
        if purl.is_empty() {
            return Err(bridge_exception(&pe, ERR_URI_MALFORMED, &[&uri]));
        }
        if refs.contains(&purl) {
            return Err(bridge_exception(&pe, ERR_XLINK_HREF_CIRCULAR_DEPENDENCIES, &[&uri]));
        }
        refs.push(purl);
        pe = ctx.get_referenced_element(&pe, &uri)?;
    }
}

fn extract_local_stop(gradient: &NodeRef, opacity: f32, ctx: &BridgeContext) -> BResult<Option<Vec<Stop>>> {
    let mut stops: Option<Vec<Stop>> = None;
    for e in gradient.child_elements() {
        if !e.is_svg("stop") {
            continue;
        }
        let s = e.get_attribute_ns(None, "offset");
        if s.is_empty() {
            return Err(bridge_exception(&e, ERR_ATTRIBUTE_MISSING, &["offset"]));
        }
        let Some(mut offset) = su::convert_ratio(&s) else { return Err(bridge_exception(&e, ERR_ATTRIBUTE_VALUE_MALFORMED, &["offset", &s])) };
        let color = convert_stop_color(&e, opacity, ctx)?;
        let list = stops.get_or_insert_with(Vec::new);
        if let Some(prev) = list.last() {
            if offset < prev.offset {
                offset = prev.offset;
            }
        }
        list.push(Stop { color, offset });
    }
    Ok(stops)
}

/// `CSSUtilities.convertStopColor(e, opacity, ctx)`.
fn convert_stop_color(e: &NodeRef, opacity: f32, ctx: &BridgeContext) -> BResult<Color> {
    let v = computed(e, STOP_COLOR_INDEX, ctx);
    let o = get_computed_style(e, STOP_OPACITY_INDEX, ctx);
    let opacity = opacity * convert_opacity(&o);
    if v.css_value_type() != CSS_VALUE_LIST {
        convert_color(&v, opacity)
    } else {
        convert_color(&v.item(0), opacity)
    }
}

/// The `Value` an ICC `paint` list starts with, for callers that only need the sRGB fallback.
pub fn first_color(v: &V) -> V {
    match v.as_ref() {
        Value::List { items, .. } => items[0].clone(),
        _ => v.clone(),
    }
}
