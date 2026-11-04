//! Ports of the Batik renderables an offscreen graphics node paints through: `gvt.filter.GraphicsNodeRable8Bit`,
//! `ext.awt.image.renderable` `FilterChainRable8Bit`, `PadRable8Bit` and `GaussianBlurRable8Bit` (with
//! `AbstractColorInterpolationRable`); the renderings they create (`GraphicsNodeRed8Bit`, `PadRed`, `Any2LsRGBRed`,
//! `Any2sRGBRed`, `FormatRed`, `GaussianBlurRed8Bit`, `BufferedImageCachableRed`); and the `GraphicsUtil.drawImage`
//! paths that draw them into a `Graphics2D`, tile by tile.

use std::rc::Rc;
use std::sync::OnceLock;

use super::{GraphicsNode, Unsupported};
use crate::asc::flex2::flash::graphics::g2d::graphic_context::{RenderingHints, KEY_INTERPOLATION, KEY_RENDERING};
use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::{AffineTransform, TYPE_TRANSLATION};
use crate::asc::flex2::fonts::java2d::sun::java2d::marlin::d_marlin_rendering_engine::get_determinant;
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::{create_transformed_shape, Rectangle2D, Shape};
use crate::asc::flex2::fonts::java2d::java::awt::graphics2d::Graphics2D;
use crate::asc::flex2::fonts::java2d::java::awt::paint::AlphaComposite;
use crate::asc::flex2::fonts::java2d::sun::awt::image::imaging_lib::{convolve_raster, transform_nearest};
use crate::asc::flex2::fonts::java2d::sun::java2d::sun_graphics2d::{IntImage, SunGraphics2D};

const DEFAULT_TILE_SIZE: i32 = 128;

/// A filter chain as the filter bridges build it; `SourceGraphic` is the filtered node's `GraphicsNodeRable8Bit`.
#[derive(Clone, Debug)]
pub enum Rable {
    SourceGraphic,
    /// `PadRable8Bit(src, padRect, ZERO_PAD)`.
    Pad { src: Rc<Rable>, rect: Rectangle2D },
    /// `FilterChainRable8Bit`: its crop (a `Pad` of the filter region) and the region.
    Chain { crop: Rc<Rable>, region: Rectangle2D },
    /// `GaussianBlurRable8Bit` and its `setColorSpaceLinear`.
    GaussianBlur { src: Rc<Rable>, sdx: f64, sdy: f64, linear: bool },
    /// `AffineRable8Bit(src, affine)` (`feOffset`).
    Affine { src: Rc<Rable>, affine: AffineTransform },
    /// `FilterResRable8Bit(src, filterResolutionX, filterResolutionY)`.
    FilterRes { src: Rc<Rable>, res_x: i32, res_y: i32 },
    /// A primitive this port does not render.
    Unsupported(String),
}

/// `java.awt.Rectangle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl JRect {
    /// `Rectangle.intersects(r)`.
    pub fn intersects(&self, r: &JRect) -> bool {
        let (mut tw, mut th, mut rw, mut rh) = (self.w, self.h, r.w, r.h);
        if rw <= 0 || rh <= 0 || tw <= 0 || th <= 0 {
            return false;
        }
        let (tx, ty, rx, ry) = (self.x, self.y, r.x, r.y);
        rw = rw.wrapping_add(rx);
        rh = rh.wrapping_add(ry);
        tw = tw.wrapping_add(tx);
        th = th.wrapping_add(ty);
        (rw < rx || rw > tx) && (rh < ry || rh > ty) && (tw < tx || tw > rx) && (th < ty || th > ry)
    }

    /// `Rectangle.intersection(r)`.
    pub fn intersection(&self, r: &JRect) -> JRect {
        let (mut tx1, mut ty1) = (self.x, self.y);
        let (rx1, ry1) = (r.x, r.y);
        let mut tx2 = tx1 as i64 + self.w as i64;
        let mut ty2 = ty1 as i64 + self.h as i64;
        let rx2 = rx1 as i64 + r.w as i64;
        let ry2 = ry1 as i64 + r.h as i64;
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
        JRect { x: tx1, y: ty1, w: tx2.max(i32::MIN as i64) as i32, h: ty2.max(i32::MIN as i64) as i32 }
    }

    fn to_rect2d(self) -> Rectangle2D {
        Rectangle2D::double(self.x as f64, self.y as f64, self.w as f64, self.h as f64)
    }
}

/// `Rectangle2D.getBounds()`.
pub fn get_bounds(r: &Rectangle2D) -> JRect {
    if r.w < 0.0 || r.h < 0.0 {
        return JRect { x: 0, y: 0, w: 0, h: 0 };
    }
    let x = r.x.floor();
    let y = r.y.floor();
    let x2 = (r.x + r.w).ceil();
    let y2 = (r.y + r.h).ceil();
    JRect { x: x as i32, y: y as i32, w: (x2 - x) as i32, h: (y2 - y) as i32 }
}

/// `Rectangle2D.intersect(src1, src2, dest)` with `dest` of `dest_like`'s class.
fn intersect2d(a: &Rectangle2D, b: &Rectangle2D, dest_like: &Rectangle2D) -> Rectangle2D {
    let x1 = a.get_min_x().max(b.get_min_x());
    let y1 = a.get_min_y().max(b.get_min_y());
    let x2 = a.get_max_x().min(b.get_max_x());
    let y2 = a.get_max_y().min(b.get_max_y());
    let (x1, x2) = if x2 < x1 { (x2, x1) } else { (x1, x2) };
    let (y1, y2) = if y2 < y1 { (y2, y1) } else { (y1, y2) };
    let mut r = *dest_like;
    r.set_rect(x1, y1, x2 - x1, y2 - y1);
    r
}

/// `Rectangle2D.intersects(r)`.
fn intersects2d(a: &Rectangle2D, b: &Rectangle2D) -> bool {
    a.intersects(b.x, b.y, b.w, b.h)
}

/// The renderable an offscreen node draws: a filter chain (or just the node) applied to `node`.
#[derive(Clone, Copy)]
pub struct Filter<'a> {
    pub rable: &'a Rable,
    pub node: &'a GraphicsNode,
}

impl<'a> Filter<'a> {
    fn with(&self, rable: &'a Rable) -> Filter<'a> {
        Filter { rable, node: self.node }
    }

    /// `getBounds2D()`.
    pub fn get_bounds2d(&self) -> Rectangle2D {
        match self.rable {
            Rable::SourceGraphic => self.node.primitive_bounds_after_get_bounds().unwrap_or_else(|| Rectangle2D::double(0.0, 0.0, 0.0, 0.0)),
            Rable::Pad { rect, .. } => *rect,
            Rable::Chain { region, .. } => *region,
            Rable::GaussianBlur { src, sdx, sdy, .. } => {
                let s = self.with(src).get_bounds2d();
                let dsqrt2pi = ((2.0 * std::f64::consts::PI).sqrt() * 3.0 / 4.0) as f32;
                let dx = (sdx * dsqrt2pi as f64) as f32;
                let dy = (sdy * dsqrt2pi as f64) as f32;
                let rad_x = 3.0 * dx / 2.0;
                let rad_y = 3.0 * dy / 2.0;
                Rectangle2D::float((s.get_min_x() - rad_x as f64) as f32, (s.get_min_y() - rad_y as f64) as f32, (s.w + 2.0 * rad_x as f64) as f32, (s.h + 2.0 * rad_y as f64) as f32)
            }
            Rable::Affine { src, affine } => create_transformed_shape(affine, &Shape::Rect(self.with(src).get_bounds2d())).get_bounds2d(),
            Rable::FilterRes { src, .. } => self.with(src).get_bounds2d(),
            Rable::Unsupported(_) => Rectangle2D::double(0.0, 0.0, 0.0, 0.0),
        }
    }

    /// `PaintRable.paintRable(g2d)`: `true` when it painted directly.
    fn paint_rable(&self, g2d: &mut dyn Graphics2D) -> Result<bool, Unsupported> {
        let over = g2d.get_composite() == AlphaComposite::SRC_OVER;
        match self.rable {
            Rable::SourceGraphic => {
                if !over {
                    return Ok(false);
                }
                self.node.primitive_paint(g2d)?;
                Ok(true)
            }
            Rable::Chain { crop, .. } => {
                if !over {
                    return Ok(false);
                }
                draw_image_filter(g2d, &self.with(crop))?;
                Ok(true)
            }
            Rable::Pad { src, rect } => {
                if !over {
                    return Ok(false);
                }
                let clip = g2d.get_clip();
                g2d.clip(&Shape::Rect(*rect));
                let r = draw_image_filter(g2d, &self.with(src));
                g2d.set_clip(clip.as_ref());
                r?;
                Ok(true)
            }
            Rable::Affine { src, affine } => {
                let at = g2d.get_transform();
                g2d.transform(affine);
                let r = draw_image_filter(g2d, &self.with(src));
                g2d.set_transform(&at);
                r?;
                Ok(true)
            }
            Rable::FilterRes { src, .. } => {
                if !over {
                    return Ok(false);
                }
                self.with(src).distribute_across(g2d)
            }
            Rable::GaussianBlur { .. } => Ok(false),
            Rable::Unsupported(what) => {
                g2d.record_unsupported(what);
                Ok(true)
            }
        }
    }

    /// `FilterResRable8Bit.distributeAcross(src, g2d)` with `self` as `src`.
    fn distribute_across(&self, g2d: &mut dyn Graphics2D) -> Result<bool, Unsupported> {
        match self.rable {
            Rable::Pad { src, rect } => {
                let clip = g2d.get_clip();
                g2d.clip(&Shape::Rect(*rect));
                let r = self.with(src).distribute_across(g2d);
                g2d.set_clip(clip.as_ref());
                r
            }
            _ => Ok(false),
        }
    }

    /// `createRendering(new RenderContext(usr2dev, aoi, hints))`.
    fn create_rendering(&self, usr2dev: &AffineTransform, aoi: Option<&Shape>, hints: &RenderingHints, outer: &mut dyn Graphics2D) -> Result<Option<Red<'a>>, Unsupported> {
        match self.rable {
            Rable::SourceGraphic => {
                let b = self.get_bounds2d();
                if b.w > 0.0 && b.h > 0.0 {
                    Ok(Some(Red::graphics_node(self.node, *usr2dev, hints.clone())))
                } else {
                    Ok(None)
                }
            }
            Rable::Chain { crop, .. } => self.with(crop).create_rendering(usr2dev, aoi, hints, outer),
            Rable::Pad { src, rect } => {
                let srcf = self.with(src);
                let aoi = aoi.cloned().unwrap_or(Shape::Rect(*rect));
                let srect = srcf.get_bounds2d();
                let mut arect = aoi.get_bounds2d();
                if !intersects2d(&arect, rect) {
                    return Ok(None);
                }
                arect = intersect2d(&arect, rect, &arect);
                let mut ri = None;
                if intersects2d(&arect, &srect) {
                    let srect = intersect2d(&srect, &arect, &srect);
                    ri = srcf.create_rendering(usr2dev, Some(&Shape::Rect(srect)), hints, outer)?;
                }
                let cr = ri.unwrap_or_else(Red::empty_1x1);
                let dev = get_bounds(&create_transformed_shape(usr2dev, &Shape::Rect(arect)).get_bounds2d());
                Ok(Some(Red::pad(cr, dev)))
            }
            Rable::GaussianBlur { src, sdx: std_x, sdy: std_y, linear } => {
                let at = usr2dev;
                let (sx, sy, shx, shy) = (at.get_scale_x(), at.get_scale_y(), at.get_shear_x(), at.get_shear_y());
                let (tx, ty) = (at.get_translate_x(), at.get_translate_y());
                let mut scale_x = (sx * sx + shy * shy).sqrt();
                let mut scale_y = (sy * sy + shx * shx).sqrt();
                let mut sdx = std_x * scale_x;
                let mut sdy = std_y * scale_y;
                let src_at;
                let res_at;
                let (mut outset_x, mut outset_y);
                if sdx < 10.0 && sdy < 10.0 && eps_eq(sdx, sdy) && eps_abs_eq(sx / scale_x, sy / scale_y) {
                    src_at = *at;
                    res_at = None;
                    outset_x = 0;
                    outset_y = 0;
                } else {
                    if sdx > 10.0 {
                        scale_x = scale_x * 10.0 / sdx;
                        sdx = 10.0;
                    }
                    if sdy > 10.0 {
                        scale_y = scale_y * 10.0 / sdy;
                        sdy = 10.0;
                    }
                    src_at = AffineTransform::get_scale_instance(scale_x, scale_y);
                    res_at = Some(AffineTransform::from_values(sx / scale_x, shy / scale_x, shx / scale_y, sy / scale_y, tx, ty));
                    outset_x = 1;
                    outset_y = 1;
                }
                let aoi = aoi.cloned().unwrap_or_else(|| Shape::Rect(self.get_bounds2d()));
                let mut dev_rect = get_bounds(&create_transformed_shape(&src_at, &aoi).get_bounds2d());
                outset_x += surround_pixels(sdx, hints);
                outset_y += surround_pixels(sdy, hints);
                dev_rect.x -= outset_x;
                dev_rect.y -= outset_y;
                dev_rect.w += 2 * outset_x;
                dev_rect.h += 2 * outset_y;
                let r = match src_at.create_inverse() {
                    Ok(inv) => create_transformed_shape(&inv, &Shape::Rect(dev_rect.to_rect2d())).get_bounds2d(),
                    Err(_) => {
                        let r = aoi.get_bounds2d();
                        Rectangle2D::double(r.x - outset_x as f64 / scale_x, r.y - outset_y as f64 / scale_y, r.w + 2.0 * outset_x as f64 / scale_x, r.h + 2.0 * outset_y as f64 / scale_y)
                    }
                };
                let Some(ri) = self.with(src).create_rendering(&src_at, Some(&Shape::Rect(r)), hints, outer)? else { return Ok(None) };
                let mut cr = if *linear { Red::to_linear(ri) } else { Red::to_srgb(ri) };
                if dev_rect != cr.bounds {
                    cr = Red::pad(cr, dev_rect);
                }
                let mut cr = Red::blur(cr, sdx, sdy, hints)?;
                if let Some(res) = res_at {
                    if !res.is_identity() {
                        cr = Red::affine(cr, res, hints.clone());
                    }
                }
                Ok(Some(cr))
            }
            Rable::Affine { src, affine } => {
                let Ok(inv) = affine.create_inverse() else { return Ok(None) };
                let aoi = aoi.map(|a| Shape::Path(create_transformed_shape(&inv, a)));
                let mut at = *usr2dev;
                at.concatenate(affine);
                self.with(src).create_rendering(&at, aoi.as_ref(), hints, outer)
            }
            Rable::FilterRes { src, res_x, res_y } => {
                let (res_x, res_y) = (*res_x, *res_y);
                if res_x <= 0 || res_y == 0 {
                    return Ok(None);
                }
                let image_rect = self.get_bounds2d();
                let dev_rect = get_bounds(&create_transformed_shape(usr2dev, &Shape::Rect(image_rect)).get_bounds2d());
                let mut scale_x = 1f32;
                if res_x < dev_rect.w {
                    scale_x = res_x as f32 / dev_rect.w as f32;
                }
                let mut scale_y = 1f32;
                if res_y < 0 {
                    scale_y = scale_x;
                } else if res_y < dev_rect.h {
                    scale_y = res_y as f32 / dev_rect.h as f32;
                }
                if scale_x >= 1.0 && scale_y >= 1.0 {
                    return self.with(src).create_rendering(usr2dev, aoi, hints, outer);
                }
                let res_scale = ((res_x as f64 / image_rect.w).min(res_y as f64 / image_rect.h)) as f32;
                let res_at = AffineTransform::get_scale_instance(res_scale as f64, res_scale as f64);
                let Some(res_red) = self.with(src).create_rendering(&res_at, None, hints, outer)? else { return Err(Unsupported("null".into())) };
                let rs = res_scale as f64;
                let residual = AffineTransform::from_values(usr2dev.get_scale_x() / rs, usr2dev.get_shear_y() / rs, usr2dev.get_shear_x() / rs, usr2dev.get_scale_y() / rs, usr2dev.get_translate_x(), usr2dev.get_translate_y());
                if !res_red.cm.linear {
                    outer.record_unsupported("a filterRes tile cache");
                }
                Ok(Some(Red::affine(res_red, residual, hints.clone())))
            }
            Rable::Unsupported(what) => {
                outer.record_unsupported(what);
                Ok(None)
            }
        }
    }
}

/// `GaussianBlurRable8Bit.eps_eq`.
fn eps_eq(f1: f64, f2: f64) -> bool {
    f1 >= f2 - 0.0001 && f1 <= f2 + 0.0001
}

fn eps_abs_eq(f1: f64, f2: f64) -> bool {
    eps_eq(f1.abs(), f2.abs())
}

fn sqrt2pi() -> f32 {
    (2.0 * std::f64::consts::PI).sqrt() as f32
}

fn red_dsqrt2pi() -> f32 {
    sqrt2pi() * 3.0f32 / 4.0f32
}

/// `GaussianBlurRed8Bit.surroundPixels(stdDev, hints)`.
fn surround_pixels(std_dev: f64, hints: &RenderingHints) -> i32 {
    let high_quality = hints.get(KEY_RENDERING).is_some_and(|v| *v == "VALUE_RENDER_QUALITY");
    if std_dev < 2.0 || high_quality {
        let s = sqrt2pi() as f64;
        let mut area_sum = (0.5 / (std_dev * s)) as f32;
        let mut i: i32 = 0;
        while area_sum < 0.499f32 {
            area_sum += (std::f64::consts::E.powf((-i * i) as f64 / (2.0 * std_dev * std_dev)) / (std_dev * s)) as f32;
            i += 1;
        }
        return i;
    }
    let diam = (red_dsqrt2pi() as f64 * std_dev + 0.5f32 as f64).floor() as i32;
    if diam % 2 == 0 {
        diam - 1 + diam / 2
    } else {
        diam - 2 + diam / 2
    }
}

/// `GaussianBlurRed8Bit.computeQualityKernelData(len, stdDev)`.
fn quality_kernel(len: i32, std_dev: f64) -> Vec<f32> {
    let mut k = vec![0f32; len as usize];
    let mid = len / 2;
    let mut sum = 0f32;
    let s = sqrt2pi() as f64;
    for i in 0..len {
        let d = i - mid;
        k[i as usize] = (std::f64::consts::E.powf((-d * d) as f64 / (2.0 * std_dev * std_dev)) / (s * std_dev)) as f32;
        sum += k[i as usize];
    }
    for v in &mut k {
        *v /= sum;
    }
    k
}

/// A packed-int `WritableRaster` at `(x, y)` (scan = width).
pub struct Raster {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub data: Vec<i32>,
}

impl Raster {
    fn new(x: i32, y: i32, w: i32, h: i32) -> Raster {
        Raster { x, y, w, h, data: vec![0; (w.max(0) as usize) * (h.max(0) as usize)] }
    }

    fn bounds(&self) -> JRect {
        JRect { x: self.x, y: self.y, w: self.w, h: self.h }
    }

    /// `createWritableChild` of `r`, copied out.
    fn child(&self, r: JRect) -> Raster {
        let mut c = Raster::new(r.x, r.y, r.w, r.h);
        c.copy_from(self);
        c
    }

    /// `GraphicsUtil.copyData_INT_PACK(src, this)`: the overlap.
    fn copy_from(&mut self, src: &Raster) {
        let x0 = self.x.max(src.x);
        let y0 = self.y.max(src.y);
        let x1 = (self.x + self.w).min(src.x + src.w);
        let y1 = (self.y + self.h).min(src.y + src.h);
        for y in y0..y1 {
            for x in x0..x1 {
                self.data[((y - self.y) * self.w + (x - self.x)) as usize] = src.data[((y - src.y) * src.w + (x - src.x)) as usize];
            }
        }
    }

    fn zero_rect(&mut self, r: JRect) {
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                self.data[((y - self.y) * self.w + (x - self.x)) as usize] = 0;
            }
        }
    }
}

/// The colour model of a rendering: always packed ARGB, in sRGB or linear RGB, premultiplied or not.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Cm {
    linear: bool,
    premult: bool,
}

const SRGB_UNPRE: Cm = Cm { linear: false, premult: false };

enum RedKind<'a> {
    /// `GraphicsNodeRed8Bit(node, node2dev, usePrimitivePaint = true, hints)`.
    GraphicsNode { node: &'a GraphicsNode, node2dev: AffineTransform, hints: RenderingHints },
    /// `BufferedImageCachableRed` of a new 1x1 `TYPE_INT_ARGB` image.
    Empty1x1,
    /// `PadRed(src, bounds, ZERO_PAD)`.
    Pad(Box<Red<'a>>),
    /// `Any2LsRGBRed(src)` of an sRGB source.
    ToLinear(Box<Red<'a>>),
    /// `Any2sRGBRed(src)` of a linear source.
    ToSrgb(Box<Red<'a>>),
    /// `FormatRed(src, cm)` from premultiplied to not.
    Unpremultiply(Box<Red<'a>>),
    /// `GaussianBlurRed8Bit`.
    Blur { src: Box<Red<'a>>, xinset: i32, yinset: i32, conv: [Option<Vec<f32>>; 2], dx: i32, dy: i32 },
    /// `AffineRed(src, src2me, hints)`: resampled through `AffineTransformOp`, which this port does not do.
    Affine { src: Box<Red<'a>>, src2me: AffineTransform, hints: RenderingHints },
    /// `TranslateRed(src, xloc, yloc)`.
    Translate { src: Box<Red<'a>>, dx: i32, dy: i32 },
}

/// A `CachableRed`: its bounds, colour model, tile grid and how to compute a region of it (`copyData`).
pub struct Red<'a> {
    kind: RedKind<'a>,
    cm: Cm,
    bounds: JRect,
    tile_grid_x_off: i32,
    tile_grid_y_off: i32,
    tile_width: i32,
    tile_height: i32,
    min_tile_x: i32,
    min_tile_y: i32,
    num_x_tiles: i32,
    num_y_tiles: i32,
}

fn srgb_to_lsrgb_lut() -> &'static [i32; 256] {
    static LUT: OnceLock<[i32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut lut = [0; 256];
        let scale = 1.0 / 255.0;
        for (i, v) in lut.iter_mut().enumerate() {
            let value = i as f64 * scale;
            let l = if value <= 0.003928 { value * (1.0 / 12.92) } else { ((value + 0.055) / 1.055).powf(2.4) };
            *v = java_round(l * 255.0);
        }
        lut
    })
}

fn linear_to_srgb_lut() -> &'static [i32; 256] {
    static LUT: OnceLock<[i32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut lut = [0; 256];
        let scale = 1.0 / 255.0;
        let exp = 1.0 / 2.4;
        for (i, v) in lut.iter_mut().enumerate() {
            let mut value = i as f64 * scale;
            if value <= 0.0031308 {
                value *= 12.92;
            } else {
                value = 1.055 * value.powf(exp) - 0.055;
            }
            *v = java_round(value * 255.0);
        }
        lut
    })
}

/// `Math.round(double)`.
fn java_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

/// `Any2sRGBRed.applyLut_INT`.
fn apply_lut(wr: &mut Raster, lut: &[i32; 256]) {
    for p in &mut wr.data {
        let pix = *p as u32;
        *p = ((pix & 0xFF00_0000) as i32) | (lut[((pix >> 16) & 0xFF) as usize] << 16) | (lut[((pix >> 8) & 0xFF) as usize] << 8) | lut[(pix & 0xFF) as usize];
    }
}

/// `GraphicsUtil.divide_INT_PACK_Data`.
fn divide_int_pack(wr: &mut Raster) {
    for p in &mut wr.data {
        let pixel = *p;
        let a = (pixel as u32 >> 24) as i32;
        if a <= 0 {
            *p = 0x00FF_FFFF;
        } else if a < 255 {
            let a_fp = 0x00FF_0000 / a;
            *p = (a << 24) | ((((pixel & 0xFF0000) >> 16).wrapping_mul(a_fp) & 0xFF0000) as i32) | (((((pixel & 0x00FF00) >> 8).wrapping_mul(a_fp) & 0xFF0000) >> 8) as i32) | ((((pixel & 0x0000FF).wrapping_mul(a_fp) & 0xFF0000) >> 16) as i32);
        }
    }
}

/// `GraphicsUtil.mult_INT_PACK_Data`.
fn mult_int_pack(wr: &mut Raster) {
    for p in &mut wr.data {
        let pixel = *p;
        let a = (pixel as u32 >> 24) as i32;
        if a < 255 {
            *p = (a << 24) | ((((pixel & 0xFF0000).wrapping_mul(a)) >> 8) & 0xFF0000) | ((((pixel & 0x00FF00) * a) >> 8) & 0x00FF00) | ((((pixel & 0x0000FF) * a) >> 8) & 0x0000FF);
        }
    }
}

impl<'a> Red<'a> {
    fn graphics_node(node: &'a GraphicsNode, node2dev: AffineTransform, hints: RenderingHints) -> Red<'a> {
        let bounds2d = node.primitive_bounds_after_get_bounds().unwrap_or_else(|| Rectangle2D::float(0.0, 0.0, 1.0, 1.0));
        let bounds = get_bounds(&create_transformed_shape(&node2dev, &Shape::Rect(bounds2d)).get_bounds2d());
        let def = DEFAULT_TILE_SIZE;
        let tg_x = def * (bounds.x / def);
        let tg_y = def * (bounds.y / def);
        let mut tw = bounds.x.wrapping_add(bounds.w).wrapping_sub(tg_x);
        if tw > def {
            tw = def;
        }
        let mut th = bounds.y.wrapping_add(bounds.h).wrapping_sub(tg_y);
        if th > def {
            th = def;
        }
        if tw <= 0 || th <= 0 {
            tw = 1;
            th = 1;
        }
        Red::init(RedKind::GraphicsNode { node, node2dev, hints }, SRGB_UNPRE, bounds, tw, th, tg_x, tg_y)
    }

    fn empty_1x1() -> Red<'a> {
        Red::init(RedKind::Empty1x1, SRGB_UNPRE, JRect { x: 0, y: 0, w: 1, h: 1 }, 1, 1, 0, 0)
    }

    /// `new PadRed(src, bounds, ZERO_PAD, hints)` with `fixSampleModel`.
    fn pad(src: Red<'a>, bounds: JRect) -> Red<'a> {
        let def = DEFAULT_TILE_SIZE;
        let mut w = src.tile_width;
        if w < def {
            w = def;
        }
        if w > bounds.w {
            w = bounds.w;
        }
        let mut h = src.tile_height;
        if h < def {
            h = def;
        }
        if h > bounds.h {
            h = bounds.h;
        }
        let cm = src.cm;
        Red::init(RedKind::Pad(Box::new(src)), cm, bounds, w, h, bounds.x, bounds.y)
    }

    /// `GraphicsUtil.convertToLsRGB(src)`.
    fn to_linear(src: Red<'a>) -> Red<'a> {
        if src.cm.linear {
            return src;
        }
        let (b, tw, th, gx, gy) = (src.bounds, src.tile_width, src.tile_height, src.tile_grid_x_off, src.tile_grid_y_off);
        Red::init(RedKind::ToLinear(Box::new(src)), Cm { linear: true, premult: false }, b, tw, th, gx, gy)
    }

    /// `GraphicsUtil.convertTosRGB(src)`.
    fn to_srgb(src: Red<'a>) -> Red<'a> {
        if !src.cm.linear {
            return src;
        }
        let (b, tw, th, gx, gy) = (src.bounds, src.tile_width, src.tile_height, src.tile_grid_x_off, src.tile_grid_y_off);
        Red::init(RedKind::ToSrgb(Box::new(src)), SRGB_UNPRE, b, tw, th, gx, gy)
    }

    /// `FormatRed.construct(src, sRGB_Unpre)`.
    fn format_unpre(src: Red<'a>) -> Red<'a> {
        if !src.cm.premult {
            return src;
        }
        let (b, tw, th, gx, gy) = (src.bounds, src.tile_width, src.tile_height, src.tile_grid_x_off, src.tile_grid_y_off);
        let cm = Cm { linear: src.cm.linear, premult: false };
        Red::init(RedKind::Unpremultiply(Box::new(src)), cm, b, tw, th, gx, gy)
    }

    /// `new GaussianBlurRed8Bit(src, stdDevX, stdDevY, hints)`.
    fn blur(src: Red<'a>, std_dev_x: f64, std_dev_y: f64, hints: &RenderingHints) -> Result<Red<'a>, Unsupported> {
        let xinset = surround_pixels(std_dev_x, hints);
        let yinset = surround_pixels(std_dev_y, hints);
        let mut b = src.bounds;
        b.x += xinset;
        b.y += yinset;
        b.w -= 2 * xinset;
        b.h -= 2 * yinset;
        if b.w <= 0 || b.h <= 0 {
            b.w = 0;
            b.h = 0;
        }
        let mut tw = src.tile_width;
        let mut th = src.tile_height;
        if tw > b.w {
            tw = b.w;
        }
        if th > b.h {
            th = b.h;
        }
        if tw <= 0 || th <= 0 {
            return Err(Unsupported(format!("Width ({tw}) and height ({th}) must be > 0")));
        }
        let high_quality = hints.get(KEY_RENDERING).is_some_and(|v| *v == "VALUE_RENDER_QUALITY");
        let mut conv = [None, None];
        let (mut dx, mut dy) = (0, 0);
        if xinset != 0 && (std_dev_x < 2.0 || high_quality) {
            conv[0] = Some(quality_kernel(xinset * 2 + 1, std_dev_x));
        } else {
            dx = (red_dsqrt2pi() as f64 * std_dev_x + 0.5f32 as f64).floor() as i32;
        }
        if yinset != 0 && (std_dev_y < 2.0 || high_quality) {
            conv[1] = Some(quality_kernel(yinset * 2 + 1, std_dev_y));
        } else {
            dy = (red_dsqrt2pi() as f64 * std_dev_y + 0.5f32 as f64).floor() as i32;
        }
        let cm = Cm { linear: src.cm.linear, premult: true };
        let (gx, gy) = (src.tile_grid_x_off + xinset, src.tile_grid_y_off + yinset);
        Ok(Red::init(RedKind::Blur { src: Box::new(src), xinset, yinset, conv, dx, dy }, cm, b, tw, th, gx, gy))
    }

    /// `new AffineRed(src, src2me, hints)`.
    fn affine(src: Red<'a>, src2me: AffineTransform, hints: RenderingHints) -> Red<'a> {
        let bounds = get_bounds(&create_transformed_shape(&src2me, &Shape::Rect(src.bounds.to_rect2d())).get_bounds2d());
        let cm = Cm { linear: src.cm.linear, premult: true };
        let def = DEFAULT_TILE_SIZE;
        let mut w = src.tile_width.max(def).min(bounds.w);
        let mut h = src.tile_height.max(def).min(bounds.h);
        if w <= 0 || h <= 0 {
            w = 1;
            h = 1;
        }
        let (px, py) = src2me.transform_point(src.tile_grid_x_off as f32 as f64, src.tile_grid_y_off as f32 as f64);
        let (gx, gy) = (px as f32 as i32, py as f32 as i32);
        Red::init(RedKind::Affine { src: Box::new(src), src2me, hints }, cm, bounds, w, h, gx, gy)
    }

    /// `new TranslateRed(src, xloc, yloc)`.
    fn translate(src: Red<'a>, xloc: i32, yloc: i32) -> Red<'a> {
        let (dx, dy) = (xloc - src.bounds.x, yloc - src.bounds.y);
        let bounds = JRect { x: xloc, y: yloc, w: src.bounds.w, h: src.bounds.h };
        let (cm, tw, th, gx, gy) = (src.cm, src.tile_width, src.tile_height, src.tile_grid_x_off + dx, src.tile_grid_y_off + dy);
        Red::init(RedKind::Translate { src: Box::new(src), dx, dy }, cm, bounds, tw, th, gx, gy)
    }

    /// `AbstractRed.init` with `updateTileGridInfo`.
    fn init(kind: RedKind<'a>, cm: Cm, bounds: JRect, tile_width: i32, tile_height: i32, tile_grid_x_off: i32, tile_grid_y_off: i32) -> Red<'a> {
        let mut r = Red { kind, cm, bounds, tile_grid_x_off, tile_grid_y_off, tile_width, tile_height, min_tile_x: 0, min_tile_y: 0, num_x_tiles: 0, num_y_tiles: 0 };
        if tile_width > 0 && tile_height > 0 {
            r.min_tile_x = r.get_x_tile(bounds.x);
            r.min_tile_y = r.get_y_tile(bounds.y);
            r.num_x_tiles = r.get_x_tile(bounds.x + bounds.w - 1) - r.min_tile_x + 1;
            r.num_y_tiles = r.get_y_tile(bounds.y + bounds.h - 1) - r.min_tile_y + 1;
        }
        r
    }

    fn get_x_tile(&self, xloc: i32) -> i32 {
        let tgx = xloc - self.tile_grid_x_off;
        if tgx >= 0 {
            tgx / self.tile_width
        } else {
            (tgx - self.tile_width + 1) / self.tile_width
        }
    }

    fn get_y_tile(&self, yloc: i32) -> i32 {
        let tgy = yloc - self.tile_grid_y_off;
        if tgy >= 0 {
            tgy / self.tile_height
        } else {
            (tgy - self.tile_height + 1) / self.tile_height
        }
    }

    /// `getData(rect)`.
    fn get_data(&self, r: JRect, outer: &mut dyn Graphics2D) -> Result<Raster, Unsupported> {
        let mut wr = Raster::new(r.x, r.y, r.w, r.h);
        self.copy_data(&mut wr, outer)?;
        Ok(wr)
    }

    /// `copyData(wr)`.
    fn copy_data(&self, wr: &mut Raster, outer: &mut dyn Graphics2D) -> Result<(), Unsupported> {
        match &self.kind {
            RedKind::GraphicsNode { node, node2dev, hints } => {
                let mut g = SunGraphics2D::new(IntImage::new(wr.w, wr.h));
                g.add_rendering_hints(hints);
                g.clip_int_rect(0, 0, wr.w, wr.h);
                g.clear_rect(0, 0, wr.w, wr.h);
                g.set_composite(AlphaComposite::SRC_OVER);
                g.translate(-wr.x as f64, -wr.y as f64);
                g.transform(node2dev);
                let r = node.primitive_paint(&mut g);
                if let Some(u) = g.unsupported.take() {
                    outer.record_unsupported(&u);
                }
                if let Some(e) = g.exception.take() {
                    return Err(Unsupported(e.split_once(": ").map(|(_, m)| m.to_string()).unwrap_or(e)));
                }
                r?;
                wr.data = g.surface.data;
            }
            RedKind::Empty1x1 => {
                let src = Raster::new(0, 0, 1, 1);
                wr.copy_from(&src);
            }
            RedKind::Pad(src) => {
                let src_r = src.bounds;
                let wr_r = wr.bounds();
                if wr_r.intersects(&src_r) {
                    let r = wr_r.intersection(&src_r);
                    let mut child = wr.child(r);
                    src.copy_data(&mut child, outer)?;
                    wr.copy_from(&child);
                }
                handle_zero(wr, src_r);
            }
            RedKind::ToLinear(src) => {
                src.copy_data(wr, outer)?;
                if src.cm.premult {
                    divide_int_pack(wr);
                }
                apply_lut(wr, srgb_to_lsrgb_lut());
            }
            RedKind::ToSrgb(src) => {
                src.copy_data(wr, outer)?;
                if src.cm.premult {
                    divide_int_pack(wr);
                }
                apply_lut(wr, linear_to_srgb_lut());
            }
            RedKind::Unpremultiply(src) => {
                let mut s = Raster::new(wr.x, wr.y, wr.w, wr.h);
                src.copy_data(&mut s, outer)?;
                for (d, &p) in wr.data.iter_mut().zip(&s.data) {
                    let a = (p as u32 >> 24) as i32;
                    if a <= 0 || a >= 255 {
                        *d = p;
                    } else {
                        let ialpha = 0x00FF_0000 / a;
                        let c = |v: i32| ((v.wrapping_mul(ialpha) as u32).wrapping_add(1 << 15) >> 16) as i32;
                        *d = (a << 24) | ((c((p >> 16) & 0xFF) & 0xFF) << 16) | ((c((p >> 8) & 0xFF) & 0xFF) << 8) | (c(p & 0xFF) & 0xFF);
                    }
                }
            }
            RedKind::Affine { src, src2me, hints } => {
                wr.data.iter_mut().for_each(|p| *p = 0);
                let Ok(me2src) = src2me.create_inverse() else { return Ok(()) };
                let mut src_r = get_bounds(&create_transformed_shape(&me2src, &Shape::Rect(wr.bounds().to_rect2d())).get_bounds2d());
                src_r = JRect { x: src_r.x - 1, y: src_r.y - 1, w: src_r.w + 2, h: src_r.h + 2 };
                if !src_r.intersects(&src.bounds) {
                    return Ok(());
                }
                let mut src_ras = src.get_data(src_r.intersection(&src.bounds), outer)?;
                let mut aff = *src2me;
                aff.concatenate(&AffineTransform::get_translate_instance(src_ras.x as f64, src_ras.y as f64));
                let (spx, spy) = me2src.transform_point(wr.x as f32 as f64, wr.y as f32 as f64);
                let (spx, spy) = (spx as f32 as f64, spy as f32 as f64);
                let (dpx, dpy) = aff.transform_point(spx - src_ras.x as f64, spy - src_ras.y as f64);
                aff.pre_concatenate(&AffineTransform::get_translate_instance(-dpx, -dpy));
                if !src.cm.premult {
                    mult_int_pack(&mut src_ras);
                }
                if src.cm.linear {
                    return Err(Unsupported("Unable to transform src image".into()));
                }
                let interp = hints.get(KEY_INTERPOLATION).copied();
                let rendering = hints.get(KEY_RENDERING).copied();
                let nearest = match interp {
                    Some("VALUE_INTERPOLATION_BILINEAR") | Some("VALUE_INTERPOLATION_BICUBIC") => false,
                    Some(_) => true,
                    None => rendering != Some("VALUE_RENDER_QUALITY"),
                };
                if !nearest {
                    outer.record_unsupported("a filter result resampled with interpolation");
                    return Ok(());
                }
                let m = [aff.get_scale_x(), aff.get_shear_y(), aff.get_shear_x(), aff.get_scale_y(), aff.get_translate_x(), aff.get_translate_y()];
                transform_nearest(&src_ras.data, src_ras.w as usize, src_ras.h as usize, &mut wr.data, wr.w as usize, wr.h as usize, m);
            }
            RedKind::Translate { src, dx, dy } => {
                let mut wr2 = Raster { x: wr.x - dx, y: wr.y - dy, w: wr.w, h: wr.h, data: std::mem::take(&mut wr.data) };
                src.copy_data(&mut wr2, outer)?;
                wr.data = wr2.data;
            }
            RedKind::Blur { src, xinset, yinset, conv, dx, dy } => {
                let (xinset, yinset, dx, dy) = (*xinset, *yinset, *dx, *dy);
                let r = JRect { x: wr.x - xinset, y: wr.y - yinset, w: wr.w + 2 * xinset, h: wr.h + 2 * yinset };
                let mut tmp1 = Raster::new(r.x, r.y, r.w, r.h);
                src.copy_data(&mut tmp1, outer)?;
                if !src.cm.premult {
                    mult_int_pack(&mut tmp1);
                }
                let (w, h) = (r.w as usize, r.h as usize);
                let mut tmp2: Option<Vec<i32>> = None;
                let skip_x;
                if xinset == 0 {
                    skip_x = 0;
                } else if let Some(k) = &conv[0] {
                    let mut out = vec![0i32; w * h];
                    convolve_raster(&tmp1.data, &mut out, w, h, k.len(), 1, k).map_err(|e| Unsupported(e.to_string()))?;
                    skip_x = (k.len() as i32 - 1) / 2;
                    tmp2 = Some(std::mem::replace(&mut tmp1.data, out));
                } else if dx & 1 == 0 {
                    box_filter_h(&mut tmp1.data, w, h, 0, 0, dx, dx / 2);
                    box_filter_h(&mut tmp1.data, w, h, dx / 2, 0, dx, dx / 2 - 1);
                    box_filter_h(&mut tmp1.data, w, h, dx - 1, 0, dx + 1, dx / 2);
                    skip_x = dx - 1 + dx / 2;
                } else {
                    box_filter_h(&mut tmp1.data, w, h, 0, 0, dx, dx / 2);
                    box_filter_h(&mut tmp1.data, w, h, dx / 2, 0, dx, dx / 2);
                    box_filter_h(&mut tmp1.data, w, h, dx - 2, 0, dx, dx / 2);
                    skip_x = dx - 2 + dx / 2;
                }
                let result = if yinset == 0 {
                    tmp1.data
                } else if let Some(k) = &conv[1] {
                    let mut out = tmp2.unwrap_or_else(|| vec![0i32; w * h]);
                    convolve_raster(&tmp1.data, &mut out, w, h, 1, k.len(), k).map_err(|e| Unsupported(e.to_string()))?;
                    out
                } else {
                    let mut d = tmp1.data;
                    if dy & 1 == 0 {
                        box_filter_v(&mut d, w, h, skip_x, 0, dy, dy / 2);
                        box_filter_v(&mut d, w, h, skip_x, dy / 2, dy, dy / 2 - 1);
                        box_filter_v(&mut d, w, h, skip_x, dy - 1, dy + 1, dy / 2);
                    } else {
                        box_filter_v(&mut d, w, h, skip_x, 0, dy, dy / 2);
                        box_filter_v(&mut d, w, h, skip_x, dy / 2, dy, dy / 2);
                        box_filter_v(&mut d, w, h, skip_x, dy - 2, dy, dy / 2);
                    }
                    d
                };
                wr.copy_from(&Raster { x: r.x, y: r.y, w: r.w, h: r.h, data: result });
            }
        }
        Ok(())
    }
}

/// `PadRed.handleZero(wr)`: everything outside the source bounds.
fn handle_zero(wr: &mut Raster, src_r: JRect) {
    let wr_r = wr.bounds();
    let mut ar = wr_r;
    let mut dr = wr_r;
    if ar.x < src_r.x {
        let mut w = src_r.x - ar.x;
        if w > ar.w {
            w = ar.w;
        }
        dr.w = w;
        wr.zero_rect(dr);
        ar.x += w;
        ar.w -= w;
    }
    if ar.y < src_r.y {
        let mut h = src_r.y - ar.y;
        if h > ar.h {
            h = ar.h;
        }
        dr = JRect { x: ar.x, y: ar.y, w: ar.w, h };
        wr.zero_rect(dr);
        ar.y += h;
        ar.h -= h;
    }
    if ar.y + ar.h > src_r.y + src_r.h {
        let mut h = (ar.y + ar.h) - (src_r.y + src_r.h);
        if h > ar.h {
            h = ar.h;
        }
        dr = JRect { x: ar.x, y: ar.y + ar.h - h, w: ar.w, h };
        wr.zero_rect(dr);
        ar.h -= h;
    }
    if ar.x + ar.w > src_r.x + src_r.w {
        let mut w = (ar.x + ar.w) - (src_r.x + src_r.w);
        if w > ar.w {
            w = ar.w;
        }
        dr = JRect { x: ar.x + ar.w - w, y: ar.y, w, h: ar.h };
        wr.zero_rect(dr);
    }
}

fn box_pixel(sum_a: i32, sum_r: i32, sum_g: i32, sum_b: i32, scale: i32) -> i32 {
    (sum_a.wrapping_mul(scale) & 0xFF00_0000u32 as i32)
        | (((sum_r.wrapping_mul(scale) & 0xFF00_0000u32 as i32) as u32 >> 8) as i32)
        | (((sum_g.wrapping_mul(scale) & 0xFF00_0000u32 as i32) as u32 >> 16) as i32)
        | (((sum_b.wrapping_mul(scale) & 0xFF00_0000u32 as i32) as u32 >> 24) as i32)
}

fn add(p: i32, s: &mut [i32; 4], sign: i32) {
    s[0] += sign * ((p as u32 >> 24) as i32);
    s[1] += sign * ((p >> 16) & 0xFF);
    s[2] += sign * ((p >> 8) & 0xFF);
    s[3] += sign * (p & 0xFF);
}

/// `GaussianBlurRed8Bit.boxFilterH(src, dest, skipX, skipY, boxSz, loc)` in place.
fn box_filter_h(px: &mut [i32], w: usize, h: usize, skip_x: i32, skip_y: i32, box_sz: i32, loc: i32) {
    let (wi, hi) = (w as i32, h as i32);
    if wi < 2 * skip_x + box_sz || hi < 2 * skip_y {
        return;
    }
    let mut buffer = vec![0i32; box_sz as usize];
    let scale = (1 << 24) / box_sz;
    for y in skip_y..hi - skip_y {
        let mut sp = (y * wi) as usize;
        let mut dp = sp;
        let row_end = sp + (wi - skip_x) as usize;
        let mut k = 0usize;
        let mut s = [0i32; 4];
        sp += skip_x as usize;
        let end = sp + box_sz as usize;
        while sp < end {
            let curr = px[sp];
            buffer[k] = curr;
            add(curr, &mut s, 1);
            k += 1;
            sp += 1;
        }
        dp += (skip_x + loc) as usize;
        let mut prev = box_pixel(s[0], s[1], s[2], s[3], scale);
        px[dp] = prev;
        dp += 1;
        k = 0;
        while sp < row_end {
            let curr = buffer[k];
            if curr == px[sp] {
                px[dp] = prev;
            } else {
                add(curr, &mut s, -1);
                let c = px[sp];
                buffer[k] = c;
                add(c, &mut s, 1);
                prev = box_pixel(s[0], s[1], s[2], s[3], scale);
                px[dp] = prev;
            }
            k = (k + 1) % box_sz as usize;
            sp += 1;
            dp += 1;
        }
    }
}

/// `GaussianBlurRed8Bit.boxFilterV(src, dest, skipX, skipY, boxSz, loc)` in place.
fn box_filter_v(px: &mut [i32], w: usize, h: usize, skip_x: i32, skip_y: i32, box_sz: i32, loc: i32) {
    let (wi, hi) = (w as i32, h as i32);
    if wi < 2 * skip_x || hi < 2 * skip_y + box_sz {
        return;
    }
    let mut buffer = vec![0i32; box_sz as usize];
    let scale = (1 << 24) / box_sz;
    let stride = w;
    for x in skip_x..wi - skip_x {
        let mut sp = x as usize;
        let mut dp = sp;
        let col_end = sp + (hi - skip_y) as usize * stride;
        let mut k = 0usize;
        let mut s = [0i32; 4];
        sp += skip_y as usize * stride;
        let end = sp + box_sz as usize * stride;
        while sp < end {
            let curr = px[sp];
            buffer[k] = curr;
            add(curr, &mut s, 1);
            k += 1;
            sp += stride;
        }
        dp += (skip_y + loc) as usize * stride;
        let mut prev = box_pixel(s[0], s[1], s[2], s[3], scale);
        px[dp] = prev;
        dp += stride;
        k = 0;
        while sp < col_end {
            let curr = buffer[k];
            if curr == px[sp] {
                px[dp] = prev;
            } else {
                add(curr, &mut s, -1);
                let c = px[sp];
                buffer[k] = c;
                add(c, &mut s, 1);
                prev = box_pixel(s[0], s[1], s[2], s[3], scale);
                px[dp] = prev;
            }
            k = (k + 1) % box_sz as usize;
            sp += stride;
            dp += stride;
        }
    }
}

/// `GraphicsUtil.drawImage(g2d, filter)`.
pub fn draw_image_filter(g2d: &mut dyn Graphics2D, filter: &Filter) -> Result<(), Unsupported> {
    if filter.paint_rable(g2d)? {
        return Ok(());
    }
    let at = g2d.get_transform();
    let clip = g2d.get_clip().map(|c| c.to_shape());
    let hints = g2d.get_rendering_hints();
    let Some(ri) = filter.create_rendering(&at, clip.as_ref(), &hints, g2d)? else { return Ok(()) };
    g2d.set_transform(&AffineTransform::new());
    let r = draw_image_red(g2d, ri);
    g2d.set_transform(&at);
    r
}

/// `GraphicsUtil.drawImage(g2d, cr)` for a rendering already in device space.
fn draw_image_red(g2d: &mut dyn Graphics2D, cr: Red) -> Result<(), Unsupported> {
    let g2d_at = g2d.get_transform();
    let composite = g2d.get_composite();
    let r = draw_image_red_inner(g2d, cr);
    g2d.set_transform(&g2d_at);
    g2d.set_composite(composite);
    r
}

fn draw_image_red_inner(g2d: &mut dyn Graphics2D, mut cr: Red) -> Result<(), Unsupported> {
    let mut at: Option<AffineTransform> = None;
    loop {
        match cr.kind {
            RedKind::Affine { src, src2me, .. } => {
                match &mut at {
                    None => at = Some(src2me),
                    Some(a) => a.concatenate(&src2me),
                }
                cr = *src;
            }
            RedKind::Translate { src, dx, dy } => {
                match &mut at {
                    None => at = Some(AffineTransform::get_translate_instance(dx as f64, dy as f64)),
                    Some(a) => a.translate(dx as f64, dy as f64),
                }
                cr = *src;
            }
            other => {
                cr.kind = other;
                break;
            }
        }
    }
    let g2d_at = g2d.get_transform();
    let at = match at {
        Some(mut a) if !a.is_identity() => {
            a.pre_concatenate(&g2d_at);
            a
        }
        _ => g2d_at,
    };
    let det = get_determinant(&at);
    if !at.is_identity() && det <= 1.0 {
        if at.get_type() != TYPE_TRANSLATION {
            g2d.record_unsupported("an offscreen image resampled through AffineTransformOp");
            return Ok(());
        }
        let xloc = cr.bounds.x.wrapping_add(at.get_translate_x() as i32);
        let yloc = cr.bounds.y.wrapping_add(at.get_translate_y() as i32);
        cr = Red::translate(cr, xloc, yloc);
    }
    let cr = Red::format_unpre(Red::to_srgb(cr));
    if !at.is_identity() && det > 1.0 {
        g2d.record_unsupported("an offscreen image resampled through AffineTransformOp");
        return Ok(());
    }
    g2d.set_transform(&AffineTransform::new());
    let cr_r = cr.bounds;
    let mut clip_r = match g2d.get_clip() {
        None => cr_r,
        Some(c) => {
            let c = get_bounds(&c.to_shape().get_bounds2d());
            if !c.intersects(&cr_r) {
                return Ok(());
            }
            c.intersection(&cr_r)
        }
    };
    let (dw, dh) = g2d.destination_size();
    let gc_r = JRect { x: 0, y: 0, w: dw, h: dh };
    if !clip_r.intersects(&gc_r) {
        return Ok(());
    }
    clip_r = clip_r.intersection(&gc_r);
    let (tw, th) = (cr.tile_width, cr.tile_height);
    if tw as i64 * th as i64 >= clip_r.w as i64 * clip_r.h as i64 {
        let wr = cr.get_data(clip_r, g2d)?;
        g2d.draw_image(&IntImage { width: wr.w, height: wr.h, data: wr.data, opaque: false }, clip_r.x, clip_r.y);
        return Ok(());
    }
    let mut xt0 = cr.min_tile_x;
    let xt1 = xt0 + cr.num_x_tiles;
    let mut yt0 = cr.min_tile_y;
    let yt1 = yt0 + cr.num_y_tiles;
    let mut yloc = yt0 * th + cr.tile_grid_y_off;
    let mut skip = (clip_r.y - yloc) / th;
    if skip < 0 {
        skip = 0;
    }
    yt0 += skip;
    let mut xloc = xt0 * tw + cr.tile_grid_x_off;
    skip = (clip_r.x - xloc) / tw;
    if skip < 0 {
        skip = 0;
    }
    xt0 += skip;
    let end_x = clip_r.x + clip_r.w - 1;
    let end_y = clip_r.y + clip_r.h - 1;
    yloc = yt0 * th + cr.tile_grid_y_off;
    let min_x = xt0 * tw + cr.tile_grid_x_off;
    let mut x_step = tw;
    xloc = min_x;
    let mut y = yt0;
    while y < yt1 {
        if yloc > end_y {
            break;
        }
        let mut x = xt0;
        while x < xt1 {
            if xloc < min_x || xloc > end_x {
                break;
            }
            let t_r = JRect { x: xloc, y: yloc, w: tw, h: th };
            let i_r = intersect_jrect(&cr_r, &t_r);
            let mut wr = Raster::new(i_r.x, i_r.y, i_r.w, i_r.h);
            cr.copy_data(&mut wr, g2d)?;
            g2d.draw_image(&IntImage { width: wr.w, height: wr.h, data: wr.data, opaque: false }, i_r.x, i_r.y);
            x += 1;
            xloc += x_step;
        }
        x_step = -x_step;
        xloc += x_step;
        y += 1;
        yloc += th;
    }
    Ok(())
}

/// `Rectangle2D.intersect(crR, tR, iR)` into a `java.awt.Rectangle` (`setFrameFromDiagonal`).
fn intersect_jrect(a: &JRect, b: &JRect) -> JRect {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.w).min(b.x + b.w);
    let y2 = (a.y + a.h).min(b.y + b.h);
    let (x1, x2) = if x2 < x1 { (x2, x1) } else { (x1, x2) };
    let (y1, y2) = if y2 < y1 { (y2, y1) } else { (y1, y2) };
    JRect { x: x1, y: y1, w: x2 - x1, h: y2 - y1 }
}
