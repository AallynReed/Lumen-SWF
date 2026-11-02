//! Ports of swfutils `flash.fonts` and the font-embedding code behind `flex2.compiler.media.FontTranscoder`.

pub mod adobe;
pub mod batik_font_manager;
pub mod cached_font_face;
pub mod cached_font_manager;
pub mod font_description;
pub mod font_face;
pub mod font_manager;
pub mod font_set;
pub mod fs_type;
pub mod glyph_entry;
pub mod java2d;
pub mod java2d_api;
pub mod jre_font_manager;
pub mod local_font;
pub mod lru_cache;
#[cfg(test)]
mod tests;
