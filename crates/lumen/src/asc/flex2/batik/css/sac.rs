//! Port of the SAC interfaces Flex's Batik fork uses (`org.w3c.flex.forks.css.sac`): the constants, `CSSException`,
//! `CSSParseException`, `DocumentHandler`, `ErrorHandler` and `SACMediaList`.

use super::parser::css_lexical_unit::LexicalUnitRef;
use super::parser::css_sac_media_list::CSSSACMediaList;
use super::parser::css_selector_list::CSSSelectorList;

pub mod lexical_unit {
    pub const SAC_OPERATOR_COMMA: i16 = 0;
    pub const SAC_OPERATOR_PLUS: i16 = 1;
    pub const SAC_OPERATOR_MINUS: i16 = 2;
    pub const SAC_OPERATOR_MULTIPLY: i16 = 3;
    pub const SAC_OPERATOR_SLASH: i16 = 4;
    pub const SAC_OPERATOR_MOD: i16 = 5;
    pub const SAC_OPERATOR_EXP: i16 = 6;
    pub const SAC_OPERATOR_LT: i16 = 7;
    pub const SAC_OPERATOR_GT: i16 = 8;
    pub const SAC_OPERATOR_LE: i16 = 9;
    pub const SAC_OPERATOR_GE: i16 = 10;
    pub const SAC_OPERATOR_TILDE: i16 = 11;
    pub const SAC_INHERIT: i16 = 12;
    pub const SAC_INTEGER: i16 = 13;
    pub const SAC_REAL: i16 = 14;
    pub const SAC_EM: i16 = 15;
    pub const SAC_EX: i16 = 16;
    pub const SAC_PIXEL: i16 = 17;
    pub const SAC_INCH: i16 = 18;
    pub const SAC_CENTIMETER: i16 = 19;
    pub const SAC_MILLIMETER: i16 = 20;
    pub const SAC_POINT: i16 = 21;
    pub const SAC_PICA: i16 = 22;
    pub const SAC_PERCENTAGE: i16 = 23;
    pub const SAC_URI: i16 = 24;
    pub const SAC_COUNTER_FUNCTION: i16 = 25;
    pub const SAC_COUNTERS_FUNCTION: i16 = 26;
    pub const SAC_RGBCOLOR: i16 = 27;
    pub const SAC_DEGREE: i16 = 28;
    pub const SAC_GRADIAN: i16 = 29;
    pub const SAC_RADIAN: i16 = 30;
    pub const SAC_MILLISECOND: i16 = 31;
    pub const SAC_SECOND: i16 = 32;
    pub const SAC_HERTZ: i16 = 33;
    pub const SAC_KILOHERTZ: i16 = 34;
    pub const SAC_IDENT: i16 = 35;
    pub const SAC_STRING_VALUE: i16 = 36;
    pub const SAC_ATTR: i16 = 37;
    pub const SAC_RECT_FUNCTION: i16 = 38;
    pub const SAC_UNICODERANGE: i16 = 39;
    pub const SAC_SUB_EXPRESSION: i16 = 40;
    pub const SAC_FUNCTION: i16 = 41;
    pub const SAC_DIMENSION: i16 = 42;
    pub const SAC_DPI: i16 = 43;
    pub const SAC_DPCM: i16 = 44;
}

pub mod selector {
    pub const SAC_CONDITIONAL_SELECTOR: i16 = 0;
    pub const SAC_ANY_NODE_SELECTOR: i16 = 1;
    pub const SAC_ROOT_NODE_SELECTOR: i16 = 2;
    pub const SAC_NEGATIVE_SELECTOR: i16 = 3;
    pub const SAC_ELEMENT_NODE_SELECTOR: i16 = 4;
    pub const SAC_TEXT_NODE_SELECTOR: i16 = 5;
    pub const SAC_CDATA_SECTION_NODE_SELECTOR: i16 = 6;
    pub const SAC_PROCESSING_INSTRUCTION_NODE_SELECTOR: i16 = 7;
    pub const SAC_COMMENT_NODE_SELECTOR: i16 = 8;
    pub const SAC_PSEUDO_ELEMENT_SELECTOR: i16 = 9;
    pub const SAC_DESCENDANT_SELECTOR: i16 = 10;
    pub const SAC_CHILD_SELECTOR: i16 = 11;
    pub const SAC_DIRECT_ADJACENT_SELECTOR: i16 = 12;
}

pub mod condition {
    pub const SAC_AND_CONDITION: i16 = 0;
    pub const SAC_OR_CONDITION: i16 = 1;
    pub const SAC_NEGATIVE_CONDITION: i16 = 2;
    pub const SAC_POSITIONAL_CONDITION: i16 = 3;
    pub const SAC_ATTRIBUTE_CONDITION: i16 = 4;
    pub const SAC_ID_CONDITION: i16 = 5;
    pub const SAC_LANG_CONDITION: i16 = 6;
    pub const SAC_ONE_OF_ATTRIBUTE_CONDITION: i16 = 7;
    pub const SAC_BEGIN_HYPHEN_ATTRIBUTE_CONDITION: i16 = 8;
    pub const SAC_CLASS_CONDITION: i16 = 9;
    pub const SAC_PSEUDO_CLASS_CONDITION: i16 = 10;
    pub const SAC_ONLY_CHILD_CONDITION: i16 = 11;
    pub const SAC_ONLY_TYPE_CONDITION: i16 = 12;
    pub const SAC_CONTENT_CONDITION: i16 = 13;
}

pub const SAC_UNSPECIFIED_ERR: i16 = 0;
pub const SAC_NOT_SUPPORTED_ERR: i16 = 1;
pub const SAC_SYNTAX_ERR: i16 = 2;

/// `CSSException`.
#[derive(Clone, Debug)]
pub struct CSSException {
    pub s: Option<String>,
    pub code: i16,
    /// The wrapped exception's message, for `getMessage()`.
    pub e: Option<Option<String>>,
}

impl CSSException {
    pub fn new(s: impl Into<String>) -> CSSException {
        CSSException { s: Some(s.into()), code: SAC_UNSPECIFIED_ERR, e: None }
    }

    pub fn with(code: i16, s: Option<String>) -> CSSException {
        CSSException { s, code, e: None }
    }

    pub fn get_message(&self) -> Option<String> {
        match (&self.s, &self.e) {
            (Some(s), _) => Some(s.clone()),
            (None, Some(e)) => e.clone(),
            _ => None,
        }
    }
}

/// `CSSParseException`.
#[derive(Clone, Debug)]
pub struct CSSParseException {
    pub message: String,
    pub uri: Option<String>,
    pub line_number: i32,
    pub column_number: i32,
}

impl CSSParseException {
    pub fn get_message(&self) -> Option<String> {
        Some(self.message.clone())
    }

    pub fn get_line_number(&self) -> i32 {
        self.line_number
    }

    pub fn get_column_number(&self) -> i32 {
        self.column_number
    }
}

/// What a SAC call can throw: a `CSSParseException`, another `CSSException`, or any other `RuntimeException`, with
/// its `getLocalizedMessage()`.
#[derive(Clone, Debug)]
pub enum CssError {
    Parse(CSSParseException),
    Css(CSSException),
    Runtime(Option<String>),
}

impl CssError {
    pub fn get_localized_message(&self) -> Option<String> {
        match self {
            CssError::Parse(e) => e.get_message(),
            CssError::Css(e) => e.get_message(),
            CssError::Runtime(m) => m.clone(),
        }
    }
}

pub type CssResult<T> = Result<T, CssError>;

/// `DocumentHandler`.
pub trait DocumentHandler {
    fn start_document(&mut self) -> CssResult<()>;
    fn end_document(&mut self) -> CssResult<()>;
    fn comment(&mut self, text: &str) -> CssResult<()>;
    fn ignorable_at_rule(&mut self, at_rule: &str) -> CssResult<()>;
    fn namespace_declaration(&mut self, prefix: Option<&str>, uri: Option<&str>) -> CssResult<()>;
    fn import_style(&mut self, uri: Option<&str>, media: &CSSSACMediaList, default_namespace_uri: Option<&str>) -> CssResult<()>;
    fn start_media(&mut self, media: &CSSSACMediaList) -> CssResult<()>;
    fn end_media(&mut self, media: &CSSSACMediaList) -> CssResult<()>;
    fn start_page(&mut self, name: Option<&str>, pseudo_page: Option<&str>) -> CssResult<()>;
    fn end_page(&mut self, name: Option<&str>, pseudo_page: Option<&str>) -> CssResult<()>;
    fn start_font_face(&mut self) -> CssResult<()>;
    fn end_font_face(&mut self) -> CssResult<()>;
    fn start_selector(&mut self, selectors: &CSSSelectorList) -> CssResult<()>;
    fn end_selector(&mut self, selectors: &CSSSelectorList) -> CssResult<()>;
    fn property(&mut self, name: &str, value: &LexicalUnitRef, important: bool) -> CssResult<()>;
}

/// `ErrorHandler`.
pub trait ErrorHandler {
    fn warning(&mut self, exception: CSSParseException) -> CssResult<()>;
    fn error(&mut self, exception: CSSParseException) -> CssResult<()>;
    fn fatal_error(&mut self, exception: CSSParseException) -> CssResult<()>;
}
