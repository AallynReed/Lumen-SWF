//! Port of `macromedia.abc.QName`.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QName {
    namespace_uri: Option<String>,
    local_part: Option<String>,
}

/// `String.hashCode()`.
pub(crate) fn java_string_hash(s: &str) -> i32 {
    crate::asc::util::jstr::units(&s).fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32))
}

impl QName {
    pub fn new(namespace_uri: Option<String>, local_part: Option<String>) -> QName {
        QName { namespace_uri, local_part }
    }

    pub fn get_namespace(&self) -> Option<&str> {
        self.namespace_uri.as_deref()
    }

    pub fn get_local_part(&self) -> Option<&str> {
        self.local_part.as_deref()
    }

    pub fn hash_code(&self) -> i32 {
        java_string_hash(self.namespace_uri.as_deref().unwrap_or("")) ^ java_string_hash(self.local_part.as_deref().unwrap_or(""))
    }
}

impl std::fmt::Display for QName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let local = self.local_part.as_deref().unwrap_or("null");
        match self.namespace_uri.as_deref() {
            None | Some("") => f.write_str(local),
            Some(ns) => write!(f, "{}:{}", ns, local),
        }
    }
}
