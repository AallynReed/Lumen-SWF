use std::io::Read;
use std::path::{Path, PathBuf};

use lumen::shape::ShapeTag;

fn inflate_lossless(code: u16, b: &[u8]) -> Vec<u8> {
    if code != 36 || b.len() < 7 {
        return b.to_vec();
    }
    let mut out = b[..7].to_vec();
    flate2::read::ZlibDecoder::new(&b[7..]).read_to_end(&mut out).ok();
    out
}

/// Imports `svg` into the root shape at `at` and compares the result with `stem.tags`
/// (the tags it added: code, length, body) and `stem.shape` in `d`. Shapes are compared as
/// decoded records, since the reference writes matrices with scale and rotation fields even when they
/// are identity. Ok(true) when the shape is byte-identical.
fn compare(orig: &lumen::Swf, at: usize, svg: &str, fill: bool, d: &Path, stem: &str) -> Result<bool, String> {
    let mut swf = orig.clone();
    lumen::svg_import::import(&mut swf, &[at], svg, fill).map_err(|e| e.to_string())?;
    let want_tags = std::fs::read(d.join(format!("{stem}.tags"))).unwrap();
    let mut want = Vec::new();
    let mut i = 0;
    while i + 6 <= want_tags.len() {
        let code = u16::from_le_bytes([want_tags[i], want_tags[i + 1]]);
        let len = u32::from_le_bytes(want_tags[i + 2..i + 6].try_into().unwrap()) as usize;
        want.push((code, want_tags[i + 6..i + 6 + len].to_vec()));
        i += 6 + len;
    }
    let added = swf.timeline.tags.len() - orig.timeline.tags.len();
    let got: Vec<(u16, Vec<u8>)> = swf.timeline.tags[at..at + added].iter().map(|t| (t.code, t.body.clone())).collect();
    if got.len() != want.len() || got.iter().zip(&want).any(|(g, w)| g.0 != w.0 || inflate_lossless(g.0, &g.1) != inflate_lossless(w.0, &w.1)) {
        return Err(format!("new tags differ ({} vs {})", got.len(), want.len()));
    }
    let got = &swf.timeline.tags[at + added];
    let want_body = std::fs::read(d.join(format!("{stem}.shape"))).unwrap();
    if got.body == want_body {
        return Ok(true);
    }
    let want_tag = lumen::tags::Tag::new(got.code, want_body);
    match (ShapeTag::read(got), ShapeTag::read(&want_tag)) {
        (Ok(g), Ok(w)) if g.bounds == w.bounds && g.edge_bounds == w.edge_bounds && g.flags == w.flags && g.shape == w.shape => Ok(false),
        (Ok(g), Ok(w)) => {
            let first = g.shape.records.iter().zip(&w.shape.records).position(|(a, b)| a != b);
            Err(format!(
                "shape differs: bounds {}/{} flags {}/{} records {}/{} first diff {:?}: {:?} vs {:?}",
                g.bounds == w.bounds,
                g.edge_bounds == w.edge_bounds,
                g.flags,
                w.flags,
                g.shape.records.len(),
                w.shape.records.len(),
                first,
                first.map(|i| &g.shape.records[i]),
                first.map(|i| &w.shape.records[i])
            ))
        }
        (a, b) => Err(format!("unreadable {:?} {:?}", a.err(), b.err())),
    }
}

/// The reference SVG export of each root corpus shape, imported back, read from
/// `LUMEN_SVG_REF` (the SvgDump driver's output), filled and at its own size.
#[test]
fn svgs_import_like_reference() {
    let Ok(dir) = std::env::var("LUMEN_SVG_REF") else { return };
    let (mut total, mut same_bytes, mut bad) = (0, 0, Vec::new());
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
    dirs.sort();
    for d in dirs {
        let Ok(swf_path) = std::fs::read_to_string(d.join("swf.txt")) else { continue };
        let Ok(data) = std::fs::read(swf_path.trim()) else { continue };
        let Ok(orig) = lumen::Swf::parse(&data) else { continue };
        let mut cases: Vec<PathBuf> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "shape")).collect();
        cases.sort();
        for c in cases {
            let stem = c.file_stem().unwrap().to_string_lossy().to_string();
            let (id, mode) = stem.split_once('_').unwrap();
            let id: u16 = id.parse().unwrap();
            let svg = std::fs::read_to_string(d.join(format!("{id}.svg"))).unwrap();
            let Some(at) = orig.timeline.tags.iter().position(|t| lumen::shape::is_shape(t.code) && t.character_id() == Some(id)) else { continue };
            total += 1;
            match compare(&orig, at, &svg, mode == "fill", &d, &stem) {
                Ok(true) => same_bytes += 1,
                Ok(false) => {}
                Err(e) => bad.push(format!("{} {stem}: {e}", swf_path.trim())),
            }
        }
    }
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("svg_import_report.txt"), bad.join("\n")).unwrap();
    eprintln!("svg import: {}/{total} ok, {same_bytes} byte-identical", total - bad.len());
    assert!(bad.is_empty(), "{} imports differ", bad.len());
}

/// Hand-written SVGs covering basic shapes, arcs, cubics, CSS, transforms, units, switch and
/// gradient inheritance, imported by the reference into one shape of each version (`LUMEN_SVG_CASES`,
/// the SvgCases driver's output, with the SVGs in `LUMEN_SVG_CASES_SRC`).
#[test]
fn svg_cases_import_like_reference() {
    let (Ok(dir), Ok(src)) = (std::env::var("LUMEN_SVG_CASES"), std::env::var("LUMEN_SVG_CASES_SRC")) else { return };
    let dir = PathBuf::from(dir);
    let index = std::fs::read_to_string(dir.join("index.txt")).unwrap();
    let mut svgs: Vec<PathBuf> = std::fs::read_dir(&src).unwrap().flatten().map(|e| e.path()).collect();
    svgs.sort();
    let (mut total, mut bad) = (0, Vec::new());
    for line in index.lines() {
        let (num, swf_path) = line.split_once('\t').unwrap();
        let orig = lumen::Swf::parse(&std::fs::read(swf_path).unwrap()).unwrap();
        let at = orig.timeline.tags.iter().position(|t| lumen::shape::is_shape(t.code)).unwrap();
        for svg in &svgs {
            let name = svg.file_stem().unwrap().to_string_lossy().to_string();
            let text = std::fs::read_to_string(svg).unwrap();
            for fill in [true, false] {
                let stem = format!("{num}_{name}_{}", if fill { "fill" } else { "size" });
                if !dir.join(format!("{stem}.shape")).exists() {
                    continue;
                }
                total += 1;
                if let Err(e) = compare(&orig, at, &text, fill, &dir, &stem) {
                    bad.push(format!("{stem}: {e}"));
                }
            }
        }
    }
    eprintln!("svg cases: {}/{total} ok", total - bad.len());
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn media_rules_apply() {
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libsrc/ffdec_lib/testdata/as2/as2.swf");
    let Ok(data) = std::fs::read(f) else { return };
    let mut swf = lumen::Swf::parse(&data).unwrap();
    let at = swf.timeline.tags.iter().position(|t| lumen::shape::is_shape(t.code)).unwrap();
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><style>@media screen { rect { fill: blue } } circle { fill: lime }</style><rect width="5" height="5"/><circle r="2"/></svg>"#;
    lumen::svg_import::import(&mut swf, &[at], svg, false).unwrap();
    let s = ShapeTag::read(&swf.timeline.tags[at]).unwrap();
    let fills: Vec<_> = s.shape.records.iter().filter_map(|r| match r {
        lumen::shape::Record::StyleChange { new_styles: Some(st), .. } => st.fills.first().cloned(),
        _ => None,
    }).collect();
    let blue = lumen::types::Rgba { r: 0, g: 0, b: 255, a: 255 };
    let lime = lumen::types::Rgba { r: 0, g: 255, b: 0, a: 255 };
    assert_eq!(fills, vec![lumen::shape::Fill::Solid(blue), lumen::shape::Fill::Solid(lime)]);
}
