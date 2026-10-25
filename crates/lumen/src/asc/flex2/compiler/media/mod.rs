//! Ports of `flex2.compiler.media`, the `[Embed]` transcoders, and `WebTierAPI.getTranscoders`.

pub mod abstract_transcoder;
pub mod data_transcoder;
pub mod image_transcoder;
pub mod jpeg_transcoder;
pub mod lossless_image_transcoder;
pub mod mime;
pub mod movie_transcoder;
pub mod pbj_transcoder;
pub mod skin_transcoder;
pub mod sound_transcoder;
#[cfg(test)]
mod tests;
pub mod velocity_boundary;
pub mod xml_transcoder;

use super::configuration::Configuration;
use super::transcoder::Transcoder;

/// Makes a transcoder for a slot of [`get_transcoders`] that is ported elsewhere.
pub type TranscoderFactory = fn(&Configuration) -> Box<dyn Transcoder>;

/// `new SVGTranscoder(showDeprecationWarnings)`, set once by the Batik port.
pub static SVG_TRANSCODER: std::sync::OnceLock<TranscoderFactory> = std::sync::OnceLock::new();
/// `new FontTranscoder(cfg)`, set once by the font port.
pub static FONT_TRANSCODER: std::sync::OnceLock<TranscoderFactory> = std::sync::OnceLock::new();

/// `WebTierAPI.getTranscoders(cfg)`, in its order; the SVG and font slots stay empty until their ports register.
pub fn get_transcoders(cfg: &Configuration) -> Vec<Box<dyn Transcoder>> {
    let mut t: Vec<Box<dyn Transcoder>> = vec![Box::new(jpeg_transcoder::JpegTranscoder::new()), Box::new(lossless_image_transcoder::LosslessImageTranscoder::new())];
    if let Some(f) = SVG_TRANSCODER.get() {
        t.push(f(cfg));
    }
    t.push(Box::new(sound_transcoder::SoundTranscoder::new()));
    t.push(Box::new(movie_transcoder::MovieTranscoder::new()));
    if let Some(f) = FONT_TRANSCODER.get() {
        t.push(f(cfg));
    }
    t.push(Box::new(data_transcoder::DataTranscoder::new()));
    t.push(Box::new(xml_transcoder::XmlTranscoder::new()));
    t.push(Box::new(skin_transcoder::SkinTranscoder::new()));
    t.push(Box::new(pbj_transcoder::PbjTranscoder::new()));
    t
}
