use std::path::Path;

use lumen::bundle::{self, Kind};
use lumen::{Compression, SaveOptions, Swf};

fn corpus(rel: &str) -> Option<Vec<u8>> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus").join(rel)).ok()
}

#[test]
fn opens_zip_and_swc() {
    let Some(zip) = corpus("libsrc/ffdec_lib/testdata/zipped.zip") else { return };
    let b = bundle::open(Path::new("zipped.zip"), &zip).unwrap();
    assert_eq!(b.kind, Kind::Zip);
    assert_eq!(b.entries.iter().map(|e| e.0.as_str()).collect::<Vec<_>>(), ["embedded.swf"]);
    let swf = Swf::parse(&b.entries[0].1).unwrap();

    let saved = swf.save(&SaveOptions { compression: Compression::None, encrypt: false }).unwrap();
    let out = bundle::replace(&zip, "embedded.swf", &saved).unwrap();
    let again = bundle::open(Path::new("zipped.zip"), &out).unwrap();
    assert_eq!(again.entries[0].1, saved);

    let Some(swc) = corpus("resources/flashlib/playerglobal32_0.swc") else { return };
    let b = bundle::open(Path::new("playerglobal.swc"), &swc).unwrap();
    assert_eq!(b.entries.iter().map(|e| e.0.as_str()).collect::<Vec<_>>(), ["library.swf"]);
    assert!(Swf::parse(&b.entries[0].1).unwrap().is_as3());
}

#[test]
fn finds_embedded_swfs() {
    let Some(zip) = corpus("libsrc/ffdec_lib/testdata/zipped.zip") else { return };
    let b = bundle::open(Path::new("zipped.zip"), &zip).unwrap();
    let swf = Swf::parse(&b.entries[0].1).unwrap();
    let fws = swf.save(&SaveOptions { compression: Compression::None, encrypt: false }).unwrap();
    let cws = swf.save(&SaveOptions { compression: Compression::Zlib, encrypt: false }).unwrap();
    let zws = swf.save(&SaveOptions { compression: Compression::Lzma, encrypt: false }).unwrap();
    let mut file = b"MZ junk FWS CWS".to_vec();
    let mut at = Vec::new();
    for s in [&fws, &cws, &zws] {
        at.push(format!("[{}]", file.len()));
        file.extend_from_slice(s);
        file.extend_from_slice(b"padding");
    }
    let found = bundle::open(Path::new("game.exe"), &file).unwrap();
    assert_eq!(found.kind, Kind::Binary);
    let keys: Vec<_> = found.entries.iter().map(|e| e.0.clone()).collect();
    for k in &at {
        assert!(keys.contains(k), "{k} not in {keys:?}");
    }
    assert_eq!(found.entries.iter().find(|e| e.0 == at[1]).unwrap().1, cws);
}
