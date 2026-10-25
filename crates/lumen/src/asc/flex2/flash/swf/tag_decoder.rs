//! Ports of `flash.swf.TagDecoder` and `flash.swf.SwfDecoder`: a SWF read into the tag object graph and its
//! `Dictionary`, as `MovieTranscoder`'s `TagLocator` and `SwcLibrary`'s `MovieDecoder` see it. Action records are
//! kept as their bytes (see `types::ActionList`).

use std::rc::Rc;

use super::dictionary::Dictionary;
use super::tags::*;
use super::types::*;

/// `flash.swf.Header`.
#[derive(Clone, Debug, Default)]
pub struct Header {
    pub compressed: bool,
    pub version: i32,
    pub length: i64,
    pub size: Rect,
    pub rate: i32,
    pub framecount: i32,
}

/// A decode failure: Java's `IOException`s, or (`runtime`) an unchecked exception such as the
/// `NullPointerException` `decodeDefineButton` always throws.
#[derive(Debug, Clone)]
pub struct DecodeError {
    pub message: String,
    pub runtime: bool,
}

impl DecodeError {
    fn io(m: impl Into<String>) -> DecodeError {
        DecodeError { message: m.into(), runtime: false }
    }
    fn runtime(m: impl Into<String>) -> DecodeError {
        DecodeError { message: m.into(), runtime: true }
    }
}

type R<T> = Result<T, DecodeError>;

/// `SwfDecoder` over bytes: byte reads leave the bit buffer alone, 16- and 32-bit reads sync it first.
pub struct SwfDecoder<'a> {
    buf: &'a [u8],
    pos: usize,
    bit_buf: i32,
    bit_pos: i32,
    pub swf_version: i32,
    /// Byte reads past the end, which Java answers with -1.
    past_end: u32,
}

impl<'a> SwfDecoder<'a> {
    pub fn new(buf: &'a [u8], swf_version: i32) -> SwfDecoder<'a> {
        SwfDecoder { buf, pos: 0, bit_buf: 0, bit_pos: 0, swf_version, past_end: 0 }
    }

    pub fn get_offset(&self) -> usize {
        self.pos
    }

    pub fn read_ui8(&mut self) -> i32 {
        match self.buf.get(self.pos) {
            Some(&b) => {
                self.pos += 1;
                b as i32
            }
            None => {
                self.past_end += 1;
                -1
            }
        }
    }

    pub fn sync_bits(&mut self) {
        self.bit_pos = 0;
    }

    pub fn read_ui16(&mut self) -> i32 {
        self.sync_bits();
        if self.buf.len() - self.pos.min(self.buf.len()) >= 2 {
            let i = self.buf[self.pos] as i32 | (self.buf[self.pos + 1] as i32) << 8;
            self.pos += 2;
            i
        } else {
            -1
        }
    }

    pub fn read_si16(&mut self) -> i32 {
        self.read_ui16() as i16 as i32
    }

    pub fn read_si32(&mut self) -> i32 {
        self.sync_bits();
        if self.buf.len() - self.pos.min(self.buf.len()) >= 4 {
            let i = i32::from_le_bytes(self.buf[self.pos..self.pos + 4].try_into().unwrap());
            self.pos += 4;
            i
        } else {
            -1
        }
    }

    pub fn read_ui32(&mut self) -> i64 {
        self.read_si32() as i64 & 0xffff_ffff
    }

    pub fn read64(&mut self) -> i64 {
        let lo = self.read_ui32();
        let hi = self.read_ui32();
        lo | (hi << 32)
    }

    pub fn read_float(&mut self) -> f32 {
        f32::from_bits(self.read_si32() as u32)
    }

    pub fn read_fixed8(&mut self) -> f32 {
        (self.read_ui16() as f64 / 256.0) as f32
    }

    pub fn read_bit(&mut self) -> bool {
        self.read_ubits(1) != 0
    }

    pub fn read_ubits(&mut self, num_bits: i32) -> i32 {
        if num_bits == 0 {
            return 0;
        }
        let mut bits_left = num_bits;
        let mut result: i32 = 0;
        if self.bit_pos == 0 {
            self.bit_buf = self.read_ui8();
            self.bit_pos = 8;
        }
        loop {
            let shift = bits_left - self.bit_pos;
            if shift > 0 {
                result |= self.bit_buf.wrapping_shl(shift as u32);
                bits_left -= self.bit_pos;
                self.bit_buf = self.read_ui8();
                self.bit_pos = 8;
            } else {
                result |= self.bit_buf >> (-shift);
                self.bit_pos -= bits_left;
                self.bit_buf &= 0xff >> (8 - self.bit_pos);
                return result;
            }
        }
    }

    pub fn read_sbits(&mut self, num_bits: i32) -> R<i32> {
        if num_bits > 32 {
            return Err(DecodeError::io("Number of bits > 32"));
        }
        let num = self.read_ubits(num_bits);
        let shift = 32 - num_bits;
        Ok(if shift >= 32 { 0 } else { num.wrapping_shl(shift as u32) >> shift })
    }

    pub fn read_fully(&mut self, n: usize) -> R<Vec<u8>> {
        if self.pos + n > self.buf.len() {
            return Err(DecodeError::io(format!("couldn't read {}", self.pos + n - self.buf.len().max(self.pos))));
        }
        let v = self.buf[self.pos..self.pos + n].to_vec();
        self.pos += n;
        Ok(v)
    }

    /// `readString()`: `readUTF`'s modified UTF-8 for SWF 6 and later (no 4-byte sequences), up to a NUL.
    pub fn read_string(&mut self) -> R<String> {
        if self.swf_version < 6 {
            let mut v = Vec::new();
            loop {
                let ch = self.read_ui8();
                if ch <= 0 {
                    break;
                }
                v.push(ch as u8);
            }
            return Ok(v.iter().map(|&c| c as char).collect());
        }
        let mut units: Vec<u16> = Vec::new();
        let bad = || DecodeError::io("UTFDataFormatException");
        loop {
            let c = self.read_ui8();
            if c <= 0 {
                break;
            }
            match c >> 4 {
                0..=7 => units.push(c as u16),
                12 | 13 => {
                    let c2 = self.read_ui8();
                    if c2 <= 0 || (c2 & 0xc0) != 0x80 {
                        return Err(bad());
                    }
                    units.push((((c & 0x1f) << 6) | (c2 & 0x3f)) as u16);
                }
                14 => {
                    let c2 = self.read_ui8();
                    let c3 = self.read_ui8();
                    if c2 <= 0 || c3 <= 0 || (c2 & 0xc0) != 0x80 || (c3 & 0xc0) != 0x80 {
                        return Err(bad());
                    }
                    units.push((((c & 0x0f) << 12) | ((c2 & 0x3f) << 6) | (c3 & 0x3f)) as u16);
                }
                _ => return Err(bad()),
            }
        }
        Ok(crate::asc::util::jstr::from_utf16(&units))
    }

    /// `readLengthString()`: a length byte, then that many bytes of which the last (the NUL) is dropped.
    pub fn read_length_string(&mut self) -> R<String> {
        let length = self.read_ui8().max(0) as usize;
        let b = self.read_fully(length)?;
        if length == 0 {
            return Err(DecodeError::runtime("String index out of range: -1"));
        }
        let b = &b[..length - 1];
        Ok(if self.swf_version >= 6 { String::from_utf8_lossy(b).into_owned() } else { b.iter().map(|&c| c as char).collect() })
    }
}

/// `TagDecoder.decodeHeader()` on an uncompressed or zlib-compressed SWF, with the body it reads tags from.
fn read_header(swf: &[u8]) -> R<(Header, Vec<u8>)> {
    if swf.len() < 8 {
        return Err(DecodeError::io("EOF"));
    }
    let mut h = Header { version: swf[3] as i8 as i32, length: (swf[4] as i64) | (swf[5] as i64) << 8 | (swf[6] as i64) << 16 | ((swf[7] as i8 as i64) << 24), ..Default::default() };
    let body = if &swf[..3] == b"CWS" {
        h.compressed = true;
        let mut out = Vec::new();
        let mut z = flate2::read::ZlibDecoder::new(&swf[8..]);
        let _ = std::io::Read::read_to_end(&mut z, &mut out);
        out
    } else if swf[0] == b'F' || swf[1] == b'W' || swf[2] == b'S' {
        swf[8..].to_vec()
    } else {
        return Err(DecodeError::io("Invalid signature found.  Not a SWF file"));
    };
    Ok((h, body))
}

/// `decodeHeader()`.
pub fn decode_header(swf: &[u8]) -> Result<Header, String> {
    let (mut h, body) = read_header(swf).map_err(|e| e.message)?;
    let mut r = SwfDecoder::new(&body, h.version);
    h.size = decode_rect(&mut r).map_err(|e| e.message)?;
    h.rate = r.read_ui8() << 8 | r.read_ui8();
    h.framecount = r.read_ui16();
    Ok(h)
}

/// A decoded SWF: what `TagLocator` (the dictionary) and `MovieDecoder` (each frame's symbol classes) collect.
pub struct Decoded {
    pub header: Header,
    pub dict: Dictionary,
    /// The frame's `symbolClass.class2tag` as `MovieDecoder` merges each `SymbolClass` into it, in map order.
    pub symbol_classes: Vec<(String, TagRef)>,
}

/// Parses a SWF the way `new TagDecoder(in).parse(handler)` does.
pub fn decode(swf: &[u8]) -> Result<Decoded, DecodeError> {
    let (header, body) = read_header(swf)?;
    let mut d = TagDecoder { header, dict: Dictionary::new(), jpeg_tables: None, frame_symbols: Default::default() };
    let mut r = SwfDecoder::new(&body, d.header.version);
    d.header.size = decode_rect(&mut r)?;
    d.header.rate = r.read_ui8() << 8 | r.read_ui8();
    d.header.framecount = r.read_ui16();
    let mut top = Vec::new();
    d.decode_tags(&mut r, &mut top, true)?;
    Ok(d.finish())
}

/// `parse` as `SwcLibrary` calls it, which keeps what was read before an `IOException`.
pub fn decode_lenient(swf: &[u8]) -> Decoded {
    let header = read_header(swf).map(|(h, _)| h).unwrap_or_default();
    let Ok((_, body)) = read_header(swf) else { return Decoded { header, dict: Dictionary::new(), symbol_classes: Vec::new() } };
    let mut d = TagDecoder { header, dict: Dictionary::new(), jpeg_tables: None, frame_symbols: Default::default() };
    let mut r = SwfDecoder::new(&body, d.header.version);
    if let Ok(size) = decode_rect(&mut r) {
        d.header.size = size;
        d.header.rate = r.read_ui8() << 8 | r.read_ui8();
        d.header.framecount = r.read_ui16();
        let mut top = Vec::new();
        let _ = d.decode_tags(&mut r, &mut top, true);
    }
    d.finish()
}

/// The dictionary `MovieTranscoder.getDictionary` keeps for a SWF.
pub fn decode_dictionary(swf: &[u8]) -> Result<Dictionary, String> {
    decode(swf).map(|d| d.dict).map_err(|e| e.message)
}

struct TagDecoder {
    header: Header,
    dict: Dictionary,
    jpeg_tables: Option<TagRef>,
    frame_symbols: crate::asc::util::jhash::JHashMap<String, TagRef>,
}

impl TagDecoder {
    fn finish(self) -> Decoded {
        let symbol_classes = self.frame_symbols.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        Decoded { header: self.header, dict: self.dict, symbol_classes }
    }
}

/// Whether `TagList` keeps a tag of this code (it has no handler for the rest).
fn tag_list_keeps(code: i32) -> bool {
    !matches!(code, codes::DEFINE_BINARY_DATA | codes::DO_ABC | codes::DO_ABC2 | codes::SYMBOL_CLASS | codes::METADATA | codes::DEFINE_FONT4 | codes::DEFINE_SCENE_AND_FRAME_LABEL_DATA)
}

fn get_tag(dict: &Dictionary, idref: i32) -> R<Option<TagRef>> {
    dict.get_tag(idref).map_err(DecodeError::runtime)
}

fn need(t: Option<TagRef>) -> R<TagRef> {
    t.ok_or_else(|| DecodeError::runtime("null"))
}

fn decode_rect(r: &mut SwfDecoder) -> R<Rect> {
    r.sync_bits();
    let n = r.read_ubits(5);
    Ok(Rect { x_min: r.read_sbits(n)?, x_max: r.read_sbits(n)?, y_min: r.read_sbits(n)?, y_max: r.read_sbits(n)? })
}

fn decode_matrix(r: &mut SwfDecoder) -> R<Matrix> {
    let mut m = Matrix::default();
    r.sync_bits();
    m.has_scale = r.read_bit();
    if m.has_scale {
        let n = r.read_ubits(5);
        m.scale_x = r.read_sbits(n)?;
        m.scale_y = r.read_sbits(n)?;
    }
    m.has_rotate = r.read_bit();
    if m.has_rotate {
        let n = r.read_ubits(5);
        m.rotate_skew0 = r.read_sbits(n)?;
        m.rotate_skew1 = r.read_sbits(n)?;
    }
    let n = r.read_ubits(5);
    m.translate_x = r.read_sbits(n)?;
    m.translate_y = r.read_sbits(n)?;
    Ok(m)
}

fn decode_rgba(r: &mut SwfDecoder) -> i32 {
    let mut c = r.read_ui8() << 16;
    c |= r.read_ui8() << 8;
    c |= r.read_ui8();
    c |= r.read_ui8() << 24;
    c
}

fn decode_rgb(r: &mut SwfDecoder) -> i32 {
    let mut c = r.read_ui8() << 16;
    c |= r.read_ui8() << 8;
    c |= r.read_ui8();
    c
}

fn decode_cxform(r: &mut SwfDecoder, alpha: bool) -> R<CXForm> {
    let mut c = CXForm { with_alpha: alpha, ..Default::default() };
    r.sync_bits();
    c.has_add = r.read_bit();
    c.has_mult = r.read_bit();
    let n = r.read_ubits(4);
    if c.has_mult {
        c.red_mult_term = r.read_sbits(n)?;
        c.green_mult_term = r.read_sbits(n)?;
        c.blue_mult_term = r.read_sbits(n)?;
        if alpha {
            c.alpha_mult_term = r.read_sbits(n)?;
        }
    }
    if c.has_add {
        c.red_add_term = r.read_sbits(n)?;
        c.green_add_term = r.read_sbits(n)?;
        c.blue_add_term = r.read_sbits(n)?;
        if alpha {
            c.alpha_add_term = r.read_sbits(n)?;
        }
    }
    Ok(c)
}

/// The action bytes `ActionDecoder.decode(length)` would re-encode, without the trailing end-of-actions byte.
fn decode_actions(r: &mut SwfDecoder, length: i64) -> R<ActionList> {
    let n = length.max(0) as usize;
    let mut bytes = r.read_fully(n.min(r.buf.len().saturating_sub(r.pos)))?;
    if bytes.last() == Some(&0) {
        bytes.pop();
    }
    Ok(ActionList { bytes })
}

impl TagDecoder {
    /// `decodeTags(handler)`: into `list` (the sprite's `TagList`, or nothing at top level).
    fn decode_tags(&mut self, r: &mut SwfDecoder, list: &mut Vec<TagRef>, top: bool) -> R<()> {
        loop {
            let h = r.read_ui16();
            if h == -1 {
                break;
            }
            let type_ = h >> 6;
            let mut length = (h & 0x3f) as i64;
            if length == 0x3f {
                length = r.read_si32() as i64;
                if length < 0 {
                    break;
                }
            }
            let o = r.get_offset();
            if type_ == 0 {
                break;
            }
            let t = self.decode_tag(r, type_, length)?;
            let consumed = (r.get_offset() - o) as i64;
            if consumed != length && consumed < length {
                r.read_fully((length - consumed) as usize)?;
            }
            if let Some(t) = t {
                let code = t.borrow().code;
                if !top && tag_list_keeps(code) {
                    list.push(t);
                }
            }
        }
        Ok(())
    }

    fn decode_tag(&mut self, r: &mut SwfDecoder, type_: i32, length: i64) -> R<Option<TagRef>> {
        use codes::*;
        let pos = r.get_offset();
        let len = length as usize;
        let t: Option<TagRef> = match type_ {
            SHOW_FRAME => Some(Tag::new_ref(SHOW_FRAME, Body::ShowFrame)),
            DEFINE_SHAPE | DEFINE_SHAPE2 | DEFINE_SHAPE3 | DEFINE_SHAPE4 => Some(self.decode_define_shape(r, type_)?),
            PLACE_OBJECT => Some(self.decode_place_object(r, length)?),
            REMOVE_OBJECT | REMOVE_OBJECT2 => {
                let mut ref_ = None;
                if type_ == REMOVE_OBJECT {
                    let idref = r.read_ui16();
                    ref_ = get_tag(&self.dict, idref)?;
                }
                let depth = r.read_ui16();
                Some(Tag::new_ref(type_, Body::RemoveObject { depth, ref_ }))
            }
            DEFINE_BINARY_DATA => {
                let id = r.read_ui16();
                let reserved = r.read_si32();
                let data = r.read_fully(len - (r.get_offset() - pos))?;
                let t = Tag::new_ref(type_, Body::DefineBinaryData { reserved, data });
                self.add(id, &t)?;
                Some(t)
            }
            DEFINE_BITS => {
                let id = r.read_ui16();
                let mut b = DefineBits::new(BitsKind::Bits);
                b.data = r.read_fully(len - (r.get_offset() - pos))?;
                b.jpeg_tables = self.jpeg_tables.clone();
                let t = Tag::new_ref(type_, Body::DefineBits(b));
                self.add(id, &t)?;
                Some(t)
            }
            DEFINE_BUTTON => {
                let _id = r.read_ui16();
                loop {
                    if self.decode_button_record(r, type_)?.is_none() {
                        break;
                    }
                }
                return Err(DecodeError::runtime("null"));
            }
            JPEG_TABLES => {
                let t = Tag::new_ref(type_, Body::Generic { data: Rc::new(r.read_fully(len)?) });
                self.jpeg_tables = Some(t.clone());
                Some(t)
            }
            SET_BACKGROUND_COLOR => Some(Tag::new_ref(type_, Body::SetBackgroundColor { color: decode_rgb(r) })),
            DEFINE_FONT => Some(self.decode_define_font(r)?),
            DEFINE_TEXT | DEFINE_TEXT2 => Some(self.decode_define_text(r, type_)?),
            DO_ACTION => Some(Tag::new_ref(type_, Body::DoAction { action_list: decode_actions(r, length)? })),
            DEFINE_FONT_INFO | DEFINE_FONT_INFO2 => Some(self.decode_define_font_info(r, type_, length)?),
            DEFINE_SOUND => {
                let id = r.read_ui16();
                r.sync_bits();
                let mut s = DefineSound { format: r.read_ubits(4), rate: r.read_ubits(2), size: r.read_ubits(1), type_: r.read_ubits(1), ..Default::default() };
                s.sample_count = r.read_ui32();
                s.data = r.read_fully(len - (r.get_offset() - pos))?;
                let t = Tag::new_ref(type_, Body::DefineSound(s));
                self.add(id, &t)?;
                Some(t)
            }
            START_SOUND => {
                let idref = r.read_ui16();
                let sound = get_tag(&self.dict, idref)?;
                let sound_info = decode_sound_info(r);
                Some(Tag::new_ref(type_, Body::StartSound { sound, sound_info }))
            }
            DEFINE_BUTTON_SOUND => Some(self.decode_define_button_sound(r)?),
            SOUND_STREAM_HEAD | SOUND_STREAM_HEAD2 => {
                r.sync_bits();
                r.read_ubits(4);
                let mut h = SoundStreamHead {
                    playback_rate: r.read_ubits(2),
                    playback_size: r.read_ubits(1),
                    playback_type: r.read_ubits(1),
                    compression: r.read_ubits(4),
                    stream_rate: r.read_ubits(2),
                    stream_size: r.read_ubits(1),
                    stream_type: r.read_ubits(1),
                    ..Default::default()
                };
                h.stream_sample_count = r.read_ui16();
                if h.compression == 2 {
                    h.latency_seek = r.read_si16();
                }
                Some(Tag::new_ref(type_, Body::SoundStreamHead(h)))
            }
            SOUND_STREAM_BLOCK | PROTECT => Some(Tag::new_ref(type_, Body::Generic { data: Rc::new(r.read_fully(len)?) })),
            DEFINE_BITS_LOSSLESS | DEFINE_BITS_LOSSLESS2 => Some(self.decode_define_bits_lossless(r, type_, length)?),
            DEFINE_BITS_JPEG2 => {
                let id = r.read_ui16();
                let mut b = DefineBits::new(BitsKind::Bits);
                b.data = r.read_fully(len - (r.get_offset() - pos))?;
                let t = Tag::new_ref(type_, Body::DefineBits(b));
                self.add(id, &t)?;
                Some(t)
            }
            DEFINE_BUTTON_CXFORM => {
                let idref = r.read_ui16();
                let button = need(get_tag(&self.dict, idref)?)?;
                let color_transform = decode_cxform(r, false)?;
                let t = Tag::new_ref(type_, Body::DefineButtonCxform(DefineButtonCxform { button: Some(Rc::downgrade(&button)), color_transform }));
                if let Body::DefineButton(b) = &mut button.borrow_mut().body {
                    b.cxform = Some(t.clone());
                } else {
                    return Err(DecodeError::runtime("ClassCastException"));
                }
                Some(t)
            }
            PLACE_OBJECT2 | PLACE_OBJECT3 => Some(self.decode_place_object23(r, type_, length)?),
            DEFINE_BUTTON2 => Some(self.decode_define_button2(r, length)?),
            DEFINE_BITS_JPEG3 => {
                let id = r.read_ui16();
                let mut b = DefineBits::new(BitsKind::Jpeg3);
                b.alpha_data_offset = r.read_ui32();
                b.data = r.read_fully(b.alpha_data_offset as usize)?;
                let rest = r.read_fully(len - (r.get_offset() - pos))?;
                let mut alpha = Vec::new();
                let _ = std::io::Read::read_to_end(&mut flate2::read::ZlibDecoder::new(&rest[..]), &mut alpha);
                b.alpha_data = alpha;
                let t = Tag::new_ref(type_, Body::DefineBits(b));
                self.add(id, &t)?;
                Some(t)
            }
            DEFINE_EDIT_TEXT => Some(self.decode_define_edit_text(r)?),
            DEFINE_SPRITE => Some(self.decode_define_sprite(r, pos + len)?),
            DEFINE_SCALING_GRID => self.decode_define_scaling_grid(r)?,
            FRAME_LABEL => {
                let label = r.read_string()?;
                let mut anchor = false;
                if self.header.version >= 6 && length - (r.get_offset() - pos) as i64 == 1 {
                    anchor = r.read_ui8() != 0;
                }
                Some(Tag::new_ref(type_, Body::FrameLabel { label, anchor }))
            }
            DEFINE_MORPH_SHAPE | DEFINE_MORPH_SHAPE2 => Some(self.decode_define_morph_shape(r, type_)?),
            DEFINE_FONT2 | DEFINE_FONT3 => Some(self.decode_define_font2_and3(r, type_)?),
            DEFINE_FONT4 => {
                let id = r.read_ui16();
                r.sync_bits();
                r.read_ubits(5);
                let mut f = DefineFont4 { has_font_data: r.read_bit(), italic: r.read_bit(), bold: r.read_bit(), ..Default::default() };
                f.font_name = Some(r.read_string()?);
                if f.has_font_data {
                    f.data = r.read_fully(len - (r.get_offset() - pos))?;
                }
                let t = Tag::new_ref(type_, Body::DefineFont4(f));
                self.add(id, &t)?;
                self.dict.add_font_face(&t);
                Some(t)
            }
            EXPORT_ASSETS => Some(self.decode_export_assets(r)?),
            IMPORT_ASSETS | IMPORT_ASSETS2 => Some(self.decode_import_assets(r, type_)?),
            DO_INIT_ACTION => {
                let idref = r.read_ui16();
                let sprite = get_tag(&self.dict, idref).ok().flatten();
                let action_list = decode_actions(r, length - 2)?;
                let t = Tag::new_ref(type_, Body::DoInitAction { sprite: sprite.as_ref().map(Rc::downgrade), action_list });
                if let Some(s) = sprite {
                    if let Some(sp) = s.borrow_mut().sprite_mut() {
                        if sp.init_action.is_none() {
                            sp.init_action = Some(t.clone());
                        }
                    }
                }
                Some(t)
            }
            DEFINE_VIDEO_STREAM => {
                let id = r.read_ui16();
                let mut v = DefineVideoStream { num_frames: r.read_ui16(), width: r.read_ui16(), height: r.read_ui16(), ..Default::default() };
                r.sync_bits();
                r.read_ubits(4);
                v.deblocking = r.read_ubits(3);
                v.smoothing = r.read_bit();
                v.codec_id = r.read_ui8();
                let t = Tag::new_ref(type_, Body::DefineVideoStream(v));
                self.add(id, &t)?;
                Some(t)
            }
            VIDEO_FRAME => {
                let idref = r.read_ui16();
                let stream = get_tag(&self.dict, idref)?;
                let frame_num = r.read_ui16();
                let video_data = r.read_fully(len - (r.get_offset() - pos))?;
                Some(Tag::new_ref(type_, Body::VideoFrame { stream, frame_num, video_data }))
            }
            SYMBOL_CLASS => {
                let count = r.read_ui16();
                let mut class2tag: crate::asc::util::jhash::JHashMap<String, TagRef> = crate::asc::util::jhash::JHashMap::with_capacity(count.max(0) as usize);
                for _ in 0..count.max(0) {
                    let idref = r.read_ui16();
                    let name = r.read_string()?;
                    if idref == 0 {
                        continue;
                    }
                    let tref = need(get_tag(&self.dict, idref)?)?;
                    class2tag.insert(name.clone(), tref.clone());
                    if tref.borrow().name.is_none() {
                        tref.borrow_mut().name = Some(name.clone());
                        self.dict.add_name(&tref, &name);
                    }
                }
                let n = class2tag.len();
                self.frame_symbols.put_all(class2tag.iter().map(|(k, v)| (k.clone(), v.clone())), n);
                Some(Tag::new_ref(type_, Body::Generic { data: Rc::new(Vec::new()) }))
            }
            DEFINE_FONT_ALIGN_ZONES => {
                let font_id = r.read_ui16();
                let font = need(get_tag(&self.dict, font_id)?)?;
                let hint = r.read_ubits(2);
                r.read_ubits(6);
                let glyphs = match &font.borrow().body {
                    Body::DefineFont2(f) => f.glyph_shape_table.len(),
                    _ => return Err(DecodeError::runtime("ClassCastException")),
                };
                let mut zone_table = Vec::with_capacity(glyphs);
                for _ in 0..glyphs {
                    let n = r.read_ui8();
                    let zone_data: Vec<i64> = (0..n.max(0)).map(|_| r.read_ui32()).collect();
                    zone_table.push(Rc::new(ZoneRecord { num_zone_data: n, zone_data, zone_mask: r.read_ui8() }));
                }
                let t = Tag::new_ref(type_, Body::DefineFontAlignZones(DefineFontAlignZones { font: Some(Rc::downgrade(&font)), csm_table_hint: hint, zone_table }));
                if let Body::DefineFont2(f) = &mut font.borrow_mut().body {
                    f.zones = Some(t.clone());
                }
                Some(t)
            }
            CSM_TEXT_SETTINGS => {
                let text_id = r.read_ui16();
                let mut c = CsmTextSettings::default();
                let mut target = None;
                if text_id != 0 {
                    let tr = need(get_tag(&self.dict, text_id)?)?;
                    c.text_reference = Some(Rc::downgrade(&tr));
                    target = Some(tr);
                }
                c.style_flags_use_saffron = r.read_ubits(2);
                c.grid_fit_type = r.read_ubits(3);
                r.read_ubits(3);
                c.thickness = r.read_ubits(32) as i64;
                c.sharpness = r.read_ubits(32) as i64;
                r.read_ubits(8);
                let t = Tag::new_ref(type_, Body::CsmTextSettings(c));
                if let Some(tr) = target {
                    match &mut tr.borrow_mut().body {
                        Body::DefineText(x) => x.csm_text_settings = Some(t.clone()),
                        Body::DefineEditText(x) => x.csm_text_settings = Some(t.clone()),
                        _ => {}
                    }
                }
                Some(t)
            }
            DEFINE_FONT_NAME => {
                let font_id = r.read_ui16();
                let font = need(get_tag(&self.dict, font_id)?)?;
                let n = DefineFontName { font: Some(Rc::downgrade(&font)), font_name: Some(r.read_string()?), copyright: Some(r.read_string()?) };
                let t = Tag::new_ref(type_, Body::DefineFontName(n));
                match &mut font.borrow_mut().body {
                    Body::DefineFont1(f) => f.license = Some(t.clone()),
                    Body::DefineFont2(f) => f.license = Some(t.clone()),
                    Body::DefineFont4(f) => f.license = Some(t.clone()),
                    _ => return Err(DecodeError::runtime("ClassCastException")),
                }
                Some(t)
            }
            SET_TAB_INDEX => {
                let depth = r.read_ui16();
                let index = r.read_ui16();
                Some(Tag::new_ref(type_, Body::SetTabIndex { depth, index }))
            }
            _ => Some(Tag::new_ref(type_, Body::Generic { data: Rc::new(r.read_fully(len)?) })),
        };
        let consumed = (r.get_offset() - pos) as i64;
        if consumed != length && type_ == codes::SOUND_STREAM_HEAD && consumed != length + 2 {
            return Err(DecodeError::io(format!("{} at pos {pos}: {consumed} bytes were read. {length} byte were required.", tag_name(type_))));
        }
        Ok(t)
    }

    fn add(&mut self, id: i32, t: &TagRef) -> R<()> {
        self.dict.add_with_id(id, t.clone()).map_err(DecodeError::runtime)
    }

    fn decode_define_shape(&mut self, r: &mut SwfDecoder, shape: i32) -> R<TagRef> {
        let id = r.read_ui16();
        let mut s = DefineShape { bounds: decode_rect(r)?, ..Default::default() };
        if shape == codes::DEFINE_SHAPE4 {
            s.edge_bounds = Some(decode_rect(r)?);
            r.read_ubits(5);
            s.uses_fill_winding_rule = r.read_bit();
            s.uses_non_scaling_strokes = r.read_bit();
            s.uses_scaling_strokes = r.read_bit();
        }
        r.sync_bits();
        let fillstyles = self.decode_fillstyles(r, shape)?;
        let linestyles = self.decode_linestyles(r, shape)?;
        let records = self.decode_shape(r, shape)?;
        s.shape_with_style = ShapeWithStyle { shape_records: records.shape_records, fillstyles: Some(fillstyles), linestyles: Some(linestyles) };
        let t = Tag::new_ref(shape, Body::DefineShape(s));
        self.add(id, &t)?;
        Ok(t)
    }

    fn decode_linestyles(&mut self, r: &mut SwfDecoder, shape: i32) -> R<Vec<LineStyle>> {
        let mut count = r.read_ui8();
        if count == 0xff {
            count = r.read_ui16();
        }
        let mut a = Vec::new();
        for _ in 0..count.max(0) {
            let mut s = LineStyle { width: r.read_ui16(), ..Default::default() };
            if shape == codes::DEFINE_SHAPE4 {
                s.flags = r.read_ui16();
                if s.has_miter_joint() {
                    s.miter_limit = r.read_ui16();
                }
            }
            if shape == codes::DEFINE_SHAPE4 && s.has_fill_style() {
                s.fill_style = Some(self.decode_fill_style(r, shape)?);
            } else if shape == codes::DEFINE_SHAPE3 || shape == codes::DEFINE_SHAPE4 {
                s.color = decode_rgba(r);
            } else {
                s.color = decode_rgb(r);
            }
            a.push(s);
        }
        Ok(a)
    }

    fn decode_fillstyles(&mut self, r: &mut SwfDecoder, shape: i32) -> R<Vec<FillStyle>> {
        let mut count = r.read_ui8();
        if count == 0xff {
            count = r.read_ui16();
        }
        let mut a = Vec::new();
        for _ in 0..count.max(0) {
            a.push(self.decode_fill_style(r, shape)?);
        }
        Ok(a)
    }

    fn decode_fill_style(&mut self, r: &mut SwfDecoder, shape: i32) -> R<FillStyle> {
        let mut s = FillStyle { type_: r.read_ui8(), ..Default::default() };
        match s.type_ {
            FILL_SOLID => {
                if shape == codes::DEFINE_SHAPE3 || shape == codes::DEFINE_SHAPE4 {
                    s.color = decode_rgba(r);
                } else if shape == codes::DEFINE_SHAPE2 || shape == codes::DEFINE_SHAPE {
                    s.color = decode_rgb(r);
                } else {
                    return Err(DecodeError::io("bad shape code"));
                }
            }
            FILL_GRADIENT | FILL_RADIAL_GRADIENT | FILL_FOCAL_RADIAL_GRADIENT => {
                s.matrix = Some(decode_matrix(r)?);
                s.gradient = Some(decode_gradient(r, shape, s.type_));
            }
            0x40..=0x43 => {
                let idref = r.read_ui16();
                s.bitmap = self.dict.get_tag(idref).ok().flatten();
                s.matrix = Some(decode_matrix(r)?);
            }
            t => return Err(DecodeError::io(format!("unrecognized fill style type: {t}"))),
        }
        Ok(s)
    }

    fn decode_shape(&mut self, r: &mut SwfDecoder, shape: i32) -> R<Shape> {
        r.sync_bits();
        let mut nf = r.read_ubits(4);
        let mut nl = r.read_ubits(4);
        Ok(Shape { shape_records: Some(self.decode_shape_records(r, shape, &mut nf, &mut nl)?) })
    }

    fn decode_glyph(&mut self, r: &mut SwfDecoder, shape: i32, count: i64) -> R<Shape> {
        let mut s = Shape::default();
        r.sync_bits();
        if count > 0 {
            let mut nf = r.read_ubits(4);
            let mut nl = r.read_ubits(4);
            if count > 1 {
                s.shape_records = Some(self.decode_shape_records(r, shape, &mut nf, &mut nl)?);
            }
        }
        Ok(s)
    }

    fn decode_shape_records(&mut self, r: &mut SwfDecoder, shape: i32, nf: &mut i32, nl: &mut i32) -> R<Vec<ShapeRecord>> {
        let mut list = Vec::new();
        loop {
            if r.past_end > 8 {
                return Err(DecodeError::io("EOF"));
            }
            if r.read_bit() {
                if r.read_bit() {
                    let nbits = 2 + r.read_ubits(4);
                    if r.read_bit() {
                        let dx = r.read_sbits(nbits)?;
                        let dy = r.read_sbits(nbits)?;
                        list.push(ShapeRecord::straight(dx, dy));
                    } else if r.read_bit() {
                        list.push(ShapeRecord::straight(0, r.read_sbits(nbits)?));
                    } else {
                        list.push(ShapeRecord::straight(r.read_sbits(nbits)?, 0));
                    }
                } else {
                    let nbits = 2 + r.read_ubits(4);
                    list.push(ShapeRecord::CurvedEdge {
                        control_delta_x: r.read_sbits(nbits)?,
                        control_delta_y: r.read_sbits(nbits)?,
                        anchor_delta_x: r.read_sbits(nbits)?,
                        anchor_delta_y: r.read_sbits(nbits)?,
                    });
                }
            } else {
                let new_styles = r.read_bit();
                let line_style = r.read_bit();
                let fill_style1 = r.read_bit();
                let fill_style0 = r.read_bit();
                let move_to = r.read_bit();
                if !(new_styles || line_style || fill_style1 || fill_style0 || move_to) {
                    break;
                }
                let mut s = StyleChangeRecord {
                    state_new_styles: new_styles,
                    state_line_style: line_style,
                    state_fill_style1: fill_style1,
                    state_fill_style0: fill_style0,
                    state_move_to: move_to,
                    ..Default::default()
                };
                if move_to {
                    let mb = r.read_ubits(5);
                    s.move_delta_x = r.read_sbits(mb)?;
                    s.move_delta_y = r.read_sbits(mb)?;
                }
                if fill_style0 {
                    s.fillstyle0 = r.read_ubits(*nf);
                }
                if fill_style1 {
                    s.fillstyle1 = r.read_ubits(*nf);
                }
                if line_style {
                    s.linestyle = r.read_ubits(*nl);
                }
                if new_styles {
                    s.fillstyles = Some(self.decode_fillstyles(r, shape)?);
                    s.linestyles = Some(self.decode_linestyles(r, shape)?);
                    r.sync_bits();
                    *nf = r.read_ubits(4);
                    *nl = r.read_ubits(4);
                }
                list.push(ShapeRecord::StyleChange(s));
            }
        }
        Ok(list)
    }

    fn decode_place_object(&mut self, r: &mut SwfDecoder, length: i64) -> R<TagRef> {
        let pos = r.get_offset();
        let idref = r.read_ui16();
        let mut p = PlaceObject { depth: r.read_ui16(), ..Default::default() };
        p.set_matrix(Some(decode_matrix(r)?));
        if length - (r.get_offset() - pos) as i64 != 0 {
            p.color_transform = Some(decode_cxform(r, false)?);
            p.flags |= HAS_CXFORM;
        }
        p.set_ref(need(get_tag(&self.dict, idref)?)?);
        Ok(Tag::new_ref(codes::PLACE_OBJECT, Body::PlaceObject(p)))
    }

    fn decode_place_object23(&mut self, r: &mut SwfDecoder, type_: i32, length: i64) -> R<TagRef> {
        let pos = r.get_offset();
        let mut p = PlaceObject { flags: r.read_ui8(), ..Default::default() };
        if type_ == codes::PLACE_OBJECT3 {
            p.flags2 = r.read_ui8();
        }
        p.depth = r.read_ui16();
        if p.has_class_name() {
            p.class_name = Some(r.read_string()?);
        }
        if p.has_char_id() {
            let idref = r.read_ui16();
            p.set_ref(need(get_tag(&self.dict, idref)?)?);
        }
        if p.has_matrix() {
            p.matrix = Some(decode_matrix(r)?);
        }
        if p.has_cxform() {
            p.color_transform = Some(decode_cxform(r, true)?);
            p.flags |= HAS_CXFORM;
        }
        if p.has_ratio() {
            p.ratio = r.read_ui16();
        }
        if p.has_name() {
            p.name = Some(r.read_string()?);
        }
        if p.has_clip_depth() {
            p.clip_depth = r.read_ui16();
        }
        if type_ == codes::PLACE_OBJECT3 {
            if p.has_filter_list() {
                p.filters = Some(decode_filter_list(r)?);
            }
            if p.has_blend_mode() {
                p.blend_mode = r.read_ui8();
            }
        }
        if p.has_clip_action() {
            let rest = (length - (r.get_offset() - pos) as i64).max(0) as usize;
            p.clip_actions = Some(ClipActions { bytes: r.read_fully(rest)? });
        }
        Ok(Tag::new_ref(type_, Body::PlaceObject(p)))
    }

    fn decode_button_record(&mut self, r: &mut SwfDecoder, type_: i32) -> R<Option<ButtonRecord>> {
        let mut b = ButtonRecord::default();
        r.sync_bits();
        let (reserved, has_blend_mode, has_filter_list);
        if type_ == codes::DEFINE_BUTTON2 {
            reserved = r.read_ubits(2);
            has_blend_mode = r.read_bit();
            has_filter_list = r.read_bit();
        } else {
            reserved = r.read_ubits(4);
            has_blend_mode = false;
            has_filter_list = false;
        }
        b.hit_test = r.read_bit();
        b.down = r.read_bit();
        b.over = r.read_bit();
        b.up = r.read_bit();
        if reserved == 0 && !b.hit_test && !b.down && !b.over && !b.up {
            return Ok(None);
        }
        let idref = r.read_ui16();
        b.character_ref = get_tag(&self.dict, idref)?;
        b.place_depth = r.read_ui16();
        b.place_matrix = Some(decode_matrix(r)?);
        if type_ == codes::DEFINE_BUTTON2 {
            b.color_transform = Some(decode_cxform(r, true)?);
            if has_filter_list {
                b.filters = Some(decode_filter_list(r)?);
            }
            if has_blend_mode {
                b.blend_mode = r.read_ui8();
            }
        }
        Ok(Some(b))
    }

    fn decode_define_button2(&mut self, r: &mut SwfDecoder, length: i64) -> R<TagRef> {
        let endpos = r.get_offset() + length as usize;
        let id = r.read_ui16();
        r.sync_bits();
        r.read_ubits(7);
        let mut b = DefineButton { track_as_menu: r.read_bit(), ..Default::default() };
        let mut action_offset = r.read_ui16();
        while let Some(rec) = self.decode_button_record(r, codes::DEFINE_BUTTON2)? {
            b.button_records.push(rec);
        }
        if action_offset > 0 {
            let mut pos = r.get_offset();
            loop {
                action_offset = r.read_ui16();
                if action_offset <= 0 {
                    break;
                }
                b.cond_actions.push(decode_button_cond_action(r, action_offset as i64 - 2)?);
                if r.get_offset() != pos + action_offset as usize {
                    return Err(DecodeError::io(format!("incorrect offset read in ButtonCondAction. read {action_offset}")));
                }
                pos = r.get_offset();
            }
            let rest = endpos as i64 - r.get_offset() as i64;
            b.cond_actions.push(decode_button_cond_action(r, rest)?);
        }
        while r.get_offset() < endpos {
            if r.read_ui8() != 0 {
                return Err(DecodeError::io("nonzero data past end of DefineButton2"));
            }
        }
        let t = Tag::new_ref(codes::DEFINE_BUTTON2, Body::DefineButton(b));
        self.add(id, &t)?;
        Ok(t)
    }

    fn decode_define_button_sound(&mut self, r: &mut SwfDecoder) -> R<TagRef> {
        let idref = r.read_ui16();
        let button = need(get_tag(&self.dict, idref)?)?;
        let mut s = DefineButtonSound { button: Some(Rc::downgrade(&button)), ..Default::default() };
        for i in 0..4 {
            let idref = r.read_ui16();
            if idref != 0 {
                let sound = get_tag(&self.dict, idref)?;
                let info = Some(decode_sound_info(r));
                match i {
                    0 => (s.sound0, s.info0) = (sound, info),
                    1 => (s.sound1, s.info1) = (sound, info),
                    2 => (s.sound2, s.info2) = (sound, info),
                    _ => (s.sound3, s.info3) = (sound, info),
                }
            }
        }
        let t = Tag::new_ref(codes::DEFINE_BUTTON_SOUND, Body::DefineButtonSound(s));
        if let Body::DefineButton(b) = &mut button.borrow_mut().body {
            b.sounds = Some(t.clone());
        } else {
            return Err(DecodeError::runtime("ClassCastException"));
        }
        Ok(t)
    }

    fn decode_define_bits_lossless(&mut self, r: &mut SwfDecoder, type_: i32, length: i64) -> R<TagRef> {
        let pos = r.get_offset();
        let id = r.read_ui16();
        let mut b = DefineBits::new(BitsKind::Lossless);
        b.format = r.read_ui8();
        b.width = r.read_ui16();
        b.height = r.read_ui16();
        let alpha = type_ == codes::DEFINE_BITS_LOSSLESS2;
        let inflate = |data: &[u8]| {
            let mut out = Vec::new();
            let _ = std::io::Read::read_to_end(&mut flate2::read::ZlibDecoder::new(data), &mut out);
            out
        };
        match b.format {
            3 => {
                let table_size = r.read_ui8() + 1;
                let data = r.read_fully(length as usize - (r.get_offset() - pos))?;
                let raw = inflate(&data);
                let mut r1 = SwfDecoder::new(&raw, self.header.version);
                let colors: Vec<i32> = (0..table_size).map(|_| if alpha { decode_rgba(&mut r1) } else { decode_rgb(&mut r1) }).collect();
                b.color_data = Some(colors);
                let width = if b.width % 4 != 0 { (b.width / 4 + 1) * 4 } else { b.width };
                let size = (width * b.height) as usize;
                let rest = &raw[r1.get_offset().min(raw.len())..];
                if alpha {
                    if rest.len() > size {
                        return Err(DecodeError::io(format!("{} bytes of bitmap data ({width}x{}) not read!", rest.len() - size, b.height)));
                    } else if rest.len() < size {
                        return Err(DecodeError::io(format!("({width}x{}) data buffer {} bytes too big...", b.height, size - rest.len())));
                    }
                } else if rest.len() < size {
                    return Err(DecodeError::io(format!("couldn't read {}", size - rest.len())));
                }
                b.data = rest[..size].to_vec();
            }
            4 | 5 => {
                let data = r.read_fully(length as usize - (r.get_offset() - pos))?;
                let raw = inflate(&data);
                let size = (b.width * b.height * 4) as usize;
                if raw.len() < size {
                    return Err(DecodeError::io(format!("couldn't read {}", size - raw.len())));
                }
                b.data = raw[..size].to_vec();
            }
            f => return Err(DecodeError::io(format!("Illegal bitmap format {f}"))),
        }
        let t = Tag::new_ref(type_, Body::DefineBits(b));
        self.add(id, &t)?;
        Ok(t)
    }

    fn decode_define_edit_text(&mut self, r: &mut SwfDecoder) -> R<TagRef> {
        let id = r.read_ui16();
        let mut e = DefineEditText { bounds: decode_rect(r)?, ..Default::default() };
        r.sync_bits();
        e.has_text = r.read_bit();
        e.word_wrap = r.read_bit();
        e.multiline = r.read_bit();
        e.password = r.read_bit();
        e.read_only = r.read_bit();
        e.has_text_color = r.read_bit();
        e.has_max_length = r.read_bit();
        e.has_font = r.read_bit();
        e.has_font_class = r.read_bit();
        e.auto_size = r.read_bit();
        e.has_layout = r.read_bit();
        e.no_select = r.read_bit();
        e.border = r.read_bit();
        e.was_static = r.read_bit();
        e.html = r.read_bit();
        e.use_outlines = r.read_bit();
        if e.has_font {
            let idref = r.read_ui16();
            e.font = get_tag(&self.dict, idref)?;
            e.height = r.read_ui16();
        }
        if e.has_font_class {
            e.font_class = Some(r.read_string()?);
            e.height = r.read_ui16();
        }
        if e.has_text_color {
            e.color = decode_rgba(r);
        }
        if e.has_max_length {
            e.max_length = r.read_ui16();
        }
        if e.has_layout {
            e.align = r.read_ui8();
            e.left_margin = r.read_ui16();
            e.right_margin = r.read_ui16();
            e.ident = r.read_ui16();
            e.leading = r.read_si16();
        }
        e.var_name = Some(r.read_string()?);
        if e.has_text {
            e.initial_text = Some(r.read_string()?);
        }
        let t = Tag::new_ref(codes::DEFINE_EDIT_TEXT, Body::DefineEditText(e));
        self.add(id, &t)?;
        Ok(t)
    }

    fn decode_define_sprite(&mut self, r: &mut SwfDecoder, endpos: usize) -> R<TagRef> {
        let id = r.read_ui16();
        let framecount = r.read_ui16();
        let mut list = Vec::new();
        self.decode_tags(r, &mut list, false)?;
        while r.get_offset() < endpos {
            if r.read_ui8() != 0 {
                return Err(DecodeError::io("nonzero data past end of sprite"));
            }
        }
        let t = Tag::new_ref(codes::DEFINE_SPRITE, Body::DefineSprite(DefineSprite { framecount, tag_list: list, ..Default::default() }));
        self.add(id, &t)?;
        Ok(t)
    }

    /// `decodeDefineScalingGrid()`; an unknown target makes it `null`, which no handler keeps.
    fn decode_define_scaling_grid(&mut self, r: &mut SwfDecoder) -> R<Option<TagRef>> {
        let idref = r.read_ui16();
        let Ok(Some(target)) = self.dict.get_tag(idref) else { return Ok(None) };
        let t = Tag::new_ref(codes::DEFINE_SCALING_GRID, Body::DefineScalingGrid(DefineScalingGrid { scaling_target: Some(Rc::downgrade(&target)), rect: Rect::default() }));
        match &mut target.borrow_mut().body {
            Body::DefineSprite(s) => s.scaling_grid = Some(t.clone()),
            Body::DefineButton(b) => b.scaling_grid = Some(t.clone()),
            _ => {}
        }
        let rect = decode_rect(r)?;
        if let Body::DefineScalingGrid(g) = &mut t.borrow_mut().body {
            g.rect = rect;
        }
        Ok(Some(t))
    }

    fn decode_define_morph_shape(&mut self, r: &mut SwfDecoder, code: i32) -> R<TagRef> {
        let id = r.read_ui16();
        let mut m = DefineMorphShape { start_bounds: decode_rect(r)?, end_bounds: decode_rect(r)?, ..Default::default() };
        if code == codes::DEFINE_MORPH_SHAPE2 {
            m.start_edge_bounds = decode_rect(r)?;
            m.end_edge_bounds = decode_rect(r)?;
            r.read_ubits(6);
            m.uses_non_scaling_strokes = r.read_bit();
            m.uses_scaling_strokes = r.read_bit();
        }
        let offset = r.read_ui32();
        let mut count = r.read_ui8();
        if count == 0xff {
            count = r.read_ui16();
        }
        for _ in 0..count.max(0) {
            let s = self.decode_morph_fill_style(r, code)?;
            m.fill_styles.push(s);
        }
        let mut count = r.read_ui8();
        if count == 0xff {
            count = r.read_ui16();
        }
        for _ in 0..count.max(0) {
            let mut s = MorphLineStyle { start_width: r.read_ui16(), end_width: r.read_ui16(), ..Default::default() };
            if code == codes::DEFINE_MORPH_SHAPE2 {
                s.start_caps_style = r.read_ubits(2);
                s.joint_style = r.read_ubits(2);
                s.has_fill = r.read_bit();
                s.no_h_scale = r.read_bit();
                s.no_v_scale = r.read_bit();
                s.pixel_hinting = r.read_bit();
                r.read_ubits(5);
                s.no_close = r.read_bit();
                s.end_caps_style = r.read_ubits(2);
                if s.joint_style == 2 {
                    s.miter_limit = r.read_ui16();
                }
            }
            if !s.has_fill {
                s.start_color = decode_rgba(r);
                s.end_color = decode_rgba(r);
            }
            if s.has_fill {
                s.fill_type = Some(self.decode_morph_fill_style(r, code)?);
            }
            m.line_styles.push(s);
        }
        m.start_edges = Some(self.decode_shape(r, codes::DEFINE_SHAPE3)?);
        if offset != 0 {
            m.end_edges = Some(self.decode_shape(r, codes::DEFINE_SHAPE3)?);
        }
        let t = Tag::new_ref(code, Body::DefineMorphShape(m));
        self.add(id, &t)?;
        Ok(t)
    }

    fn decode_morph_fill_style(&mut self, r: &mut SwfDecoder, shape: i32) -> R<MorphFillStyle> {
        let mut s = MorphFillStyle { type_: r.read_ui8(), ..Default::default() };
        match s.type_ {
            FILL_SOLID => {
                s.start_color = decode_rgba(r);
                s.end_color = decode_rgba(r);
            }
            FILL_GRADIENT | FILL_RADIAL_GRADIENT | FILL_FOCAL_RADIAL_GRADIENT => {
                s.start_gradient_matrix = Some(decode_matrix(r)?);
                s.end_gradient_matrix = Some(decode_matrix(r)?);
                let num = r.read_ui8();
                let recs = (0..num.max(0))
                    .map(|_| {
                        let start_ratio = r.read_ui8();
                        let start_color = decode_rgba(r);
                        let end_ratio = r.read_ui8();
                        let end_color = decode_rgba(r);
                        MorphGradRecord { start_ratio, end_ratio, start_color, end_color }
                    })
                    .collect();
                s.grad_records = Some(recs);
                if s.type_ == FILL_FOCAL_RADIAL_GRADIENT && shape == codes::DEFINE_MORPH_SHAPE2 {
                    s.ratio1 = r.read_si16();
                    s.ratio2 = r.read_si16();
                }
            }
            0x40..=0x43 => {
                let idref = r.read_ui16();
                s.bitmap = self.dict.get_tag(idref).ok().flatten();
                s.start_bitmap_matrix = Some(decode_matrix(r)?);
                s.end_bitmap_matrix = Some(decode_matrix(r)?);
            }
            t => return Err(DecodeError::io(format!("unrecognized fill style type: {t}"))),
        }
        Ok(s)
    }

    fn decode_define_font(&mut self, r: &mut SwfDecoder) -> R<TagRef> {
        let id = r.read_ui16();
        let offset = r.read_ui16();
        let num_glyphs = (offset / 2).max(0);
        for _ in 1..num_glyphs {
            r.read_ui16();
        }
        let mut f = DefineFont1::default();
        for _ in 0..num_glyphs {
            f.glyph_shape_table.push(self.decode_shape(r, codes::DEFINE_SHAPE3)?);
        }
        let t = Tag::new_ref(codes::DEFINE_FONT, Body::DefineFont1(f));
        self.add(id, &t)?;
        self.dict.add_font_face(&t);
        Ok(t)
    }

    fn decode_define_font_info(&mut self, r: &mut SwfDecoder, code: i32, length: i64) -> R<TagRef> {
        let pos = r.get_offset();
        let idref = r.read_ui16();
        let font = need(get_tag(&self.dict, idref)?)?;
        let mut i = DefineFontInfo { font: Some(Rc::downgrade(&font)), ..Default::default() };
        i.name = Some(r.read_length_string()?);
        r.sync_bits();
        r.read_ubits(3);
        i.shift_jis = r.read_bit();
        i.ansi = r.read_bit();
        i.italic = r.read_bit();
        i.bold = r.read_bit();
        i.wide_codes = r.read_bit();
        if code == codes::DEFINE_FONT_INFO2 {
            i.lang_code = r.read_ui8();
        }
        let mut len = length - (r.get_offset() - pos) as i64;
        if i.wide_codes {
            len /= 2;
            i.code_table = (0..len.max(0)).map(|_| r.read_ui16() as u16).collect();
        } else {
            i.code_table = (0..len.max(0)).map(|_| r.read_ui8() as u16).collect();
        }
        let t = Tag::new_ref(code, Body::DefineFontInfo(i));
        if let Body::DefineFont1(f) = &mut font.borrow_mut().body {
            f.font_info = Some(t.clone());
        } else {
            return Err(DecodeError::runtime("ClassCastException"));
        }
        Ok(t)
    }

    fn decode_define_font2_and3(&mut self, r: &mut SwfDecoder, code: i32) -> R<TagRef> {
        let id = r.read_ui16();
        r.sync_bits();
        let mut f = DefineFont2 {
            has_layout: r.read_bit(),
            shift_jis: r.read_bit(),
            small_text: r.read_bit(),
            ansi: r.read_bit(),
            wide_offsets: r.read_bit(),
            wide_codes: r.read_bit(),
            italic: r.read_bit(),
            bold: r.read_bit(),
            ..Default::default()
        };
        f.lang_code = r.read_ui8();
        f.font_name = Some(r.read_length_string()?);
        let num_glyphs = r.read_ui16().max(0) as usize;
        let offsets: Vec<i64> = (0..num_glyphs).map(|_| if f.wide_offsets { r.read_ui32() } else { r.read_ui16() as i64 }).collect();
        let mut code_table_offset = 0i64;
        if num_glyphs > 0 {
            code_table_offset = if f.wide_offsets { r.read_ui32() } else { r.read_ui16() as i64 };
        }
        for i in 0..num_glyphs {
            let glyph_length = if i < num_glyphs - 1 { offsets[i + 1] - offsets[i] } else { code_table_offset - offsets[i] };
            f.glyph_shape_table.push(self.decode_glyph(r, codes::DEFINE_SHAPE3, glyph_length)?);
        }
        f.code_table = (0..num_glyphs).map(|_| if f.wide_codes { r.read_ui16() as u16 } else { r.read_ui8() as u16 }).collect();
        if f.has_layout {
            f.ascent = r.read_si16();
            f.descent = r.read_si16();
            f.leading = r.read_si16();
            f.advance_table = (0..num_glyphs).map(|_| r.read_si16() as i16).collect();
            for _ in 0..num_glyphs {
                f.bounds_table.push(decode_rect(r)?);
            }
            f.kerning_count = r.read_ui16();
            for _ in 0..f.kerning_count.max(0) {
                let code1 = if f.wide_codes { r.read_ui16() } else { r.read_ui8() };
                let code2 = if f.wide_codes { r.read_ui16() } else { r.read_ui8() };
                f.kerning_table.push(KerningRecord { code1, code2, adjustment: r.read_ui16() });
            }
        }
        let t = Tag::new_ref(code, Body::DefineFont2(f));
        self.add(id, &t)?;
        self.dict.add_font_face(&t);
        Ok(t)
    }

    fn decode_define_text(&mut self, r: &mut SwfDecoder, type_: i32) -> R<TagRef> {
        let id = r.read_ui16();
        let mut d = DefineText { bounds: decode_rect(r)?, matrix: decode_matrix(r)?, ..Default::default() };
        let glyph_bits = r.read_ui8();
        let advance_bits = r.read_ui8();
        loop {
            let flags = r.read_ui8();
            if flags == 0 || flags == -1 {
                break;
            }
            let mut t = TextRecord { flags, ..Default::default() };
            if t.has_font() {
                let idref = r.read_ui16();
                t.font = get_tag(&self.dict, idref)?;
            }
            if t.has_color() {
                t.color = if type_ == codes::DEFINE_TEXT { decode_rgb(r) } else { decode_rgba(r) };
            }
            if t.has_x() {
                t.x_offset = r.read_si16();
            }
            if t.has_y() {
                t.y_offset = r.read_si16();
            }
            if t.has_height() {
                t.height = r.read_ui16();
            }
            let count = r.read_ui8();
            r.sync_bits();
            for _ in 0..count.max(0) {
                let index = r.read_ubits(glyph_bits);
                let advance = r.read_sbits(advance_bits)?;
                t.entries.push(GlyphEntry { index, advance });
            }
            d.records.push(t);
        }
        let t = Tag::new_ref(type_, Body::DefineText(d));
        self.add(id, &t)?;
        Ok(t)
    }

    fn decode_export_assets(&mut self, r: &mut SwfDecoder) -> R<TagRef> {
        let count = r.read_ui16();
        let mut exports = Vec::new();
        for _ in 0..count.max(0) {
            let idref = r.read_ui16();
            let name = r.read_string()?;
            let tref = need(get_tag(&self.dict, idref)?)?;
            exports.push(tref.clone());
            if tref.borrow().name.is_none() {
                tref.borrow_mut().name = Some(name.clone());
                self.dict.add_name(&tref, &name);
            }
        }
        Ok(Tag::new_ref(codes::EXPORT_ASSETS, Body::ExportAssets { exports }))
    }

    fn decode_import_assets(&mut self, r: &mut SwfDecoder, code: i32) -> R<TagRef> {
        let mut ia = ImportAssets { url: Some(r.read_string()?), ..Default::default() };
        if code == codes::IMPORT_ASSETS2 {
            ia.download_now = r.read_ui8() == 1;
            if r.read_ui8() == 1 {
                ia.sha1 = Some(r.read_fully(20)?);
            }
        }
        let count = r.read_ui16();
        for _ in 0..count.max(0) {
            let id = r.read_ui16();
            let rec = Tag::new_ref(codes::IMPORT_ASSETS, Body::ImportRecord);
            let name = r.read_string()?;
            rec.borrow_mut().name = Some(name.clone());
            ia.import_records.push(rec.clone());
            self.add(id, &rec)?;
            self.dict.add_name(&rec, &name);
        }
        Ok(Tag::new_ref(code, Body::ImportAssets(ia)))
    }
}

fn decode_sound_info(r: &mut SwfDecoder) -> SoundInfo {
    let mut i = SoundInfo::default();
    r.sync_bits();
    r.read_ubits(2);
    i.sync_stop = r.read_bit();
    i.sync_no_multiple = r.read_bit();
    let has_envelope = r.read_bit();
    let has_loops = r.read_bit();
    let has_out_point = r.read_bit();
    let has_in_point = r.read_bit();
    if has_in_point {
        i.in_point = r.read_ui32();
    }
    if has_out_point {
        i.out_point = r.read_ui32();
    }
    if has_loops {
        i.loop_count = r.read_ui16();
    }
    if has_envelope {
        let points = r.read_ui8();
        i.records = Some((0..points.max(0)).map(|_| r.read64()).collect());
    }
    i
}

fn decode_button_cond_action(r: &mut SwfDecoder, length: i64) -> R<ButtonCondAction> {
    r.sync_bits();
    let mut a = ButtonCondAction {
        key_press: r.read_ubits(7),
        over_down_to_idle: r.read_bit(),
        idle_to_over_down: r.read_bit(),
        out_down_to_idle: r.read_bit(),
        out_down_to_over_down: r.read_bit(),
        over_down_to_out_down: r.read_bit(),
        over_down_to_over_up: r.read_bit(),
        over_up_to_over_down: r.read_bit(),
        over_up_to_idle: r.read_bit(),
        idle_to_over_up: r.read_bit(),
        ..Default::default()
    };
    a.action_list = Some(decode_actions(r, length - 2)?);
    Ok(a)
}

fn decode_gradient(r: &mut SwfDecoder, shape: i32, filltype: i32) -> Gradient {
    r.sync_bits();
    let mut g = Gradient { spread_mode: r.read_ubits(2), interpolation_mode: r.read_ubits(2), ..Default::default() };
    let count = r.read_ubits(4);
    for _ in 0..count {
        let ratio = r.read_ui8();
        let color = if shape == codes::DEFINE_SHAPE || shape == codes::DEFINE_SHAPE2 { decode_rgb(r) } else { decode_rgba(r) };
        g.records.push(GradRecord { ratio, color });
    }
    if filltype == FILL_FOCAL_RADIAL_GRADIENT {
        g.focal_point = Some(r.read_fixed8());
    }
    g
}

fn decode_filter_list(r: &mut SwfDecoder) -> R<FilterList> {
    let count = r.read_ui8();
    let mut filters = Vec::new();
    let gradient = |r: &mut SwfDecoder| {
        let numcolors = r.read_ui8();
        let gradient_colors = (0..numcolors.max(0)).map(|_| decode_rgba(r)).collect();
        let gradient_ratio = (0..numcolors.max(0)).map(|_| r.read_ui8()).collect();
        GradientFilter {
            numcolors,
            gradient_colors,
            gradient_ratio,
            blur_x: r.read_si32(),
            blur_y: r.read_si32(),
            angle: r.read_si32(),
            distance: r.read_si32(),
            strength: r.read_ui16(),
            flags: r.read_ui8(),
        }
    };
    for _ in 0..count.max(0) {
        let f = match r.read_ui8() {
            0 => Filter::DropShadow {
                color: decode_rgba(r),
                blur_x: r.read_si32(),
                blur_y: r.read_si32(),
                angle: r.read_si32(),
                distance: r.read_si32(),
                strength: r.read_ui16(),
                flags: r.read_ui8(),
            },
            1 => Filter::Blur { blur_x: r.read_si32(), blur_y: r.read_si32(), passes: r.read_ui8() },
            2 => Filter::Glow { color: decode_rgba(r), blur_x: r.read_si32(), blur_y: r.read_si32(), strength: r.read_ui16(), flags: r.read_ui8() },
            3 => {
                let highlight_color = decode_rgba(r);
                let shadow_color = decode_rgba(r);
                Filter::Bevel {
                    shadow_color,
                    highlight_color,
                    blur_x: r.read_si32(),
                    blur_y: r.read_si32(),
                    angle: r.read_si32(),
                    distance: r.read_si32(),
                    strength: r.read_ui16(),
                    flags: r.read_ui8(),
                }
            }
            4 => Filter::GradientGlow(gradient(r)),
            5 => {
                let matrix_x = r.read_ui8();
                let matrix_y = r.read_ui8();
                let divisor = r.read_float();
                let bias = r.read_float();
                let matrix = (0..(matrix_x * matrix_y).max(0)).map(|_| r.read_float()).collect();
                let _color = decode_rgba(r);
                Filter::Convolution { matrix_x, matrix_y, divisor, bias, matrix, flags: r.read_ui8() }
            }
            6 => Filter::ColorMatrix { values: (0..20).map(|_| r.read_float()).collect() },
            7 => Filter::GradientBevel(gradient(r)),
            _ => continue,
        };
        filters.push(Rc::new(f));
    }
    Ok(filters)
}
