//! Port of mxmlc's driver (Flex SDK 4.8.0 `flex2.compiler`, `flex2.linker`, `flex2.tools`) for pure-ActionScript
//! builds: the same units compiled in the same order against the same libraries, linked into the same SWF.

pub mod batik;
pub mod compiler;
pub mod flash;
pub mod linker;
pub mod swf;
pub mod tools;
pub mod velocity;

use std::path::PathBuf;

pub use compiler::toolkit::{Diagnostic, Level};

/// The mxmlc options a pure-ActionScript build uses, with mxmlc's defaults.
#[derive(Clone, Debug)]
pub struct ApplicationOptions {
    pub main: PathBuf,
    pub source_path: Vec<PathBuf>,
    pub library_path: Vec<PathBuf>,
    pub external_library_path: Vec<PathBuf>,
    /// `-target-player`, e.g. `"11.2"`.
    pub target_player: String,
    pub swf_version: i32,
    pub default_size: (i32, i32),
    pub default_frame_rate: i32,
    pub default_background_color: i32,
    pub debug: bool,
    pub strict: bool,
    pub optimize: bool,
    pub omit_trace_statements: bool,
    pub warnings: bool,
    /// `-define=NS::name,value`.
    pub defines: Vec<(String, String)>,
    /// `ProductInfo`'s compile date in milliseconds; `None` uses the clock.
    pub compile_date: Option<i64>,
    /// Write a `CWS` file as mxmlc does; false writes `FWS`.
    pub compress: bool,
    /// Compile against the crate's bundled `playerglobal` as an external library.
    pub playerglobal: bool,
    /// `-keep-generated-actionscript`: where generated sources are written.
    pub generated_directory: Option<PathBuf>,
    /// The style options: `-defaults-css-url`, `-theme`, the namespace manifests, ...
    pub css: compiler::css::options::CssOptions,
}

impl Default for ApplicationOptions {
    fn default() -> Self {
        ApplicationOptions {
            main: PathBuf::new(),
            source_path: Vec::new(),
            library_path: Vec::new(),
            external_library_path: Vec::new(),
            target_player: "11.1".to_string(),
            swf_version: 14,
            default_size: (500, 375),
            default_frame_rate: 24,
            default_background_color: 0xFFFFFF,
            debug: false,
            strict: true,
            optimize: true,
            omit_trace_statements: true,
            warnings: true,
            defines: Vec::new(),
            compile_date: None,
            compress: true,
            playerglobal: false,
            generated_directory: None,
            css: Default::default(),
        }
    }
}

/// A compiled application.
pub struct Application {
    pub swf: Vec<u8>,
    /// Warnings, in mxmlc's order.
    pub diagnostics: Vec<Diagnostic>,
    /// Each compiled source with its ABC before linking, in compile order.
    pub units: Vec<(String, Vec<u8>)>,
}

impl ApplicationOptions {
    pub fn configuration(&self) -> compiler::configuration::Configuration {
        let mut c = compiler::configuration::Configuration::default();
        let mut tp = self.target_player.split('.').map(|p| p.parse::<i32>().unwrap_or(0));
        c.target_player_major = tp.next().unwrap_or(11);
        c.target_player_minor = tp.next().unwrap_or(0);
        c.swf_version = self.swf_version;
        c.default_width = self.default_size.0;
        c.default_height = self.default_size.1;
        c.frame_rate = self.default_frame_rate;
        c.background_color = self.default_background_color;
        c.debug = self.debug;
        c.strict = self.strict;
        c.optimize = self.optimize;
        c.omit_trace_statements = self.omit_trace_statements;
        c.warnings = self.warnings;
        c.source_path = self.source_path.clone();
        c.library_path = self.library_path.clone();
        c.external_library_path = self.external_library_path.clone();
        c.defines = self
            .defines
            .iter()
            .map(|(name, value)| {
                let (ns, n) = name.split_once("::").unwrap_or(("", name.as_str()));
                crate::asc::util::context::ConfigVar { ns: ns.to_string(), name: n.to_string(), value: value.clone() }
            })
            .collect();
        c.compile_date = self.compile_date;
        c.generated_directory = self.generated_directory.as_ref().map(|p| p.to_string_lossy().into_owned());
        c.css = self.css.clone();
        c
    }
}

/// Runs `f` on a thread with a stack deep enough for the compiler's recursion, turning a panic into an error.
fn on_compiler_thread<T: Send + 'static>(f: impl FnOnce() -> Result<T, Vec<Diagnostic>> + Send + 'static) -> Result<T, Vec<Diagnostic>> {
    let handle = std::thread::Builder::new().stack_size(1 << 30).spawn(move || {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
            Ok(r) => r,
            Err(e) => {
                let msg = e.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| e.downcast_ref::<String>().cloned()).unwrap_or_default();
                let mut d = compiler::toolkit::take_diagnostics();
                d.push(Diagnostic { level: Level::Error, path: None, line: -1, col: -1, message: msg.clone(), code: -1, text: format!("Error: {msg}") });
                Err(d)
            }
        }
    });
    match handle {
        Ok(h) => h.join().unwrap_or_else(|_| Err(Vec::new())),
        Err(e) => Err(vec![Diagnostic { level: Level::Error, path: None, line: -1, col: -1, message: e.to_string(), code: -1, text: e.to_string() }]),
    }
}

/// Compiles an application as `mxmlc` does for a `.as` root, returning the SWF or mxmlc's errors.
pub fn compile_application(opts: &ApplicationOptions) -> Result<Application, Vec<Diagnostic>> {
    let opts = opts.clone();
    on_compiler_thread(move || {
        let configuration = opts.configuration();
        compiler::toolkit::reset(configuration.warnings);
        let libraries = if opts.playerglobal { vec![(compiler::abc_library::playerglobal(), true)] } else { Vec::new() };
        let compiled = tools::mxmlc::mxmlc(configuration, &opts.main, libraries);
        let diagnostics = compiler::toolkit::take_diagnostics();
        let errors = diagnostics.iter().any(|d| d.level == Level::Error);
        let units = compiled
            .units
            .iter()
            .filter(|u| !u.borrow().get_source().borrow().is_swc_script_owner())
            .map(|u| {
                let u = u.borrow();
                (u.get_source().borrow().get_name().to_string(), u.get_byte_codes().to_vec())
            })
            .collect();
        match compiled.movie {
            Some(movie) if !errors => match swf::encode(&movie, opts.compress) {
                Ok(swf) => Ok(Application { swf, diagnostics, units }),
                Err(e) => Err(vec![Diagnostic { level: Level::Error, path: None, line: -1, col: -1, message: e.to_string(), code: -1, text: e.to_string() }]),
            },
            _ => Err(diagnostics),
        }
    })
}

/// One ActionScript source compiled against a set of existing ABC blocks (e.g. the classes of a SWF being edited), as
/// mxmlc would compile it among them.
#[derive(Clone, Debug, Default)]
pub struct UnitOptions {
    pub source: String,
    /// The path the source is reported under; relative to `root`, it gives the package.
    pub path: PathBuf,
    pub root: PathBuf,
    /// ABC blocks whose definitions the source compiles against; they are not part of the result.
    pub abcs: Vec<Vec<u8>>,
    /// `ns:name` definitions dropped from `abcs`, such as the class the source replaces.
    pub exclude: Vec<String>,
    /// `abcs` were compiled in the same build as the source, as when it replaces one of their classes: their code's
    /// references to it then act as a referencing source's would, which inherits the unit's slots, and the unit is
    /// generated only after them, as it was after the sources that pulled it in.
    pub same_build: bool,
    /// Classes (`pkg.Name`) whose class-level `[Embed]` keeps the symbol the target SWF already binds them to.
    pub keep_embedded: Vec<String>,
    pub source_path: Vec<PathBuf>,
    pub library_path: Vec<PathBuf>,
    pub external_library_path: Vec<PathBuf>,
    pub app: ApplicationOptions,
}

pub struct CompiledUnit {
    pub definitions: Vec<String>,
    /// The unit's ABC as mxmlc emits it, before linking.
    pub abc: Vec<u8>,
    /// The same ABC after `PostLink`'s merge pass on its own (debug info and metadata as a release link keeps them).
    pub optimized: Vec<u8>,
    /// Other sources pulled in from `source_path`.
    pub dependencies: Vec<(String, Vec<u8>)>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Compiles `opts.source` as `mxmlc` compiles a source among others, without linking.
pub fn compile_unit(opts: &UnitOptions) -> Result<CompiledUnit, Vec<Diagnostic>> {
    let opts = opts.clone();
    on_compiler_thread(move || {
        let mut configuration = opts.app.configuration();
        configuration.source_path = opts.source_path.clone();
        configuration.library_path = opts.library_path.clone();
        configuration.external_library_path = opts.external_library_path.clone();
        compiler::toolkit::reset(configuration.warnings);
        let library = match compiler::abc_library::library("abc", &opts.abcs, &opts.exclude, opts.same_build) {
            Ok(l) => l,
            Err(e) => return Err(vec![Diagnostic { level: Level::Error, path: None, line: -1, col: -1, message: e.clone(), code: -1, text: format!("Error: {e}") }]),
        };
        let name = compiler::source::canonical_path(&opts.path);
        let root_dir = compiler::source::canonical_path(&opts.root);
        let rel = std::path::Path::new(&name).strip_prefix(&root_dir).map(|p| p.to_path_buf()).unwrap_or_default();
        let relative_path = rel.parent().map(|p| p.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/")).unwrap_or_default();
        let short_name = opts.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let file = compiler::source::VirtualFile::InMemory {
            bytes: std::rc::Rc::new(opts.source.as_bytes().to_vec()),
            name,
            mime_type: compiler::util::mime_mappings::AS,
            last_modified: 0,
        };
        let root = compiler::source::Source::new(file, Some(root_dir), &relative_path, &short_name, compiler::source::Owner::SourceList, false, true, true);
        let depends_on = if opts.same_build { vec![library.clone()] } else { Vec::new() };
        let mut libraries = vec![(library, true)];
        if opts.app.playerglobal {
            libraries.insert(0, (compiler::abc_library::playerglobal(), true));
        }
        compiler::as3::embed_evaluator::KEEP_EMBEDDED.with(|k| *k.borrow_mut() = opts.keep_embedded.iter().cloned().collect());
        let compiled = tools::mxmlc::compile_unit(configuration.clone(), root.clone(), libraries, &depends_on);
        compiler::as3::embed_evaluator::KEEP_EMBEDDED.with(|k| k.borrow_mut().clear());
        let diagnostics = compiler::toolkit::take_diagnostics();
        if diagnostics.iter().any(|d| d.level == Level::Error) {
            return Err(diagnostics);
        }
        let Some(unit) = root.borrow().get_compilation_unit() else { return Err(diagnostics) };
        let (definitions, abc) = {
            let u = unit.borrow();
            (u.top_level_definitions.iter().map(|q| q.to_string()).collect(), u.get_byte_codes().to_vec())
        };
        let mut abcs = vec![linker::DoAbc { name: String::new(), flag: 1, abc: abc.clone() }];
        let keep = configuration.keep_as3_metadata.clone().unwrap_or_default();
        tools::post_link::merge(&mut abcs, configuration.debug || configuration.verbose_stacktraces, true, "", &keep);
        let dependencies = compiled
            .units
            .iter()
            .filter(|u| !std::rc::Rc::ptr_eq(u, &unit))
            .filter(|u| !u.borrow().get_source().borrow().is_swc_script_owner())
            .map(|u| {
                let u = u.borrow();
                (u.get_source().borrow().get_name().to_string(), u.get_byte_codes().to_vec())
            })
            .collect();
        Ok(CompiledUnit { definitions, abc, optimized: abcs.remove(0).abc, dependencies, diagnostics })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYERGLOBAL: &str = r"C:\Program Files\Adobe\Adobe Animate 2024\Common\Configuration\ActionScript 3.0\FP11.2\playerglobal.swc";

    fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lumen_flex2_{name}_{}", std::process::id())).join("src");
        for (rel, text) in files {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        dir
    }

    fn options(src: &PathBuf) -> ApplicationOptions {
        ApplicationOptions {
            main: src.join("Main.as"),
            source_path: vec![src.clone()],
            external_library_path: vec![PathBuf::from(PLAYERGLOBAL)],
            target_player: "11.2".into(),
            swf_version: 15,
            default_background_color: 0,
            compile_date: Some(0),
            ..Default::default()
        }
    }

    const MAIN: &str = "package { import flash.display.Sprite; import a.Helper; public class Main extends Sprite { public function Main() { var h:Helper = new Helper(); h.n = Helper.twice(h.n); } } }";
    const HELPER: &str = "package a { public class Helper { public var n:int = 2; public function Helper() {} public static function twice(x:int):int { return x * 2; } } }";

    #[test]
    fn links_a_multi_file_application() {
        if !std::path::Path::new(PLAYERGLOBAL).exists() {
            return;
        }
        let src = project("app", &[("Main.as", MAIN), ("a/Helper.as", HELPER)]);
        let app = compile_application(&options(&src)).unwrap_or_else(|d| panic!("{}", d.iter().map(|d| d.text.clone()).collect::<Vec<_>>().join("\n")));
        assert_eq!(app.units.len(), 2);
        let swf = crate::swf::Swf::parse(&app.swf).unwrap();
        let codes: Vec<u16> = swf.timeline.tags.iter().map(|t| t.code).collect();
        assert_eq!(codes, [69, 65, 9, 41, 43, 82, 76, 1, 0]);
        let abc = &swf.timeline.tags[5].body;
        assert_eq!(&abc[..11], b"\x01\0\0\0frame1\0");

        let unit = compile_unit(&UnitOptions {
            source: MAIN.to_string(),
            path: src.join("Main.as"),
            root: src.clone(),
            abcs: vec![abc[11..].to_vec()],
            exclude: vec!["Main".into()],
            external_library_path: vec![PathBuf::from(PLAYERGLOBAL)],
            app: options(&src),
            ..Default::default()
        })
        .unwrap_or_else(|d| panic!("{}", d.iter().map(|d| d.text.clone()).collect::<Vec<_>>().join("\n")));
        assert_eq!(unit.definitions, ["Main"]);
        assert_eq!(unit.abc, app.units[0].1);
    }

    #[test]
    fn reports_errors_as_mxmlc_does() {
        if !std::path::Path::new(PLAYERGLOBAL).exists() {
            return;
        }
        let text = "package { import flash.display.Sprite; public class Main extends Sprite { public function Main() { nope = 1; } } }";
        let src = project("err", &[("Main.as", text)]);
        let d = compile_application(&options(&src)).err().expect("an error");
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].line, d[0].col), (1, 100));
        let tail = format!("(1): col: 100 Error: Access of undefined property nope.\n\n{text}\n{}^", " ".repeat(99));
        assert!(d[0].text.ends_with(&tail), "{}", d[0].text);
    }
}
