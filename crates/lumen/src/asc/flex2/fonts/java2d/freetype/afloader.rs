//! Stand-in for FreeType 2.14.2 `src/autofit/afloader.c` (as built into JDK 17.0.19): the auto-hinter is not ported, so auto-hinted loads fail.

use crate::asc::flex2::fonts::java2d::freetype::fterrors::FT_ERR_UNIMPLEMENTED_FEATURE;
use crate::asc::flex2::fonts::java2d::freetype::ftobjs::{FtFace, FtGlyphSlot};
use crate::asc::flex2::fonts::java2d::freetype::fttypes::FtError;
use crate::asc::flex2::fonts::java2d::freetype::ttobjs::TtSize;

pub fn af_loader_load_glyph(_face: &mut FtFace, _size: &mut TtSize, _slot: &mut FtGlyphSlot, _glyph_index: u32, _load_flags: i32) -> FtError {
    FT_ERR_UNIMPLEMENTED_FEATURE
}
