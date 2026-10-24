use std::path::Path;

use ffdec::shape::{is_shape, ShapeTag};

fn corpus() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libsrc/ffdec_lib/testdata")];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("swf")) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Every corpus shape written back reads as the same shape; most are byte-identical.
#[test]
fn shapes_write_back() {
    let (mut total, mut same, mut bad) = (0, 0, Vec::new());
    for f in corpus() {
        let Ok(swf) = ffdec::Swf::parse(&std::fs::read(&f).unwrap()) else { continue };
        swf.walk(|path, t| {
            if !is_shape(t.code) {
                return;
            }
            let Ok(s) = ShapeTag::read(t) else { return };
            total += 1;
            let body = s.write();
            if body == t.body {
                same += 1;
            }
            let tag = ffdec::tags::Tag { code: t.code, body: body.clone(), ..t.clone() };
            match ShapeTag::read(&tag) {
                Ok(b) if b.shape == s.shape && b.bounds == s.bounds && b.flags == s.flags && b.edge_bounds == s.edge_bounds => {}
                _ => bad.push(format!("{} {path:?}", f.display())),
            }
        });
    }
    eprintln!("shapes: {}/{total} read back, {same} byte-identical", total - bad.len());
    assert!(bad.is_empty(), "{:?}", &bad[..bad.len().min(10)]);
}

/// Compares image-to-shape import with the reference output, read from
/// `FFDEC_SHAPE_REF` (the ShapeDump driver's output: per SWF, per image and fill mode, the new
/// image tag and the shape body).
#[test]
fn shape_images_match_reference() {
    let Ok(dir) = std::env::var("FFDEC_SHAPE_REF") else { return };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libsrc/ffdec_lib/testdata");
    let images = [("png", std::fs::read(root.join("as2/as2/LIBRARY/Database-Delete-32.png")).unwrap()), ("jpg", std::fs::read(root.join("gfx/test_images/LIBRARY/pony_small.jpg")).unwrap())];
    let (mut total, mut bad) = (0, Vec::new());
    for d in std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()) {
        let Ok(swf_path) = std::fs::read_to_string(d.join("swf.txt")) else { continue };
        let Ok(data) = std::fs::read(swf_path.trim()) else { continue };
        let Ok(orig) = ffdec::Swf::parse(&data) else { continue };
        let Some(at) = orig.timeline.tags.iter().position(|t| is_shape(t.code)) else { continue };
        for (name, img) in &images {
            for fill in [true, false] {
                let case = format!("{name}_{}", if fill { "fill" } else { "size" });
                let Ok(want_shape) = std::fs::read(d.join(format!("{case}.shape"))) else { continue };
                total += 1;
                let want_img = std::fs::read(d.join(format!("{case}.img"))).unwrap();
                let want_code: u16 = std::fs::read_to_string(d.join(format!("{case}.code"))).unwrap().trim().parse().unwrap();
                let mut swf = orig.clone();
                let label = format!("{} {case}", swf_path.trim());
                if let Err(e) = ffdec::importers::shape_image(&mut swf, &[at], img, fill) {
                    bad.push(format!("{label}: {e}"));
                    continue;
                }
                let (t_img, t_shape) = (&swf.timeline.tags[at], &swf.timeline.tags[at + 1]);
                let inflate = |b: &[u8]| -> Vec<u8> {
                    use std::io::Read;
                    let mut out = b[..7.min(b.len())].to_vec();
                    flate2::read::ZlibDecoder::new(b.get(7..).unwrap_or(&[])).read_to_end(&mut out).ok();
                    out
                };
                let same_img = t_img.body == want_img || (t_img.code == 36 && inflate(&t_img.body) == inflate(&want_img));
                if t_img.code != want_code || !same_img {
                    bad.push(format!("{label}: image differs (code {} vs {want_code}, {} vs {} bytes)", t_img.code, t_img.body.len(), want_img.len()));
                } else if t_shape.body != want_shape {
                    bad.push(format!("{label}: shape differs
 got {:02x?}
want {:02x?}", t_shape.body, want_shape));
                }
            }
        }
    }
    eprintln!("shape images: {}/{total} ok", total - bad.len());
    assert!(bad.is_empty(), "{:#?}", &bad[..bad.len().min(10)]);
}
