use std::path::Path;

use ffdec::shape::{is_shape, ShapeTag};

fn corpus() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("../../jpexs-decompiler/libsrc/ffdec_lib/testdata")];
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

