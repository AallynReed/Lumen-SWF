//! Port of afe.jar `com.adobe.fontengine.FontEngineException` and its `font` subclasses (`InvalidFontException`, ...).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExceptionKind {
    FontEngine,
    InvalidFont,
    UnsupportedFont,
    FontLoading,
    InvalidGlyph,
    /// `java.io.IOException`, which afe lets through its `throws` clauses.
    Io,
}

impl ExceptionKind {
    pub fn class_name(self) -> &'static str {
        match self {
            ExceptionKind::FontEngine => "com.adobe.fontengine.FontEngineException",
            ExceptionKind::InvalidFont => "com.adobe.fontengine.font.InvalidFontException",
            ExceptionKind::UnsupportedFont => "com.adobe.fontengine.font.UnsupportedFontException",
            ExceptionKind::FontLoading => "com.adobe.fontengine.font.FontLoadingException",
            ExceptionKind::InvalidGlyph => "com.adobe.fontengine.font.InvalidGlyphException",
            ExceptionKind::Io => "java.io.IOException",
        }
    }
}

/// A thrown afe exception: its class and `getMessage()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontEngineException {
    pub kind: ExceptionKind,
    pub message: Option<String>,
}

pub type FontResult<T> = Result<T, FontEngineException>;

impl FontEngineException {
    pub fn new(kind: ExceptionKind, message: impl Into<String>) -> FontEngineException {
        FontEngineException { kind, message: Some(message.into()) }
    }
    /// `new X()`.
    pub fn bare(kind: ExceptionKind) -> FontEngineException {
        FontEngineException { kind, message: None }
    }
    /// `new X(throwable)`, whose message is `throwable.toString()`.
    pub fn from_cause(kind: ExceptionKind, cause_to_string: impl Into<String>) -> FontEngineException {
        FontEngineException { kind, message: Some(cause_to_string.into()) }
    }
    pub fn invalid_font(message: impl Into<String>) -> FontEngineException {
        Self::new(ExceptionKind::InvalidFont, message)
    }
    pub fn unsupported_font(message: impl Into<String>) -> FontEngineException {
        Self::new(ExceptionKind::UnsupportedFont, message)
    }
    pub fn font_loading(message: impl Into<String>) -> FontEngineException {
        Self::new(ExceptionKind::FontLoading, message)
    }
    pub fn invalid_glyph(message: impl Into<String>) -> FontEngineException {
        Self::new(ExceptionKind::InvalidGlyph, message)
    }
    pub fn io(message: impl Into<String>) -> FontEngineException {
        Self::new(ExceptionKind::Io, message)
    }
    /// `getMessage()`.
    pub fn get_message(&self) -> Option<&str> {
        self.message.as_deref()
    }
    /// `Throwable.toString()`.
    pub fn to_java_string(&self) -> String {
        match &self.message {
            Some(m) => format!("{}: {}", self.kind.class_name(), m),
            None => self.kind.class_name().to_string(),
        }
    }
    pub fn is(&self, kind: ExceptionKind) -> bool {
        self.kind == kind
    }
}

impl FontEngineException {
    /// `instanceof FontException` (every afe font exception but the base class and `IOException`).
    pub fn is_font_exception(&self) -> bool {
        !matches!(self.kind, ExceptionKind::FontEngine | ExceptionKind::Io)
    }
}
