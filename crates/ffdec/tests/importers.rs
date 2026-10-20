use std::io::{Cursor, Read};

use ffdec::importers;
use ffdec::io::Writer;
use ffdec::tags::{code, Tag};
use ffdec::{Compression, SaveOptions, Swf};

/// An uncompressed SWF 8 holding `tags`, parsed back.
fn swf_with(tags: &[Tag]) -> Swf {
    let mut w = Writer::new();
    for t in tags {
        t.write(&mut w);
    }
    w.bytes(&[0, 0]);
    let mut data = b"FWS\x08".to_vec();
    data.extend_from_slice(&(15 + w.data.len() as u32 - 2).to_le_bytes());
    data.extend_from_slice(&[0, 0, 24, 1, 0]);
    data.extend_from_slice(&w.data);
    Swf::parse(&data).unwrap()
}

fn reload(swf: &Swf) -> Swf {
    Swf::parse(&swf.save(&SaveOptions { compression: Compression::None, encrypt: false }).unwrap()).unwrap()
}

fn png(rgba: [u8; 4]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(4, 4, image::Rgba(rgba));
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

#[test]
fn binary_data_keeps_its_header() {
    let mut swf = swf_with(&[Tag::new(code::DEFINE_BINARY_DATA, vec![1, 0, 7, 0, 0, 0, 10])]);
    importers::binary(&mut swf, &[0], b"new").unwrap();
    assert_eq!(reload(&swf).tag(&[0]).unwrap().body, [1, 0, 7, 0, 0, 0, b'n', b'e', b'w']);
}

#[test]
fn lossless2_is_premultiplied() {
    let mut swf = swf_with(&[Tag::new(code::DEFINE_BITS_LOSSLESS2, vec![2, 0, 5, 1, 0, 1, 0])]);
    importers::image(&mut swf, &[0], &png([255, 0, 0, 102])).unwrap();
    let t = reload(&swf).tag(&[0]).unwrap().clone();
    assert_eq!(t.body[..7], [2, 0, 5, 4, 0, 4, 0]);
    let mut px = Vec::new();
    flate2::read::ZlibDecoder::new(&t.body[7..]).read_to_end(&mut px).unwrap();
    assert_eq!(px[..4], [102, 102, 0, 0]);
}

#[test]
fn define_bits_becomes_jpeg2() {
    let mut swf = swf_with(&[Tag::new(code::DEFINE_BITS, vec![3, 0, 0xff, 0xd8])]);
    let data = png([0, 0, 255, 255]);
    importers::image(&mut swf, &[0], &data).unwrap();
    let t = reload(&swf).tag(&[0]).unwrap().clone();
    assert_eq!((t.code, &t.body[2..]), (code::DEFINE_BITS_JPEG2, &data[..]));
}

#[test]
fn nested_edits_rewrite_the_sprite() {
    let mut inner = Writer::new();
    Tag::new(code::DEFINE_BINARY_DATA, vec![5, 0, 0, 0, 0, 0, 1]).write(&mut inner);
    inner.bytes(&[0, 0]);
    let mut body = vec![10, 0, 1, 0];
    body.extend_from_slice(&inner.data);
    let mut swf = swf_with(&[Tag::new(code::DEFINE_SPRITE, body)]);
    importers::binary(&mut swf, &[0, 0], b"sprite payload").unwrap();
    let back = reload(&swf);
    assert_eq!(back.tag(&[0]).unwrap().body[..4], [10, 0, 1, 0]);
    assert_eq!(&back.tag(&[0, 0]).unwrap().body[6..], b"sprite payload");
}

#[test]
fn wav_is_stored_uncompressed() {
    let pcm = [0u8, 0, 1, 0, 2, 0, 3, 0];
    let mut wav = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x44\xac\0\0\x88\x58\x01\0\x02\0\x10\0data".to_vec();
    wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(&pcm);
    let mut swf = swf_with(&[Tag::new(code::DEFINE_SOUND, vec![4, 0, 0x22, 0, 0, 0, 0])]);
    importers::sound(&mut swf, &[0], &wav).unwrap();
    let t = reload(&swf).tag(&[0]).unwrap().clone();
    assert_eq!(t.body[..7], [4, 0, 0x3e, 4, 0, 0, 0]);
    assert_eq!(&t.body[7..], &pcm);
}
