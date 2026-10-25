//! Dotted names, identifier escaping and multiname resolution as printed in AS3.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::abc::{ns_kind, Abc, Multiname};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Part {
    pub name: String,
    pub attr: bool,
    pub suffix: String,
}

/// A dotted name chain.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Dotted(pub Vec<Part>);

/// Java's `String.split("\\.")`, which drops trailing empty parts.
fn java_split(s: &str) -> Vec<&str> {
    let mut v: Vec<&str> = s.split('.').collect();
    while v.last() == Some(&"") {
        v.pop();
    }
    v
}

fn split_suffix(part: &str) -> (&str, &str) {
    if let Some(p) = part.rfind('#') {
        let digits = &part[p + 1..];
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            return (&part[..p], &part[p..]);
        }
    }
    (part, "")
}

impl Dotted {
    pub fn new(parts: &[&str]) -> Dotted {
        Dotted(parts.iter().map(|p| Part { name: p.to_string(), ..Default::default() }).collect())
    }

    pub fn one(name: &str, suffix: &str) -> Dotted {
        Dotted(vec![Part { name: name.into(), attr: false, suffix: suffix.into() }])
    }

    pub fn parse_no_suffix(s: &str) -> Dotted {
        if s.is_empty() {
            return Dotted::default();
        }
        Dotted(java_split(s).into_iter().map(|p| Part { name: p.into(), ..Default::default() }).collect())
    }

    pub fn parse_with_suffix(s: &str) -> Dotted {
        if s.is_empty() {
            return Dotted::default();
        }
        Dotted(
            java_split(s)
                .into_iter()
                .map(|p| {
                    let (n, sfx) = split_suffix(p);
                    Part { name: n.into(), attr: false, suffix: sfx.into() }
                })
                .collect(),
        )
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn last(&self) -> &str {
        self.0.last().map_or("", |p| p.name.as_str())
    }

    pub fn without_last(&self) -> Dotted {
        if self.0.len() < 2 {
            return Dotted::default();
        }
        Dotted(self.0[..self.0.len() - 1].to_vec())
    }

    pub fn add(&self, name: &str, suffix: &str) -> Dotted {
        let mut v = self.0.clone();
        v.push(Part { name: name.into(), attr: false, suffix: suffix.into() });
        Dotted(v)
    }

    pub fn add_with_suffix(&self, name: &str) -> Dotted {
        let (n, s) = split_suffix(name);
        self.add(n, s)
    }

    fn render(&self, ids: Option<&Idents>, raw: bool, with_suffix: bool) -> String {
        let mut out = String::new();
        for (i, p) in self.0.iter().enumerate() {
            if i > 0 {
                out.push('.');
            }
            if p.attr {
                out.push('@');
            }
            let last_star = i == self.0.len() - 1 && p.name == "*";
            match ids {
                Some(ids) if !raw && !last_star => out.push_str(&ids.print(&p.name)),
                _ => out.push_str(&p.name),
            }
            if with_suffix {
                out.push_str(&p.suffix);
            }
        }
        out
    }

    pub fn raw(&self) -> String {
        self.render(None, true, true)
    }

    pub fn printable(&self, ids: &Idents) -> String {
        self.render(Some(ids), false, true)
    }
}

const RESERVED_AS3: &[&str] = &[
    "as", "break", "case", "catch", "class", "const", "continue", "default", "delete", "do", "else", "extends", "false", "finally", "for",
    "function", "if", "implements", "import", "in", "instanceof", "interface", "internal", "is", "new", "null", "package", "private",
    "protected", "public", "return", "super", "switch", "this", "throw", "true", "try", "typeof", "use", "var", "void", "while", "with",
];

pub fn is_reserved(s: &str) -> bool {
    RESERVED_AS3.contains(&s.trim())
}

fn ident_first(c: char) -> bool {
    c == '_' || c == '$' || c.is_alphabetic()
}

fn ident_next(c: char) -> bool {
    ident_first(c) || c.is_numeric()
}

/// Whether `s` is a valid AS3 identifier.
pub fn is_valid_name(s: &str) -> bool {
    if is_reserved(s) {
        return false;
    }
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if ident_first(c) => cs.all(ident_next),
        _ => false,
    }
}

/// `Name$hex-digits[ByteArray]`, the form Flex gives embedded assets.
fn embed_stem(s: &str) -> Option<String> {
    let (stem, rest) = s.split_once('$')?;
    let mut sc = stem.chars();
    if !sc.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') || !sc.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let (hex, tail) = rest.split_once('-')?;
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let (digits, byte_array) = match tail.strip_suffix("ByteArray") {
        Some(d) => (d, true),
        None => (tail, false),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(if byte_array { format!("{stem}_ByteArray") } else { stem.to_string() })
}

/// `escapeOIdentifier`: the text inside `§...§`.
pub fn escape_oidentifier(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\u{c}' => out.push_str("\\f"),
            '\\' => out.push_str("\\\\"),
            '\u{a7}' => out.push_str("\\\u{a7}"),
            c if (c as u32) < 32 => out.push_str(&format!("\\x{:02X}", c as u32)),
            c => {
                let run = chars[i..].iter().take_while(|&&x| x == c).count();
                if run > 20 {
                    out.push_str(&format!("\\{{{run}}}"));
                    i += run - 1;
                }
                out.push(c);
            }
        }
        i += 1;
    }
    out
}

/// Per-SWF identifier printing state: the obfuscated-identifier map.
#[derive(Default)]
pub struct Idents {
    map: RefCell<Vec<(String, String)>>,
}

impl Idents {
    /// Every identifier printed under another name: (name in the ABC, printed name).
    pub fn renamed(&self) -> Vec<(String, String)> {
        self.map.borrow().clone()
    }

    fn map_get(&self, s: &str) -> Option<String> {
        self.map.borrow().iter().find(|(k, _)| k == s).map(|(_, v)| v.clone())
    }

    /// `IdentifiersDeobfuscation.printIdentifier(swf, used, true, s)`.
    pub fn print(&self, s: &str) -> String {
        if s.is_empty() {
            return String::new();
        }
        if s.starts_with('\u{a7}') && s.ends_with('\u{a7}') {
            return s.to_string();
        }
        if is_valid_name(s) {
            return s.to_string();
        }
        if let Some(stem) = embed_stem(s) {
            if let Some(v) = self.map_get(s) {
                return v;
            }
            let v = {
                let map = self.map.borrow();
                let taken = |c: &str| map.iter().any(|(k, v)| k == c || v == c);
                if is_valid_name(&stem) && !taken(&stem) {
                    stem.clone()
                } else {
                    (2..1000).map(|n| format!("{stem}_{n}")).find(|c| is_valid_name(c) && !taken(c)).unwrap_or_else(|| format!("_SafeStr_{}", map.len()))
                }
            };
            self.map.borrow_mut().push((s.to_string(), v.clone()));
            return v;
        }
        format!("\u{a7}{}\u{a7}", escape_oidentifier(s))
    }
}

/// Multiname name resolution, mirroring `Multiname.getName` and friends.
pub struct Names<'a> {
    pub abc: &'a Abc,
    pub ids: &'a Idents,
    /// Namespace URI to custom namespace name, from `AbcIndexing.nsValueToName`.
    pub custom_ns: &'a HashMap<String, Dotted>,
    pub display_ns: &'a HashSet<usize>,
}

impl<'a> Names<'a> {
    pub fn mn(&self, i: usize) -> Option<&'a Multiname> {
        if i == 0 {
            return None;
        }
        self.abc.constants.multinames.get(i)
    }

    pub fn s(&self, i: usize) -> &'a str {
        self.abc.str(i)
    }

    pub fn ns_name(&self, ns: usize) -> Dotted {
        match self.abc.constants.namespaces.get(ns) {
            Some(n) if ns != 0 && n.name != 0 => Dotted::parse_no_suffix(self.s(n.name)),
            _ => Dotted::default(),
        }
    }

    pub fn ns_raw_name(&self, ns: usize) -> String {
        match self.abc.constants.namespaces.get(ns) {
            Some(n) if ns != 0 && n.name != 0 => self.s(n.name).to_string(),
            _ => String::new(),
        }
    }

    pub fn ns_kind(&self, ns: usize) -> u8 {
        self.abc.constants.namespaces.get(ns).map_or(0, |n| n.kind)
    }

    fn name_index(m: &Multiname) -> usize {
        match m {
            Multiname::QName { name, .. } | Multiname::RtqName { name, .. } | Multiname::Multiname { name, .. } => *name,
            _ => 0,
        }
    }

    pub fn suffix(&self, i: usize) -> String {
        match self.mn(i) {
            Some(Multiname::QName { ns, .. }) if self.display_ns.contains(&i) => format!("#{ns}"),
            Some(_) if self.display_ns.contains(&i) => "#0".into(),
            _ => String::new(),
        }
    }

    pub fn own_ns(&self, i: usize) -> usize {
        match self.mn(i) {
            Some(Multiname::QName { ns, .. }) => *ns,
            _ => 0,
        }
    }

    pub fn ns_set(&self, i: usize) -> Option<&'a Vec<usize>> {
        match self.mn(i) {
            Some(Multiname::Multiname { ns_set, .. } | Multiname::MultinameL { ns_set, .. }) if *ns_set != 0 => self.abc.constants.ns_sets.get(*ns_set),
            _ => None,
        }
    }

    fn strip_api_mark(n: &str) -> &str {
        match n.chars().last() {
            Some(c) if ('\u{e000}'..='\u{f8ff}').contains(&c) => &n[..n.len() - c.len_utf8()],
            _ => n,
        }
    }

    fn nonversioned_names(&self, set: &[usize]) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for &n in set {
            let v = Self::strip_api_mark(&self.ns_raw_name(n)).to_string();
            if !names.contains(&v) {
                names.push(v);
            }
        }
        names
    }

    /// `getSimpleNamespaceName`.
    pub fn simple_ns_name(&self, i: usize) -> Option<Dotted> {
        match self.mn(i)? {
            Multiname::QName { ns, .. } => Some(if *ns == 0 { Dotted::default() } else { self.ns_name(*ns) }),
            Multiname::Multiname { .. } | Multiname::MultinameL { .. } => {
                let set = self.ns_set(i)?;
                let names = self.nonversioned_names(set);
                if names.len() != 1 {
                    return None;
                }
                Some(Dotted::parse_no_suffix(&names[0]))
            }
            _ => None,
        }
    }

    /// `getSimpleNamespaceKind`.
    pub fn simple_ns_kind(&self, i: usize) -> u8 {
        match self.mn(i) {
            Some(Multiname::QName { ns, .. }) => {
                if *ns == 0 {
                    0
                } else {
                    self.ns_kind(*ns)
                }
            }
            Some(Multiname::Multiname { .. } | Multiname::MultinameL { .. }) => {
                let Some(set) = self.ns_set(i) else { return 0 };
                let mut kinds: Vec<u8> = Vec::new();
                for &n in set {
                    let k = self.ns_kind(n);
                    if !kinds.contains(&k) {
                        kinds.push(k);
                    }
                }
                if self.nonversioned_names(set).len() != 1 || kinds.len() != 1 {
                    return 0;
                }
                kinds[0]
            }
            _ => 0,
        }
    }

    /// The namespace used by `getName`: own namespace, or the sole member of a namespace set.
    pub fn single_ns(&self, i: usize) -> Option<usize> {
        match self.mn(i)? {
            Multiname::QName { ns, .. } if *ns != 0 => Some(*ns),
            Multiname::Multiname { .. } | Multiname::MultinameL { .. } => match self.ns_set(i) {
                Some(s) if s.len() == 1 => Some(s[0]),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn custom_ns_of(&self, i: usize) -> Option<Dotted> {
        let kind = self.simple_ns_kind(i);
        if kind != ns_kind::NAMESPACE && kind != ns_kind::PACKAGE_INTERNAL {
            return None;
        }
        let name = self.simple_ns_name(i).unwrap_or_default().raw();
        self.custom_ns_of_value(&name)
    }

    pub fn custom_ns_of_namespace(&self, ns: usize) -> Option<Dotted> {
        let kind = self.ns_kind(ns);
        if kind != ns_kind::NAMESPACE && kind != ns_kind::PACKAGE_INTERNAL {
            return None;
        }
        let name = self.ns_raw_name(ns);
        self.custom_ns_of_value(&name)
    }

    fn custom_ns_of_value(&self, name: &str) -> Option<Dotted> {
        if name == "http://adobe.com/AS3/2006/builtin" {
            return Some(Dotted::new(&["AS3"]));
        }
        Some(self.custom_ns.get(name).cloned().unwrap_or_default())
    }

    fn type_name_str(&self, i: usize, fqn: &[Dotted], raw: bool, with_suffix: bool, depth: u8) -> String {
        let Some(Multiname::TypeName { name, params }) = self.mn(i) else { return String::new() };
        if depth > 16 {
            return "\u{a7}\u{a7}cyclic_typename()".into();
        }
        let mut out = self.name_depth(*name, fqn, raw, with_suffix, depth + 1);
        if !params.is_empty() {
            out.push_str(".<");
            for (k, &p) in params.iter().enumerate() {
                if k > 0 {
                    out.push(',');
                }
                if p == 0 {
                    out.push('*');
                } else {
                    out.push_str(&self.name_depth(p, fqn, raw, with_suffix, depth + 1));
                }
            }
            out.push('>');
        }
        out
    }

    /// `Multiname.getName(..., fullyQualifiedNames, dontDeobfuscate, withSuffix)`.
    pub fn name(&self, i: usize, fqn: &[Dotted], raw: bool, with_suffix: bool) -> String {
        self.name_depth(i, fqn, raw, with_suffix, 0)
    }

    fn name_depth(&self, i: usize, fqn: &[Dotted], raw: bool, with_suffix: bool, depth: u8) -> String {
        let Some(m) = self.mn(i) else { return String::new() };
        if matches!(m, Multiname::TypeName { .. }) {
            return self.type_name_str(i, fqn, raw, with_suffix, depth);
        }
        let ni = Self::name_index(m);
        if ni == 0 {
            return if m.is_attribute() { "@*".into() } else { "*".into() };
        }
        let name = self.s(ni);
        let is_public = self.single_ns(i).is_some_and(|ns| matches!(self.ns_kind(ns), ns_kind::PACKAGE | ns_kind::PACKAGE_INTERNAL));
        if is_public && !fqn.is_empty() && fqn.contains(&Dotted::parse_with_suffix(name)) {
            let dc = self.name_with_ns(i, with_suffix);
            return if raw { dc.raw() } else { dc.printable(self.ids) };
        }
        self.plain(m, name, raw, with_suffix, i)
    }

    fn plain(&self, m: &Multiname, name: &str, raw: bool, with_suffix: bool, i: usize) -> String {
        let mut out = String::new();
        if m.is_attribute() {
            out.push('@');
        }
        out.push_str(&if raw { name.to_string() } else { self.ids.print(name) });
        if with_suffix {
            out.push_str(&self.suffix(i));
        }
        out
    }

    /// `getNameAndCustomNamespace`: the name plus the custom namespace it is qualified with, if any.
    pub fn name_and_custom_ns(&self, i: usize, fqn: &[Dotted], raw: bool, with_suffix: bool) -> (String, Option<Dotted>) {
        let Some(m) = self.mn(i) else { return (String::new(), None) };
        if matches!(m, Multiname::TypeName { .. }) {
            return (self.type_name_str(i, fqn, raw, with_suffix, 0), None);
        }
        let ni = Self::name_index(m);
        if ni == 0 {
            return (if m.is_attribute() { "@*".into() } else { "*".into() }, None);
        }
        let name = self.s(ni);
        let kind = self.simple_ns_kind(i);
        let mut custom = None;
        if kind == ns_kind::NAMESPACE || kind == ns_kind::PACKAGE_INTERNAL {
            if let Some(dc) = self.custom_ns_of(i) {
                let ns = dc.last().to_string();
                if !dc.is_empty() && ns != "AS3" {
                    let id = if raw { ns.clone() } else { self.ids.print(&ns) };
                    if !id.is_empty() {
                        custom = Some(dc);
                    }
                }
            }
        }
        if kind == ns_kind::PACKAGE && !fqn.is_empty() && fqn.contains(&Dotted::parse_with_suffix(name)) {
            let dc = self.name_with_ns(i, with_suffix);
            return (if raw { dc.raw() } else { dc.printable(self.ids) }, custom);
        }
        (self.plain(m, name, raw, with_suffix, i), custom)
    }

    /// `getNameWithCustomNamespace`: `ns::name` for custom namespaces.
    pub fn name_with_custom_ns(&self, i: usize, fqn: &[Dotted], raw: bool, with_suffix: bool) -> String {
        let (name, custom) = self.name_and_custom_ns(i, fqn, raw, with_suffix);
        match custom {
            Some(dc) => {
                let ns = dc.last();
                let id = if raw { ns.to_string() } else { self.ids.print(ns) };
                format!("{id}::{name}")
            }
            None => name,
        }
    }

    /// `getNameWithNamespace(..., withSuffix)`.
    pub fn name_with_ns(&self, i: usize, with_suffix: bool) -> Dotted {
        let mut ns_name = self.simple_ns_name(i);
        if ns_name.is_none() {
            if let Some(ns) = self.single_ns(i) {
                ns_name = Some(self.ns_name(ns));
            }
        }
        let name = self.name(i, &[], true, false);
        let sfx = if with_suffix { self.suffix(i) } else { String::new() };
        match ns_name {
            Some(n) => n.add(&name, &sfx),
            None => Dotted::one(&name, &sfx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_parse() {
        assert_eq!(Dotted::parse_no_suffix("a.b").raw(), "a.b");
        assert_eq!(Dotted::parse_no_suffix("a.").len(), 1);
        assert_eq!(Dotted::parse_with_suffix("x#12").0[0].suffix, "#12");
    }

    #[test]
    fn identifiers() {
        let ids = Idents::default();
        assert_eq!(ids.print("abc"), "abc");
        assert_eq!(ids.print("in"), "\u{a7}in\u{a7}");
        assert_eq!(ids.print("a-b"), "\u{a7}a-b\u{a7}");
        assert_eq!(ids.print("Img$1f2e-123"), "Img");
    }
}
