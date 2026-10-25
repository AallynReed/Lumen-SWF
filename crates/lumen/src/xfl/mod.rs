//! SWF to XFL/FLA: the library (bitmaps, sounds, videos,
//! fonts and symbols), every timeline with its layers, masks and tweens, the scripts and the
//! publish settings, written as an XFL folder or a zipped CS5+ FLA.
//!
//! Item ids and timestamps follow the export time. Pre-CS5 binary FLA formats are
//! not written.

pub mod convert;
pub mod fixer;
pub mod geom;
pub mod media;
pub mod model;
pub mod motion;
pub mod num;
pub mod scripts;
pub mod shapes;
pub mod text;
pub mod transform;
pub mod tween;
pub mod xml;

pub use convert::{convert, Converter, FlaVersion, Options};
