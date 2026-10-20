use std::path::{Path, PathBuf};

fn corpus() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../jpexs-decompiler/libsrc/ffdec_lib/testdata");
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

/// Unmodified files must decompress and re-save byte for byte.
#[test]
fn jpexs_corpus_roundtrip() {
    let files = corpus();
    if files.is_empty() {
        eprintln!("JPEXS test corpus not found; skipping");
        return;
    }
    let mut failures = Vec::new();
    for f in &files {
        let data = std::fs::read(f).unwrap();
        let swf = match ffdec::Swf::parse(&data) {
            Ok(s) => s,
            Err(ffdec::Error::BadSignature(_)) => continue,
            Err(e) => {
                failures.push(format!("{}: parse: {e}", f.display()));
                continue;
            }
        };
        ffdec::tree::resource_tree(&swf);
        ffdec::tree::tag_list(&swf);
        swf.walk(|_, t| {
            ffdec::info::describe(t);
        });
        let saved = swf.uncompressed_bytes().unwrap();
        let reference = ffdec::container::decode(&data).unwrap();
        let mut expected = saved[..8].to_vec();
        expected.extend_from_slice(&reference.body);
        expected.extend_from_slice(&reference.trailer);
        if saved != expected {
            failures.push(format!("{}: body differs", f.display()));
            continue;
        }
        if swf.header.compression == ffdec::Compression::None && !swf.header.encrypted && saved != data {
            failures.push(format!("{}: file differs", f.display()));
        }
        for c in [ffdec::Compression::Zlib, ffdec::Compression::Lzma] {
            let packed = swf.save(&ffdec::SaveOptions { compression: c, encrypt: false }).unwrap();
            let again = ffdec::Swf::parse(&packed).unwrap();
            if again.body_bytes() != swf.body_bytes() {
                failures.push(format!("{}: {c:?} recompression differs", f.display()));
            }
        }
    }
    eprintln!("{} files checked", files.len());
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
