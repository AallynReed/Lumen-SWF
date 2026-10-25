//! AS3 source replacement through the mxmlc port (`lumen::edit::set_script_source`).

use std::path::{Path, PathBuf};

use lumen::abc::decomp::{entry_source, index::AbcIndex, SwfContext};
use lumen::abc::scripts::entries;
use lumen::abc::{tag_abc, Abc};
use lumen::Swf;

/// The DoABC tag defining `class`.
fn abc_tag(swf: &Swf, class: &str) -> Vec<usize> {
    let mut path = Vec::new();
    swf.walk(|p, t| {
        if let Some((d, _)) = tag_abc(t) {
            if path.is_empty() && Abc::parse(d).is_ok_and(|a| entries(&a).iter().any(|e| e.full_name() == class)) {
                path = p.to_vec();
            }
        }
    });
    path
}

/// The ABC with `File.as$N` namespace numbers dropped; they count the namespaces of the whole build.
fn without_file_numbers(bytes: &[u8]) -> Vec<u8> {
    let mut abc = Abc::parse(bytes).unwrap();
    for s in abc.constants.strings.iter_mut() {
        if let Some(k) = s.find(".as$") {
            let n = s[k + 4..].chars().take_while(char::is_ascii_digit).count();
            s.replace_range(k + 4..k + 4 + n, "");
        }
    }
    abc.to_bytes()
}

fn abc_at(swf: &Swf, path: &[usize]) -> Abc {
    Abc::parse(tag_abc(swf.tag(path).unwrap()).unwrap().0).unwrap()
}

fn replace(swf: &mut Swf, path: &[usize], class: &str, text: &str) -> Result<(), lumen::action::asm::ParseError> {
    let id = entries(&abc_at(swf, path)).into_iter().find(|e| e.full_name() == class).expect("class").id();
    lumen::edit::set_script_source(swf, path, &id, text)
}

const MAIN: &str = "package { import flash.display.Sprite; import a.Helper; public class Main extends Sprite { public function Main() { var h:Helper = new Helper(); h.n = Helper.twice(h.n); trace(h.label()); } } }";
const HELPER: &str = "package a { public class Helper { public static const NAME:String = \"helper\"; public var n:int = 2; public function Helper() {} public static function twice(x:int):int { return x * 2; } public function label():String { return NAME + n; } } }\nclass Hidden { }";

/// An application built by the mxmlc port with its bundled playerglobal: replacing either class with its own source
/// gives the ABC back unchanged but for file-internal namespace numbers.
#[test]
fn own_source_gives_the_same_abc() {
    let dir = std::env::temp_dir().join(format!("lumen_as3_replace_{}", std::process::id())).join("src");
    std::fs::create_dir_all(dir.join("a")).unwrap();
    std::fs::write(dir.join("Main.as"), MAIN).unwrap();
    std::fs::write(dir.join("a/Helper.as"), HELPER).unwrap();
    let opts = lumen::asc::flex2::ApplicationOptions {
        main: dir.join("Main.as"),
        source_path: vec![dir.clone()],
        playerglobal: true,
        omit_trace_statements: false,
        compile_date: Some(0),
        ..Default::default()
    };
    let app = lumen::asc::flex2::compile_application(&opts).unwrap_or_else(|d| panic!("{}", d.iter().map(|d| d.text.clone()).collect::<Vec<_>>().join("\n")));
    let swf = Swf::parse(&app.swf).unwrap();
    let path = abc_tag(&swf, "Main");
    let before = without_file_numbers(tag_abc(swf.tag(&path).unwrap()).unwrap().0);
    for (class, text) in [("a.Helper", HELPER), ("Main", MAIN)] {
        let mut edited = Swf::parse(&app.swf).unwrap();
        replace(&mut edited, &path, class, text).unwrap_or_else(|e| panic!("{class}: line {}: {}", e.line, e.message));
        assert_eq!(without_file_numbers(tag_abc(edited.tag(&path).unwrap()).unwrap().0), before, "{class}");
    }
}

#[test]
fn reports_mxmlc_errors_with_their_line() {
    let dir = std::env::temp_dir().join(format!("lumen_as3_replace_err_{}", std::process::id())).join("src");
    std::fs::create_dir_all(dir.join("a")).unwrap();
    std::fs::write(dir.join("Main.as"), MAIN).unwrap();
    std::fs::write(dir.join("a/Helper.as"), HELPER).unwrap();
    let opts = lumen::asc::flex2::ApplicationOptions { main: dir.join("Main.as"), source_path: vec![dir], playerglobal: true, ..Default::default() };
    let Ok(app) = lumen::asc::flex2::compile_application(&opts) else { panic!("build failed") };
    let mut swf = Swf::parse(&app.swf).unwrap();
    let path = abc_tag(&swf, "a.Helper");
    let text = "package a {\n public class Helper {\n  public function Helper() { nope = 1; }\n }\n}";
    let e = replace(&mut swf, &path, "a.Helper", text).err().expect("an error");
    assert_eq!(e.line, 3);
    assert!(e.message.contains("Helper.as(3): col: 30 Error: Access of undefined property nope."), "{}", e.message);
}

fn corpus(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libsrc/ffdec_lib/testdata").join(rel)
}

/// A class of a Flex debug build replaced with its own source still decompiles, at the same script index, and the
/// classes around it are untouched.
#[test]
fn replaces_a_class_of_a_flex_build() {
    let Ok(data) = std::fs::read(corpus("as3_new/bin/as3_new.flex.swf")) else { return };
    let mut swf = Swf::parse(&data).unwrap();
    let path = abc_tag(&swf, "tests.TestAndInt");
    let before = abc_at(&swf, &path);
    let ctx = SwfContext::of(&swf);
    let old_index = AbcIndex::for_abcs(&[&before]);
    let all = entries(&before);
    let target = all.iter().find(|e| e.full_name() == "tests.TestAndInt").expect("class");
    let text = std::fs::read_to_string(corpus("as3_new/src/tests/TestAndInt.as")).unwrap();
    replace(&mut swf, &path, "tests.TestAndInt", &text).unwrap_or_else(|e| panic!("line {}: {}", e.line, e.message));

    let saved = swf.save(&lumen::SaveOptions { compression: lumen::Compression::None, encrypt: false }).unwrap();
    let swf = Swf::parse(&saved).unwrap();
    let after = abc_at(&swf, &path);
    assert_eq!(after.scripts.len(), before.scripts.len());
    let new_index = AbcIndex::for_abcs(&[&after]);
    let replaced = entries(&after).into_iter().find(|e| e.full_name() == "tests.TestAndInt").unwrap();
    assert_eq!(replaced.script, target.script);
    let text = entry_source(&new_index, &after, &replaced, &SwfContext::of(&swf));
    // Release code, but the source's trace calls are kept.
    assert!(text.contains("public class TestAndInt") && text.contains("var _loc2_:int = 5;") && text.contains("trace(\"okay\")"), "{text}");
    for e in all.iter().filter(|e| e.script.abs_diff(target.script) == 1) {
        let n = entries(&after).into_iter().find(|x| x.full_name() == e.full_name()).unwrap();
        assert_eq!(entry_source(&new_index, &after, &n, &ctx), entry_source(&old_index, &before, e, &ctx), "{}", e.full_name());
    }
}
