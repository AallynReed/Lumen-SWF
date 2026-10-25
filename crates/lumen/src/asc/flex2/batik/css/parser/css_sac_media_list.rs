//! Ports of `CSSSACMediaList`, `CSSSACMediaQuery` and `CSSSACMediaExpression` (Flex's media query support).

use super::css_lexical_unit::LexicalUnitRef;

#[derive(Debug)]
pub struct CSSSACMediaExpression {
    media_feature: String,
    expr: Option<LexicalUnitRef>,
}

impl CSSSACMediaExpression {
    pub fn new(media_feature: String, expr: Option<LexicalUnitRef>) -> CSSSACMediaExpression {
        CSSSACMediaExpression { media_feature, expr }
    }

    pub fn get_media_feature(&self) -> &str {
        &self.media_feature
    }

    pub fn get_expr(&self) -> Option<&LexicalUnitRef> {
        self.expr.as_ref()
    }

    pub fn append_to_string_buffer(&self, target: &mut String) {
        target.push('(');
        target.push_str(&self.media_feature);
        if let Some(e) = &self.expr {
            target.push(':');
            e.append_to_string_buffer(target);
        }
        target.push(')');
    }
}

#[derive(Debug)]
pub struct CSSSACMediaQuery {
    not: bool,
    media_type: Option<String>,
    expressions: Vec<CSSSACMediaExpression>,
}

impl CSSSACMediaQuery {
    pub fn new(not: bool, media_type: Option<String>) -> CSSSACMediaQuery {
        CSSSACMediaQuery { not, media_type, expressions: Vec::new() }
    }

    pub fn is_not(&self) -> bool {
        self.not
    }

    pub fn get_media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }

    pub fn get_num_expressions(&self) -> i32 {
        self.expressions.len() as i32
    }

    pub fn append_expression(&mut self, expr: Option<CSSSACMediaExpression>) {
        if let Some(e) = expr {
            self.expressions.push(e);
        }
    }

    pub fn get_expression(&self, index: i32) -> Option<&CSSSACMediaExpression> {
        if index < 0 {
            return None;
        }
        self.expressions.get(index as usize)
    }
}

impl std::fmt::Display for CSSSACMediaQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut buf = String::new();
        if let Some(t) = &self.media_type {
            if self.not {
                buf.push_str("not ");
            }
            buf.push_str(t);
            for e in &self.expressions {
                buf.push_str(" and ");
                e.append_to_string_buffer(&mut buf);
            }
        } else if !self.expressions.is_empty() {
            for e in &self.expressions[..self.expressions.len() - 1] {
                e.append_to_string_buffer(&mut buf);
                buf.push_str(" and ");
            }
            self.expressions[self.expressions.len() - 1].append_to_string_buffer(&mut buf);
        }
        f.write_str(&buf)
    }
}

#[derive(Debug, Default)]
pub struct CSSSACMediaList {
    list: Vec<CSSSACMediaQuery>,
}

impl CSSSACMediaList {
    pub fn new() -> CSSSACMediaList {
        CSSSACMediaList::default()
    }

    /// `CSSSACMediaList.ALL`.
    pub fn all() -> CSSSACMediaList {
        CSSSACMediaList { list: vec![CSSSACMediaQuery::new(false, Some("all".into()))] }
    }

    pub fn get_length(&self) -> i32 {
        self.list.len() as i32
    }

    pub fn item(&self, index: i32) -> Option<String> {
        self.item_as_query(index).map(|q| q.to_string())
    }

    pub fn item_as_query(&self, index: i32) -> Option<&CSSSACMediaQuery> {
        if index < 0 {
            return None;
        }
        self.list.get(index as usize)
    }

    /// A missing query becomes `QUERY_NOT_ALL`.
    pub fn append(&mut self, item: Option<CSSSACMediaQuery>) {
        self.list.push(item.unwrap_or_else(|| CSSSACMediaQuery::new(true, Some("all".into()))));
    }

    pub fn append_str(&mut self, item: &str) {
        self.append(Some(CSSSACMediaQuery::new(false, Some(item.to_string()))));
    }
}
