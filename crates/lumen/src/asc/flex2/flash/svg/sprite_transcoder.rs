//! Port of `flash.svg.SpriteTranscoder` with the `XMLAbstractTranscoder.transcode` it inherits: parse the
//! document, build its GVT tree and paint it into a `SpriteGraphics2D`.

use crate::asc::flex2::batik::bridge::bridge_context::{print_stderr, BridgeContext};
use crate::asc::flex2::batik::bridge::gvt_builder::GvtBuilder;
use crate::asc::flex2::batik::bridge::BridgeError;
use crate::asc::flex2::batik::dom::{self, DocError};
use crate::asc::flex2::flash::graphics::g2d::sprite_graphics2d::SpriteGraphics2D;
use crate::asc::flex2::flash::swf::tags::TagRef;

/// How `transcode` failed: an `IOException` the transcoder printed (and wrapped), or another exception's message.
#[derive(Debug)]
pub enum TranscodeFailure {
    /// `TranscoderException(ex)` around an `IOException`, which `XMLAbstractTranscoder` also printed.
    Io(String),
    /// `TranscoderException(ex)` around a `DOMException` or a `BridgeException`.
    Wrapped(String),
    /// Another runtime exception: its message.
    Runtime(Option<String>),
    /// Something this port does not render.
    Unsupported(String),
}

pub struct SpriteTranscoder {
    pub tags: Vec<TagRef>,
    pub width: i32,
    pub height: i32,
}

impl SpriteTranscoder {
    /// `transcode(new TranscoderInput(stream) with uri, null)`.
    pub fn transcode(uri: &str, bytes: Vec<u8>) -> Result<SpriteTranscoder, TranscodeFailure> {
        let document = match dom::create_svg_document(Some(uri), bytes) {
            Ok(d) => d,
            Err(DocError::Io(m)) => {
                print_stderr(&format!("java.io.IOException: {m}"));
                return Err(TranscodeFailure::Io(m));
            }
            Err(DocError::Dom(d)) => return Err(TranscodeFailure::Wrapped(d.0)),
        };
        let ctx = BridgeContext::new(document.clone());
        let builder = GvtBuilder { ctx: &ctx };
        let root = match builder.build_document(&document) {
            Ok(r) => r,
            Err(BridgeError::Bridge(m)) => return Err(TranscodeFailure::Wrapped(m)),
            Err(BridgeError::Runtime(m)) => return Err(TranscodeFailure::Runtime(m)),
            Err(BridgeError::Unsupported(m)) => return Err(TranscodeFailure::Unsupported(m)),
        };
        let Some((w, h)) = ctx.document_size.get() else { return Err(TranscodeFailure::Runtime(None)) };
        let mut swf2d = SpriteGraphics2D::new(w, h);
        if let Some(root) = &root {
            root.paint(&mut swf2d).map_err(|u| TranscodeFailure::Runtime(Some(u.0)))?;
            if let Some(u) = swf2d.unsupported.take() {
                return Err(TranscodeFailure::Unsupported(u));
            }
        } else {
            return Err(TranscodeFailure::Runtime(None));
        }
        let b = swf2d.get_bounds();
        let width = ((b.x_max - b.x_min) as f64 / 20.0).round_ties_even() as i32;
        let height = ((b.y_max - b.y_min) as f64 / 20.0).round_ties_even() as i32;
        Ok(SpriteTranscoder { tags: swf2d.define_tags, width, height })
    }
}
