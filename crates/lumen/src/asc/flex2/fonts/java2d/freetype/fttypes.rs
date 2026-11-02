//! Port of FreeType 2.14.2 `include/freetype/fttypes.h` and `ftimage.h` (as built into JDK 17.0.19).

pub type FtPos = i32;
pub type FtFixed = i32;
pub type FtF26Dot6 = i32;
pub type FtF2Dot14 = i16;
pub type FtError = i32;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct FtVector {
    pub x: FtPos,
    pub y: FtPos,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct FtUnitVector {
    pub x: FtF2Dot14,
    pub y: FtF2Dot14,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct FtMatrix {
    pub xx: FtFixed,
    pub xy: FtFixed,
    pub yx: FtFixed,
    pub yy: FtFixed,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct FtBBox {
    pub x_min: FtPos,
    pub y_min: FtPos,
    pub x_max: FtPos,
    pub y_max: FtPos,
}

/// `points`, `tags` hold exactly `n_points` entries and `contours` exactly `n_contours`.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct FtOutline {
    pub n_contours: i32,
    pub n_points: i32,
    pub points: Vec<FtVector>,
    pub tags: Vec<u8>,
    pub contours: Vec<u16>,
    pub flags: i32,
}

/// `FT_Outline_Funcs` with the JDK's `shift = 0`, `delta = 0` unless overridden.
pub trait FtOutlineFuncs {
    fn move_to(&mut self, to: &FtVector) -> i32;
    fn line_to(&mut self, to: &FtVector) -> i32;
    fn conic_to(&mut self, control: &FtVector, to: &FtVector) -> i32;
    fn cubic_to(&mut self, control1: &FtVector, control2: &FtVector, to: &FtVector) -> i32;
    fn shift(&self) -> i32 {
        0
    }
    fn delta(&self) -> FtPos {
        0
    }
}

pub const FT_OUTLINE_NONE: i32 = 0x0;
pub const FT_OUTLINE_OWNER: i32 = 0x1;
pub const FT_OUTLINE_EVEN_ODD_FILL: i32 = 0x2;
pub const FT_OUTLINE_REVERSE_FILL: i32 = 0x4;
pub const FT_OUTLINE_IGNORE_DROPOUTS: i32 = 0x8;
pub const FT_OUTLINE_SMART_DROPOUTS: i32 = 0x10;
pub const FT_OUTLINE_INCLUDE_STUBS: i32 = 0x20;
pub const FT_OUTLINE_OVERLAP: i32 = 0x40;
pub const FT_OUTLINE_HIGH_PRECISION: i32 = 0x100;
pub const FT_OUTLINE_SINGLE_PASS: i32 = 0x200;

pub const FT_OUTLINE_CONTOURS_MAX: u32 = 0xFFFF;
pub const FT_OUTLINE_POINTS_MAX: u32 = 0xFFFF;

pub const FT_CURVE_TAG_ON: u8 = 0x01;
pub const FT_CURVE_TAG_CONIC: u8 = 0x00;
pub const FT_CURVE_TAG_CUBIC: u8 = 0x02;
pub const FT_CURVE_TAG_HAS_SCANMODE: u8 = 0x04;
pub const FT_CURVE_TAG_TOUCH_X: u8 = 0x08;
pub const FT_CURVE_TAG_TOUCH_Y: u8 = 0x10;
pub const FT_CURVE_TAG_TOUCH_BOTH: u8 = FT_CURVE_TAG_TOUCH_X | FT_CURVE_TAG_TOUCH_Y;

#[inline]
pub fn ft_curve_tag(flag: u8) -> u8 {
    flag & 0x03
}

pub const fn ft_image_tag(a: u8, b: u8, c: u8, d: u8) -> u32 {
    ((a as u32) << 24) | ((b as u32) << 16) | ((c as u32) << 8) | d as u32
}

pub const FT_GLYPH_FORMAT_NONE: u32 = 0;
pub const FT_GLYPH_FORMAT_COMPOSITE: u32 = ft_image_tag(b'c', b'o', b'm', b'p');
pub const FT_GLYPH_FORMAT_BITMAP: u32 = ft_image_tag(b'b', b'i', b't', b's');
pub const FT_GLYPH_FORMAT_OUTLINE: u32 = ft_image_tag(b'o', b'u', b't', b'l');
pub const FT_GLYPH_FORMAT_PLOTTER: u32 = ft_image_tag(b'p', b'l', b'o', b't');
pub const FT_GLYPH_FORMAT_SVG: u32 = ft_image_tag(b'S', b'V', b'G', b' ');

pub const FT_RENDER_MODE_NORMAL: i32 = 0;
pub const FT_RENDER_MODE_LIGHT: i32 = 1;
pub const FT_RENDER_MODE_MONO: i32 = 2;
pub const FT_RENDER_MODE_LCD: i32 = 3;
pub const FT_RENDER_MODE_LCD_V: i32 = 4;
pub const FT_RENDER_MODE_SDF: i32 = 5;

#[inline]
pub fn add_long(a: i32, b: i32) -> i32 {
    a.wrapping_add(b)
}

#[inline]
pub fn sub_long(a: i32, b: i32) -> i32 {
    a.wrapping_sub(b)
}

#[inline]
pub fn mul_long(a: i32, b: i32) -> i32 {
    a.wrapping_mul(b)
}

#[inline]
pub fn neg_long(a: i32) -> i32 {
    0i32.wrapping_sub(a)
}

#[inline]
pub fn ft_pix_floor(x: i32) -> i32 {
    x & !63
}

#[inline]
pub fn ft_pix_round(x: i32) -> i32 {
    ft_pix_floor(x.wrapping_add(32))
}

#[inline]
pub fn ft_pix_ceil(x: i32) -> i32 {
    ft_pix_floor(x.wrapping_add(63))
}

#[inline]
pub fn ft_pix_round_long(x: i32) -> i32 {
    ft_pix_floor(add_long(x, 32))
}

#[inline]
pub fn ft_pix_ceil_long(x: i32) -> i32 {
    ft_pix_floor(add_long(x, 63))
}

#[inline]
pub fn ft_pad_floor(x: i32, n: i32) -> i32 {
    x & !(n - 1)
}

#[inline]
pub fn ft_pad_round(x: i32, n: i32) -> i32 {
    ft_pad_floor(x.wrapping_add(n / 2), n)
}

#[inline]
pub fn ft_pad_ceil(x: i32, n: i32) -> i32 {
    ft_pad_floor(x.wrapping_add(n - 1), n)
}

#[inline]
pub fn ft_pad_round_long(x: i32, n: i32) -> i32 {
    ft_pad_floor(add_long(x, n / 2), n)
}

#[inline]
pub fn ft_abs(a: i32) -> i32 {
    if a < 0 {
        a.wrapping_neg()
    } else {
        a
    }
}
