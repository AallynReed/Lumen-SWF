use std::path::{Path, PathBuf};

use ffdec::text::{formatted, set_formatted, StaticText};

/// Compares formatted text and its re-import with JPEXS 26.4, read from `FFDEC_TEXT_REF` (the
/// TextDump driver's output: one folder per SWF with swf.txt, id.txt and id.bin).
#[test]
fn texts_match_jpexs() {
    let Ok(dir) = std::env::var("FFDEC_TEXT_REF") else { return };
    let (mut total, mut bad) = (0, Vec::new());
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
    dirs.sort();
    for d in dirs {
        let Ok(swf_path) = std::fs::read_to_string(d.join("swf.txt")) else { continue };
        let Ok(data) = std::fs::read(swf_path.trim()) else { continue };
        let Ok(swf) = ffdec::Swf::parse(&data) else { continue };
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            let Some(id) = p.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<u16>().ok()) else { continue };
            if p.extension().is_none_or(|x| x != "txt") {
                continue;
            }
            total += 1;
            let label = format!("{} {id}", Path::new(swf_path.trim()).file_name().unwrap().to_string_lossy());
            let expected = std::fs::read_to_string(&p).unwrap().replace("\r\n", "\n");
            let tag = swf.tag(&swf.find_character(id).unwrap()).unwrap();
            let mut t = StaticText::read(tag).unwrap();
            let got = formatted(&swf, &t, false);
            if got != expected {
                let at = got.chars().zip(expected.chars()).position(|(a, b)| a != b).unwrap_or(got.len().min(expected.len()));
                bad.push(format!("{label}: text differs at {at}: got {:?} want {:?}", got.chars().skip(at.saturating_sub(20)).take(60).collect::<String>(), expected.chars().skip(at.saturating_sub(20)).take(60).collect::<String>()));
                continue;
            }
            let Ok(bin) = std::fs::read(p.with_extension("bin")) else { continue };
            match set_formatted(&swf, &mut t, &expected, None) {
                Ok(()) if t.write() != bin => bad.push(format!("{label}: body differs")),
                Ok(()) => {}
                Err(e) => bad.push(format!("{label}: {e}")),
            }
        }
    }
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("text_report.txt"), bad.join("\n")).unwrap();
    eprintln!("text: {}/{total} ok", total - bad.len());
    assert!(bad.is_empty(), "{} texts differ", bad.len());
}

fn template() -> Option<ffdec::Swf> {
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../jpexs-decompiler/libsrc/ffdec_lib/testdata/swt_templates/f5_template.swf");
    ffdec::Swf::parse(&std::fs::read(f).ok()?).ok()
}

fn plain(swf: &ffdec::Swf, id: u16) -> Vec<String> {
    let tag = swf.tag(&swf.find_character(id).unwrap()).unwrap();
    let mut fonts = std::collections::HashMap::new();
    swf.walk(|_, t| {
        if let Ok(f) = ffdec::export::font::Font::read(swf, t) {
            fonts.insert(f.id, f);
        }
    });
    ffdec::export::text::records(tag, &fonts).unwrap()
}

#[test]
fn plain_records_replace_the_text_runs() {
    let Some(mut swf) = template() else { return };
    let path = swf.find_character(7).unwrap();
    let mut records = plain(&swf, 7);
    records[0] = "Textnormal".into();
    ffdec::importers::import(&mut swf, &path, records.join("\r\n--- RECORDSEPARATOR ---\r\n").as_bytes()).unwrap();
    assert_eq!(plain(&swf, 7), records);
}

#[test]
fn formatted_errors_name_their_line() {
    let Some(mut swf) = template() else { return };
    let path = swf.find_character(7).unwrap();
    let e = ffdec::importers::import(&mut swf, &path, b"[\nxmin 0\nfont 4\nheight 240\n]normal\xc2\xa4").unwrap_err();
    assert!(e.to_string().contains("line 5") && e.to_string().contains("no character"), "{e}");
    let e = ffdec::text::import(&mut swf, &path, "[\nxmin 0\ncolor red\n]").unwrap_err();
    assert_eq!(e.line, 3);
}
