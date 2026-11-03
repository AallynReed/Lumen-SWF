//! Ports of `ThreadLocalToolkit`'s logging, `LocalLogger` and the `ConsoleLogger` text format.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warning,
    Error,
}

/// One message as mxmlc's `ConsoleLogger` prints it.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub level: Level,
    pub path: Option<String>,
    /// -1 when absent.
    pub line: i32,
    pub col: i32,
    pub message: String,
    /// ASC error code, -1 when absent.
    pub code: i32,
    /// The exact text mxmlc writes to stderr, without the trailing blank line.
    pub text: String,
}

/// A `CompilerMessage` subclass with its localized text already filled in.
#[derive(Clone, Debug)]
pub struct CompilerMessage {
    pub level: Level,
    pub text: String,
    pub path: Option<String>,
    pub line: i32,
    pub col: i32,
}

impl CompilerMessage {
    pub fn error(text: impl Into<String>) -> CompilerMessage {
        CompilerMessage { level: Level::Error, text: text.into(), path: None, line: -1, col: -1 }
    }

    pub fn warning(text: impl Into<String>) -> CompilerMessage {
        CompilerMessage { level: Level::Warning, text: text.into(), path: None, line: -1, col: -1 }
    }
}

/// `LocalLogger`: counts a source's messages and forwards them while connected.
#[derive(Default)]
pub struct LocalLogger {
    error_count: Cell<i32>,
    warning_count: Cell<i32>,
    connected: Cell<bool>,
    source: RefCell<Option<std::rc::Weak<RefCell<super::source::Source>>>>,
    warnings: RefCell<Vec<Warning>>,
}

/// `LocalLogger.Warning`: a recorded warning, kept for the incremental cache.
#[derive(Clone, Debug, Default)]
pub struct Warning {
    pub path: Option<String>,
    pub warning: Option<String>,
    pub source: Option<String>,
    pub line: Option<i32>,
    pub col: Option<i32>,
    pub error_code: Option<i32>,
}

impl LocalLogger {
    pub fn new() -> Rc<LocalLogger> {
        Rc::new(LocalLogger { error_count: Cell::new(0), warning_count: Cell::new(0), connected: Cell::new(true), source: RefCell::new(None), warnings: RefCell::new(Vec::new()) })
    }

    /// `recordWarning(path, line, col, warning, source, errorCode)`.
    pub fn record_warning(&self, w: Warning) {
        self.warnings.borrow_mut().push(w);
        self.warning_count.set(self.warning_count.get() + 1);
    }

    /// `getWarnings()`.
    pub fn get_warnings(&self) -> Vec<Warning> {
        self.warnings.borrow().clone()
    }

    /// `new LocalLogger(original, source)`: the source whose messages it carries.
    pub fn set_source(&self, s: &super::source::SourceRef) {
        *self.source.borrow_mut() = Some(Rc::downgrade(s));
    }

    /// `checkPath`: a message about another file (generated code) is reported against the source, without a line.
    fn misrouted(&self, p: &str) -> Option<String> {
        let s = self.source.borrow().as_ref()?.upgrade()?;
        let s = s.borrow();
        (s.get_name_for_reporting() != p && !s.is_included_file(p)).then(|| s.get_name_for_reporting().to_string())
    }

    /// `checkPath` for a message whose path may be null (a null path is never the source's).
    fn misrouted_opt(&self, p: Option<&str>) -> Option<String> {
        match p {
            Some(p) => self.misrouted(p),
            None => {
                let s = self.source.borrow().as_ref()?.upgrade()?;
                let name = s.borrow().get_name_for_reporting().to_string();
                Some(name)
            }
        }
    }

    pub fn error_count(&self) -> i32 {
        self.error_count.get()
    }

    pub fn warning_count(&self) -> i32 {
        self.warning_count.get()
    }

    pub fn disconnect(&self) {
        self.connected.set(false);
    }

    pub fn is_connected(&self) -> bool {
        self.connected.get()
    }
}

#[derive(Default)]
struct Toolkit {
    diagnostics: Vec<Diagnostic>,
    errors: i32,
    warnings: i32,
    logger: Option<Rc<LocalLogger>>,
    warnings_enabled: bool,
}

thread_local! {
    static TOOLKIT: RefCell<Toolkit> = RefCell::new(Toolkit { warnings_enabled: true, ..Default::default() });
}

/// Starts a fresh compilation's console logger.
pub fn reset(warnings_enabled: bool) {
    TOOLKIT.with(|t| *t.borrow_mut() = Toolkit { warnings_enabled, ..Default::default() });
}

pub fn take_diagnostics() -> Vec<Diagnostic> {
    TOOLKIT.with(|t| std::mem::take(&mut t.borrow_mut().diagnostics))
}

pub fn get_logger() -> Option<Rc<LocalLogger>> {
    TOOLKIT.with(|t| t.borrow().logger.clone())
}

pub fn set_logger(logger: Option<Rc<LocalLogger>>) {
    TOOLKIT.with(|t| t.borrow_mut().logger = logger);
}

/// `ThreadLocalToolkit.errorCount()`: the current logger's count.
pub fn error_count() -> i32 {
    TOOLKIT.with(|t| {
        let t = t.borrow();
        match &t.logger {
            Some(l) => l.error_count(),
            None => t.errors,
        }
    })
}

/// The console logger's count, whatever logger is current.
pub fn console_error_count() -> i32 {
    TOOLKIT.with(|t| t.borrow().errors)
}

fn line_pointer(col: i32, source: Option<&str>) -> String {
    if col <= 0 {
        return "^".to_string();
    }
    let units: Vec<u16> = source.map(|s| crate::asc::util::jstr::units(s).collect()).unwrap_or_default();
    let mut b = String::new();
    for i in 0..(col - 1) as usize {
        if i < units.len() && units[i] == '\t' as u16 {
            b.push('\t');
        } else {
            b.push(' ');
        }
    }
    b.push('^');
    b
}

/// `FileUtil.readLine(path, line)`.
pub fn line_text(path: &str, line: i32) -> String {
    let Ok(bytes) = std::fs::read(path) else { return String::new() };
    // Java's "UTF8" reader keeps a byte-order mark.
    let text = String::from_utf8_lossy(&bytes);
    // `FileUtil.readLine` reads the first line for any line below 2.
    let line = line.max(1);
    let mut n = 1;
    let mut start = 0;
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\r' || b[i] == b'\n' {
            if n == line {
                return text[start..i].to_string();
            }
            if b[i] == b'\r' && i + 1 < b.len() && b[i + 1] == b'\n' {
                i += 1;
            }
            n += 1;
            start = i + 1;
        }
        i += 1;
    }
    if n == line {
        text[start..].to_string()
    } else {
        String::new()
    }
}

fn level_word(level: Level) -> &'static str {
    match level {
        Level::Error => "Error",
        Level::Warning => "Warning",
        Level::Info => "",
    }
}

fn console(d: Diagnostic) {
    TOOLKIT.with(|t| {
        let mut t = t.borrow_mut();
        match d.level {
            Level::Error => t.errors += 1,
            Level::Warning => {
                if !t.warnings_enabled {
                    return;
                }
                t.warnings += 1
            }
            Level::Info => {}
        }
        t.diagnostics.push(d);
    });
}

fn forward(level: Level) -> bool {
    TOOLKIT.with(|t| {
        let t = t.borrow();
        match &t.logger {
            Some(l) => {
                match level {
                    Level::Error => l.error_count.set(l.error_count.get() + 1),
                    Level::Warning => l.warning_count.set(l.warning_count.get() + 1),
                    Level::Info => {}
                }
                l.is_connected()
            }
            None => true,
        }
    })
}

/// `ConsoleLogger.logError(path, line, col, error, source)` and its warning twin.
fn sink_asc(level: Level, path: &str, line: i32, col: i32, msg: &str, source: &str, code: i32) {
    let text = format!("{}({}): col: {} {}: {}\n\n{}\n{}", path, line, col, level_word(level), msg, source, line_pointer(col, Some(source)));
    console(Diagnostic { level, path: Some(path.to_string()), line, col, message: msg.to_string(), code, text });
}

/// `ConsoleLogger.logError(path, error)` / `logWarning(path, warning)`.
fn sink_path(level: Level, path: &str, msg: &str, code: i32) {
    let text = format!("{}\n\n{}: {}", path, level_word(level), msg);
    console(Diagnostic { level, path: Some(path.to_string()), line: -1, col: -1, message: msg.to_string(), code, text });
}

/// `ConsoleLogger.logError(error)`.
fn sink_plain(level: Level, msg: &str) {
    let text = if level == Level::Info { msg.to_string() } else { format!("{}: {}", level_word(level), msg) };
    console(Diagnostic { level, path: None, line: -1, col: -1, message: msg.to_string(), code: -1, text });
}

/// `ConsoleLogger.log(m, source)`.
fn sink_message(m: CompilerMessage, source: Option<&str>) {
    let mut prefix = String::new();
    if let Some(p) = &m.path {
        prefix.push_str(p);
        if m.line != -1 {
            prefix.push_str(&format!("({})", m.line));
            if m.col != -1 {
                prefix.push_str(&format!(": col: {}", m.col));
            }
        }
        prefix.push_str(": ");
    }
    match m.level {
        Level::Error => prefix.push_str("Error: "),
        Level::Warning => prefix.push_str("Warning: "),
        Level::Info => {}
    }
    let mut ex = String::new();
    if let (Some(p), true) = (&m.path, m.line != -1) {
        let s = match source {
            Some(s) => s.to_string(),
            None => line_text(p, m.line),
        };
        ex.push_str("\n\n");
        ex.push_str(&s);
    }
    if m.col != -1 {
        ex.push('\n');
        // `ConsoleLogger.log(m, source)` points along `source` itself, so tabs are kept only when one is given.
        ex.push_str(&line_pointer(m.col, source));
    }
    let text = format!("{prefix}{}{ex}", m.text);
    console(Diagnostic { level: m.level, path: m.path.clone(), line: m.line, col: m.col, message: m.text, code: -1, text });
}

/// `ThreadLocalToolkit.log(message, source)`: a message about a whole source.
pub fn log_for(m: CompilerMessage, name_for_reporting: &str) {
    log(m, Some(name_for_reporting), -1, -1, None);
}

/// `ConsoleLogger.logError(path, line, error)` / `logWarning(path, line, warning)`.
fn sink_path_line(level: Level, path: &str, line: i32, msg: &str) {
    let sep = if level == Level::Error { ":  " } else { ": " };
    let text = format!("{}({}){}{}: {}\n\n{}", path, line, sep, level_word(level), msg, line_text(path, line));
    console(Diagnostic { level, path: Some(path.to_string()), line, col: -1, message: msg.to_string(), code: -1, text });
}

/// `ConsoleLogger.logError(path, line, col, error)`: the line is read from `path` for the pointer.
fn sink_path_line_col(level: Level, path: &str, line: i32, col: i32, msg: &str) {
    let source = line_text(path, line);
    let text = format!("{}({}): col: {} {}: {}\n\n{}\n{}", path, line, col, level_word(level), msg, source, line_pointer(col, Some(&source)));
    console(Diagnostic { level, path: Some(path.to_string()), line, col, message: msg.to_string(), code: -1, text });
}

/// One call on a Java `Logger`, so adapters can rewrite it before it reaches the `LocalLogger` and the console.
#[derive(Clone, Debug)]
pub enum LogCall {
    /// `logError(error)`.
    Text { level: Level, msg: String },
    /// `logError(path, error[, errorCode])`.
    Path { level: Level, path: String, msg: String, code: Option<i32> },
    /// `logError(path, line, error[, errorCode])`.
    PathLine { level: Level, path: String, line: i32, msg: String, code: Option<i32> },
    /// `logError(path, line, col, error)`.
    PathLineCol { level: Level, path: String, line: i32, col: i32, msg: String },
    /// `logError(path, line, col, error, source[, errorCode])`.
    PathLineColSource { level: Level, path: String, line: i32, col: i32, msg: String, source: String, code: Option<i32> },
    /// `log(message[, source])`.
    Message { m: CompilerMessage, source: Option<String> },
}

/// A `Logger` installed with `ThreadLocalToolkit.setLogger` over another (`AbstractLogAdapter`): it passes each call on,
/// possibly rewritten, to its original with `forward_log`.
pub trait LogAdapter {
    fn log(&self, call: LogCall);
}

thread_local! {
    static ADAPTER: RefCell<Option<Rc<dyn LogAdapter>>> = const { RefCell::new(None) };
}

/// `ThreadLocalToolkit.getLogger()` when it is an adapter; `None` means the `LocalLogger`/console chain.
pub fn get_log_adapter() -> Option<Rc<dyn LogAdapter>> {
    ADAPTER.with(|a| a.borrow().clone())
}

/// `ThreadLocalToolkit.setLogger(adapter)`; `None` restores the `LocalLogger`/console chain.
pub fn set_log_adapter(adapter: Option<Rc<dyn LogAdapter>>) {
    ADAPTER.with(|a| *a.borrow_mut() = adapter);
}

/// `original.logX(...)` from an adapter whose original was `original`.
pub fn forward_log(original: &Option<Rc<dyn LogAdapter>>, call: LogCall) {
    match original {
        Some(a) => a.log(call),
        None => deliver(call),
    }
}

fn dispatch(call: LogCall) {
    let adapter = get_log_adapter();
    forward_log(&adapter, call);
}

/// `LocalLogger.checkPath`'s marker for a message about a file other than the source.
fn misroute_info(p: &str, line: i32, col: i32) -> String {
    format!(" [Generated code (use -keep to save): Path: {p}, Line: {line}, Column: {col}]")
}

/// The `LocalLogger` (counts, `checkPath` misrouting) and then the `ConsoleLogger`.
pub fn deliver(call: LogCall) {
    let local = get_logger();
    if let Some(l) = &local {
        let w = |path: Option<&str>, line: Option<i32>, col: Option<i32>, msg: &str, source: Option<&str>, code: Option<i32>| Warning {
            path: path.map(str::to_string),
            warning: Some(msg.to_string()),
            source: source.map(str::to_string),
            line,
            col,
            error_code: code,
        };
        let recorded = match &call {
            LogCall::Text { level: Level::Warning, msg } => Some(w(None, None, None, msg, None, None)),
            LogCall::Path { level: Level::Warning, path, msg, code } => Some(w(Some(path), None, None, msg, None, *code)),
            LogCall::PathLine { level: Level::Warning, path, line, msg, code } => Some(w(Some(path), Some(*line), None, msg, None, *code)),
            LogCall::PathLineCol { level: Level::Warning, path, line, col, msg } => Some(w(Some(path), Some(*line), Some(*col), msg, None, None)),
            LogCall::PathLineColSource { level: Level::Warning, path, line, col, msg, source, code } => Some(w(Some(path), Some(*line), Some(*col), msg, Some(source), *code)),
            LogCall::Message { m, .. } if m.level == Level::Warning => Some(w(m.path.as_deref(), Some(m.line), Some(m.col), &m.text, None, None)),
            _ => None,
        };
        if let Some(r) = recorded {
            l.warnings.borrow_mut().push(r);
        }
    }
    let misrouted = |p: Option<&str>| local.as_ref().and_then(|l| l.misrouted_opt(p));
    match call {
        LogCall::Text { level, msg } => {
            if forward(level) {
                sink_plain(level, &msg);
            }
        }
        LogCall::Path { level, path, msg, code } => {
            if forward(level) {
                match misrouted(Some(&path)) {
                    None => sink_path(level, &path, &msg, code.unwrap_or(-1)),
                    Some(name) => sink_path(level, &name, &format!("{msg}{}", misroute_info(&path, 0, 0)), -1),
                }
            }
        }
        LogCall::PathLine { level, path, line, msg, code } => {
            if forward(level) {
                match misrouted(Some(&path)) {
                    None => sink_path_line(level, &path, line, &msg),
                    Some(name) => sink_path(level, &name, &format!("{msg}{}", misroute_info(&path, line, 0)), code.unwrap_or(-1)),
                }
            }
        }
        LogCall::PathLineCol { level, path, line, col, msg } => {
            if forward(level) {
                match misrouted(Some(&path)) {
                    None => sink_path_line_col(level, &path, line, col, &msg),
                    Some(name) => sink_path(level, &name, &format!("{msg}{}", misroute_info(&path, line, col)), -1),
                }
            }
        }
        LogCall::PathLineColSource { level, path, line, col, msg, source, code } => {
            if forward(level) {
                match misrouted(Some(&path)) {
                    None => sink_asc(level, &path, line, col, &msg, &source, code.unwrap_or(-1)),
                    Some(name) => sink_path(level, &name, &format!("{msg}{}", misroute_info(&path, line, col)), code.unwrap_or(-1)),
                }
            }
        }
        LogCall::Message { mut m, mut source } => {
            if let Some(name) = misrouted(m.path.as_deref()) {
                m.path = Some(name);
                m.line = -1;
                m.col = -1;
                source = None;
            }
            if forward(m.level) {
                sink_message(m, source.as_deref());
            }
        }
    }
}

/// `logError(path, line, col, error, source, errorCode)` and its warning twin, as ASC's handler calls them.
pub fn log_asc(level: Level, path: &str, line: i32, col: i32, msg: &str, source: &str, code: i32) {
    dispatch(LogCall::PathLineColSource { level, path: path.into(), line, col, msg: msg.into(), source: source.into(), code: Some(code) });
}

/// `logError(path, error)` / `logWarning(path, warning)`.
pub fn log_path(level: Level, path: &str, msg: &str, code: i32) {
    dispatch(LogCall::Path { level, path: path.into(), msg: msg.into(), code: (code != -1).then_some(code) });
}

/// `logInfo(info)`, and `ConsoleLogger`'s own lines (`needsCompilation`, `includedFileUpdated`).
pub fn log_info(msg: &str) {
    log_plain(Level::Info, msg);
}

/// `logError(error)`.
pub fn log_plain(level: Level, msg: &str) {
    dispatch(LogCall::Text { level, msg: msg.into() });
}

/// `ThreadLocalToolkit.log(message, path, line, col, source)`, and the shorter overloads with `None`s.
pub fn log(mut m: CompilerMessage, path: Option<&str>, line: i32, col: i32, source: Option<&str>) {
    if let Some(p) = path {
        m.path = Some(p.to_string());
        m.line = line;
        m.col = col;
    }
    dispatch(LogCall::Message { m, source: source.map(str::to_string) });
}

/// `logError(path, line, error)` / `logWarning(path, line, warning)` (CSS parser messages).
pub fn log_path_line(level: Level, path: &str, line: i32, msg: &str) {
    dispatch(LogCall::PathLine { level, path: path.into(), line, msg: msg.into(), code: None });
}

/// `logError(path, line, col, error)` / `logWarning(path, line, col, warning)`.
pub fn log_path_line_col(level: Level, path: &str, line: i32, col: i32, msg: &str) {
    dispatch(LogCall::PathLineCol { level, path: path.into(), line, col, msg: msg.into() });
}

/// `ThreadLocalToolkit.getLogger().logX(...)` for an explicit call shape.
pub fn log_call(call: LogCall) {
    dispatch(call);
}

impl LocalLogger {
    /// A logger restored from the incremental cache: `new LocalLogger(null)`, so not connected.
    pub fn restored(warnings: Vec<Warning>) -> Rc<LocalLogger> {
        let l = LocalLogger::new();
        l.connected.set(false);
        for w in warnings {
            l.record_warning(w);
        }
        l
    }

    /// `displayWarnings(ThreadLocalToolkit.getLogger())`.
    pub fn display_warnings(&self) {
        for w in self.get_warnings() {
            let msg = w.warning.clone().unwrap_or_default();
            let call = match &w.path {
                None => LogCall::Text { level: Level::Warning, msg },
                Some(path) => {
                    let path = path.clone();
                    match (w.line, w.col, &w.source, w.error_code) {
                        (None, _, _, code) => LogCall::Path { level: Level::Warning, path, msg, code },
                        (Some(line), None, _, code) => LogCall::PathLine { level: Level::Warning, path, line, msg, code },
                        (Some(line), Some(col), None, None) => LogCall::PathLineCol { level: Level::Warning, path, line, col, msg },
                        (Some(line), Some(_), None, Some(code)) => LogCall::PathLine { level: Level::Warning, path, line, msg, code: Some(code) },
                        (Some(line), Some(col), Some(source), code) => LogCall::PathLineColSource { level: Level::Warning, path, line, col, msg, source: source.clone(), code },
                    }
                }
            };
            dispatch(call);
        }
    }
}
