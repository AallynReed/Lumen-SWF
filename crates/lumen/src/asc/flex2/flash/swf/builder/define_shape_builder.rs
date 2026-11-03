//! Ports of swfutils `flash.swf.builder.tags.DefineShapeBuilder` and `flash.swf.builder.types.ShapeWithStyleBuilder`.

use super::fill_style_builder::{build_fill_style, build_line_style};
use super::shape_builder::{PathIteratorWrapper, Point, ShapeBuilder};
use crate::asc::flex2::flash::graphics::g2d::graphic_context::GraphicContext;
use crate::asc::flex2::flash::swf::tags::{codes, Body, DefineShape, Tag, TagRef};
use crate::asc::flex2::flash::swf::types::{FillStyle, LineStyle, Rect, ShapeRecord, ShapeWithStyle};
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::Shape;

pub struct ShapeWithStyleBuilder {
    builder: ShapeBuilder,
    linestyles: Vec<LineStyle>,
    fillstyles: Vec<FillStyle>,
}

impl ShapeWithStyleBuilder {
    pub fn new(shape: &Shape, gc: &GraphicContext, outline: bool, fill: bool) -> ShapeWithStyleBuilder {
        let origin = Point::new(gc.get_pen().0, gc.get_pen().1);
        let paint = gc.get_paint();
        let stroke = gc.get_stroke();
        let mut fs = None;
        let mut ls = None;
        if fill && paint.is_some() {
            fs = build_fill_style(paint, &shape.get_bounds2d(), &gc.get_transform());
        }
        if outline {
            ls = Some(build_line_style(paint, Some(stroke)));
        }
        let mut b = ShapeWithStyleBuilder { builder: ShapeBuilder::with_origin(origin), linestyles: Vec::with_capacity(2), fillstyles: Vec::with_capacity(2) };
        if fill {
            if let Some(fs) = fs {
                b.builder.set_use_fill_style0(true);
                let i = b.add_fill_style(fs);
                b.builder.set_current_fill_style0(i);
            }
        }
        if let Some(ls) = ls {
            let i = b.add_line_style(ls);
            b.builder.set_current_line_style(i);
        }
        b.builder.process_shape(&mut PathIteratorWrapper::new(shape.get_path_iterator(None)));
        b
    }

    pub fn build(self) -> ShapeWithStyle {
        ShapeWithStyle { shape_records: self.builder.build().shape_records, fillstyles: Some(self.fillstyles), linestyles: Some(self.linestyles) }
    }

    pub fn add_fill_style(&mut self, fs: FillStyle) -> i32 {
        self.fillstyles.push(fs);
        self.fillstyles.len() as i32
    }

    pub fn add_line_style(&mut self, ls: LineStyle) -> i32 {
        self.linestyles.push(ls);
        self.linestyles.len() as i32
    }
}

/// `new DefineShapeBuilder(shape, graphicContext, outline, fill).build()`: a DefineShape3.
pub fn build_define_shape(shape: &Shape, gc: &GraphicContext, outline: bool, fill: bool) -> TagRef {
    let sws = ShapeWithStyleBuilder::new(shape, gc, outline, fill).build();
    let bounds = get_bounds(sws.shape_records.as_deref().unwrap_or(&[]), sws.linestyles.as_deref().unwrap_or(&[]));
    Tag::new_ref(
        codes::DEFINE_SHAPE3,
        Body::DefineShape(DefineShape { bounds, shape_with_style: sws, uses_fill_winding_rule: false, uses_non_scaling_strokes: false, uses_scaling_strokes: false, edge_bounds: None, scaling_grid: None }),
    )
}

/// `DefineShapeBuilder.getBounds(records, lineStyles)`.
pub fn get_bounds(records: &[ShapeRecord], line_styles: &[LineStyle]) -> Rect {
    if records.is_empty() {
        return Rect::default();
    }
    let (mut x1, mut y1, mut x2, mut y2) = (0i32, 0i32, 0i32, 0i32);
    let (mut x, mut y) = (0i32, 0i32);
    let mut first_move = true;
    for r in records {
        match r {
            ShapeRecord::StyleChange(scr) => {
                x = scr.move_delta_x;
                y = scr.move_delta_y;
                if first_move {
                    x1 = x;
                    y1 = y;
                    x2 = x;
                    y2 = y;
                    first_move = false;
                }
            }
            ShapeRecord::StraightEdge { delta_x, delta_y } => {
                x = x.wrapping_add(*delta_x);
                y = y.wrapping_add(*delta_y);
            }
            ShapeRecord::CurvedEdge { control_delta_x, control_delta_y, anchor_delta_x, anchor_delta_y } => {
                x = x.wrapping_add(*control_delta_x).wrapping_add(*anchor_delta_x);
                y = y.wrapping_add(*control_delta_y).wrapping_add(*anchor_delta_y);
            }
        }
        if x < x1 {
            x1 = x;
        }
        if y < y1 {
            y1 = y;
        }
        if x > x2 {
            x2 = x;
        }
        if y > y2 {
            y2 = y;
        }
    }
    if !line_styles.is_empty() {
        let mut width = 20;
        for ls in line_styles {
            if width < ls.width {
                width = ls.width;
            }
        }
        let stroke = (width as f64 * 0.5).round_ties_even() as i32 as f64;
        x1 = (x1 as f64 - stroke).round_ties_even() as i32;
        y1 = (y1 as f64 - stroke).round_ties_even() as i32;
        x2 = (x2 as f64 + stroke).round_ties_even() as i32;
        y2 = (y2 as f64 + stroke).round_ties_even() as i32;
    }
    Rect::new(x1, x2, y1, y2)
}
