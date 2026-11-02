//! Port of FreeType 2.14.2 `src/base/ftobjs.c` (as built into JDK 17.0.19): face opening, sizing and glyph loading for the TrueType driver.

use std::rc::Rc;

use crate::asc::flex2::fonts::java2d::freetype::afloader::af_loader_load_glyph;
use crate::asc::flex2::fonts::java2d::freetype::afmodule::AfFaceGlobals;
use crate::asc::flex2::fonts::java2d::freetype::ftcalc::*;
use crate::asc::flex2::fonts::java2d::freetype::fterrors::*;
use crate::asc::flex2::fonts::java2d::freetype::ftgloadr::FtGlyphLoader;
use crate::asc::flex2::fonts::java2d::freetype::ftoutln::*;
use crate::asc::flex2::fonts::java2d::freetype::ftstream::FtStream;
use crate::asc::flex2::fonts::java2d::freetype::ttdriver::{tt_glyph_load, tt_size_request};
use crate::asc::flex2::fonts::java2d::freetype::ttobjs::{tt_face_init, TtSize};
pub use crate::asc::flex2::fonts::java2d::freetype::fttypes::*;
use crate::asc::flex2::fonts::java2d::freetype::tttypes::{FtBitmapSize, TtFace};

pub const FT_FACE_FLAG_SCALABLE: i32 = 1 << 0;
pub const FT_FACE_FLAG_FIXED_SIZES: i32 = 1 << 1;
pub const FT_FACE_FLAG_FIXED_WIDTH: i32 = 1 << 2;
pub const FT_FACE_FLAG_SFNT: i32 = 1 << 3;
pub const FT_FACE_FLAG_HORIZONTAL: i32 = 1 << 4;
pub const FT_FACE_FLAG_VERTICAL: i32 = 1 << 5;
pub const FT_FACE_FLAG_KERNING: i32 = 1 << 6;
pub const FT_FACE_FLAG_FAST_GLYPHS: i32 = 1 << 7;
pub const FT_FACE_FLAG_MULTIPLE_MASTERS: i32 = 1 << 8;
pub const FT_FACE_FLAG_GLYPH_NAMES: i32 = 1 << 9;
pub const FT_FACE_FLAG_EXTERNAL_STREAM: i32 = 1 << 10;
pub const FT_FACE_FLAG_HINTER: i32 = 1 << 11;
pub const FT_FACE_FLAG_CID_KEYED: i32 = 1 << 12;
pub const FT_FACE_FLAG_TRICKY: i32 = 1 << 13;
pub const FT_FACE_FLAG_COLOR: i32 = 1 << 14;
pub const FT_FACE_FLAG_VARIATION: i32 = 1 << 15;
pub const FT_FACE_FLAG_SVG: i32 = 1 << 16;
pub const FT_FACE_FLAG_SBIX: i32 = 1 << 17;
pub const FT_FACE_FLAG_SBIX_OVERLAY: i32 = 1 << 18;

pub const FT_STYLE_FLAG_ITALIC: i32 = 1 << 0;
pub const FT_STYLE_FLAG_BOLD: i32 = 1 << 1;

pub const FT_LOAD_DEFAULT: i32 = 0x0;
pub const FT_LOAD_NO_SCALE: i32 = 1 << 0;
pub const FT_LOAD_NO_HINTING: i32 = 1 << 1;
pub const FT_LOAD_RENDER: i32 = 1 << 2;
pub const FT_LOAD_NO_BITMAP: i32 = 1 << 3;
pub const FT_LOAD_VERTICAL_LAYOUT: i32 = 1 << 4;
pub const FT_LOAD_FORCE_AUTOHINT: i32 = 1 << 5;
pub const FT_LOAD_CROP_BITMAP: i32 = 1 << 6;
pub const FT_LOAD_PEDANTIC: i32 = 1 << 7;
pub const FT_LOAD_ADVANCE_ONLY: i32 = 1 << 8;
pub const FT_LOAD_IGNORE_GLOBAL_ADVANCE_WIDTH: i32 = 1 << 9;
pub const FT_LOAD_NO_RECURSE: i32 = 1 << 10;
pub const FT_LOAD_IGNORE_TRANSFORM: i32 = 1 << 11;
pub const FT_LOAD_MONOCHROME: i32 = 1 << 12;
pub const FT_LOAD_LINEAR_DESIGN: i32 = 1 << 13;
pub const FT_LOAD_SBITS_ONLY: i32 = 1 << 14;
pub const FT_LOAD_NO_AUTOHINT: i32 = 1 << 15;
pub const FT_LOAD_COLOR: i32 = 1 << 20;
pub const FT_LOAD_COMPUTE_METRICS: i32 = 1 << 21;
pub const FT_LOAD_BITMAP_METRICS_ONLY: i32 = 1 << 22;
pub const FT_LOAD_SVG_ONLY: i32 = 1 << 23;
pub const FT_LOAD_NO_SVG: i32 = 1 << 24;

pub const fn ft_load_target(x: i32) -> i32 {
    (x & 15) << 16
}

pub const FT_LOAD_TARGET_NORMAL: i32 = ft_load_target(FT_RENDER_MODE_NORMAL);
pub const FT_LOAD_TARGET_LIGHT: i32 = ft_load_target(FT_RENDER_MODE_LIGHT);
pub const FT_LOAD_TARGET_MONO: i32 = ft_load_target(FT_RENDER_MODE_MONO);
pub const FT_LOAD_TARGET_LCD: i32 = ft_load_target(FT_RENDER_MODE_LCD);
pub const FT_LOAD_TARGET_LCD_V: i32 = ft_load_target(FT_RENDER_MODE_LCD_V);

#[inline]
pub fn ft_load_target_mode(x: i32) -> i32 {
    (x >> 16) & 15
}

pub const FT_SIZE_REQUEST_TYPE_NOMINAL: i32 = 0;
pub const FT_SIZE_REQUEST_TYPE_REAL_DIM: i32 = 1;
pub const FT_SIZE_REQUEST_TYPE_BBOX: i32 = 2;
pub const FT_SIZE_REQUEST_TYPE_CELL: i32 = 3;
pub const FT_SIZE_REQUEST_TYPE_SCALES: i32 = 4;
pub const FT_SIZE_REQUEST_TYPE_MAX: i32 = 5;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct FtGlyphMetrics {
    pub width: FtPos,
    pub height: FtPos,
    pub hori_bearing_x: FtPos,
    pub hori_bearing_y: FtPos,
    pub hori_advance: FtPos,
    pub vert_bearing_x: FtPos,
    pub vert_bearing_y: FtPos,
    pub vert_advance: FtPos,
}

#[derive(Clone, Default)]
pub struct FtGlyphSlot {
    pub glyph_index: u32,
    pub format: u32,
    pub metrics: FtGlyphMetrics,
    pub linear_hori_advance: FtFixed,
    pub linear_vert_advance: FtFixed,
    pub advance: FtVector,
    pub outline: FtOutline,
    pub num_subglyphs: u32,
    pub control_len: u32,
    pub lsb_delta: FtPos,
    pub rsb_delta: FtPos,
    pub(crate) loader: FtGlyphLoader,
    pub(crate) load_flags: i32,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct FtSizeMetrics {
    pub x_ppem: u16,
    pub y_ppem: u16,
    pub x_scale: FtFixed,
    pub y_scale: FtFixed,
    pub ascender: FtPos,
    pub descender: FtPos,
    pub height: FtPos,
    pub max_advance: FtPos,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct FtSizeRequest {
    pub type_: i32,
    pub width: i32,
    pub height: i32,
    pub hori_resolution: u32,
    pub vert_resolution: u32,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct FtFaceInternal {
    pub transform_matrix: FtMatrix,
    pub transform_delta: FtVector,
    pub transform_flags: i32,
}

#[derive(Clone, Default)]
pub struct FtFace {
    pub num_faces: i32,
    pub face_index: i32,
    pub face_flags: i32,
    pub style_flags: i32,
    pub num_glyphs: i32,
    pub family_name: Option<String>,
    pub style_name: Option<String>,
    pub num_fixed_sizes: i32,
    pub available_sizes: Vec<FtBitmapSize>,
    pub bbox: FtBBox,
    pub units_per_em: u16,
    pub ascender: i16,
    pub descender: i16,
    pub height: i16,
    pub max_advance_width: i16,
    pub max_advance_height: i16,
    pub underline_position: i16,
    pub underline_thickness: i16,
    pub glyph: FtGlyphSlot,
    pub(crate) size: TtSize,
    pub(crate) internal: FtFaceInternal,
    pub(crate) tt: TtFace,
    pub(crate) autohint_globals: Option<Box<AfFaceGlobals>>,
}

impl FtFace {
    pub fn size_metrics(&self) -> &FtSizeMetrics {
        &self.size.root_metrics
    }

    pub fn is_scalable(&self) -> bool {
        self.face_flags & FT_FACE_FLAG_SCALABLE != 0
    }

    pub fn is_sfnt(&self) -> bool {
        self.face_flags & FT_FACE_FLAG_SFNT != 0
    }

    pub fn is_tricky(&self) -> bool {
        self.face_flags & FT_FACE_FLAG_TRICKY != 0
    }

    pub fn is_fixed_width(&self) -> bool {
        self.face_flags & FT_FACE_FLAG_FIXED_WIDTH != 0
    }

    pub fn has_fixed_sizes(&self) -> bool {
        self.face_flags & FT_FACE_FLAG_FIXED_SIZES != 0
    }

    pub fn has_vertical(&self) -> bool {
        self.face_flags & FT_FACE_FLAG_VERTICAL != 0
    }

    pub fn has_multiple_masters(&self) -> bool {
        self.face_flags & FT_FACE_FLAG_MULTIPLE_MASTERS != 0
    }

    pub fn has_sbix(&self) -> bool {
        self.face_flags & FT_FACE_FLAG_SBIX != 0
    }
}

/// `FT_Init_FreeType` + the JDK's `interpreter-version = 35` + `FT_Open_Face` on a stream over the whole file.
pub fn ft_open_face(data: Rc<Vec<u8>>, face_index: i32) -> Result<FtFace, FtError> {
    let mut face_index = face_index;
    if face_index > 0 {
        face_index &= 0x7FFF_FFFF;
    } else {
        face_index = -((-face_index) & 0x7FFF_FFFF);
    }
    let mut face = FtFace::default();
    face.tt.stream = FtStream::new(data);
    let error = match tt_face_init(&mut face, face_index) {
        Ok(()) => FT_ERR_OK,
        Err(e) => e,
    };
    if error != FT_ERR_OK {
        if error != FT_ERR_UNKNOWN_FILE_FORMAT {
            return Err(error);
        }
        return Err(FT_ERR_UNKNOWN_FILE_FORMAT);
    }
    face.glyph.loader.create_extra();
    face.size = TtSize::new();
    if face.is_scalable() {
        if face.height < 0 {
            face.height = face.height.wrapping_neg();
        }
        if !face.has_vertical() {
            face.max_advance_height = face.height;
        }
    }
    if face.has_fixed_sizes() {
        for bsize in face.available_sizes.iter_mut() {
            if bsize.height < 0 {
                bsize.height = bsize.height.wrapping_neg();
            }
            if bsize.x_ppem < 0 {
                bsize.x_ppem = bsize.x_ppem.wrapping_neg();
            }
            if bsize.y_ppem < 0 {
                bsize.y_ppem = bsize.y_ppem.wrapping_neg();
            }
            if bsize.height < 0 || bsize.x_ppem < 0 || bsize.y_ppem < 0 {
                *bsize = FtBitmapSize::default();
            }
        }
    }
    face.internal.transform_matrix = FtMatrix { xx: 0x10000, xy: 0, yx: 0, yy: 0x10000 };
    face.internal.transform_delta = FtVector::default();
    Ok(face)
}

pub fn ft_set_transform(face: &mut FtFace, matrix: Option<&FtMatrix>, delta: Option<&FtVector>) {
    let internal = &mut face.internal;
    internal.transform_flags = 0;
    internal.transform_matrix = match matrix {
        Some(m) => *m,
        None => FtMatrix { xx: 0x10000, xy: 0, yx: 0, yy: 0x10000 },
    };
    let m = internal.transform_matrix;
    if (m.xy | m.yx) != 0 || m.xx != 0x10000 || m.yy != 0x10000 {
        internal.transform_flags |= 1;
    }
    internal.transform_delta = match delta {
        Some(d) => *d,
        None => FtVector::default(),
    };
    let d = internal.transform_delta;
    if (d.x | d.y) != 0 {
        internal.transform_flags |= 2;
    }
}

fn ft_glyphslot_grid_fit_metrics(slot: &mut FtGlyphSlot, vertical: bool) {
    let metrics = &mut slot.metrics;
    if vertical {
        metrics.hori_bearing_x = ft_pix_floor(metrics.hori_bearing_x);
        metrics.hori_bearing_y = ft_pix_ceil_long(metrics.hori_bearing_y);
        let right = ft_pix_ceil_long(add_long(metrics.vert_bearing_x, metrics.width));
        let bottom = ft_pix_ceil_long(add_long(metrics.vert_bearing_y, metrics.height));
        metrics.vert_bearing_x = ft_pix_floor(metrics.vert_bearing_x);
        metrics.vert_bearing_y = ft_pix_floor(metrics.vert_bearing_y);
        metrics.width = sub_long(right, metrics.vert_bearing_x);
        metrics.height = sub_long(bottom, metrics.vert_bearing_y);
    } else {
        metrics.vert_bearing_x = ft_pix_floor(metrics.vert_bearing_x);
        metrics.vert_bearing_y = ft_pix_floor(metrics.vert_bearing_y);
        let right = ft_pix_ceil_long(add_long(metrics.hori_bearing_x, metrics.width));
        let bottom = ft_pix_floor(sub_long(metrics.hori_bearing_y, metrics.height));
        metrics.hori_bearing_x = ft_pix_floor(metrics.hori_bearing_x);
        metrics.hori_bearing_y = ft_pix_ceil_long(metrics.hori_bearing_y);
        metrics.width = sub_long(right, metrics.hori_bearing_x);
        metrics.height = sub_long(metrics.hori_bearing_y, bottom);
    }
    metrics.hori_advance = ft_pix_round_long(metrics.hori_advance);
    metrics.vert_advance = ft_pix_round_long(metrics.vert_advance);
}

pub(crate) fn ft_glyphslot_clear(slot: &mut FtGlyphSlot) {
    slot.glyph_index = 0;
    slot.metrics = FtGlyphMetrics::default();
    slot.outline = FtOutline::default();
    slot.num_subglyphs = 0;
    slot.control_len = 0;
    slot.format = FT_GLYPH_FORMAT_NONE;
    slot.linear_hori_advance = 0;
    slot.linear_vert_advance = 0;
    slot.advance = FtVector::default();
    slot.lsb_delta = 0;
    slot.rsb_delta = 0;
}

pub fn ft_load_glyph(face: &mut FtFace, glyph_index: u32, load_flags: i32) -> FtError {
    let mut slot = std::mem::take(&mut face.glyph);
    let mut size = std::mem::take(&mut face.size);
    let e = ft_load_glyph_internal(face, &mut size, &mut slot, glyph_index, load_flags);
    face.glyph = slot;
    face.size = size;
    e
}

pub(crate) fn ft_load_glyph_internal(face: &mut FtFace, size: &mut TtSize, slot: &mut FtGlyphSlot, glyph_index: u32, load_flags: i32) -> FtError {
    let mut load_flags = load_flags;
    ft_glyphslot_clear(slot);
    if size.root_metrics.x_ppem == 0 || size.root_metrics.y_ppem == 0 {
        load_flags |= FT_LOAD_NO_SCALE;
    }
    if load_flags & FT_LOAD_NO_RECURSE != 0 {
        load_flags |= FT_LOAD_NO_SCALE | FT_LOAD_IGNORE_TRANSFORM;
    }
    if load_flags & FT_LOAD_NO_SCALE != 0 {
        load_flags |= FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP;
        load_flags &= !FT_LOAD_RENDER;
    }
    if load_flags & FT_LOAD_BITMAP_METRICS_ONLY != 0 {
        load_flags &= !FT_LOAD_RENDER;
    }
    let mut autohint = false;
    let tm = face.internal.transform_matrix;
    if load_flags & FT_LOAD_NO_HINTING == 0
        && load_flags & FT_LOAD_NO_AUTOHINT == 0
        && face.is_scalable()
        && !face.is_tricky()
        && (load_flags & FT_LOAD_IGNORE_TRANSFORM != 0 || (tm.yx == 0 && tm.xx != 0) || (tm.xx == 0 && tm.yx != 0))
    {
        if load_flags & FT_LOAD_FORCE_AUTOHINT != 0 {
            autohint = true;
        } else {
            let mode = ft_load_target_mode(load_flags);
            if mode == FT_RENDER_MODE_LIGHT
                || (face.is_sfnt() && face.tt.num_locations != 0 && face.tt.font_program.is_empty() && face.tt.cvt_program.len() <= 7)
            {
                autohint = true;
            }
        }
    }
    let mut error;
    if autohint {
        let transform_flags = face.internal.transform_flags;
        face.internal.transform_flags = 0;
        error = af_loader_load_glyph(face, size, slot, glyph_index, load_flags);
        face.internal.transform_flags = transform_flags;
    } else {
        error = tt_glyph_load(face, size, slot, glyph_index, load_flags);
        if error != 0 {
            return error;
        }
        if slot.format == FT_GLYPH_FORMAT_OUTLINE {
            error = ft_outline_check(&slot.outline);
            if error != 0 {
                return error;
            }
            if load_flags & FT_LOAD_NO_HINTING == 0 {
                ft_glyphslot_grid_fit_metrics(slot, load_flags & FT_LOAD_VERTICAL_LAYOUT != 0);
            }
        }
    }
    if load_flags & FT_LOAD_VERTICAL_LAYOUT != 0 {
        slot.advance.x = 0;
        slot.advance.y = slot.metrics.vert_advance;
    } else {
        slot.advance.x = slot.metrics.hori_advance;
        slot.advance.y = 0;
    }
    if load_flags & FT_LOAD_LINEAR_DESIGN == 0 && face.is_scalable() {
        let metrics = &size.root_metrics;
        slot.linear_hori_advance = ft_mul_div(slot.linear_hori_advance, metrics.x_scale, 64);
        slot.linear_vert_advance = ft_mul_div(slot.linear_vert_advance, metrics.y_scale, 64);
    }
    if load_flags & FT_LOAD_IGNORE_TRANSFORM == 0 {
        let internal = face.internal;
        if internal.transform_flags != 0 {
            if slot.format == FT_GLYPH_FORMAT_OUTLINE {
                // the smooth renderer's transform_glyph, whose result replaces `error`
                error = FT_ERR_OK;
                ft_outline_transform(&mut slot.outline, &internal.transform_matrix);
                ft_outline_translate(&mut slot.outline, internal.transform_delta.x, internal.transform_delta.y);
            }
            ft_vector_transform(&mut slot.advance, &internal.transform_matrix);
        }
    }
    slot.glyph_index = glyph_index;
    slot.load_flags = load_flags;
    error
}

pub fn ft_request_metrics(face: &FtFace, metrics: &mut FtSizeMetrics, req: &FtSizeRequest) -> FtError {
    if face.is_scalable() {
        let (mut w, mut h): (i32, i32);
        match req.type_ {
            FT_SIZE_REQUEST_TYPE_NOMINAL => {
                w = face.units_per_em as i32;
                h = w;
            }
            FT_SIZE_REQUEST_TYPE_REAL_DIM => {
                w = face.ascender as i32 - face.descender as i32;
                h = w;
            }
            FT_SIZE_REQUEST_TYPE_BBOX => {
                w = face.bbox.x_max.wrapping_sub(face.bbox.x_min);
                h = face.bbox.y_max.wrapping_sub(face.bbox.y_min);
            }
            FT_SIZE_REQUEST_TYPE_CELL => {
                w = face.max_advance_width as i32;
                h = face.ascender as i32 - face.descender as i32;
            }
            _ => {
                w = 0;
                h = 0;
            }
        }
        let mut scaled_w;
        let mut scaled_h;
        if req.type_ == FT_SIZE_REQUEST_TYPE_SCALES {
            metrics.x_scale = req.width;
            metrics.y_scale = req.height;
            if metrics.x_scale == 0 {
                metrics.x_scale = metrics.y_scale;
            } else if metrics.y_scale == 0 {
                metrics.y_scale = metrics.x_scale;
            }
            scaled_w = 0;
            scaled_h = 0;
        } else {
            if w < 0 {
                w = -w;
            }
            if h < 0 {
                h = -h;
            }
            scaled_w = ft_request_width(req);
            scaled_h = ft_request_height(req);
            if req.height != 0 || req.width == 0 {
                if h == 0 {
                    return FT_ERR_DIVIDE_BY_ZERO;
                }
                metrics.y_scale = ft_div_fix(scaled_h, h);
            }
            if req.width != 0 {
                if w == 0 {
                    return FT_ERR_DIVIDE_BY_ZERO;
                }
                metrics.x_scale = ft_div_fix(scaled_w, w);
            } else {
                metrics.x_scale = metrics.y_scale;
                scaled_w = ft_mul_div(scaled_h, w, h);
            }
            if req.height == 0 {
                metrics.y_scale = metrics.x_scale;
                scaled_h = ft_mul_div(scaled_w, h, w);
            }
            if req.type_ == FT_SIZE_REQUEST_TYPE_CELL {
                if metrics.y_scale > metrics.x_scale {
                    metrics.y_scale = metrics.x_scale;
                } else {
                    metrics.x_scale = metrics.y_scale;
                }
            }
        }
        if req.type_ != FT_SIZE_REQUEST_TYPE_NOMINAL {
            scaled_w = ft_mul_fix(face.units_per_em as i32, metrics.x_scale);
            scaled_h = ft_mul_fix(face.units_per_em as i32, metrics.y_scale);
        }
        scaled_w = scaled_w.wrapping_add(32) >> 6;
        scaled_h = scaled_h.wrapping_add(32) >> 6;
        if scaled_w > 0xFFFF || scaled_h > 0xFFFF {
            return FT_ERR_INVALID_PIXEL_SIZE;
        }
        metrics.x_ppem = scaled_w as u16;
        metrics.y_ppem = scaled_h as u16;
        ft_recompute_scaled_metrics(face, metrics);
    } else {
        *metrics = FtSizeMetrics::default();
        metrics.x_scale = 1 << 16;
        metrics.y_scale = 1 << 16;
    }
    FT_ERR_OK
}

pub fn ft_request_width(req: &FtSizeRequest) -> i32 {
    if req.hori_resolution != 0 {
        (req.width.wrapping_mul(req.hori_resolution as i32).wrapping_add(36)) / 72
    } else {
        req.width
    }
}

pub fn ft_request_height(req: &FtSizeRequest) -> i32 {
    if req.vert_resolution != 0 {
        (req.height.wrapping_mul(req.vert_resolution as i32).wrapping_add(36)) / 72
    } else {
        req.height
    }
}

pub fn ft_recompute_scaled_metrics(face: &FtFace, metrics: &mut FtSizeMetrics) {
    metrics.ascender = ft_pix_ceil(ft_mul_fix(face.ascender as i32, metrics.y_scale));
    metrics.descender = ft_pix_floor(ft_mul_fix(face.descender as i32, metrics.y_scale));
    metrics.height = ft_pix_round(ft_mul_fix(face.height as i32, metrics.y_scale));
    metrics.max_advance = ft_pix_round(ft_mul_fix(face.max_advance_width as i32, metrics.x_scale));
}

pub fn ft_select_metrics(face: &FtFace, metrics: &mut FtSizeMetrics, strike_index: u32) {
    let bsize = face.available_sizes[strike_index as usize];
    metrics.x_ppem = ((bsize.x_ppem + 32) >> 6) as u16;
    metrics.y_ppem = ((bsize.y_ppem + 32) >> 6) as u16;
    if face.is_scalable() {
        metrics.x_scale = ft_div_fix(bsize.x_ppem, face.units_per_em as i32);
        metrics.y_scale = ft_div_fix(bsize.y_ppem, face.units_per_em as i32);
        ft_recompute_scaled_metrics(face, metrics);
    } else {
        metrics.x_scale = 1 << 16;
        metrics.y_scale = 1 << 16;
        metrics.ascender = bsize.y_ppem;
        metrics.descender = 0;
        metrics.height = (bsize.height as i32) << 6;
        metrics.max_advance = bsize.x_ppem;
    }
}

pub fn ft_match_size(face: &FtFace, req: &FtSizeRequest, ignore_width: bool) -> Result<u32, FtError> {
    if !face.has_fixed_sizes() {
        return Err(FT_ERR_INVALID_FACE_HANDLE);
    }
    if req.type_ != FT_SIZE_REQUEST_TYPE_NOMINAL {
        return Err(FT_ERR_UNIMPLEMENTED_FEATURE);
    }
    let mut w = ft_request_width(req);
    let mut h = ft_request_height(req);
    if req.width != 0 && req.height == 0 {
        h = w;
    } else if req.width == 0 && req.height != 0 {
        w = h;
    }
    w = ft_pix_round(w);
    h = ft_pix_round(h);
    if w == 0 || h == 0 {
        return Err(FT_ERR_INVALID_PIXEL_SIZE);
    }
    for i in 0..face.num_fixed_sizes as usize {
        let bsize = &face.available_sizes[i];
        if h != ft_pix_round(bsize.y_ppem) {
            continue;
        }
        if w == ft_pix_round(bsize.x_ppem) || ignore_width {
            return Ok(i as u32);
        }
    }
    Err(FT_ERR_INVALID_PIXEL_SIZE)
}

pub fn ft_request_size(face: &mut FtFace, req: &FtSizeRequest) -> FtError {
    if req.width < 0 || req.height < 0 || req.type_ >= FT_SIZE_REQUEST_TYPE_MAX {
        return FT_ERR_INVALID_ARGUMENT;
    }
    let mut size = std::mem::take(&mut face.size);
    size.autohint_x_scale = 0;
    let e = tt_size_request(face, &mut size, req);
    face.size = size;
    e
}

pub fn ft_set_char_size(face: &mut FtFace, char_width: FtF26Dot6, char_height: FtF26Dot6, horz_resolution: u32, vert_resolution: u32) -> FtError {
    let (mut cw, mut ch) = (char_width, char_height);
    let (mut hr, mut vr) = (horz_resolution, vert_resolution);
    if cw == 0 {
        cw = ch;
    } else if ch == 0 {
        ch = cw;
    }
    if hr == 0 {
        hr = vr;
    } else if vr == 0 {
        vr = hr;
    }
    if cw < 64 {
        cw = 64;
    }
    if ch < 64 {
        ch = 64;
    }
    if hr == 0 {
        hr = 72;
        vr = 72;
    }
    let req = FtSizeRequest { type_: FT_SIZE_REQUEST_TYPE_NOMINAL, width: cw, height: ch, hori_resolution: hr, vert_resolution: vr };
    ft_request_size(face, &req)
}

pub fn ft_activate_size(_face: &mut FtFace) -> FtError {
    FT_ERR_OK
}
