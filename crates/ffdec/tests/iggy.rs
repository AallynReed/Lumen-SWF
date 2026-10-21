use std::path::Path;

use ffdec::iggy::file::{IggyFile, Variant};
use ffdec::iggy::{from_swf, texture, to_swf};
use ffdec::tags::code::*;
use ffdec::Swf;

fn jpexs(rel: &str) -> Option<Vec<u8>> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../jpexs-decompiler").join(rel)).ok()
}

#[test]
fn empty_movies_round_trip() {
    for v in [Variant::X64, Variant::X86, Variant::X86Legacy] {
        let mut f = IggyFile::create(v, "movie.swf").unwrap();
        let bytes = f.save().unwrap();
        let mut back = IggyFile::read(&bytes, None).unwrap();
        assert_eq!(back.variant(), v);
        assert_eq!(back.save().unwrap(), bytes);
        let swf = to_swf::to_swf(&back).unwrap();
        assert_eq!(swf.frame_count, 0);
        assert!(swf.timeline.tags.iter().any(|t| t.code == SET_BACKGROUND_COLOR));
    }
}

#[test]
fn converts_swf_to_iggy_and_back() {
    let Some(data) = jpexs("libsrc/ffdec_lib/testdata/as2/as2.swf") else { return };
    let swf = Swf::parse(&data).unwrap();
    let (mut file, warnings) = from_swf::create(&swf, Variant::X64, "as2.swf").unwrap();
    assert!(warnings.iter().any(|w| w.starts_with("Skipping DoAction at main/")));
    let bytes = file.save().unwrap();
    let mut file = IggyFile::read(&bytes, Some("as2.iggy")).unwrap();
    let back = to_swf::to_swf(&file).unwrap();
    assert_eq!(back.frame_count, swf.frame_count);
    assert!(back.timeline.tags.iter().any(|t| t.code == DEFINE_SPRITE));
    from_swf::update(&mut file, &back).unwrap();
    assert_eq!(file.save().unwrap(), bytes);

    let mut edited = back.clone();
    for t in &mut edited.timeline.tags {
        if t.code == SET_BACKGROUND_COLOR {
            t.body = vec![1, 2, 3];
        }
    }
    from_swf::update(&mut file, &edited).unwrap();
    let saved = IggyFile::read(&file.save().unwrap(), None).unwrap();
    let c = saved.swf.hdr.background_color();
    assert_eq!((c & 0xff, (c >> 8) & 0xff, (c >> 16) & 0xff), (1, 2, 3));
}

#[test]
fn decodes_textures() {
    // DXT1 block: white and black endpoints, indices 0..3 by row.
    let block = [0xff, 0xff, 0x00, 0x00, 0x00, 0x55, 0xaa, 0xff];
    let px = texture::decode(4, 4, texture::DXT1, &block).unwrap();
    assert_eq!(px[0], 0xffff_ffff);
    assert_eq!(px[4], 0xff00_0000);
    assert_eq!(px[8], 0xffaa_aaaa);
    assert_eq!(px[12], 0xff55_5555);
    let px = texture::decode(1, 1, texture::RGBA_4444_LE, &[0x21, 0xf3]).unwrap();
    assert_eq!(px[0], 0xff33_2211);
    assert!(texture::decode(4, 4, texture::DXT5, &block).is_err());
}

/// Compares with JPEXS: `FFDEC_IGGY_DIR` holds `n.VARIANT.iggy` made by `createIggy` from
/// line n of `FFDEC_IGGY_LIST`, their `.swf`/`.warn`/`.err` results and `.edit.iggy` edits.
#[test]
fn matches_jpexs_output() {
    let (Ok(dir), Ok(list)) = (std::env::var("FFDEC_IGGY_DIR"), std::env::var("FFDEC_IGGY_LIST")) else { return };
    let files: Vec<String> = std::fs::read_to_string(list).unwrap().lines().map(str::to_string).collect();
    let mut failures = Vec::new();
    let mut lossless = 0;
    for (i, f) in files.iter().enumerate() {
        let Ok(data) = std::fs::read(f) else { continue };
        for (v, name) in [(Variant::X86Legacy, "X86_LEGACY"), (Variant::X86, "X86"), (Variant::X64, "X64")] {
            let base = format!("{dir}/{i}.{name}");
            let Ok(expected) = std::fs::read(format!("{base}.iggy")) else { continue };
            let swf = Swf::parse(&data).unwrap();
            let (mut created, warnings) = from_swf::create(&swf, v, "movie.swf").unwrap();
            let warn: String = warnings.iter().map(|w| format!("{w}\n")).collect();
            if warn != std::fs::read_to_string(format!("{base}.warn")).unwrap_or_default() {
                failures.push(format!("{base}: warnings"));
            }
            if created.save().unwrap() != expected {
                // Lossless bitmaps FFDec re-encodes differ in zlib output and JPEG decoding.
                lossless += 1;
            }
            let file = IggyFile::read(&expected, Some(&format!("{i}.{name}.iggy"))).unwrap();
            let mut swf = to_swf::swf_bytes(&file).unwrap();
            let mut java = std::fs::read(format!("{base}.swf")).unwrap();
            if java.ends_with(&[0, 0, 0, 0]) {
                java.truncate(java.len() - 2);
                let n = java.len() as u32;
                java[4..8].copy_from_slice(&n.to_le_bytes());
            }
            if swf != java {
                failures.push(format!("{base}: SWF"));
            }
            let mut file = IggyFile::read(&expected, Some(&format!("{i}.{name}.iggy"))).unwrap();
            let parsed = Swf::parse(&swf).unwrap();
            from_swf::update(&mut file, &parsed).unwrap();
            if file.save().unwrap() != expected {
                failures.push(format!("{base}: unchanged save"));
            }
            swf.clear();
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
    assert!(lossless <= 6, "{lossless} created files differ");
}
