//! Ports of `flash.swf.Tag`, `flash.swf.tags.*` and `flash.swf.TagValues`: the SWF tag object graph, with each
//! class's `equals` (which `Dictionary` and `MovieEncoder` rely on to merge equal definitions) and `getReferences`.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use super::types::*;

pub type TagRef = Rc<RefCell<Tag>>;
pub type WeakTag = Weak<RefCell<Tag>>;

pub mod codes {
    pub const END: i32 = 0;
    pub const SHOW_FRAME: i32 = 1;
    pub const DEFINE_SHAPE: i32 = 2;
    pub const PLACE_OBJECT: i32 = 4;
    pub const REMOVE_OBJECT: i32 = 5;
    pub const DEFINE_BITS: i32 = 6;
    pub const DEFINE_BUTTON: i32 = 7;
    pub const JPEG_TABLES: i32 = 8;
    pub const SET_BACKGROUND_COLOR: i32 = 9;
    pub const DEFINE_FONT: i32 = 10;
    pub const DEFINE_TEXT: i32 = 11;
    pub const DO_ACTION: i32 = 12;
    pub const DEFINE_FONT_INFO: i32 = 13;
    pub const DEFINE_SOUND: i32 = 14;
    pub const START_SOUND: i32 = 15;
    pub const DEFINE_BUTTON_SOUND: i32 = 17;
    pub const SOUND_STREAM_HEAD: i32 = 18;
    pub const SOUND_STREAM_BLOCK: i32 = 19;
    pub const DEFINE_BITS_LOSSLESS: i32 = 20;
    pub const DEFINE_BITS_JPEG2: i32 = 21;
    pub const DEFINE_SHAPE2: i32 = 22;
    pub const DEFINE_BUTTON_CXFORM: i32 = 23;
    pub const PROTECT: i32 = 24;
    pub const PLACE_OBJECT2: i32 = 26;
    pub const REMOVE_OBJECT2: i32 = 28;
    pub const DEFINE_SHAPE3: i32 = 32;
    pub const DEFINE_TEXT2: i32 = 33;
    pub const DEFINE_BUTTON2: i32 = 34;
    pub const DEFINE_BITS_JPEG3: i32 = 35;
    pub const DEFINE_BITS_LOSSLESS2: i32 = 36;
    pub const DEFINE_EDIT_TEXT: i32 = 37;
    pub const DEFINE_SPRITE: i32 = 39;
    pub const PRODUCT_INFO: i32 = 41;
    pub const FRAME_LABEL: i32 = 43;
    pub const SOUND_STREAM_HEAD2: i32 = 45;
    pub const DEFINE_MORPH_SHAPE: i32 = 46;
    pub const DEFINE_FONT2: i32 = 48;
    pub const EXPORT_ASSETS: i32 = 56;
    pub const IMPORT_ASSETS: i32 = 57;
    pub const ENABLE_DEBUGGER: i32 = 58;
    pub const DO_INIT_ACTION: i32 = 59;
    pub const DEFINE_VIDEO_STREAM: i32 = 60;
    pub const VIDEO_FRAME: i32 = 61;
    pub const DEFINE_FONT_INFO2: i32 = 62;
    pub const DEBUG_ID: i32 = 63;
    pub const ENABLE_DEBUGGER2: i32 = 64;
    pub const SCRIPT_LIMITS: i32 = 65;
    pub const SET_TAB_INDEX: i32 = 66;
    pub const FILE_ATTRIBUTES: i32 = 69;
    pub const PLACE_OBJECT3: i32 = 70;
    pub const IMPORT_ASSETS2: i32 = 71;
    pub const DO_ABC: i32 = 72;
    pub const DEFINE_FONT_ALIGN_ZONES: i32 = 73;
    pub const CSM_TEXT_SETTINGS: i32 = 74;
    pub const DEFINE_FONT3: i32 = 75;
    pub const SYMBOL_CLASS: i32 = 76;
    pub const METADATA: i32 = 77;
    pub const DEFINE_SCALING_GRID: i32 = 78;
    pub const DO_ABC2: i32 = 82;
    pub const DEFINE_SHAPE4: i32 = 83;
    pub const DEFINE_MORPH_SHAPE2: i32 = 84;
    pub const DEFINE_SCENE_AND_FRAME_LABEL_DATA: i32 = 86;
    pub const DEFINE_BINARY_DATA: i32 = 87;
    pub const DEFINE_FONT_NAME: i32 = 88;
    pub const DEFINE_FONT4: i32 = 91;
}

/// `TagValues.names`, as `BadScalingGridTarget` reports them.
pub fn tag_name(code: i32) -> &'static str {
    const NAMES: [&str; 92] = [
        "End", "ShowFrame", "DefineShape", "FreeCharacter", "PlaceObject", "RemoveObject", "DefineBits", "DefineButton", "JPEGTables",
        "SetBackgroundColor", "DefineFont", "DefineText", "DoAction", "DefineFontInfo", "DefineSound", "StartSound", "StopSound",
        "DefineButtonSound", "SoundStreamHead", "SoundStreamBlock", "DefineBitsLossless", "DefineBitsJPEG2", "DefineShape2",
        "DefineButtonCxform", "Protect", "PathsArePostScript", "PlaceObject2", "27 (invalid)", "RemoveObject2", "SyncFrame",
        "30 (invalid)", "FreeAll", "DefineShape3", "DefineText2", "DefineButton2", "DefineBitsJPEG3", "DefineBitsLossless2",
        "DefineEditText", "DefineVideo", "DefineSprite", "NameCharacter", "ProductInfo", "DefineTextFormat", "FrameLabel",
        "DefineBehavior", "SoundStreamHead2", "DefineMorphShape", "FrameTag", "DefineFont2", "GenCommand", "DefineCommandObj",
        "CharacterSet", "FontRef", "DefineFunction", "PlaceFunction", "GenTagObject", "ExportAssets", "ImportAssets",
        "EnableDebugger", "DoInitAction", "DefineVideoStream", "VideoFrame", "DefineFontInfo2", "DebugID", "EnableDebugger2",
        "ScriptLimits", "SetTabIndex", "DefineShape4", "68 (invalid)", "FileAttributes", "PlaceObject3", "ImportAssets2", "DoABC",
        "DefineFontAlignZones", "CSMTextSettings", "DefineFont3", "SymbolClass", "Metadata", "ScalingGrid", "79 (invalid)",
        "80 (invalid)", "81 (invalid)", "DoABC2", "DefineShape4", "DefineMorphShape2", "85 (invalid)", "DefineSceneAndFrameLabelData",
        "DefineBinaryData", "DefineFontName", "89 (unknown)  ", "90 (unknown)  ", "DefineFont4",
    ];
    NAMES.get(code as usize).copied().unwrap_or("unknown")
}

/// Which `DefineBits` subclass a bitmap tag is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BitsKind {
    Bits,
    Jpeg3,
    Lossless,
}

#[derive(Clone)]
pub struct DefineBits {
    pub kind: BitsKind,
    pub jpeg_tables: Option<TagRef>,
    pub data: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub alpha_data_offset: i64,
    pub alpha_data: Vec<u8>,
    pub format: i32,
    pub color_data: Option<Vec<i32>>,
}

impl DefineBits {
    pub fn new(kind: BitsKind) -> DefineBits {
        DefineBits { kind, jpeg_tables: None, data: Vec::new(), width: 0, height: 0, alpha_data_offset: 0, alpha_data: Vec::new(), format: 0, color_data: None }
    }
}

pub const FORMAT_8_BIT_COLORMAPPED: i32 = 3;
pub const FORMAT_15_BIT_RGB: i32 = 4;
pub const FORMAT_24_BIT_RGB: i32 = 5;

#[derive(Clone, Default)]
pub struct DefineSound {
    pub format: i32,
    pub rate: i32,
    pub size: i32,
    pub type_: i32,
    pub sample_count: i64,
    pub data: Vec<u8>,
}

#[derive(Clone, Default)]
pub struct DefineSprite {
    pub framecount: i32,
    pub tag_list: Vec<TagRef>,
    pub init_action: Option<TagRef>,
    pub scaling_grid: Option<TagRef>,
    pub depth_counter: i32,
}

#[derive(Clone, Default)]
pub struct DefineShape {
    pub bounds: Rect,
    pub shape_with_style: ShapeWithStyle,
    pub uses_fill_winding_rule: bool,
    pub uses_non_scaling_strokes: bool,
    pub uses_scaling_strokes: bool,
    pub edge_bounds: Option<Rect>,
    pub scaling_grid: Option<TagRef>,
}

#[derive(Clone, Default)]
pub struct DefineScalingGrid {
    pub scaling_target: Option<WeakTag>,
    pub rect: Rect,
}

pub const HAS_CLIP_ACTION: i32 = 1 << 7;
pub const HAS_CLIP_DEPTH: i32 = 1 << 6;
pub const HAS_NAME: i32 = 1 << 5;
pub const HAS_RATIO: i32 = 1 << 4;
pub const HAS_CXFORM: i32 = 1 << 3;
pub const HAS_MATRIX: i32 = 1 << 2;
pub const HAS_CHARACTER: i32 = 1 << 1;
pub const HAS_MOVE: i32 = 1;
pub const HAS_IMAGE: i32 = 1 << 4;
pub const HAS_CLASS_NAME: i32 = 1 << 3;
pub const HAS_CACHE_AS_BITMAP: i32 = 1 << 2;
pub const HAS_BLEND_MODE: i32 = 1 << 1;
pub const HAS_FILTER_LIST: i32 = 1;

#[derive(Clone, Default)]
pub struct PlaceObject {
    pub flags: i32,
    pub flags2: i32,
    pub ratio: i32,
    pub name: Option<String>,
    pub clip_depth: i32,
    pub clip_actions: Option<ClipActions>,
    pub depth: i32,
    pub matrix: Option<Matrix>,
    pub color_transform: Option<CXForm>,
    pub ref_: Option<TagRef>,
    pub filters: Option<FilterList>,
    pub blend_mode: i32,
    pub class_name: Option<String>,
}

impl PlaceObject {
    pub fn has_clip_action(&self) -> bool {
        (self.flags & HAS_CLIP_ACTION) != 0
    }
    pub fn has_clip_depth(&self) -> bool {
        (self.flags & HAS_CLIP_DEPTH) != 0
    }
    pub fn has_name(&self) -> bool {
        (self.flags & HAS_NAME) != 0
    }
    pub fn has_ratio(&self) -> bool {
        (self.flags & HAS_RATIO) != 0
    }
    pub fn has_char_id(&self) -> bool {
        (self.flags & HAS_CHARACTER) != 0
    }
    pub fn has_matrix(&self) -> bool {
        (self.flags & HAS_MATRIX) != 0
    }
    pub fn has_cxform(&self) -> bool {
        (self.flags & HAS_CXFORM) != 0
    }
    pub fn has_filter_list(&self) -> bool {
        (self.flags2 & HAS_FILTER_LIST) != 0
    }
    pub fn has_blend_mode(&self) -> bool {
        (self.flags2 & HAS_BLEND_MODE) != 0
    }
    pub fn has_class_name(&self) -> bool {
        (self.flags2 & HAS_CLASS_NAME) != 0
    }

    pub fn set_ref(&mut self, r: TagRef) {
        self.ref_ = Some(r);
        self.flags |= HAS_CHARACTER;
    }

    pub fn set_matrix(&mut self, m: Option<Matrix>) {
        self.flags = if m.is_some() { self.flags | HAS_MATRIX } else { self.flags & !HAS_MATRIX };
        self.matrix = m;
    }

    pub fn set_name(&mut self, name: Option<String>) {
        self.flags = if name.is_some() { self.flags | HAS_NAME } else { self.flags & !HAS_NAME };
        self.name = name;
    }

    pub fn set_clip_actions(&mut self, actions: Option<ClipActions>) {
        self.flags = if actions.is_some() { self.flags | HAS_CLIP_ACTION } else { self.flags & !HAS_CLIP_ACTION };
        self.clip_actions = actions;
    }
}

#[derive(Clone, Default)]
pub struct DefineButton {
    pub button_records: Vec<ButtonRecord>,
    pub sounds: Option<TagRef>,
    pub cxform: Option<TagRef>,
    pub scaling_grid: Option<TagRef>,
    pub track_as_menu: bool,
    pub cond_actions: Vec<ButtonCondAction>,
}

#[derive(Clone, Default)]
pub struct DefineButtonCxform {
    pub button: Option<WeakTag>,
    pub color_transform: CXForm,
}

#[derive(Clone, Default)]
pub struct DefineButtonSound {
    pub sound0: Option<TagRef>,
    pub info0: Option<SoundInfo>,
    pub sound1: Option<TagRef>,
    pub info1: Option<SoundInfo>,
    pub sound2: Option<TagRef>,
    pub info2: Option<SoundInfo>,
    pub sound3: Option<TagRef>,
    pub info3: Option<SoundInfo>,
    pub button: Option<WeakTag>,
}

#[derive(Clone, Default)]
pub struct DefineEditText {
    pub bounds: Rect,
    pub has_text: bool,
    pub word_wrap: bool,
    pub multiline: bool,
    pub password: bool,
    pub read_only: bool,
    pub has_text_color: bool,
    pub has_max_length: bool,
    pub has_font: bool,
    pub has_font_class: bool,
    pub auto_size: bool,
    pub has_layout: bool,
    pub no_select: bool,
    pub border: bool,
    pub was_static: bool,
    pub html: bool,
    pub use_outlines: bool,
    pub font: Option<TagRef>,
    pub font_class: Option<String>,
    pub height: i32,
    pub color: i32,
    pub max_length: i32,
    pub align: i32,
    pub left_margin: i32,
    pub right_margin: i32,
    pub ident: i32,
    pub leading: i32,
    pub var_name: Option<String>,
    pub initial_text: Option<String>,
    pub csm_text_settings: Option<TagRef>,
}

#[derive(Clone, Default)]
pub struct DefineText {
    pub bounds: Rect,
    pub matrix: Matrix,
    pub records: Vec<TextRecord>,
    pub csm_text_settings: Option<TagRef>,
}

#[derive(Clone, Default)]
pub struct DefineFont1 {
    pub glyph_shape_table: Vec<Shape>,
    pub font_info: Option<TagRef>,
    pub license: Option<TagRef>,
}

/// `DefineFont2`, and `DefineFont3` (code 75) with its `zones`.
#[derive(Clone, Default)]
pub struct DefineFont2 {
    pub small_text: bool,
    pub has_layout: bool,
    pub shift_jis: bool,
    pub ansi: bool,
    pub wide_offsets: bool,
    pub wide_codes: bool,
    pub italic: bool,
    pub bold: bool,
    pub lang_code: i32,
    pub font_name: Option<String>,
    pub code_table: Vec<u16>,
    pub ascent: i32,
    pub descent: i32,
    pub leading: i32,
    pub glyph_shape_table: Vec<Shape>,
    pub advance_table: Vec<i16>,
    pub bounds_table: Vec<Rect>,
    pub kerning_count: i32,
    pub kerning_table: Vec<KerningRecord>,
    pub zones: Option<TagRef>,
    pub license: Option<TagRef>,
}

#[derive(Clone, Default)]
pub struct DefineFont4 {
    pub has_font_data: bool,
    pub small_text: bool,
    pub italic: bool,
    pub bold: bool,
    pub lang_code: i32,
    pub font_name: Option<String>,
    pub data: Vec<u8>,
    pub license: Option<TagRef>,
}

#[derive(Clone, Default)]
pub struct DefineFontInfo {
    pub font: Option<WeakTag>,
    pub name: Option<String>,
    pub shift_jis: bool,
    pub ansi: bool,
    pub italic: bool,
    pub bold: bool,
    pub wide_codes: bool,
    pub code_table: Vec<u16>,
    pub lang_code: i32,
}

#[derive(Clone, Default)]
pub struct DefineFontAlignZones {
    pub font: Option<WeakTag>,
    pub csm_table_hint: i32,
    pub zone_table: Vec<Rc<ZoneRecord>>,
}

#[derive(Clone, Default)]
pub struct CsmTextSettings {
    pub text_reference: Option<WeakTag>,
    pub style_flags_use_saffron: i32,
    pub grid_fit_type: i32,
    pub thickness: i64,
    pub sharpness: i64,
}

#[derive(Clone, Default)]
pub struct DefineFontName {
    pub font: Option<WeakTag>,
    pub font_name: Option<String>,
    pub copyright: Option<String>,
}

#[derive(Clone, Default)]
pub struct DefineMorphShape {
    pub start_bounds: Rect,
    pub end_bounds: Rect,
    pub start_edge_bounds: Rect,
    pub end_edge_bounds: Rect,
    pub reserved: i32,
    pub uses_non_scaling_strokes: bool,
    pub uses_scaling_strokes: bool,
    pub fill_styles: Vec<MorphFillStyle>,
    pub line_styles: Vec<MorphLineStyle>,
    pub start_edges: Option<Shape>,
    pub end_edges: Option<Shape>,
}

#[derive(Clone, Default)]
pub struct DefineVideoStream {
    pub num_frames: i32,
    pub width: i32,
    pub height: i32,
    pub deblocking: i32,
    pub smoothing: bool,
    pub codec_id: i32,
}

#[derive(Clone, Default)]
pub struct SoundStreamHead {
    pub playback_rate: i32,
    pub playback_size: i32,
    pub playback_type: i32,
    pub compression: i32,
    pub stream_rate: i32,
    pub stream_size: i32,
    pub stream_type: i32,
    pub stream_sample_count: i32,
    pub latency_seek: i32,
}

#[derive(Clone, Default)]
pub struct ImportAssets {
    pub url: Option<String>,
    pub import_records: Vec<TagRef>,
    pub download_now: bool,
    pub sha1: Option<Vec<u8>>,
}

/// The tag classes; `Tag.code` and `DefineTag.name` live on [`Tag`].
#[derive(Clone)]
pub enum Body {
    DefineBits(DefineBits),
    DefineSound(DefineSound),
    DefineBinaryData { reserved: i32, data: Vec<u8> },
    DefineSprite(DefineSprite),
    DefineShape(DefineShape),
    DefineScalingGrid(DefineScalingGrid),
    PlaceObject(PlaceObject),
    DefineButton(DefineButton),
    DefineButtonCxform(DefineButtonCxform),
    DefineButtonSound(DefineButtonSound),
    DefineEditText(DefineEditText),
    DefineText(DefineText),
    DefineFont1(DefineFont1),
    DefineFont2(DefineFont2),
    DefineFont4(DefineFont4),
    DefineFontInfo(DefineFontInfo),
    DefineFontAlignZones(DefineFontAlignZones),
    CsmTextSettings(CsmTextSettings),
    DefineFontName(DefineFontName),
    DefineMorphShape(DefineMorphShape),
    DefineVideoStream(DefineVideoStream),
    VideoFrame { stream: Option<TagRef>, frame_num: i32, video_data: Vec<u8> },
    DoAction { action_list: ActionList },
    DoInitAction { sprite: Option<WeakTag>, action_list: ActionList },
    RemoveObject { depth: i32, ref_: Option<TagRef> },
    StartSound { sound: Option<TagRef>, sound_info: SoundInfo },
    SoundStreamHead(SoundStreamHead),
    FrameLabel { label: String, anchor: bool },
    ShowFrame,
    ExportAssets { exports: Vec<TagRef> },
    ImportAssets(ImportAssets),
    ImportRecord,
    SetBackgroundColor { color: i32 },
    SetTabIndex { depth: i32, index: i32 },
    /// `GenericTag`: `equals` compares the arrays by identity.
    Generic { data: Rc<Vec<u8>> },
}

pub struct Tag {
    pub code: i32,
    pub name: Option<String>,
    pub body: Body,
}

impl Tag {
    pub fn new(code: i32, body: Body) -> Tag {
        Tag { code, name: None, body }
    }

    pub fn new_ref(code: i32, body: Body) -> TagRef {
        Rc::new(RefCell::new(Tag::new(code, body)))
    }

    /// `instanceof DefineTag`.
    pub fn is_define_tag(&self) -> bool {
        matches!(
            self.body,
            Body::DefineBits(_)
                | Body::DefineSound(_)
                | Body::DefineBinaryData { .. }
                | Body::DefineSprite(_)
                | Body::DefineShape(_)
                | Body::DefineButton(_)
                | Body::DefineEditText(_)
                | Body::DefineText(_)
                | Body::DefineFont1(_)
                | Body::DefineFont2(_)
                | Body::DefineFont4(_)
                | Body::DefineFontAlignZones(_)
                | Body::CsmTextSettings(_)
                | Body::DefineFontName(_)
                | Body::DefineMorphShape(_)
                | Body::DefineVideoStream(_)
                | Body::ImportRecord
        )
    }

    /// `instanceof DefineFont`.
    pub fn is_define_font(&self) -> bool {
        matches!(self.body, Body::DefineFont1(_) | Body::DefineFont2(_) | Body::DefineFont4(_))
    }

    pub fn sprite(&self) -> Option<&DefineSprite> {
        match &self.body {
            Body::DefineSprite(s) => Some(s),
            _ => None,
        }
    }

    pub fn sprite_mut(&mut self) -> Option<&mut DefineSprite> {
        match &mut self.body {
            Body::DefineSprite(s) => Some(s),
            _ => None,
        }
    }

    /// `DefineFont.getFontName()`.
    pub fn get_font_name(&self) -> Option<String> {
        match &self.body {
            Body::DefineFont1(f) => f.font_info.as_ref().and_then(|i| match &i.borrow().body {
                Body::DefineFontInfo(info) => info.name.clone(),
                _ => None,
            }),
            Body::DefineFont2(f) => f.font_name.clone(),
            Body::DefineFont4(f) => f.font_name.clone(),
            _ => None,
        }
    }

    pub fn is_bold(&self) -> bool {
        match &self.body {
            Body::DefineFont1(f) => f.font_info.as_ref().is_some_and(|i| matches!(&i.borrow().body, Body::DefineFontInfo(info) if info.bold)),
            Body::DefineFont2(f) => f.bold,
            Body::DefineFont4(f) => f.bold,
            _ => false,
        }
    }

    pub fn is_italic(&self) -> bool {
        match &self.body {
            Body::DefineFont1(f) => f.font_info.as_ref().is_some_and(|i| matches!(&i.borrow().body, Body::DefineFontInfo(info) if info.italic)),
            Body::DefineFont2(f) => f.italic,
            Body::DefineFont4(f) => f.italic,
            _ => false,
        }
    }

    pub fn license(&self) -> Option<TagRef> {
        match &self.body {
            Body::DefineFont1(f) => f.license.clone(),
            Body::DefineFont2(f) => f.license.clone(),
            Body::DefineFont4(f) => f.license.clone(),
            _ => None,
        }
    }

    /// `getReferences()`: the tags this one refers to by id.
    pub fn get_references(&self) -> Vec<TagRef> {
        let mut refs = Vec::new();
        let up = |w: &Option<WeakTag>, refs: &mut Vec<TagRef>| {
            if let Some(t) = w.as_ref().and_then(|w| w.upgrade()) {
                refs.push(t);
            }
        };
        match &self.body {
            Body::DefineBits(b) => refs.extend(b.jpeg_tables.clone()),
            Body::DefineSprite(s) => {
                for t in &s.tag_list {
                    refs.extend(t.borrow().get_references());
                }
            }
            Body::DefineShape(s) => s.shape_with_style.get_reference_list(&mut refs),
            Body::DefineScalingGrid(g) => up(&g.scaling_target, &mut refs),
            Body::PlaceObject(p) => {
                if p.has_char_id() {
                    refs.extend(p.ref_.clone());
                }
            }
            Body::DefineButton(b) => {
                for r in &b.button_records {
                    refs.extend(r.character_ref.clone());
                }
            }
            Body::DefineButtonCxform(c) => up(&c.button, &mut refs),
            Body::DefineButtonSound(s) => {
                up(&s.button, &mut refs);
                for x in [&s.sound0, &s.sound1, &s.sound2, &s.sound3] {
                    refs.extend(x.clone());
                }
            }
            Body::DefineEditText(e) => refs.extend(e.font.clone()),
            Body::DefineText(t) => {
                for r in &t.records {
                    r.get_reference_list(&mut refs);
                }
            }
            Body::DefineFont1(f) => {
                for g in &f.glyph_shape_table {
                    g.get_reference_list(&mut refs);
                }
            }
            Body::DefineFont2(f) => {
                for g in &f.glyph_shape_table {
                    g.get_reference_list(&mut refs);
                }
            }
            Body::DefineFontInfo(i) => up(&i.font, &mut refs),
            Body::DefineMorphShape(m) => {
                if let Some(s) = &m.start_edges {
                    s.get_reference_list(&mut refs);
                }
                if let Some(s) = &m.end_edges {
                    s.get_reference_list(&mut refs);
                }
                for style in &m.fill_styles {
                    if style.has_bitmap_id() {
                        refs.extend(style.bitmap.clone());
                    }
                }
            }
            Body::VideoFrame { stream, .. } => refs.extend(stream.clone()),
            Body::DoInitAction { sprite, .. } => up(sprite, &mut refs),
            Body::RemoveObject { ref_, .. } => refs.extend(ref_.clone()),
            Body::StartSound { sound, .. } => refs.extend(sound.clone()),
            Body::ExportAssets { exports } => refs.extend(exports.iter().cloned()),
            _ => {}
        }
        refs
    }

    /// `equals(Object)`.
    pub fn jeq(&self, o: &Tag) -> bool {
        if o.code != self.code {
            return false;
        }
        let names = o.name == self.name;
        match (&self.body, &o.body) {
            (Body::DefineBits(a), Body::DefineBits(b)) => {
                let base = names && b.data == a.data && b.width == a.width && b.height == a.height && opt_tag_eq(&b.jpeg_tables, &a.jpeg_tables);
                match a.kind {
                    BitsKind::Bits => base,
                    BitsKind::Jpeg3 => base && b.kind == BitsKind::Jpeg3 && b.alpha_data_offset == a.alpha_data_offset && b.alpha_data == a.alpha_data,
                    BitsKind::Lossless => base && b.kind == BitsKind::Lossless && b.format == a.format && b.color_data == a.color_data,
                }
            }
            (Body::DefineSound(a), Body::DefineSound(b)) => {
                names && b.format == a.format && b.rate == a.rate && b.size == a.size && b.type_ == a.type_ && b.sample_count == a.sample_count && b.data == a.data
            }
            (Body::DefineBinaryData { data: a, .. }, Body::DefineBinaryData { data: b, .. }) => names && a == b,
            (Body::DefineSprite(a), Body::DefineSprite(b)) => {
                names && b.framecount == a.framecount && tag_list_eq(&b.tag_list, &a.tag_list) && opt_tag_eq(&b.scaling_grid, &a.scaling_grid) && opt_tag_eq(&b.init_action, &a.init_action)
            }
            (Body::DefineShape(a), Body::DefineShape(b)) => {
                names
                    && b.bounds == a.bounds
                    && b.shape_with_style.jeq(&a.shape_with_style)
                    && b.edge_bounds == a.edge_bounds
                    && b.uses_fill_winding_rule == a.uses_fill_winding_rule
                    && b.uses_non_scaling_strokes == a.uses_non_scaling_strokes
                    && b.uses_scaling_strokes == a.uses_scaling_strokes
            }
            (Body::DefineScalingGrid(a), Body::DefineScalingGrid(b)) => weak_ptr_eq(&b.scaling_target, &a.scaling_target) && b.rect == a.rect,
            (Body::PlaceObject(a), Body::PlaceObject(b)) => {
                b.flags == a.flags
                    && b.flags2 == a.flags2
                    && b.ratio == a.ratio
                    && b.name == a.name
                    && b.clip_depth == a.clip_depth
                    && b.clip_actions.jeq(&a.clip_actions)
                    && b.depth == a.depth
                    && b.matrix.jeq(&a.matrix)
                    && b.color_transform.jeq(&a.color_transform)
                    && opt_tag_eq(&b.ref_, &a.ref_)
                    && b.blend_mode == a.blend_mode
                    && b.class_name == a.class_name
            }
            (Body::DefineButton(a), Body::DefineButton(b)) => {
                names
                    && list_eq(&b.button_records, &a.button_records)
                    && opt_tag_eq(&b.sounds, &a.sounds)
                    && opt_tag_eq(&b.cxform, &a.cxform)
                    && b.track_as_menu == a.track_as_menu
                    && list_eq(&b.cond_actions, &a.cond_actions)
            }
            (Body::DefineButtonCxform(a), Body::DefineButtonCxform(b)) => opt_weak_eq(&b.button, &a.button) && b.color_transform.jeq(&a.color_transform),
            (Body::DefineButtonSound(a), Body::DefineButtonSound(b)) => {
                opt_tag_eq(&b.sound0, &a.sound0)
                    && b.info0.jeq(&a.info0)
                    && opt_tag_eq(&b.sound1, &a.sound1)
                    && b.info1.jeq(&a.info1)
                    && opt_tag_eq(&b.sound2, &a.sound2)
                    && b.info2.jeq(&a.info2)
                    && opt_tag_eq(&b.sound3, &a.sound3)
                    && b.info3.jeq(&a.info3)
            }
            (Body::DefineEditText(a), Body::DefineEditText(b)) => {
                names
                    && b.bounds == a.bounds
                    && b.has_text == a.has_text
                    && b.word_wrap == a.word_wrap
                    && b.multiline == a.multiline
                    && b.password == a.password
                    && b.read_only == a.read_only
                    && b.has_text_color == a.has_text_color
                    && b.has_max_length == a.has_max_length
                    && b.has_font == a.has_font
                    && b.has_font_class == a.has_font_class
                    && b.auto_size == a.auto_size
                    && b.has_layout == a.has_layout
                    && b.no_select == a.no_select
                    && b.border == a.border
                    && b.was_static == a.was_static
                    && b.html == a.html
                    && b.use_outlines == a.use_outlines
                    && opt_tag_eq(&b.font, &a.font)
                    && b.height == a.height
                    && b.color == a.color
                    && b.max_length == a.max_length
                    && b.align == a.align
                    && b.left_margin == a.left_margin
                    && b.right_margin == a.right_margin
                    && b.ident == a.ident
                    && b.leading == a.leading
                    && b.font_class == a.font_class
                    && b.var_name == a.var_name
                    && b.initial_text == a.initial_text
            }
            (Body::DefineText(a), Body::DefineText(b)) => names && b.bounds == a.bounds && b.matrix == a.matrix && list_eq(&b.records, &a.records),
            (Body::DefineFont1(a), Body::DefineFont1(b)) => names && opt_tag_eq(&b.font_info, &a.font_info) && list_eq(&b.glyph_shape_table, &a.glyph_shape_table),
            (Body::DefineFont2(a), Body::DefineFont2(b)) => {
                names
                    && b.has_layout == a.has_layout
                    && b.shift_jis == a.shift_jis
                    && b.ansi == a.ansi
                    && b.italic == a.italic
                    && b.bold == a.bold
                    && b.lang_code == a.lang_code
                    && b.ascent == a.ascent
                    && b.descent == a.descent
                    && b.leading == a.leading
                    && b.kerning_count == a.kerning_count
                    && b.font_name == a.font_name
                    && list_eq(&b.glyph_shape_table, &a.glyph_shape_table)
                    && b.code_table == a.code_table
                    && b.advance_table == a.advance_table
                    && b.bounds_table == a.bounds_table
                    && list_eq(&b.kerning_table, &a.kerning_table)
            }
            (Body::DefineFont4(a), Body::DefineFont4(b)) => {
                names
                    && b.has_font_data == a.has_font_data
                    && b.italic == a.italic
                    && b.bold == a.bold
                    && b.lang_code == a.lang_code
                    && b.small_text == a.small_text
                    && b.font_name == a.font_name
                    && b.data == a.data
            }
            (Body::DefineFontInfo(a), Body::DefineFontInfo(b)) => {
                b.name == a.name && b.shift_jis == a.shift_jis && b.ansi == a.ansi && b.italic == a.italic && b.bold == a.bold && b.code_table == a.code_table && b.lang_code == a.lang_code
            }
            (Body::DefineFontAlignZones(a), Body::DefineFontAlignZones(b)) => {
                names && opt_weak_eq(&a.font, &b.font) && a.csm_table_hint == b.csm_table_hint && list_eq(&a.zone_table, &b.zone_table)
            }
            (Body::CsmTextSettings(a), Body::CsmTextSettings(b)) => {
                names
                    && opt_weak_eq(&a.text_reference, &b.text_reference)
                    && a.style_flags_use_saffron == b.style_flags_use_saffron
                    && a.grid_fit_type == b.grid_fit_type
                    && a.thickness == b.thickness
                    && a.sharpness == b.sharpness
            }
            (Body::DefineFontName(a), Body::DefineFontName(b)) => names && opt_weak_eq(&a.font, &b.font) && a.font_name == b.font_name && a.copyright == b.copyright,
            (Body::DefineMorphShape(a), Body::DefineMorphShape(b)) => {
                let eq = names
                    && b.start_bounds == a.start_bounds
                    && b.end_bounds == a.end_bounds
                    && list_eq(&b.fill_styles, &a.fill_styles)
                    && list_eq(&b.line_styles, &a.line_styles)
                    && b.start_edges.jeq(&a.start_edges)
                    && b.end_edges.jeq(&a.end_edges);
                if eq && self.code == codes::DEFINE_MORPH_SHAPE2 {
                    b.start_edge_bounds == a.start_edge_bounds
                        && b.end_edge_bounds == a.end_edge_bounds
                        && b.uses_non_scaling_strokes == a.uses_non_scaling_strokes
                        && b.uses_scaling_strokes == a.uses_scaling_strokes
                } else {
                    eq
                }
            }
            (Body::DefineVideoStream(a), Body::DefineVideoStream(b)) => {
                names && b.num_frames == a.num_frames && b.width == a.width && b.height == a.height && b.deblocking == a.deblocking && b.smoothing == a.smoothing && b.codec_id == a.codec_id
            }
            (Body::VideoFrame { stream: s1, frame_num: f1, video_data: d1 }, Body::VideoFrame { stream: s2, frame_num: f2, video_data: d2 }) => {
                opt_tag_eq(s2, s1) && f1 == f2 && d1 == d2
            }
            (Body::DoAction { action_list: a }, Body::DoAction { action_list: b }) => a == b,
            (Body::DoInitAction { action_list: a, .. }, Body::DoInitAction { action_list: b, .. }) => a == b,
            (Body::RemoveObject { depth: d1, ref_: r1 }, Body::RemoveObject { depth: d2, ref_: r2 }) => d1 == d2 && opt_tag_eq(r2, r1),
            (Body::StartSound { sound: s1, sound_info: i1 }, Body::StartSound { sound: s2, sound_info: i2 }) => opt_tag_eq(s2, s1) && i1 == i2,
            (Body::SoundStreamHead(a), Body::SoundStreamHead(b)) => {
                a.playback_rate == b.playback_rate
                    && a.playback_size == b.playback_size
                    && a.playback_type == b.playback_type
                    && a.compression == b.compression
                    && a.stream_rate == b.stream_rate
                    && a.stream_size == b.stream_size
                    && a.stream_type == b.stream_type
                    && a.stream_sample_count == b.stream_sample_count
                    && a.latency_seek == b.latency_seek
            }
            (Body::FrameLabel { label: l1, anchor: a1 }, Body::FrameLabel { label: l2, anchor: a2 }) => l1 == l2 && a1 == a2,
            (Body::ShowFrame, Body::ShowFrame) => true,
            (Body::ExportAssets { exports: a }, Body::ExportAssets { exports: b }) => tag_list_eq(b, a),
            (Body::ImportAssets(a), Body::ImportAssets(b)) => {
                b.url == a.url && b.download_now == a.download_now && b.sha1 == a.sha1 && tag_list_eq(&b.import_records, &a.import_records)
            }
            (Body::ImportRecord, Body::ImportRecord) => names,
            (Body::SetBackgroundColor { color: a }, Body::SetBackgroundColor { color: b }) => a == b,
            (Body::SetTabIndex { depth: d1, index: i1 }, Body::SetTabIndex { depth: d2, index: i2 }) => d1 == d2 && i1 == i2,
            (Body::Generic { data: a }, Body::Generic { data: b }) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// `Tag.equals(o1, o2)`.
pub fn tag_eq(a: &TagRef, b: &TagRef) -> bool {
    Rc::ptr_eq(a, b) || a.borrow().jeq(&b.borrow())
}

pub fn opt_tag_eq(a: &Option<TagRef>, b: &Option<TagRef>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => tag_eq(a, b),
        _ => false,
    }
}

fn tag_list_eq(a: &[TagRef], b: &[TagRef]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| tag_eq(x, y))
}

fn weak_ptr_eq(a: &Option<WeakTag>, b: &Option<WeakTag>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Weak::ptr_eq(a, b),
        _ => false,
    }
}

fn opt_weak_eq(a: &Option<WeakTag>, b: &Option<WeakTag>) -> bool {
    match (a.as_ref().and_then(|w| w.upgrade()), b.as_ref().and_then(|w| w.upgrade())) {
        (None, None) => true,
        (Some(a), Some(b)) => tag_eq(&a, &b),
        _ => false,
    }
}

/// `new DefineSprite(name)`.
pub fn new_sprite(name: Option<String>) -> TagRef {
    let t = Tag::new_ref(codes::DEFINE_SPRITE, Body::DefineSprite(DefineSprite::default()));
    t.borrow_mut().name = name;
    t
}

/// `new DefineSprite(source)`: the semi-shallow copy `MovieTranscoder` takes before adding a scaling grid.
pub fn copy_sprite(source: &TagRef) -> TagRef {
    let src = source.borrow();
    let s = src.sprite().expect("ClassCastException");
    let copy = new_sprite(src.name.clone());
    {
        let mut c = copy.borrow_mut();
        let cs = c.sprite_mut().unwrap();
        cs.tag_list.extend(s.tag_list.iter().cloned());
        cs.init_action = s.init_action.clone();
        cs.framecount = s.framecount;
    }
    if let Some(grid) = &s.scaling_grid {
        let rect = match &grid.borrow().body {
            Body::DefineScalingGrid(g) => g.rect,
            _ => Rect::default(),
        };
        let g = Tag::new_ref(codes::DEFINE_SCALING_GRID, Body::DefineScalingGrid(DefineScalingGrid { scaling_target: Some(Rc::downgrade(&copy)), rect }));
        copy.borrow_mut().sprite_mut().unwrap().scaling_grid = Some(g);
    }
    copy
}

/// `new PlaceObject(ref, depth)`.
pub fn new_place_object(ref_: TagRef, depth: i32) -> PlaceObject {
    let mut p = PlaceObject { depth, ..Default::default() };
    p.set_ref(ref_);
    p
}
