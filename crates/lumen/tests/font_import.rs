mod common;

use std::path::Path;

use lumen::font_edit::Font2;
use lumen::shape::Record;
use lumen::tags::code::{DEFINE_FONT2, DEFINE_FONT3};

/// Corpus fonts written back read the same; glyph shapes written with minimal field sizes are
/// not always the original bytes.
#[test]
fn fonts_write_back() {
    let per = common::par_swfs(|f, swf| {
        let (mut total, mut same, mut bad) = (0, 0, Vec::new());
        swf.walk(|path, t| {
            if t.code != DEFINE_FONT2 && t.code != DEFINE_FONT3 {
                return;
            }
            total += 1;
            match Font2::read(t.code, &t.body).map(|mut font| (font.write(), font)) {
                Ok((b, _)) if b == t.body => same += 1,
                Ok((b, font)) if Font2::read(t.code, &b).is_ok_and(|again| again == font) => {}
                Ok(_) => bad.push(format!("{} {path:?}: reads back differently", f.display())),
                Err(e) => bad.push(format!("{} {path:?}: {e}", f.display())),
            }
        });
        (total, same, bad)
    });
    let (mut total, mut same, mut bad) = (0, 0, Vec::new());
    for (t, k, b) in per {
        total += t;
        same += k;
        bad.extend(b);
    }
    eprintln!("fonts: {}/{total} read back the same, {same} byte-identical", total - bad.len());
    assert!(bad.is_empty(), "{bad:#?}");
}

/// Largest coordinate difference between two glyphs' points, or None if their record kinds
/// differ.
fn glyph_distance(a: &[Record], b: &[Record]) -> Option<i32> {
    if a.len() != b.len() {
        return None;
    }
    let mut d = 0;
    let (mut pa, mut pb) = ((0, 0), (0, 0));
    for (x, y) in a.iter().zip(b) {
        match (x, y) {
            (Record::StyleChange { move_to: m1, .. }, Record::StyleChange { move_to: m2, .. }) => {
                if let (Some(m1), Some(m2)) = (m1, m2) {
                    (pa, pb) = (*m1, *m2);
                } else if m1.is_some() != m2.is_some() {
                    return None;
                }
            }
            (Record::Straight { dx: ax, dy: ay }, Record::Straight { dx: bx, dy: by }) => {
                pa = (pa.0 + ax, pa.1 + ay);
                pb = (pb.0 + bx, pb.1 + by);
            }
            (Record::Curved { cdx, cdy, adx, ady }, Record::Curved { cdx: c2, cdy: d2, adx: a2, ady: b2 }) => {
                d = d.max((pa.0 + cdx - pb.0 - c2).abs()).max((pa.1 + cdy - pb.1 - d2).abs());
                pa = (pa.0 + cdx + adx, pa.1 + cdy + ady);
                pb = (pb.0 + c2 + a2, pb.1 + d2 + b2);
            }
            _ => return None,
        }
        d = d.max((pa.0 - pb.0).abs()).max((pa.1 - pb.1).abs());
    }
    Some(d)
}

/// Compares adding characters from a font file with the reference output, read
/// from `LUMEN_FONT_REF` (`dir=fontfile;...`, each dir the FontDump driver's output). The reference takes
/// outlines and advances through Java's AWT, which hints them and may start a composite glyph's
/// contours elsewhere, so glyphs are counted rather than required to match.
#[test]
fn fonts_import_like_reference() {
    let Ok(refs) = std::env::var("LUMEN_FONT_REF") else { return };
    let (mut total, mut exact, mut reordered, mut worst, mut bad) = (0, 0, 0, 0, Vec::new());
    for pair in refs.split(';') {
        let (dir, font_file) = pair.split_once('=').unwrap();
        let ttf = std::fs::read(font_file).unwrap();
        for d in std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()) {
            let Ok(swf_path) = std::fs::read_to_string(d.join("swf.txt")) else { continue };
            let orig = lumen::Swf::parse(&std::fs::read(swf_path.trim()).unwrap()).unwrap();
            let Some(at) = orig.timeline.tags.iter().position(|t| t.code == DEFINE_FONT2 || t.code == DEFINE_FONT3) else { continue };
            for mode in ["same", "add"] {
                let Ok(want) = std::fs::read(d.join(format!("{mode}.font"))) else { continue };
                let chars: Vec<char> = std::fs::read_to_string(d.join(format!("{mode}.chars"))).unwrap().chars().collect();
                let label = format!("{} {mode} {}", swf_path.trim(), Path::new(font_file).file_name().unwrap().to_string_lossy());
                let mut swf = orig.clone();
                if let Err(e) = lumen::font_edit::add_characters(&mut swf, &[at], &ttf, &chars) {
                    bad.push(format!("{label}: {e}"));
                    continue;
                }
                let code = swf.timeline.tags[at].code;
                let got = Font2::read(code, &swf.timeline.tags[at].body).unwrap();
                let want = Font2::read(code, &want).unwrap();
                if got.codes != want.codes || got.flags != want.flags {
                    bad.push(format!("{label}: codes or flags differ"));
                    continue;
                }
                let (gl, wl) = (got.layout.as_ref(), want.layout.as_ref());
                if let (Some(g), Some(w)) = (gl, wl) {
                    for (i, (a, b)) in g.advances.iter().zip(&w.advances).enumerate() {
                        if (*a as i32 - *b as i32).abs() > 4 * got.v3.then_some(20).unwrap_or(1) {
                            bad.push(format!("{label}: advance of {:?} is {a}, expected {b}", char::from_u32(got.codes[i] as u32)));
                        }
                    }
                }
                for (i, (a, b)) in got.glyphs.iter().zip(&want.glyphs).enumerate() {
                    total += 1;
                    match glyph_distance(a, b) {
                        Some(0) => exact += 1,
                        Some(dist) => worst = worst.max(dist),
                        None => reordered += 1,
                    }
                }
                let texts: Vec<Vec<u8>> = swf.timeline.tags.iter().filter(|t| t.code == 11 || t.code == 33).map(|t| t.body.clone()).collect();
                let want_texts = std::fs::read(d.join(format!("{mode}.texts"))).unwrap();
                let mut wt = Vec::new();
                let mut i = 0;
                while i + 4 <= want_texts.len() {
                    let len = u32::from_le_bytes(want_texts[i..i + 4].try_into().unwrap()) as usize;
                    wt.push(want_texts[i + 4..i + 4 + len].to_vec());
                    i += 4 + len;
                }
                let read = |b: &Vec<u8>| lumen::text::StaticText::read(&lumen::tags::Tag::new(11, b.clone())).map(|t| t.records).ok();
                if texts.len() != wt.len() || texts.iter().zip(&wt).any(|(a, b)| read(a) != read(b)) {
                    bad.push(format!("{label}: static texts differ"));
                }
            }
        }
    }
    eprintln!("font glyphs: {exact}/{total} exact, {reordered} with contours in another order, others within {worst} units");
    assert!(exact * 100 >= total * 95, "only {exact} of {total} glyphs match");
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("font_import_report.txt"), bad.join("\n")).unwrap();
    assert!(bad.is_empty(), "{} problems, first: {:?}", bad.len(), bad.first());
}

#[test]
fn replaced_fonts_relayout_their_texts() {
    let (Ok(data), Ok(ttf)) = (
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libsrc/ffdec_lib/testdata/debug_game/as2_scoring.swf")),
        std::fs::read(r"C:\Windows\Fonts\consola.ttf"),
    ) else {
        return;
    };
    let mut swf = lumen::Swf::parse(&data).unwrap();
    let font = swf.find_character(2).unwrap();
    lumen::importers::import(&mut swf, &font, &ttf).unwrap();
    let text = lumen::text::StaticText::read(swf.tag(&swf.find_character(3).unwrap()).unwrap()).unwrap();
    let advances: Vec<i32> = text.records.iter().flat_map(|r| r.glyphs.iter().map(|g| g.1)).collect();
    assert!(advances.len() > 2 && advances.iter().all(|&a| a == advances[0]), "{advances:?}");
}
