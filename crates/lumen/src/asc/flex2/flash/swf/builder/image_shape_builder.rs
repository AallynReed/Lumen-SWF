//! Port of swfutils `flash.swf.builder.tags.ImageShapeBuilder`.

use super::shape_builder::{Point, ShapeBuilder};
use crate::asc::flex2::flash::swf::tags::{codes, Body, DefineShape, Tag, TagRef};
use crate::asc::flex2::flash::swf::types::{FillStyle, Matrix, Rect, ShapeWithStyle, FILL_BITS, FILL_BITS_CLIP};

/// `ImageShapeBuilder.buildImage(tag, width, height)`: a DefineShape3 filled with the bitmap at twips scale.
pub fn build_image(tag: &TagRef, width: i32, height: i32) -> TagRef {
    let mut matrix = Matrix::default();
    matrix.set_scale(20.0, 20.0);
    let fs = FillStyle::new(FILL_BITS | FILL_BITS_CLIP, matrix, tag.clone());
    let mut builder = ShapeBuilder::with_origin(Point::new(0.0, 0.0));
    builder.set_use_fill_style0(true);
    builder.set_current_fill_style0(1);
    builder.move_to(0.0, 0.0);
    builder.straight(width as f64, 0.0);
    builder.straight(width as f64, height as f64);
    builder.straight(0.0, height as f64);
    builder.straight(0.0, 0.0);
    builder.correct_rounding_errors();
    let sws = ShapeWithStyle { shape_records: builder.build().shape_records, fillstyles: Some(vec![fs]), linestyles: Some(Vec::new()) };
    Tag::new_ref(
        codes::DEFINE_SHAPE3,
        Body::DefineShape(DefineShape { bounds: Rect::new(0, width * 20, 0, height * 20), shape_with_style: sws, uses_fill_winding_rule: false, uses_non_scaling_strokes: false, uses_scaling_strokes: false, edge_bounds: None, scaling_grid: None }),
    )
}
