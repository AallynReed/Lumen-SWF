//! Port of `flash.swf.TagEncoder`: the tag bodies, written to `tagw` and appended to the movie or sprite `writer`
//! with their headers. Action records are written as the bytes they were read with (see `types::ActionList`).

use super::dictionary::Dictionary;
use super::swf_encoder::{max_num, min_bits, CompressionLevel, SwfEncoder};
use super::tags::{codes::*, BitsKind, Body, Tag, TagRef};
use super::types::*;

pub struct TagEncoder {
    pub writer: SwfEncoder,
    tagw: SwfEncoder,
    frames: i32,
    debug: bool,
    pub dict: Dictionary,
    swf_version: i32,
}

impl TagEncoder {
    /// `TagEncoder()` after `header(h)`; `debug` is whether a `DebugEncoder` exists.
    pub fn new(swf_version: i32, debug: bool) -> TagEncoder {
        TagEncoder { writer: SwfEncoder::new(swf_version), tagw: SwfEncoder::new(swf_version), frames: 0, debug, dict: Dictionary::new(), swf_version }
    }

    pub fn get_compression_level(&self) -> CompressionLevel {
        if self.debug {
            CompressionLevel::BestSpeed
        } else {
            CompressionLevel::BestCompression
        }
    }

    /// The tag stream written so far, taken out of the writer.
    pub fn take_bytes(&mut self) -> Vec<u8> {
        let b = self.writer.to_byte_array();
        self.writer.reset();
        b
    }

    pub fn encode_rect(r: &Rect, w: &mut SwfEncoder) {
        let n = r.nbits();
        w.write_ubits(n, 5);
        w.write_sbits(r.x_min, n);
        w.write_sbits(r.x_max, n);
        w.write_sbits(r.y_min, n);
        w.write_sbits(r.y_max, n);
        w.flush_bits();
    }

    fn encode_tag(&mut self, tag: &Tag) {
        let level = self.get_compression_level();
        self.tagw.compress(level);
        let len = self.tagw.get_pos() as i32;
        let long = self.is_long_header(tag);
        self.encode_tag_header(tag.code, len, long);
        let mut tagw = std::mem::replace(&mut self.tagw, SwfEncoder::new(self.swf_version));
        tagw.write_to(&mut self.writer, level);
        tagw.reset();
        self.tagw = tagw;
    }

    fn is_long_header(&self, t: &Tag) -> bool {
        match t.code {
            DEFINE_BITS | DEFINE_BITS_JPEG2 | DEFINE_BITS_JPEG3 | DEFINE_BITS_LOSSLESS | DEFINE_BITS_LOSSLESS2 => true,
            SOUND_STREAM_BLOCK => true,
            DEFINE_BUTTON | DEFINE_BUTTON2 | DEFINE_SPRITE | DO_INIT_ACTION | DO_ACTION => self.debug,
            PLACE_OBJECT2 => self.debug && matches!(&t.body, Body::PlaceObject(p) if p.has_clip_action()),
            _ => false,
        }
    }

    fn encode_tag_header(&mut self, code: i32, length: i32, long_header: bool) {
        if long_header || length >= 63 {
            self.writer.write_ui16((code << 6) | 63);
            self.writer.write32(length);
        } else {
            self.writer.write_ui16((code << 6) | length);
        }
    }

    fn id_of(&self, t: &Option<TagRef>) -> i32 {
        self.dict.get_id(t.as_ref())
    }

    fn weak_id(&self, t: &Option<super::tags::WeakTag>) -> i32 {
        let t = t.as_ref().and_then(|w| w.upgrade());
        self.dict.get_id(t.as_ref())
    }

    /// `tag.visit(this)`.
    pub fn visit(&mut self, tag_ref: &TagRef) {
        let special = match &tag_ref.borrow().body {
            Body::DefineSprite(_) => 1,
            Body::DefineFont2(_) => 2,
            Body::DefineFontInfo(_) => 3,
            _ => 0,
        };
        match special {
            1 => return self.define_sprite(tag_ref),
            2 => return self.define_font2(tag_ref),
            3 => return self.define_font_info(tag_ref),
            _ => {}
        }
        let tag = tag_ref.borrow();
        match &tag.body {
            Body::DefineScalingGrid(g) => {
                let idref = self.weak_id(&g.scaling_target);
                self.tagw.write_ui16(idref);
                Self::encode_rect(&g.rect, &mut self.tagw);
                self.encode_tag(&tag);
            }
            Body::DefineBinaryData { reserved, data } => {
                self.encode_tag_header(tag.code, 6 + data.len() as i32, false);
                let id = self.dict.add(tag_ref);
                self.writer.write_ui16(id);
                self.writer.write32(*reserved);
                self.writer.write(data);
            }
            Body::DefineBits(b) => match b.kind {
                BitsKind::Bits => {
                    self.encode_tag_header(tag.code, 2 + b.data.len() as i32, true);
                    let id = self.dict.add(tag_ref);
                    self.writer.write_ui16(id);
                    self.writer.write(&b.data);
                }
                BitsKind::Jpeg3 => {
                    let id = self.dict.add(tag_ref);
                    self.tagw.write_ui16(id);
                    self.tagw.write32(b.data.len() as i32);
                    self.tagw.write(&b.data);
                    self.tagw.mark_comp();
                    self.tagw.write(&b.alpha_data);
                    self.encode_tag(&tag);
                }
                BitsKind::Lossless => {
                    let id = self.dict.add(tag_ref);
                    self.tagw.write_ui16(id);
                    self.tagw.write_ui8(b.format);
                    self.tagw.write_ui16(b.width);
                    self.tagw.write_ui16(b.height);
                    let alpha = tag.code == DEFINE_BITS_LOSSLESS2;
                    match b.format {
                        3 => {
                            let colors = b.color_data.as_deref().unwrap_or(&[]);
                            self.tagw.write_ui8(colors.len() as i32 - 1);
                            self.tagw.mark_comp();
                            for &c in colors {
                                if alpha {
                                    Self::encode_rgba(c, &mut self.tagw);
                                } else {
                                    Self::encode_rgb(c, &mut self.tagw);
                                }
                            }
                            self.tagw.write(&b.data);
                        }
                        4 | 5 => {
                            self.tagw.mark_comp();
                            self.tagw.write(&b.data);
                        }
                        _ => {}
                    }
                    self.encode_tag(&tag);
                }
            },
            Body::DefineButton(b) => {
                let id = self.dict.add(tag_ref);
                self.tagw.write_ui16(id);
                if tag.code == DEFINE_BUTTON {
                    for r in &b.button_records {
                        self.encode_button_record(r, tag.code);
                    }
                    self.tagw.write_ui8(0);
                    let first = b.cond_actions.first().expect("ArrayIndexOutOfBoundsException: 0");
                    self.tagw.write(&first.action_list.as_ref().map(|a| a.bytes.clone()).unwrap_or_default());
                    self.tagw.write_ui8(0);
                } else {
                    self.tagw.write_ubits(0, 7);
                    self.tagw.write_bit(b.track_as_menu);
                    let offset_pos = self.tagw.get_pos();
                    self.tagw.write_ui16(0);
                    for r in &b.button_records {
                        self.encode_button_record(r, tag.code);
                    }
                    self.tagw.write_ui8(0);
                    if !b.cond_actions.is_empty() {
                        let p = self.tagw.get_pos();
                        self.tagw.write_ui16_at(offset_pos, (p - offset_pos) as i32);
                        let n = b.cond_actions.len();
                        for (i, c) in b.cond_actions.iter().enumerate() {
                            self.encode_button_cond_action(c, i + 1 == n);
                        }
                    }
                }
                self.encode_tag(&tag);
            }
            Body::DefineButtonCxform(c) => {
                let idref = self.weak_id(&c.button);
                self.tagw.write_ui16(idref);
                Self::encode_cxform(&c.color_transform, &mut self.tagw);
                self.encode_tag(&tag);
            }
            Body::DefineButtonSound(s) => {
                let idref = self.weak_id(&s.button);
                self.tagw.write_ui16(idref);
                for (sound, info) in [(&s.sound0, &s.info0), (&s.sound1, &s.info1), (&s.sound2, &s.info2), (&s.sound3, &s.info3)] {
                    if sound.is_some() {
                        let id = self.id_of(sound);
                        self.tagw.write_ui16(id);
                        Self::encode_sound_info(info.as_ref().unwrap(), &mut self.tagw);
                    } else {
                        self.tagw.write_ui16(0);
                    }
                }
                self.encode_tag(&tag);
            }
            Body::DefineEditText(e) => {
                let id = self.dict.add(tag_ref);
                self.tagw.write_ui16(id);
                Self::encode_rect(&e.bounds, &mut self.tagw);
                let w = &mut self.tagw;
                for b in [e.has_text, e.word_wrap, e.multiline, e.password, e.read_only, e.has_text_color, e.has_max_length, e.has_font] {
                    w.write_bit(b);
                }
                w.write_bit(e.has_font_class && !e.has_font);
                for b in [e.auto_size, e.has_layout, e.no_select, e.border, e.was_static, e.html, e.use_outlines] {
                    w.write_bit(b);
                }
                w.flush_bits();
                if e.has_font {
                    let idref = self.id_of(&e.font);
                    self.tagw.write_ui16(idref);
                    self.tagw.write_ui16(e.height);
                } else if e.has_font_class {
                    self.tagw.write_string(e.font_class.as_deref().unwrap_or("null"));
                    self.tagw.write_ui16(e.height);
                }
                let w = &mut self.tagw;
                if e.has_text_color {
                    Self::encode_rgba(e.color, w);
                }
                if e.has_max_length {
                    w.write_ui16(e.max_length);
                }
                if e.has_layout {
                    w.write_ui8(e.align);
                    w.write_ui16(e.left_margin);
                    w.write_ui16(e.right_margin);
                    w.write_ui16(e.ident);
                    w.write_si16(e.leading);
                }
                w.write_string(e.var_name.as_deref().unwrap_or("null"));
                if e.has_text {
                    w.write_string(e.initial_text.as_deref().unwrap_or("null"));
                }
                self.encode_tag(&tag);
            }
            Body::DefineFont1(f) => {
                let id = self.dict.add(tag_ref);
                self.tagw.write_ui16(id);
                let count = f.glyph_shape_table.len();
                let offset_pos = self.tagw.get_pos();
                for _ in 0..count {
                    self.tagw.write_ui16(0);
                }
                for (i, g) in f.glyph_shape_table.iter().enumerate() {
                    let p = self.tagw.get_pos();
                    self.tagw.write_ui16_at(offset_pos + 2 * i, (p - offset_pos) as i32);
                    self.encode_shape(&g.shape_records, true, DEFINE_SHAPE3, 1, 0);
                }
                self.encode_tag(&tag);
            }
            Body::DefineFont2(_) => unreachable!(),
            Body::DefineFont4(f) => {
                let id = self.dict.add(tag_ref);
                self.tagw.write_ui16(id);
                self.tagw.write_ubits(0, 5);
                self.tagw.write_bit(f.has_font_data);
                self.tagw.write_bit(f.italic);
                self.tagw.write_bit(f.bold);
                self.tagw.flush_bits();
                self.tagw.write_string(f.font_name.as_deref().unwrap_or("null"));
                if f.has_font_data {
                    self.tagw.write(&f.data);
                }
                self.encode_tag(&tag);
            }
            Body::DefineFontAlignZones(z) => {
                let font_id = self.weak_id(&z.font);
                self.tagw.write_ui16(font_id);
                self.tagw.write_ubits(z.csm_table_hint, 2);
                self.tagw.write_ubits(0, 6);
                for record in &z.zone_table {
                    self.tagw.write_ui8(record.num_zone_data);
                    for j in 0..record.num_zone_data as usize {
                        self.tagw.write32(record.zone_data[j] as i32);
                    }
                    self.tagw.write_ui8(record.zone_mask);
                }
                self.encode_tag(&tag);
            }
            Body::CsmTextSettings(c) => {
                let text_id = if c.text_reference.is_some() { self.weak_id(&c.text_reference) } else { 0 };
                self.tagw.write_ui16(text_id);
                self.tagw.write_ubits(c.style_flags_use_saffron, 2);
                self.tagw.write_ubits(c.grid_fit_type, 3);
                self.tagw.write_ubits(0, 3);
                self.tagw.write_ubits(c.thickness as i32, 32);
                self.tagw.write_ubits(c.sharpness as i32, 32);
                self.tagw.write_ubits(0, 8);
                self.encode_tag(&tag);
            }
            Body::DefineFontName(n) => {
                let font_id = self.weak_id(&n.font);
                self.tagw.write_ui16(font_id);
                self.tagw.write_string(n.font_name.as_deref().unwrap_or(""));
                self.tagw.write_string(n.copyright.as_deref().unwrap_or(""));
                self.encode_tag(&tag);
            }
            Body::DefineFontInfo(_) => unreachable!(),
            Body::DefineMorphShape(m) => {
                let id = self.dict.add(tag_ref);
                self.tagw.write_ui16(id);
                Self::encode_rect(&m.start_bounds, &mut self.tagw);
                Self::encode_rect(&m.end_bounds, &mut self.tagw);
                if tag.code == DEFINE_MORPH_SHAPE2 {
                    Self::encode_rect(&m.start_edge_bounds, &mut self.tagw);
                    Self::encode_rect(&m.end_edge_bounds, &mut self.tagw);
                    self.tagw.write_ubits(m.reserved, 6);
                    self.tagw.write_ubits(m.uses_non_scaling_strokes as i32, 1);
                    self.tagw.write_ubits(m.uses_scaling_strokes as i32, 1);
                }
                self.tagw.write32(0);
                let pos = self.tagw.get_pos();
                let count = m.fill_styles.len() as i32;
                if count >= 0xFF {
                    self.tagw.write_ui8(0xFF);
                    self.tagw.write_ui16(count);
                } else {
                    self.tagw.write_ui8(count);
                }
                for style in &m.fill_styles {
                    self.encode_morph_fillstyle(style, tag.code);
                }
                self.encode_morph_linestyles(&m.line_styles, tag.code);
                let start = m.start_edges.as_ref().and_then(|s| s.shape_records.clone());
                self.encode_shape(&start, m.start_edges.is_some(), DEFINE_SHAPE3, m.fill_styles.len() as i32, m.line_styles.len() as i32);
                let p = self.tagw.get_pos();
                self.tagw.write32_at(pos - 4, (p - pos) as i32);
                let end = m.end_edges.as_ref().and_then(|s| s.shape_records.clone());
                self.encode_shape(&end, m.end_edges.is_some(), DEFINE_SHAPE3, 0, 0);
                self.encode_tag(&tag);
            }
            Body::DefineShape(s) => {
                let id = self.dict.add(tag_ref);
                self.tagw.write_ui16(id);
                Self::encode_rect(&s.bounds, &mut self.tagw);
                if tag.code == DEFINE_SHAPE4 {
                    Self::encode_rect(s.edge_bounds.as_ref().expect("NullPointerException"), &mut self.tagw);
                    self.tagw.write_ubits(0, 5);
                    self.tagw.write_bit(s.uses_fill_winding_rule);
                    self.tagw.write_bit(s.uses_non_scaling_strokes);
                    self.tagw.write_bit(s.uses_scaling_strokes);
                }
                let sws = &s.shape_with_style;
                self.encode_fillstyles(&sws.fillstyles, tag.code);
                self.encode_linestyles(&sws.linestyles, tag.code);
                let fill_count = sws.fillstyles.as_ref().map_or(0, |f| f.len()) as i32;
                let line_count = sws.linestyles.as_ref().map_or(0, |l| l.len()) as i32;
                self.encode_shape(&sws.shape_records, true, tag.code, fill_count, line_count);
                self.encode_tag(&tag);
            }
            Body::DefineSound(s) => {
                let id = self.dict.add(tag_ref);
                let w = &mut self.tagw;
                w.write_ui16(id);
                w.write_ubits(s.format, 4);
                w.write_ubits(s.rate, 2);
                w.write_ubits(s.size, 1);
                w.write_ubits(s.type_, 1);
                w.write32(s.sample_count as i32);
                w.write(&s.data);
                self.encode_tag(&tag);
            }
            Body::DefineSprite(_) => unreachable!(),
            Body::DefineText(t) => {
                let id = self.dict.add(tag_ref);
                self.tagw.write_ui16(id);
                Self::encode_rect(&t.bounds, &mut self.tagw);
                Self::encode_matrix(&t.matrix, &mut self.tagw);
                let mut glyph_bits = 0;
                let mut advance_bits = 0;
                for tr in &t.records {
                    for entry in &tr.entries {
                        while entry.get_index() > (1 << glyph_bits) {
                            glyph_bits += 1;
                        }
                        while entry.advance.wrapping_abs() > (1 << advance_bits) {
                            advance_bits += 1;
                        }
                    }
                }
                glyph_bits += 1;
                advance_bits += 1;
                self.tagw.write_ui8(glyph_bits);
                advance_bits += 1;
                self.tagw.write_ui8(advance_bits);
                for record in &t.records {
                    self.encode_text_record(record, tag.code, glyph_bits, advance_bits);
                }
                self.tagw.write_ui8(0);
                self.encode_tag(&tag);
            }
            Body::DefineVideoStream(v) => {
                let id = self.dict.add(tag_ref);
                let w = &mut self.tagw;
                w.write_ui16(id);
                w.write_ui16(v.num_frames);
                w.write_ui16(v.width);
                w.write_ui16(v.height);
                w.write_ubits(0, 4);
                w.write_ubits(v.deblocking, 3);
                w.write_bit(v.smoothing);
                w.write_ui8(v.codec_id);
                self.encode_tag(&tag);
            }
            Body::DoAction { action_list } => {
                self.tagw.write(&action_list.bytes);
                self.tagw.write_ui8(0);
                self.encode_tag(&tag);
            }
            Body::DoInitAction { sprite, action_list } => {
                let idref = self.weak_id(sprite);
                self.tagw.write_ui16(idref);
                self.tagw.write(&action_list.bytes);
                self.tagw.write_ui8(0);
                self.encode_tag(&tag);
            }
            Body::ExportAssets { exports } => {
                self.tagw.write_ui16(exports.len() as i32);
                for r in exports {
                    let idref = self.dict.get_id(Some(r));
                    self.tagw.write_ui16(idref);
                    let name = r.borrow().name.clone().unwrap_or_default();
                    self.tagw.write_string(&name);
                    self.dict.add_name(r, &name);
                }
                self.encode_tag(&tag);
            }
            Body::FrameLabel { label, anchor } => {
                self.tagw.write_string(label);
                if *anchor && self.swf_version >= 6 {
                    self.tagw.write_ui8(1);
                }
                self.encode_tag(&tag);
            }
            Body::ImportAssets(ia) => {
                self.tagw.write_string(ia.url.as_deref().unwrap_or("null"));
                if tag.code == IMPORT_ASSETS2 {
                    self.tagw.write_ui8(ia.download_now as i32);
                    self.tagw.write_ui8(ia.sha1.is_some() as i32);
                    if let Some(sha) = &ia.sha1 {
                        self.tagw.write(sha);
                    }
                }
                self.tagw.write_ui16(ia.import_records.len() as i32);
                for r in &ia.import_records {
                    let id = self.dict.add(r);
                    self.tagw.write_ui16(id);
                    let name = r.borrow().name.clone().unwrap_or_default();
                    self.tagw.write_string(&name);
                }
                self.encode_tag(&tag);
            }
            Body::PlaceObject(p) => {
                if tag.code == PLACE_OBJECT {
                    let idref = self.id_of(&p.ref_);
                    self.tagw.write_ui16(idref);
                    self.tagw.write_ui16(p.depth);
                    Self::encode_matrix(p.matrix.as_ref().expect("NullPointerException"), &mut self.tagw);
                    if let Some(c) = &p.color_transform {
                        Self::encode_cxform(c, &mut self.tagw);
                    }
                } else {
                    self.place_object23(p, tag.code);
                }
                self.encode_tag(&tag);
            }
            Body::RemoveObject { depth, ref_ } => {
                if tag.code == REMOVE_OBJECT {
                    self.encode_tag_header(tag.code, 4, false);
                    let idref = self.id_of(ref_);
                    self.writer.write_ui16(idref);
                    self.writer.write_ui16(*depth);
                } else {
                    self.encode_tag_header(tag.code, 2, false);
                    self.writer.write_ui16(*depth);
                }
            }
            Body::SetBackgroundColor { color } => {
                self.encode_tag_header(tag.code, 3, false);
                Self::encode_rgb(*color, &mut self.writer);
            }
            Body::ShowFrame => {
                self.encode_tag_header(tag.code, 0, false);
                self.frames += 1;
            }
            Body::SoundStreamHead(h) => {
                let mut length = 4;
                if h.compression == 2 {
                    length += 2;
                }
                self.encode_tag_header(tag.code, length, false);
                let w = &mut self.writer;
                w.write_ubits(0, 4);
                w.write_ubits(h.playback_rate, 2);
                w.write_ubits(h.playback_size, 1);
                w.write_ubits(h.playback_type, 1);
                w.write_ubits(h.compression, 4);
                w.write_ubits(h.stream_rate, 2);
                w.write_ubits(h.stream_size, 1);
                w.write_ubits(h.stream_type, 1);
                w.write_ui16(h.stream_sample_count);
                if h.compression == 2 {
                    w.write_si16(h.latency_seek);
                }
            }
            Body::StartSound { sound, sound_info } => {
                let idref = self.id_of(sound);
                self.tagw.write_ui16(idref);
                Self::encode_sound_info(sound_info, &mut self.tagw);
                self.encode_tag(&tag);
            }
            Body::VideoFrame { stream, frame_num, video_data } => {
                self.encode_tag_header(tag.code, 4 + video_data.len() as i32, false);
                let idref = self.id_of(stream);
                self.writer.write_ui16(idref);
                self.writer.write_ui16(*frame_num);
                self.writer.write(video_data);
            }
            Body::SetTabIndex { depth, index } => {
                self.tagw.write_ui16(*depth);
                self.tagw.write_ui16(*index);
                self.encode_tag(&tag);
            }
            Body::Generic { data } => {
                self.encode_tag_header(tag.code, data.len() as i32, false);
                self.writer.write(data);
            }
            Body::ImportRecord => {}
        }
    }

    /// `symbolClass(tag)`, from the class map in its iteration order.
    pub fn symbol_class(&mut self, class2tag: &[(String, TagRef)], top_level_class: Option<&str>) {
        self.tagw.write_ui16(class2tag.len() as i32 + top_level_class.is_some() as i32);
        for (name, r) in class2tag {
            let idref = self.dict.get_id(Some(r));
            self.tagw.write_ui16(idref);
            self.tagw.write_string(name);
        }
        if let Some(top) = top_level_class {
            self.tagw.write_ui16(0);
            self.tagw.write_string(top);
        }
        let tag = Tag::new(SYMBOL_CLASS, Body::ShowFrame);
        self.encode_tag(&tag);
    }

    fn define_sprite(&mut self, tag_ref: &TagRef) {
        let id = self.dict.add(tag_ref);
        let (framecount, children) = {
            let t = tag_ref.borrow();
            let s = t.sprite().unwrap();
            (s.framecount, s.tag_list.clone())
        };
        self.tagw.write_ui16(id);
        self.tagw.write_ui16(framecount);
        let old_frames = self.frames;
        self.frames = 0;
        let inner = std::mem::replace(&mut self.tagw, SwfEncoder::new(self.swf_version));
        let old_writer = std::mem::replace(&mut self.writer, inner);
        for t in &children {
            if !t.borrow().is_define_tag() {
                self.visit(t);
            }
        }
        self.writer.write_ui16(0);
        let frames = self.frames;
        self.writer.write_ui16_at(2, frames);
        self.tagw = std::mem::replace(&mut self.writer, old_writer);
        self.frames = old_frames;
        let t = tag_ref.borrow();
        self.encode_tag(&t);
    }

    fn define_font2(&mut self, tag_ref: &TagRef) {
        let id = self.dict.add(tag_ref);
        self.tagw.write_ui16(id);
        let start_pos = self.tagw.get_pos();
        let mut t = tag_ref.borrow_mut();
        let code = t.code;
        let Body::DefineFont2(f) = &mut t.body else { unreachable!() };
        if code == DEFINE_FONT3 {
            f.wide_codes = true;
        }
        if !f.wide_codes && f.code_table.iter().any(|&c| c > 255) {
            f.wide_codes = true;
        }
        'again: loop {
            let w = &mut self.tagw;
            for b in [f.has_layout, f.shift_jis, f.small_text, f.ansi, f.wide_offsets, f.wide_codes, f.italic, f.bold] {
                w.write_bit(b);
            }
            w.flush_bits();
            w.write_ui8(f.lang_code);
            w.write_length_string(f.font_name.as_deref().unwrap_or("null"));
            let count = f.glyph_shape_table.len();
            w.write_ui16(count as i32);
            let offset_pos = w.get_pos();
            for _ in 0..count {
                if f.wide_offsets {
                    w.write32(0);
                } else {
                    w.write_ui16(0);
                }
            }
            if count > 0 {
                if f.wide_offsets {
                    w.write32(0);
                } else {
                    w.write_ui16(0);
                }
            }
            for i in 0..count {
                let offset = self.tagw.get_pos() - offset_pos;
                if !f.wide_offsets && offset > 65535 {
                    f.wide_offsets = true;
                    self.tagw.set_pos(start_pos);
                    continue 'again;
                }
                if f.wide_offsets {
                    self.tagw.write32_at(offset_pos + 4 * i, offset as i32);
                } else {
                    self.tagw.write_ui16_at(offset_pos + 2 * i, offset as i32);
                }
                let recs = f.glyph_shape_table[i].shape_records.clone();
                self.encode_shape(&recs, true, DEFINE_SHAPE3, 1, 0);
            }
            let offset = self.tagw.get_pos() - offset_pos;
            if !f.wide_offsets && offset > 65535 {
                f.wide_offsets = true;
                self.tagw.set_pos(start_pos);
                continue 'again;
            }
            if f.wide_offsets {
                self.tagw.write32_at(offset_pos + 4 * count, offset as i32);
            } else {
                self.tagw.write_ui16_at(offset_pos + 2 * count, offset as i32);
            }
            let w = &mut self.tagw;
            for &c in &f.code_table {
                if f.wide_codes {
                    w.write_ui16(c as i32);
                } else {
                    w.write_ui8(c as i32);
                }
            }
            if f.has_layout {
                w.write_si16(f.ascent);
                w.write_si16(f.descent);
                w.write_si16(f.leading);
                for &a in &f.advance_table {
                    w.write_si16(a as i32);
                }
                for r in &f.bounds_table {
                    Self::encode_rect(r, w);
                }
                w.write_ui16(f.kerning_table.len() as i32);
                for k in f.kerning_table.clone() {
                    if !f.wide_codes && (k.code1 > 255 || k.code2 > 255) {
                        f.wide_codes = true;
                        self.tagw.set_pos(start_pos);
                        continue 'again;
                    }
                    let w = &mut self.tagw;
                    if f.wide_codes {
                        w.write_ui16(k.code1);
                        w.write_ui16(k.code2);
                    } else {
                        w.write_ui8(k.code1);
                        w.write_ui8(k.code2);
                    }
                    w.write_ui16(k.adjustment);
                }
            }
            break;
        }
        drop(t);
        let t = tag_ref.borrow();
        self.encode_tag(&t);
    }

    fn define_font_info(&mut self, tag_ref: &TagRef) {
        let mut t = tag_ref.borrow_mut();
        let code = t.code;
        let Body::DefineFontInfo(info) = &mut t.body else { unreachable!() };
        let idref = self.weak_id(&info.font);
        let w = &mut self.tagw;
        w.write_ui16(idref);
        w.write_length_string(info.name.as_deref().unwrap_or("null"));
        w.write_ubits(0, 3);
        w.write_bit(info.shift_jis);
        w.write_bit(info.ansi);
        w.write_bit(info.italic);
        w.write_bit(info.bold);
        if code == DEFINE_FONT_INFO2 {
            info.wide_codes = true;
            w.write_bit(true);
            w.write_ui8(info.lang_code);
        } else {
            if !info.wide_codes && info.code_table.iter().any(|&c| c > 255) {
                info.wide_codes = true;
            }
            w.write_bit(info.wide_codes);
        }
        for &c in &info.code_table {
            if info.wide_codes {
                w.write_ui16(c as i32);
            } else {
                w.write_ui8(c as i32);
            }
        }
        drop(t);
        let t = tag_ref.borrow();
        self.encode_tag(&t);
    }

    fn place_object23(&mut self, p: &super::tags::PlaceObject, code: i32) {
        self.tagw.write_ui8(p.flags);
        if code == PLACE_OBJECT3 {
            self.tagw.write_ui8(p.flags2);
        }
        self.tagw.write_ui16(p.depth);
        if p.has_class_name() {
            self.tagw.write_string(p.class_name.as_deref().unwrap_or("null"));
        }
        if p.has_char_id() {
            let idref = self.id_of(&p.ref_);
            self.tagw.write_ui16(idref);
        }
        if p.has_matrix() {
            Self::encode_matrix(p.matrix.as_ref().expect("NullPointerException"), &mut self.tagw);
        }
        if p.has_cxform() {
            Self::encode_cxforma(p.color_transform.as_ref().expect("NullPointerException"), &mut self.tagw);
        }
        if p.has_ratio() {
            self.tagw.write_ui16(p.ratio);
        }
        if p.has_name() {
            self.tagw.write_string(p.name.as_deref().unwrap_or("null"));
        }
        if p.has_clip_depth() {
            self.tagw.write_ui16(p.clip_depth);
        }
        if code == PLACE_OBJECT3 {
            if p.has_filter_list() {
                Self::encode_filter_list(p.filters.as_deref().unwrap_or(&[]), &mut self.tagw);
            }
            if p.has_blend_mode() {
                self.tagw.write_ui8(p.blend_mode);
            }
        }
        if p.has_clip_action() {
            if let Some(c) = &p.clip_actions {
                self.tagw.write(&c.bytes);
            }
        }
    }

    fn encode_button_record(&mut self, record: &ButtonRecord, define_button: i32) {
        let id = self.id_of(&record.character_ref);
        let w = &mut self.tagw;
        if define_button == DEFINE_BUTTON2 {
            w.write_ubits(0, 2);
            w.write_bit(record.blend_mode != -1);
            w.write_bit(record.filters.is_some());
        } else {
            w.write_ubits(0, 4);
        }
        w.write_bit(record.hit_test);
        w.write_bit(record.down);
        w.write_bit(record.over);
        w.write_bit(record.up);
        w.write_ui16(id);
        w.write_ui16(record.place_depth);
        Self::encode_matrix(record.place_matrix.as_ref().expect("NullPointerException"), w);
        if define_button == DEFINE_BUTTON2 {
            Self::encode_cxforma(record.color_transform.as_ref().expect("NullPointerException"), w);
            if let Some(f) = &record.filters {
                Self::encode_filter_list(f, w);
            }
            if record.blend_mode != -1 {
                w.write_ui8(record.blend_mode);
            }
        }
    }

    fn encode_button_cond_action(&mut self, c: &ButtonCondAction, last: bool) {
        let w = &mut self.tagw;
        let pos = w.get_pos();
        w.write_ui16(0);
        w.write_ubits(c.key_press, 7);
        for b in [
            c.over_down_to_idle,
            c.idle_to_over_down,
            c.out_down_to_idle,
            c.out_down_to_over_down,
            c.over_down_to_out_down,
            c.over_down_to_over_up,
            c.over_up_to_over_down,
            c.over_up_to_idle,
            c.idle_to_over_up,
        ] {
            w.write_bit(b);
        }
        if let Some(a) = &c.action_list {
            w.write(&a.bytes);
        }
        w.write_ui8(0);
        if !last {
            let p = w.get_pos();
            w.write_ui16_at(pos, (p - pos) as i32);
        }
    }

    pub fn encode_cxforma(c: &CXForm, w: &mut SwfEncoder) {
        w.write_bit(c.has_add);
        w.write_bit(c.has_mult);
        let nbits = c.nbits();
        w.write_ubits(nbits, 4);
        if c.has_mult {
            w.write_sbits(c.red_mult_term, nbits);
            w.write_sbits(c.green_mult_term, nbits);
            w.write_sbits(c.blue_mult_term, nbits);
            w.write_sbits(c.alpha_mult_term, nbits);
        }
        if c.has_add {
            w.write_sbits(c.red_add_term, nbits);
            w.write_sbits(c.green_add_term, nbits);
            w.write_sbits(c.blue_add_term, nbits);
            w.write_sbits(c.alpha_add_term, nbits);
        }
        w.flush_bits();
    }

    pub fn encode_cxform(c: &CXForm, w: &mut SwfEncoder) {
        w.write_bit(c.has_add);
        w.write_bit(c.has_mult);
        let nbits = c.nbits();
        w.write_ubits(nbits, 4);
        if c.has_mult {
            w.write_sbits(c.red_mult_term, nbits);
            w.write_sbits(c.green_mult_term, nbits);
            w.write_sbits(c.blue_mult_term, nbits);
        }
        if c.has_add {
            w.write_sbits(c.red_add_term, nbits);
            w.write_sbits(c.green_add_term, nbits);
            w.write_sbits(c.blue_add_term, nbits);
        }
        w.flush_bits();
    }

    pub fn encode_matrix(m: &Matrix, w: &mut SwfEncoder) {
        w.write_bit(m.has_scale);
        if m.has_scale {
            let n = m.n_scale_bits();
            w.write_ubits(n, 5);
            w.write_sbits(m.scale_x, n);
            w.write_sbits(m.scale_y, n);
        }
        w.write_bit(m.has_rotate);
        if m.has_rotate {
            let n = m.n_rotate_bits();
            w.write_ubits(n, 5);
            w.write_sbits(m.rotate_skew0, n);
            w.write_sbits(m.rotate_skew1, n);
        }
        let n = m.n_translate_bits();
        w.write_ubits(n, 5);
        w.write_sbits(m.translate_x, n);
        w.write_sbits(m.translate_y, n);
        w.flush_bits();
    }

    fn encode_sound_info(info: &SoundInfo, w: &mut SwfEncoder) {
        w.write_ubits(0, 2);
        w.write_bit(info.sync_stop);
        w.write_bit(info.sync_no_multiple);
        w.write_bit(info.records.is_some());
        w.write_bit(info.loop_count as i64 != SOUND_INFO_UNINITIALIZED);
        w.write_bit(info.out_point != SOUND_INFO_UNINITIALIZED);
        w.write_bit(info.in_point != SOUND_INFO_UNINITIALIZED);
        if info.in_point != SOUND_INFO_UNINITIALIZED {
            w.write32(info.in_point as i32);
        }
        if info.out_point != SOUND_INFO_UNINITIALIZED {
            w.write32(info.out_point as i32);
        }
        if info.loop_count as i64 != SOUND_INFO_UNINITIALIZED {
            w.write_ui16(info.loop_count);
        }
        if let Some(records) = &info.records {
            w.write_ui8(records.len() as i32);
            for &r in records {
                w.write64(r);
            }
        }
    }

    fn encode_rgb(rgb: i32, w: &mut SwfEncoder) {
        w.write_ui8(((rgb as u32) >> 16) as i32);
        w.write_ui8((((rgb as u32) >> 8) & 255) as i32);
        w.write_ui8(rgb & 255);
    }

    fn encode_rgba(rgba: i32, w: &mut SwfEncoder) {
        let v = rgba as u32;
        w.write_ui8(((v >> 16) & 255) as i32);
        w.write_ui8(((v >> 8) & 255) as i32);
        w.write_ui8((v & 255) as i32);
        w.write_ui8((v >> 24) as i32);
    }

    /// `encodeShape(s, w, shape, nFillStyles, nLineStyles)`; `present` is `s != null`.
    pub fn encode_shape(&mut self, records: &Option<Vec<ShapeRecord>>, present: bool, shape: i32, n_fill_styles: i32, n_line_styles: i32) {
        let mut num_fill_bits = min_bits(n_fill_styles, 0);
        let mut num_line_bits = min_bits(n_line_styles, 0);
        self.tagw.write_ubits(num_fill_bits, 4);
        self.tagw.write_ubits(num_line_bits, 4);
        if present {
            for record in records.iter().flatten() {
                match record {
                    ShapeRecord::StyleChange(s) => {
                        self.tagw.write_bit(false);
                        self.encode_style_change_record(s, &mut num_fill_bits, &mut num_line_bits, shape);
                    }
                    ShapeRecord::StraightEdge { delta_x, delta_y } => {
                        let w = &mut self.tagw;
                        w.write_bit(true);
                        w.write_bit(true);
                        let nbits = min_bits(max_num(*delta_x, *delta_y, 0, 0), 1).max(2);
                        w.write_ubits(nbits - 2, 4);
                        if *delta_x == 0 {
                            w.write_ubits(1, 2);
                            w.write_sbits(*delta_y, nbits);
                        } else if *delta_y == 0 {
                            w.write_ubits(0, 2);
                            w.write_sbits(*delta_x, nbits);
                        } else {
                            w.write_bit(true);
                            w.write_sbits(*delta_x, nbits);
                            w.write_sbits(*delta_y, nbits);
                        }
                    }
                    ShapeRecord::CurvedEdge { control_delta_x, control_delta_y, anchor_delta_x, anchor_delta_y } => {
                        let w = &mut self.tagw;
                        w.write_bit(true);
                        w.write_bit(false);
                        let nbits = min_bits(max_num(*control_delta_x, *control_delta_y, *anchor_delta_x, *anchor_delta_y), 1).max(2);
                        w.write_ubits(nbits - 2, 4);
                        w.write_sbits(*control_delta_x, nbits);
                        w.write_sbits(*control_delta_y, nbits);
                        w.write_sbits(*anchor_delta_x, nbits);
                        w.write_sbits(*anchor_delta_y, nbits);
                    }
                }
            }
        }
        self.tagw.write_ubits(0, 6);
        self.tagw.flush_bits();
    }

    fn encode_style_change_record(&mut self, s: &StyleChangeRecord, num_fill_bits: &mut i32, num_line_bits: &mut i32, shape: i32) {
        let w = &mut self.tagw;
        w.write_bit(s.state_new_styles);
        w.write_bit(s.state_line_style);
        w.write_bit(s.state_fill_style1);
        w.write_bit(s.state_fill_style0);
        w.write_bit(s.state_move_to);
        if s.state_move_to {
            let move_bits = s.n_move_bits();
            w.write_ubits(move_bits, 5);
            w.write_sbits(s.move_delta_x, move_bits);
            w.write_sbits(s.move_delta_y, move_bits);
        }
        if s.state_fill_style0 {
            w.write_ubits(s.fillstyle0, *num_fill_bits);
        }
        if s.state_fill_style1 {
            w.write_ubits(s.fillstyle1, *num_fill_bits);
        }
        if s.state_line_style {
            w.write_ubits(s.linestyle, *num_line_bits);
        }
        if s.state_new_styles {
            w.flush_bits();
            self.encode_fillstyles(&s.fillstyles, shape);
            self.encode_linestyles(&s.linestyles, shape);
            *num_fill_bits = min_bits(s.fillstyles.as_ref().expect("NullPointerException").len() as i32, 0);
            *num_line_bits = min_bits(s.linestyles.as_ref().expect("NullPointerException").len() as i32, 0);
            self.tagw.write_ubits(*num_fill_bits, 4);
            self.tagw.write_ubits(*num_line_bits, 4);
        }
    }

    fn encode_linestyles(&mut self, linestyles: &Option<Vec<LineStyle>>, shape: i32) {
        let count = linestyles.as_ref().map_or(0, |l| l.len()) as i32;
        if count > 0xFF {
            self.tagw.write_ui8(0xFF);
            self.tagw.write_ui16(count);
        } else {
            self.tagw.write_ui8(count);
        }
        for ls in linestyles.iter().flatten() {
            self.tagw.write_ui16(ls.width);
            if shape == DEFINE_SHAPE4 {
                self.tagw.write_ui16(ls.flags);
                if ls.has_miter_joint() {
                    self.tagw.write_ui16(ls.miter_limit);
                }
            }
            if shape == DEFINE_SHAPE4 && ls.has_fill_style() {
                self.encode_fill_style(ls.fill_style.as_ref().expect("NullPointerException"), shape);
            } else if shape == DEFINE_SHAPE3 || shape == DEFINE_SHAPE4 {
                Self::encode_rgba(ls.color, &mut self.tagw);
            } else {
                Self::encode_rgb(ls.color, &mut self.tagw);
            }
        }
    }

    fn encode_fillstyles(&mut self, fillstyles: &Option<Vec<FillStyle>>, shape: i32) {
        let count = fillstyles.as_ref().map_or(0, |f| f.len()) as i32;
        if count >= 0xFF {
            self.tagw.write_ui8(0xFF);
            self.tagw.write_ui16(count);
        } else {
            self.tagw.write_ui8(count);
        }
        for style in fillstyles.iter().flatten() {
            self.encode_fill_style(style, shape);
        }
    }

    fn encode_fill_style(&mut self, style: &FillStyle, shape: i32) {
        self.tagw.write_ui8(style.type_);
        match style.type_ {
            FILL_SOLID => {
                if shape == DEFINE_SHAPE3 || shape == DEFINE_SHAPE4 {
                    Self::encode_rgba(style.color, &mut self.tagw);
                } else {
                    Self::encode_rgb(style.color, &mut self.tagw);
                }
            }
            FILL_GRADIENT | FILL_RADIAL_GRADIENT | FILL_FOCAL_RADIAL_GRADIENT => {
                Self::encode_matrix(style.matrix.as_ref().expect("NullPointerException"), &mut self.tagw);
                let g = style.gradient.as_ref().expect("NullPointerException");
                let w = &mut self.tagw;
                w.write_ubits(g.spread_mode, 2);
                w.write_ubits(g.interpolation_mode, 2);
                w.write_ubits(g.records.len() as i32, 4);
                for r in &g.records {
                    w.write_ui8(r.ratio);
                    if shape == DEFINE_SHAPE3 || shape == DEFINE_SHAPE4 {
                        Self::encode_rgba(r.color, w);
                    } else {
                        Self::encode_rgb(r.color, w);
                    }
                }
                if let Some(f) = g.focal_point {
                    w.write_fixed8(f);
                }
            }
            0x40..=0x43 => {
                let id = self.dict.add(style.bitmap.as_ref().expect("NullPointerException"));
                self.tagw.write_ui16(id);
                Self::encode_matrix(style.matrix.as_ref().expect("NullPointerException"), &mut self.tagw);
            }
            _ => {}
        }
    }

    fn encode_morph_fillstyle(&mut self, style: &MorphFillStyle, code: i32) {
        self.tagw.write_ui8(style.type_);
        match style.type_ {
            FILL_SOLID => {
                Self::encode_rgba(style.start_color, &mut self.tagw);
                Self::encode_rgba(style.end_color, &mut self.tagw);
            }
            FILL_GRADIENT | FILL_RADIAL_GRADIENT | FILL_FOCAL_RADIAL_GRADIENT => {
                let w = &mut self.tagw;
                Self::encode_matrix(style.start_gradient_matrix.as_ref().expect("NullPointerException"), w);
                Self::encode_matrix(style.end_gradient_matrix.as_ref().expect("NullPointerException"), w);
                let recs = style.grad_records.as_deref().unwrap_or(&[]);
                w.write_ui8(recs.len() as i32);
                for r in recs {
                    w.write_ui8(r.start_ratio);
                    Self::encode_rgba(r.start_color, w);
                    w.write_ui8(r.end_ratio);
                    Self::encode_rgba(r.end_color, w);
                }
                if style.type_ == FILL_FOCAL_RADIAL_GRADIENT && code == DEFINE_MORPH_SHAPE2 {
                    w.write_si16(style.ratio1);
                    w.write_si16(style.ratio2);
                }
            }
            0x40..=0x43 => {
                let id = self.dict.add(style.bitmap.as_ref().expect("NullPointerException"));
                self.tagw.write_ui16(id);
                Self::encode_matrix(style.start_bitmap_matrix.as_ref().expect("NullPointerException"), &mut self.tagw);
                Self::encode_matrix(style.end_bitmap_matrix.as_ref().expect("NullPointerException"), &mut self.tagw);
            }
            _ => {}
        }
    }

    fn encode_morph_linestyles(&mut self, line_styles: &[MorphLineStyle], code: i32) {
        let n = line_styles.len() as i32;
        if n >= 0xFF {
            self.tagw.write_ui8(0xFF);
            self.tagw.write_ui16(n);
        } else {
            self.tagw.write_ui8(n);
        }
        for style in line_styles {
            let w = &mut self.tagw;
            w.write_ui16(style.start_width);
            w.write_ui16(style.end_width);
            if code == DEFINE_MORPH_SHAPE2 {
                w.write_ubits(style.start_caps_style, 2);
                w.write_ubits(style.joint_style, 2);
                w.write_bit(style.has_fill);
                w.write_bit(style.no_h_scale);
                w.write_bit(style.no_v_scale);
                w.write_bit(style.pixel_hinting);
                w.write_ubits(0, 5);
                w.write_bit(style.no_close);
                w.write_ubits(style.end_caps_style, 2);
                if style.joint_style == 2 {
                    w.write_ui16(style.miter_limit);
                }
            }
            if !style.has_fill {
                Self::encode_rgba(style.start_color, w);
                Self::encode_rgba(style.end_color, w);
            }
            if style.has_fill {
                let ft = style.fill_type.clone().expect("NullPointerException");
                self.encode_morph_fillstyle(&ft, code);
            }
        }
    }

    fn encode_text_record(&mut self, record: &TextRecord, type_: i32, glyph_bits: i32, advance_bits: i32) {
        let font_id = self.id_of(&record.font);
        let w = &mut self.tagw;
        w.write_ui8(record.flags);
        if record.has_font() {
            w.write_ui16(font_id);
        }
        if record.has_color() {
            if type_ == DEFINE_TEXT2 {
                Self::encode_rgba(record.color, w);
            } else {
                Self::encode_rgb(record.color, w);
            }
        }
        if record.has_x() {
            w.write_si16(record.x_offset);
        }
        if record.has_y() {
            w.write_si16(record.y_offset);
        }
        if record.has_height() {
            w.write_ui16(record.height);
        }
        w.write_ui8(record.entries.len() as i32);
        for e in &record.entries {
            w.write_ubits(e.get_index(), glyph_bits);
            w.write_sbits(e.advance, advance_bits);
        }
        w.flush_bits();
    }

    fn encode_filter_list(filters: &[std::rc::Rc<Filter>], w: &mut SwfEncoder) {
        w.write_ui8(filters.len() as i32);
        for f in filters {
            w.write_ui8(f.get_id());
            match f.as_ref() {
                Filter::DropShadow { color, blur_x, blur_y, angle, distance, strength, flags } => {
                    Self::encode_rgba(*color, w);
                    w.write32(*blur_x);
                    w.write32(*blur_y);
                    w.write32(*angle);
                    w.write32(*distance);
                    w.write_ui16(*strength);
                    w.write_ui8(*flags);
                }
                Filter::Blur { blur_x, blur_y, passes } => {
                    w.write32(*blur_x);
                    w.write32(*blur_y);
                    w.write_ui8(*passes);
                }
                Filter::Convolution { matrix_x, matrix_y, divisor, bias, matrix, flags } => {
                    w.write_ui8(*matrix_x);
                    w.write_ui8(*matrix_y);
                    w.write_float(*divisor);
                    w.write_float(*bias);
                    for m in matrix {
                        w.write_float(*m);
                    }
                    w.write_ui8(*flags);
                }
                Filter::Glow { color, blur_x, blur_y, strength, flags } => {
                    Self::encode_rgba(*color, w);
                    w.write32(*blur_x);
                    w.write32(*blur_y);
                    w.write_ui16(*strength);
                    w.write_ui8(*flags);
                }
                Filter::Bevel { shadow_color, highlight_color, blur_x, blur_y, angle, distance, strength, flags } => {
                    Self::encode_rgba(*highlight_color, w);
                    Self::encode_rgba(*shadow_color, w);
                    w.write32(*blur_x);
                    w.write32(*blur_y);
                    w.write32(*angle);
                    w.write32(*distance);
                    w.write_ui16(*strength);
                    w.write_ui8(*flags);
                }
                Filter::ColorMatrix { values } => {
                    for i in 0..20 {
                        w.write_float(values.get(i).copied().unwrap_or(0.0));
                    }
                }
                Filter::GradientGlow(g) | Filter::GradientBevel(g) => {
                    w.write_ui8(g.numcolors);
                    for i in 0..g.numcolors as usize {
                        Self::encode_rgba(g.gradient_colors[i], w);
                    }
                    for i in 0..g.numcolors as usize {
                        w.write_ui8(g.gradient_ratio[i]);
                    }
                    w.write32(g.blur_x);
                    w.write32(g.blur_y);
                    w.write32(g.angle);
                    w.write32(g.distance);
                    w.write_ui16(g.strength);
                    w.write_ui8(g.flags);
                }
            }
        }
    }
}
