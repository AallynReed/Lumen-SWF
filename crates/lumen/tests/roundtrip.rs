mod common;

/// Unmodified files must decompress and re-save byte for byte.
#[test]
fn corpus_roundtrip() {
    let files = common::swf_files();
    if files.is_empty() {
        eprintln!("test corpus not found; skipping");
        return;
    }
    let failures: Vec<String> = common::par_map(&files, |f| {
        let mut failures = Vec::new();
        let data = std::fs::read(f).unwrap();
        let swf = match lumen::Swf::parse(&data) {
            Ok(s) => s,
            Err(lumen::Error::BadSignature(_)) => return failures,
            Err(e) => {
                failures.push(format!("{}: parse: {e}", f.display()));
                return failures;
            }
        };
        lumen::tree::resource_tree(&swf);
        lumen::tree::tag_list(&swf);
        swf.walk(|_, t| {
            lumen::info::describe(t);
        });
        let saved = swf.uncompressed_bytes().unwrap();
        let reference = lumen::container::decode(&data).unwrap();
        let mut expected = saved[..8].to_vec();
        expected.extend_from_slice(&reference.body);
        expected.extend_from_slice(&reference.trailer);
        if saved != expected {
            failures.push(format!("{}: body differs", f.display()));
            return failures;
        }
        if swf.header.compression == lumen::Compression::None && !swf.header.encrypted && saved != data {
            failures.push(format!("{}: file differs", f.display()));
        }
        for c in [lumen::Compression::Zlib, lumen::Compression::Lzma] {
            let packed = swf.save(&lumen::SaveOptions { compression: c, encrypt: false }).unwrap();
            let again = lumen::Swf::parse(&packed).unwrap();
            if again.body_bytes() != swf.body_bytes() {
                failures.push(format!("{}: {c:?} recompression differs", f.display()));
            }
        }
        failures
    })
    .concat();
    eprintln!("{} files checked", files.len());
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
