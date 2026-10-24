//! Rust port of the Free Flash Decompiler core library.

pub mod abc;
pub mod amf;
pub mod asc;
pub mod bundle;
pub mod action;
pub mod container;
pub mod edit;
pub mod error;
pub mod export;
pub mod filters;
pub mod font_edit;
pub mod harman;
pub mod iggy;
pub mod importers;
pub mod info;
pub mod io;
pub mod morph;
pub mod names;
pub mod render;
pub mod search;
pub mod svg_import;
pub mod swf;
pub mod shape;
pub mod tags;
pub mod text;
pub mod tree;
pub mod types;

pub use container::Compression;
pub use error::{Error, Result};
pub use swf::{SaveOptions, Swf};
pub use tags::Tag;

/// Turns automatic deobfuscation on or off for everything decompiled afterwards.
pub fn set_auto_deobfuscate(on: bool) {
    abc::decomp::ctx::AUTO_DEOBFUSCATE.store(on, std::sync::atomic::Ordering::Relaxed);
}
pub mod xfl;
