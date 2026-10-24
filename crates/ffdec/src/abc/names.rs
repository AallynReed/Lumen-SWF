//! Namespace and multiname text, as printed in p-code and AS3.

use super::{Abc, Multiname};

pub const NS_KINDS: [(u8, &str, &str); 7] = [
    (0x08, "Namespace", ""),
    (0x05, "PrivateNamespace", "private"),
    (0x16, "PackageNamespace", "public"),
    (0x17, "PackageInternalNs", "internal"),
    (0x18, "ProtectedNamespace", "protected"),
    (0x19, "ExplicitNamespace", "explicit"),
    (0x1a, "StaticProtectedNs", "protected"),
];

pub fn ns_kind_name(kind: u8) -> &'static str {
    NS_KINDS.iter().find(|k| k.0 == kind).map_or("?", |k| k.1)
}

/// Escapes a p-code string, including the `\{n}` run-length form for long runs.
pub fn escape_pcode(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\'' => out.push_str("\\'"),
            c if (c as u32) < 32 => out.push_str(&format!("\\x{:02x}", c as u32)),
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

/// Escapes an ActionScript string literal.
pub fn escape_as(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\'' => out.push_str("\\'"),
            c if (c as u32) < 32 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn quoted(abc: &Abc, i: usize) -> String {
    match abc.constants.string(i) {
        Some(s) => format!("\"{}\"", escape_pcode(s)),
        None => "null".into(),
    }
}

impl Abc {
    pub fn str(&self, i: usize) -> &str {
        self.constants.string(i).unwrap_or("")
    }

    pub fn ns_pcode(&self, index: usize) -> String {
        if index == 0 {
            return "null".into();
        }
        let Some(ns) = self.constants.namespaces.get(index) else { return format!("Unknown({index})") };
        let mut name = if ns.name == 0 { None } else { Some(self.str(ns.name).to_string()) };
        // Duplicate namespaces of the same kind and name get a distinguishing suffix.
        let sub = self.constants.namespaces[1..=index].iter().filter(|n| n.kind == ns.kind && n.name == ns.name).count() - 1;
        let mut version = String::new();
        if let Some(n) = &mut name {
            if let Some(last) = n.chars().last() {
                if ('\u{e000}'..='\u{f8ff}').contains(&last) {
                    n.pop();
                    version = format!("\\u{:04x}", last as u32);
                }
            }
        }
        let name = match name {
            Some(n) => format!("\"{}{version}\"", escape_pcode(&n)),
            None => "null".into(),
        };
        let sub = if sub > 0 { format!(",\"{sub}\"") } else { String::new() };
        format!("{}({name}{sub})", ns_kind_name(ns.kind))
    }

    pub fn ns_set_pcode(&self, index: usize) -> String {
        if index == 0 {
            return "null".into();
        }
        let Some(set) = self.constants.ns_sets.get(index) else { return format!("Unknown({index})") };
        format!("[{}]", set.iter().map(|&n| self.ns_pcode(n)).collect::<Vec<_>>().join(","))
    }

    pub fn multiname_pcode(&self, index: usize) -> String {
        self.multiname_pcode_depth(index, 0)
    }

    fn multiname_pcode_depth(&self, index: usize, depth: u8) -> String {
        if index == 0 {
            return "null".into();
        }
        let Some(m) = self.constants.multinames.get(index) else { return format!("Unknown({index})") };
        let kind = kind_name(m);
        match m {
            Multiname::QName { ns, name, .. } => format!("{kind}({},{})", self.ns_pcode(*ns), quoted(self, *name)),
            Multiname::RtqName { name, .. } => format!("{kind}({})", quoted(self, *name)),
            Multiname::RtqNameL { .. } => format!("{kind}()"),
            Multiname::Multiname { name, ns_set, .. } => format!("{kind}({},{})", quoted(self, *name), self.ns_set_pcode(*ns_set)),
            Multiname::MultinameL { ns_set, .. } => format!("{kind}({})", self.ns_set_pcode(*ns_set)),
            Multiname::TypeName { name, params } => {
                if depth > 16 {
                    return "CyclicTypeName".into();
                }
                let ps: Vec<String> = params.iter().map(|&p| self.multiname_pcode_depth(p, depth + 1)).collect();
                format!("{kind}({}<{}>)", self.multiname_pcode_depth(*name, depth + 1), ps.join(","))
            }
        }
    }

    /// The local name of a multiname, without namespace.
    pub fn local_name(&self, index: usize) -> String {
        match self.constants.multinames.get(index) {
            Some(Multiname::QName { name, .. } | Multiname::RtqName { name, .. } | Multiname::Multiname { name, .. }) => self.str(*name).to_string(),
            Some(Multiname::TypeName { name, params }) => {
                let ps: Vec<String> = params.iter().map(|&p| if p == 0 { "*".into() } else { self.local_name(p) }).collect();
                format!("{}.<{}>", self.local_name(*name), ps.join(","))
            }
            _ => String::new(),
        }
    }

    /// Package (namespace name) of a QName, or of the first package namespace of a multiname.
    pub fn package_of(&self, index: usize) -> String {
        let ns = match self.constants.multinames.get(index) {
            Some(Multiname::QName { ns, .. }) => *ns,
            Some(Multiname::Multiname { ns_set, .. }) => self.constants.ns_sets.get(*ns_set).and_then(|s| s.first().copied()).unwrap_or(0),
            _ => 0,
        };
        self.constants.namespaces.get(ns).map(|n| self.str(n.name).to_string()).unwrap_or_default()
    }

    pub fn ns_kind_of(&self, index: usize) -> u8 {
        match self.constants.multinames.get(index) {
            Some(Multiname::QName { ns, .. }) => self.constants.namespaces.get(*ns).map_or(0, |n| n.kind),
            _ => 0,
        }
    }

    /// `package.Name`, or just `Name` in the top-level package.
    pub fn full_name(&self, index: usize) -> String {
        let pkg = self.package_of(index);
        let name = self.local_name(index);
        if pkg.is_empty() { name } else { format!("{pkg}.{name}") }
    }
}

fn kind_name(m: &Multiname) -> &'static str {
    match m {
        Multiname::QName { attr: false, .. } => "QName",
        Multiname::QName { attr: true, .. } => "QNameA",
        Multiname::RtqName { attr: false, .. } => "RTQName",
        Multiname::RtqName { attr: true, .. } => "RTQNameA",
        Multiname::RtqNameL { attr: false } => "RTQNameL",
        Multiname::RtqNameL { attr: true } => "RTQNameLA",
        Multiname::Multiname { attr: false, .. } => "Multiname",
        Multiname::Multiname { attr: true, .. } => "MultinameA",
        Multiname::MultinameL { attr: false, .. } => "MultinameL",
        Multiname::MultinameL { attr: true, .. } => "MultinameLA",
        Multiname::TypeName { .. } => "TypeName",
    }
}

/// ECMAScript `Number.prototype.toString()` for doubles.
pub fn ecma_number(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if v == 0.0 {
        return "0".into();
    }
    let neg = v < 0.0;
    let sci = format!("{:e}", v.abs());
    let (mant, exp) = sci.split_once('e').unwrap();
    let digits: String = mant.chars().filter(|c| c.is_ascii_digit()).collect();
    let k = digits.len() as i32;
    let n = exp.parse::<i32>().unwrap() + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let sign = if e >= 0 { "+" } else { "-" };
        if k == 1 { format!("{digits}e{sign}{}", e.abs()) } else { format!("{}.{}e{sign}{}", &digits[..1], &digits[1..], e.abs()) }
    };
    if neg { format!("-{body}") } else { body }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_like_javascript() {
        assert_eq!(ecma_number(1.0), "1");
        assert_eq!(ecma_number(0.5), "0.5");
        assert_eq!(ecma_number(-123.456), "-123.456");
        assert_eq!(ecma_number(1e21), "1e+21");
        assert_eq!(ecma_number(1e20), "100000000000000000000");
        assert_eq!(ecma_number(0.000001), "0.000001");
        assert_eq!(ecma_number(1e-7), "1e-7");
        assert_eq!(ecma_number(1.5e-7), "1.5e-7");
        assert_eq!(ecma_number(f64::NAN), "NaN");
    }

    #[test]
    fn pcode_escapes() {
        assert_eq!(escape_pcode("a\"b\n"), "a\\\"b\\n");
        assert_eq!(escape_pcode(&"x".repeat(25)), "\\{25}x");
    }
}
