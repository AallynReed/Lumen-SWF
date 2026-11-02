//! Port of `flex2.compiler.mxml.gen.TextGen`.

/// `quoteWord(s)`: no escaping, hence the name.
pub fn quote_word(s: &str) -> String {
    format!("\"{s}\"")
}

/// `toSepList(stringIter, sep)`.
pub fn to_sep_list<S: AsRef<str>>(string_iter: impl IntoIterator<Item = S>, sep: &str) -> String {
    let mut sb = String::new();
    for (i, s) in string_iter.into_iter().enumerate() {
        if i > 0 {
            sb.push_str(sep);
        }
        sb.push_str(s.as_ref());
    }
    sb
}

/// `toCommaList(stringIter)`.
pub fn to_comma_list<S: AsRef<str>>(string_iter: impl IntoIterator<Item = S>) -> String {
    to_sep_list(string_iter, ", ")
}

/// `prefix(stringIter, prefix)`.
pub fn prefix<S: AsRef<str>>(string_iter: impl IntoIterator<Item = S>, prefix: &str) -> String {
    let mut sb = String::new();
    for s in string_iter {
        sb.push_str(prefix);
        sb.push_str(s.as_ref());
    }
    sb
}

/// `stripParens(sourceExpression)`.
pub fn strip_parens(source_expression: &str) -> String {
    if source_expression.starts_with('(') && source_expression.ends_with(')') {
        let n = crate::asc::util::jstr::utf16_len(source_expression);
        return crate::asc::util::jstr::substring(source_expression, 1, n - 1);
    }
    source_expression.to_string()
}
