//! Ports of Batik's `gvt` package as static SVG painting uses it: `AbstractGraphicsNode` (paint, bounds,
//! `normalizeRectangle`), `CompositeGraphicsNode`, `CanvasGraphicsNode`, `RootGraphicsNode`, `ShapeNode`, and the
//! `FillShapePainter`/`StrokeShapePainter`/`CompositeShapePainter` shape painters.

use std::rc::Rc;

use crate::asc::flex2::batik::ext::awt::geom::{compute_arc, SEG_ARCTO};
use crate::asc::flex2::flash::graphics::g2d::graphic_context::RenderingHints;
use crate::asc::flex2::flash::graphics::g2d::sprite_graphics2d::SpriteGraphics2D;
use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::AffineTransform;
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::{create_transformed_shape, Rectangle2D, Shape, SEG_CLOSE, SEG_CUBICTO, SEG_LINETO, SEG_MOVETO, SEG_QUADTO};
use crate::asc::flex2::fonts::java2d::java::awt::paint::{AlphaComposite, BasicStroke, Paint};

const EPSILON: f64 = 1e-6;

/// `ShapePainter`.
#[derive(Clone, Debug)]
pub enum ShapePainter {
    Fill { paint: Option<Paint> },
    Stroke { stroke: Option<BasicStroke>, paint: Option<Paint> },
    Composite(Vec<ShapePainter>),
    /// `MarkerShapePainter` over the shape's `ExtendedPathIterator` segments.
    Marker { segments: Vec<(i32, [f64; 7])>, start: Option<Marker>, middle: Option<Marker>, end: Option<Marker> },
}

/// `gvt.Marker`.
#[derive(Clone, Debug)]
pub struct Marker {
    pub node: Rc<GraphicsNode>,
    pub ref_: (f64, f64),
    pub orient: f64,
}

impl ShapePainter {
    fn paint(&self, shape: &Shape, g2d: &mut SpriteGraphics2D) -> Result<(), Unsupported> {
        match self {
            ShapePainter::Fill { paint } => {
                if paint.is_some() {
                    g2d.graphic_context.set_paint(paint.clone());
                    g2d.fill(shape);
                }
            }
            ShapePainter::Stroke { stroke, paint } => {
                if let (Some(s), Some(_)) = (stroke, paint) {
                    g2d.graphic_context.set_paint(paint.clone());
                    g2d.graphic_context.set_stroke(s.clone());
                    g2d.draw(shape);
                }
            }
            ShapePainter::Composite(ps) => {
                for p in ps {
                    p.paint(shape, g2d)?;
                }
            }
            ShapePainter::Marker { .. } => {
                let group = self.build_marker_group();
                if !group.children_ref().is_empty() {
                    group.paint(g2d)?;
                }
            }
        }
        Ok(())
    }

    /// `MarkerShapePainter.buildMarkerGroup()`.
    fn build_marker_group(&self) -> GraphicsNode {
        let ShapePainter::Marker { segments, start, middle, end } = self else { unreachable!() };
        let mut group = GraphicsNode::composite();
        if let Some(m) = start {
            if let Some(p) = build_start_marker_proxy(segments, m) {
                group.children_mut().push(p);
            }
        }
        if let Some(m) = middle {
            for p in build_middle_marker_proxies(segments, m) {
                group.children_mut().push(p);
            }
        }
        if let Some(m) = end {
            if let Some(p) = build_end_marker_proxy(segments, m) {
                group.children_mut().push(p);
            }
        }
        group
    }

    /// `getPaintedBounds2D()`; a stroke's outline (`createStrokedShape`) is taken as the shape grown by half its width.
    fn get_painted_bounds2d(&self, shape: &Shape) -> Option<Rectangle2D> {
        match self {
            ShapePainter::Fill { paint } => paint.as_ref().map(|_| shape.get_bounds2d()),
            ShapePainter::Stroke { stroke, paint } => {
                let (s, _) = (stroke.as_ref()?, paint.as_ref()?);
                let b = shape.get_bounds2d();
                let hw = s.width as f64 / 2.0;
                Some(Rectangle2D::float((b.x - hw) as f32, (b.y - hw) as f32, (b.w + 2.0 * hw) as f32, (b.h + 2.0 * hw) as f32))
            }
            ShapePainter::Marker { .. } => self.build_marker_group().get_primitive_bounds(),
            ShapePainter::Composite(ps) => {
                let mut bounds: Option<Rectangle2D> = None;
                for p in ps {
                    let Some(pb) = p.get_painted_bounds2d(shape) else { continue };
                    bounds = Some(match bounds {
                        None => pb,
                        Some(b) => b.create_union(&pb),
                    });
                }
                bounds
            }
        }
    }
}

/// `ClipRable8Bit`: the clip path and whether it asks for an antialiased clip.
#[derive(Clone, Debug)]
pub struct ClipRable {
    pub clip_path: Shape,
    pub use_aa: bool,
}

#[derive(Clone, Debug)]
pub enum Kind {
    Composite(Vec<GraphicsNode>),
    /// `CanvasGraphicsNode`'s position and viewing transforms.
    Canvas { children: Vec<GraphicsNode>, position_transform: Option<AffineTransform>, viewing_transform: Option<AffineTransform> },
    Shape { shape: Option<Shape>, painter: Option<ShapePainter> },
    /// `ProxyGraphicsNode`: paints its source.
    Proxy(Rc<GraphicsNode>),
    /// A node this port does not paint (text, a raster image): painting it records why.
    Unsupported(String),
}

#[derive(Clone, Debug)]
pub struct GraphicsNode {
    pub kind: Kind,
    pub transform: Option<AffineTransform>,
    pub composite: Option<AlphaComposite>,
    pub clip: Option<ClipRable>,
    /// A filter: painting then goes through an offscreen raster, which this port does not render.
    pub filter: bool,
    /// A mask: its luminance conversion fails on JDK 17 (`CMMException`), as it does for mxmlc.
    pub mask: bool,
    pub hints: Option<RenderingHints>,
    pub visible: bool,
    /// `ImageNode`: a composite that honours its visibility.
    pub is_image: bool,
}

/// A runtime exception painting threw: its message.
#[derive(Debug, Clone)]
pub struct Unsupported(pub String);

/// The first thing painting met that this port does not render; painting goes on past it, as a later exception
/// would still end the transcoding the same way mxmlc's does.
fn record_unsupported(g2d: &mut SpriteGraphics2D, what: &str) {
    if g2d.unsupported.is_none() {
        g2d.unsupported = Some(what.to_string());
    }
}

impl GraphicsNode {
    pub fn new(kind: Kind) -> GraphicsNode {
        GraphicsNode { kind, transform: None, composite: None, clip: None, filter: false, mask: false, hints: None, visible: true, is_image: false }
    }

    pub fn composite() -> GraphicsNode {
        GraphicsNode::new(Kind::Composite(Vec::new()))
    }

    pub fn children_mut(&mut self) -> &mut Vec<GraphicsNode> {
        match &mut self.kind {
            Kind::Composite(c) | Kind::Canvas { children: c, .. } => c,
            _ => panic!("ClassCastException: not a CompositeGraphicsNode"),
        }
    }

    pub fn children_ref(&self) -> &[GraphicsNode] {
        match &self.kind {
            Kind::Composite(c) | Kind::Canvas { children: c, .. } => c,
            _ => &[],
        }
    }

    pub fn is_composite(&self) -> bool {
        matches!(self.kind, Kind::Composite(_) | Kind::Canvas { .. })
    }

    fn update_canvas_transform(&mut self) {
        if let Kind::Canvas { position_transform, viewing_transform, .. } = &self.kind {
            let t = match (position_transform, viewing_transform) {
                (Some(p), v) => {
                    let mut t = *p;
                    if let Some(v) = v {
                        t.concatenate(v);
                    }
                    t
                }
                (None, Some(v)) => *v,
                (None, None) => AffineTransform::new(),
            };
            self.transform = Some(t);
        }
    }

    pub fn set_position_transform(&mut self, at: AffineTransform) {
        if let Kind::Canvas { position_transform, .. } = &mut self.kind {
            *position_transform = Some(at);
        }
        self.update_canvas_transform();
    }

    pub fn set_viewing_transform(&mut self, at: AffineTransform) {
        if let Kind::Canvas { viewing_transform, .. } = &mut self.kind {
            *viewing_transform = Some(at);
        }
        self.update_canvas_transform();
    }

    /// `getPrimitiveBounds()`.
    pub fn get_primitive_bounds(&self) -> Option<Rectangle2D> {
        match &self.kind {
            Kind::Shape { shape, painter } => {
                if !self.visible {
                    return None;
                }
                let shape = shape.as_ref()?;
                match painter {
                    None => Some(shape.get_bounds2d()),
                    Some(p) => p.get_painted_bounds2d(shape),
                }
            }
            Kind::Proxy(src) => src.get_transformed_bounds(&AffineTransform::new()),
            Kind::Unsupported(_) => Some(Rectangle2D::double(-1e9, -1e9, 2e9, 2e9)),
            Kind::Composite(ch) | Kind::Canvas { children: ch, .. } => {
                if self.is_image && !self.visible {
                    return None;
                }
                let mut it = ch.iter();
                let mut bounds = None;
                for c in it.by_ref() {
                    bounds = c.get_transformed_bounds(&AffineTransform::new());
                    if bounds.is_some() {
                        break;
                    }
                }
                let mut bounds = bounds?;
                for c in it {
                    if let Some(ctb) = c.get_transformed_bounds(&AffineTransform::new()) {
                        bounds = bounds.create_union(&ctb);
                    }
                }
                Some(bounds)
            }
        }
    }

    /// `getGeometryBounds()`.
    pub fn get_geometry_bounds(&self) -> Option<Rectangle2D> {
        match &self.kind {
            Kind::Shape { shape, .. } => shape.as_ref().map(|s| self.normalize_rectangle(s.get_bounds2d())),
            Kind::Proxy(src) => src.get_transformed_geometry_bounds(&AffineTransform::new()),
            Kind::Unsupported(_) => Some(Rectangle2D::double(-1e9, -1e9, 2e9, 2e9)),
            Kind::Composite(ch) | Kind::Canvas { children: ch, .. } => {
                let mut it = ch.iter();
                let mut gb = None;
                for c in it.by_ref() {
                    gb = c.get_transformed_geometry_bounds(&AffineTransform::new());
                    if gb.is_some() {
                        break;
                    }
                }
                let mut gb = gb?;
                for c in it {
                    if let Some(cgb) = c.get_transformed_geometry_bounds(&AffineTransform::new()) {
                        gb = gb.create_union(&cgb);
                    }
                }
                Some(gb)
            }
        }
    }

    fn concat_transform(&self, txf: &AffineTransform) -> AffineTransform {
        let mut t = *txf;
        if let Some(tr) = &self.transform {
            t.concatenate(tr);
        }
        t
    }

    fn get_transformed_geometry_bounds(&self, txf: &AffineTransform) -> Option<Rectangle2D> {
        let t = self.concat_transform(txf);
        match &self.kind {
            Kind::Shape { .. } | Kind::Proxy(_) | Kind::Unsupported(_) => {
                let b = self.get_geometry_bounds()?;
                Some(create_transformed_shape(&t, &Shape::Rect(b)).get_bounds2d())
            }
            Kind::Composite(ch) | Kind::Canvas { children: ch, .. } => {
                if t.get_shear_x() == 0.0 && t.get_shear_y() == 0.0 {
                    return get_transformed_bbox(self.get_geometry_bounds(), &t);
                }
                let mut gb: Option<Rectangle2D> = None;
                for c in ch {
                    if let Some(cgb) = c.get_transformed_geometry_bounds(&t) {
                        gb = Some(match gb {
                            None => cgb,
                            Some(g) => g.create_union(&cgb),
                        });
                    }
                }
                gb
            }
        }
    }

    fn get_transformed_primitive_bounds(&self, txf: &AffineTransform) -> Option<Rectangle2D> {
        let t = self.concat_transform(txf);
        match &self.kind {
            Kind::Shape { .. } | Kind::Proxy(_) | Kind::Unsupported(_) => {
                let b = self.get_primitive_bounds()?;
                Some(create_transformed_shape(&t, &Shape::Rect(b)).get_bounds2d())
            }
            Kind::Composite(ch) | Kind::Canvas { children: ch, .. } => {
                if t.get_shear_x() == 0.0 && t.get_shear_y() == 0.0 {
                    return get_transformed_bbox(self.get_primitive_bounds(), &t);
                }
                let mut tpb: Option<Rectangle2D> = None;
                for c in ch {
                    if let Some(ctb) = c.get_transformed_bounds(&t) {
                        tpb = Some(match tpb {
                            None => ctb,
                            Some(g) => g.create_union(&ctb),
                        });
                    }
                }
                tpb
            }
        }
    }

    /// `getTransformedBounds(txf)`.
    pub fn get_transformed_bounds(&self, txf: &AffineTransform) -> Option<Rectangle2D> {
        let t = self.concat_transform(txf);
        let mut tb = self.get_transformed_primitive_bounds(txf)?;
        if let Some(c) = &self.clip {
            let cb = create_transformed_shape(&t, &c.clip_path).get_bounds2d();
            tb = intersect(&tb, &cb);
        }
        Some(tb)
    }

    /// `getBounds()`.
    pub fn get_bounds(&self) -> Option<Rectangle2D> {
        let mut b = self.get_primitive_bounds()?;
        if let Some(c) = &self.clip {
            let cr = c.clip_path.get_bounds2d();
            if cr.intersects(b.x, b.y, b.w, b.h) {
                b = intersect(&b, &cr);
            }
        }
        Some(self.normalize_rectangle(b))
    }

    fn normalize_rectangle(&self, b: Rectangle2D) -> Rectangle2D {
        if b.w < EPSILON {
            if b.h < EPSILON {
                return Rectangle2D::double(b.x, b.y, EPSILON, EPSILON);
            }
            let tmp_w = (b.h * EPSILON).max(b.w);
            return Rectangle2D::double(b.x, b.y, tmp_w, b.h);
        } else if b.h < EPSILON {
            let tmp_h = (b.w * EPSILON).max(b.h);
            return Rectangle2D::double(b.x, b.y, b.w, tmp_h);
        }
        b
    }

    /// `paint(g2d)` (`ShapeNode.paint` adds the visibility test).
    pub fn paint(&self, g2d: &mut SpriteGraphics2D) -> Result<(), Unsupported> {
        if (matches!(self.kind, Kind::Shape { .. }) || self.is_image) && !self.visible {
            return Ok(());
        }
        if let Some(c) = &self.composite {
            if c.alpha < 0.001 {
                return Ok(());
            }
        }
        let Some(bounds) = self.get_bounds() else { return Ok(()) };
        let default_clip = g2d.graphic_context.get_clip();
        let default_composite = g2d.graphic_context.get_composite();
        let default_transform = g2d.graphic_context.get_transform();
        let mut default_hints = None;
        if let Some(h) = &self.hints {
            default_hints = Some(g2d.graphic_context.get_rendering_hints());
            g2d.graphic_context.add_rendering_hints(h);
        }
        if let Some(t) = &self.transform {
            g2d.graphic_context.transform(t);
        }
        if let Some(c) = &self.composite {
            g2d.graphic_context.set_composite(*c);
        }
        if let Some(c) = &self.clip {
            g2d.graphic_context.clip(Some(&c.clip_path));
        }
        let mut paint_needed = true;
        if let Some(cb) = g2d.graphic_context.get_clip_bounds2d() {
            if !bounds.intersects(cb.x, cb.y, cb.w, cb.h) {
                paint_needed = false;
            }
        }
        let mut result = Ok(());
        if paint_needed {
            let antialiased_clip = self.clip.as_ref().is_some_and(|c| c.use_aa && !(matches!(c.clip_path, Shape::Rect(_)) && g2d.graphic_context.get_transform().get_shear_x() == 0.0 && g2d.graphic_context.get_transform().get_shear_y() == 0.0));
            let offscreen = self.filter || self.mask || self.composite.is_some_and(|c| c != AlphaComposite::SRC_OVER) || antialiased_clip;
            if !offscreen {
                result = self.primitive_paint(g2d);
            } else if self.mask && !self.filter {
                result = Err(Unsupported("Invalid image layout".into()));
            } else {
                record_unsupported(g2d, "an offscreen (rasterized) node: opacity, a filter or an antialiased clip");
            }
        }
        if let Some(h) = default_hints {
            g2d.graphic_context.set_rendering_hints(h);
        }
        g2d.graphic_context.set_transform(&default_transform);
        g2d.graphic_context.set_clip(default_clip.as_ref());
        if self.composite.is_some() {
            g2d.graphic_context.set_composite(default_composite);
        }
        result
    }

    fn primitive_paint(&self, g2d: &mut SpriteGraphics2D) -> Result<(), Unsupported> {
        match &self.kind {
            Kind::Shape { shape, painter } => {
                if let (Some(s), Some(p)) = (shape, painter) {
                    p.paint(s, g2d)?;
                }
                Ok(())
            }
            Kind::Proxy(src) => src.paint(g2d),
            Kind::Unsupported(what) => {
                record_unsupported(g2d, what);
                Ok(())
            }
            Kind::Composite(ch) | Kind::Canvas { children: ch, .. } => {
                for c in ch {
                    c.paint(g2d)?;
                }
                Ok(())
            }
        }
    }
}

/// `Rectangle2D.intersect(src1, src2, dest)` with `dest` of `src1`'s class.
fn intersect(a: &Rectangle2D, b: &Rectangle2D) -> Rectangle2D {
    a.create_intersection(b)
}

/// `CompositeGraphicsNode.getTransformedBBox(r2d, t)`.
fn get_transformed_bbox(r: Option<Rectangle2D>, t: &AffineTransform) -> Option<Rectangle2D> {
    let r = r?;
    let (mut x, w, mut y, h) = (r.x, r.w, r.y, r.h);
    let mut sx = t.get_scale_x();
    let mut sy = t.get_scale_y();
    if sx < 0.0 {
        x = -(x + w);
        sx = -sx;
    }
    if sy < 0.0 {
        y = -(y + h);
        sy = -sy;
    }
    Some(Rectangle2D::float((x * sx + t.get_translate_x()) as f32, (y * sy + t.get_translate_y()) as f32, (w * sx) as f32, (h * sy) as f32))
}

fn terminating_point(c: &[f64; 7], t: i32) -> (f64, f64) {
    match t {
        SEG_CUBICTO => (c[4], c[5]),
        SEG_LINETO | SEG_MOVETO => (c[0], c[1]),
        SEG_QUADTO => (c[2], c[3]),
        SEG_ARCTO => (c[5], c[6]),
        _ => panic!("Error"),
    }
}

fn normalize(dx: f64, dy: f64) -> [f64; 2] {
    let n = (dx * dx + dy * dy).sqrt();
    [dx / n, dy / n]
}

/// The arc tangent `computeInSlope`/`computeOutSlope` take at an arc's end (`at_end`) or start.
fn arc_slope(from: (f64, f64), c: &[f64; 7], at_end: bool) -> (f64, f64) {
    let large = c[3] != 0.0;
    let go_left = c[4] != 0.0;
    let Shape::Arc { w, h, start, extent, .. } = compute_arc(from.0, from.1, c[0], c[1], c[2], large, go_left, c[5], c[6]) else { unreachable!() };
    let theta = (if at_end { start + extent } else { start }) * 0.017453292519943295;
    let mut dx = -w / 2.0 * theta.sin();
    let mut dy = h / 2.0 * theta.cos();
    if c[2] != 0.0 {
        let ang = -c[2] * 0.017453292519943295;
        let (sin_a, cos_a) = (ang.sin(), ang.cos());
        let tdx = dx * cos_a - dy * sin_a;
        let tdy = dx * sin_a + dy * cos_a;
        dx = tdx;
        dy = tdy;
    }
    if go_left {
        dx = -dx;
    } else {
        dy = -dy;
    }
    (dx, dy)
}

fn compute_in_slope(prev: Option<(&[f64; 7], i32)>, curr: &[f64; 7], ct: i32) -> Option<[f64; 2]> {
    let ce = terminating_point(curr, ct);
    let (dx, dy) = match ct {
        SEG_LINETO => {
            let (p, pt) = prev.unwrap();
            let pe = terminating_point(p, pt);
            (ce.0 - pe.0, ce.1 - pe.1)
        }
        SEG_QUADTO => (ce.0 - curr[0], ce.1 - curr[1]),
        SEG_CUBICTO => (ce.0 - curr[2], ce.1 - curr[3]),
        SEG_ARCTO => {
            let (p, pt) = prev.unwrap();
            arc_slope(terminating_point(p, pt), curr, true)
        }
        SEG_CLOSE => panic!("Error"),
        _ => return None,
    };
    if dx == 0.0 && dy == 0.0 {
        return None;
    }
    Some(normalize(dx, dy))
}

fn compute_out_slope(curr: &[f64; 7], ct: i32, next: Option<(&[f64; 7], i32)>) -> Option<[f64; 2]> {
    let ce = terminating_point(curr, ct);
    let (n, nt) = next?;
    let (dx, dy) = match nt {
        SEG_CLOSE => (0.0, 0.0),
        SEG_CUBICTO | SEG_LINETO | SEG_QUADTO => (n[0] - ce.0, n[1] - ce.1),
        SEG_ARCTO => arc_slope(ce, n, false),
        _ => return None,
    };
    if dx == 0.0 && dy == 0.0 {
        return None;
    }
    Some(normalize(dx, dy))
}

fn compute_rotation(prev: Option<(&[f64; 7], i32)>, curr: &[f64; 7], ct: i32, next: Option<(&[f64; 7], i32)>) -> f64 {
    let mut in_slope = compute_in_slope(prev, curr, ct);
    let mut out_slope = compute_out_slope(curr, ct, next);
    if in_slope.is_none() {
        in_slope = out_slope;
    }
    if out_slope.is_none() {
        out_slope = in_slope;
    }
    let (Some(i), Some(o)) = (in_slope, out_slope) else { return 0.0 };
    let dx = i[0] + o[0];
    let dy = i[1] + o[1];
    if dx == 0.0 && dy == 0.0 {
        i[1].atan2(i[0]) * 180.0 / std::f64::consts::PI + 90.0
    } else {
        dy.atan2(dx) * 180.0 / std::f64::consts::PI
    }
}

fn compute_marker_transform(m: &Marker, pos: (f64, f64), rotation: f64) -> AffineTransform {
    let mut txf = AffineTransform::new();
    txf.translate(pos.0 - m.ref_.0, pos.1 - m.ref_.1);
    if !rotation.is_nan() {
        txf.rotate_at(rotation * std::f64::consts::PI / 180.0, m.ref_.0, m.ref_.1);
    }
    txf
}

fn proxy(m: &Marker, t: AffineTransform) -> GraphicsNode {
    let mut gn = GraphicsNode::new(Kind::Proxy(m.node.clone()));
    gn.transform = Some(t);
    gn
}

fn build_start_marker_proxy(segs: &[(i32, [f64; 7])], m: &Marker) -> Option<GraphicsNode> {
    let (t, coords) = *segs.first()?;
    if t != SEG_MOVETO {
        return None;
    }
    let pos = (coords[0], coords[1]);
    let mut rotation = m.orient;
    if rotation.is_nan() {
        if let Some(&(mut nt, mut next)) = segs.get(1) {
            if nt == SEG_CLOSE {
                nt = SEG_LINETO;
                next[0] = coords[0];
                next[1] = coords[1];
            }
            rotation = compute_rotation(None, &coords, t, Some((&next, nt)));
        }
    }
    Some(proxy(m, compute_marker_transform(m, pos, rotation)))
}

fn build_end_marker_proxy(segs: &[(i32, [f64; 7])], m: &Marker) -> Option<GraphicsNode> {
    let (t, coords) = *segs.first()?;
    if t != SEG_MOVETO {
        return None;
    }
    let mut n_points = 1;
    let mut move_to = [coords[0], coords[1]];
    let mut last_but_one = [0f64; 7];
    let mut last = coords;
    let mut last_type = t;
    let mut last_but_one_type = 0;
    for &(st, c) in &segs[1..] {
        last_but_one = last;
        last_but_one_type = last_type;
        last = c;
        last_type = st;
        if st == SEG_MOVETO {
            move_to = [c[0], c[1]];
        } else if st == SEG_CLOSE {
            last_type = SEG_LINETO;
            last[0] = move_to[0];
            last[1] = move_to[1];
        }
        n_points += 1;
    }
    if n_points < 2 {
        return None;
    }
    let pos = terminating_point(&last, last_type);
    let mut rotation = m.orient;
    if rotation.is_nan() {
        rotation = compute_rotation(Some((&last_but_one, last_but_one_type)), &last, last_type, None);
    }
    Some(proxy(m, compute_marker_transform(m, pos, rotation)))
}

fn build_middle_marker_proxies(segs: &[(i32, [f64; 7])], m: &Marker) -> Vec<GraphicsNode> {
    let mut out = Vec::new();
    let Some(&(mut prev_type, mut prev)) = segs.first() else { return out };
    if prev_type != SEG_MOVETO {
        return out;
    }
    let mut move_to = [prev[0], prev[1]];
    let Some(&(mut curr_type, mut curr)) = segs.get(1) else { return out };
    if curr_type == SEG_MOVETO {
        move_to = [curr[0], curr[1]];
    } else if curr_type == SEG_CLOSE {
        curr_type = SEG_LINETO;
        curr[0] = move_to[0];
        curr[1] = move_to[1];
    }
    for &(nt, n) in &segs[2..] {
        let (mut next_type, mut next) = (nt, n);
        if next_type == SEG_MOVETO {
            move_to = [next[0], next[1]];
        } else if next_type == SEG_CLOSE {
            next_type = SEG_LINETO;
            next[0] = move_to[0];
            next[1] = move_to[1];
        }
        let pos = terminating_point(&curr, curr_type);
        let mut rotation = m.orient;
        if rotation.is_nan() {
            rotation = compute_rotation(Some((&prev, prev_type)), &curr, curr_type, Some((&next, next_type)));
        }
        out.push(proxy(m, compute_marker_transform(m, pos, rotation)));
        prev = curr;
        prev_type = curr_type;
        curr = next;
        curr_type = next_type;
    }
    out
}
