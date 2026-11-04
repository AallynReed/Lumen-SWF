//! `sun.java2d.marlin.MarlinConst` with the `MarlinProperties` defaults it reads.

pub const DO_CLIP: bool = true;
pub const DO_CLIP_RUNTIME_ENABLE: bool = false;
pub const DO_CLIP_SUBDIVIDER: bool = true;
pub const USE_SIMPLIFIER: bool = false;
pub const USE_PATH_SIMPLIFIER: bool = false;
pub const DISABLE_2ND_STROKER_CLIPPING: bool = true;

pub const SUBPIXEL_LG_POSITIONS_X: i32 = 8;
pub const SUBPIXEL_LG_POSITIONS_Y: i32 = 3;
pub const MIN_SUBPIXEL_LG_POSITIONS: i32 = if SUBPIXEL_LG_POSITIONS_X < SUBPIXEL_LG_POSITIONS_Y {
    SUBPIXEL_LG_POSITIONS_X
} else {
    SUBPIXEL_LG_POSITIONS_Y
};
pub const MIN_SUBPIXELS: f32 = (1 << MIN_SUBPIXEL_LG_POSITIONS) as f32;

/// `MarlinProperties.getSubdividerMinLength()`.
pub const SUBDIVIDER_MIN_LENGTH: f32 = 100.0;
/// `MarlinProperties.getCurveLengthError()`.
pub const CURVE_LEN_ERR: f32 = 0.01;

pub const JOIN_MITER: i32 = 0;
pub const JOIN_ROUND: i32 = 1;
pub const JOIN_BEVEL: i32 = 2;

pub const CAP_BUTT: i32 = 0;
pub const CAP_ROUND: i32 = 1;
pub const CAP_SQUARE: i32 = 2;

pub const OUTCODE_TOP: i32 = 1;
pub const OUTCODE_BOTTOM: i32 = 2;
pub const OUTCODE_LEFT: i32 = 4;
pub const OUTCODE_RIGHT: i32 = 8;
