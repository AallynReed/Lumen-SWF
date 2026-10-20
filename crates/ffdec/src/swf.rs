use crate::container::{self, Compression, EncodeOptions, Header};
use crate::error::Result;
use crate::io::{Reader, Writer};
use crate::tags::{code, Tag, Timeline};
use crate::types::Rect;

pub struct Swf {
    pub header: Header,
    pub frame_rect: Rect,
    pub frame_rate: f32,
    pub frame_count: u16,
    /// Raw RECT/rate/count bytes, rewritten as-is while the values are unchanged.
    movie_header_raw: Vec<u8>,
    original_rect: Rect,
    pub timeline: Timeline,
    pub file_trailer: Vec<u8>,
    pub warnings: Vec<String>,
    original_body_len: usize,
}

pub struct SaveOptions {
    pub compression: Compression,
    pub encrypt: bool,
}

impl Swf {
    pub fn parse(data: &[u8]) -> Result<Swf> {
        let decoded = container::decode(data)?;
        let mut warnings = decoded.warnings;
        let body = decoded.body;
        let mut r = Reader::new(&body);
        let frame_rect = Rect::read(&mut r)?;
        let frame_rate = r.fixed8()?;
        let frame_count = r.u16()?;
        let header_end = r.pos();
        let timeline = Timeline::parse(&mut r, 8 + header_end, 0);
        if !timeline.has_end {
            warnings.push("tag stream has no End tag".into());
        }
        for t in &timeline.tags {
            if let Some(e) = &t.error {
                warnings.push(format!("{} at 0x{:x}: {e}", t.name(), t.offset));
            }
        }
        Ok(Swf {
            header: decoded.header,
            frame_rect,
            frame_rate,
            frame_count,
            movie_header_raw: body[..header_end].to_vec(),
            original_rect: frame_rect,
            timeline,
            file_trailer: decoded.trailer,
            warnings,
            original_body_len: body.len(),
        })
    }

    pub fn is_as3(&self) -> bool {
        self.file_attributes().map(|f| f & 0x08 != 0).unwrap_or_else(|| {
            self.timeline.tags.iter().any(|t| t.code == code::DO_ABC || t.code == code::DO_ABC2)
        })
    }

    pub fn file_attributes(&self) -> Option<u32> {
        let t = self.timeline.tags.iter().find(|t| t.code == code::FILE_ATTRIBUTES)?;
        Reader::new(&t.body).u32().ok()
    }

    pub fn body_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        if self.frame_rect == self.original_rect && self.movie_header_unchanged() {
            w.bytes(&self.movie_header_raw);
        } else {
            self.frame_rect.write(&mut w);
            w.i16((self.frame_rate * 256.0).round() as i16);
            w.u16(self.frame_count);
        }
        self.timeline.write(&mut w);
        w.data
    }

    fn movie_header_unchanged(&self) -> bool {
        let mut r = Reader::new(&self.movie_header_raw);
        let _ = Rect::read(&mut r);
        r.fixed8().ok() == Some(self.frame_rate) && r.u16().ok() == Some(self.frame_count)
    }

    /// The decompressed file: an `FWS`/`GFX` header followed by the body.
    pub fn uncompressed_bytes(&self) -> Result<Vec<u8>> {
        self.save(&SaveOptions { compression: Compression::None, encrypt: false })
    }

    pub fn save(&self, opts: &SaveOptions) -> Result<Vec<u8>> {
        let body = self.body_bytes();
        let file_size = if body.len() == self.original_body_len {
            self.header.file_size
        } else {
            (body.len() + 8) as u32
        };
        let trailer = if opts.compression == Compression::None { &self.file_trailer[..] } else { &[] };
        container::encode(
            &body,
            trailer,
            &EncodeOptions {
                compression: opts.compression,
                gfx: self.header.gfx,
                encrypt: opts.encrypt,
                version: self.header.version,
                file_size,
                lzma_props: self.header.lzma_props,
            },
        )
    }

    /// Resolves a tag path: indices from the root timeline down through sprites.
    pub fn tag(&self, path: &[usize]) -> Option<&Tag> {
        let (first, rest) = path.split_first()?;
        let mut tag = self.timeline.tags.get(*first)?;
        for &i in rest {
            tag = tag.sprite.as_ref()?.tags.get(i)?;
        }
        Some(tag)
    }

    pub fn tag_mut(&mut self, path: &[usize]) -> Option<&mut Tag> {
        self.timeline.tag_mut(path)
    }

    pub fn replace_tag(&mut self, path: &[usize], tag: Tag) -> Result<()> {
        self.timeline.replace_tag(path, tag)
    }

    pub fn remove_tag(&mut self, path: &[usize]) -> Result<Tag> {
        self.timeline.remove_tag(path)
    }

    pub fn insert_tag(&mut self, path: &[usize], tag: Tag) -> Result<()> {
        self.timeline.insert_tag(path, tag)
    }

    pub fn replace_tag_body(&mut self, path: &[usize], new_body: &[u8]) -> Result<()> {
        let tag = self.tag_mut(path).ok_or_else(|| crate::error::Error::Other("tag not found".into()))?;
        tag.body = new_body.to_vec();
        if tag.code == code::DEFINE_SPRITE {
            let mut sr = Reader::new(&tag.body);
            if sr.skip(4).is_ok() {
                tag.sprite = Some(Timeline::parse(&mut sr, tag.offset + tag.header_len() + 4, 1));
            }
        }
        if path.len() > 1 {
            self.timeline.sync_sprites();
        }
        Ok(())
    }

    pub fn sync_sprites(&mut self) {
        self.timeline.sync_sprites();
    }

    pub fn replace_as2_script(&mut self, path: &[usize], actions: &crate::action::ActionList) -> Result<()> {
        let tag = self.tag(path).ok_or_else(|| crate::error::Error::Other("tag not found".into()))?;
        let bytes = actions.to_bytes();
        let new_body = match tag.code {
            code::DO_ACTION => bytes,
            code::DO_INIT_ACTION => {
                let char_id = tag.character_id().unwrap_or(0);
                let mut b = char_id.to_le_bytes().to_vec();
                b.extend_from_slice(&bytes);
                b
            }
            _ => bytes,
        };
        self.replace_tag_body(path, &new_body)
    }

    pub fn find_character(&self, id: u16) -> Option<Vec<usize>> {
        let mut found = None;
        self.walk(|path, tag| {
            if found.is_none() && crate::tags::defines_character(tag.code) && tag.character_id() == Some(id) {
                found = Some(path.to_vec());
            }
        });
        found
    }

    pub fn walk<'a>(&'a self, mut f: impl FnMut(&[usize], &'a Tag)) {
        fn go<'a>(tl: &'a Timeline, path: &mut Vec<usize>, f: &mut dyn FnMut(&[usize], &'a Tag)) {
            for (i, t) in tl.tags.iter().enumerate() {
                path.push(i);
                f(path, t);
                if let Some(s) = &t.sprite {
                    go(s, path, f);
                }
                path.pop();
            }
        }
        go(&self.timeline, &mut Vec::new(), &mut f);
    }
}
