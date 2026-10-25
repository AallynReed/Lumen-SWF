//! Compiles the embed cases listed in `LUMEN_EMBED_CASES` (one directory per line, each with `src/Main.as` and the
//! mxmlc output `out.swf` it is compared with) the way the SDK's default `flex-config.xml` compiles them.

use std::path::{Path, PathBuf};

use crate::asc::flex2::compiler::configuration::Configuration;

const SDK: &str = r"S:\Desktop\Projects\Rust-FFDec\flex-sdk-bin\sdk";

fn swcs_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|r| r.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "swc")).collect()).unwrap_or_default();
    v.sort();
    v
}

/// The metadata XML and `ProductInfo` compile date of the mxmlc output, so the two SWFs can match.
fn oracle_fields(swf: &[u8]) -> (Option<String>, Option<i64>) {
    let Ok(s) = crate::swf::Swf::parse(swf) else { return (None, None) };
    let mut md = None;
    let mut date = None;
    for t in &s.timeline.tags {
        if t.code == 77 {
            md = Some(String::from_utf8_lossy(&t.body[..t.body.len().saturating_sub(1)]).into_owned());
        }
        if t.code == 41 && t.body.len() >= 26 {
            date = Some(i64::from_le_bytes(t.body[18..26].try_into().unwrap()));
        }
    }
    (md, date)
}

fn configuration(case: &Path) -> Configuration {
    let fw = Path::new(SDK).join("frameworks");
    let mut c = Configuration::default();
    c.source_path = vec![case.join("src")];
    let mut lib = swcs_in(&fw.join("libs"));
    lib.extend(swcs_in(&fw.join("libs").join("mx")));
    lib.extend(swcs_in(&fw.join("locale").join("en_US")));
    lib.extend(swcs_in(&fw.join("libs").join("player").join("11.1")));
    c.library_path = lib;
    c.external_library_path = vec![fw.join("libs").join("player").join("11.1").join("playerglobal.swc")];
    c.keep_as3_metadata = Some(["Bindable", "Managed", "ChangeEvent", "NonCommittingChangeEvent", "Transient"].iter().map(|s| s.to_string()).collect());
    c.flex_major = 4;
    c.flex_minor = 8;
    c.flex_build = 1359417;
    c.generated_directory = Some(case.join("rgen").to_string_lossy().into_owned());
    if let Ok(o) = std::fs::read(case.join("out.swf")) {
        let mut b = o.clone();
        if b.first() == Some(&b'C') {
            let mut d = b[..8].to_vec();
            let _ = std::io::Read::read_to_end(&mut flate2::read::ZlibDecoder::new(&b[8..]), &mut d);
            b = d;
            b[0] = b'F';
        }
        let (md, date) = oracle_fields(&b);
        c.metadata = md;
        c.compile_date = date;
    }
    if case.join("debug").exists() {
        c.debug = true;
    }
    c
}

fn run_case(case: PathBuf) {
    let main = case.join("src").join("Main.as");
    let _ = std::fs::remove_dir_all(case.join("rgen"));
    let configuration = configuration(&case);
    let compress = true;
    let handle = std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(move || {
            crate::asc::flex2::compiler::toolkit::reset(configuration.warnings);
            let compiled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::asc::flex2::tools::mxmlc::mxmlc(configuration, &main, Vec::new())));
            let diagnostics = crate::asc::flex2::compiler::toolkit::take_diagnostics();
            let mut log: String = diagnostics.iter().map(|d| d.text.clone() + "\n\n").collect();
            let swf = match compiled {
                Ok(c) => c.movie.and_then(|m| crate::asc::flex2::swf::encode(&m, compress).ok()),
                Err(e) => {
                    log.push_str(&format!("PANIC: {}\n", e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()));
                    None
                }
            };
            (swf, log)
        })
        .unwrap();
    let (swf, log) = handle.join().unwrap_or((None, "thread died\n".to_string()));
    let _ = std::fs::write(case.join("rust.log"), log);
    match swf {
        Some(s) => {
            let _ = std::fs::write(case.join("rust.swf"), s);
        }
        None => {
            let _ = std::fs::remove_file(case.join("rust.swf"));
        }
    }
}

#[test]
fn embed_cases() {
    let Ok(list) = std::env::var("LUMEN_EMBED_CASES") else { return };
    let text = std::fs::read_to_string(&list).unwrap_or(list);
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        run_case(PathBuf::from(line));
    }
}

#[test]
fn embed_class_template_matches_mxmlc() {
    use super::velocity_boundary::{merge_embed_class, EmbedClassContext};
    let c = EmbedClassContext {
        package_name: String::new(),
        base_class: Some("mx.core.SpriteAsset".into()),
        asset_type: Some("flash.display.DisplayObject".into()),
        embed_class: "Main_v0".into(),
        embed_map: vec![("symbol".into(), "lib.Media".into()), ("_line".into(), "5".into())],
        embed_props: vec![("vid".into(), "flash.display.DisplayObject".into())],
    };
    let want = "\n\npackage \n{\n\nimport mx.core.SpriteAsset;\nimport flash.display.DisplayObject;\n\n[ExcludeClass]\n[Embed(symbol=\"lib.Media\", _line=\"5\")]\n\npublic class Main_v0 extends mx.core.SpriteAsset \n{\n    public function Main_v0() \n    { \n\t    super(); \n    }\n\n    public var vid:flash.display.DisplayObject;\n}\n\n}\n";
    assert_eq!(merge_embed_class(&c).as_deref(), Some(want));
}
