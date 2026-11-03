//! The configuration checksums of `flex2.compiler.config.ConfigurationBuffer` (`checksum`, `checksum_ts`,
//! `link_checksum_ts`) for mxmlc's command line, as `Mxmlc.processConfiguration` builds the buffer:
//! `DefaultsConfigurator.loadDefaults`, `CommandLineConfigurator.parse`, the `-load-config` files and the project
//! `<app>-config.xml` through `FileConfigurator`, then `commit`. Each variable's `ConfigurationInfo` (checksum flag,
//! getter type, argument count and names, setter parameters) comes from `configuration_vars.txt`, read off Java's
//! reflection data.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::source::canonical_path;

const VARS: &str = include_str!("configuration_vars.txt");
const COMMAND_LINE: &str = "command line";
const DEFAULTS: &str = "defaults";
const FILE_SPECS: &str = "file-specs";

#[derive(Clone, Copy, PartialEq)]
enum Getter {
    None,
    Other,
    File,
    Files,
}

struct Info {
    do_checksum: bool,
    getter: Getter,
    arg_count: i32,
    allow_multiple: bool,
    is_path: bool,
    is_greedy: bool,
    arg_names: Vec<String>,
    params: &'static str,
}

impl Info {
    fn is_boolean(&self) -> bool {
        self.arg_count <= 1 && self.params == "B"
    }

    fn arg_name(&self, i: usize) -> String {
        if self.arg_names.is_empty() {
            return String::new();
        }
        self.arg_names[i.min(self.arg_names.len() - 1)].clone()
    }
}

fn infos() -> &'static HashMap<&'static str, Info> {
    static INFOS: OnceLock<HashMap<&'static str, Info>> = OnceLock::new();
    INFOS.get_or_init(|| {
        let mut m = HashMap::new();
        for line in VARS.lines() {
            let f: Vec<&str> = line.split(' ').collect();
            if f.len() < 9 {
                continue;
            }
            m.insert(
                f[0],
                Info {
                    do_checksum: f[1] == "1",
                    getter: match f[2] {
                        "O" => Getter::Other,
                        "F" => Getter::File,
                        "FA" => Getter::Files,
                        _ => Getter::None,
                    },
                    arg_count: f[3].parse().unwrap_or(1),
                    allow_multiple: f[4] == "1",
                    is_path: f[5] == "1",
                    is_greedy: f[6] == "1",
                    arg_names: if f[7] == "-" { Vec::new() } else { f[7].split(',').map(String::from).collect() },
                    params: f[8],
                },
            );
        }
        m
    })
}

/// `Configuration.getAliases()` plus `CommandLineConfigurator.getAliases`' leaf names.
fn aliases() -> &'static HashMap<String, String> {
    static ALIASES: OnceLock<HashMap<String, String>> = OnceLock::new();
    ALIASES.get_or_init(|| {
        let mut m: HashMap<String, String> = [
            ("l", "compiler.library-path"),
            ("el", "compiler.external-library-path"),
            ("sp", "compiler.source-path"),
            ("rsl", "runtime-shared-libraries"),
            ("keep", "compiler.keep-generated-actionscript"),
            ("o", "output"),
            ("rslp", "runtime-shared-library-path"),
            ("static-rsls", "static-link-runtime-shared-libraries"),
            ("rsl-domain", "runtime-shared-library-settings.application-domain"),
            ("minimum-supported-version", "compiler.mxml.minimum-supported-version"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        for var in infos().keys() {
            if let Some(i) = var.rfind('.') {
                m.entry(var[i + 1..].to_string()).or_insert_with(|| var.to_string());
            }
        }
        m
    })
}

#[derive(Clone)]
struct Value {
    args: Vec<String>,
    source: String,
    context: Option<String>,
}

/// The checksum half of `ConfigurationBuffer`.
#[derive(Default)]
pub struct ConfigurationBuffer {
    vars: Vec<(String, Vec<Value>)>,
    tokens: HashMap<String, String>,
    compile_checksum: String,
    compile_checksum_ts: String,
    link_checksum: String,
    link_checksum_ts: String,
}

/// `ConfigurationBuffer.calculateChecksum(String)`: the sum of the UTF-8 bytes as Java's signed bytes.
pub fn calculate_checksum(s: &str) -> i32 {
    s.as_bytes().iter().fold(0i32, |c, &b| c.wrapping_add(b as i8 as i32))
}

/// `Integer.decode` / `Long.decode`.
fn decode(s: &str) -> Option<i64> {
    let (neg, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let v = if let Some(h) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")).or_else(|| rest.strip_prefix('#')) {
        i64::from_str_radix(h, 16).ok()?
    } else if rest.len() > 1 && rest.starts_with('0') {
        i64::from_str_radix(&rest[1..], 8).ok()?
    } else {
        rest.parse().ok()?
    };
    Some(if neg { -v } else { v })
}

fn last_modified(p: &Path) -> i64 {
    std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as i64)
}

/// A resolved configuration file: `getName()`, `isDirectory()`, `getLastModified()`.
#[derive(Clone)]
struct ConfigFile {
    name: String,
    is_dir: bool,
    last_modified: i64,
}

/// `ConfigurationPathResolver.getVirtualFile`: the configuration's directory, then the path itself, then the
/// working directory.
fn resolve(path: &str, context: Option<&str>) -> Option<ConfigFile> {
    let p = Path::new(path);
    let mut candidates: Vec<PathBuf> = Vec::new();
    if !p.is_absolute() {
        if let Some(c) = context {
            candidates.push(Path::new(c).join(p));
        }
    }
    candidates.push(p.to_path_buf());
    for c in candidates {
        if c.exists() {
            return Some(ConfigFile { name: canonical_path(&c), is_dir: c.is_dir(), last_modified: last_modified(&c) });
        }
    }
    None
}

impl ConfigurationBuffer {
    fn info(var: &str) -> Option<&'static Info> {
        infos().get(var)
    }

    fn unalias(var: &str) -> String {
        aliases().get(var).cloned().unwrap_or_else(|| var.to_string())
    }

    pub fn set_token(&mut self, token: &str, value: &str) {
        self.tokens.insert(token.to_string(), value.to_string());
    }

    pub fn get_token(&self, token: &str) -> Option<String> {
        self.tokens.get(token).cloned()
    }

    /// `setVar(var, vals, source, line, contextPath, append)`.
    fn set_var(&mut self, var: &str, args: Vec<String>, source: &str, context: Option<String>, append: bool) -> Result<(), String> {
        let var = Self::unalias(var);
        let info = Self::info(&var).ok_or_else(|| format!("unknown configuration variable '{var}'"))?;
        if info.arg_count != -1 && args.len() as i32 != info.arg_count {
            return Err(format!("incorrect argument count for {var}"));
        }
        let val = Value { args, source: source.to_string(), context };
        match self.vars.iter_mut().find(|(v, _)| *v == var) {
            Some((_, vals)) => {
                if !append && vals[0].source != val.source {
                    vals.clear();
                } else if !info.allow_multiple {
                    return Err(format!("'{var}' can't be set more than once"));
                }
                vals.push(val);
            }
            None => self.vars.push((var, vec![val])),
        }
        Ok(())
    }

    fn clear_var(&mut self, var: &str) {
        let var = Self::unalias(var);
        self.vars.retain(|(v, _)| *v != var);
    }

    /// `clearSourceVars(source)`.
    fn clear_source_vars(&mut self, source: &str) {
        for (_, vals) in &mut self.vars {
            vals.retain(|v| v.source != source);
        }
        self.vars.retain(|(_, vals)| !vals.is_empty());
    }

    fn get_var(&self, var: &str) -> Option<&Vec<Value>> {
        self.vars.iter().find(|(v, _)| v == var).map(|(_, vals)| vals)
    }

    /// `processValues`: `${token}` substitution.
    fn process_values(&self, args: &[String]) -> Vec<String> {
        args.iter()
            .map(|arg| {
                let mut arg = arg.clone();
                for _ in 0..100 {
                    let Some(o) = arg.find("${") else { break };
                    let Some(c) = arg[o..].find('}').map(|c| c + o) else { break };
                    let token = &arg[o + 2..c];
                    let Some(value) = self.get_token(token) else { break };
                    arg = format!("{}{}{}", &arg[..o], value, &arg[c + 1..]);
                }
                arg
            })
            .collect()
    }

    /// `calculateChecksum(VirtualFile)`: a configuration file mxmlc loaded.
    fn calculate_file_checksum(&mut self, f: &ConfigFile) {
        self.compile_checksum.push_str(&f.name);
        self.compile_checksum_ts.push_str(&f.last_modified.to_string());
    }

    /// `DefaultsConfigurator.loadDefaults`.
    fn load_defaults(&mut self) -> Result<(), String> {
        let set = |b: &mut Self, var: &str, val: &[&str]| b.set_var(var, val.iter().map(|s| s.to_string()).collect(), DEFAULTS, None, false);
        set(self, "target-player", &["11.1"])?;
        set(self, "swf-version", &["14"])?;
        for v in [
            "compiler.accessible",
            "compiler.es",
            "compiler.mobile",
            "compiler.verbose-stacktraces",
            "compiler.show-dependency-warnings",
            "compiler.keep-generated-actionscript",
            "compiler.keep-generated-signatures",
            "compiler.disable-incremental-optimizations",
            "compiler.allow-source-path-overlap",
        ] {
            set(self, v, &["false"])?;
        }
        for v in [
            "lazy-init",
            "compiler.omit-trace-statements",
            "compiler.optimize",
            "compiler.strict",
            "compiler.show-actionscript-warnings",
            "compiler.as3",
            "compiler.show-deprecation-warnings",
            "compiler.show-shadowed-device-font-warnings",
            "compiler.show-binding-warnings",
            "compiler.fonts.advanced-anti-aliasing",
            "generate-frame-loader",
            "use-network",
        ] {
            set(self, v, &["true"])?;
        }
        set(self, "debug-password", &[""])?;
        set(self, "framework", &["halo"])?;
        set(self, "compiler.theme", &["${flexlib}/themes/Spark/spark.css"])?;
        set(self, "compiler.locale", &["en_US"])?;
        set(self, "compiler.translation-format", &["flex2.compiler.i18n.PropertyTranslationFormat"])?;
        set(self, "default-frame-rate", &["24"])?;
        set(self, "default-background-color", &["0xFFFFFF"])?;
        set(self, "default-size", &["500", "375"])?;
        set(self, "load-config", &["${flexlib}/${configname}-config.xml"])?;
        set(self, "default-script-limits", &["1000", "60"])?;
        set(self, "compiler.fonts.max-cached-fonts", &["20"])?;
        set(self, "compiler.fonts.max-glyphs-per-face", &["1000"])?;
        set(self, "compiler.fonts.managers", &["flash.fonts.JREFontManager", "flash.fonts.BatikFontManager"])?;
        let snapshot = if cfg!(target_os = "macos") { "${flexlib}/macFonts.ser" } else { "${flexlib}/localFonts.ser" };
        set(self, "compiler.fonts.local-fonts-snapshot", &[snapshot])?;
        set(self, "compiler.debug", &["false"])?;
        set(self, "compiler.use-resource-bundle-metadata", &["true"])?;
        Ok(())
    }

    /// `CommandLineConfigurator.parse(buffer, defaultvar, args)`.
    fn parse_command_line(&mut self, args: &[String], default_var: &str) -> Result<(), String> {
        let is_an_argument = |a: &str| a.starts_with('-') && a.len() > 1 && !a.as_bytes()[1].is_ascii_digit();
        let sep = if cfg!(windows) { ';' } else { ':' };
        let mut i = 0;
        let mut dash = true;
        let mut seen: Vec<String> = Vec::new();
        while i < args.len() {
            let mut append = false;
            let mut arg_list: Vec<String> = Vec::new();
            let var: String;
            let mut done = false;
            if args[i] == "--" {
                dash = false;
                var = default_var.to_string();
                i += 1;
            } else if dash && args[i].starts_with('+') {
                let a = &args[i];
                let c = if a.len() > 1 && a.as_bytes()[1] == b'+' { 2 } else { 1 };
                i += 1;
                match a.find('=') {
                    Some(eq) => self.set_token(&a[c..eq], &a[eq + 1..]),
                    None => {
                        let rest = args.get(i).ok_or("insufficient arguments")?.clone();
                        i += 1;
                        self.set_token(&a[c..], &rest);
                    }
                }
                continue;
            } else if dash && is_an_argument(&args[i]) {
                let a = &args[i];
                let c = if a.as_bytes()[1] == b'-' { 2 } else { 1 };
                i += 1;
                let (name, rest) = if let Some(pe) = a.find("+=") {
                    append = true;
                    (&a[c..pe], Some(&a[pe + 2..]))
                } else if let Some(eq) = a.find('=') {
                    (&a[c..eq], Some(&a[eq + 1..]))
                } else {
                    (&a[c..], None)
                };
                let v = Self::unalias(name);
                let info = Self::info(&v).ok_or_else(|| format!("unknown configuration variable '{v}'"))?;
                if let Some(rest) = rest {
                    if rest.is_empty() {
                        self.clear_var(&v);
                        continue;
                    }
                    let parts: Vec<String> = if info.is_path { rest.split([',', sep]).map(String::from).collect() } else { rest.split(',').map(String::from).collect() };
                    let parts = java_split_trailing(parts);
                    arg_list.extend(parts);
                    if info.arg_count != -1 && arg_list.len() as i32 != info.arg_count {
                        return Err(format!("incorrect argument count for {v}"));
                    }
                    done = true;
                }
                var = v;
            } else {
                var = default_var.to_string();
            }
            if !done {
                let info = Self::info(&var).ok_or_else(|| format!("unknown configuration variable '{var}'"))?;
                if info.is_boolean() {
                    arg_list.push("true".to_string());
                } else {
                    let count = info.arg_count;
                    while i < args.len() {
                        if !info.is_greedy && dash && is_an_argument(&args[i]) {
                            if count == -1 {
                                break;
                            }
                            return Err(format!("incorrect argument count for {var}"));
                        }
                        arg_list.push(args[i].clone());
                        i += 1;
                        if arg_list.len() as i32 == count {
                            break;
                        }
                    }
                    if count != -1 && arg_list.len() as i32 != count {
                        return Err(format!("incorrect argument count for {var}"));
                    }
                }
            }
            if seen.contains(&var) && var == default_var {
                return Err("interspersed defaults".to_string());
            }
            self.set_var(&var, arg_list, COMMAND_LINE, None, append)?;
            seen.push(var);
        }
        Ok(())
    }

    /// `FileConfigurator.load(buffer, in, path, context, "flex-config")`.
    fn load_file(&mut self, file: &ConfigFile) -> Result<(), String> {
        let text = std::fs::read_to_string(&file.name).map_err(|e| format!("{}: {e}", file.name))?;
        let doc = roxmltree::Document::parse(&text).map_err(|e| format!("{}: {e}", file.name))?;
        let context = Path::new(&file.name).parent().map(|p| p.to_string_lossy().into_owned());
        let root = doc.root_element();
        if root.tag_name().name() != "flex-config" {
            return Err(format!("{}: expected flex-config", file.name));
        }
        self.load_children(root, "", &file.name, &context)
    }

    fn load_children(&mut self, node: roxmltree::Node, base: &str, source: &str, context: &Option<String>) -> Result<(), String> {
        let text_of = |n: roxmltree::Node| n.children().filter(|c| c.is_text()).map(|c| c.text().unwrap_or("").trim().to_string()).collect::<String>();
        for child in node.children().filter(|c| c.is_element()) {
            let element = child.tag_name().name();
            let fullname = if base.is_empty() { element.to_string() } else { format!("{base}.{element}") };
            if let Some(info) = Self::info(&fullname) {
                let append = child.attribute("append").is_some_and(|a| a.eq_ignore_ascii_case("true"));
                let items: Vec<(String, String)> = child.children().filter(|c| c.is_element()).map(|c| (c.tag_name().name().to_string(), text_of(c))).collect();
                let args: Vec<String> = if info.arg_count == 1 {
                    vec![text_of(child)]
                } else {
                    let mut names: Vec<&str> = Vec::new();
                    let unique = items.iter().all(|(n, _)| {
                        let fresh = !names.contains(&n.as_str());
                        names.push(n);
                        fresh
                    });
                    if info.arg_count > 1 && unique {
                        (0..items.len()).filter_map(|i| items.iter().find(|(n, _)| *n == info.arg_name(i)).map(|(_, v)| v.clone())).collect()
                    } else {
                        items.into_iter().map(|(_, v)| v).collect()
                    }
                };
                self.set_var(&fullname, args, source, context.clone(), append)?;
            } else if infos().keys().any(|v| v.starts_with(&format!("{fullname}."))) {
                self.load_children(child, &fullname, source, context)?;
            } else {
                return Err(format!("unknown configuration variable '{fullname}'"));
            }
        }
        Ok(())
    }

    /// The four checksum strings `commit` builds, setter by setter.
    fn commit(&mut self) {
        let target_player = self.get_var("target-player").and_then(|v| v.last()).map(|v| self.process_values(&v.args)[0].clone()).unwrap_or_else(|| "11.1".into());
        let mut tp = target_player.split('.').map(|p| p.parse::<i32>().unwrap_or(0));
        let (major, minor) = (tp.next().unwrap_or(0), tp.next().unwrap_or(0));
        let locales: Vec<String> = self.get_var("compiler.locale").map(|vals| vals.iter().flat_map(|v| self.process_values(&v.args)).collect()).unwrap_or_default();
        let playerglobal_home = std::env::var("PLAYERGLOBAL_HOME").unwrap_or_else(|_| "{playerglobalHome}".into());
        let air_home = std::env::var("AIR_HOME").unwrap_or_else(|_| "{airHome}".into());
        let vars = self.vars.clone();
        for (var, vals) in &vars {
            let Some(info) = Self::info(var) else { continue };
            let mut files: Vec<ConfigFile> = Vec::new();
            for val in vals {
                let mut args = self.process_values(&val.args);
                if matches!(var.as_str(), "externs" | "includes" | "include-resource-bundles") {
                    // `Configuration.toQNameString` rewrites the setter's list in place.
                    for a in &mut args {
                        if !a.contains(':') {
                            if let Some(dot) = a.rfind('.') {
                                *a = format!("{}:{}", &a[..dot], &a[dot + 1..]);
                            }
                        }
                    }
                }
                let ctx = val.context.as_deref();
                match info.getter {
                    Getter::Files if var == "compiler.namespaces.namespace" => {
                        for m in args.iter().skip(1).step_by(2) {
                            files.extend(resolve(m, ctx));
                        }
                    }
                    Getter::Files => {
                        for a in &args {
                            let a = a.replace("{playerglobalHome}", &playerglobal_home).replace("{airHome}", &air_home);
                            let a = a.replace("{targetPlayerMajorVersion}", &major.to_string()).replace("{targetPlayerMinorVersion}", &minor.to_string());
                            if a.contains("{locale}") {
                                for l in &locales {
                                    files.extend(resolve(&a.replace("{locale}", l), ctx));
                                }
                            } else {
                                files.extend(resolve(&a, ctx));
                            }
                        }
                    }
                    Getter::File => {
                        if let Some(f) = args.first().and_then(|a| resolve(a, ctx)) {
                            files = vec![f];
                        }
                    }
                    _ => {}
                }
                let mut s = var.clone();
                let n_params = info.params.len().max(1);
                let list = matches!(info.params, "L" | "A");
                for i in 0..n_params {
                    if matches!(info.getter, Getter::File | Getter::Files) {
                        for f in &files {
                            s.push_str(&f.name);
                        }
                        continue;
                    }
                    if list {
                        for a in &args {
                            s.push_str(a);
                        }
                    } else if let Some(a) = args.get(i) {
                        match info.params.as_bytes()[i] {
                            b'B' => s.push_str(if a.trim().eq_ignore_ascii_case("true") { "true" } else { "false" }),
                            b'I' | b'J' => s.push_str(&decode(a).map_or_else(|| a.clone(), |v| v.to_string())),
                            _ => s.push_str(a),
                        }
                    }
                }
                let mut ts = String::new();
                if matches!(info.getter, Getter::File | Getter::Files) {
                    for f in files.iter().filter(|f| !f.is_dir) {
                        ts.push_str(&f.last_modified.to_string());
                    }
                }
                if info.do_checksum {
                    self.compile_checksum.push_str(&s);
                    self.compile_checksum_ts.push_str(&ts);
                }
                self.link_checksum.push_str(&s);
                self.link_checksum_ts.push_str(&ts);
            }
        }
    }

    /// `checksum()`.
    pub fn checksum(&self) -> i32 {
        calculate_checksum(&self.compile_checksum)
    }

    /// `checksum_ts()`.
    pub fn checksum_ts(&self) -> i32 {
        calculate_checksum(&format!("{}{}", self.compile_checksum, self.compile_checksum_ts))
    }

    /// `link_checksum_ts()`.
    pub fn link_checksum_ts(&self) -> i32 {
        calculate_checksum(&format!("{}{}", self.link_checksum, self.link_checksum_ts))
    }

    /// `Mxmlc.processConfiguration` for mxmlc's `args`, with `flexlib` when no `+flexlib` token is given.
    pub fn mxmlc(args: &[String], flexlib: Option<&Path>) -> Result<ConfigurationBuffer, String> {
        let mut b = ConfigurationBuffer::default();
        b.load_defaults()?;
        b.parse_command_line(args, FILE_SPECS)?;
        if b.get_token("flexlib").is_none() {
            let f = flexlib.map_or_else(|| ".".to_string(), |p| p.to_string_lossy().into_owned());
            b.set_token("flexlib", &f);
        }
        if b.get_token("framework").is_none() {
            b.set_token("framework", "halo");
        }
        if b.get_token("configname").is_none() {
            b.set_token("configname", "flex");
        }
        if b.get_token("build.number").is_none() {
            b.set_token("build.number", "1359417");
        }
        let configs: Vec<Value> = b.get_var("load-config").cloned().unwrap_or_default();
        for cv in configs {
            for path in b.process_values(&cv.args) {
                let f = resolve(&path, cv.context.as_deref()).ok_or_else(|| format!("unable to open '{path}'"))?;
                b.calculate_file_checksum(&f);
                b.load_file(&f)?;
            }
        }
        let project = b.get_var(FILE_SPECS).and_then(|v| v.last()).and_then(|cv| cv.args.last().cloned());
        if let Some(val) = project {
            if let Some(index) = val.rfind('.') {
                let p = format!("{}-config.xml", &val[..index]);
                if let Some(f) = resolve(&p, None).filter(|f| !f.is_dir) {
                    b.calculate_file_checksum(&f);
                    b.load_file(&f)?;
                }
            }
        }
        b.clear_source_vars(COMMAND_LINE);
        b.parse_command_line(args, FILE_SPECS)?;
        b.commit();
        Ok(b)
    }
}

/// `String.split` drops trailing empty strings.
fn java_split_trailing(mut parts: Vec<String>) -> Vec<String> {
    while parts.len() > 1 && parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    parts
}
