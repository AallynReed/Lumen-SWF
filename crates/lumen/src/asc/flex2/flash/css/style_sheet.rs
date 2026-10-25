//! Port of `flash.css.StyleSheet`.

use super::rule::RuleRef;
use super::style_parser::StyleParser;

#[derive(Default)]
pub struct StyleSheet {
    rules: Option<Vec<RuleRef>>,
    disabled: bool,
    errors_exist: bool,
    check_deprecation: bool,
}

impl StyleSheet {
    pub fn new() -> StyleSheet {
        StyleSheet::default()
    }

    pub fn check_deprecation(&mut self, check_deprecation: bool) {
        self.check_deprecation = check_deprecation;
    }

    fn finish(&mut self, parser: &StyleParser) -> bool {
        self.rules = Some(parser.get_rules());
        self.errors_exist = parser.errors_exist();
        !self.errors_exist
    }

    /// `parse(mxmlPath, mxmlLineNumber, reader, ...)`: false where Java returns null.
    pub fn parse_mxml(&mut self, mxml_path: &str, mxml_line_number: i32, chars: Vec<u16>) -> bool {
        let p = StyleParser::from_mxml(mxml_path, mxml_line_number, chars, self.check_deprecation);
        self.finish(&p)
    }

    /// `parse(cssPath, reader, ...)`.
    pub fn parse_reader(&mut self, css_path: &str, chars: Vec<u16>) -> bool {
        let p = StyleParser::from_reader(css_path, chars, self.check_deprecation);
        self.finish(&p)
    }

    /// `parse(name, cssIn, ...)`.
    pub fn parse(&mut self, name: &str, css_in: &[u8]) -> bool {
        let p = StyleParser::from_stream(name, css_in, self.check_deprecation);
        self.finish(&p)
    }

    pub fn errors_exist(&self) -> bool {
        self.errors_exist
    }

    pub fn delete_rule(&mut self, index: usize) {
        if let Some(r) = &mut self.rules {
            r.remove(index);
        }
    }

    pub fn get_css_rules(&self) -> Option<&Vec<RuleRef>> {
        self.rules.as_ref()
    }

    pub fn get_disabled(&self) -> bool {
        self.disabled
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }
}
