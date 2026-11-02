//! Port of flex-fontkit.jar `flash.fonts.flashtype.ADFTypeSystem`.

pub const ADF_PEN_MOVETO_CMD: i32 = 0;
pub const ADF_PEN_LINETO_CMD: i32 = 1;
pub const ADF_PEN_CURVTO_CMD: i32 = 2;

#[derive(Clone, Debug, Default)]
pub struct ADFGenAttrs {
    pub max_level: i64,
    pub max_error: f32,
    pub dist_eps: f32,
}

#[derive(Clone, Debug, Default)]
pub struct ADFPath {
    pub glyph_min_x: f32,
    pub glyph_min_y: f32,
    pub glyph_max_x: f32,
    pub glyph_max_y: f32,
    pub font_units_per_em: f32,
    pub char_code: u16,
    pub num_contours: i32,
    pub num_pen_cmds: i64,
    pub pen_cmds: Vec<ADFPenCmd>,
    pub algn_zones: Option<Vec<i64>>,
    pub algn_zones_mask: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ADFPenCmd {
    pub op_code: i64,
    pub x: f32,
    pub y: f32,
    pub cx: f32,
    pub cy: f32,
}
