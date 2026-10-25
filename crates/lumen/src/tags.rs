//! Tag records: raw bodies kept byte-exact, with nested sprite timelines parsed out.

use crate::error::Result;
use crate::io::{Reader, Writer};

pub mod code {
    pub const END: u16 = 0;
    pub const SHOW_FRAME: u16 = 1;
    pub const DEFINE_SHAPE: u16 = 2;
    pub const FREE_CHARACTER: u16 = 3;
    pub const PLACE_OBJECT: u16 = 4;
    pub const REMOVE_OBJECT: u16 = 5;
    pub const DEFINE_BITS: u16 = 6;
    pub const DEFINE_BUTTON: u16 = 7;
    pub const JPEG_TABLES: u16 = 8;
    pub const SET_BACKGROUND_COLOR: u16 = 9;
    pub const DEFINE_FONT: u16 = 10;
    pub const DEFINE_TEXT: u16 = 11;
    pub const DO_ACTION: u16 = 12;
    pub const DEFINE_FONT_INFO: u16 = 13;
    pub const DEFINE_SOUND: u16 = 14;
    pub const START_SOUND: u16 = 15;
    pub const DEFINE_BUTTON_SOUND: u16 = 17;
    pub const SOUND_STREAM_HEAD: u16 = 18;
    pub const SOUND_STREAM_BLOCK: u16 = 19;
    pub const DEFINE_BITS_LOSSLESS: u16 = 20;
    pub const DEFINE_BITS_JPEG2: u16 = 21;
    pub const DEFINE_SHAPE2: u16 = 22;
    pub const DEFINE_BUTTON_CXFORM: u16 = 23;
    pub const PROTECT: u16 = 24;
    pub const PLACE_OBJECT2: u16 = 26;
    pub const REMOVE_OBJECT2: u16 = 28;
    pub const DEFINE_SHAPE3: u16 = 32;
    pub const DEFINE_TEXT2: u16 = 33;
    pub const DEFINE_BUTTON2: u16 = 34;
    pub const DEFINE_BITS_JPEG3: u16 = 35;
    pub const DEFINE_BITS_LOSSLESS2: u16 = 36;
    pub const DEFINE_EDIT_TEXT: u16 = 37;
    pub const DEFINE_VIDEO: u16 = 38;
    pub const DEFINE_SPRITE: u16 = 39;
    pub const NAME_CHARACTER: u16 = 40;
    pub const PRODUCT_INFO: u16 = 41;
    pub const FRAME_LABEL: u16 = 43;
    pub const SOUND_STREAM_HEAD2: u16 = 45;
    pub const DEFINE_MORPH_SHAPE: u16 = 46;
    pub const DEFINE_FONT2: u16 = 48;
    pub const EXPORT_ASSETS: u16 = 56;
    pub const IMPORT_ASSETS: u16 = 57;
    pub const ENABLE_DEBUGGER: u16 = 58;
    pub const DO_INIT_ACTION: u16 = 59;
    pub const DEFINE_VIDEO_STREAM: u16 = 60;
    pub const VIDEO_FRAME: u16 = 61;
    pub const DEFINE_FONT_INFO2: u16 = 62;
    pub const DEBUG_ID: u16 = 63;
    pub const ENABLE_DEBUGGER2: u16 = 64;
    pub const SCRIPT_LIMITS: u16 = 65;
    pub const SET_TAB_INDEX: u16 = 66;
    pub const FILE_ATTRIBUTES: u16 = 69;
    pub const PLACE_OBJECT3: u16 = 70;
    pub const IMPORT_ASSETS2: u16 = 71;
    pub const DO_ABC: u16 = 72;
    pub const DEFINE_FONT_ALIGN_ZONES: u16 = 73;
    pub const CSM_SETTINGS: u16 = 74;
    pub const DEFINE_FONT3: u16 = 75;
    pub const SYMBOL_CLASS: u16 = 76;
    pub const METADATA: u16 = 77;
    pub const DEFINE_SCALING_GRID: u16 = 78;
    pub const DO_ABC2: u16 = 82;
    pub const DEFINE_SHAPE4: u16 = 83;
    pub const DEFINE_MORPH_SHAPE2: u16 = 84;
    pub const DEFINE_SCENE_AND_FRAME_LABEL_DATA: u16 = 86;
    pub const DEFINE_BINARY_DATA: u16 = 87;
    pub const DEFINE_FONT_NAME: u16 = 88;
    pub const START_SOUND2: u16 = 89;
    pub const DEFINE_BITS_JPEG4: u16 = 90;
    pub const DEFINE_FONT4: u16 = 91;
    pub const ENABLE_TELEMETRY: u16 = 93;
    pub const PLACE_OBJECT4: u16 = 94;
    pub const IGGY_EXTERNAL_IMAGE: u16 = 513;
    pub const GFX_EXPORTER_INFO: u16 = 1000;
    pub const GFX_DEFINE_EXTERNAL_IMAGE: u16 = 1001;
    pub const GFX_FONT_TEXTURE_INFO: u16 = 1002;
    pub const GFX_DEFINE_EXTERNAL_GRADIENT: u16 = 1003;
    pub const GFX_DEFINE_GRADIENT_MAP: u16 = 1004;
    pub const GFX_DEFINE_COMPACTED_FONT: u16 = 1005;
    pub const GFX_DEFINE_EXTERNAL_SOUND: u16 = 1006;
    pub const GFX_DEFINE_EXTERNAL_STREAM_SOUND: u16 = 1007;
    pub const GFX_DEFINE_SUB_IMAGE: u16 = 1008;
    pub const GFX_DEFINE_EXTERNAL_IMAGE2: u16 = 1009;
}

pub fn tag_name(code: u16) -> Option<&'static str> {
    Some(match code {
        0 => "End",
        1 => "ShowFrame",
        2 => "DefineShape",
        3 => "FreeCharacter",
        4 => "PlaceObject",
        5 => "RemoveObject",
        6 => "DefineBits",
        7 => "DefineButton",
        8 => "JPEGTables",
        9 => "SetBackgroundColor",
        10 => "DefineFont",
        11 => "DefineText",
        12 => "DoAction",
        13 => "DefineFontInfo",
        14 => "DefineSound",
        15 => "StartSound",
        17 => "DefineButtonSound",
        18 => "SoundStreamHead",
        19 => "SoundStreamBlock",
        20 => "DefineBitsLossless",
        21 => "DefineBitsJPEG2",
        22 => "DefineShape2",
        23 => "DefineButtonCxform",
        24 => "Protect",
        25 => "PathsArePostScript",
        26 => "PlaceObject2",
        28 => "RemoveObject2",
        29 => "SyncFrame",
        31 => "FreeAll",
        32 => "DefineShape3",
        33 => "DefineText2",
        34 => "DefineButton2",
        35 => "DefineBitsJPEG3",
        36 => "DefineBitsLossless2",
        37 => "DefineEditText",
        38 => "DefineVideo",
        39 => "DefineSprite",
        40 => "NameCharacter",
        41 => "ProductInfo",
        42 => "DefineTextFormat",
        43 => "FrameLabel",
        45 => "SoundStreamHead2",
        46 => "DefineMorphShape",
        48 => "DefineFont2",
        49 => "GenCommand",
        51 => "CharacterSet",
        52 => "FontRef",
        56 => "ExportAssets",
        57 => "ImportAssets",
        58 => "EnableDebugger",
        59 => "DoInitAction",
        60 => "DefineVideoStream",
        61 => "VideoFrame",
        62 => "DefineFontInfo2",
        63 => "DebugID",
        64 => "EnableDebugger2",
        65 => "ScriptLimits",
        66 => "SetTabIndex",
        69 => "FileAttributes",
        70 => "PlaceObject3",
        71 => "ImportAssets2",
        72 => "DoABC",
        73 => "DefineFontAlignZones",
        74 => "CSMSettings",
        75 => "DefineFont3",
        76 => "SymbolClass",
        77 => "Metadata",
        78 => "DefineScalingGrid",
        82 => "DoABC2",
        83 => "DefineShape4",
        84 => "DefineMorphShape2",
        85 => "PlaceImagePrivate",
        86 => "DefineSceneAndFrameLabelData",
        87 => "DefineBinaryData",
        88 => "DefineFontName",
        89 => "StartSound2",
        90 => "DefineBitsJPEG4",
        91 => "DefineFont4",
        93 => "EnableTelemetry",
        94 => "PlaceObject4",
        1000 => "ExporterInfo",
        1001 => "DefineExternalImage",
        1002 => "FontTextureInfo",
        1003 => "DefineExternalGradient",
        1004 => "DefineGradientMap",
        1005 => "DefineCompactedFont",
        1006 => "DefineExternalSound",
        1007 => "DefineExternalStreamSound",
        1008 => "DefineSubImage",
        513 => "IggyExternalImage",
        1009 => "DefineExternalImage2",
        _ => return None,
    })
}

pub fn display_name(code: u16) -> String {
    tag_name(code).map(str::to_owned).unwrap_or_else(|| format!("Unknown({code})"))
}

/// Tags whose first field is the character ID they define.
pub fn defines_character(code: u16) -> bool {
    use code::*;
    matches!(
        code,
        DEFINE_SHAPE
            | DEFINE_SHAPE2
            | DEFINE_SHAPE3
            | DEFINE_SHAPE4
            | DEFINE_BITS
            | DEFINE_BITS_JPEG2
            | DEFINE_BITS_JPEG3
            | DEFINE_BITS_JPEG4
            | DEFINE_BITS_LOSSLESS
            | DEFINE_BITS_LOSSLESS2
            | DEFINE_BUTTON
            | DEFINE_BUTTON2
            | DEFINE_FONT
            | DEFINE_FONT2
            | DEFINE_FONT3
            | DEFINE_FONT4
            | DEFINE_TEXT
            | DEFINE_TEXT2
            | DEFINE_EDIT_TEXT
            | DEFINE_SOUND
            | DEFINE_VIDEO_STREAM
            | DEFINE_SPRITE
            | DEFINE_MORPH_SHAPE
            | DEFINE_MORPH_SHAPE2
            | DEFINE_BINARY_DATA
            | IGGY_EXTERNAL_IMAGE
            | GFX_DEFINE_EXTERNAL_IMAGE
            | GFX_DEFINE_EXTERNAL_IMAGE2
            | GFX_DEFINE_SUB_IMAGE
            | GFX_DEFINE_EXTERNAL_SOUND
            | GFX_DEFINE_EXTERNAL_STREAM_SOUND
            | GFX_DEFINE_COMPACTED_FONT
    )
}

/// Tags whose first field references another character (for grouping and lookups).
pub fn references_character(code: u16) -> bool {
    use code::*;
    matches!(
        code,
        DEFINE_FONT_INFO
            | DEFINE_FONT_INFO2
            | DEFINE_FONT_ALIGN_ZONES
            | DEFINE_FONT_NAME
            | CSM_SETTINGS
            | DEFINE_SCALING_GRID
            | DEFINE_BUTTON_SOUND
            | DEFINE_BUTTON_CXFORM
            | VIDEO_FRAME
            | START_SOUND
            | DO_INIT_ACTION
            | NAME_CHARACTER
            | FREE_CHARACTER
    )
}

/// Image tags are always written with a long header, as Flash expects.
fn force_long(code: u16) -> bool {
    use code::*;
    matches!(
        code,
        DEFINE_BITS | DEFINE_BITS_JPEG2 | DEFINE_BITS_JPEG3 | DEFINE_BITS_JPEG4 | DEFINE_BITS_LOSSLESS | DEFINE_BITS_LOSSLESS2
    )
}

#[derive(Debug, Clone)]
pub struct Tag {
    pub code: u16,
    pub long_header: bool,
    /// Byte offset of the tag header within its parent (the SWF body or the sprite body).
    pub offset: usize,
    pub body: Vec<u8>,
    /// Parsed children of a DefineSprite.
    pub sprite: Option<Timeline>,
    pub error: Option<String>,
    /// Length from the header of a tag cut off by the end of the file.
    pub declared_len: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct Timeline {
    pub tags: Vec<Tag>,
    /// Bytes after the End tag (or after a truncated tag stream).
    pub trailer: Vec<u8>,
    pub has_end: bool,
}

impl Tag {
    pub fn new(code: u16, body: Vec<u8>) -> Self {
        Tag { code, long_header: force_long(code), offset: 0, body, sprite: None, error: None, declared_len: None }
    }

    pub fn name(&self) -> String {
        display_name(self.code)
    }

    pub fn character_id(&self) -> Option<u16> {
        if self.code == code::IGGY_EXTERNAL_IMAGE {
            return self.body.get(1..3).map(|b| u16::from_le_bytes([b[0], b[1]]));
        }
        if (defines_character(self.code) || references_character(self.code)) && self.body.len() >= 2 {
            Some(u16::from_le_bytes([self.body[0], self.body[1]]))
        } else {
            None
        }
    }

    pub fn header_len(&self) -> usize {
        if self.needs_long() { 6 } else { 2 }
    }

    fn needs_long(&self) -> bool {
        self.long_header || self.body.len() >= 0x3f
    }

    pub fn write(&self, w: &mut Writer) {
        let len = self.declared_len.map_or(self.body.len(), |l| l as usize);
        if self.needs_long() {
            w.u16((self.code << 6) | 0x3f);
            w.u32(len as u32);
        } else {
            w.u16((self.code << 6) | len as u16);
        }
        w.bytes(&self.body);
    }
}

impl Timeline {
    pub fn parse(r: &mut Reader, base: usize, depth: usize) -> Timeline {
        let mut tl = Timeline::default();
        while !r.is_empty() {
            let start = r.pos();
            let mut probe = r.clone();
            let Ok(head) = probe.u16() else { break };
            let code = head >> 6;
            let mut len = (head & 0x3f) as usize;
            let long_header = len == 0x3f;
            if long_header {
                match probe.u32() {
                    Ok(l) => len = l as usize,
                    Err(_) => break,
                }
            }
            let header_len = if long_header { 6 } else { 2 };
            if probe.remaining() < len {
                // Truncated final tag: keep what is there, flagged as an error.
                let body = probe.rest().to_vec();
                *r = probe;
                tl.tags.push(Tag {
                    code,
                    long_header,
                    offset: base + start,
                    body,
                    sprite: None,
                    error: Some(format!("tag truncated: declared {len} bytes")),
                    declared_len: Some(len as u32),
                });
                tl.trailer.clear();
                return tl;
            }
            let body = probe.bytes(len).unwrap().to_vec();
            *r = probe;
            let mut tag = Tag { code, long_header, offset: base + start, body, sprite: None, error: None, declared_len: None };
            if code == code::DEFINE_SPRITE && depth < 16 {
                let mut sr = Reader::new(&tag.body);
                if sr.skip(4).is_ok() {
                    tag.sprite = Some(Timeline::parse(&mut sr, base + start + header_len + 4, depth + 1));
                }
            }
            let is_end = code == code::END;
            tl.tags.push(tag);
            if is_end {
                tl.has_end = true;
                break;
            }
        }
        tl.trailer = r.rest().to_vec();
        tl
    }

    pub fn write(&self, w: &mut Writer) {
        for t in &self.tags {
            t.write(w);
        }
        w.bytes(&self.trailer);
    }

    pub fn frame_count(&self) -> usize {
        let shows = self.tags.iter().filter(|t| t.code == code::SHOW_FRAME).count();
        let tail = self
            .tags
            .iter()
            .rev()
            .take_while(|t| t.code != code::SHOW_FRAME)
            .any(|t| t.code != code::END);
        shows + tail as usize
    }
}

pub fn sprite_header(body: &[u8]) -> Result<(u16, u16)> {
    let mut r = Reader::new(body);
    Ok((r.u16()?, r.u16()?))
}
