//! Style code generation driven by the spec files of the Java `StyleGen` harness.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use super::css_file::{self, CssFile};
use super::document::CssConfiguration;
use super::styles_container::StylesContainer;
use crate::asc::flex2::compiler::source::{Owner, Source, VirtualFile};
use crate::asc::flex2::compiler::swc::Swc;
use crate::asc::flex2::compiler::util::name_mappings::get_name_mappings;
use crate::asc::util::context::ContextStatics;

fn file(kind: &str, a: &str, b: Option<&str>) -> CssFile {
    if kind == "zip" {
        CssFile::Zip { swc: a.to_string(), entry: b.unwrap_or_default().to_string() }
    } else {
        CssFile::local(Path::new(a))
    }
}

/// Runs a `StyleGen` spec; returns what the harness writes.
pub fn run_spec(spec: &str) -> (String, String) {
    let mut config = CssConfiguration { keep_generated_actionscript: true, ..Default::default() };
    let (mut class_name, mut root, mut out) = (String::new(), String::new(), String::new());
    let mut def_names = Vec::new();
    let mut manifests: Vec<(String, Vec<std::path::PathBuf>)> = Vec::new();
    let mut swcs = Vec::new();
    for line in spec.lines() {
        let p: Vec<&str> = line.split('\t').collect();
        match p[0] {
            "class" => class_name = p[1].to_string(),
            "root" => root = p[1].to_string(),
            "out" => out = p[1].to_string(),
            "gendir" => config.generated_directory = Some(p[1].to_string()),
            "css" => config.defaults_css_files.push(file(p[1], p[2], p.get(3).copied())),
            "theme" => config.theme_css_files.push(file(p[1], p[2], p.get(3).copied())),
            "defaultsurl" => config.defaults_css_url = Some(file(p[1], p[2], p.get(3).copied())),
            "ns" => manifests.push((p[1].to_string(), vec![p[2].into()])),
            "def" => def_names.push(p[1].to_string()),
            "swc" => swcs.push(Swc::load(Path::new(p[1])).unwrap()),
            "flag" => match p[1] {
                "getAllowDuplicateDefaultStyleDeclarations" => config.allow_duplicate_default_style_declarations = p[2] == "true",
                "keepAllTypeSelectors" => config.keep_all_type_selectors = p[2] == "true",
                "getCompatibilityVersion" => config.compatibility_version = i32::from_str_radix(p[2].trim_start_matches("0x"), 16).unwrap(),
                "getQualifiedTypeSelectors" => config.qualified_type_selectors = p[2] == "true",
                _ => {}
            },
            _ => {}
        }
    }
    css_file::set_swcs(swcs);
    let mappings = Rc::new(get_name_mappings(&manifests));
    let per_compile_data = Rc::new(RefCell::new(ContextStatics::new()));
    let source = Source::new(VirtualFile::local(Path::new(&root)), None, "", &class_name, Owner::None, false, true, false);
    let unit = Source::new_compilation_unit(&source, None);
    let sc = StylesContainer::new(Rc::new(config), unit.clone(), per_compile_data);
    sc.set_name_mappings(Some(mappings));
    sc.load_default_styles();
    let (_, extra) = sc.process_dependencies(&def_names, &|_| None, Some(""), Some(&class_name));
    let mut b = String::new();
    for s in extra {
        let super::styles_container::StyleSource::New(s) = s else { continue };
        let name = Path::new(&s.name).file_name().unwrap().to_string_lossy().into_owned();
        b.push_str(&format!("=== {name}\n{}", s.code));
    }
    let mixins: Vec<String> = unit.borrow().mixins.iter().cloned().collect();
    b.push_str(&format!("=== mixins [{}]\n", mixins.join(", ")));
    (out, b)
}

/// `LUMEN_STYLEGEN_SPECS=<spec>;<spec>...` writes `<spec>.rust` for each.
#[test]
fn style_gen_specs() {
    let specs = match (std::env::var("LUMEN_STYLEGEN_SPECS"), std::env::var("LUMEN_STYLEGEN_LIST")) {
        (Ok(s), _) => s.split(';').map(str::to_string).collect::<Vec<_>>(),
        (_, Ok(l)) => std::fs::read_to_string(l).unwrap().lines().map(str::to_string).collect(),
        _ => return,
    };
    for spec in specs.iter().filter(|s| !s.is_empty()) {
        crate::asc::flex2::compiler::toolkit::reset(true);
        let text = std::fs::read_to_string(spec).unwrap();
        let r = std::panic::catch_unwind(|| run_spec(&text));
        let diags = crate::asc::flex2::compiler::toolkit::take_diagnostics();
        let mut log = String::new();
        for d in diags {
            log.push_str(&d.text);
            log.push_str("\n\n");
        }
        if let Ok((_, b)) = r {
            std::fs::write(format!("{spec}.rust"), b).unwrap();
        }
        std::fs::write(format!("{spec}.rust.log"), log).unwrap();
    }
}

/// `LUMEN_CSS_APP=<App.as>;<generated dir>` compiles a Flex application against the SDK with its default
/// `flex-config.xml` style options, keeping the generated sources.
#[test]
fn css_app_compile() {
    let Ok(spec) = std::env::var("LUMEN_CSS_APP") else { return };
    let mut parts = spec.split(';');
    let main = std::path::PathBuf::from(parts.next().unwrap());
    let gen = std::path::PathBuf::from(parts.next().unwrap());
    let fw = std::path::PathBuf::from(r"S:\Desktop\Projects\Rust-FFDec\flex-sdk-bin\sdk\frameworks");
    let theme = std::env::var("LUMEN_CSS_THEME").map_or_else(|_| vec![fw.join(r"themes\Spark\spark.css")], |t| t.split(';').map(std::path::PathBuf::from).collect());
    let mut css = super::options::CssOptions { theme, defaults_css_url: std::env::var("LUMEN_CSS_DEFAULTS_URL").ok().map(std::path::PathBuf::from), ..Default::default() };
    for (uri, m) in [("http://ns.adobe.com/mxml/2009", "mxml-2009-manifest.xml"), ("library://ns.adobe.com/flex/spark", "spark-manifest.xml"), ("library://ns.adobe.com/flex/mx", "mx-manifest.xml"), ("http://www.adobe.com/2006/mxml", "mxml-manifest.xml")] {
        css.namespaces.push((uri.to_string(), vec![fw.join(m)]));
    }
    let pure = std::env::var("LUMEN_CSS_APP_PURE").is_ok();
    if pure {
        css = Default::default();
    }
    let opts = crate::asc::flex2::ApplicationOptions {
        main: main.clone(),
        source_path: vec![main.parent().unwrap().to_path_buf()],
        library_path: [fw.join("libs"), fw.join(r"libs\mx"), fw.join(r"locale\en_US")]
            .iter()
            .flat_map(|d| {
                let mut v: Vec<_> = std::fs::read_dir(d).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "swc")).collect();
                v.sort();
                v
            })
            .filter(|_| !pure)
            .collect(),
        external_library_path: vec![fw.join(r"libs\player\11.1\playerglobal.swc")],
        target_player: "11.1".into(),
        swf_version: 14,
        compile_date: Some(0),
        optimize: std::env::var("LUMEN_CSS_APP_OPTIMIZE").map_or(true, |v| v == "1"),
        generated_directory: Some(gen),
        css,
        ..Default::default()
    };
    match crate::asc::flex2::compile_application(&opts) {
        Ok(app) => {
            for d in &app.diagnostics {
                eprintln!("{}", d.text);
            }
            eprintln!("swf {} bytes, {} units", app.swf.len(), app.units.len());
            let dir = opts.generated_directory.as_ref().unwrap().join("units");
            std::fs::create_dir_all(&dir).unwrap();
            for (name, abc) in &app.units {
                let n = std::path::Path::new(name).file_stem().unwrap().to_string_lossy().into_owned();
                std::fs::write(dir.join(format!("{n}.abc")), abc).unwrap();
            }
            std::fs::write(opts.generated_directory.as_ref().unwrap().join("out.swf"), &app.swf).unwrap();
        }
        Err(d) => {
            eprintln!("compile failed with {} diagnostics", d.len());
            for d in d.iter().take(20) {
                eprintln!("{}
", d.text);
            }
        }
    }
}

#[test]
fn extracts_and_generates_a_style_module() {
    let dir = std::env::temp_dir().join(format!("lumen_css_module_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a.css");
    std::fs::write(&path, "global { font-size: 12 }\n.big, .huge { color: #f00; rollOverEffect: Fade }\n").unwrap();
    crate::asc::flex2::compiler::toolkit::reset(true);
    let source = Source::new(VirtualFile::local(&path), None, "", "a", Owner::None, false, true, false);
    let name = source.borrow().get_name().to_string();
    let mut sheet = crate::asc::flex2::flash::css::style_sheet::StyleSheet::new();
    assert!(sheet.parse(&name, &std::fs::read(&path).unwrap()));
    let mut module = super::style_module::StyleModule::new(source.clone(), Rc::new(RefCell::new(ContextStatics::new())));
    module.set_name("a".into());
    let module = super::style_module::PlainStyleModule(RefCell::new(module));
    use super::style_module::StyleModuleOps;
    module.extract_styles(Some(&sheet), false).unwrap();
    let defs = module.module().get_style_defs();
    assert_eq!(defs.iter().map(|d| d.borrow().get_subject().to_string()).collect::<Vec<_>>(), ["global"]);
    let code = super::template_context::merge("flex2/compiler/css/StyleModule.vm", false, vec![("styleModule", super::template_context::style_module(&module.module()))], "a").unwrap();
    assert!(code.contains("public class a extends ModuleBase implements IStyleModule"), "{code}");
    assert!(code.contains("style.mx_internal::setLocalStyle('fontSize', 12);"), "{code}");
    assert!(crate::asc::flex2::compiler::toolkit::take_diagnostics().is_empty());
}
