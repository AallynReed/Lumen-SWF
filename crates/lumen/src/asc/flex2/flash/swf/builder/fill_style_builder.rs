//! Ports of swfutils `flash.swf.builder.types.FillStyleBuilder`, `LineStyleBuilder` and `MatrixBuilder`.

use crate::asc::flex2::flash::swf::types::{FillStyle, GradRecord, Gradient, LineStyle, Matrix, FILL_LINEAR_GRADIENT, FILL_RADIAL_GRADIENT};
use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::{AffineTransform, TYPE_MASK_ROTATION};
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::Rectangle2D;
use crate::asc::flex2::fonts::java2d::java::awt::paint::{BasicStroke, Color, Paint};

const TWIPS_PER_PIXEL: i32 = 20;
const GRADIENT_SQUARE: i32 = 32768;
const FIXED_POINT_MULTIPLE: i32 = 65536;

/// `SwfUtils.colorToInt(color)`.
pub fn color_to_int(c: &Color) -> i32 {
    (c.get_blue() & 0xFF) | ((c.get_green() & 0xFF) << 8) | ((c.get_red() & 0xFF) << 16) | ((c.get_alpha() & 0xFF) << 24)
}

fn rint(v: f64) -> i32 {
    v.round_ties_even() as i32
}

/// `MatrixBuilder.build(at)`.
pub fn build_matrix(at: &AffineTransform) -> Matrix {
    let mut m = Matrix { scale_x: rint(at.get_scale_x() * FIXED_POINT_MULTIPLE as f64), scale_y: rint(at.get_scale_y() * FIXED_POINT_MULTIPLE as f64), ..Default::default() };
    if m.scale_x != 0 || m.scale_y != 0 {
        m.has_scale = true;
    }
    m.rotate_skew0 = rint(at.get_shear_y() * FIXED_POINT_MULTIPLE as f64);
    m.rotate_skew1 = rint(at.get_shear_x() * FIXED_POINT_MULTIPLE as f64);
    if m.rotate_skew0 != 0 || m.rotate_skew1 != 0 {
        m.has_rotate = true;
        if at.get_type() & TYPE_MASK_ROTATION != 0 {
            m.has_scale = true;
        }
    }
    m.translate_x = rint(at.get_translate_x() * TWIPS_PER_PIXEL as f64);
    m.translate_y = rint(at.get_translate_y() * TWIPS_PER_PIXEL as f64);
    m
}

/// `MatrixBuilder.getTranslateInstance(tx, ty)`.
pub fn get_translate_instance(tx: f64, ty: f64) -> Matrix {
    Matrix { translate_x: rint(tx * TWIPS_PER_PIXEL as f64), translate_y: rint(ty * TWIPS_PER_PIXEL as f64), ..Default::default() }
}

/// `LineStyleBuilder.build(paint, thickness)`.
pub fn build_line_style(paint: Option<&Paint>, thickness: Option<&BasicStroke>) -> LineStyle {
    let mut ls = LineStyle::default();
    if let Some(Paint::Color(c)) = paint {
        ls.color = color_to_int(c);
    }
    if let Some(s) = thickness {
        ls.width = rint((s.get_line_width() * TWIPS_PER_PIXEL as f32) as f64);
    }
    ls
}

fn grad_records(ratios: &[f32], colors: &[Color]) -> Vec<GradRecord> {
    let len = ratios.len().min(8);
    (0..len).map(|i| GradRecord { ratio: rint((255.0f32 * ratios[i]) as f64), color: color_to_int(&colors[i]) }).collect()
}

/// `FillStyleBuilder.build(paint, bounds, transform)`.
pub fn build_fill_style(paint: Option<&Paint>, bounds: &Rectangle2D, _transform: &AffineTransform) -> Option<FillStyle> {
    let paint = paint?;
    let width = bounds.w;
    let height = bounds.h;
    match paint {
        Paint::Color(c) => Some(FillStyle::solid(color_to_int(c))),
        Paint::LinearGradient(lgp) => {
            let gt = object_bounding_box_transform(Some(lgp.start), Some(lgp.end), width, height, width, height);
            let colors = &lgp.base.colors;
            let ratios = &lgp.base.fractions;
            if colors.is_empty() || colors.len() != ratios.len() {
                return None;
            }
            if colors.len() == 1 {
                return Some(FillStyle::solid(color_to_int(&colors[0])));
            }
            Some(FillStyle {
                type_: FILL_LINEAR_GRADIENT,
                matrix: Some(build_matrix(&gt)),
                gradient: Some(Gradient { records: grad_records(ratios, colors), ..Default::default() }),
                ..Default::default()
            })
        }
        Paint::RadialGradient(rgp) => {
            let diameter = rgp.radius as f64 * 2.0;
            let outer_x = diameter * rgp.base.gradient_transform.get_scale_x();
            let outer_y = diameter * rgp.base.gradient_transform.get_scale_y();
            let gt = object_bounding_box_transform(None, None, width, height, outer_x, outer_y);
            Some(FillStyle {
                type_: FILL_RADIAL_GRADIENT,
                matrix: Some(build_matrix(&gt)),
                gradient: Some(Gradient { records: grad_records(&rgp.base.fractions, &rgp.base.colors), ..Default::default() }),
                ..Default::default()
            })
        }
        Paint::Pattern => None,
    }
}

fn object_bounding_box_transform(gp1: Option<(f64, f64)>, gp2: Option<(f64, f64)>, width: f64, height: f64, scale_width: f64, scale_height: f64) -> AffineTransform {
    let mut at = AffineTransform::new();
    at.translate(width / 2.0, height / 2.0);
    at.scale(scale_width * TWIPS_PER_PIXEL as f64 / GRADIENT_SQUARE as f64, scale_height * TWIPS_PER_PIXEL as f64 / GRADIENT_SQUARE as f64);
    if let (Some(p1), Some(p2)) = (gp1, gp2) {
        if p2.0 - p1.0 != 0.0 {
            let mx = p2.0 - p1.0;
            let my = p2.1 - p1.1;
            let gradient = my / mx;
            let mut angle = gradient.atan();
            if mx < 0.0 {
                angle += std::f64::consts::PI;
            } else if my < 0.0 {
                angle += std::f64::consts::PI * 2.0;
            }
            if angle != 0.0 {
                at.rotate(angle);
            }
        }
    }
    at
}
