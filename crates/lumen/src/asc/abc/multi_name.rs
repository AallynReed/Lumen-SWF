//! Port of `macromedia.abc.MultiName`.

use super::q_name::java_string_hash;

#[derive(Debug, Clone)]
pub struct MultiName {
    pub local_part: Option<String>,
    pub namespace_uri: Option<Vec<Option<String>>>,
}

impl MultiName {
    pub fn new(name: Option<String>, namespaces: Option<Vec<Option<String>>>) -> MultiName {
        MultiName { local_part: name, namespace_uri: namespaces }
    }

    pub fn hash_code(&self) -> i32 {
        let local = java_string_hash(self.local_part.as_deref().unwrap_or(""));
        match self.namespace_uri.as_deref() {
            Some(ns) if !ns.is_empty() => {
                let mut hash = java_string_hash(ns[0].as_deref().unwrap_or(""));
                for n in &ns[1..] {
                    hash ^= java_string_hash(n.as_deref().unwrap_or(""));
                }
                hash ^ local
            }
            _ => local,
        }
    }
}

impl PartialEq for MultiName {
    fn eq(&self, m_name: &MultiName) -> bool {
        let ns_uri = m_name.namespace_uri.as_deref().unwrap_or(&[]);
        let namespace_uri = self.namespace_uri.as_deref().unwrap_or(&[]);
        if ns_uri.len() != namespace_uri.len() {
            return false;
        }
        let matched = ns_uri.iter().zip(namespace_uri).all(|(a, b)| a == b);
        matched && m_name.local_part == self.local_part
    }
}

impl std::fmt::Display for MultiName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("{")?;
        let ns = self.namespace_uri.as_deref().unwrap_or(&[]);
        for (i, n) in ns.iter().enumerate() {
            f.write_str(n.as_deref().unwrap_or("null"))?;
            if i < ns.len() - 1 {
                f.write_str(",")?;
            }
        }
        write!(f, "}}::{}", self.local_part.as_deref().unwrap_or("null"))
    }
}
