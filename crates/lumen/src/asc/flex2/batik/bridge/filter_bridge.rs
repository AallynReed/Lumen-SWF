//! Ports of Batik's `SVGFilterElementBridge` (`CSSUtilities.convertFilter`), `AbstractSVGFilterPrimitiveElementBridge`,
//! `SVGFeGaussianBlurElementBridge` and the `SVGUtilities` filter region and resolution conversions.

use std::collections::HashMap;
use std::rc::Rc;

use super::bridge_context::BridgeContext;
use super::css_utilities::get_computed_style;
use super::svg_utilities::{self as su, OBJECT_BOUNDING_BOX, USER_SPACE_ON_USE};
use super::unit_processor::{self as up, create_context, Context};
use super::{bridge_exception, BResult, BridgeError, ERR_ATTRIBUTE_VALUE_MALFORMED, ERR_CSS_URI_BAD_TARGET, ERR_XLINK_HREF_CIRCULAR_DEPENDENCIES};
use crate::asc::flex2::batik::css::engine::COLOR_INTERPOLATION_FILTERS_INDEX;
use crate::asc::flex2::batik::dom::{self, NodeRef, SVG_NAMESPACE_URI};
use crate::asc::flex2::batik::gvt::filter::Rable;
use crate::asc::flex2::batik::gvt::GraphicsNode;
use crate::asc::flex2::batik::parser;
use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::AffineTransform;
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::Rectangle2D;

/// The filter primitive bridges `SVGBridgeExtension` registers.
const PRIMITIVES: &[&str] = &[
    "feBlend", "feColorMatrix", "feComponentTransfer", "feComposite", "feConvolveMatrix", "feDiffuseLighting", "feDisplacementMap", "feFlood", "feGaussianBlur", "feImage", "feMerge", "feMorphology",
    "feOffset", "feSpecularLighting", "feTile", "feTurbulence",
];

/// `CSSUtilities.convertFilter` with `SVGFilterElementBridge.createFilter`, for a filter URI.
pub fn create_filter(ctx: &BridgeContext, filtered_element: &NodeRef, uri: &str, filtered_node: &GraphicsNode) -> BResult<Option<Rc<Rable>>> {
    let filter_element = ctx.get_referenced_element(filtered_element, uri)?;
    if !filter_element.is_svg("filter") {
        return Err(bridge_exception(filtered_element, ERR_CSS_URI_BAD_TARGET, &[uri]));
    }
    let filter_region = convert_filter_chain_region(&filter_element, filtered_element, filtered_node, ctx)?;
    let source_graphic = Rc::new(Rable::Pad { src: Rc::new(Rable::SourceGraphic), rect: filter_region });
    let filter_res = convert_filter_res(&filter_element, ctx)?;
    let (res_x, res_y) = (filter_res[0] as i32, filter_res[1] as i32);
    let mut map: HashMap<String, Rc<Rable>> = HashMap::new();
    map.insert("SourceGraphic".into(), source_graphic.clone());
    let input = build_filter_primitives(&filter_element, filter_region, filtered_element, filtered_node, source_graphic.clone(), &mut map, ctx)?;
    match input {
        Some(i) if !Rc::ptr_eq(&i, &source_graphic) => {
            let src = if res_x >= 0 { Rc::new(Rable::FilterRes { src: i, res_x, res_y }) } else { i };
            let crop = Rc::new(Rable::Pad { src, rect: filter_region });
            Ok(Some(Rc::new(Rable::Chain { crop, region: filter_region })))
        }
        _ => Ok(None),
    }
}

/// `buildFilterPrimitives`: the local primitives, else those of the `xlink:href` chain.
fn build_filter_primitives(filter_element: &NodeRef, region: Rectangle2D, filtered_element: &NodeRef, node: &GraphicsNode, input: Rc<Rable>, map: &mut HashMap<String, Rc<Rable>>, ctx: &BridgeContext) -> BResult<Option<Rc<Rable>>> {
    let mut refs: Vec<String> = Vec::new();
    let mut fe = filter_element.clone();
    loop {
        let new_in = build_local_filter_primitives(&fe, region, filtered_element, node, input.clone(), map, ctx)?;
        match &new_in {
            Some(n) if Rc::ptr_eq(n, &input) => {}
            _ => return Ok(new_in),
        }
        let uri = su::get_xlink_href(&fe);
        if uri.is_empty() {
            return Ok(Some(input));
        }
        let url = dom::resolve_url(fe.get_url().as_deref(), &uri);
        if refs.contains(&url) {
            return Err(bridge_exception(&fe, ERR_XLINK_HREF_CIRCULAR_DEPENDENCIES, &[&uri]));
        }
        refs.push(url);
        fe = ctx.get_referenced_element(&fe, &uri)?;
    }
}

/// `buildLocalFilterPrimitives`.
fn build_local_filter_primitives(filter_element: &NodeRef, region: Rectangle2D, filtered_element: &NodeRef, node: &GraphicsNode, mut input: Rc<Rable>, map: &mut HashMap<String, Rc<Rable>>, ctx: &BridgeContext) -> BResult<Option<Rc<Rable>>> {
    for e in filter_element.child_elements() {
        if e.namespace_uri() != Some(SVG_NAMESPACE_URI) {
            continue;
        }
        let Some(name) = e.local_name() else { continue };
        if !PRIMITIVES.contains(&name) {
            continue;
        }
        let f = if name == "feGaussianBlur" {
            create_gaussian_blur(ctx, &e, filtered_element, node, input.clone(), region, map)?
        } else if name == "feOffset" {
            create_offset(ctx, &e, filtered_element, node, input.clone(), region, map)?
        } else {
            match get_in(&e, filtered_element, node, input.clone(), map, ctx)? {
                None => None,
                Some(_) => {
                    let f = Rc::new(Rable::Unsupported(format!("the {name} filter primitive")));
                    update_filter_map(&e, &f, map);
                    Some(f)
                }
            }
        };
        match f {
            None => return Ok(None),
            Some(f) => input = f,
        }
    }
    Ok(Some(input))
}

/// `AbstractSVGFilterPrimitiveElementBridge.getIn`.
fn get_in(e: &NodeRef, filtered_element: &NodeRef, node: &GraphicsNode, input: Rc<Rable>, map: &HashMap<String, Rc<Rable>>, ctx: &BridgeContext) -> BResult<Option<Rc<Rable>>> {
    let s = e.get_attribute_ns(None, "in");
    if s.is_empty() {
        return Ok(Some(input));
    }
    get_filter_source(e, &s, filtered_element, node, map, ctx)
}

/// `AbstractSVGFilterPrimitiveElementBridge.getFilterSource`; the sources this port does not render are
/// `Unsupported` nodes.
fn get_filter_source(_e: &NodeRef, s: &str, _filtered_element: &NodeRef, _node: &GraphicsNode, map: &HashMap<String, Rc<Rable>>, _ctx: &BridgeContext) -> BResult<Option<Rc<Rable>>> {
    let src_g = map.get("SourceGraphic").cloned();
    let source = match s {
        "SourceGraphic" => src_g,
        "SourceAlpha" | "StrokePaint" | "FillPaint" | "BackgroundImage" | "BackgroundAlpha" => Some(Rc::new(Rable::Unsupported(format!("the {s} filter input")))),
        _ => None,
    };
    Ok(source.or_else(|| map.get(s).cloned()))
}

/// `updateFilterMap`.
fn update_filter_map(e: &NodeRef, f: &Rc<Rable>, map: &mut HashMap<String, Rc<Rable>>) {
    let s = e.get_attribute_ns(None, "result");
    if !s.is_empty() && !s.trim().is_empty() {
        map.insert(s, f.clone());
    }
}

/// `CSSUtilities.convertColorInterpolationFilters(e)`.
fn convert_color_interpolation_filters(e: &NodeRef, ctx: &BridgeContext) -> bool {
    get_computed_style(e, COLOR_INTERPOLATION_FILTERS_INDEX, ctx).string_value() == "linearrgb"
}

/// `SVGFeGaussianBlurElementBridge.createFilter`.
fn create_gaussian_blur(ctx: &BridgeContext, e: &NodeRef, filtered_element: &NodeRef, node: &GraphicsNode, input: Rc<Rable>, region: Rectangle2D, map: &mut HashMap<String, Rc<Rable>>) -> BResult<Option<Rc<Rable>>> {
    let sd = convert_std_deviation(e)?;
    if sd[0] < 0.0 || sd[1] < 0.0 {
        let v = format!("{}{}", java_float(sd[0]), java_float(sd[1]));
        return Err(bridge_exception(e, ERR_ATTRIBUTE_VALUE_MALFORMED, &["stdDeviation", &v]));
    }
    let Some(input) = get_in(e, filtered_element, node, input, map, ctx)? else { return Ok(None) };
    let default_region = crate::asc::flex2::batik::gvt::filter::Filter { rable: &input, node }.get_bounds2d();
    let primitive_region = convert_filter_primitive_region(e, filtered_element, node, default_region, region, ctx)?;
    let pad = Rc::new(Rable::Pad { src: input, rect: primitive_region });
    let linear = convert_color_interpolation_filters(e, ctx);
    let blur = Rc::new(Rable::GaussianBlur { src: pad, sdx: sd[0] as f64, sdy: sd[1] as f64, linear });
    let filter = Rc::new(Rable::Pad { src: blur, rect: primitive_region });
    update_filter_map(e, &filter, map);
    Ok(Some(filter))
}

/// `Float.toString(f)` for the error message.
fn java_float(f: f32) -> String {
    crate::xfl::num::java_float(f)
}

/// `convertStdDeviation(filterElement)`.
fn convert_std_deviation(e: &NodeRef) -> BResult<[f32; 2]> {
    let s = e.get_attribute_ns(None, "stdDeviation");
    if s.is_empty() {
        return Ok([0.0, 0.0]);
    }
    let malformed = || bridge_exception(e, ERR_ATTRIBUTE_VALUE_MALFORMED, &["stdDeviation", &s]);
    let mut tokens = s.split([' ', ',']).filter(|t| !t.is_empty());
    let Some(t0) = tokens.next() else { return Err(BridgeError::Runtime(None)) };
    let a = parser::java_parse_float(t0).ok_or_else(malformed)?;
    let b = match tokens.next() {
        Some(t1) => parser::java_parse_float(t1).ok_or_else(malformed)?,
        None => a,
    };
    if tokens.next().is_some() {
        return Err(malformed());
    }
    Ok([a, b])
}

/// `SVGUtilities.convertFilterRes(filterElement, ctx)`.
fn convert_filter_res(e: &NodeRef, ctx: &BridgeContext) -> BResult<[f32; 2]> {
    let s = su::get_chainable_attribute_ns(e, "filterRes", ctx)?;
    let malformed = || bridge_exception(e, ERR_ATTRIBUTE_VALUE_MALFORMED, &["filterRes", &s]);
    let mut vals: [Option<f32>; 2] = [None, None];
    if !s.is_empty() {
        let mut tokens = s.split(' ').filter(|t| !t.is_empty());
        let Some(t0) = tokens.next() else { return Err(BridgeError::Runtime(None)) };
        vals[0] = Some(parser::java_parse_float(t0).ok_or_else(malformed)?);
        if let Some(t1) = tokens.next() {
            vals[1] = Some(parser::java_parse_float(t1).ok_or_else(malformed)?);
        }
        if tokens.next().is_some() {
            return Err(malformed());
        }
    }
    let mut res = [0f32; 2];
    match vals[0] {
        None => res[0] = -1.0,
        Some(v) => {
            res[0] = v;
            if v < 0.0 {
                return Err(malformed());
            }
        }
    }
    match vals[1] {
        None => res[1] = res[0],
        Some(v) => {
            res[1] = v;
            if v < 0.0 {
                return Err(malformed());
            }
        }
    }
    Ok(res)
}

/// `SVGUtilities.convertFilterChainRegion`.
fn convert_filter_chain_region(fe: &NodeRef, filtered_element: &NodeRef, node: &GraphicsNode, ctx: &BridgeContext) -> BResult<Rectangle2D> {
    let attr = |n: &str, d: &str| -> BResult<String> {
        let s = su::get_chainable_attribute_ns(fe, n, ctx)?;
        Ok(if s.is_empty() { d.to_string() } else { s })
    };
    let x = attr("x", "-10%")?;
    let y = attr("y", "-10%")?;
    let w = attr("width", "120%")?;
    let h = attr("height", "120%")?;
    let units = su::get_chainable_attribute_ns(fe, "filterUnits", ctx)?;
    let units_type = if units.is_empty() { OBJECT_BOUNDING_BOX } else { su::parse_coordinate_system(fe, "filterUnits", &units)? };
    let uctx = create_context(ctx, filtered_element);
    let region = convert_region(&x, &y, &w, &h, units_type, node, &uctx)?;
    let units = su::get_chainable_attribute_ns(fe, "filterMarginsUnits", ctx)?;
    let units_type = if units.is_empty() { USER_SPACE_ON_USE } else { su::parse_coordinate_system(fe, "filterMarginsUnits", &units)? };
    extend_region(fe, units_type, node, region, &uctx)
}

/// `SVGUtilities.extendRegion` with the `mx`/`my`/`mw`/`mh` margins of `e`.
fn extend_region(e: &NodeRef, units_type: i16, node: &GraphicsNode, mut region: Rectangle2D, uctx: &Context) -> BResult<Rectangle2D> {
    let m = |n: &str| {
        let s = e.get_attribute_ns(None, n);
        if s.is_empty() {
            "0".to_string()
        } else {
            s
        }
    };
    let (dx_s, dy_s, dw_s, dh_s) = (m("mx"), m("my"), m("mw"), m("mh"));
    let (dx, dy, dw, dh);
    if units_type == USER_SPACE_ON_USE {
        dx = up::svg_horizontal_coordinate_to_user_space(&dx_s, "mx", uctx)?;
        dy = up::svg_vertical_coordinate_to_user_space(&dy_s, "my", uctx)?;
        dw = up::svg_horizontal_coordinate_to_user_space(&dw_s, "mw", uctx)?;
        dh = up::svg_vertical_coordinate_to_user_space(&dh_s, "mh", uctx)?;
    } else {
        match node.get_geometry_bounds() {
            None => (dx, dy, dw, dh) = (0.0, 0.0, 0.0, 0.0),
            Some(b) => {
                dx = (up::svg_to_object_bounding_box(&dx_s, "mx", up::HORIZONTAL_LENGTH, uctx)? as f64 * b.w) as f32;
                dy = (up::svg_to_object_bounding_box(&dy_s, "my", up::VERTICAL_LENGTH, uctx)? as f64 * b.h) as f32;
                dw = (up::svg_to_object_bounding_box(&dw_s, "mw", up::HORIZONTAL_LENGTH, uctx)? as f64 * b.w) as f32;
                dh = (up::svg_to_object_bounding_box(&dh_s, "mh", up::VERTICAL_LENGTH, uctx)? as f64 * b.h) as f32;
            }
        }
    }
    let (x, y, w, h) = (region.x, region.y, region.w, region.h);
    region.set_rect(x + dx as f64, y + dy as f64, w + dw as f64, h + dh as f64);
    Ok(region)
}

/// `SVGUtilities.convertRegion`.
fn convert_region(x_s: &str, y_s: &str, w_s: &str, h_s: &str, units_type: i16, node: &GraphicsNode, uctx: &Context) -> BResult<Rectangle2D> {
    let (mut x, mut y, mut w, mut h): (f64, f64, f64, f64);
    if units_type == OBJECT_BOUNDING_BOX {
        x = up::svg_to_object_bounding_box(x_s, "x", up::HORIZONTAL_LENGTH, uctx)? as f64;
        y = up::svg_to_object_bounding_box(y_s, "y", up::VERTICAL_LENGTH, uctx)? as f64;
        w = up::svg_length_to_object_bounding_box(w_s, "width", up::HORIZONTAL_LENGTH, uctx)? as f64;
        h = up::svg_length_to_object_bounding_box(h_s, "height", up::VERTICAL_LENGTH, uctx)? as f64;
        match node.get_geometry_bounds() {
            Some(b) => {
                x = b.x + x * b.w;
                y = b.y + y * b.h;
                w *= b.w;
                h *= b.h;
            }
            None => (x, y, w, h) = (0.0, 0.0, 0.0, 0.0),
        }
    } else {
        x = up::svg_horizontal_coordinate_to_user_space(x_s, "x", uctx)? as f64;
        y = up::svg_vertical_coordinate_to_user_space(y_s, "y", uctx)? as f64;
        w = up::svg_horizontal_length_to_user_space(w_s, "width", uctx)? as f64;
        h = up::svg_vertical_length_to_user_space(h_s, "height", uctx)? as f64;
    }
    Ok(Rectangle2D::double(x, y, w, h))
}

/// `SVGUtilities.convertFilterPrimitiveRegion`.
fn convert_filter_primitive_region(e: &NodeRef, filtered_element: &NodeRef, node: &GraphicsNode, default_region: Rectangle2D, filter_region: Rectangle2D, ctx: &BridgeContext) -> BResult<Rectangle2D> {
    let parent = e.get_parent_node().filter(|p| p.is_element());
    let units = match &parent {
        Some(p) => su::get_chainable_attribute_ns(p, "primitiveUnits", ctx)?,
        None => String::new(),
    };
    let units_type = if units.is_empty() { USER_SPACE_ON_USE } else { su::parse_coordinate_system(e, "filterUnits", &units)? };
    let (x_s, y_s, w_s, h_s) = (e.get_attribute_ns(None, "x"), e.get_attribute_ns(None, "y"), e.get_attribute_ns(None, "width"), e.get_attribute_ns(None, "height"));
    let (mut x, mut y, mut w, mut h) = (default_region.get_min_x(), default_region.get_min_y(), default_region.w, default_region.h);
    let uctx = create_context(ctx, filtered_element);
    if units_type == OBJECT_BOUNDING_BOX {
        if let Some(b) = node.get_geometry_bounds() {
            if !x_s.is_empty() {
                x = b.x + up::svg_to_object_bounding_box(&x_s, "x", up::HORIZONTAL_LENGTH, &uctx)? as f64 * b.w;
            }
            if !y_s.is_empty() {
                y = b.y + up::svg_to_object_bounding_box(&y_s, "y", up::VERTICAL_LENGTH, &uctx)? as f64 * b.h;
            }
            if !w_s.is_empty() {
                w = up::svg_length_to_object_bounding_box(&w_s, "width", up::HORIZONTAL_LENGTH, &uctx)? as f64 * b.w;
            }
            if !h_s.is_empty() {
                h = up::svg_length_to_object_bounding_box(&h_s, "height", up::VERTICAL_LENGTH, &uctx)? as f64 * b.h;
            }
        }
    } else {
        if !x_s.is_empty() {
            x = up::svg_horizontal_coordinate_to_user_space(&x_s, "x", &uctx)? as f64;
        }
        if !y_s.is_empty() {
            y = up::svg_vertical_coordinate_to_user_space(&y_s, "y", &uctx)? as f64;
        }
        if !w_s.is_empty() {
            w = up::svg_horizontal_length_to_user_space(&w_s, "width", &uctx)? as f64;
        }
        if !h_s.is_empty() {
            h = up::svg_vertical_length_to_user_space(&h_s, "height", &uctx)? as f64;
        }
    }
    let region = Rectangle2D::double(x, y, w, h);
    let units = match &parent {
        Some(p) => su::get_chainable_attribute_ns(p, "filterPrimitiveMarginsUnits", ctx)?,
        None => String::new(),
    };
    let units_type = if units.is_empty() { USER_SPACE_ON_USE } else { su::parse_coordinate_system(e, "filterPrimitiveMarginsUnits", &units)? };
    let region = extend_region(e, units_type, node, region, &uctx)?;
    let x1 = region.get_min_x().max(filter_region.get_min_x());
    let y1 = region.get_min_y().max(filter_region.get_min_y());
    let x2 = region.get_max_x().min(filter_region.get_max_x());
    let y2 = region.get_max_y().min(filter_region.get_max_y());
    let (x1, x2) = if x2 < x1 { (x2, x1) } else { (x1, x2) };
    let (y1, y2) = if y2 < y1 { (y2, y1) } else { (y1, y2) };
    Ok(Rectangle2D::double(x1, y1, x2 - x1, y2 - y1))
}

/// `SVGFeOffsetElementBridge.createFilter`.
fn create_offset(ctx: &BridgeContext, e: &NodeRef, filtered_element: &NodeRef, node: &GraphicsNode, input: Rc<Rable>, region: Rectangle2D, map: &mut HashMap<String, Rc<Rable>>) -> BResult<Option<Rc<Rable>>> {
    let Some(input) = get_in(e, filtered_element, node, input, map, ctx)? else { return Ok(None) };
    let default_region = crate::asc::flex2::batik::gvt::filter::Filter { rable: &input, node }.get_bounds2d();
    let primitive_region = convert_filter_primitive_region(e, filtered_element, node, default_region, region, ctx)?;
    let dx = convert_number(e, "dx", 0.0)?;
    let dy = convert_number(e, "dy", 0.0)?;
    let affine = AffineTransform::get_translate_instance(dx as f64, dy as f64);
    let pad = Rc::new(Rable::Pad { src: input, rect: primitive_region });
    let filter = Rc::new(Rable::Pad { src: Rc::new(Rable::Affine { src: pad, affine }), rect: primitive_region });
    update_filter_map(e, &filter, map);
    Ok(Some(filter))
}

/// `AbstractSVGFilterPrimitiveElementBridge.convertNumber`.
fn convert_number(e: &NodeRef, attr: &str, default: f32) -> BResult<f32> {
    let s = e.get_attribute_ns(None, attr);
    if s.is_empty() {
        return Ok(default);
    }
    parser::java_parse_float(&s).ok_or_else(|| bridge_exception(e, ERR_ATTRIBUTE_VALUE_MALFORMED, &[attr, &s]))
}
