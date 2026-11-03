//! Port of Batik's `bridge.UnitProcessor` (over `parser.UnitProcessor` and its `DefaultContext`).

use super::bridge_context::{BridgeContext, PIXEL_UNIT_TO_MILLIMETER};
use super::{bridge_exception, BResult, ERR_ATTRIBUTE_VALUE_MALFORMED, ERR_LENGTH_NEGATIVE};
use crate::asc::flex2::batik::css::engine::FONT_SIZE_INDEX;
use crate::asc::flex2::batik::dom::NodeRef;
use crate::asc::flex2::batik::parser::{self, *};

pub const HORIZONTAL_LENGTH: i16 = 2;
pub const VERTICAL_LENGTH: i16 = 1;
pub const OTHER_LENGTH: i16 = 0;

/// `UnitProcessor.DefaultContext`.
pub struct Context<'a> {
    pub ctx: &'a BridgeContext,
    pub e: NodeRef,
}

pub fn create_context<'a>(ctx: &'a BridgeContext, e: &NodeRef) -> Context<'a> {
    Context { ctx, e: e.clone() }
}

impl Context<'_> {
    fn font_size(&self) -> f32 {
        super::css_utilities::get_computed_style(&self.e, FONT_SIZE_INDEX, self.ctx).float_value()
    }

    fn viewport(&self) -> (f32, f32) {
        self.ctx.get_viewport(&self.e)
    }
}

fn percentages_to_pixels(v: f32, d: i16, c: &Context) -> f32 {
    if d == HORIZONTAL_LENGTH {
        let w = c.viewport().0;
        w * v / 100.0f32
    } else if d == VERTICAL_LENGTH {
        let h = c.viewport().1;
        h * v / 100.0f32
    } else {
        let (w, h) = c.viewport();
        let (w, h) = (w as f64, h as f64);
        let vpp = (w * w + h * h).sqrt() / 2f64.sqrt();
        (vpp * v as f64 / 100.0) as f32
    }
}

/// `parser.UnitProcessor.svgToUserSpace(v, type, d, ctx)`.
pub fn svg_value_to_user_space(v: f32, t: i16, d: i16, c: &Context) -> f32 {
    let pmm = PIXEL_UNIT_TO_MILLIMETER;
    match t {
        SVG_LENGTHTYPE_MM => v / pmm,
        SVG_LENGTHTYPE_CM => v * 10.0f32 / pmm,
        SVG_LENGTHTYPE_IN => v * 25.4f32 / pmm,
        SVG_LENGTHTYPE_PT => v * 25.4f32 / (72.0f32 * pmm),
        SVG_LENGTHTYPE_PC => v * 25.4f32 / (6.0f32 * pmm),
        SVG_LENGTHTYPE_EMS => v * c.font_size(),
        SVG_LENGTHTYPE_EXS => v * 0.5f32 * c.font_size(),
        SVG_LENGTHTYPE_PERCENTAGE => percentages_to_pixels(v, d, c),
        _ => v,
    }
}

fn malformed(c: &Context, attr: &str, s: &str) -> super::BridgeError {
    bridge_exception(&c.e, ERR_ATTRIBUTE_VALUE_MALFORMED, &[attr, s])
}

pub fn svg_to_user_space(s: &str, attr: &str, d: i16, c: &Context) -> BResult<f32> {
    let (v, t) = parser::parse_length(s).map_err(|_| malformed(c, attr, s))?;
    Ok(svg_value_to_user_space(v, t, d, c))
}

pub fn svg_length_to_user_space(s: &str, attr: &str, d: i16, c: &Context) -> BResult<f32> {
    let v = svg_to_user_space(s, attr, d, c)?;
    if v < 0.0 {
        return Err(bridge_exception(&c.e, ERR_LENGTH_NEGATIVE, &[attr, s]));
    }
    Ok(v)
}

pub fn svg_to_object_bounding_box(s: &str, attr: &str, d: i16, c: &Context) -> BResult<f32> {
    let (v, t) = parser::parse_length(s).map_err(|_| malformed(c, attr, s))?;
    Ok(match t {
        SVG_LENGTHTYPE_NUMBER => v,
        SVG_LENGTHTYPE_PERCENTAGE => v / 100.0f32,
        _ => svg_value_to_user_space(v, t, d, c),
    })
}

pub fn svg_length_to_object_bounding_box(s: &str, attr: &str, d: i16, c: &Context) -> BResult<f32> {
    let v = svg_to_object_bounding_box(s, attr, d, c)?;
    if v < 0.0 {
        return Err(bridge_exception(&c.e, ERR_LENGTH_NEGATIVE, &[attr, s]));
    }
    Ok(v)
}

pub fn svg_horizontal_coordinate_to_user_space(s: &str, attr: &str, c: &Context) -> BResult<f32> {
    svg_to_user_space(s, attr, HORIZONTAL_LENGTH, c)
}

pub fn svg_vertical_coordinate_to_user_space(s: &str, attr: &str, c: &Context) -> BResult<f32> {
    svg_to_user_space(s, attr, VERTICAL_LENGTH, c)
}

pub fn svg_horizontal_length_to_user_space(s: &str, attr: &str, c: &Context) -> BResult<f32> {
    svg_length_to_user_space(s, attr, HORIZONTAL_LENGTH, c)
}

pub fn svg_vertical_length_to_user_space(s: &str, attr: &str, c: &Context) -> BResult<f32> {
    svg_length_to_user_space(s, attr, VERTICAL_LENGTH, c)
}

pub fn svg_other_length_to_user_space(s: &str, attr: &str, c: &Context) -> BResult<f32> {
    svg_length_to_user_space(s, attr, OTHER_LENGTH, c)
}
