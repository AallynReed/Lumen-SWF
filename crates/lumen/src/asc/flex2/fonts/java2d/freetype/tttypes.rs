//! Port of FreeType 2.14.2 `include/freetype/internal/tttypes.h` and `tttables.h` (as built into JDK 17.0.19).

use crate::asc::flex2::fonts::java2d::freetype::ftstream::FtStream;
use crate::asc::flex2::fonts::java2d::freetype::fttypes::*;

pub const fn tag(s: &[u8; 4]) -> u32 {
    ft_image_tag(s[0], s[1], s[2], s[3])
}

pub const TTAG_TTCF: u32 = tag(b"ttcf");
pub const TTAG_OTTO: u32 = tag(b"OTTO");
pub const TTAG_TRUE: u32 = tag(b"true");
pub const TTAG_TYP1: u32 = tag(b"typ1");
pub const TTAG_0XA5KBD: u32 = 0xA56B_6264;
pub const TTAG_0XA5LST: u32 = 0xA56C_7374;
pub const TTAG_HEAD: u32 = tag(b"head");
pub const TTAG_BHED: u32 = tag(b"bhed");
pub const TTAG_SING: u32 = tag(b"SING");
pub const TTAG_META: u32 = tag(b"META");
pub const TTAG_HMTX: u32 = tag(b"hmtx");
pub const TTAG_VMTX: u32 = tag(b"vmtx");
pub const TTAG_HHEA: u32 = tag(b"hhea");
pub const TTAG_VHEA: u32 = tag(b"vhea");
pub const TTAG_MAXP: u32 = tag(b"maxp");
pub const TTAG_NAME: u32 = tag(b"name");
pub const TTAG_CMAP: u32 = tag(b"cmap");
pub const TTAG_POST: u32 = tag(b"post");
pub const TTAG_OS2: u32 = tag(b"OS/2");
pub const TTAG_GLYF: u32 = tag(b"glyf");
pub const TTAG_LOCA: u32 = tag(b"loca");
pub const TTAG_CFF: u32 = tag(b"CFF ");
pub const TTAG_CFF2: u32 = tag(b"CFF2");
pub const TTAG_CVT: u32 = tag(b"cvt ");
pub const TTAG_FPGM: u32 = tag(b"fpgm");
pub const TTAG_PREP: u32 = tag(b"prep");
pub const TTAG_HDMX: u32 = tag(b"hdmx");
pub const TTAG_CBLC: u32 = tag(b"CBLC");
pub const TTAG_CBDT: u32 = tag(b"CBDT");
pub const TTAG_EBLC: u32 = tag(b"EBLC");
pub const TTAG_BLOC: u32 = tag(b"bloc");
pub const TTAG_SBIX: u32 = tag(b"sbix");
pub const TTAG_FVAR: u32 = tag(b"fvar");

pub const TT_FACE_FLAG_VAR_FVAR: u32 = 1 << 0;
pub const TT_FACE_FLAG_VAR_HADVANCE: u32 = 1 << 1;
pub const TT_FACE_FLAG_VAR_VADVANCE: u32 = 1 << 5;

pub const TT_PLATFORM_APPLE_UNICODE: u16 = 0;
pub const TT_PLATFORM_MACINTOSH: u16 = 1;
pub const TT_PLATFORM_ISO: u16 = 2;
pub const TT_PLATFORM_MICROSOFT: u16 = 3;
pub const TT_MAC_ID_ROMAN: u16 = 0;
pub const TT_MAC_LANGID_ENGLISH: u16 = 0;
pub const TT_MS_ID_SYMBOL_CS: u16 = 0;
pub const TT_MS_ID_UNICODE_CS: u16 = 1;
pub const TT_MS_ID_UCS_4: u16 = 10;

pub const TT_NAME_ID_FONT_FAMILY: u16 = 1;
pub const TT_NAME_ID_FONT_SUBFAMILY: u16 = 2;
pub const TT_NAME_ID_TYPOGRAPHIC_FAMILY: u16 = 16;
pub const TT_NAME_ID_TYPOGRAPHIC_SUBFAMILY: u16 = 17;
pub const TT_NAME_ID_WWS_FAMILY: u16 = 21;
pub const TT_NAME_ID_WWS_SUBFAMILY: u16 = 22;

#[derive(Clone, Copy, Default, Debug)]
pub struct TtTable {
    pub tag: u32,
    pub check_sum: u32,
    pub offset: u32,
    pub length: u32,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct TtHeader {
    pub table_version: u32,
    pub font_revision: u32,
    pub check_sum_adjust: i32,
    pub magic_number: i32,
    pub flags: u16,
    pub units_per_em: u16,
    pub created: [u32; 2],
    pub modified: [u32; 2],
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
    pub mac_style: u16,
    pub lowest_rec_ppem: u16,
    pub font_direction: i16,
    pub index_to_loc_format: i16,
    pub glyph_data_format: i16,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct TtMaxProfile {
    pub version: i32,
    pub num_glyphs: u16,
    pub max_points: u16,
    pub max_contours: u16,
    pub max_composite_points: u16,
    pub max_composite_contours: u16,
    pub max_zones: u16,
    pub max_twilight_points: u16,
    pub max_storage: u16,
    pub max_function_defs: u16,
    pub max_instruction_defs: u16,
    pub max_stack_elements: u16,
    pub max_size_of_instructions: u16,
    pub max_component_elements: u16,
    pub max_component_depth: u16,
}

/// `TT_HoriHeader`, also used for `vhea` (`number_of_hmetrics` is then `number_Of_VMetrics`).
#[derive(Clone, Copy, Default, Debug)]
pub struct TtHoriHeader {
    pub version: u32,
    pub ascender: i16,
    pub descender: i16,
    pub line_gap: i16,
    pub advance_width_max: u16,
    pub min_left_side_bearing: i16,
    pub min_right_side_bearing: i16,
    pub x_max_extent: i16,
    pub caret_slope_rise: i16,
    pub caret_slope_run: i16,
    pub caret_offset: i16,
    pub reserved: [i16; 4],
    pub metric_data_format: i16,
    pub number_of_hmetrics: u16,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct TtOs2 {
    pub version: u16,
    pub x_avg_char_width: i16,
    pub us_weight_class: u16,
    pub us_width_class: u16,
    pub fs_type: i16,
    pub y_subscript_x_size: i16,
    pub y_subscript_y_size: i16,
    pub y_subscript_x_offset: i16,
    pub y_subscript_y_offset: i16,
    pub y_superscript_x_size: i16,
    pub y_superscript_y_size: i16,
    pub y_superscript_x_offset: i16,
    pub y_superscript_y_offset: i16,
    pub y_strikeout_size: i16,
    pub y_strikeout_position: i16,
    pub s_family_class: i16,
    pub panose: [u8; 10],
    pub ul_unicode_range: [u32; 4],
    pub ach_vend_id: [u8; 4],
    pub fs_selection: u16,
    pub us_first_char_index: u16,
    pub us_last_char_index: u16,
    pub s_typo_ascender: i16,
    pub s_typo_descender: i16,
    pub s_typo_line_gap: i16,
    pub us_win_ascent: u16,
    pub us_win_descent: u16,
    pub ul_code_page_range1: u32,
    pub ul_code_page_range2: u32,
    pub sx_height: i16,
    pub s_cap_height: i16,
    pub us_default_char: u16,
    pub us_break_char: u16,
    pub us_max_context: u16,
    pub us_lower_optical_point_size: u16,
    pub us_upper_optical_point_size: u16,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct TtPostscript {
    pub format_type: i32,
    pub italic_angle: i32,
    pub underline_position: i16,
    pub underline_thickness: i16,
    pub is_fixed_pitch: u32,
    pub min_mem_type42: u32,
    pub max_mem_type42: u32,
    pub min_mem_type1: u32,
    pub max_mem_type1: u32,
}

#[derive(Clone, Default, Debug)]
pub struct TtNameRec {
    pub platform_id: u16,
    pub encoding_id: u16,
    pub language_id: u16,
    pub name_id: u16,
    pub string_length: u16,
    pub string_offset: u32,
    pub string: Option<Vec<u8>>,
}

#[derive(Clone, Default, Debug)]
pub struct TtLangTagRec {
    pub string_length: u16,
    pub string_offset: u32,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct FtBitmapSize {
    pub height: i16,
    pub width: i16,
    pub size: FtPos,
    pub x_ppem: FtPos,
    pub y_ppem: FtPos,
}

/// `TT_FaceRec` without the embedded `FT_FaceRec` root (that lives in `FtFace`).
#[derive(Clone, Default)]
pub struct TtFace {
    pub stream: FtStream,
    pub ttc_version: i32,
    pub ttc_offsets: Vec<u32>,
    pub format_tag: u32,
    pub dir_tables: Vec<TtTable>,
    pub header: TtHeader,
    pub horizontal: TtHoriHeader,
    pub max_profile: TtMaxProfile,
    pub vertical_info: bool,
    pub vertical: TtHoriHeader,
    pub os2: TtOs2,
    pub postscript: TtPostscript,
    pub cmap_table: Vec<u8>,
    pub name_format: u16,
    pub names: Vec<TtNameRec>,
    pub lang_tags: Vec<TtLangTagRec>,
    pub horz_metrics_offset: u32,
    pub horz_metrics_size: u32,
    pub vert_metrics_offset: u32,
    pub vert_metrics_size: u32,
    pub num_locations: u32,
    pub glyph_locations: Vec<u8>,
    pub glyf_len: u32,
    pub glyf_offset: u32,
    pub hdmx_table: Vec<u8>,
    pub hdmx_record_count: u32,
    pub hdmx_record_size: u32,
    pub hdmx_records: Vec<usize>,
    pub font_program: Vec<u8>,
    pub cvt_program: Vec<u8>,
    pub cvt: Vec<i32>,
    pub sbit_num_strikes: u32,
    pub sbit_strike_map: Vec<u32>,
    pub sbit_table_type: u32,
    pub sbit_strikes: Vec<(u8, u8, i8, i8, u8)>,
    pub sbit_table: Vec<u8>,
    pub variation_support: u32,
    pub var_default_named_instance: u32,
}
