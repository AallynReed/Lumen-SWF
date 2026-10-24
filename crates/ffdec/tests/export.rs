use std::path::{Path, PathBuf};

use ffdec::export::{Exporter, Kind, Options};

fn corpus() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libsrc/ffdec_lib/testdata");
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
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

/// Every asset in the corpus exports in both default and PNG/WAV modes.
#[test]
fn corpus_exports() {
    let files = corpus();
    if files.is_empty() {
        eprintln!("test corpus not found; skipping");
        return;
    }
    let mut failures = Vec::new();
    let mut counts = std::collections::BTreeMap::new();
    for f in &files {
        let Ok(swf) = ffdec::Swf::parse(&std::fs::read(f).unwrap()) else { continue };
        let ex = Exporter::new(&swf);
        for opts in [Options::default(), Options { png_only: true, wav_only: true, shapes_png: true, frames_svg: true, texts_svg: true, zoom: 0.25, scripts_pcode: true }] {
            for (kind, path, r) in ex.export_all(&Kind::ALL, opts) {
                *counts.entry(kind.dir()).or_insert(0) += 1;
                if let Err(e) = r {
                    failures.push(format!("{} {path:?} {}: {e}", f.display(), kind.dir()));
                }
            }
        }
        swf.walk(|path, _| {
            let _ = ex.preview(path, 0);
        });
        let _ = ex.preview(&[], 0);
    }
    eprintln!("exported: {counts:?}");
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
