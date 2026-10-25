use std::path::Path;

use lumen::xfl::{convert, FlaVersion, Options};
use lumen::Swf;

fn corpus(rel: &str) -> Option<Vec<u8>> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus").join(rel)).ok()
}

fn file<'a>(out: &'a lumen::xfl::convert::Output, name: &str) -> &'a str {
    std::str::from_utf8(&out.fla.iter().find(|f| f.0 == name).unwrap().1).unwrap()
}

#[test]
fn converts_timelines_and_tweens() {
    let Some(data) = corpus("libsrc/ffdec_lib/testdata/morphshape_ease/morphshape_ease.swf") else { return };
    let swf = Swf::parse(&data).unwrap();
    let opts = Options { version: if swf.is_as3() { FlaVersion::Cc } else { FlaVersion::Cs6 }, timestamp: Some(0), ..Default::default() };
    let out = convert(&swf, "movie", &opts).unwrap();
    let dom = file(&out, "DOMDocument.xml");
    assert!(dom.starts_with("<DOMDocument xmlns=\"http://ns.adobe.com/xfl/2008/\""));
    assert!(dom.contains("tweenType=\"shape\""));
    assert!(file(&out, "PublishSettings.xml").contains("<flashFileName>movie.swf</flashFileName>"));

    let Some(data) = corpus("libsrc/ffdec_lib/testdata/motion_object/41_ease_sine/reference.swf") else { return };
    let swf = Swf::parse(&data).unwrap();
    let opts = Options { version: if swf.is_as3() { FlaVersion::Cc } else { FlaVersion::Cs6 }, timestamp: Some(0), ..Default::default() };
    let out = convert(&swf, "movie", &opts).unwrap();
    assert!(file(&out, "DOMDocument.xml").contains("tweenType=\"motion object\""));
}

/// Compares with reference output: `LUMEN_XFL_LIST` lists pairs of lines, a reference folder
/// (holding `movie/`) and its SWF.
#[test]
fn matches_reference_output() {
    let Ok(list) = std::env::var("LUMEN_XFL_LIST") else { return };
    let text = std::fs::read_to_string(list).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let norm = |s: &str| {
        let re = regex::Regex::new(r#"(itemID|lastModified|sourceLastImported|creatorInfo|versionInfo|majorVersion)="[^"]*"|(M \d+) \d+\.dat|(\.png"[^>]*?)externalFileSize="\d+""#).unwrap();
        re.replace_all(s, "$2$3").into_owned()
    };
    let (mut same, mut total) = (0, 0);
    for pair in lines.chunks(2) {
        let [refdir, swf] = pair else { continue };
        let dir = Path::new(refdir).join("movie");
        let Ok(expected) = std::fs::read_to_string(dir.join("DOMDocument.xml")) else { continue };
        let swf = Swf::parse(&std::fs::read(swf).unwrap()).unwrap();
        let version = if swf.is_as3() { FlaVersion::Cc } else { FlaVersion::Cs6 };
        let out = convert(&swf, "movie", &Options { version, ..Default::default() }).unwrap();
        total += 1;
        let (a, b) = (norm(file(&out, "DOMDocument.xml")), norm(&expected));
        if a == b {
            same += 1;
        } else {
            let i = a.bytes().zip(b.bytes()).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
            eprintln!("differs: {refdir} at {i}: {:?} vs {:?}", &a[i.saturating_sub(60)..(i + 20).min(a.len())], &b[i.saturating_sub(60)..(i + 20).min(b.len())]);
        }
    }
    eprintln!("{same} / {total} DOMDocument.xml match");
}
