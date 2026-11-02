//! Port of swfutils `flash.fonts.FontDescription`, plus the `java.net.URL` of a font file it can carry.

use std::path::Path;

/// A `file:` `java.net.URL` as `File.toURL()` makes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JUrl {
    /// `toExternalForm()`.
    pub external_form: String,
    /// The local path it was made from.
    pub path: String,
}

impl JUrl {
    /// `file.toURL()`: the absolute path with `/` separators, a leading `/`, and a trailing one for a directory.
    pub fn from_path(p: &Path) -> JUrl {
        let abs = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().map(|d| d.join(p)).unwrap_or_else(|_| p.to_path_buf()) };
        let mut s = normalize_windows(&abs.to_string_lossy()).replace('\\', "/");
        if !s.starts_with('/') {
            s.insert(0, '/');
        }
        if !s.ends_with('/') && abs.is_dir() {
            s.push('/');
        }
        JUrl { external_form: format!("file:{s}"), path: abs.to_string_lossy().into_owned() }
    }

    pub fn to_external_form(&self) -> &str {
        &self.external_form
    }

    /// `getProtocol()`.
    pub fn get_protocol(&self) -> &str {
        self.external_form.split(':').next().unwrap_or("")
    }
}

/// `WinNTFileSystem.normalize`: `/` to `\` and repeated separators collapsed (a leading `\\` kept).
fn normalize_windows(s: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = false;
    for (i, c) in s.chars().enumerate() {
        let sep = c == '\\' || c == '/';
        if sep {
            if prev_sep && i > 1 {
                continue;
            }
            out.push('\\');
        } else {
            out.push(c);
        }
        prev_sep = sep;
    }
    if out.len() > 3 && out.ends_with('\\') {
        out.pop();
    }
    out
}

/// `FontDescription.source`: a `URL` or a font name `String`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontSource {
    Url(JUrl),
    Name(String),
}

impl std::fmt::Display for FontSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontSource::Url(u) => f.write_str(&u.external_form),
            FontSource::Name(n) => f.write_str(n),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FontDescription {
    pub alias: Option<String>,
    pub source: Option<FontSource>,
    pub style: i32,
    pub unicode_ranges: Option<String>,
    pub advanced_anti_aliasing: bool,
    pub compact_font_format: bool,
}

/// `equals`: everything but `alias`.
impl PartialEq for FontDescription {
    fn eq(&self, o: &Self) -> bool {
        self.style == o.style && self.compact_font_format == o.compact_font_format && self.advanced_anti_aliasing == o.advanced_anti_aliasing && self.unicode_ranges == o.unicode_ranges && self.source == o.source
    }
}
