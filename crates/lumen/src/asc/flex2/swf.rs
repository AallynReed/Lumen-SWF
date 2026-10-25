//! The tag sequence `flash.swf.MovieEncoder` writes for a linked `Movie`, with the bodies `flash.swf.TagEncoder` gives
//! them, built on the crate's own SWF writer.

use crate::container::Compression;
use crate::swf::{SaveOptions, Swf};
use crate::tags::{code, Tag};
use crate::types::Rect;

use super::linker::Movie;

/// `SwfEncoder.writeString` for SWF 6 and later.
fn string(b: &mut Vec<u8>, s: &str) {
    b.extend_from_slice(&crate::asc::util::jstr::to_java_utf8_bytes(s));
    b.push(0);
}

/// `MovieEncoder.export(movie)`: the tags, in order.
pub fn tags(m: &Movie) -> Vec<Tag> {
    let mut tags = Vec::new();
    if let Some(fa) = &m.file_attributes {
        let has_metadata = fa.has_metadata || m.metadata.is_some();
        let b0 = (fa.use_direct_blit as u8) << 6
            | (fa.use_gpu as u8) << 5
            | (has_metadata as u8) << 4
            | (fa.action_script3 as u8) << 3
            | (fa.use_network as u8);
        tags.push(Tag::new(code::FILE_ATTRIBUTES, vec![b0, 0, 0, 0]));
    }
    if let Some(xml) = &m.metadata {
        let mut b = Vec::new();
        string(&mut b, xml);
        tags.push(Tag::new(code::METADATA, b));
    }
    if let Some(password) = &m.enable_debugger {
        let mut b = vec![0x75, 0x19];
        string(&mut b, password);
        tags.push(Tag::new(code::ENABLE_DEBUGGER2, b));
    }
    if let Some(uuid) = &m.debug_uuid {
        tags.push(Tag::new(code::DEBUG_ID, uuid.to_vec()));
    }
    if let Some((recursion, time)) = m.script_limits {
        let mut b = Vec::new();
        b.extend_from_slice(&(recursion as u16).to_le_bytes());
        b.extend_from_slice(&(time as u16).to_le_bytes());
        tags.push(Tag::new(code::SCRIPT_LIMITS, b));
    }
    let c = m.bgcolor;
    tags.push(Tag::new(code::SET_BACKGROUND_COLOR, vec![(c >> 16) as u8, (c >> 8) as u8, c as u8]));
    let p = &m.product_info;
    let mut b = Vec::new();
    b.extend_from_slice(&(p.product as u32).to_le_bytes());
    b.extend_from_slice(&(p.edition as u32).to_le_bytes());
    b.push(p.major_version);
    b.push(p.minor_version);
    b.extend_from_slice(&(p.build as u64).to_le_bytes());
    b.extend_from_slice(&(p.compile_date as u64).to_le_bytes());
    tags.push(Tag::new(code::PRODUCT_INFO, b));

    let mut associate_root_class = m.top_level_class.is_some();
    for frame in &m.frames {
        if let Some(label) = &frame.label {
            let mut b = Vec::new();
            string(&mut b, label);
            tags.push(Tag::new(code::FRAME_LABEL, b));
        }
        for abc in &frame.do_abcs {
            let mut b = Vec::with_capacity(abc.abc.len() + abc.name.len() + 5);
            b.extend_from_slice(&(abc.flag as u32).to_le_bytes());
            string(&mut b, &abc.name);
            b.extend_from_slice(&abc.abc);
            tags.push(Tag::new(code::DO_ABC2, b));
        }
        if associate_root_class {
            let mut b = Vec::new();
            b.extend_from_slice(&1u16.to_le_bytes());
            b.extend_from_slice(&0u16.to_le_bytes());
            string(&mut b, m.top_level_class.as_deref().unwrap());
            tags.push(Tag::new(code::SYMBOL_CLASS, b));
        }
        associate_root_class = false;
        tags.push(Tag::new(code::SHOW_FRAME, Vec::new()));
    }
    tags.push(Tag::new(code::END, Vec::new()));
    tags
}

/// The movie as a `Swf`, uncompressed in memory.
pub fn to_swf(m: &Movie) -> Swf {
    let rect = Rect { x_min: 0, x_max: m.width, y_min: 0, y_max: m.height };
    Swf::new(m.version as u8, rect, m.framerate as f32, m.frames.len() as u16, tags(m))
}

/// `CompilerAPI.encode`: zlib-compressed for SWF 6 and later, as `Header.useCompression` decides.
pub fn encode(m: &Movie, compress: bool) -> crate::error::Result<Vec<u8>> {
    let compression = if compress && m.version >= 6 { Compression::Zlib } else { Compression::None };
    to_swf(m).save(&SaveOptions { compression, encrypt: false })
}
