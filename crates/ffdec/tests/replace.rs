use std::io::Cursor;
use ffdec::export::Exporter;
use ffdec::replace::{replace_binary, replace_image};
use ffdec::tags::{code, Tag, Timeline};
use ffdec::{Compression, SaveOptions, Swf};
use image::{GenericImageView, Rgba, RgbaImage};

fn minimal_swf(tags: Vec<Tag>) -> Swf {
    let mut swf = Swf::parse(&[
        b'F', b'W', b'S', 8,
        32, 0, 0, 0, // file size 32
        0x78, 0x00, 0x05, 0x5f, 0x00, 0x00, 0x0f, 0xa0, 0x00, // frame rect
        0x00, 0x18, // 24 fps
        1, 0, // 1 frame
        0, 0, // End tag
    ]).unwrap();
    swf.timeline.tags = tags;
    if !swf.timeline.tags.iter().any(|t| t.code == code::END) {
        swf.timeline.tags.push(Tag::new(code::END, vec![]));
    }
    swf
}

#[test]
fn test_replace_binary_data() {
    let tag = Tag::new(code::DEFINE_BINARY_DATA, vec![1, 0, 0, 0, 0, 0, 10, 20, 30]);
    let mut swf = minimal_swf(vec![tag]);

    replace_binary(&mut swf, &[0], b"hello world replaced").unwrap();

    let saved = swf.save(&SaveOptions { compression: Compression::None, encrypt: false }).unwrap();
    let reloaded = Swf::parse(&saved).unwrap();

    let tag = reloaded.tag(&[0]).unwrap();
    assert_eq!(tag.code, code::DEFINE_BINARY_DATA);
    assert_eq!(tag.character_id(), Some(1));
    assert_eq!(&tag.body[6..], b"hello world replaced");
}

#[test]
fn test_replace_lossless_image() {
    // Tag with char_id=2, format=5, width=1, height=1
    let initial_body = vec![2, 0, 5, 1, 0, 1, 0];
    let tag = Tag::new(code::DEFINE_BITS_LOSSLESS2, initial_body);
    let mut swf = minimal_swf(vec![tag]);

    // Create a 4x4 RGBA PNG in memory
    let mut img = RgbaImage::new(4, 4);
    for p in img.pixels_mut() {
        *p = Rgba([255, 0, 0, 200]);
    }
    let mut png_bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png).unwrap();

    replace_image(&mut swf, &[0], &png_bytes).unwrap();

    let saved = swf.save(&SaveOptions { compression: Compression::None, encrypt: false }).unwrap();
    let reloaded = Swf::parse(&saved).unwrap();

    let tag = reloaded.tag(&[0]).unwrap();
    assert_eq!(tag.code, code::DEFINE_BITS_LOSSLESS2);
    assert_eq!(tag.character_id(), Some(2));

    // Export through exporter to verify image roundtrip
    let exporter = Exporter::new(&reloaded);
    let files = exporter.export(&[0], Default::default()).unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0].name.ends_with(".png"));

    let decoded = image::load_from_memory(&files[0].data).unwrap();
    assert_eq!(decoded.dimensions(), (4, 4));
}

#[test]
fn test_replace_tag_in_sprite() {
    let inner_binary = Tag::new(code::DEFINE_BINARY_DATA, vec![5, 0, 0, 0, 0, 0, 1, 2, 3]);
    let end_tag = Tag::new(code::END, vec![]);
    let mut sprite_tag = Tag::new(code::DEFINE_SPRITE, vec![10, 0, 1, 0]); // sprite_id=10, frames=1
    sprite_tag.sprite = Some(Timeline {
        tags: vec![inner_binary, end_tag],
        trailer: vec![],
        has_end: true,
    });
    sprite_tag.sync_sprite();

    let mut swf = minimal_swf(vec![sprite_tag]);
    assert_eq!(swf.tag(&[0, 0]).unwrap().code, code::DEFINE_BINARY_DATA);

    // Replace tag inside sprite
    replace_binary(&mut swf, &[0, 0], b"sprite payload").unwrap();

    let saved = swf.save(&SaveOptions { compression: Compression::None, encrypt: false }).unwrap();
    let reloaded = Swf::parse(&saved).unwrap();

    let reloaded_inner = reloaded.tag(&[0, 0]).unwrap();
    assert_eq!(reloaded_inner.code, code::DEFINE_BINARY_DATA);
    assert_eq!(&reloaded_inner.body[6..], b"sprite payload");
}
