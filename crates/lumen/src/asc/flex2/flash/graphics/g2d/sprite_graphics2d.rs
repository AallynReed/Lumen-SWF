//! Port of swfutils `flash.graphics.g2d.SpriteGraphics2D` (with the `AbstractGraphics2D` methods it inherits):
//! each `draw` or `fill` becomes a DefineShape3 placed by a PlaceObject2.

use super::graphic_context::GraphicContext;
use crate::asc::flex2::flash::swf::builder::define_shape_builder::build_define_shape;
use crate::asc::flex2::flash::swf::builder::fill_style_builder::{build_matrix, get_translate_instance};
use crate::asc::flex2::flash::swf::tags::{codes, Body, PlaceObject, Tag, TagRef};
use crate::asc::flex2::flash::swf::types::Rect;
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::{create_transformed_shape, Shape};
use crate::asc::flex2::fonts::java2d::java::awt::paint::Paint;

pub struct SpriteGraphics2D {
    pub graphic_context: GraphicContext,
    pub define_tags: Vec<TagRef>,
    pub depth: i32,
    pub width: i32,
    pub height: i32,
    pub bounds: Rect,
    /// What the painted tree held that this port does not render, if anything.
    pub unsupported: Option<String>,
}

impl SpriteGraphics2D {
    pub fn new(width: i32, height: i32) -> SpriteGraphics2D {
        SpriteGraphics2D { graphic_context: GraphicContext::new(), define_tags: Vec::new(), depth: 1, width, height, bounds: Rect::default(), unsupported: None }
    }

    pub fn draw(&mut self, shape: &Shape) {
        self.define_shape(shape, true, false);
    }

    pub fn fill(&mut self, shape: &Shape) {
        self.define_shape(shape, false, true);
    }

    fn define_shape(&mut self, shape: &Shape, draw: bool, fill: bool) {
        let is_gradient = fill && is_gradient_fill(self.graphic_context.get_paint());
        let mut old_pen = None;
        let transformed;
        let shape = if is_gradient {
            transformed = Shape::Path(create_transformed_shape(&self.graphic_context.get_transform(), shape));
            old_pen = Some(self.graphic_context.set_pen(get_shape_start(&transformed)));
            &transformed
        } else {
            shape
        };
        let ds3 = build_define_shape(shape, &self.graphic_context, draw, fill);
        self.define_tags.push(ds3.clone());
        let matrix = if is_gradient {
            let (ox, oy) = self.graphic_context.get_pen();
            get_translate_instance(ox, oy)
        } else {
            build_matrix(&self.graphic_context.get_transform())
        };
        let b = match &ds3.borrow().body {
            Body::DefineShape(d) => d.bounds,
            _ => unreachable!(),
        };
        self.apply_bounds(
            b.x_min.wrapping_add(matrix.translate_x),
            b.y_min.wrapping_add(matrix.translate_y),
            b.x_max.wrapping_add(matrix.translate_x),
            b.y_max.wrapping_add(matrix.translate_y),
        );
        let mut po2 = PlaceObject::default();
        po2.set_matrix(Some(matrix));
        po2.set_ref(ds3);
        po2.depth = self.depth;
        self.depth += 1;
        if let Some(p) = old_pen {
            self.graphic_context.set_pen(p);
        }
        self.define_tags.push(Tag::new_ref(codes::PLACE_OBJECT2, Body::PlaceObject(po2)));
    }

    pub fn get_tags(&self) -> &[TagRef] {
        &self.define_tags
    }

    fn apply_bounds(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        let b = &mut self.bounds;
        if x1 < b.x_min {
            b.x_min = x1;
        }
        if y1 < b.y_min {
            b.y_min = y1;
        }
        if x1 > b.x_max {
            b.x_max = x1;
        }
        if y1 > b.y_max {
            b.y_max = y1;
        }
        if x2 < b.x_min {
            b.x_min = x2;
        }
        if y2 < b.y_min {
            b.y_min = y2;
        }
        if x2 > b.x_max {
            b.x_max = x2;
        }
        if y2 > b.y_max {
            b.y_max = y2;
        }
    }

    pub fn get_bounds(&self) -> Rect {
        self.bounds
    }
}

/// `isGradientFill(paint)`: only `java.awt.GradientPaint` (or a class literally named `MultipleGradientPaint`)
/// passes, so Batik's gradient paints never do.
fn is_gradient_fill(_paint: Option<&Paint>) -> bool {
    false
}

fn get_shape_start(shape: &Shape) -> (f64, f64) {
    let b = shape.get_bounds2d();
    (b.get_min_x(), b.get_min_y())
}
