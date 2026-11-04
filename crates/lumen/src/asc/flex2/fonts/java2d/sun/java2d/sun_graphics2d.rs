//! Port of JDK 17 `sun.java2d.SunGraphics2D` drawing into a `BufferedImage.TYPE_INT_ARGB` (`BufImgSurfaceData`,
//! IntArgb), with the pipes `SurfaceData.validatePipe` installs when antialiasing is off: `LoopPipe`,
//! `PixelToParallelogramConverter` (`colorViaPgram`), `SpanShapeRenderer.Composite` over `AlphaPaintPipe`
//! (`paintShape`) and `DrawImage`'s blits.

use super::loops::draw_parallelogram::draw_parallelogram;
use super::loops::fill_parallelogram::{fill_parallelogram, FillTarget};
use super::loops::int_argb::{extra_alpha_to_int, int_argb_to_int_argb_srcover_maskblit, int_rgb_to_int_argb_convert};
use super::loops::process_path::{draw_path, fill_path, DrawHandler, SetPixelHandler, PH_STROKE_DEFAULT, PH_STROKE_PURE};
use super::marlin::{d_marlin_rendering_engine::get_determinant, stroke_to};
use super::pipe::shape_span_iterator::ShapeSpanIterator;
use crate::asc::flex2::batik::ext::awt::multiple_gradient_paint_context::{GradientHints, GradientPaintContext, GradientPaintRef, HintValue};
use crate::asc::flex2::flash::graphics::g2d::graphic_context::{RenderingHints, KEY_ANTIALIASING, KEY_COLOR_RENDERING, KEY_RENDERING, KEY_STROKE_CONTROL};
use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::AffineTransform;
use crate::asc::flex2::fonts::java2d::java::awt::geom::area::Area;
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::{create_transformed_shape, Path2D, Rectangle2D, Shape};
use crate::asc::flex2::fonts::java2d::java::awt::paint::{AlphaComposite, BasicStroke, Color, Paint, JOIN_MITER, CAP_ROUND};

pub const PAINT_OPAQUECOLOR: i32 = 0;
pub const PAINT_ALPHACOLOR: i32 = 1;
pub const PAINT_CUSTOM: i32 = 6;
pub const COMP_ISCOPY: i32 = 0;
pub const COMP_ALPHA: i32 = 1;
pub const STROKE_THIN: i32 = 0;
pub const STROKE_THINDASHED: i32 = 1;
pub const STROKE_WIDE: i32 = 2;
pub const TRANSFORM_ISIDENT: i32 = 0;
pub const TRANSFORM_INT_TRANSLATE: i32 = 1;
pub const TRANSFORM_ANY_TRANSLATE: i32 = 2;
pub const TRANSFORM_TRANSLATESCALE: i32 = 3;
pub const CLIP_DEVICE: i32 = 0;
pub const CLIP_RECTANGULAR: i32 = 1;
pub const CLIP_SHAPE: i32 = 2;
const STROKE_HINT_PURE: i32 = 2;
const NON_RECTILINEAR_TRANSFORM_MASK: i32 = 48;
const TILE_SIZE: i32 = 32;

/// A `BufferedImage` of packed int pixels: `TYPE_INT_ARGB` (sRGB, not premultiplied) or, when `opaque`,
/// `TYPE_INT_RGB`.
#[derive(Clone, Debug, PartialEq)]
pub struct IntImage {
    pub width: i32,
    pub height: i32,
    pub data: Vec<i32>,
    pub opaque: bool,
}

impl IntImage {
    pub fn new(width: i32, height: i32) -> IntImage {
        IntImage { width, height, data: vec![0; (width.max(0) as usize) * (height.max(0) as usize)], opaque: false }
    }
}

/// `usrClip`: a `java.awt.Rectangle`, a `Rectangle2D.Float`/`Double`, an `Area`, or another device-space shape.
#[derive(Clone, Debug)]
pub enum UsrClip {
    Rect(i32, i32, i32, i32),
    Rect2D(Rectangle2D),
    Shape(Shape),
    Area(Area),
}

impl UsrClip {
    /// The clip as a `java.awt.Shape` (an `Area` as the path its iterator gives).
    pub fn to_shape(&self) -> Shape {
        match self {
            UsrClip::Rect(x, y, w, h) => Shape::Rect(Rectangle2D::double(*x as f64, *y as f64, *w as f64, *h as f64)),
            UsrClip::Rect2D(r) => Shape::Rect(*r),
            UsrClip::Shape(s) => s.clone(),
            UsrClip::Area(a) => {
                let mut p = Path2D::new_double(1);
                p.append(&a.get_path_iterator(None), false);
                Shape::Path(p)
            }
        }
    }

    fn from_shape(s: &Shape) -> UsrClip {
        match s {
            Shape::Rect(r) => UsrClip::Rect2D(*r),
            s => UsrClip::Shape(s.clone()),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ImageComp {
    SrcOverNoEa,
    SrcOver,
    SrcNoEa,
    Clear,
}

/// The paint context `AlphaPaintPipe.startSequence` makes: a `ColorPaintContext` or one of Batik's gradients.
enum PaintCtx {
    Color(i32),
    Gradient(GradientPaintContext),
}

pub struct SunGraphics2D {
    pub surface: IntImage,
    transform: AffineTransform,
    transform_state: i32,
    trans_x: i32,
    trans_y: i32,
    paint: Paint,
    paint_state: i32,
    pixel: i32,
    composite: AlphaComposite,
    image_comp: ImageComp,
    composite_state: i32,
    stroke: BasicStroke,
    stroke_is_default: bool,
    stroke_state: i32,
    stroke_hint: i32,
    antialias: bool,
    usr_clip: Option<UsrClip>,
    clip_state: i32,
    /// `clipRegion`'s bounds; a non-rectangular region is not rendered by this port.
    clip_region: [i32; 4],
    /// A `CLIP_SHAPE` region's bands: `(loy, hiy, spans)`.
    clip_bands: Vec<(i32, i32, Vec<(i32, i32)>)>,
    hints: RenderingHints,
    /// What was met that this port does not render.
    pub unsupported: Option<String>,
    /// A runtime exception painting threw (a gradient's), as Java would propagate it.
    pub exception: Option<String>,
}

/// `Region.dimAdd(start, dim)`.
fn dim_add(start: i32, dim: i32) -> i32 {
    if dim <= 0 {
        return start;
    }
    let r = start.wrapping_add(dim);
    if r < start {
        i32::MAX
    } else {
        r
    }
}

/// `Region.clipRound(v)`.
fn clip_round(v: f64) -> i32 {
    let x = v - 0.5;
    if x < -2147483648.0 {
        return i32::MIN;
    }
    if x > 2147483647.0 {
        return i32::MAX;
    }
    x.ceil() as i32
}

/// `AffineTransform.transform(double[] srcPts, 0, dstPts, 0, n)`.
fn transform_pts(at: &AffineTransform, pts: &mut [f64]) {
    at.transform_coords(pts);
}

impl SunGraphics2D {
    /// `bi.createGraphics()`: white paint, `SrcOver`, the default stroke, no clip.
    pub fn new(surface: IntImage) -> SunGraphics2D {
        let (w, h) = (surface.width, surface.height);
        SunGraphics2D {
            surface,
            transform: AffineTransform::new(),
            transform_state: TRANSFORM_ISIDENT,
            trans_x: 0,
            trans_y: 0,
            paint: Paint::Color(Color { value: -1 }),
            paint_state: PAINT_OPAQUECOLOR,
            pixel: -1,
            composite: AlphaComposite::SRC_OVER,
            image_comp: ImageComp::SrcOverNoEa,
            composite_state: COMP_ISCOPY,
            stroke: BasicStroke::default(),
            stroke_is_default: true,
            stroke_state: STROKE_THIN,
            stroke_hint: 0,
            antialias: false,
            usr_clip: None,
            clip_state: CLIP_DEVICE,
            clip_region: [0, 0, w, h],
            clip_bands: Vec::new(),
            hints: RenderingHints::new(),
            unsupported: None,
            exception: None,
        }
    }

    fn record_unsupported(&mut self, what: &str) {
        if self.unsupported.is_none() {
            self.unsupported = Some(what.to_string());
        }
    }

    // --- rendering hints ---

    /// `setRenderingHint(key, value)`.
    pub fn set_rendering_hint(&mut self, k: &'static str, v: &'static str) {
        if k == KEY_ANTIALIASING {
            self.antialias = v == "VALUE_ANTIALIAS_ON";
        } else if k == KEY_STROKE_CONTROL {
            self.stroke_hint = match v {
                "VALUE_STROKE_NORMALIZE" => 1,
                "VALUE_STROKE_PURE" => 2,
                _ => 0,
            };
        }
        self.hints.insert(k, v);
    }

    pub fn add_rendering_hints(&mut self, h: &RenderingHints) {
        for (k, v) in h {
            self.set_rendering_hint(k, v);
        }
    }

    /// `setRenderingHints(map)`: the defaults, then `map`.
    pub fn set_rendering_hints(&mut self, h: &RenderingHints) {
        self.hints = RenderingHints::new();
        self.antialias = false;
        self.stroke_hint = 0;
        self.add_rendering_hints(h);
    }

    pub fn get_rendering_hints(&self) -> RenderingHints {
        self.hints.clone()
    }

    pub fn get_rendering_hint(&self, k: &str) -> Option<&'static str> {
        self.hints.get(k).copied()
    }

    // --- transform ---

    pub fn get_transform(&self) -> AffineTransform {
        self.transform
    }

    pub fn set_transform(&mut self, at: &AffineTransform) {
        self.transform = *at;
        self.invalidate_transform();
    }

    pub fn transform(&mut self, at: &AffineTransform) {
        self.transform.concatenate(at);
        self.invalidate_transform();
    }

    pub fn translate(&mut self, tx: f64, ty: f64) {
        self.transform.translate(tx, ty);
        self.invalidate_transform();
    }

    /// `invalidateTransform()`.
    fn invalidate_transform(&mut self) {
        let t = self.transform.get_type();
        if t == 0 {
            self.transform_state = TRANSFORM_ISIDENT;
            self.trans_x = 0;
            self.trans_y = 0;
        } else if t == 1 {
            let dtx = self.transform.get_translate_x();
            let dty = self.transform.get_translate_y();
            self.trans_x = (dtx + 0.5).floor() as i32;
            self.trans_y = (dty + 0.5).floor() as i32;
            self.transform_state = if dtx == self.trans_x as f64 && dty == self.trans_y as f64 { TRANSFORM_INT_TRANSLATE } else { TRANSFORM_ANY_TRANSLATE };
        } else if t & 120 == 0 {
            self.transform_state = TRANSFORM_TRANSLATESCALE;
            self.trans_x = 0;
            self.trans_y = 0;
        } else {
            self.transform_state = 4;
            self.trans_x = 0;
            self.trans_y = 0;
        }
        self.validate_basic_stroke();
    }

    // --- paint, composite, stroke ---

    pub fn get_composite(&self) -> AlphaComposite {
        self.composite
    }

    /// `setComposite(AlphaComposite)` for `SrcOver` with an alpha.
    pub fn set_composite(&mut self, c: AlphaComposite) {
        let comp = if c.alpha == 1.0 { ImageComp::SrcOverNoEa } else { ImageComp::SrcOver };
        self.set_comp(c, comp);
    }

    fn set_comp(&mut self, c: AlphaComposite, comp: ImageComp) {
        let state = match comp {
            ImageComp::SrcOverNoEa => {
                if self.paint_state == PAINT_OPAQUECOLOR || (self.paint_state > PAINT_ALPHACOLOR && self.paint_is_opaque()) {
                    COMP_ISCOPY
                } else {
                    COMP_ALPHA
                }
            }
            ImageComp::SrcNoEa | ImageComp::Clear => COMP_ISCOPY,
            ImageComp::SrcOver => COMP_ALPHA,
        };
        self.composite_state = state;
        self.image_comp = comp;
        self.composite = c;
        if self.paint_state <= PAINT_ALPHACOLOR {
            self.validate_color();
        }
    }

    /// `Paint.getTransparency()` of a non-colour paint.
    fn paint_is_opaque(&self) -> bool {
        match &self.paint {
            Paint::Color(c) => c.get_alpha() == 255,
            Paint::LinearGradient(g) => g.base.colors.iter().all(|c| c.get_alpha() == 255),
            Paint::RadialGradient(g) => g.base.colors.iter().all(|c| c.get_alpha() == 255),
            Paint::Pattern => false,
        }
    }

    /// `validateColor()`.
    fn validate_color(&mut self) {
        let Paint::Color(fg) = self.paint else { return };
        let mut eargb = fg.value;
        if self.image_comp == ImageComp::Clear {
            eargb = 0;
        } else if self.composite_state <= COMP_ALPHA && self.image_comp != ImageComp::SrcNoEa && self.image_comp != ImageComp::SrcOverNoEa {
            let a = java_round_f(self.composite.alpha * ((eargb as u32 >> 24) as f32));
            eargb = (eargb & 0x00ffffff) | (a << 24);
        }
        self.pixel = eargb;
    }

    /// `setPaint(paint)` (`setColor` for a colour).
    pub fn set_paint(&mut self, p: Option<Paint>) {
        let Some(p) = p else { return };
        if let Paint::Color(c) = p {
            self.paint = Paint::Color(c);
            self.validate_color();
            if (self.pixel >> 24) == -1 {
                self.paint_state = PAINT_OPAQUECOLOR;
                if self.image_comp == ImageComp::SrcOverNoEa {
                    self.composite_state = COMP_ISCOPY;
                }
            } else {
                self.paint_state = PAINT_ALPHACOLOR;
                if self.image_comp == ImageComp::SrcOverNoEa {
                    self.composite_state = COMP_ALPHA;
                }
            }
            return;
        }
        self.paint = p;
        if self.image_comp == ImageComp::SrcOverNoEa {
            self.composite_state = if self.paint_is_opaque() { COMP_ISCOPY } else { COMP_ALPHA };
        }
        self.paint_state = PAINT_CUSTOM;
    }

    pub fn set_stroke(&mut self, s: BasicStroke) {
        self.stroke = s;
        self.stroke_is_default = false;
        self.validate_basic_stroke();
    }

    /// `validateBasicStroke(bs)`.
    fn validate_basic_stroke(&mut self) {
        let bs = &self.stroke;
        let dashed = bs.dash.is_some();
        if self.transform_state < TRANSFORM_TRANSLATESCALE {
            if self.antialias {
                self.stroke_state = if bs.width as f64 <= MIN_PEN_SIZE_AA { if dashed { STROKE_THINDASHED } else { STROKE_THIN } } else { STROKE_WIDE };
            } else if self.stroke_is_default {
                self.stroke_state = STROKE_THIN;
            } else if bs.width <= 1.0 {
                self.stroke_state = if dashed { STROKE_THINDASHED } else { STROKE_THIN };
            } else {
                self.stroke_state = STROKE_WIDE;
            }
            return;
        }
        let t = &self.transform;
        let mut widthsquared = if t.get_type() & 36 == 0 {
            get_determinant(t).abs()
        } else {
            let a = t.get_scale_x();
            let c = t.get_shear_x();
            let b = t.get_shear_y();
            let d = t.get_scale_y();
            let ea = a * a + b * b;
            let eb = 2.0 * (a * c + b * d);
            let ec = c * c + d * d;
            let hypot = (eb * eb + (ea - ec) * (ea - ec)).sqrt();
            (ea + ec + hypot) / 2.0
        };
        if !self.stroke_is_default {
            widthsquared *= (bs.width * bs.width) as f64;
        }
        let min = if self.antialias { MIN_PEN_SIZE_AA * MIN_PEN_SIZE_AA } else { 1.000000001 };
        self.stroke_state = if widthsquared <= min { if dashed { STROKE_THINDASHED } else { STROKE_THIN } } else { STROKE_WIDE };
    }

    // --- clip ---

    /// `transformShape(s)` into device space.
    fn transform_shape(&self, s: &UsrClip) -> UsrClip {
        if self.transform_state > TRANSFORM_INT_TRANSLATE {
            transform_shape_at(&self.transform, s)
        } else {
            transform_shape_int(self.trans_x, self.trans_y, s)
        }
    }

    /// `clip(s)`.
    pub fn clip(&mut self, s: &Shape) {
        let mut s = self.transform_shape(&UsrClip::from_shape(s));
        if let Some(u) = &self.usr_clip {
            s = match intersect_shapes(u, &s) {
                Some(s) => s,
                None => {
                    self.record_unsupported("a clip intersection of curved shapes (java.awt.geom.Area)");
                    s
                }
            };
        }
        self.usr_clip = Some(s);
        self.validate_comp_clip();
    }

    /// `clip(new Rectangle(x, y, w, h))`.
    pub fn clip_int_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let mut s = self.transform_shape(&UsrClip::Rect(x, y, w, h));
        if let Some(u) = &self.usr_clip {
            s = intersect_shapes(u, &s).unwrap_or(s);
        }
        self.usr_clip = Some(s);
        self.validate_comp_clip();
    }

    /// `setClip(s)`.
    pub fn set_clip(&mut self, s: Option<&Shape>) {
        self.usr_clip = s.map(|s| self.transform_shape(&UsrClip::from_shape(s)));
        self.validate_comp_clip();
    }

    /// `setClip(s)` with a clip `getClip` returned.
    pub fn set_clip_usr(&mut self, s: Option<&UsrClip>) {
        self.usr_clip = s.map(|s| self.transform_shape(s));
        self.validate_comp_clip();
    }

    /// `getClip()`: `untransformShape(usrClip)`.
    pub fn get_clip(&self) -> Option<UsrClip> {
        let u = self.usr_clip.as_ref()?;
        if self.transform_state > TRANSFORM_INT_TRANSLATE {
            let inv = self.transform.create_inverse().ok()?;
            Some(transform_shape_at(&inv, u))
        } else {
            Some(transform_shape_int(-self.trans_x, -self.trans_y, u))
        }
    }

    /// `validateCompClip()`.
    fn validate_comp_clip(&mut self) {
        let (w, h) = (self.surface.width, self.surface.height);
        match &self.usr_clip {
            None => {
                self.clip_state = CLIP_DEVICE;
                self.clip_region = [0, 0, w, h];
            }
            Some(UsrClip::Rect(x, y, cw, ch)) => {
                self.clip_state = CLIP_RECTANGULAR;
                self.clip_region = intersect_xyxy([0, 0, w, h], *x, *y, dim_add(*x, *cw), dim_add(*y, *ch));
            }
            Some(UsrClip::Rect2D(r)) => {
                self.clip_state = CLIP_RECTANGULAR;
                let (a, b, c, d) = (r.get_min_x(), r.get_min_y(), r.get_max_x(), r.get_max_y());
                self.clip_region = if a.is_nan() || b.is_nan() || c.is_nan() || d.is_nan() { [0, 0, 0, 0] } else { intersect_xyxy([0, 0, w, h], clip_round(a), clip_round(b), clip_round(c), clip_round(d)) };
            }
            Some(u @ (UsrClip::Shape(_) | UsrClip::Area(_))) => {
                let s = u.to_shape();
                let mut sr = ShapeSpanIterator::new(self.stroke_hint != STROKE_HINT_PURE);
                sr.set_output_area_xyxy(0, 0, w, h);
                sr.append_path(&s.get_path_iterator(None), s.get_winding_rule());
                let b = sr.get_path_box();
                let mut sp = [0i32; 4];
                let mut rows: Vec<[i32; 4]> = Vec::new();
                while sr.next_span(&mut sp) {
                    rows.push(sp);
                }
                let bands = region_bands(&b, &rows);
                let bbox = bands_bbox(&bands);
                if bands.len() <= 1 && bands.first().map_or(true, |b| b.2.len() == 1) {
                    self.clip_state = CLIP_RECTANGULAR;
                    self.clip_region = bbox;
                    self.clip_bands.clear();
                } else {
                    self.clip_state = CLIP_SHAPE;
                    self.clip_region = bbox;
                    self.clip_bands = bands;
                }
            }
        }
    }

    fn clip_ok(&mut self) -> bool {
        if self.antialias {
            self.record_unsupported("antialiased rendering in an offscreen raster");
            return false;
        }
        true
    }

    fn color_pipe(&self) -> bool {
        self.paint_state <= PAINT_ALPHACOLOR && self.composite_state <= COMP_ISCOPY && self.clip_state != CLIP_SHAPE
    }

    // --- primitives ---

    /// `fillRect(x, y, w, h)` with `AlphaComposite.Clear` and an identity transform: `LoopPipe.fillRect`.
    pub fn clear_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let c = self.composite;
        let ic = self.image_comp;
        self.set_comp(AlphaComposite::SRC_OVER, ImageComp::Clear);
        if w > 0 && h > 0 {
            let b = intersect_xyxy(self.clip_region, x + self.trans_x, y + self.trans_y, dim_add(x + self.trans_x, w), dim_add(y + self.trans_y, h));
            let pixel = self.pixel;
            self.set_rect(b, pixel);
        }
        if ic == ImageComp::Clear {
            self.set_comp(c, ImageComp::Clear);
        } else {
            self.set_composite(c);
        }
    }

    fn set_rect(&mut self, b: [i32; 4], pixel: i32) {
        let w = self.surface.width as usize;
        for y in b[1]..b[3] {
            for x in b[0]..b[2] {
                self.surface.data[y as usize * w + x as usize] = pixel;
            }
        }
    }

    /// `fill(s)`: `shapepipe.fill(this, s)`.
    pub fn fill(&mut self, s: &Shape) {
        if !self.clip_ok() {
            return;
        }
        if self.color_pipe() {
            if let Shape::Rect(r) = s {
                if r.w > 0.0 && r.h > 0.0 {
                    self.fill_rectangle(r.x, r.y, r.w, r.h);
                }
                return;
            }
            self.loop_fill(s);
        } else {
            self.span_fill(s);
        }
    }

    /// `draw(s)`: `shapepipe.draw(this, s)`.
    pub fn draw(&mut self, s: &Shape) {
        if !self.clip_ok() {
            return;
        }
        if !self.color_pipe() {
            self.span_draw(s);
            return;
        }
        match s {
            Shape::Rect(r) => {
                if self.stroke.join != JOIN_MITER || self.stroke.dash.is_some() {
                    self.loop_draw(s);
                } else if r.w >= 0.0 && r.h >= 0.0 {
                    let lw = self.stroke.width as f64;
                    self.draw_rectangle(r.x, r.y, r.w, r.h, lw);
                }
            }
            Shape::Line { x1, y1, x2, y2 } => {
                if !self.draw_general_line(*x1 as f64, *y1 as f64, *x2 as f64, *y2 as f64) {
                    self.loop_draw(s);
                }
            }
            _ => self.loop_draw(s),
        }
    }

    fn clip_target_bounds(&self) -> [i32; 4] {
        intersect_xyxy(self.clip_region, 0, 0, self.surface.width, self.surface.height)
    }

    fn set_pixel_handler(&mut self) -> SetPixelHandler<'_> {
        let b = self.clip_target_bounds();
        let scan = self.surface.width as usize;
        let pixel = self.pixel;
        SetPixelHandler { pixels: &mut self.surface.data, scan, pixel, lox: b[0], loy: b[1], hix: b[2], hiy: b[3] }
    }

    fn fill_target(&mut self) -> FillTarget<'_> {
        let b = self.clip_target_bounds();
        let scan = self.surface.width as usize;
        let pixel = self.pixel;
        FillTarget { raster: &mut self.surface.data, scan, clip_lox: b[0], clip_loy: b[1], clip_hix: b[2], clip_hiy: b[3], pixel }
    }

    /// `new Path2D.Float(s)` (or `(s, transform)`) with the translation `LoopPipe` passes.
    fn loop_path(&self, s: &Shape) -> (Vec<f32>, Vec<u8>, i32, i32) {
        let (segs, tx, ty) = if self.transform_state <= TRANSFORM_INT_TRANSLATE { (s.get_path_iterator(None), self.trans_x, self.trans_y) } else { (s.get_path_iterator(Some(&self.transform)), 0, 0) };
        let mut coords = Vec::new();
        let mut types = Vec::new();
        for (t, c) in &segs {
            types.push(*t as u8);
            let n = match *t {
                0 | 1 => 2,
                2 => 4,
                3 => 6,
                _ => 0,
            };
            coords.extend(c[..n].iter().map(|v| *v as f32));
        }
        (coords, types, tx, ty)
    }

    fn ph_stroke(&self) -> i32 {
        if self.stroke_hint == STROKE_HINT_PURE {
            PH_STROKE_PURE
        } else {
            PH_STROKE_DEFAULT
        }
    }

    /// `LoopPipe.fill`.
    fn loop_fill(&mut self, s: &Shape) {
        if self.stroke_state == STROKE_THIN {
            let (coords, types, tx, ty) = self.loop_path(s);
            let (rule, stroke) = (s.get_winding_rule(), self.ph_stroke());
            let mut h = self.set_pixel_handler();
            fill_path(&mut h, tx, ty, &coords, &types, rule, stroke);
            return;
        }
        let mut sr = ShapeSpanIterator::new(self.stroke_hint != STROKE_HINT_PURE);
        let c = self.clip_region;
        sr.set_output_area_xyxy(c[0], c[1], c[2], c[3]);
        let at = if self.transform_state == TRANSFORM_ISIDENT { None } else { Some(&self.transform) };
        sr.append_path(&s.get_path_iterator(at), s.get_winding_rule());
        self.fill_spans(&mut sr);
    }

    /// `LoopPipe.draw`.
    fn loop_draw(&mut self, s: &Shape) {
        if self.stroke_state == STROKE_THIN {
            let (coords, types, tx, ty) = self.loop_path(s);
            let stroke = self.ph_stroke();
            let mut h = self.set_pixel_handler();
            draw_path(&mut h, tx, ty, &coords, &types, stroke);
            return;
        }
        let mut sr = self.get_stroke_spans(s);
        self.fill_spans(&mut sr);
    }

    /// `LoopPipe.getStrokeSpans`.
    fn get_stroke_spans(&self, s: &Shape) -> ShapeSpanIterator {
        let mut sr = ShapeSpanIterator::new(false);
        let c = self.clip_region;
        sr.set_output_area_xyxy(c[0], c[1], c[2], c[3]);
        sr.set_rule(1);
        let bs = &self.stroke;
        let thin = self.stroke_state <= STROKE_THINDASHED;
        let normalize = self.stroke_hint != STROKE_HINT_PURE;
        stroke_to(&|t| s.get_path_iterator(t), &self.transform, Some(c), bs.width, bs.cap, bs.join, bs.miterlimit, bs.dash.as_deref(), bs.dash_phase, thin, normalize, false, &mut sr);
        sr
    }

    /// `LoopPipe.fillSpans` with a `FillSpans` loop.
    fn fill_spans(&mut self, sr: &mut ShapeSpanIterator) {
        let mut sp = [0i32; 4];
        let pixel = self.pixel;
        while sr.next_span(&mut sp) {
            self.set_rect(intersect_xyxy([0, 0, self.surface.width, self.surface.height], sp[0], sp[1], sp[2], sp[3]), pixel);
        }
    }

    /// `PixelToParallelogramConverter.normalize(v)` with `normPosition` 0.25.
    fn normalize(v: f64) -> f64 {
        (v + 0.25).floor() + 0.25
    }

    /// `PixelToParallelogramConverter.fillRectangle`.
    fn fill_rectangle(&mut self, rx: f64, ry: f64, rw: f64, rh: f64) {
        let t = &self.transform;
        let mut dx1 = t.get_scale_x();
        let mut dy1 = t.get_shear_y();
        let mut dx2 = t.get_shear_x();
        let mut dy2 = t.get_scale_y();
        let mut px = rx * dx1 + ry * dx2 + t.get_translate_x();
        let mut py = rx * dy1 + ry * dy2 + t.get_translate_y();
        dx1 *= rw;
        dy1 *= rw;
        dx2 *= rh;
        dy2 *= rh;
        if self.stroke_hint != STROKE_HINT_PURE {
            let newx = Self::normalize(px);
            let newy = Self::normalize(py);
            dx1 = Self::normalize(px + dx1) - newx;
            dy1 = Self::normalize(py + dy1) - newy;
            dx2 = Self::normalize(px + dx2) - newx;
            dy2 = Self::normalize(py + dy2) - newy;
            px = newx;
            py = newy;
        }
        fill_parallelogram(&mut self.fill_target(), px, py, dx1, dy1, dx2, dy2);
    }

    /// `PixelToParallelogramConverter.drawRectangle`.
    fn draw_rectangle(&mut self, rx: f64, ry: f64, rw: f64, rh: f64, lw: f64) {
        let t = &self.transform;
        let mut dx1 = t.get_scale_x();
        let mut dy1 = t.get_shear_y();
        let mut dx2 = t.get_shear_x();
        let mut dy2 = t.get_scale_y();
        let mut px = rx * dx1 + ry * dx2 + t.get_translate_x();
        let mut py = rx * dy1 + ry * dy2 + t.get_translate_y();
        let mut lw1 = len(dx1, dy1) * lw;
        let mut lw2 = len(dx2, dy2) * lw;
        dx1 *= rw;
        dy1 *= rw;
        dx2 *= rh;
        dy2 *= rh;
        if self.stroke_hint != STROKE_HINT_PURE {
            let newx = Self::normalize(px);
            let newy = Self::normalize(py);
            dx1 = Self::normalize(px + dx1) - newx;
            dy1 = Self::normalize(py + dy1) - newy;
            dx2 = Self::normalize(px + dx2) - newx;
            dy2 = Self::normalize(py + dy2) - newy;
            px = newx;
            py = newy;
        }
        lw1 = lw1.max(1.0);
        lw2 = lw2.max(1.0);
        let len1 = len(dx1, dy1);
        let len2 = len(dx2, dy2);
        if lw1 >= len1 || lw2 >= len2 {
            self.fill_outer_parallelogram(px, py, dx1, dy1, dx2, dy2, len1, len2, lw1, lw2);
        } else {
            draw_parallelogram(&mut self.fill_target(), px, py, dx1, dy1, dx2, dy2, lw1 / len1, lw2 / len2);
        }
    }

    /// `PixelToParallelogramConverter.fillOuterParallelogram`.
    #[allow(clippy::too_many_arguments)]
    fn fill_outer_parallelogram(&mut self, mut px: f64, mut py: f64, mut dx1: f64, mut dy1: f64, mut dx2: f64, mut dy2: f64, len1: f64, len2: f64, lw1: f64, lw2: f64) {
        let mut udx1 = dx1 / len1;
        let mut udy1 = dy1 / len1;
        let mut udx2 = dx2 / len2;
        let mut udy2 = dy2 / len2;
        if len1 == 0.0 {
            if len2 == 0.0 {
                udx2 = 0.0;
                udy2 = 1.0;
            }
            udx1 = udy2;
            udy1 = -udx2;
        } else if len2 == 0.0 {
            udx2 = udy1;
            udy2 = -udx1;
        }
        udx1 *= lw1;
        udy1 *= lw1;
        udx2 *= lw2;
        udy2 *= lw2;
        px -= (udx1 + udx2) / 2.0;
        py -= (udy1 + udy2) / 2.0;
        dx1 += udx1;
        dy1 += udy1;
        dx2 += udx2;
        dy2 += udy2;
        fill_parallelogram(&mut self.fill_target(), px, py, dx1, dy1, dx2, dy2);
    }

    /// `PixelToParallelogramConverter.drawGeneralLine`.
    fn draw_general_line(&mut self, ux1: f64, uy1: f64, ux2: f64, uy2: f64) -> bool {
        if self.stroke_state == STROKE_THINDASHED {
            return false;
        }
        let cap = self.stroke.cap;
        if cap == CAP_ROUND || self.stroke.dash.is_some() {
            return false;
        }
        let mut lw = self.stroke.width as f64;
        let mut dx = ux2 - ux1;
        let mut dy = uy2 - uy1;
        let (mut x1, mut y1, mut x2, mut y2);
        match self.transform_state {
            3 | 4 => {
                let mut c = [ux1, uy1, ux2, uy2];
                transform_pts(&self.transform, &mut c);
                (x1, y1, x2, y2) = (c[0], c[1], c[2], c[3]);
            }
            1 | 2 => {
                let tx = self.transform.get_translate_x();
                let ty = self.transform.get_translate_y();
                (x1, y1, x2, y2) = (ux1 + tx, uy1 + ty, ux2 + tx, uy2 + ty);
            }
            _ => (x1, y1, x2, y2) = (ux1, uy1, ux2, uy2),
        }
        if self.stroke_hint != STROKE_HINT_PURE {
            if self.stroke_state == STROKE_THIN {
                let ix1 = (x1 - self.trans_x as f64).floor() as i32;
                let iy1 = (y1 - self.trans_y as f64).floor() as i32;
                let ix2 = (x2 - self.trans_x as f64).floor() as i32;
                let iy2 = (y2 - self.trans_y as f64).floor() as i32;
                let (tx, ty) = (self.trans_x, self.trans_y);
                let mut h = self.set_pixel_handler();
                h.draw_line(ix1 + tx, iy1 + ty, ix2 + tx, iy2 + ty);
                return true;
            }
            x1 = Self::normalize(x1);
            y1 = Self::normalize(y1);
            x2 = Self::normalize(x2);
            y2 = Self::normalize(y2);
        }
        if self.transform_state >= TRANSFORM_TRANSLATESCALE {
            let mut l = len(dx, dy);
            if l == 0.0 {
                l = 1.0;
                dx = 1.0;
            }
            let (ddx, ddy) = self.transform.delta_transform(dy / l, -dx / l);
            lw *= len(ddx, ddy);
        }
        lw = lw.max(1.0);
        dx = x2 - x1;
        dy = y2 - y1;
        let l = len(dx, dy);
        let (udx, udy);
        if l == 0.0 {
            if cap == 0 {
                return true;
            }
            udx = lw;
            udy = 0.0;
        } else {
            udx = lw * dx / l;
            udy = lw * dy / l;
        }
        let mut px = x1 + udy / 2.0;
        let mut py = y1 - udx / 2.0;
        if cap == 2 {
            px -= udx / 2.0;
            py -= udy / 2.0;
            dx += udx;
            dy += udy;
        }
        fill_parallelogram(&mut self.fill_target(), px, py, -udy, udx, dx, dy);
        true
    }

    // --- SpanShapeRenderer.Composite over AlphaPaintPipe ---

    /// `SpanShapeRenderer.fill`.
    fn span_fill(&mut self, s: &Shape) {
        if let Shape::Rect(r) = s {
            if self.transform.get_type() & NON_RECTILINEAR_TRANSFORM_MASK == 0 {
                self.render_rect(s, r);
                return;
            }
        }
        let mut sr = ShapeSpanIterator::new(self.stroke_hint != STROKE_HINT_PURE);
        let c = self.clip_region;
        sr.set_output_area_xyxy(c[0], c[1], c[2], c[3]);
        sr.append_path(&s.get_path_iterator(Some(&self.transform)), s.get_winding_rule());
        self.render_spans(s, &mut sr);
    }

    /// `SpanShapeRenderer.draw`.
    fn span_draw(&mut self, s: &Shape) {
        let mut sr = self.get_stroke_spans(s);
        self.render_spans(s, &mut sr);
    }

    /// `SpanShapeRenderer.renderRect`.
    fn render_rect(&mut self, s: &Shape, r: &Rectangle2D) {
        let mut c = [r.x, r.y, r.w, r.h];
        c[2] += c[0];
        c[3] += c[1];
        if c[2] <= c[0] || c[3] <= c[1] {
            return;
        }
        transform_pts(&self.transform, &mut c);
        if c[2] < c[0] {
            c.swap(0, 2);
        }
        if c[3] < c[1] {
            c.swap(1, 3);
        }
        let abox0 = [c[0] as i32, c[1] as i32, c[2] as i32, c[3] as i32];
        let dev_r = (abox0[0], abox0[1], abox0[2] - abox0[0], abox0[3] - abox0[1]);
        let abox = self.clip_box_to_bounds(abox0);
        if abox[0] >= abox[2] || abox[1] >= abox[3] {
            return;
        }
        let Some(mut ctx) = self.start_sequence(s, dev_r) else { return };
        if self.clip_state != CLIP_SHAPE {
            self.render_box(&mut ctx, abox[0], abox[1], abox[2] - abox[0], abox[3] - abox[1]);
        } else {
            for sp in region_spans(&self.clip_bands, abox) {
                if !self.render_box(&mut ctx, sp[0], sp[1], sp[2] - sp[0], sp[3] - sp[1]) {
                    break;
                }
            }
        }
        end_sequence(ctx);
    }

    /// `Region.clipBoxToBounds(bbox)`.
    fn clip_box_to_bounds(&self, mut b: [i32; 4]) -> [i32; 4] {
        let c = self.clip_region;
        if b[0] < c[0] {
            b[0] = c[0];
        }
        if b[1] < c[1] {
            b[1] = c[1];
        }
        if b[2] > c[2] {
            b[2] = c[2];
        }
        if b[3] > c[3] {
            b[3] = c[3];
        }
        b
    }

    /// `SpanShapeRenderer.renderSpans`.
    fn render_spans(&mut self, s: &Shape, sr: &mut ShapeSpanIterator) {
        let b = sr.get_path_box();
        let dev_r = (b[0], b[1], b[2].wrapping_sub(b[0]), b[3].wrapping_sub(b[1]));
        let abox = self.clip_box_to_bounds(b);
        if abox[0] >= abox[2] || abox[1] >= abox[3] {
            return;
        }
        sr.intersect_clip_box(abox[0], abox[1], abox[2], abox[3]);
        let Some(mut ctx) = self.start_sequence(s, dev_r) else { return };
        let mut sp = [0i32; 4];
        'spans: while sr.next_span(&mut sp) {
            let pieces = if self.clip_state == CLIP_SHAPE { region_clip(&self.clip_bands, sp) } else { vec![sp] };
            for p in pieces {
                if !self.render_box(&mut ctx, p[0], p[1], p[2] - p[0], p[3] - p[1]) {
                    break 'spans;
                }
            }
        }
        end_sequence(ctx);
    }

    /// `AlphaPaintPipe.startSequence`: `paint.createContext(...)`.
    fn start_sequence(&mut self, s: &Shape, dev_r: (i32, i32, i32, i32)) -> Option<PaintCtx> {
        let hint = |k: &str, speed: &str, quality: &str| match self.hints.get(k) {
            Some(v) if *v == speed => HintValue::Speed,
            Some(v) if *v == quality => HintValue::Quality,
            _ => HintValue::Default,
        };
        let hints = GradientHints {
            rendering: hint(KEY_RENDERING, "VALUE_RENDER_SPEED", "VALUE_RENDER_QUALITY"),
            color_rendering: hint(KEY_COLOR_RENDERING, "VALUE_COLOR_RENDER_SPEED", "VALUE_COLOR_RENDER_QUALITY"),
        };
        let ub = s.get_bounds2d();
        let ub = (ub.x, ub.y, ub.w, ub.h);
        let r = match &self.paint {
            Paint::Color(c) => return Some(PaintCtx::Color(c.value)),
            Paint::LinearGradient(g) => GradientPaintContext::create(GradientPaintRef::Linear(g), dev_r, ub, &self.transform, hints),
            Paint::RadialGradient(g) => GradientPaintContext::create(GradientPaintRef::Radial(g), dev_r, ub, &self.transform, hints),
            Paint::Pattern => {
                self.record_unsupported("a pattern fill in an offscreen raster");
                return None;
            }
        };
        match r {
            Ok(c) => Some(PaintCtx::Gradient(c)),
            Err(e) => {
                self.throw(e);
                None
            }
        }
    }

    fn throw(&mut self, e: String) {
        if self.exception.is_none() {
            self.exception = Some(e);
        }
    }

    /// `AlphaPaintPipe.renderPathTile(ctx, null, ...)`: paint tiles blitted with the current composite.
    fn render_box(&mut self, ctx: &mut PaintCtx, x: i32, y: i32, w: i32, h: i32) -> bool {
        let opaque = match ctx {
            PaintCtx::Color(_) => false,
            PaintCtx::Gradient(g) => !g.color_model().0,
        };
        let comp = if self.image_comp == ImageComp::SrcOverNoEa && opaque { ImageComp::SrcNoEa } else { self.image_comp };
        let mut rely = 0;
        while rely < h {
            let ty = y + rely;
            let th = (h - rely).min(TILE_SIZE);
            let mut relx = 0;
            while relx < w {
                let tx = x + relx;
                let tw = (w - relx).min(TILE_SIZE);
                let color_tile;
                let (pixels, scan): (&[i32], usize) = match ctx {
                    PaintCtx::Color(c) => {
                        color_tile = vec![*c; (tw * th) as usize];
                        (&color_tile, tw as usize)
                    }
                    PaintCtx::Gradient(g) => match g.get_raster(tx, ty, tw, th) {
                        Ok(r) => r,
                        Err(e) => {
                            self.throw(e);
                            return false;
                        }
                    },
                };
                let src_h = (pixels.len() / scan.max(1)) as i32;
                blit(&mut self.surface, None, pixels, scan, scan as i32, src_h, opaque, comp, self.composite.alpha, 0, 0, tx, ty, tw, th);
                relx += TILE_SIZE;
            }
            rely += TILE_SIZE;
        }
        true
    }

    // --- images ---

    /// `drawImage(img, x, y, null)`: `DrawImage.copyImage` -> `blitSurfaceData`.
    pub fn draw_image(&mut self, img: &IntImage, x: i32, y: i32) {
        if self.transform_state > TRANSFORM_ANY_TRANSLATE {
            self.record_unsupported("a transformed image draw in an offscreen raster");
            return;
        }
        let comp = if self.image_comp == ImageComp::SrcOverNoEa && img.opaque { ImageComp::SrcNoEa } else { self.image_comp };
        if comp == ImageComp::Clear || (comp == ImageComp::SrcOver && img.opaque) {
            self.record_unsupported("an image composite this port does not blit");
            return;
        }
        let clips = if self.clip_state == CLIP_SHAPE { region_spans(&self.clip_bands, self.clip_region) } else { vec![self.clip_region] };
        for clip in clips {
            blit(&mut self.surface, Some(clip), &img.data, img.width as usize, img.width, img.height, img.opaque, comp, self.composite.alpha, 0, 0, x + self.trans_x, y + self.trans_y, img.width, img.height);
        }
    }
}

pub const MIN_PEN_SIZE_AA: f64 = 0.0;

/// `Math.round(float)`.
fn java_round_f(f: f32) -> i32 {
    (f as f64 + 0.5).floor() as i32
}

/// `PixelToParallelogramConverter.len(x, y)`.
fn len(x: f64, y: f64) -> f64 {
    if x == 0.0 {
        y.abs()
    } else if y == 0.0 {
        x.abs()
    } else {
        (x * x + y * y).sqrt()
    }
}

fn intersect_xyxy(b: [i32; 4], x1: i32, y1: i32, x2: i32, y2: i32) -> [i32; 4] {
    let mut r = b;
    if r[0] < x1 {
        r[0] = x1;
    }
    if r[1] < y1 {
        r[1] = y1;
    }
    if r[2] > x2 {
        r[2] = x2;
    }
    if r[3] > y2 {
        r[3] = y2;
    }
    if r[2] <= r[0] || r[3] <= r[1] {
        [r[0], r[1], r[0], r[1]]
    } else {
        r
    }
}


fn end_sequence(ctx: PaintCtx) {
    if let PaintCtx::Gradient(mut g) = ctx {
        g.dispose();
    }
}

/// The native `Blit`/`MaskBlit` bounds handling (`SurfaceData_IntersectBounds`/`IntersectBlitBounds`), then the
/// loop for `comp`.
#[allow(clippy::too_many_arguments)]
fn blit(dst: &mut IntImage, clip: Option<[i32; 4]>, src: &[i32], src_scan: usize, src_w: i32, src_h: i32, opaque: bool, comp: ImageComp, alpha: f32, sx: i32, sy: i32, dx: i32, dy: i32, w: i32, h: i32) {
    let mut d = [dx, dy, dx.saturating_add(w), dy.saturating_add(h)];
    if let Some(c) = clip {
        d = intersect_xyxy(d, c[0], c[1], c[2], c[3]);
    }
    let mut s = [sx, sy, sx.saturating_add(w), sy.saturating_add(h)];
    s = intersect_xyxy(s, 0, 0, src_w, src_h);
    let (ox, oy) = (dx - sx, dy - sy);
    d = intersect_xyxy(d, s[0] + ox, s[1] + oy, s[2] + ox, s[3] + oy);
    d = intersect_xyxy(d, 0, 0, dst.width, dst.height);
    let (bw, bh) = (d[2] - d[0], d[3] - d[1]);
    if bw <= 0 || bh <= 0 {
        return;
    }
    let src_off = (d[1] - oy) as usize * src_scan + (d[0] - ox) as usize;
    let dst_scan = dst.width as usize;
    let dst_off = d[1] as usize * dst_scan + d[0] as usize;
    match comp {
        ImageComp::SrcNoEa => {
            if opaque {
                int_rgb_to_int_argb_convert(src, src_off, src_scan, &mut dst.data, dst_off, dst_scan, bw as usize, bh as usize);
            } else {
                for y in 0..bh as usize {
                    dst.data[dst_off + y * dst_scan..dst_off + y * dst_scan + bw as usize].copy_from_slice(&src[src_off + y * src_scan..src_off + y * src_scan + bw as usize]);
                }
            }
        }
        ImageComp::SrcOverNoEa | ImageComp::SrcOver => {
            let ea = if comp == ImageComp::SrcOverNoEa { 255 } else { extra_alpha_to_int(alpha) };
            int_argb_to_int_argb_srcover_maskblit(src, src_off, src_scan, &mut dst.data, dst_off, dst_scan, None, bw as usize, bh as usize, ea);
        }
        ImageComp::Clear => {}
    }
}

/// `SunGraphics2D.transformShape(tx, ty, s)`.
fn transform_shape_int(tx: i32, ty: i32, s: &UsrClip) -> UsrClip {
    match s {
        UsrClip::Rect(x, y, w, h) => UsrClip::Rect(x.wrapping_add(tx), y.wrapping_add(ty), *w, *h),
        UsrClip::Rect2D(r) => UsrClip::Rect2D(Rectangle2D::double(r.x + tx as f64, r.y + ty as f64, r.w, r.h)),
        UsrClip::Shape(_) | UsrClip::Area(_) => {
            if tx == 0 && ty == 0 {
                clone_shape(s)
            } else {
                UsrClip::Shape(Shape::Path(create_transformed_shape(&AffineTransform::get_translate_instance(tx as f64, ty as f64), &s.to_shape())))
            }
        }
    }
}

/// `SunGraphics2D.transformShape(at, s)`.
fn transform_shape_at(at: &AffineTransform, s: &UsrClip) -> UsrClip {
    let rect = match s {
        UsrClip::Rect(x, y, w, h) => Some(Rectangle2D::double(*x as f64, *y as f64, *w as f64, *h as f64)),
        UsrClip::Rect2D(r) => Some(*r),
        UsrClip::Shape(_) | UsrClip::Area(_) => None,
    };
    if let Some(r) = rect {
        if at.get_type() & NON_RECTILINEAR_TRANSFORM_MASK == 0 {
            let mut m = [r.x, r.y, r.x + r.w, r.y + r.h];
            transform_pts(at, &mut m);
            if (r.w > 0.0) != (m[2] - m[0] > 0.0) {
                m.swap(0, 2);
            }
            if (r.h > 0.0) != (m[3] - m[1] > 0.0) {
                m.swap(1, 3);
            }
            return UsrClip::Rect2D(Rectangle2D::double(m[0], m[1], m[2] - m[0], m[3] - m[1]));
        }
    }
    if at.is_identity() {
        return clone_shape(s);
    }
    UsrClip::Shape(Shape::Path(create_transformed_shape(at, &s.to_shape())))
}

/// `intersectShapes(s1, s2, true, true)`; `None` where Java needs an `Area`.
fn intersect_shapes(s1: &UsrClip, s2: &UsrClip) -> Option<UsrClip> {
    if let (UsrClip::Rect(ax, ay, aw, ah), UsrClip::Rect(bx, by, bw, bh)) = (s1, s2) {
        let (mut tx1, mut ty1) = (*ax, *ay);
        let (rx1, ry1) = (*bx, *by);
        let mut tx2 = tx1 as i64 + *aw as i64;
        let mut ty2 = ty1 as i64 + *ah as i64;
        let rx2 = rx1 as i64 + *bw as i64;
        let ry2 = ry1 as i64 + *bh as i64;
        if tx1 < rx1 {
            tx1 = rx1;
        }
        if ty1 < ry1 {
            ty1 = ry1;
        }
        if tx2 > rx2 {
            tx2 = rx2;
        }
        if ty2 > ry2 {
            ty2 = ry2;
        }
        tx2 -= tx1 as i64;
        ty2 -= ty1 as i64;
        return Some(UsrClip::Rect(tx1, ty1, tx2.max(i32::MIN as i64) as i32, ty2.max(i32::MIN as i64) as i32));
    }
    let as_rect = |s: &UsrClip| match s {
        UsrClip::Rect(x, y, w, h) => Some((*x as f64, *y as f64, *w as f64, *h as f64)),
        UsrClip::Rect2D(r) => Some((r.x, r.y, r.w, r.h)),
        UsrClip::Shape(_) | UsrClip::Area(_) => None,
    };
    let (r, s) = if let Some(r) = as_rect(s1) { (r, s2) } else if let Some(r) = as_rect(s2) { (r, s1) } else { return Some(intersect_by_area(s1, s2)) };
    if let Some(r2) = as_rect(s) {
        let x1 = r.0.max(r2.0);
        let x2 = (r.0 + r.2).min(r2.0 + r2.2);
        let y1 = r.1.max(r2.1);
        let y2 = (r.1 + r.3).min(r2.1 + r2.3);
        let out = if x2 - x1 < 0.0 || y2 - y1 < 0.0 { Rectangle2D::float(0.0, 0.0, 0.0, 0.0) } else { set_frame_from_diagonal_float(x1, y1, x2, y2) };
        return Some(UsrClip::Rect2D(out));
    }
    let b = match s {
        UsrClip::Area(a) => {
            let (x, y, w, h) = a.get_bounds2d();
            Rectangle2D::double(x, y, w, h)
        }
        _ => s.to_shape().get_bounds2d(),
    };
    let contains = r.2 > 0.0 && r.3 > 0.0 && b.w > 0.0 && b.h > 0.0 && b.x >= r.0 && b.y >= r.1 && b.x + b.w <= r.0 + r.2 && b.y + b.h <= r.1 + r.3;
    if contains {
        return Some(clone_shape(s));
    }
    Some(intersect_by_area(&UsrClip::Rect2D(Rectangle2D::double(r.0, r.1, r.2, r.3)), s))
}

/// `new Rectangle2D.Float().setFrameFromDiagonal(x1, y1, x2, y2)`.
fn set_frame_from_diagonal_float(x1: f64, y1: f64, x2: f64, y2: f64) -> Rectangle2D {
    let (x1, x2) = if x2 < x1 { (x2, x1) } else { (x1, x2) };
    let (y1, y2) = if y2 < y1 { (y2, y1) } else { (y1, y2) };
    let mut r = Rectangle2D::float(0.0, 0.0, 0.0, 0.0);
    r.set_rect(x1, y1, x2 - x1, y2 - y1);
    r
}

impl crate::asc::flex2::fonts::java2d::java::awt::graphics2d::Graphics2D for SunGraphics2D {
    fn get_clip(&self) -> Option<UsrClip> {
        SunGraphics2D::get_clip(self)
    }
    fn set_clip(&mut self, c: Option<&UsrClip>) {
        self.set_clip_usr(c);
    }
    fn clip(&mut self, s: &Shape) {
        SunGraphics2D::clip(self, s);
    }
    fn get_composite(&self) -> AlphaComposite {
        self.composite
    }
    fn set_composite(&mut self, c: AlphaComposite) {
        SunGraphics2D::set_composite(self, c);
    }
    fn get_transform(&self) -> AffineTransform {
        self.transform
    }
    fn set_transform(&mut self, t: &AffineTransform) {
        SunGraphics2D::set_transform(self, t);
    }
    fn transform(&mut self, t: &AffineTransform) {
        SunGraphics2D::transform(self, t);
    }
    fn get_rendering_hints(&self) -> RenderingHints {
        self.hints.clone()
    }
    fn add_rendering_hints(&mut self, h: &RenderingHints) {
        SunGraphics2D::add_rendering_hints(self, h);
    }
    fn set_rendering_hints(&mut self, h: RenderingHints) {
        SunGraphics2D::set_rendering_hints(self, &h);
    }
    fn set_paint(&mut self, p: Option<Paint>) {
        SunGraphics2D::set_paint(self, p);
    }
    fn set_stroke(&mut self, s: BasicStroke) {
        SunGraphics2D::set_stroke(self, s);
    }
    fn fill(&mut self, s: &Shape) {
        SunGraphics2D::fill(self, s);
    }
    fn draw(&mut self, s: &Shape) {
        SunGraphics2D::draw(self, s);
    }
    fn draw_image(&mut self, img: &IntImage, x: i32, y: i32) {
        SunGraphics2D::draw_image(self, img, x, y);
    }
    fn destination_size(&self) -> (i32, i32) {
        (self.surface.width, self.surface.height)
    }
    fn record_unsupported(&mut self, what: &str) {
        SunGraphics2D::record_unsupported(self, what);
    }
}

/// `Region.getInstance(box, spans)`: the spans clamped to `box`, adjacent spans on a row joined, identical
/// consecutive rows joined into bands.
fn region_bands(b: &[i32; 4], spans: &[[i32; 4]]) -> Vec<(i32, i32, Vec<(i32, i32)>)> {
    let mut rows: Vec<(i32, Vec<(i32, i32)>)> = Vec::new();
    for s in spans {
        let (x0, y0, x1, y1) = (s[0].max(b[0]), s[1].max(b[1]), s[2].min(b[2]), s[3].min(b[3]));
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        for y in y0..y1 {
            match rows.last_mut() {
                Some((ry, r)) if *ry == y => match r.last_mut() {
                    Some(last) if last.1 == x0 => last.1 = x1,
                    _ => r.push((x0, x1)),
                },
                _ => rows.push((y, vec![(x0, x1)])),
            }
        }
    }
    let mut bands: Vec<(i32, i32, Vec<(i32, i32)>)> = Vec::new();
    for (y, r) in rows {
        match bands.last_mut() {
            Some(band) if band.1 == y && band.2 == r => band.1 = y + 1,
            _ => bands.push((y, y + 1, r)),
        }
    }
    bands
}

fn bands_bbox(bands: &[(i32, i32, Vec<(i32, i32)>)]) -> [i32; 4] {
    if bands.is_empty() {
        return [0, 0, 0, 0];
    }
    let lox = bands.iter().map(|b| b.2.first().unwrap().0).min().unwrap();
    let hix = bands.iter().map(|b| b.2.last().unwrap().1).max().unwrap();
    [lox, bands[0].0, hix, bands.last().unwrap().1]
}

/// `Region.getSpanIterator(bbox)`: each band's spans clipped to `bbox`.
fn region_spans(bands: &[(i32, i32, Vec<(i32, i32)>)], b: [i32; 4]) -> Vec<[i32; 4]> {
    let mut out = Vec::new();
    for (y0, y1, spans) in bands {
        let (y0, y1) = ((*y0).max(b[1]), (*y1).min(b[3]));
        if y1 <= y0 {
            continue;
        }
        for &(x0, x1) in spans {
            let (x0, x1) = (x0.max(b[0]), x1.min(b[2]));
            if x1 > x0 {
                out.push([x0, y0, x1, y1]);
            }
        }
    }
    out
}

/// `Region.filter(spans)` (`RegionClipSpanIterator`) for one span: its overlaps with the region, left to right.
fn region_clip(bands: &[(i32, i32, Vec<(i32, i32)>)], sp: [i32; 4]) -> Vec<[i32; 4]> {
    region_spans(bands, sp)
}

/// `cloneShape(s)`: `new GeneralPath(s)`.
fn clone_shape(s: &UsrClip) -> UsrClip {
    let sh = s.to_shape();
    let mut p = Path2D::new_float(sh.get_winding_rule());
    p.append(&sh.get_path_iterator(None), false);
    UsrClip::Shape(Shape::Path(p))
}

/// `intersectByArea(s1, s2, true, true)`.
fn intersect_by_area(s1: &UsrClip, s2: &UsrClip) -> UsrClip {
    let area = |s: &UsrClip| match s {
        UsrClip::Area(a) => a.clone(),
        _ => {
            let sh = s.to_shape();
            Area::from_path(&sh.get_path_iterator(None), sh.get_winding_rule())
        }
    };
    let mut a1 = area(s1);
    a1.intersect(&area(s2));
    if a1.is_rectangular() {
        let (x, y, w, h) = a1.get_bounds2d();
        let b = crate::asc::flex2::batik::gvt::filter::get_bounds(&Rectangle2D::double(x, y, w, h));
        return UsrClip::Rect(b.x, b.y, b.w, b.h);
    }
    UsrClip::Area(a1)
}
