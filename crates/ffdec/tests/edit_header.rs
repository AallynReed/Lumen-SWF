use std::path::Path;

use ffdec::edit::{header, set_header};
use ffdec::swf::SaveOptions;

fn sample() -> Option<ffdec::Swf> {
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../jpexs-decompiler/libsrc/ffdec_lib/testdata/debug_game/as2_scoring.swf");
    ffdec::Swf::parse(&std::fs::read(f).ok()?).ok()
}

fn saved(swf: &ffdec::Swf) -> ffdec::Swf {
    let opts = SaveOptions { compression: swf.header.compression, encrypt: false };
    ffdec::Swf::parse(&swf.save(&opts).unwrap()).unwrap()
}

#[test]
fn header_edits_are_saved() {
    let Some(mut swf) = sample() else { return };
    let mut h = header(&swf);
    h.version = 9;
    h.frame_rate = 200.5;
    h.width = 321.5;
    h.height = 123.0;
    h.background = Some("#12ab34".into());
    set_header(&mut swf, &h).unwrap();
    let again = header(&saved(&swf));
    assert_eq!(again, h);
}

#[test]
fn bad_header_values_are_refused() {
    let Some(mut swf) = sample() else { return };
    let before = header(&swf);
    for bad in [ffdec::edit::HeaderEdit { width: 0.0, ..before.clone() }, ffdec::edit::HeaderEdit { background: Some("red".into()), ..before.clone() }, ffdec::edit::HeaderEdit { frame_rate: 300.0, ..before.clone() }] {
        assert!(set_header(&mut swf, &bad).is_err());
    }
    assert_eq!(header(&swf), before);
}

#[test]
fn removing_a_show_frame_keeps_frame_counts() {
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../jpexs-decompiler/libsrc/ffdec_lib/testdata/as2/as2.swf");
    let Some(mut swf) = std::fs::read(f).ok().and_then(|d| ffdec::Swf::parse(&d).ok()) else { return };
    let (sprite, show) = swf
        .timeline
        .tags
        .iter()
        .enumerate()
        .find_map(|(i, t)| t.sprite.as_ref().and_then(|s| s.tags.iter().position(|c| c.code == ffdec::tags::code::SHOW_FRAME).filter(|_| s.frame_count() > 1)).map(|j| (i, j)))
        .expect("a sprite with frames");
    let frames = swf.timeline.tags[sprite].sprite.as_ref().unwrap().frame_count();
    swf.remove_tag(&[sprite, show]).unwrap();
    let swf = saved(&swf);
    let t = &swf.timeline.tags[sprite];
    assert_eq!(t.sprite.as_ref().unwrap().frame_count(), frames - 1);
    assert_eq!(u16::from_le_bytes([t.body[2], t.body[3]]) as usize, frames - 1);
}
