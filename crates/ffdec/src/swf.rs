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

    /// Replaces a tag's body, re-reading it if it is a sprite and rewriting the sprites that
    /// contain it.
    pub fn set_tag_body(&mut self, path: &[usize], body: Vec<u8>) -> Result<()> {
        let code = self.tag(path).map_or(0, |t| t.code);
        self.set_tag(path, code, body)
    }

    /// `set_tag_body` that may also change the tag's type.
    pub fn set_tag(&mut self, path: &[usize], code: u16, body: Vec<u8>) -> Result<()> {
        fn go(tl: &mut Timeline, path: &[usize], code: u16, body: Vec<u8>) -> Option<()> {
            let (first, rest) = path.split_first()?;
            let tag = tl.tags.get_mut(*first)?;
            if rest.is_empty() {
                tag.code = code;
                tag.body = body;
                tag.declared_len = None;
                tag.error = None;
                if tag.code == code::DEFINE_SPRITE {
                    let mut r = Reader::new(&tag.body);
                    tag.sprite = r.skip(4).ok().map(|_| Timeline::parse(&mut r, tag.offset + tag.header_len() + 4, 1));
                }
                return Some(());
            }
            go(tag.sprite.as_mut()?, rest, code, body)?;
            let mut w = Writer::new();
            w.bytes(tag.body.get(..4)?);
            tag.sprite.as_ref()?.write(&mut w);
            tag.body = w.data;
            tag.declared_len = None;
            Some(())
        }
        go(&mut self.timeline, path, code, body).ok_or_else(|| crate::error::Error::Other(format!("no tag at {path:?}")))
    }

    /// Path of the tag defining a character.
    pub fn find_character(&self, id: u16) -> Option<Vec<usize>> {
        let mut found = None;
        self.walk(|path, t| {
            if found.is_none() && crate::tags::defines_character(t.code) && t.character_id() == Some(id) {
                found = Some(path.to_vec());
            }
        });
        found
    }

    /// Stores an ABC in its DoABC/DoABC2 tag, keeping a DoABC2's flags and name.
    pub fn set_abc(&mut self, path: &[usize], abc: &crate::abc::Abc) -> Result<()> {
        let tag = self.tag(path).ok_or_else(|| crate::error::Error::Other(format!("no tag at {path:?}")))?;
        let (data, _) = crate::abc::tag_abc(tag).ok_or_else(|| crate::error::Error::Other("not an ABC tag".into()))?;
        let mut body = tag.body[..tag.body.len() - data.len()].to_vec();
        body.extend(abc.to_bytes());
        self.set_tag_body(path, body)
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
