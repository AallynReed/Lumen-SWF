//! AMF reading, writing and text, checked on the JPEXS test data. With `FFDEC_AMF_TEXT` set to a
//! folder of FFDec's own text (name.txt, dates in UTC), the text must match it too.

use std::path::{Path, PathBuf};

use ffdec::amf::{self, Sol};

fn data(dir: &str) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../jpexs-decompiler/libsrc/ffdec_lib/testdata").join(dir);
    let mut v: Vec<PathBuf> = std::fs::read_dir(root).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    v.sort();
    v
}

/// Our text with integral doubles printed the way FFDec prints them.
fn as_ffdec(s: &str) -> String {
    regex::Regex::new(r"(\d)\.0\b").unwrap().replace_all(s, "$1").into_owned()
}

fn expect(name: &str, ours: &str) {
    let Ok(dir) = std::env::var("FFDEC_AMF_TEXT") else { return };
    if let Ok(out) = std::env::var("FFDEC_AMF_OUT") {
        std::fs::write(Path::new(&out).join(format!("{name}.txt")), as_ffdec(ours)).unwrap();
        return;
    }
    let want = std::fs::read_to_string(Path::new(&dir).join(format!("{name}.txt"))).unwrap();
    // FFDec merges amf0test's duplicate keys and truncates an object at a member it can not read.
    if name == "amf0test.sol" || name == "noserializer_object_sealed.bin" {
        return;
    }
    if want.starts_with("ERROR") {
        assert!(ours.starts_with("ERROR"), "{name}: FFDec failed but we read it");
    } else {
        assert_eq!(as_ffdec(ours), want, "{name}");
    }
}

#[test]
fn amf3_values_round_trip() {
    for f in data("amf3/generated") {
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        let bytes = std::fs::read(&f).unwrap();
        match amf::read_amf3(&bytes) {
            Ok((a, v)) => {
                let text = amf::to_text(&a, &v);
                expect(&name, &text);
                assert_eq!(amf::write_amf3(&a, &v).unwrap(), bytes, "{name}: write");
                let (a2, v2) = amf::from_text(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
                assert_eq!(amf::write_amf3(&a2, &v2).unwrap(), bytes, "{name}: text round trip");
            }
            Err(e) => expect(&name, &format!("ERROR {e}")),
        }
    }
}

#[test]
fn sol_files_round_trip() {
    for f in data("sharedobjects/data") {
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        let bytes = std::fs::read(&f).unwrap();
        match Sol::parse(&bytes) {
            Ok(sol) => {
                let text = sol.to_text();
                expect(&name, &text);
                assert_eq!(sol.write().unwrap(), bytes, "{name}: write");
                let mut again = sol.clone();
                again.set_text(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
                assert_eq!(again.write().unwrap(), bytes, "{name}: text round trip");
            }
            Err(e) => expect(&name, &format!("ERROR {e}")),
        }
    }
}

#[test]
fn place_object4_shows_its_metadata() {
    let Some(simple) = data("amf3/generated").into_iter().find(|p| p.ends_with("simple.bin")) else { return };
    let mut body = vec![0x02, 0x00, 1, 0, 7, 0];
    body.extend(std::fs::read(simple).unwrap());
    let fields = ffdec::info::describe(&ffdec::Tag::new(94, body));
    let amf = fields.iter().find(|f| f.name == "AMF data").expect("AMF data field");
    assert!(amf.value.contains("\"b\": 29"), "{}", amf.value);
}
