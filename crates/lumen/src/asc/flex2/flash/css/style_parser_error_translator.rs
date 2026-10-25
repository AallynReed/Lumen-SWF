//! Port of `flash.css.StyleParserErrorTranslator`: Batik's messages as mxmlc reports them.

use crate::asc::flex2::batik::css::parser::lexical_units as lu;

fn token(t: &str) -> String {
    format!("Invalid CSS syntax(Unexpected token: '{t}').")
}

fn syntax(t: &str) -> String {
    format!("Invalid CSS syntax(Unexpected {t}).")
}

fn units(t: &str) -> String {
    format!("Invalid CSS syntax(Unexpected {t} units).")
}

fn array(t: &str) -> String {
    format!("Invalid CSS syntax(Unexpected token: '{t}'). To set styles of type Array, use a comma delimited list.  For example, {{ foo: red, blue, green }}.")
}

/// `MessageFormat("Unexpected token: {0,number,integer} (see LexicalUnits).").parse(m)`.
fn parse_token(m: &str) -> Option<i64> {
    let rest = m.strip_prefix("Unexpected token: ")?;
    let end = rest.find(" (see LexicalUnits).")?;
    let digits: String = rest[..end].chars().filter(|&c| c != ',').collect();
    digits.parse::<i64>().ok()
}

pub fn get_user_friendly_errror(batik_message: Option<&str>) -> Option<String> {
    let m = batik_message?;
    if m.starts_with("Unexpected token:") {
        let Some(code) = parse_token(m) else { return Some(m.to_string()) };
        let r = match code as i32 {
            lu::ANY => token("*"),
            lu::AT_KEYWORD => token("@ident"),
            lu::CDC => token("-->"),
            lu::CDO => token("'"),
            lu::EOF => token("EOF"),
            lu::LEFT_CURLY_BRACE => token("{"),
            lu::RIGHT_CURLY_BRACE => token("}"),
            lu::EQUAL => token("="),
            lu::PLUS => token("+"),
            lu::MINUS => token("-"),
            lu::COMMA => token(","),
            lu::DOT => token("."),
            lu::SEMI_COLON => token(";"),
            lu::PRECEDE => token(">"),
            lu::DIVIDE => token("/"),
            lu::LEFT_BRACKET => array("["),
            lu::RIGHT_BRACKET => token("]"),
            lu::LEFT_BRACE => token("("),
            lu::RIGHT_BRACE => token(")"),
            lu::COLON => token(":"),
            lu::SPACE => syntax("space"),
            lu::COMMENT => syntax("comment"),
            lu::STRING => syntax("string"),
            lu::IDENTIFIER => syntax("identifier"),
            lu::IMPORTANT_SYMBOL => token("!important"),
            lu::INTEGER => syntax("integer"),
            lu::DASHMATCH => token("|="),
            lu::INCLUDES => token("~="),
            lu::HASH => token("#"),
            lu::IMPORT_SYMBOL => token("@import"),
            lu::CHARSET_SYMBOL => token("@charset"),
            lu::FONT_FACE_SYMBOL => token("@font-face"),
            lu::MEDIA_SYMBOL => token("@media"),
            lu::PAGE_SYMBOL => token("@page"),
            lu::DIMENSION => syntax("dimension"),
            lu::EX => units("ex"),
            lu::EM => units("em"),
            lu::CM => units("cm"),
            lu::MM => units("mm"),
            lu::IN => units("in"),
            lu::MS => units("ms"),
            lu::HZ => units("hz"),
            lu::PERCENTAGE => units("percentage"),
            lu::S => units("S"),
            lu::PC => units("pc"),
            lu::PT => units("pt"),
            lu::PX => units("px"),
            lu::DEG => units("deg"),
            lu::RAD => units("rad"),
            lu::GRAD => units("grad"),
            lu::KHZ => units("khz"),
            lu::URI => syntax("URI"),
            lu::FUNCTION => syntax("identifier"),
            lu::UNICODE_RANGE => syntax("unicode range"),
            lu::REAL => syntax("real number"),
            _ => m.to_string(),
        };
        Some(r)
    } else if m == "Invalid identifier start character: _." {
        Some("Invalid character found in CSS. Use quotes to surround font family names such as '_sans'.".to_string())
    } else if m == "character" || m == "identifier.character" {
        Some("Unable to parse CSS file.".to_string())
    } else {
        Some(m.to_string())
    }
}
