//! Port of Batik's `bridge.ViewBox`: `viewBox` parsing and the `preserveAspectRatio` transform.

use super::{bridge_exception, BResult, ERR_ATTRIBUTE_VALUE_MALFORMED};
use crate::asc::flex2::batik::dom::NodeRef;
use crate::asc::flex2::batik::parser::{self, *};
use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::AffineTransform;

/// `getPreserveAspectRatioTransform(e, w, h)`.
pub fn get_preserve_aspect_ratio_transform(e: &NodeRef, w: f32, h: f32) -> BResult<AffineTransform> {
    let view_box = e.get_attribute_ns(None, "viewBox");
    let aspect_ratio = e.get_attribute_ns(None, "preserveAspectRatio");
    if view_box.is_empty() {
        return Ok(AffineTransform::new());
    }
    let vb = parse_view_box_attribute(e, &view_box)?;
    let (align, meet) = parser::parse_preserve_aspect_ratio(&aspect_ratio).map_err(|_| bridge_exception(e, ERR_ATTRIBUTE_VALUE_MALFORMED, &["preserveAspectRatio", &aspect_ratio]))?;
    Ok(transform_for(vb, align, meet, w, h))
}

/// `parseViewBoxAttribute(e, value)`.
pub fn parse_view_box_attribute(e: &NodeRef, value: &str) -> BResult<Option<[f32; 4]>> {
    if value.is_empty() {
        return Ok(None);
    }
    let malformed = || bridge_exception(e, ERR_ATTRIBUTE_VALUE_MALFORMED, &["viewBox", value]);
    let mut vb = [0f32; 4];
    let mut i = 0;
    for t in value.split([' ', ',']).filter(|t| !t.is_empty()) {
        if i >= 4 {
            break;
        }
        vb[i] = parser::java_parse_float(t).ok_or_else(malformed)?;
        i += 1;
    }
    if i != 4 || vb[2] < 0.0 || vb[3] < 0.0 {
        return Err(malformed());
    }
    if vb[2] == 0.0 || vb[3] == 0.0 {
        return Ok(None);
    }
    Ok(Some(vb))
}

/// `getPreserveAspectRatioTransform(vb, align, meet, w, h)`.
pub fn transform_for(vb: Option<[f32; 4]>, align: i16, meet: bool, w: f32, h: f32) -> AffineTransform {
    let Some(vb) = vb else { return AffineTransform::new() };
    let mut result = AffineTransform::new();
    let vpar = vb[2] / vb[3];
    let svgar = w / h;
    if align == SVG_PRESERVEASPECTRATIO_NONE {
        result.scale((w / vb[2]) as f64, (h / vb[3]) as f64);
        result.translate(-vb[0] as f64, -vb[1] as f64);
    } else if (vpar < svgar && meet) || (vpar >= svgar && !meet) {
        let sf = h / vb[3];
        result.scale(sf as f64, sf as f64);
        match align {
            SVG_PRESERVEASPECTRATIO_XMINYMIN | SVG_PRESERVEASPECTRATIO_XMINYMID | SVG_PRESERVEASPECTRATIO_XMINYMAX => result.translate(-vb[0] as f64, -vb[1] as f64),
            SVG_PRESERVEASPECTRATIO_XMIDYMIN | SVG_PRESERVEASPECTRATIO_XMIDYMID | SVG_PRESERVEASPECTRATIO_XMIDYMAX => result.translate((-vb[0] - (vb[2] - w * vb[3] / h) / 2.0) as f64, -vb[1] as f64),
            _ => result.translate((-vb[0] - (vb[2] - w * vb[3] / h)) as f64, -vb[1] as f64),
        }
    } else {
        let sf = w / vb[2];
        result.scale(sf as f64, sf as f64);
        match align {
            SVG_PRESERVEASPECTRATIO_XMINYMIN | SVG_PRESERVEASPECTRATIO_XMIDYMIN | SVG_PRESERVEASPECTRATIO_XMAXYMIN => result.translate(-vb[0] as f64, -vb[1] as f64),
            SVG_PRESERVEASPECTRATIO_XMINYMID | SVG_PRESERVEASPECTRATIO_XMIDYMID | SVG_PRESERVEASPECTRATIO_XMAXYMID => result.translate(-vb[0] as f64, (-vb[1] - (vb[3] - h * vb[2] / w) / 2.0) as f64),
            _ => result.translate(-vb[0] as f64, (-vb[1] - (vb[3] - h * vb[2] / w)) as f64),
        }
    }
    result
}
