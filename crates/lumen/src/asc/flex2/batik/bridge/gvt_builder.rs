//! Ports of Batik's `bridge.GVTBuilder` and the graphics node bridges it dispatches to:
//! `AbstractGraphicsNodeBridge`, `SVGGElementBridge`, `SVGAElementBridge`, `SVGSVGElementBridge`,
//! `SVGSwitchElementBridge`, `SVGUseElementBridge`, `SVGShapeElementBridge`/`SVGDecoratedShapeElementBridge` with
//! the rect, circle, ellipse, line, polyline, polygon and path bridges, and `SVGClipPathElementBridge`.

use super::bridge_context::BridgeContext;
use super::css_utilities::{self as cu};
use super::paint_server;
use super::svg_utilities::{self as su, OBJECT_BOUNDING_BOX, USER_SPACE_ON_USE};
use super::unit_processor::{self as up, create_context};
use super::view_box;
use super::{bridge_exception, BResult, BridgeError, ERR_ATTRIBUTE_MISSING, ERR_ATTRIBUTE_VALUE_MALFORMED, ERR_CSS_URI_BAD_TARGET};
use crate::asc::flex2::batik::css::engine::value::CSS_IDENT;
use crate::asc::flex2::batik::css::engine::{CSSEngine, CLIP_PATH_INDEX, FILTER_INDEX, MARKER_END_INDEX, MARKER_MID_INDEX, MARKER_START_INDEX, MASK_INDEX, STROKE_WIDTH_INDEX};
use crate::asc::flex2::batik::ext::awt::geom::SEG_ARCTO;
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::{SEG_CLOSE, SEG_LINETO, SEG_MOVETO, SEG_QUADTO};
use crate::asc::flex2::batik::dom::{self, NodeRef, SVG_NAMESPACE_URI};
use crate::asc::flex2::batik::gvt::{ClipRable, GraphicsNode, Kind, Marker, ShapePainter};
use crate::asc::flex2::batik::parser;
use crate::asc::flex2::flash::graphics::g2d::graphic_context::{RenderingHints, KEY_ANTIALIASING};
use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::AffineTransform;
use crate::asc::flex2::fonts::java2d::java::awt::geom::area::Area;
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::{create_transformed_shape, Path2D, Rectangle2D, Shape};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Bridge {
    Svg,
    G,
    A,
    Switch,
    Use,
    Rect,
    Circle,
    Ellipse,
    Line,
    Polyline,
    Polygon,
    Path,
    Image,
    Text,
    /// `desc` and `title`, the `GenericBridge`s.
    Generic,
    /// A bridge that does not make graphics nodes (gradients, clip paths, filters, ...).
    Other,
}

const OTHER_BRIDGES: &[&str] = &[
    "altGlyph", "clipPath", "color-profile", "feBlend", "feColorMatrix", "feComponentTransfer", "feComposite", "feFuncA", "feFuncR", "feFuncG", "feFuncB", "feConvolveMatrix", "feDiffuseLighting",
    "feDisplacementMap", "feDistantLight", "feFlood", "feGaussianBlur", "feImage", "feMerge", "feMergeNode", "feMorphology", "feOffset", "fePointLight", "feSpecularLighting", "feSpotLight", "feTile",
    "feTurbulence", "font", "font-face", "filter", "glyph", "hkern", "linearGradient", "marker", "mask", "missing-glyph", "pattern", "radialGradient", "stop", "textPath", "vkern",
];

/// `ctx.getBridge(e)`: the bridges `SVGBridgeExtension` registers, by SVG local name.
fn get_bridge(e: &NodeRef) -> Option<Bridge> {
    if e.namespace_uri() != Some(SVG_NAMESPACE_URI) {
        return None;
    }
    Some(match e.local_name()? {
        "svg" => Bridge::Svg,
        "g" => Bridge::G,
        "a" => Bridge::A,
        "switch" => Bridge::Switch,
        "use" => Bridge::Use,
        "rect" => Bridge::Rect,
        "circle" => Bridge::Circle,
        "ellipse" => Bridge::Ellipse,
        "line" => Bridge::Line,
        "polyline" => Bridge::Polyline,
        "polygon" => Bridge::Polygon,
        "path" => Bridge::Path,
        "image" => Bridge::Image,
        "text" => Bridge::Text,
        "desc" | "title" => Bridge::Generic,
        n if OTHER_BRIDGES.contains(&n) => Bridge::Other,
        _ => return None,
    })
}

fn is_graphics_bridge(b: Bridge) -> bool {
    !matches!(b, Bridge::Generic | Bridge::Other)
}

/// The elements that implement `SVGTests`, which `switch` considers.
const SVG_TESTS: &[&str] = &[
    "a", "altGlyph", "animate", "animateColor", "animateMotion", "animateTransform", "circle", "clipPath", "cursor", "defs", "ellipse", "foreignObject", "g", "image", "line", "mask", "path", "pattern",
    "polygon", "polyline", "rect", "set", "svg", "switch", "text", "textPath", "tref", "tspan", "use",
];

pub struct GvtBuilder<'a> {
    pub ctx: &'a BridgeContext,
}

impl GvtBuilder<'_> {
    /// `build(ctx, document)`: the `RootGraphicsNode`.
    pub fn build_document(&self, document: &NodeRef) -> BResult<Option<GraphicsNode>> {
        self.ctx.initialize_document(document);
        let mut root = GraphicsNode::composite();
        let svg = document.get_document_element().unwrap();
        let Some(bridge) = get_bridge(&svg).filter(|b| is_graphics_bridge(*b)) else { return Ok(None) };
        let Some(mut top) = self.create_graphics_node(bridge, &svg)? else { return Ok(None) };
        self.build_composite(&svg, &mut top)?;
        self.build_graphics_node_attrs(bridge, &svg, &mut top)?;
        root.children_mut().push(top);
        Ok(Some(root))
    }

    /// `build(ctx, e)` for one element.
    pub fn build(&self, e: &NodeRef) -> BResult<Option<GraphicsNode>> {
        let Some(bridge) = get_bridge(e) else { return Ok(None) };
        if !is_graphics_bridge(bridge) {
            return Ok(None);
        }
        if !cu::convert_display(e, self.ctx) {
            return Ok(None);
        }
        let Some(mut gn) = self.create_graphics_node(bridge, e)? else { return Ok(None) };
        if is_composite(bridge) {
            self.build_composite(e, &mut gn)?;
        }
        self.build_graphics_node_attrs(bridge, e, &mut gn)?;
        Ok(Some(gn))
    }

    fn build_composite(&self, e: &NodeRef, parent: &mut GraphicsNode) -> BResult<()> {
        for n in e.child_elements() {
            self.build_graphics_node(&n, parent)?;
        }
        Ok(())
    }

    fn build_graphics_node(&self, e: &NodeRef, parent: &mut GraphicsNode) -> BResult<()> {
        let Some(bridge) = get_bridge(e) else { return Ok(()) };
        if !is_graphics_bridge(bridge) {
            return Ok(());
        }
        if !cu::convert_display(e, self.ctx) {
            return Ok(());
        }
        if let Some(mut gn) = self.create_graphics_node(bridge, e)? {
            if is_composite(bridge) {
                self.build_composite(e, &mut gn)?;
            }
            self.build_graphics_node_attrs(bridge, e, &mut gn)?;
            parent.children_mut().push(gn);
        }
        Ok(())
    }

    fn create_graphics_node(&self, bridge: Bridge, e: &NodeRef) -> BResult<Option<GraphicsNode>> {
        let ctx = self.ctx;
        match bridge {
            Bridge::Svg => self.create_svg(e),
            Bridge::G | Bridge::A => {
                let Some(mut gn) = self.abstract_create(e, GraphicsNode::composite())? else { return Ok(None) };
                gn.hints = cu::convert_color_rendering(e, None, ctx);
                Ok(Some(gn))
            }
            Bridge::Switch => {
                let mut ref_node = None;
                for r in e.child_elements() {
                    if r.namespace_uri() == Some(SVG_NAMESPACE_URI) && SVG_TESTS.contains(&r.local_name().unwrap_or("")) && su::match_user_agent(&r) {
                        ref_node = self.build(&r)?;
                        break;
                    }
                }
                let Some(ref_node) = ref_node else { return Ok(None) };
                let mut group = GraphicsNode::composite();
                group.children_mut().push(ref_node);
                let s = e.get_attribute_ns(None, "transform");
                if !s.is_empty() {
                    group.transform = Some(su::convert_transform(e, "transform", &s)?);
                }
                Ok(Some(group))
            }
            Bridge::Use => self.create_use(e),
            Bridge::Image => self.create_image(e),
            Bridge::Text => self.abstract_create(e, GraphicsNode::new(Kind::Unsupported("text".into()))),
            _ => {
                let Some(mut node) = self.abstract_create(e, GraphicsNode::new(Kind::Shape { shape: None, painter: None }))? else { return Ok(None) };
                let shape = self.build_shape(bridge, e)?;
                if let Kind::Shape { shape: s, .. } = &mut node.kind {
                    *s = shape;
                }
                let hints = cu::convert_color_rendering(e, None, ctx);
                node.hints = cu::convert_shape_rendering(e, hints, ctx);
                Ok(Some(node))
            }
        }
    }

    /// `AbstractGraphicsNodeBridge.createGraphicsNode`.
    fn abstract_create(&self, e: &NodeRef, mut node: GraphicsNode) -> BResult<Option<GraphicsNode>> {
        if !su::match_user_agent(e) {
            return Ok(None);
        }
        let s = e.get_attribute_ns(None, "transform");
        if !s.is_empty() {
            node.transform = Some(su::convert_transform(e, "transform", &s)?);
        }
        node.visible = cu::convert_visibility(e, self.ctx);
        Ok(Some(node))
    }

    /// `buildGraphicsNode(ctx, e, node)` of each bridge.
    fn build_graphics_node_attrs(&self, bridge: Bridge, e: &NodeRef, node: &mut GraphicsNode) -> BResult<()> {
        let ctx = self.ctx;
        match bridge {
            Bridge::Switch => return Ok(()),
            Bridge::Svg => {
                node.composite = Some(cu::convert_opacity(e, ctx));
                self.convert_filter_and_mask(e, node)?;
                ctx.close_viewport();
                return Ok(());
            }
            Bridge::Rect | Bridge::Circle | Bridge::Ellipse | Bridge::Line | Bridge::Polyline | Bridge::Polygon | Bridge::Path => {
                let painter = self.create_shape_painter(bridge, e, node)?;
                if let Kind::Shape { shape, painter: p } = &mut node.kind {
                    if shape.is_some() {
                        *p = painter;
                    }
                }
            }
            _ => {}
        }
        node.composite = Some(cu::convert_opacity(e, ctx));
        self.convert_filter_and_mask(e, node)?;
        node.clip = self.convert_clip_path(e, node)?;
        Ok(())
    }

    fn convert_filter_and_mask(&self, e: &NodeRef, node: &mut GraphicsNode) -> BResult<()> {
        for (idx, local) in [(FILTER_INDEX, "filter"), (MASK_INDEX, "mask")] {
            if let Some(uri) = cu::uri_property(e, idx, self.ctx) {
                if idx == FILTER_INDEX {
                    node.filter = super::filter_bridge::create_filter(self.ctx, e, &uri, node)?;
                    continue;
                }
                let target = self.ctx.get_referenced_element(e, &uri)?;
                if !target.is_svg(local) {
                    return Err(bridge_exception(e, ERR_CSS_URI_BAD_TARGET, &[&uri]));
                }
                node.mask = true;
            }
        }
        Ok(())
    }

    /// `CSSUtilities.convertClipPath` with `SVGClipPathElementBridge.createClip`: the `Area` of the children's outlines;
    /// an outline that is not a shape (text) is kept as its bounds.
    fn convert_clip_path(&self, e: &NodeRef, clipped: &GraphicsNode) -> BResult<Option<ClipRable>> {
        let Some(uri) = cu::uri_property(e, CLIP_PATH_INDEX, self.ctx) else { return Ok(None) };
        let cp = self.ctx.get_referenced_element(e, &uri)?;
        if !cp.is_svg("clipPath") {
            return Err(bridge_exception(e, ERR_CSS_URI_BAD_TARGET, &[&uri]));
        }
        let s = cp.get_attribute_ns(None, "transform");
        let mut tx = if !s.is_empty() { su::convert_transform(&cp, "transform", &s)? } else { AffineTransform::new() };
        let s = cp.get_attribute_ns(None, "clipPathUnits");
        let units = if s.is_empty() { USER_SPACE_ON_USE } else { su::parse_coordinate_system(&cp, "clipPathUnits", &s)? };
        if units == OBJECT_BOUNDING_BOX {
            tx = su::to_object_bbox(&tx, clipped);
        }
        let mut clip_path = Area::new();
        let mut has_children = false;
        for child in cp.child_elements() {
            let Some(clip_node) = self.build(&child)? else { continue };
            has_children = true;
            let mut at = clip_node.transform.unwrap_or(tx);
            if clip_node.transform.is_some() {
                at.pre_concatenate(&tx);
            }
            let mut path = match outline_path(&clip_node) {
                Some(p) => p,
                None => {
                    let mut p = Path2D::new_float(1);
                    if let Some(ob) = outline_bounds(&clip_node) {
                        p.append(&Shape::Rect(ob).get_path_iterator(None), false);
                    }
                    p
                }
            };
            path.winding_rule = cu::convert_clip_rule(&child, self.ctx);
            let outline = Shape::Path(create_transformed_shape(&at, &Shape::Path(path)));
            let mut area = Area::from_path(&outline.get_path_iterator(None), outline.get_winding_rule());
            let mut outline_node = GraphicsNode::new(Kind::Shape { shape: Some(outline), painter: None });
            outline_node.visible = true;
            if let Some(c) = self.convert_clip_path(&child, &outline_node)? {
                area.subtract(&Area::from_path(&c.clip_path.get_path_iterator(None), c.clip_path.get_winding_rule()));
            }
            clip_path.add(&area);
        }
        if !has_children {
            return Ok(None);
        }
        let mut clip_path_node = GraphicsNode::new(Kind::Shape { shape: Some(area_shape(&clip_path)), painter: None });
        clip_path_node.visible = true;
        if let Some(c) = self.convert_clip_path(&cp, &clip_path_node)? {
            clip_path.subtract(&Area::from_path(&c.clip_path.get_path_iterator(None), c.clip_path.get_winding_rule()));
        }
        let hints: Option<RenderingHints> = cu::convert_shape_rendering(&cp, None, self.ctx);
        let use_aa = hints.as_ref().and_then(|h| h.get(KEY_ANTIALIASING)).is_some_and(|v| *v == "VALUE_ANTIALIAS_ON");
        let (x, y, w, h) = clip_path.get_bounds2d();
        Ok(Some(ClipRable { clip_path: area_shape(&clip_path), area_bounds: Some(Rectangle2D::double(x, y, w, h)), use_aa }))
    }

    fn create_shape_painter(&self, bridge: Bridge, e: &NodeRef, node: &GraphicsNode) -> BResult<Option<ShapePainter>> {
        let ctx = self.ctx;
        match bridge {
            Bridge::Rect | Bridge::Circle | Bridge::Ellipse => {
                if let Kind::Shape { shape: Some(s), .. } = &node.kind {
                    let r = s.get_bounds2d();
                    if r.w == 0.0 || r.h == 0.0 {
                        return Ok(None);
                    }
                }
                paint_server::convert_fill_and_stroke(e, node, ctx)
            }
            _ => {
                let fill_and_stroke = if bridge == Bridge::Line { paint_server::convert_stroke_painter(e, node, ctx)? } else { paint_server::convert_fill_and_stroke(e, node, ctx)? };
                let marker_painter = self.convert_markers(bridge, e, node)?;
                Ok(match marker_painter {
                    Some(mp) => match fill_and_stroke {
                        Some(fs) => Some(ShapePainter::Composite(vec![fs, mp])),
                        None => Some(mp),
                    },
                    None => fill_and_stroke,
                })
            }
        }
    }

    fn build_shape(&self, bridge: Bridge, e: &NodeRef) -> BResult<Option<Shape>> {
        let ctx = self.ctx;
        let uctx = create_context(ctx, e);
        let attr = |n: &str| e.get_attribute_ns(None, n);
        let coord = |n: &str, horizontal: bool| -> BResult<f32> {
            let s = attr(n);
            if s.is_empty() {
                return Ok(0.0);
            }
            if horizontal {
                up::svg_horizontal_coordinate_to_user_space(&s, n, &uctx)
            } else {
                up::svg_vertical_coordinate_to_user_space(&s, n, &uctx)
            }
        };
        let required = |n: &str, d: i16| -> BResult<f32> {
            let s = attr(n);
            if s.is_empty() {
                return Err(bridge_exception(e, ERR_ATTRIBUTE_MISSING, &[n, &s]));
            }
            up::svg_length_to_user_space(&s, n, d, &uctx)
        };
        Ok(match bridge {
            Bridge::Rect => {
                let x = coord("x", true)?;
                let y = coord("y", false)?;
                let w = required("width", up::HORIZONTAL_LENGTH)?;
                let h = required("height", up::VERTICAL_LENGTH)?;
                let s = attr("rx");
                let rxs = !s.is_empty();
                let mut rx = if rxs { up::svg_horizontal_length_to_user_space(&s, "rx", &uctx)? } else { 0.0 };
                rx = if rx > w / 2.0 { w / 2.0 } else { rx };
                let s = attr("ry");
                let rys = !s.is_empty();
                let mut ry = if rys { up::svg_vertical_length_to_user_space(&s, "ry", &uctx)? } else { 0.0 };
                ry = if ry > h / 2.0 { h / 2.0 } else { ry };
                let rect = Shape::Rect(Rectangle2D::float(x, y, w, h));
                let round = |aw: f32, ah: f32| Shape::RoundRect { x, y, w, h, aw, ah };
                Some(if rxs && rys {
                    if rx == 0.0 || ry == 0.0 {
                        rect
                    } else {
                        round(rx * 2.0, ry * 2.0)
                    }
                } else if rxs {
                    if rx == 0.0 {
                        rect
                    } else {
                        round(rx * 2.0, rx * 2.0)
                    }
                } else if rys {
                    if ry == 0.0 {
                        rect
                    } else {
                        round(ry * 2.0, ry * 2.0)
                    }
                } else {
                    rect
                })
            }
            Bridge::Circle => {
                let cx = coord("cx", true)?;
                let cy = coord("cy", false)?;
                let r = required("r", up::OTHER_LENGTH)?;
                let w = r * 2.0;
                Some(Shape::Ellipse { x: cx - r, y: cy - r, w, h: w })
            }
            Bridge::Ellipse => {
                let cx = coord("cx", true)?;
                let cy = coord("cy", false)?;
                let rx = required("rx", up::HORIZONTAL_LENGTH)?;
                let ry = required("ry", up::VERTICAL_LENGTH)?;
                Some(Shape::Ellipse { x: cx - rx, y: cy - ry, w: rx * 2.0, h: ry * 2.0 })
            }
            Bridge::Line => {
                let x1 = coord("x1", true)?;
                let y1 = coord("y1", false)?;
                let x2 = coord("x2", true)?;
                let y2 = coord("y2", false)?;
                Some(Shape::Line { x1, y1, x2, y2 })
            }
            Bridge::Polyline | Bridge::Polygon => {
                let s = attr("points");
                if s.is_empty() {
                    return Ok(None);
                }
                let (path, r) = parser::parse_points(&s, cu::convert_fill_rule(e, ctx), bridge == Bridge::Polygon);
                if r.is_err() {
                    return Err(bridge_exception(e, ERR_ATTRIBUTE_VALUE_MALFORMED, &["points"]));
                }
                Some(Shape::Path(path))
            }
            Bridge::Path => {
                let s = attr("d");
                if s.is_empty() {
                    return Ok(None);
                }
                let (path, r) = parser::parse_path(&s, cu::convert_fill_rule(e, ctx));
                if r.is_err() {
                    return Err(bridge_exception(e, ERR_ATTRIBUTE_VALUE_MALFORMED, &["d"]));
                }
                Some(Shape::Path(path.path))
            }
            _ => None,
        })
    }

    /// `SVGSVGElementBridge.createGraphicsNode`.
    fn create_svg(&self, e: &NodeRef) -> BResult<Option<GraphicsNode>> {
        let ctx = self.ctx;
        if !su::match_user_agent(e) {
            return Ok(None);
        }
        let mut cgn = GraphicsNode::new(Kind::Canvas { children: Vec::new(), position_transform: None, viewing_transform: None });
        let uctx = create_context(ctx, e);
        let doc = e.get_owner_document();
        let is_outermost = doc.get_document_element().is_some_and(|r| std::rc::Rc::ptr_eq(&r, e));
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        if !is_outermost {
            let s = e.get_attribute_ns(None, "x");
            if !s.is_empty() {
                x = up::svg_horizontal_coordinate_to_user_space(&s, "x", &uctx)?;
            }
            let s = e.get_attribute_ns(None, "y");
            if !s.is_empty() {
                y = up::svg_vertical_coordinate_to_user_space(&s, "y", &uctx)?;
            }
        }
        let mut s = e.get_attribute_ns(None, "width");
        if s.is_empty() {
            s = "100%".into();
        }
        let w = up::svg_horizontal_length_to_user_space(&s, "width", &uctx)?;
        let mut s = e.get_attribute_ns(None, "height");
        if s.is_empty() {
            s = "100%".into();
        }
        let h = up::svg_vertical_length_to_user_space(&s, "height", &uctx)?;
        cgn.visible = cu::convert_visibility(e, ctx);
        let viewing_transform = view_box::get_preserve_aspect_ratio_transform(e, w, h)?;
        let mut actual_width = w;
        let mut actual_height = h;
        if let Ok(inv) = viewing_transform.create_inverse() {
            actual_width = (w as f64 * inv.get_scale_x()) as f32;
            actual_height = (h as f64 * inv.get_scale_y()) as f32;
        }
        let position_transform = AffineTransform::get_translate_instance(x as f64, y as f64);
        if !is_outermost {
            cgn.set_position_transform(position_transform);
        } else if std::rc::Rc::ptr_eq(&doc, &ctx.document) {
            ctx.document_size.set(Some(((w + 0.5f32) as i32, (h + 0.5f32) as i32)));
        }
        cgn.set_viewing_transform(viewing_transform);
        if cu::convert_overflow(e, ctx) {
            let r = match cu::convert_clip(e, ctx) {
                None => Rectangle2D::float(x, y, w, h),
                Some(o) => Rectangle2D::float(x + o[3], y + o[0], w - o[1] - o[3], h - o[2] - o[0]),
            };
            let mut at = position_transform;
            at.concatenate(&viewing_transform);
            if let Ok(inv) = at.create_inverse() {
                let clip = create_transformed_shape(&inv, &Shape::Rect(r));
                cgn.clip = Some(ClipRable { clip_path: Shape::Path(clip), area_bounds: None, use_aa: false });
            }
        }
        cgn.hints = cu::convert_color_rendering(e, None, ctx);
        ctx.open_viewport(e, (actual_width, actual_height));
        Ok(Some(cgn))
    }

    /// `SVGUseElementBridge.createGraphicsNode` and `buildCompositeGraphicsNode`.
    fn create_use(&self, e: &NodeRef) -> BResult<Option<GraphicsNode>> {
        let ctx = self.ctx;
        if !su::match_user_agent(e) {
            return Ok(None);
        }
        let uri = su::get_xlink_href(e);
        if uri.is_empty() {
            return Err(bridge_exception(e, ERR_ATTRIBUTE_MISSING, &["xlink:href"]));
        }
        let ref_element = ctx.get_referenced_element(e, &uri)?;
        let document = e.get_owner_document();
        let mut local_ref = dom::import_node(&document, &ref_element);
        if local_ref.local_name() == Some("symbol") {
            let svg = dom::create_element_ns(&document, Some(SVG_NAMESPACE_URI), "svg").map_err(|d| BridgeError::Runtime(Some(d.0)))?;
            for a in local_ref.attributes_in_order().iter() {
                svg.set_attribute_ns(a.ns.as_deref(), &a.qname, &a.value);
            }
            let kids: Vec<NodeRef> = local_ref.children.borrow().clone();
            for k in kids {
                svg.append_child(k);
            }
            local_ref = svg;
        }
        if local_ref.local_name() == Some("svg") {
            let w = e.get_attribute_ns(None, "width");
            if !w.is_empty() {
                local_ref.set_attribute_ns(None, "width", &w);
            }
            let h = e.get_attribute_ns(None, "height");
            if !h.is_empty() {
                local_ref.set_attribute_ns(None, "height", &h);
            }
        }
        let root = dom::create_imported_root(&document, e);
        root.append_child(local_ref.clone());
        *e.elem().unwrap().imported_root.borrow_mut() = Some(root);
        let base = uri.split('#').next().unwrap_or("");
        if !base.is_empty() {
            local_ref.set_attribute_ns(Some(dom::XML_NAMESPACE_URI), "base", base);
        }
        let ref_engine = ref_element.get_css_engine().expect("CSS engine");
        CSSEngine::import_cascaded_style_maps(&ref_element, &ref_engine, &local_ref, ctx);
        let ref_node = self.build(&local_ref)?;
        let mut gn = GraphicsNode::composite();
        match ref_node {
            Some(n) => gn.children_mut().push(n),
            None => return Err(BridgeError::Runtime(Some("null is not a GraphicsNode".into()))),
        }
        let uctx = create_context(ctx, e);
        let mut x = 0.0f32;
        let s = e.get_attribute_ns(None, "x");
        if !s.is_empty() {
            x = up::svg_horizontal_coordinate_to_user_space(&s, "x", &uctx)?;
        }
        let mut y = 0.0f32;
        let s = e.get_attribute_ns(None, "y");
        if !s.is_empty() {
            y = up::svg_vertical_coordinate_to_user_space(&s, "y", &uctx)?;
        }
        let s = e.get_attribute_ns(None, "transform");
        let mut at = AffineTransform::get_translate_instance(x as f64, y as f64);
        if !s.is_empty() {
            at.pre_concatenate(&su::convert_transform(e, "transform", &s)?);
        }
        gn.transform = Some(at);
        gn.visible = cu::convert_visibility(e, ctx);
        gn.hints = cu::convert_color_rendering(e, None, ctx);
        Ok(Some(gn))
    }
}

/// Batik's `bridge/BrokenLink.svg` resource, which a missing image is drawn as.
const BROKEN_LINK_SVG: &str = r##"<svg viewBox="0 0 55.521 60.508" preserveAspectRatio="none"
  xmlns="http://www.w3.org/2000/svg"
  xmlns:xlink="http://www.w3.org/1999/xlink"  >
  <defs>
    <g id="__Broken_Link_Border" style="stroke-width:0.2">
      <path d="M21.927,58.088l4.999-6.834l-4.999-4.166l0.404-1.441l3.526-12.556l0.235-0.837l-2.489-3.268l-0.177-0.232l1.222-7.64c-0.656-0.432-1.607-0.455-2.575,0.15c-0.065,0.047-0.132,0.104-0.197,0.161c-0.379-0.118-0.806-0.078-1.253,0.162c0.737-0.991,1.604-2.324,2.37-3.884l-2.566-3.949l0.096-0.35l1.404-5.15l-1.151-6.167H1.927v56.333h19.798l0.202-0.333z"/>
      <path d="M53.593,2.087h-24.11l-0.39,8.833l1.749,4.664l0.251,0.669l-0.124,0.965c0.061,0.173,0.13,0.331,0.21,0.468c-0.104,0.174-0.226,0.33-0.334,0.5l-0.585,4.567l0.072,1.644l0.261,5.856l-0.384,3.934l-0.283,2.9l2.875,6.468l0.459,1.032l-3.334,5.833l4,7.833l0.08,0.167h19.587V2.087z"/>
    </g>
  </defs>
  <g style="fill-rule:nonzero; clip-rule:nonzero; stroke-miterlimit:4;">
    <g style="stroke:none;">
      <path style="stroke:black;fill:none;stroke-width:0.3"
        d="M55.371,60.358H0.15V0.15h55.221v60.208z"/>
      <use xlink:href="#__Broken_Link_Border" fill="white" fill-opacity="0.5"/>
      <path style="fill:#33CC33"
        d="M32.886,43.506c0.656-0.116,1.3-0.415,1.752-0.932c0.412-0.412,0.799-0.814,1.167-1.217c0.011,0.006,0.021,0.012,0.032,0.016c0.029-0.047,0.057-0.096,0.086-0.143c1.289-1.43,2.303-2.883,3.035-4.896c0.516-1.721-0.82-2.205-2.148-2.076c-0.212-1.113-1.281-1.869-2.893-1.283c-0.111,0.031-0.225,0.09-0.339,0.164c-0.809-0.396-2.029-0.135-2.78,0.314c-0.219,0.147-0.378,0.394-0.504,0.685l-0.282,2.9l2.874,6.468z"/>
      <path style="fill:#33CC33"
        d="M21.198,37.293c-0.479,0.961-0.72,2.881,0.961,2.881c0.004,0.002,0.008,0,0.013,0.002c-0.304,0.807-0.617,1.613-0.974,2.398c-1.019,1.891-0.093,2.895,1.219,3.022l3.527-12.556c-1.904,1.104-3.428,2.277-4.746,4.252z"/>
      <path style="fill:#6666FF"
        d="M24.734,21.065c-0.656-0.432-1.607-0.455-2.575,0.15c-0.065,0.047-0.132,0.104-0.197,0.161c-0.379-0.118-0.806-0.078-1.253,0.162c0.737-0.991,1.604-2.324,2.37-3.884l-2.566-3.949l0.096-0.35c-3.104,1.641-5.654,3.857-7.81,6.66c-0.72,0.96-1.92,2.88-0.96,4.319c0.619,1.034,2.126,0.464,2.993-0.327c-0.946,1.314-2.54,3.808-3.713,6.808c-0.96,2.4,0.479,4.799,2.88,3.119c1.376-0.951,2.7-1.955,3.942-3.023c0.134,0.188,0.406,0.115,0.857-0.336c0.221-0.221,0.44-0.445,0.661-0.668c0.672,0.652,1.843,0.77,2.938-0.053c0.061-0.09,0.136-0.168,0.203-0.252c0.35-0.127,0.725-0.373,1.088-0.665l-0.177-0.232l1.222-7.64z"/>
      <path style="fill:#FF0000"
        d="M31.054,17.17c0.061,0.173,0.13,0.331,0.21,0.468c-0.104,0.174-0.226,0.33-0.334,0.5l-0.585,4.567l0.073,1.644c0.423,0.282,0.97,0.352,1.579-0.015c0.227-0.129,0.439-0.273,0.658-0.412c0.451,0.664,1.158,1.001,1.982,0.412c0.153-0.092,0.306-0.176,0.459-0.266c-0.298,1.618-0.162,3.473,1.434,3.401c0.37,0.98,1.139,1.785,2.188,1.186c1.439-0.959,2.879-1.92,4.32-2.879c1.439-0.721,2.158-2.641,1.92-4.321c0-1.08-0.977-2.341-2.223-2.058c0.021-0.034,0.041-0.068,0.063-0.103c0.719-1.199,1.199-2.64,0.719-4.079c-0.24-0.96-1.439-1.921-2.398-1.2c-1.075,0.645-2.147,1.24-3.201,1.832c-0.16-0.339-0.371-0.642-0.618-0.871c0.073-0.16,0.144-0.32,0.218-0.481c0.48-1.2,0.721-6-1.92-4.56c-1.199,0.72-2.16,1.439-3.119,2.399c-0.829,0.829-1.417,2.001-1.551,3.201l0.251,0.669l-0.124,0.965z"/>
      <use xlink:href="#__Broken_Link_Border"
        fill="none" stroke="black"/>
    </g>
    <g id="__More_About">
      <circle style="fill:#FFFF66;fill-opacity:0.7;stroke:black;stroke-width:0.6;"
        cx="48.4" cy="53.25" r="3.5"/>
      <path style="fill:black;stroke:none;" d="M48.942,54.758c0,0.126,0,0.461,0.074,0.558c0.082,0.112,0.275,0.134,0.394,0.134v0.238c-0.297-0.007-0.602-0.022-0.914-0.029c-0.335,0.007-0.662,0.022-0.997,0.029v-0.238c0.119,0,0.313-0.022,0.395-0.134c0.074-0.097,0.074-0.431,0.074-0.558v-1.286c0-0.446-0.082-0.61-0.476-0.602v-0.246c0.483-0.037,0.959-0.104,1.45-0.223v2.356z M47.805,51.212c0-0.313,0.253-0.565,0.565-0.565c0.32,0,0.55,0.253,0.55,0.565c0,0.319-0.26,0.55-0.572,0.55c-0.283,0-0.543-0.268-0.543-0.55z"/>
    </g>
  </g>
</svg>
"##;

impl GvtBuilder<'_> {
    /// `SVGImageElementBridge.createGraphicsNode`: SVG images and broken links (a raster image is unsupported).
    fn create_image(&self, e: &NodeRef) -> BResult<Option<GraphicsNode>> {
        let ctx = self.ctx;
        let Some(mut image_node) = self.abstract_create(e, GraphicsNode::composite())? else { return Ok(None) };
        image_node.is_image = true;
        let uri = su::get_xlink_href(e);
        if uri.is_empty() {
            return Err(bridge_exception(e, ERR_ATTRIBUTE_MISSING, &["xlink:href"]));
        }
        if uri.contains('#') {
            return Err(bridge_exception(e, ERR_ATTRIBUTE_VALUE_MALFORMED, &["xlink:href", &uri]));
        }
        let base = dom::get_cascaded_xml_base(e);
        let purl = dom::resolve_url(base.as_deref(), &uri);
        let bounds = self.get_image_bounds(e)?;
        let node = if bounds.w == 0.0 || bounds.h == 0.0 {
            GraphicsNode::new(Kind::Shape { shape: Some(Shape::Rect(bounds)), painter: None })
        } else {
            let bytes = purl.strip_prefix("file:").and_then(|p| std::fs::read(super::bridge_context::file_path(p)).ok());
            match bytes {
                None if purl.starts_with("file:") => {
                    let doc = dom::create_svg_document(None, BROKEN_LINK_SVG.as_bytes().to_vec()).map_err(|_| BridgeError::Runtime(None))?;
                    self.create_svg_image_node(e, &doc)?
                }
                None => GraphicsNode::new(Kind::Unsupported("an image that is not a local file".into())),
                Some(b) => {
                    let raster = || GraphicsNode::new(Kind::Unsupported("a raster image".into()));
                    if b.starts_with(b"\x89PNG") || b.starts_with(&[0xff, 0xd8]) || b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
                        raster()
                    } else {
                        match dom::create_svg_document(Some(&purl), b) {
                            Ok(doc) => self.create_svg_image_node(e, &doc)?,
                            Err(_) => raster(),
                        }
                    }
                }
            }
        };
        image_node.children_mut().push(node);
        image_node.hints = cu::convert_color_rendering(e, None, ctx);
        Ok(Some(image_node))
    }

    /// `getImageBounds(ctx, element)`.
    fn get_image_bounds(&self, e: &NodeRef) -> BResult<Rectangle2D> {
        let uctx = create_context(self.ctx, e);
        let s = e.get_attribute_ns(None, "x");
        let x = if s.is_empty() { 0.0 } else { up::svg_horizontal_coordinate_to_user_space(&s, "x", &uctx)? };
        let s = e.get_attribute_ns(None, "y");
        let y = if s.is_empty() { 0.0 } else { up::svg_vertical_coordinate_to_user_space(&s, "y", &uctx)? };
        let s = e.get_attribute_ns(None, "width");
        if s.is_empty() {
            return Err(bridge_exception(e, ERR_ATTRIBUTE_MISSING, &["width"]));
        }
        let w = up::svg_horizontal_length_to_user_space(&s, "width", &uctx)?;
        let s = e.get_attribute_ns(None, "height");
        if s.is_empty() {
            return Err(bridge_exception(e, ERR_ATTRIBUTE_MISSING, &["height"]));
        }
        let h = up::svg_vertical_length_to_user_space(&s, "height", &uctx)?;
        Ok(Rectangle2D::float(x, y, w, h))
    }

    /// `createSVGImageNode(ctx, e, imgDocument)`, with `initializeViewport`.
    fn create_svg_image_node(&self, e: &NodeRef, img_document: &NodeRef) -> BResult<GraphicsNode> {
        let ctx = self.ctx;
        let sub = BridgeContext::new(img_document.clone());
        sub.initialize_document(img_document);
        let mut result = GraphicsNode::composite();
        let bounds = self.get_image_bounds(e)?;
        if bounds.w == 0.0 || bounds.h == 0.0 {
            result.children_mut().push(GraphicsNode::new(Kind::Shape { shape: Some(Shape::Rect(bounds)), painter: None }));
            return Ok(result);
        }
        let svg = img_document.get_document_element().unwrap();
        let mut node = (GvtBuilder { ctx: &sub }).build(&svg)?.ok_or_else(|| BridgeError::Runtime(Some("null is not a GraphicsNode".into())))?;
        node.clip = None;
        node.set_viewing_transform(AffineTransform::new());
        result.children_mut().push(node);
        let vb = view_box::parse_view_box_attribute(e, &svg.get_attribute_ns(None, "viewBox"))?;
        let (x, y, w, h) = (bounds.x as f32, bounds.y as f32, bounds.w as f32, bounds.h as f32);
        let aspect = e.get_attribute_ns(None, "preserveAspectRatio");
        let (align, meet) = parser::parse_preserve_aspect_ratio(&aspect).map_err(|_| bridge_exception(e, ERR_ATTRIBUTE_VALUE_MALFORMED, &["preserveAspectRatio", &aspect]))?;
        let mut at = view_box::transform_for(vb, align, meet, w, h);
        at.pre_concatenate(&AffineTransform::get_translate_instance(x as f64, y as f64));
        result.transform = Some(at);
        if cu::convert_overflow(e, ctx) {
            let r = match cu::convert_clip(e, ctx) {
                None => Rectangle2D::float(x, y, w, h),
                Some(o) => Rectangle2D::float(x + o[3], y + o[0], w - o[1] - o[3], h - o[2] - o[0]),
            };
            if let Ok(inv) = at.create_inverse() {
                result.clip = Some(ClipRable { clip_path: Shape::Path(create_transformed_shape(&inv, &Shape::Rect(r))), area_bounds: None, use_aa: false });
            }
        }
        Ok(result)
    }
}

impl GvtBuilder<'_> {
    /// `PaintServer.convertMarkers(e, node, ctx)`.
    fn convert_markers(&self, bridge: Bridge, e: &NodeRef, node: &GraphicsNode) -> BResult<Option<ShapePainter>> {
        let mut markers = Vec::new();
        for idx in [MARKER_START_INDEX, MARKER_MID_INDEX, MARKER_END_INDEX] {
            let v = cu::get_computed_style(e, idx, self.ctx);
            if v.primitive_type() == CSS_IDENT {
                markers.push(None);
                continue;
            }
            let uri = v.string_value().to_string();
            let m = self.ctx.get_referenced_element(e, &uri)?;
            if !m.is_svg("marker") {
                return Err(bridge_exception(e, ERR_CSS_URI_BAD_TARGET, &[&uri]));
            }
            markers.push(self.create_marker(&m, e)?);
        }
        if markers.iter().all(Option::is_none) {
            return Ok(None);
        }
        let Kind::Shape { shape: Some(shape), .. } = &node.kind else { return Err(BridgeError::Runtime(None)) };
        let segments = if bridge == Bridge::Path {
            let (p, _) = parser::parse_path(&e.get_attribute_ns(None, "d"), cu::convert_fill_rule(e, self.ctx));
            let mut segs = Vec::new();
            let mut vi = 0;
            for &t in &p.types {
                let n = match t {
                    SEG_CLOSE => 0,
                    SEG_MOVETO | SEG_LINETO => 2,
                    SEG_QUADTO => 4,
                    SEG_ARCTO => 7,
                    _ => 6,
                };
                let mut c = [0f64; 7];
                for (k, v) in p.values[vi..vi + n].iter().enumerate() {
                    c[k] = *v as f64;
                }
                vi += n;
                segs.push((t, c));
            }
            segs
        } else {
            shape.get_path_iterator(None).into_iter().map(|(t, c)| (t, [c[0], c[1], c[2], c[3], c[4], c[5], 0.0])).collect()
        };
        let mut it = markers.into_iter();
        let (start, middle, end) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
        Ok(Some(ShapePainter::Marker { segments, start, middle, end }))
    }

    /// `SVGMarkerElementBridge.createMarker(ctx, markerElement, paintedElement)`.
    fn create_marker(&self, marker: &NodeRef, painted: &NodeRef) -> BResult<Option<Marker>> {
        let ctx = self.ctx;
        let mut content = GraphicsNode::composite();
        let mut has_children = false;
        for child in marker.child_elements() {
            if let Some(n) = self.build(&child)? {
                has_children = true;
                content.children_mut().push(n);
            }
        }
        if !has_children {
            return Ok(None);
        }
        let uctx = create_context(ctx, painted);
        let s = marker.get_attribute_ns(None, "markerWidth");
        let marker_width = if s.is_empty() { 3.0 } else { up::svg_horizontal_length_to_user_space(&s, "markerWidth", &uctx)? };
        if marker_width == 0.0 {
            return Ok(None);
        }
        let s = marker.get_attribute_ns(None, "markerHeight");
        let marker_height = if s.is_empty() { 3.0 } else { up::svg_vertical_length_to_user_space(&s, "markerHeight", &uctx)? };
        if marker_height == 0.0 {
            return Ok(None);
        }
        let s = marker.get_attribute_ns(None, "orient");
        let orient = if s.is_empty() {
            0.0
        } else if s == "auto" {
            f64::NAN
        } else {
            parser::java_parse_float(&s).ok_or_else(|| bridge_exception(marker, ERR_ATTRIBUTE_VALUE_MALFORMED, &["orient", &s]))? as f64
        };
        let stroke_width = cu::get_computed_style(painted, STROKE_WIDTH_INDEX, ctx).float_value();
        let s = marker.get_attribute_ns(None, "markerUnits");
        let units = if s.is_empty() {
            su::STROKE_WIDTH
        } else {
            match s.as_str() {
                "userSpaceOnUse" => USER_SPACE_ON_USE,
                "strokeWidth" => su::STROKE_WIDTH,
                _ => return Err(bridge_exception(marker, ERR_ATTRIBUTE_VALUE_MALFORMED, &["markerUnits", &s])),
            }
        };
        let mut marker_txf = AffineTransform::new();
        if units == su::STROKE_WIDTH {
            marker_txf.scale(stroke_width as f64, stroke_width as f64);
        }
        marker_txf.concatenate(&view_box::get_preserve_aspect_ratio_transform(marker, marker_width, marker_height)?);
        content.transform = Some(marker_txf);
        if cu::convert_overflow(marker, ctx) {
            let r = match cu::convert_clip(marker, ctx) {
                None => Rectangle2D::float(0.0, 0.0, stroke_width * marker_width, stroke_width * marker_height),
                Some(o) => Rectangle2D::float(o[3], o[0], stroke_width * marker_width - o[1] - o[3], stroke_width * marker_height - o[2] - o[0]),
            };
            let mut comp = GraphicsNode::composite();
            comp.children_mut().push(content);
            comp.clip = Some(ClipRable { clip_path: Shape::Rect(r), area_bounds: None, use_aa: false });
            content = comp;
        }
        let s = marker.get_attribute_ns(None, "refX");
        let ref_x = if s.is_empty() { 0.0 } else { up::svg_horizontal_coordinate_to_user_space(&s, "refX", &uctx)? };
        let s = marker.get_attribute_ns(None, "refY");
        let ref_y = if s.is_empty() { 0.0 } else { up::svg_vertical_coordinate_to_user_space(&s, "refY", &uctx)? };
        let (rx, ry) = marker_txf.transform_point(ref_x as f64, ref_y as f64);
        Ok(Some(Marker { node: std::rc::Rc::new(content), ref_: (rx as f32 as f64, ry as f32 as f64), orient }))
    }
}

fn is_composite(b: Bridge) -> bool {
    matches!(b, Bridge::Svg | Bridge::G | Bridge::A)
}

/// The bounds of `getOutline()`: a shape's geometry, or a group's children outlines in its space.
fn outline_bounds(n: &GraphicsNode) -> Option<Rectangle2D> {
    match &n.kind {
        Kind::Shape { shape, .. } => shape.as_ref().map(|s| s.get_bounds2d()),
        Kind::Proxy(src) => outline_bounds(src),
        Kind::Unsupported(_) => n.get_geometry_bounds(),
        Kind::Composite(ch) | Kind::Canvas { children: ch, .. } => {
            let mut out: Option<Rectangle2D> = None;
            for c in ch {
                if let Some(b) = outline_bounds(c) {
                    let b = match &c.transform {
                        Some(t) => create_transformed_shape(t, &Shape::Rect(b)).get_bounds2d(),
                        None => b,
                    };
                    out = Some(match out {
                        None => b,
                        Some(o) => o.create_union(&b),
                    });
                }
            }
            out
        }
    }
}

/// `new GeneralPath(node.getOutline())`: a shape's outline, or a composite's children's outlines appended through
/// their transforms; `None` where the outline is not a plain shape (text, images).
fn outline_path(n: &GraphicsNode) -> Option<Path2D> {
    match &n.kind {
        Kind::Shape { shape, .. } => {
            let s = shape.as_ref()?;
            let mut p = Path2D::new_float(s.get_winding_rule());
            p.append(&s.get_path_iterator(None), false);
            Some(p)
        }
        Kind::Composite(ch) | Kind::Canvas { children: ch, .. } => {
            let mut p = Path2D::new_float(1);
            for c in ch {
                let o = outline_path(c)?;
                let segs = match &c.transform {
                    Some(t) => Shape::Path(create_transformed_shape(t, &Shape::Path(o))).get_path_iterator(None),
                    None => Shape::Path(o).get_path_iterator(None),
                };
                p.append(&segs, false);
            }
            Some(p)
        }
        _ => None,
    }
}

/// An `Area` as the shape its path iterator draws.
fn area_shape(a: &Area) -> Shape {
    let mut p = Path2D::new_double(1);
    p.append(&a.get_path_iterator(None), false);
    Shape::Path(p)
}
