//! Ports of `flex2.compiler.util`: `Name`, `QName`, `MultiName`, `QNameList`, `MultiNameMap`, `NameFormatter` and `MimeMappings`.

use std::fmt;

use crate::asc::util::jhash::{string_hash, JHashMap, JavaHash};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QName {
    namespace_uri: String,
    pub local_part: String,
}

impl QName {
    /// `new QName(qname)`: splits at the first colon.
    pub fn parse(qname: &str) -> QName {
        match qname.find(':') {
            None => QName { namespace_uri: String::new(), local_part: qname.to_string() },
            Some(i) => QName { namespace_uri: qname[..i].to_string(), local_part: qname[i + 1..].to_string() },
        }
    }

    pub fn new(namespace_uri: &str, local_part: &str) -> QName {
        QName { namespace_uri: namespace_uri.to_string(), local_part: local_part.to_string() }
    }

    pub fn get_namespace(&self) -> &str {
        &self.namespace_uri
    }

    pub fn set_namespace(&mut self, namespace_uri: &str) {
        self.namespace_uri = namespace_uri.to_string();
    }

    pub fn get_local_part(&self) -> &str {
        &self.local_part
    }

    pub fn equals(&self, namespace_uri: &str, local_part: &str) -> bool {
        self.namespace_uri == namespace_uri && self.local_part == local_part
    }
}

impl JavaHash for QName {
    fn java_hash(&self) -> i32 {
        if !self.namespace_uri.is_empty() {
            string_hash(&self.namespace_uri) ^ string_hash(&self.local_part)
        } else {
            string_hash(&self.local_part)
        }
    }
}

impl fmt::Display for QName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.namespace_uri.is_empty() {
            f.write_str(&self.local_part)
        } else {
            write!(f, "{}:{}", self.namespace_uri, self.local_part)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiName {
    pub namespace_uri: Vec<String>,
    pub local_part: String,
}

impl MultiName {
    /// `new MultiName(qname)`.
    pub fn parse(qname: &str) -> MultiName {
        match qname.find(':') {
            None => MultiName { namespace_uri: vec![String::new()], local_part: qname.to_string() },
            Some(i) => MultiName { namespace_uri: vec![qname[..i].to_string()], local_part: qname[i + 1..].to_string() },
        }
    }

    pub fn new(namespace_uri: &str, local_part: &str) -> MultiName {
        MultiName { namespace_uri: vec![namespace_uri.to_string()], local_part: local_part.to_string() }
    }

    pub fn with_namespaces(namespace_uri: Vec<String>, local_part: &str) -> MultiName {
        MultiName { namespace_uri, local_part: local_part.to_string() }
    }

    pub fn get_namespace(&self) -> &[String] {
        &self.namespace_uri
    }

    pub fn get_local_part(&self) -> &str {
        &self.local_part
    }
}

impl JavaHash for MultiName {
    fn java_hash(&self) -> i32 {
        let mut hash = string_hash(&self.local_part);
        for ns in &self.namespace_uri {
            hash ^= string_hash(ns);
        }
        hash
    }
}

impl fmt::Display for MultiName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]::{}", self.namespace_uri.join(", "), self.local_part)
    }
}

/// `flex2.compiler.util.Name`: a `QName` and a `MultiName` are never equal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Name {
    Q(QName),
    M(MultiName),
}

impl Name {
    pub fn local_part(&self) -> &str {
        match self {
            Name::Q(q) => &q.local_part,
            Name::M(m) => &m.local_part,
        }
    }
}

impl JavaHash for Name {
    fn java_hash(&self) -> i32 {
        match self {
            Name::Q(q) => q.java_hash(),
            Name::M(m) => m.java_hash(),
        }
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Name::Q(q) => q.fmt(f),
            Name::M(m) => m.fmt(f),
        }
    }
}

/// `QNameList`: an `ArrayList` that ignores duplicates.
#[derive(Clone, Debug, Default)]
pub struct QNameList(pub Vec<QName>);

impl QNameList {
    pub fn add(&mut self, q: QName) {
        if !self.0.contains(&q) {
            self.0.push(q);
        }
    }

    pub fn contains(&self, ns: &str, name: &str) -> bool {
        self.0.iter().any(|q| q.equals(ns, name))
    }

    pub fn first(&self) -> Option<&QName> {
        self.0.first()
    }

    pub fn last(&self) -> Option<&QName> {
        self.0.last()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn iter(&self) -> std::slice::Iter<'_, QName> {
        self.0.iter()
    }

    /// `getStringSet()`, a `LinkedHashSet` in list order.
    pub fn get_string_set(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::with_capacity(self.0.len());
        for q in &self.0 {
            let s = q.to_string();
            if !v.contains(&s) {
                v.push(s);
            }
        }
        v
    }
}

impl fmt::Display for QNameList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v: Vec<String> = self.0.iter().map(|q| q.to_string()).collect();
        f.write_str(&v.join(";"))
    }
}

/// `MultiNameMap`.
#[derive(Default)]
pub struct MultiNameMap {
    s: Option<JHashMap<MultiName, QName>>,
    preferred_size: usize,
}

impl MultiNameMap {
    pub fn with_capacity(size: usize) -> MultiNameMap {
        MultiNameMap { s: None, preferred_size: size }
    }

    pub fn contains_key(&self, ns: &[String], name: &str) -> bool {
        match &self.s {
            None => false,
            Some(s) => s.contains_key(&MultiName::with_namespaces(ns.to_vec(), name)),
        }
    }

    pub fn put(&mut self, key: MultiName, value: QName) -> Option<QName> {
        let size = self.preferred_size;
        self.s.get_or_insert_with(|| JHashMap::with_capacity(size)).insert(key, value)
    }

    pub fn get(&self, key: &MultiName) -> Option<&QName> {
        self.s.as_ref().and_then(|s| s.get(key))
    }

    pub fn len(&self) -> usize {
        self.s.as_ref().map_or(0, |s| s.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&mut self) {
        if let Some(s) = &mut self.s {
            s.clear();
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&MultiName, &QName)> {
        self.s.iter().flat_map(|s| s.iter())
    }
}

/// `StandardDefs.CLASS_VECTOR`.
pub const CLASS_VECTOR: &str = "__AS3__.vec.Vector";
/// `SymbolTable.VECTOR`.
pub const SYMBOL_VECTOR: &str = "__AS3__.vec:Vector";

pub mod name_formatter {
    use super::{MultiName, QName, CLASS_VECTOR, SYMBOL_VECTOR};

    pub fn to_dot_ns(ns: &str, n: &str) -> String {
        if !ns.is_empty() {
            format!("{ns}.{n}")
        } else {
            n.to_string()
        }
    }

    pub fn to_colon_ns(ns: &str, n: &str) -> String {
        if !ns.is_empty() {
            format!("{ns}:{n}")
        } else {
            n.to_string()
        }
    }

    pub fn to_dot(n: &str) -> String {
        n.replace(':', ".")
    }

    pub fn to_dot_delim(n: &str, delimiter: char) -> String {
        n.replace(delimiter, ".")
    }

    pub fn to_colon(n: &str) -> String {
        if let Some(rest) = n.strip_prefix(CLASS_VECTOR) {
            format!("{SYMBOL_VECTOR}{rest}")
        } else if n.starts_with(SYMBOL_VECTOR) {
            n.to_string()
        } else {
            match to_dot(n).rfind('.') {
                Some(i) if i > 0 => format!("{}:{}", &n[..i], &n[i + 1..]),
                _ => n.to_string(),
            }
        }
    }

    pub fn normalize_package_name(n: &str) -> String {
        n.strip_suffix("$internal").unwrap_or(n).to_string()
    }

    pub fn retrieve_class_name(n: &str) -> String {
        let dot = to_dot(n);
        if dot.starts_with(&format!("{CLASS_VECTOR}.<")) {
            format!("Vector{}", &n[CLASS_VECTOR.len() + 1..])
        } else {
            match dot.rfind('.') {
                None => n.to_string(),
                Some(i) => n[i + 1..].to_string(),
            }
        }
    }

    pub fn to_multi_name(n: &str) -> MultiName {
        match to_dot(n).rfind('.') {
            Some(i) => MultiName::with_namespaces(vec![n[..i].to_string()], &n[i + 1..]),
            None => MultiName::with_namespaces(vec![String::new()], n),
        }
    }

    pub fn to_qname(n: &str) -> QName {
        match to_dot(n).rfind('.') {
            Some(i) => QName::new(&n[..i], &n[i + 1..]),
            None => QName::new("", n),
        }
    }
}

/// `CompilerAPI.constructClassName`.
pub fn construct_class_name(namespace_uri: &str, local_part: &str) -> String {
    if namespace_uri.is_empty() {
        local_part.to_string()
    } else {
        format!("{namespace_uri}:{local_part}")
    }
}

pub mod mime_mappings {
    pub const MXML: &str = "text/mxml";
    pub const AS: &str = "text/as";
    pub const FXG: &str = "text/fxg";
    pub const ABC: &str = "application/x-actionscript-bytecode";
    pub const CSS: &str = "text/css";

    pub fn get_extension(mime_type: &str) -> &'static str {
        match mime_type {
            MXML => ".mxml",
            AS => ".as",
            FXG => ".fxg",
            ABC => ".abc",
            CSS => ".css",
            _ => "",
        }
    }

    pub fn get_mime_type(name: &str) -> Option<&'static str> {
        let lower = name.to_ascii_lowercase();
        [MXML, AS, FXG, ABC, CSS].into_iter().find(|m| lower.ends_with(get_extension(m)))
    }
}

pub mod name_mappings;
