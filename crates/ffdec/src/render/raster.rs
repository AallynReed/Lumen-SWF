//! SVG → PNG through resvg, with system fonts for dynamic text.

use std::sync::{Arc, OnceLock};

use resvg::{tiny_skia, usvg};

use crate::error::{Error, Result};

pub fn fonts() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    })
    .clone()
}

/// Rasterises an SVG document at `zoom` × its own size.
pub fn png(svg: &str, zoom: f32) -> Result<Vec<u8>> {
    let opt = usvg::Options { fontdb: fonts(), ..Default::default() };
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| Error::Other(format!("SVG: {e}")))?;
    let size = tree.size();
    let w = ((size.width() * zoom).ceil() as u32).clamp(1, 16384);
    let h = ((size.height() * zoom).ceil() as u32).clamp(1, 16384);
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or_else(|| Error::Other("image too large".into()))?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(zoom, zoom), &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|e| Error::Other(format!("PNG: {e}")))
}
