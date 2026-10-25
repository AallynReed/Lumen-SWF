//! Ports of `flash.swf.builder.tags.DefineBitsLosslessBuilder` and `DefineBitsBuilder`.

use crate::asc::flex2::flash::swf::tags::{codes, BitsKind, Body, DefineBits, Tag, TagRef, FORMAT_24_BIT_RGB};

/// `DefineBitsLosslessBuilder.build(pixels, width, height)`: premultiplied ARGB, left zero where alpha is zero.
pub fn build_lossless(pixels: &[i32], width: i32, height: i32) -> TagRef {
    let mut b = DefineBits::new(BitsKind::Lossless);
    b.format = FORMAT_24_BIT_RGB;
    b.width = width;
    b.height = height;
    b.data = vec![0u8; pixels.len() * 4];
    for (i, &p) in pixels.iter().enumerate() {
        let offset = i * 4;
        let alpha = (p >> 24) & 0xff;
        b.data[offset] = alpha as u8;
        if alpha != 0 {
            b.data[offset + 1] = ((((p >> 16) & 0xff) * alpha) / 255) as u8;
            b.data[offset + 2] = ((((p >> 8) & 0xff) * alpha) / 255) as u8;
            b.data[offset + 3] = (((p & 0xff) * alpha) / 255) as u8;
        }
    }
    Tag::new_ref(codes::DEFINE_BITS_LOSSLESS2, Body::DefineBits(b))
}

/// `DefineBitsBuilder.build(image)`: a `DefineBitsJPEG2` holding the JPEG as it is.
pub fn build_jpeg(data: Vec<u8>) -> TagRef {
    let mut b = DefineBits::new(BitsKind::Bits);
    b.data = data;
    Tag::new_ref(codes::DEFINE_BITS_JPEG2, Body::DefineBits(b))
}
