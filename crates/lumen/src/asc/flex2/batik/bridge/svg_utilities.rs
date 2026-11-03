//! Port of Batik's `bridge.SVGUtilities` (the parts the bridges here call) and `XLinkSupport.getXLinkHref`.

use super::bridge_context::BridgeContext;
use super::unit_processor::{self as up, Context};
use super::{bridge_exception, BResult, BridgeError, ERR_ATTRIBUTE_VALUE_MALFORMED, ERR_XLINK_HREF_CIRCULAR_DEPENDENCIES};
use crate::asc::flex2::batik::dom::{self, NodeRef, XLINK_NAMESPACE_URI};
use crate::asc::flex2::batik::gvt::GraphicsNode;
use crate::asc::flex2::batik::parser;
use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::AffineTransform;

pub const USER_SPACE_ON_USE: i16 = 1;
pub const OBJECT_BOUNDING_BOX: i16 = 2;
pub const STROKE_WIDTH: i16 = 3;

const FEATURES: &[&str] = &["org.w3c.svg", "org.w3c.svg.lang", "org.w3c.svg.static"];

/// `XLinkSupport.getXLinkHref(e)`.
pub fn get_xlink_href(e: &NodeRef) -> String {
    e.get_attribute_ns(Some(XLINK_NAMESPACE_URI), "href")
}

fn tokens<'a>(s: &'a str, delims: &'a str) -> impl Iterator<Item = &'a str> {
    s.split(move |c: char| delims.contains(c)).filter(|t| !t.is_empty())
}

/// `matchUserAgent(elt, ua)` for the transcoder's user agent (language `en`, no extensions).
pub fn match_user_agent(elt: &NodeRef) -> bool {
    if elt.has_attribute_ns(None, "systemLanguage") {
        let sl = elt.get_attribute_ns(None, "systemLanguage");
        if sl.is_empty() {
            return false;
        }
        let ok = tokens(&sl, ", ").any(|s| {
            tokens("en", ", ").any(|t| {
                if s.starts_with(t) {
                    return s.len() <= t.len() || s.as_bytes()[t.len()] == b'-';
                }
                false
            })
        });
        if !ok {
            return false;
        }
    }
    if elt.has_attribute_ns(None, "requiredFeatures") {
        let rf = elt.get_attribute_ns(None, "requiredFeatures");
        if rf.is_empty() {
            return false;
        }
        if tokens(&rf, " ").any(|s| !FEATURES.contains(&s)) {
            return false;
        }
    }
    if elt.has_attribute_ns(None, "requiredExtensions") {
        let re = elt.get_attribute_ns(None, "requiredExtensions");
        if re.is_empty() {
            return false;
        }
        if tokens(&re, " ").next().is_some() {
            return false;
        }
    }
    true
}

/// `getChainableAttributeNS(element, null, attrName, ctx)`.
pub fn get_chainable_attribute_ns(element: &NodeRef, attr: &str, ctx: &BridgeContext) -> BResult<String> {
    let mut e = element.clone();
    let mut refs: Vec<String> = Vec::new();
    loop {
        let v = e.get_attribute_ns(None, attr);
        if !v.is_empty() {
            return Ok(v);
        }
        let uri = get_xlink_href(&e);
        if uri.is_empty() {
            return Ok(String::new());
        }
        let purl = dom::resolve_url(e.get_url().as_deref(), &uri);
        if refs.contains(&purl) {
            return Err(bridge_exception(&e, ERR_XLINK_HREF_CIRCULAR_DEPENDENCIES, &[&uri]));
        }
        let Some(next) = ctx.uri_get_element(&e, &purl)? else { return Err(BridgeError::Runtime(None)) };
        refs.push(purl);
        e = next;
    }
}

/// `convertPoint(xStr, xAttr, yStr, yAttr, unitsType, uctx)`.
pub fn convert_point(x_str: &str, x_attr: &str, y_str: &str, y_attr: &str, units: i16, uctx: &Context) -> BResult<(f64, f64)> {
    let (x, y) = match units {
        OBJECT_BOUNDING_BOX => (up::svg_to_object_bounding_box(x_str, x_attr, up::HORIZONTAL_LENGTH, uctx)?, up::svg_to_object_bounding_box(y_str, y_attr, up::VERTICAL_LENGTH, uctx)?),
        _ => (up::svg_horizontal_coordinate_to_user_space(x_str, x_attr, uctx)?, up::svg_vertical_coordinate_to_user_space(y_str, y_attr, uctx)?),
    };
    Ok((x as f64, y as f64))
}

/// `convertLength(length, attr, unitsType, uctx)`.
pub fn convert_length(length: &str, attr: &str, units: i16, uctx: &Context) -> BResult<f32> {
    match units {
        OBJECT_BOUNDING_BOX => up::svg_length_to_object_bounding_box(length, attr, up::OTHER_LENGTH, uctx),
        _ => up::svg_other_length_to_user_space(length, attr, uctx),
    }
}

/// `parseCoordinateSystem(e, attr, coordinateSystem)`.
pub fn parse_coordinate_system(e: &NodeRef, attr: &str, cs: &str) -> BResult<i16> {
    match cs {
        "userSpaceOnUse" => Ok(USER_SPACE_ON_USE),
        "objectBoundingBox" => Ok(OBJECT_BOUNDING_BOX),
        _ => Err(bridge_exception(e, ERR_ATTRIBUTE_VALUE_MALFORMED, &[attr, cs])),
    }
}

/// `convertTransform(e, attr, transform)`.
pub fn convert_transform(e: &NodeRef, attr: &str, transform: &str) -> BResult<AffineTransform> {
    parser::parse_transform(transform).map_err(|_| bridge_exception(e, ERR_ATTRIBUTE_VALUE_MALFORMED, &[attr, transform]))
}

/// `toObjectBBox(Tx, node)`.
pub fn to_object_bbox(tx: &AffineTransform, node: &GraphicsNode) -> AffineTransform {
    let mut mx = AffineTransform::new();
    if let Some(b) = node.get_geometry_bounds() {
        mx.translate(b.x, b.y);
        mx.scale(b.w, b.h);
    }
    mx.concatenate(tx);
    mx
}

/// `convertRatio(v)`; a bad number is a `NumberFormatException`.
pub fn convert_ratio(v: &str) -> Option<f32> {
    let (v, d) = match v.strip_suffix('%') {
        Some(s) => (s, 100.0f32),
        None => (v, 1.0f32),
    };
    let r = parser::java_parse_float(v)? / d;
    Some(r.clamp(0.0, 1.0))
}
