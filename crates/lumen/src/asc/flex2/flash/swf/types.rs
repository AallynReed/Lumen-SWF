//! Ports of `flash.swf.types`: the records inside SWF tags, with Java's `equals` as `jeq`.

use std::rc::Rc;

use super::swf_encoder::{max_num, min_bits};
use super::tags::{tag_eq, TagRef};

/// `Object.equals` for the record types, as each Java class defines it.
pub trait JEq {
    fn jeq(&self, other: &Self) -> bool;
}

impl<T: JEq> JEq for Option<T> {
    fn jeq(&self, other: &Self) -> bool {
        match (self, other) {
            (None, None) => true,
            (Some(a), Some(b)) => a.jeq(b),
            _ => false,
        }
    }
}

/// `Arrays.equals` / `ArrayLists.equals` over records.
pub fn list_eq<T: JEq>(a: &[T], b: &[T]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.jeq(y))
}

pub fn opt_list_eq<T: JEq>(a: &Option<Vec<T>>, b: &Option<Vec<T>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => list_eq(a, b),
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x_min: i32,
    pub x_max: i32,
    pub y_min: i32,
    pub y_max: i32,
}

impl Rect {
    pub fn new(x_min: i32, x_max: i32, y_min: i32, y_max: i32) -> Rect {
        Rect { x_min, x_max, y_min, y_max }
    }

    pub fn nbits(&self) -> i32 {
        min_bits(max_num(self.x_min, self.x_max, self.y_min, self.y_max), 1)
    }

    pub fn get_width(&self) -> i32 {
        self.x_max - self.x_min
    }

    pub fn get_height(&self) -> i32 {
        self.y_max - self.y_min
    }
}

impl JEq for Rect {
    fn jeq(&self, o: &Self) -> bool {
        self == o
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Matrix {
    pub has_scale: bool,
    pub scale_x: i32,
    pub scale_y: i32,
    pub has_rotate: bool,
    pub rotate_skew0: i32,
    pub rotate_skew1: i32,
    pub translate_x: i32,
    pub translate_y: i32,
}

impl Matrix {
    pub fn n_translate_bits(&self) -> i32 {
        min_bits(max_num(self.translate_x, self.translate_y, 0, 0), 1)
    }

    pub fn n_rotate_bits(&self) -> i32 {
        min_bits(max_num(self.rotate_skew0, self.rotate_skew1, 0, 0), 1)
    }

    pub fn n_scale_bits(&self) -> i32 {
        min_bits(max_num(self.scale_x, self.scale_y, 0, 0), 1)
    }

    pub fn set_rotate(&mut self, r0: f64, r1: f64) {
        self.has_rotate = true;
        self.rotate_skew0 = (65536.0 * r0) as i32;
        self.rotate_skew1 = (65536.0 * r1) as i32;
    }

    pub fn set_scale(&mut self, x: f64, y: f64) {
        self.has_scale = true;
        self.scale_x = (65536.0 * x) as i32;
        self.scale_y = (65536.0 * y) as i32;
    }
}

impl JEq for Matrix {
    fn jeq(&self, o: &Self) -> bool {
        self == o
    }
}

/// `CXForm`, and `CXFormWithAlpha` when `with_alpha`.
#[derive(Clone, Copy, Debug, Default)]
pub struct CXForm {
    pub with_alpha: bool,
    pub has_add: bool,
    pub has_mult: bool,
    pub red_mult_term: i32,
    pub green_mult_term: i32,
    pub blue_mult_term: i32,
    pub alpha_mult_term: i32,
    pub red_add_term: i32,
    pub green_add_term: i32,
    pub blue_add_term: i32,
    pub alpha_add_term: i32,
}

impl CXForm {
    pub fn nbits(&self) -> i32 {
        if self.with_alpha {
            let max_mult = max_num(self.red_mult_term, self.green_mult_term, self.blue_mult_term, self.alpha_mult_term);
            let max_add = max_num(self.red_add_term, self.green_add_term, self.blue_add_term, self.alpha_add_term);
            min_bits(max_num(max_mult, max_add, 0, 0), 1)
        } else {
            let max = max_num(self.red_mult_term, self.green_mult_term, self.blue_mult_term, self.red_add_term);
            let max = max_num(self.green_add_term, self.blue_add_term, max, 0);
            min_bits(max, 1)
        }
    }
}

impl JEq for CXForm {
    fn jeq(&self, o: &Self) -> bool {
        let base = o.has_add == self.has_add
            && o.has_mult == self.has_mult
            && o.red_mult_term == self.red_mult_term
            && o.green_mult_term == self.green_mult_term
            && o.blue_mult_term == self.blue_mult_term
            && o.red_add_term == self.red_add_term
            && o.green_add_term == self.green_add_term
            && o.blue_add_term == self.blue_add_term;
        if !self.with_alpha {
            return base;
        }
        base && o.with_alpha && o.alpha_mult_term == self.alpha_mult_term && o.alpha_add_term == self.alpha_add_term
    }
}

pub const FILL_SOLID: i32 = 0;
pub const FILL_GRADIENT: i32 = 0x10;
pub const FILL_LINEAR_GRADIENT: i32 = 0x10;
pub const FILL_RADIAL_GRADIENT: i32 = 0x12;
pub const FILL_FOCAL_RADIAL_GRADIENT: i32 = 0x13;
pub const FILL_BITS: i32 = 0x40;
pub const FILL_BITS_CLIP: i32 = 0x01;
pub const FILL_BITS_NOSMOOTH: i32 = 0x02;

#[derive(Clone, Default)]
pub struct FillStyle {
    pub type_: i32,
    pub color: i32,
    pub gradient: Option<Gradient>,
    pub matrix: Option<Matrix>,
    pub bitmap: Option<TagRef>,
}

impl FillStyle {
    pub fn new(type_: i32, matrix: Matrix, bitmap: TagRef) -> FillStyle {
        FillStyle { type_, color: 0, gradient: None, matrix: Some(matrix), bitmap: Some(bitmap) }
    }

    pub fn solid(color: i32) -> FillStyle {
        FillStyle { type_: FILL_SOLID, color, ..Default::default() }
    }

    pub fn has_bitmap_id(&self) -> bool {
        (self.type_ & FILL_BITS) != 0
    }
}

impl JEq for FillStyle {
    fn jeq(&self, o: &Self) -> bool {
        o.type_ == self.type_
            && o.color == self.color
            && match (&o.gradient, &self.gradient) {
                (None, None) => true,
                (Some(a), Some(b)) => a.jeq(b),
                _ => false,
            }
            && o.matrix.jeq(&self.matrix)
            && match (&o.bitmap, &self.bitmap) {
                (None, None) => true,
                (Some(a), Some(b)) => tag_eq(a, b),
                _ => false,
            }
    }
}

#[derive(Clone, Default)]
pub struct LineStyle {
    pub width: i32,
    pub color: i32,
    pub flags: i32,
    pub miter_limit: i32,
    pub fill_style: Option<FillStyle>,
}

impl LineStyle {
    pub fn has_fill_style(&self) -> bool {
        (self.flags & 0x0008) != 0
    }

    pub fn has_miter_joint(&self) -> bool {
        (self.flags & 0x0030) == 0x0020
    }
}

impl JEq for LineStyle {
    fn jeq(&self, o: &Self) -> bool {
        o.width == self.width && o.color == self.color
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GradRecord {
    pub ratio: i32,
    pub color: i32,
}

impl JEq for GradRecord {
    fn jeq(&self, o: &Self) -> bool {
        self == o
    }
}

/// `Gradient`, and `FocalGradient` when `focal_point` is set.
#[derive(Clone, Default)]
pub struct Gradient {
    pub spread_mode: i32,
    pub interpolation_mode: i32,
    pub records: Vec<GradRecord>,
    pub focal_point: Option<f32>,
}

impl JEq for Gradient {
    fn jeq(&self, o: &Self) -> bool {
        let base = o.spread_mode == self.spread_mode && o.interpolation_mode == self.interpolation_mode && list_eq(&o.records, &self.records);
        match self.focal_point {
            None => base,
            Some(f) => o.focal_point.is_some_and(|of| of == f) && base,
        }
    }
}

#[derive(Clone, Default)]
pub struct StyleChangeRecord {
    pub state_new_styles: bool,
    pub state_line_style: bool,
    pub state_fill_style1: bool,
    pub state_fill_style0: bool,
    pub state_move_to: bool,
    pub move_delta_x: i32,
    pub move_delta_y: i32,
    pub fillstyle0: i32,
    pub fillstyle1: i32,
    pub linestyle: i32,
    pub fillstyles: Option<Vec<FillStyle>>,
    pub linestyles: Option<Vec<LineStyle>>,
}

impl StyleChangeRecord {
    pub fn set_move(&mut self, x: i32, y: i32) {
        self.state_move_to = true;
        self.move_delta_x = x;
        self.move_delta_y = y;
    }

    pub fn n_move_bits(&self) -> i32 {
        min_bits(max_num(self.move_delta_x, self.move_delta_y, 0, 0), 1)
    }

    pub fn set_fill_style1(&mut self, index: i32) {
        self.state_fill_style1 = true;
        self.fillstyle1 = index;
    }

    pub fn set_fill_style0(&mut self, index: i32) {
        self.state_fill_style0 = true;
        self.fillstyle0 = index;
    }

    pub fn set_linestyle(&mut self, index: i32) {
        self.state_line_style = true;
        self.linestyle = index;
    }
}

#[derive(Clone)]
pub enum ShapeRecord {
    StyleChange(StyleChangeRecord),
    StraightEdge { delta_x: i32, delta_y: i32 },
    CurvedEdge { control_delta_x: i32, control_delta_y: i32, anchor_delta_x: i32, anchor_delta_y: i32 },
}

impl ShapeRecord {
    pub fn straight(delta_x: i32, delta_y: i32) -> ShapeRecord {
        ShapeRecord::StraightEdge { delta_x, delta_y }
    }

    /// `getReferenceList`.
    pub fn get_reference_list(&self, list: &mut Vec<TagRef>) {
        if let ShapeRecord::StyleChange(s) = self {
            for style in s.fillstyles.iter().flatten() {
                if style.has_bitmap_id() {
                    if let Some(b) = &style.bitmap {
                        list.push(b.clone());
                    }
                }
            }
        }
    }
}

impl JEq for ShapeRecord {
    fn jeq(&self, o: &Self) -> bool {
        match (self, o) {
            (ShapeRecord::StyleChange(a), ShapeRecord::StyleChange(b)) => {
                b.state_new_styles == a.state_new_styles
                    && b.state_line_style == a.state_line_style
                    && b.state_fill_style1 == a.state_fill_style1
                    && b.state_fill_style0 == a.state_fill_style0
                    && b.state_move_to == a.state_move_to
                    && b.move_delta_x == a.move_delta_x
                    && b.move_delta_y == a.move_delta_y
                    && b.fillstyle0 == a.fillstyle0
                    && b.fillstyle1 == a.fillstyle1
                    && b.linestyle == a.linestyle
                    && opt_list_eq(&b.fillstyles, &a.fillstyles)
                    && opt_list_eq(&b.linestyles, &a.linestyles)
            }
            (ShapeRecord::StraightEdge { delta_x: ax, delta_y: ay }, ShapeRecord::StraightEdge { delta_x: bx, delta_y: by }) => ax == bx && ay == by,
            (
                ShapeRecord::CurvedEdge { control_delta_x: a0, control_delta_y: a1, anchor_delta_x: a2, anchor_delta_y: a3 },
                ShapeRecord::CurvedEdge { control_delta_x: b0, control_delta_y: b1, anchor_delta_x: b2, anchor_delta_y: b3 },
            ) => a0 == b0 && a1 == b1 && a2 == b2 && a3 == b3,
            _ => false,
        }
    }
}

/// `Shape`.
#[derive(Clone, Default)]
pub struct Shape {
    pub shape_records: Option<Vec<ShapeRecord>>,
}

impl Shape {
    pub fn get_reference_list(&self, refs: &mut Vec<TagRef>) {
        for rec in self.shape_records.iter().flatten() {
            rec.get_reference_list(refs);
        }
    }
}

impl JEq for Shape {
    fn jeq(&self, o: &Self) -> bool {
        opt_list_eq(&o.shape_records, &self.shape_records)
    }
}

/// `ShapeWithStyle`.
#[derive(Clone, Default)]
pub struct ShapeWithStyle {
    pub shape_records: Option<Vec<ShapeRecord>>,
    pub fillstyles: Option<Vec<FillStyle>>,
    pub linestyles: Option<Vec<LineStyle>>,
}

impl ShapeWithStyle {
    pub fn get_reference_list(&self, refs: &mut Vec<TagRef>) {
        for rec in self.shape_records.iter().flatten() {
            rec.get_reference_list(refs);
        }
        for style in self.fillstyles.iter().flatten() {
            if style.has_bitmap_id() {
                if let Some(b) = &style.bitmap {
                    refs.push(b.clone());
                }
            }
        }
    }
}

impl JEq for ShapeWithStyle {
    fn jeq(&self, o: &Self) -> bool {
        opt_list_eq(&o.shape_records, &self.shape_records) && opt_list_eq(&o.fillstyles, &self.fillstyles) && opt_list_eq(&o.linestyles, &self.linestyles)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MorphGradRecord {
    pub start_ratio: i32,
    pub end_ratio: i32,
    pub start_color: i32,
    pub end_color: i32,
}

impl JEq for MorphGradRecord {
    fn jeq(&self, o: &Self) -> bool {
        self == o
    }
}

#[derive(Clone, Default)]
pub struct MorphFillStyle {
    pub type_: i32,
    pub start_color: i32,
    pub end_color: i32,
    pub start_gradient_matrix: Option<Matrix>,
    pub end_gradient_matrix: Option<Matrix>,
    pub grad_records: Option<Vec<MorphGradRecord>>,
    pub bitmap: Option<TagRef>,
    pub start_bitmap_matrix: Option<Matrix>,
    pub end_bitmap_matrix: Option<Matrix>,
    pub ratio1: i32,
    pub ratio2: i32,
}

impl MorphFillStyle {
    pub fn has_bitmap_id(&self) -> bool {
        (self.type_ & FILL_BITS) != 0
    }
}

impl JEq for MorphFillStyle {
    fn jeq(&self, o: &Self) -> bool {
        o.type_ == self.type_
            && o.start_color == self.start_color
            && o.end_color == self.end_color
            && o.ratio1 == self.ratio1
            && o.ratio2 == self.ratio2
            && o.start_gradient_matrix.jeq(&self.start_gradient_matrix)
            && o.end_gradient_matrix.jeq(&self.end_gradient_matrix)
            && opt_list_eq(&o.grad_records, &self.grad_records)
            && match (&o.bitmap, &self.bitmap) {
                (None, None) => true,
                (Some(a), Some(b)) => tag_eq(a, b),
                _ => false,
            }
            && o.start_bitmap_matrix.jeq(&self.start_bitmap_matrix)
            && o.end_bitmap_matrix.jeq(&self.end_bitmap_matrix)
    }
}

#[derive(Clone, Default)]
pub struct MorphLineStyle {
    pub start_width: i32,
    pub end_width: i32,
    pub start_caps_style: i32,
    pub joint_style: i32,
    pub has_fill: bool,
    pub no_h_scale: bool,
    pub no_v_scale: bool,
    pub pixel_hinting: bool,
    pub no_close: bool,
    pub end_caps_style: i32,
    pub miter_limit: i32,
    pub fill_type: Option<MorphFillStyle>,
    pub start_color: i32,
    pub end_color: i32,
}

impl JEq for MorphLineStyle {
    fn jeq(&self, o: &Self) -> bool {
        o.start_width == self.start_width
            && o.end_width == self.end_width
            && o.start_caps_style == self.start_caps_style
            && o.joint_style == self.joint_style
            && o.has_fill == self.has_fill
            && o.no_h_scale == self.no_h_scale
            && o.no_v_scale == self.no_v_scale
            && o.pixel_hinting == self.pixel_hinting
            && o.no_close == self.no_close
            && o.end_caps_style == self.end_caps_style
            && o.miter_limit == self.miter_limit
            && o.fill_type.jeq(&self.fill_type)
            && o.start_color == self.start_color
            && o.end_color == self.end_color
    }
}

/// The `flash.swf.types` filters; compared by identity, as Java does.
#[derive(Clone, Debug)]
pub enum Filter {
    DropShadow { color: i32, blur_x: i32, blur_y: i32, angle: i32, distance: i32, strength: i32, flags: i32 },
    Blur { blur_x: i32, blur_y: i32, passes: i32 },
    Glow { color: i32, blur_x: i32, blur_y: i32, strength: i32, flags: i32 },
    Bevel { shadow_color: i32, highlight_color: i32, blur_x: i32, blur_y: i32, angle: i32, distance: i32, strength: i32, flags: i32 },
    GradientGlow(GradientFilter),
    Convolution { matrix_x: i32, matrix_y: i32, divisor: f32, bias: f32, matrix: Vec<f32>, flags: i32 },
    ColorMatrix { values: Vec<f32> },
    GradientBevel(GradientFilter),
}

#[derive(Clone, Debug, Default)]
pub struct GradientFilter {
    pub numcolors: i32,
    pub gradient_colors: Vec<i32>,
    pub gradient_ratio: Vec<i32>,
    pub blur_x: i32,
    pub blur_y: i32,
    pub angle: i32,
    pub distance: i32,
    pub strength: i32,
    pub flags: i32,
}

impl Filter {
    pub fn get_id(&self) -> i32 {
        match self {
            Filter::DropShadow { .. } => 0,
            Filter::Blur { .. } => 1,
            Filter::Glow { .. } => 2,
            Filter::Bevel { .. } => 3,
            Filter::GradientGlow(_) => 4,
            Filter::Convolution { .. } => 5,
            Filter::ColorMatrix { .. } => 6,
            Filter::GradientBevel(_) => 7,
        }
    }
}

pub type FilterList = Vec<Rc<Filter>>;

fn filters_eq(a: &Option<FilterList>, b: &Option<FilterList>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.len() == b.len() && a.iter().zip(b).all(|(x, y)| Rc::ptr_eq(x, y)),
        _ => false,
    }
}

#[derive(Clone)]
pub struct ButtonRecord {
    pub hit_test: bool,
    pub down: bool,
    pub over: bool,
    pub up: bool,
    pub character_ref: Option<TagRef>,
    pub place_depth: i32,
    pub place_matrix: Option<Matrix>,
    pub color_transform: Option<CXForm>,
    pub filters: Option<FilterList>,
    pub blend_mode: i32,
}

impl Default for ButtonRecord {
    fn default() -> Self {
        ButtonRecord {
            hit_test: false,
            down: false,
            over: false,
            up: false,
            character_ref: None,
            place_depth: 0,
            place_matrix: None,
            color_transform: None,
            filters: None,
            blend_mode: -1,
        }
    }
}

impl JEq for ButtonRecord {
    fn jeq(&self, o: &Self) -> bool {
        o.hit_test == self.hit_test
            && o.down == self.down
            && o.over == self.over
            && o.up == self.up
            && o.blend_mode == self.blend_mode
            && filters_eq(&o.filters, &self.filters)
            && match (&o.character_ref, &self.character_ref) {
                (None, None) => true,
                (Some(a), Some(b)) => tag_eq(a, b),
                _ => false,
            }
            && o.place_depth == self.place_depth
            && o.place_matrix.jeq(&self.place_matrix)
            && o.color_transform.jeq(&self.color_transform)
    }
}

/// `ActionList`, kept as the encoded action records (without the end-of-actions byte).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActionList {
    pub bytes: Vec<u8>,
}

impl JEq for ActionList {
    fn jeq(&self, o: &Self) -> bool {
        self == o
    }
}

#[derive(Clone, Default)]
pub struct ButtonCondAction {
    pub key_press: i32,
    pub over_down_to_idle: bool,
    pub idle_to_over_down: bool,
    pub out_down_to_idle: bool,
    pub out_down_to_over_down: bool,
    pub over_down_to_out_down: bool,
    pub over_down_to_over_up: bool,
    pub over_up_to_over_down: bool,
    pub over_up_to_idle: bool,
    pub idle_to_over_up: bool,
    pub action_list: Option<ActionList>,
}

impl JEq for ButtonCondAction {
    fn jeq(&self, o: &Self) -> bool {
        o.key_press == self.key_press
            && o.over_down_to_idle == self.over_down_to_idle
            && o.idle_to_over_down == self.idle_to_over_down
            && o.out_down_to_idle == self.out_down_to_idle
            && o.out_down_to_over_down == self.out_down_to_over_down
            && o.over_down_to_out_down == self.over_down_to_out_down
            && o.over_down_to_over_up == self.over_down_to_over_up
            && o.over_up_to_over_down == self.over_up_to_over_down
            && o.over_up_to_idle == self.over_up_to_idle
            && o.idle_to_over_up == self.idle_to_over_up
            && o.action_list.jeq(&self.action_list)
    }
}

/// `ClipActions`, kept as its encoded bytes (from the reserved UI16 through the end flag).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClipActions {
    pub bytes: Vec<u8>,
}

impl JEq for ClipActions {
    fn jeq(&self, o: &Self) -> bool {
        self == o
    }
}

pub const SOUND_INFO_UNINITIALIZED: i64 = -1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundInfo {
    pub sync_stop: bool,
    pub sync_no_multiple: bool,
    pub in_point: i64,
    pub out_point: i64,
    pub loop_count: i32,
    pub records: Option<Vec<i64>>,
}

impl Default for SoundInfo {
    fn default() -> Self {
        SoundInfo { sync_stop: false, sync_no_multiple: false, in_point: -1, out_point: -1, loop_count: -1, records: Some(Vec::new()) }
    }
}

impl JEq for SoundInfo {
    fn jeq(&self, o: &Self) -> bool {
        self == o
    }
}

#[derive(Clone, Debug, Default)]
pub struct GlyphEntry {
    pub index: i32,
    pub advance: i32,
}

impl GlyphEntry {
    pub fn get_index(&self) -> i32 {
        self.index
    }
}

impl JEq for GlyphEntry {
    fn jeq(&self, o: &Self) -> bool {
        o.index == self.index && o.advance == self.advance
    }
}

#[derive(Clone)]
pub struct TextRecord {
    pub flags: i32,
    pub color: i32,
    pub x_offset: i32,
    pub y_offset: i32,
    pub height: i32,
    pub font: Option<TagRef>,
    pub entries: Vec<GlyphEntry>,
}

impl Default for TextRecord {
    fn default() -> Self {
        TextRecord { flags: 128, color: 0, x_offset: 0, y_offset: 0, height: 0, font: None, entries: Vec::new() }
    }
}

impl TextRecord {
    pub fn has_font(&self) -> bool {
        (self.flags & 8) != 0
    }
    pub fn has_color(&self) -> bool {
        (self.flags & 4) != 0
    }
    pub fn has_x(&self) -> bool {
        (self.flags & 1) != 0
    }
    pub fn has_y(&self) -> bool {
        (self.flags & 2) != 0
    }
    pub fn has_height(&self) -> bool {
        (self.flags & 8) != 0
    }

    pub fn get_reference_list(&self, refs: &mut Vec<TagRef>) {
        if self.has_font() {
            if let Some(f) = &self.font {
                refs.push(f.clone());
            }
        }
    }
}

impl JEq for TextRecord {
    fn jeq(&self, o: &Self) -> bool {
        o.flags == self.flags
            && o.color == self.color
            && o.x_offset == self.x_offset
            && o.y_offset == self.y_offset
            && o.height == self.height
            && match (&o.font, &self.font) {
                (None, None) => true,
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                _ => false,
            }
            && list_eq(&o.entries, &self.entries)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KerningRecord {
    pub code1: i32,
    pub code2: i32,
    pub adjustment: i32,
}

impl JEq for KerningRecord {
    fn jeq(&self, o: &Self) -> bool {
        self == o
    }
}

/// `flash.swf.tags.ZoneRecord`; its `equals` starts from `Object.equals`, so only the same record is equal.
#[derive(Debug, Default)]
pub struct ZoneRecord {
    pub num_zone_data: i32,
    pub zone_data: Vec<i64>,
    pub zone_mask: i32,
}

impl JEq for Rc<ZoneRecord> {
    fn jeq(&self, o: &Self) -> bool {
        Rc::ptr_eq(self, o)
    }
}
