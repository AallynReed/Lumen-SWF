//! Compiles the font cases listed in `LUMEN_FONT_CASES` (one directory per line, each with `src/Main.as`, an
//! optional `flags` file, and the mxmlc output `out.swf`) as the SDK's default `flex-config.xml` compiles them.

use std::path::{Path, PathBuf};

use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::compiler::fonts_configuration::FontsConfiguration;

const SDK: &str = r"S:\Desktop\Projects\Rust-FFDec\flex-sdk-bin\sdk";

fn swcs_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|r| r.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "swc")).collect()).unwrap_or_default();
    v.sort();
    v
}

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

/// The `-option=value` flags of `flags`, applied as mxmlc applies them after `flex-config.xml`.
fn apply_flags(c: &mut Configuration, case: &Path) {
    let Ok(text) = std::fs::read_to_string(case.join("flags")) else { return };
    for f in text.split_whitespace() {
        let (k, v) = f.split_once('=').unwrap_or((f, "true"));
        match k.trim_start_matches('-') {
            "compiler.fonts.advanced-anti-aliasing" | "compiler.fonts.flash-type" => c.flex.fonts.flash_type = v.eq_ignore_ascii_case("true"),
            "compiler.fonts.max-glyphs-per-face" => c.flex.fonts.max_glyphs_per_face = Some(v.to_string()),
            "compiler.fonts.max-cached-fonts" => c.flex.fonts.max_cached_fonts = Some(v.to_string()),
            "compiler.fonts.local-fonts-snapshot" => {
                let p = case.join(v);
                if p.exists() {
                    c.flex.fonts.local_fonts_snapshot = Some(crate::asc::flex2::compiler::transcoder::local_name(&p));
                }
            }
            "compiler.fonts.local-font-paths" => {
                let l = c.flex.fonts.local_font_paths.get_or_insert_with(Vec::new);
                l.clear();
                for p in v.split(',') {
                    let p = case.join(p);
                    if p.exists() {
                        l.push(crate::asc::flex2::compiler::transcoder::local_name(&p));
                    }
                }
            }
            "compatibility-version" | "compiler.mxml.compatibility-version" => {
                let n: Vec<i32> = v.split('.').map(|p| p.parse().unwrap_or(0)).collect();
                c.css.compatibility_version = Some((n.first().copied().unwrap_or(0), n.get(1).copied().unwrap_or(0), n.get(2).copied().unwrap_or(0)));
                if n.first().copied().unwrap_or(0) <= 3 {
                    c.css.qualified_type_selectors = false;
                }
            }
            "compiler.show-shadowed-device-font-warnings" => c.flex.show_shadowed_device_font_warnings = v.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
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
    c.flex.fonts = FontsConfiguration::flex_config_defaults();
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
    apply_flags(&mut c, case);
    c
}

fn run_case(case: PathBuf) {
    let main = case.join("src").join("Main.as");
    let _ = std::fs::remove_dir_all(case.join("rgen"));
    let configuration = configuration(&case);
    let handle = std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(move || {
            let _ = std::env::set_current_dir(&case);
            crate::asc::flex2::compiler::media::font_transcoder::register();
            crate::asc::flex2::compiler::toolkit::reset(configuration.warnings);
            let compiled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::asc::flex2::tools::mxmlc::mxmlc(configuration, &main, Vec::new())));
            let diagnostics = crate::asc::flex2::compiler::toolkit::take_diagnostics();
            let mut log: String = diagnostics.iter().map(|d| d.text.clone() + "\n\n").collect();
            let swf = match compiled {
                Ok(c) => c.movie.and_then(|m| crate::asc::flex2::swf::encode(&m, true).ok()),
                Err(e) => {
                    log.push_str(&format!("PANIC: {}\n", e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()));
                    None
                }
            };
            (case, swf, log)
        })
        .unwrap();
    let Ok((case, swf, log)) = handle.join() else { return };
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
fn font_cases() {
    let Ok(list) = std::env::var("LUMEN_FONT_CASES") else { return };
    let text = std::fs::read_to_string(&list).unwrap_or(list);
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        run_case(PathBuf::from(line));
    }
}

#[test]
fn flex_config_fonts_section() {
    let fw = Path::new(SDK).join("frameworks");
    let Ok(opts) = crate::asc::flex2::compiler::file_configurator::mxmlc_defaults(&fw) else { return };
    let want = FontsConfiguration::flex_config_defaults();
    assert_eq!(opts.flex.fonts.managers, want.managers);
    assert_eq!(opts.flex.fonts.max_cached_fonts, want.max_cached_fonts);
    assert_eq!(opts.flex.fonts.max_glyphs_per_face, want.max_glyphs_per_face);
    assert!(opts.flex.fonts.flash_type);
    assert!(opts.flex.fonts.local_fonts_snapshot.is_none() && opts.flex.fonts.local_font_paths.is_none());
}
