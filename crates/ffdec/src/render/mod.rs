//! Vector rendering of shapes, morph shapes, texts, buttons, sprites and frames to SVG,
//! and rasterising that SVG to PNG.

pub mod display;
pub mod paths;
pub mod svg;

pub use display::Renderer;
