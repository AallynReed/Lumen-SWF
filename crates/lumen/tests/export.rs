mod common;

use lumen::export::{Exporter, Kind, Options};

/// Every asset in the corpus exports in both default and PNG/WAV modes.
#[test]
fn corpus_exports() {
    let files = common::swf_files();
    if files.is_empty() {
        eprintln!("test corpus not found; skipping");
        return;
    }
    let per = common::par_map(&files, |f| {
        let mut failures = Vec::new();
        let mut counts = std::collections::BTreeMap::new();
        let Ok(swf) = lumen::Swf::parse(&std::fs::read(f).unwrap()) else { return (failures, counts) };
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
        (failures, counts)
    });
    let mut failures = Vec::new();
    let mut counts = std::collections::BTreeMap::new();
    for (f, c) in per {
        failures.extend(f);
        for (k, n) in c {
            *counts.entry(k).or_insert(0) += n;
        }
    }
    eprintln!("exported: {counts:?}");
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
