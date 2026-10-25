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
}

impl LocalLogger {
    pub fn new() -> Rc<LocalLogger> {
        Rc::new(LocalLogger { error_count: Cell::new(0), warning_count: Cell::new(0), connected: Cell::new(true) })
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
fn line_text(path: &str, line: i32) -> String {
    let Ok(bytes) = std::fs::read(path) else { return String::new() };
    let text = String::from_utf8_lossy(&bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
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

/// `logError(path, line, col, error, source, errorCode)` and its warning twin, as ASC's handler calls them.
pub fn log_asc(level: Level, path: &str, line: i32, col: i32, msg: &str, source: &str, code: i32) {
    if !forward(level) {
        return;
    }
    let text = format!("{}({}): col: {} {}: {}\n\n{}\n{}", path, line, col, level_word(level), msg, source, line_pointer(col, Some(source)));
    console(Diagnostic { level, path: Some(path.to_string()), line, col, message: msg.to_string(), code, text });
}

/// `logError(path, error)` / `logWarning(path, warning)`.
pub fn log_path(level: Level, path: &str, msg: &str, code: i32) {
    if !forward(level) {
        return;
    }
    let text = format!("{}\n\n{}: {}", path, level_word(level), msg);
    console(Diagnostic { level, path: Some(path.to_string()), line: -1, col: -1, message: msg.to_string(), code, text });
}

/// `logError(error)`.
pub fn log_plain(level: Level, msg: &str) {
    if !forward(level) {
        return;
    }
    let text = if level == Level::Info { msg.to_string() } else { format!("{}: {}", level_word(level), msg) };
    console(Diagnostic { level, path: None, line: -1, col: -1, message: msg.to_string(), code: -1, text });
}

/// `ThreadLocalToolkit.log(message, path, line, col, source)`, and the shorter overloads with `None`s.
pub fn log(mut m: CompilerMessage, path: Option<&str>, line: i32, col: i32, source: Option<&str>) {
    if let Some(p) = path {
        m.path = Some(p.to_string());
        m.line = line;
        m.col = col;
    }
    if !forward(m.level) {
        return;
    }
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
    let mut src_owned = None;
    if let (Some(p), true) = (&m.path, m.line != -1) {
        let s = match source {
            Some(s) => s.to_string(),
            None => line_text(p, m.line),
        };
        ex.push_str("\n\n");
        ex.push_str(&s);
        src_owned = Some(s);
    }
    if m.col != -1 {
        ex.push('\n');
        ex.push_str(&line_pointer(m.col, source.or(src_owned.as_deref())));
    }
    let text = format!("{prefix}{}{ex}", m.text);
    console(Diagnostic { level: m.level, path: m.path.clone(), line: m.line, col: m.col, message: m.text, code: -1, text });
}

/// `ThreadLocalToolkit.log(message, source)`: a message about a whole source.
pub fn log_for(m: CompilerMessage, name_for_reporting: &str) {
    log(m, Some(name_for_reporting), -1, -1, None);
}

/// `logError(path, line, error)` / `logWarning(path, line, warning)` (CSS parser messages).
pub fn log_path_line(level: Level, path: &str, line: i32, msg: &str) {
    if !forward(level) {
        return;
    }
    let sep = if level == Level::Error { ":  " } else { ": " };
    let text = format!("{}({}){}{}: {}\n\n{}", path, line, sep, level_word(level), msg, line_text(path, line));
    console(Diagnostic { level, path: Some(path.to_string()), line, col: -1, message: msg.to_string(), code: -1, text });
}
