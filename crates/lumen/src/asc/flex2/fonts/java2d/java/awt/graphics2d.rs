//! `java.awt.Graphics2D`: the calls Batik's graphics nodes, shape painters and `GraphicsUtil` make, implemented by
//! swfutils' `SpriteGraphics2D` and by Java2D's `SunGraphics2D` (an offscreen raster).

use super::geom::affine_transform::AffineTransform;
use super::geom::shapes::Shape;
use super::paint::{AlphaComposite, BasicStroke, Paint};
use crate::asc::flex2::flash::graphics::g2d::graphic_context::RenderingHints;
use crate::asc::flex2::fonts::java2d::sun::java2d::sun_graphics2d::{IntImage, UsrClip};

pub trait Graphics2D {
    fn get_clip(&self) -> Option<UsrClip>;
    fn set_clip(&mut self, c: Option<&UsrClip>);
    fn clip(&mut self, s: &Shape);
    fn get_composite(&self) -> AlphaComposite;
    fn set_composite(&mut self, c: AlphaComposite);
    fn get_transform(&self) -> AffineTransform;
    fn set_transform(&mut self, t: &AffineTransform);
    fn transform(&mut self, t: &AffineTransform);
    fn get_rendering_hints(&self) -> RenderingHints;
    fn add_rendering_hints(&mut self, h: &RenderingHints);
    fn set_rendering_hints(&mut self, h: RenderingHints);
    fn set_paint(&mut self, p: Option<Paint>);
    fn set_stroke(&mut self, s: BasicStroke);
    fn fill(&mut self, s: &Shape);
    fn draw(&mut self, s: &Shape);
    /// `drawImage(img, x, y, null)` of a `TYPE_INT_ARGB` image.
    fn draw_image(&mut self, img: &IntImage, x: i32, y: i32);
    /// `GraphicsUtil.getDestination(g2d)`'s size: the image a `KEY_BUFFERED_IMAGE` hint names.
    fn destination_size(&self) -> (i32, i32);
    /// Notes something this port does not render; the transcoding then fails with it.
    fn record_unsupported(&mut self, what: &str);
}
